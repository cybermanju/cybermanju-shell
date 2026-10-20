#!/usr/bin/env python3
"""Package the local worker implementation; never stage, commit, or authenticate."""
from pathlib import Path
import difflib
import hashlib
import json
import subprocess
import tempfile

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
OUT = ROOT / 'evidence/nvidia-device-worker'
PATCH = ROOT / 'local-prs/asterinas-nvidia-device-worker.patch'
NAMES = [
    'kernel/comps/nvidia-platform/src/lib.rs',
    'kernel/comps/nvidia-platform/src/worker.rs',
    'kernel/comps/nvidia-platform/src/worker/tests.rs',
    'kernel/comps/nvidia-platform/README.md',
    'tools/graphics/run_cpu_kernel_tests.sh',
]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    chunks, changes = [], []
    for name in NAMES:
        before = OUT / 'before' / name
        old = before.read_bytes() if before.is_file() else b''
        new = (REPO / name).read_bytes()
        assert new.endswith(b'\n') and new != old, name
        chunks.append(f'diff --git a/{name} b/{name}\n')
        if not before.is_file():
            chunks.append('new file mode 100644\n')
        chunks.extend(difflib.unified_diff(old.decode().splitlines(True), new.decode().splitlines(True), fromfile='a/' + name if before.is_file() else '/dev/null', tofile='b/' + name, n=3))
        changes.append(dict(path=name, before_sha256=sha(old), after_sha256=sha(new), new_file=not before.is_file()))
    PATCH.write_text(''.join(chunks))
    with tempfile.TemporaryDirectory(prefix='nvidia-worker-patch-') as directory:
        target = Path(directory)
        for name in NAMES:
            before = OUT / 'before' / name
            if before.is_file():
                path = target / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(before.read_bytes())
        subprocess.run(['git', 'apply', '--check', str(PATCH)], cwd=target, check=True)
        subprocess.run(['git', 'apply', str(PATCH)], cwd=target, check=True)
        for name in NAMES:
            assert (target / name).read_bytes() == (REPO / name).read_bytes(), name
    assert subprocess.check_output(['git', 'ls-files', '-s'], cwd=REPO) == (OUT / 'index-before.txt').read_bytes()
    result = dict(patch=str(PATCH), patch_sha256=sha(PATCH.read_bytes()), prerequisite='All fifteen patches through asterinas-irq-retire-storage.patch', files=changes, exact_round_trip=True, original_index_unchanged=True, authenticated=False, committed=False, pushed=False, remote_actions_run=False, hardware_gpu_acceptance=False, rm_gsp_initialized=False)
    (OUT / 'patch-validation.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
