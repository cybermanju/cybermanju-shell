#!/usr/bin/env python3
"""Build and validate the incremental NVIDIA IRQ/worker teardown patch."""
from pathlib import Path
import difflib
import hashlib
import json
import subprocess
import tempfile

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
OUT = ROOT / 'evidence/native-2030'
PATCH17_VALIDATION = ROOT / 'evidence/nvidia-worker-flush/stack-validation.json'
PATCH18 = ROOT / 'local-prs/asterinas-nvidia-irq-worker-teardown.patch'
FILES = [
    'kernel/comps/nvidia-platform/src/worker.rs',
    'kernel/comps/nvidia-platform/src/worker/tests.rs',
    'kernel/comps/nvidia-platform/README.md',
]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    manifest = json.loads(PATCH17_VALIDATION.read_text())
    patches = [item['name'] for item in manifest['prerequisite_patches']]
    expected_hashes = {item['name']: item['sha256'] for item in manifest['prerequisite_patches']}
    assert len(patches) == 16 and manifest['patch_number'] == 17
    patches.append(Path(manifest['incremental_patch']).name)
    expected_hashes[patches[-1]] = manifest['incremental_patch_sha256']
    chunks = []
    for name in FILES:
        old = (OUT / 'before' / Path(name).name).read_bytes()
        new = (REPO / name).read_bytes()
        assert old != new and new.endswith(b'\n'), name
        chunks.append(f'diff --git a/{name} b/{name}\n')
        chunks.extend(difflib.unified_diff(
            old.decode().splitlines(True), new.decode().splitlines(True),
            fromfile='a/' + name, tofile='b/' + name, n=3))
    PATCH18.write_text(''.join(chunks))

    base = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO).decode().strip()
    with tempfile.TemporaryDirectory(prefix='aster-native-2030-patch18-') as tempdir:
        temp = Path(tempdir)
        archive, target = temp / 'base.tar', temp / 'source'
        target.mkdir()
        subprocess.run(['git', 'archive', '--format=tar', '--output', str(archive), base], cwd=REPO, check=True)
        subprocess.run(['tar', '-xf', str(archive), '-C', str(target)], check=True)
        for name in patches:
            patch = ROOT / 'local-prs' / name
            assert sha(patch.read_bytes()) == expected_hashes[name], name
            subprocess.run(['git', 'apply', '--check', str(patch)], cwd=target, check=True)
            subprocess.run(['git', 'apply', str(patch)], cwd=target, check=True)
        subprocess.run(['git', 'apply', '--check', str(PATCH18)], cwd=target, check=True)
        subprocess.run(['git', 'apply', str(PATCH18)], cwd=target, check=True)
        for name in FILES:
            assert (target / name).read_bytes() == (REPO / name).read_bytes(), name

    before_index = (OUT / 'index-tree.before').read_text().strip()
    after_index = subprocess.check_output(['git', 'write-tree'], cwd=REPO).decode().strip()
    assert before_index == after_index, 'Git index changed'
    result = {
        'patch_number': 18,
        'base': base,
        'prerequisite_patch_count': len(patches),
        'existing_patches_reused_without_regeneration': patches,
        'incremental_patch': str(PATCH18),
        'incremental_patch_sha256': sha(PATCH18.read_bytes()),
        'files': FILES,
        'patch18_applies_after_patch17': True,
        'target_bytes_exact': True,
        'index_tree_before': before_index,
        'index_tree_after': after_index,
        'original_index_unchanged': True,
        'authenticated': False,
        'committed': False,
        'pushed': False,
        'remote_actions_run': False,
        'gpu_acceptance': False,
        'rm_gsp_initialized': False,
    }
    (OUT / 'patch-validation.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
