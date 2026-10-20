#!/usr/bin/env python3
"""Generate local follow-up diff against the previously delivered patch stack."""
import difflib
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
PATCHES = ROOT / 'local-prs'
EVIDENCE = ROOT / 'evidence/nvport'
BASE = '00a4842d976db6723b8224be2e57ac8105f13ee2'
STACK = [
    'asterinas-dumb-buffer-layout.patch',
    'asterinas-simpledrm-exact-pixel-format.patch',
    'asterinas-graphics-prerequisite-check.patch',
    'asterinas-vulkan-compute-ci.patch',
    'asterinas-nvidia-nvport-native-ci.patch',
]
NAME = 'asterinas-nvidia-boot-pci-tcg.patch'


def run(*args, cwd=REPO):
    return subprocess.check_output(args, cwd=cwd)


index_before = run('git', 'ls-files', '-s')
paths = run('git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z').decode().split('\0')
paths = sorted(set(p for p in paths if p and not (p.endswith('/OSDK.toml') and p != 'OSDK.toml')))
with tempfile.TemporaryDirectory(prefix='aster-nvidia-followup-') as temporary:
    tree = Path(temporary) / 'tree'
    run('git', 'worktree', 'add', '--detach', str(tree), BASE)
    try:
        for patch in STACK:
            run('git', 'apply', '--whitespace=error', str(PATCHES / patch), cwd=tree)
        parts = []
        changed = []
        for relative in paths:
            old_path, new_path = tree / relative, REPO / relative
            if old_path.is_symlink() or new_path.is_symlink():
                assert old_path.is_symlink() and new_path.is_symlink(), relative
                assert old_path.readlink() == new_path.readlink(), relative
                continue
            if old_path.is_dir() or new_path.is_dir():
                assert old_path.is_dir() and new_path.is_dir(), relative
                continue
            old = old_path.read_bytes() if old_path.exists() else b''
            new = new_path.read_bytes() if new_path.exists() else b''
            if old == new:
                continue
            # No generated binary is expected in this source-only patch.
            old_text, new_text = old.decode(), new.decode()
            changed.append(relative)
            parts.append(f'diff --git a/{relative} b/{relative}\n')
            if not old_path.exists():
                parts.append('new file mode 100644\n')
            elif not new_path.exists():
                parts.append('deleted file mode 100644\n')
            parts.extend(difflib.unified_diff(
                old_text.splitlines(keepends=True), new_text.splitlines(keepends=True),
                fromfile=f'a/{relative}' if old_path.exists() else '/dev/null',
                tofile=f'b/{relative}' if new_path.exists() else '/dev/null',
            ))
        assert changed, 'No incremental changes found'
        payload = ''.join(parts).encode()
        patch_path = PATCHES / NAME
        patch_path.write_bytes(payload)
        run('git', 'apply', '--check', '--whitespace=error', str(patch_path), cwd=tree)
        run('git', 'apply', '--whitespace=error', str(patch_path), cwd=tree)
        for relative in changed:
            assert (tree / relative).read_bytes() == (REPO / relative).read_bytes(), relative
        run('git', 'diff', '--check', cwd=tree)
        with (EVIDENCE / 'followup-clean-graphics-tests.log').open('wb') as log:
            subprocess.run(['python3', '-m', 'unittest', 'discover', '-s', 'tools/graphics', '-v'],
                           cwd=tree, stdout=log, stderr=subprocess.STDOUT, check=True)
        index_after = run('git', 'ls-files', '-s')
        assert index_before == index_after, 'Original index changed'
        report = {
            'status': 'pass', 'scope': 'incremental source patch application/equality and host script tests; not GPU evidence',
            'base': BASE, 'prerequisites': STACK, 'patch': NAME,
            'sha256': hashlib.sha256(payload).hexdigest(), 'changed_files': changed,
            'original_index_unchanged': True,
        }
        (EVIDENCE / 'followup-patch-validation.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))
    finally:
        run('git', 'worktree', 'remove', '--force', str(tree))
