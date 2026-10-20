#!/usr/bin/env python3
"""Create an incremental local patch; do not modify the concurrent shared tree."""
from pathlib import Path
import difflib
import hashlib
import json
import subprocess
import tempfile

ROOT = Path('/workspace/asterinas-gpui')
OUT = ROOT / 'evidence/nvidia-msix-run-1900'
ISOLATED = ROOT / 'asterinas-irq-retire-local'
NAME = 'ostd/src/irq/top_half.rs'
PATCH = ROOT / 'local-prs/asterinas-irq-retire-storage.patch'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    old = (OUT / 'top_half-before.rs').read_bytes()
    new = (ISOLATED / NAME).read_bytes()
    assert new.endswith(b'\n') and old != new
    patch = f'diff --git a/{NAME} b/{NAME}\n' + ''.join(difflib.unified_diff(old.decode().splitlines(True), new.decode().splitlines(True), fromfile='a/' + NAME, tofile='b/' + NAME, n=3))
    PATCH.write_text(patch)
    with tempfile.TemporaryDirectory(prefix='irq-retire-storage-apply-') as directory:
        target = Path(directory)
        dest = target / NAME
        dest.parent.mkdir(parents=True)
        dest.write_bytes(old)
        subprocess.run(['git', 'apply', '--check', str(PATCH)], cwd=target, check=True)
        subprocess.run(['git', 'apply', str(PATCH)], cwd=target, check=True)
        assert dest.read_bytes() == new
    check = subprocess.run(['git', 'apply', '--check', str(PATCH)], cwd=ROOT / 'asterinas', capture_output=True, text=True)
    index = subprocess.check_output(['git', 'ls-files', '-s'], cwd=ROOT / 'asterinas')
    assert index == (OUT / 'index-before.txt').read_bytes()
    regression = json.loads((OUT / 'retire-storage-results.json').read_text())
    assert regression['source_sha256'] == sha(new) and all(run['expected'] for run in regression['runs'])
    result = dict(patch=str(PATCH), isolated_implementation=str(ISOLATED / NAME), incoming_sha256=sha(old), tested_source_sha256=sha(new), patch_sha256=sha(PATCH.read_bytes()), exact_round_trip=True, shared_tree_apply_check=check.returncode == 0, shared_tree_apply_check_stderr=check.stderr, shared_tree_modified=False, merged=False, prerequisite='MSI-X implementation containing IrqLine::retire; intended after asterinas-nvidia-msix-transaction.patch when that pending concurrent patch is finalized', full_stack_validated=False, original_index_unchanged=True, hardware_gpu_acceptance=False, authenticated=False, committed=False, pushed=False, remote_actions_run=False)
    (OUT / 'patch-validation.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
