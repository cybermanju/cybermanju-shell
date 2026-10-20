#!/usr/bin/env python3
"""Generate only this local incremental patch without commits or index changes."""
from pathlib import Path
import difflib
import hashlib
import json
import subprocess
import tempfile

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
OUT = ROOT / 'evidence/nvidia-pcie-topology'
PATCH = ROOT / 'local-prs/asterinas-nvidia-pcie-topology.patch'

def git(*args):
    return subprocess.check_output(['git', *args], cwd=REPO)

def sha(data):
    return hashlib.sha256(data).hexdigest()

def main():
    names = set(git('diff', '--name-only', '-z').decode().split('\0'))
    names.update(git('ls-files', '--others', '--exclude-standard', '-z').decode().split('\0'))
    chunks, changes = [], []
    for name in sorted(names - {''}):
        path = REPO / name
        if not path.is_file() or name.endswith('/OSDK.toml'):
            continue
        before = OUT / 'before' / name
        if before.is_file():
            old, existed = before.read_bytes(), True
        else:
            result = subprocess.run(['git', 'show', f'HEAD:{name}'], cwd=REPO, capture_output=True)
            old, existed = (result.stdout, True) if result.returncode == 0 else (b'', False)
        new = path.read_bytes()
        if old == new:
            continue
        if not new.endswith(b'\n'):
            raise RuntimeError('Incomplete text: ' + name)
        chunks.append(f'diff --git a/{name} b/{name}\n')
        if not existed:
            chunks.append('new file mode 100644\n')
        chunks.extend(difflib.unified_diff(old.decode().splitlines(True), new.decode().splitlines(True),
            fromfile=f'a/{name}' if existed else '/dev/null', tofile=f'b/{name}', n=3))
        changes.append(dict(path=name, old_sha256=sha(old), new_sha256=sha(new), new_file=not existed))
    PATCH.write_text(''.join(chunks))
    with tempfile.TemporaryDirectory(prefix='aster-pcie-apply-') as tmp:
        target = Path(tmp)
        for change in changes:
            if change['new_file']:
                continue
            name = change['path']
            before = OUT / 'before' / name
            old = before.read_bytes() if before.is_file() else git('show', f'HEAD:{name}')
            dest = target / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(old)
        subprocess.run(['git', 'apply', '--check', str(PATCH)], cwd=target, check=True)
        subprocess.run(['git', 'apply', str(PATCH)], cwd=target, check=True)
        for change in changes:
            if (target / change['path']).read_bytes() != (REPO / change['path']).read_bytes():
                raise RuntimeError('Patch bytes differ: ' + change['path'])
    if git('ls-files', '-s') != (OUT / 'index-before.txt').read_bytes():
        raise RuntimeError('Git index changed')
    result = dict(patch=str(PATCH), patch_sha256=sha(PATCH.read_bytes()), files=changes,
        prerequisite='All ten ordered Asterinas patches through asterinas-nvidia-isolated-vtd.patch',
        exact_incremental_round_trip=True, index_unchanged=True,
        authenticated=False, committed=False, pushed=False, remote_actions_run=False,
        hardware_gpu_test=False, rm_gsp_initialized=False)
    (OUT / 'patch-validation.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))

if __name__ == '__main__':
    main()
