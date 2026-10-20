#!/usr/bin/env python3
"""Build local incremental patch against the saved incoming worktree, not HEAD."""
from pathlib import Path
import difflib
import hashlib
import json
import subprocess
import tempfile

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
OUT = ROOT / 'evidence/nvidia-device-dma'
PATCH = ROOT / 'local-prs/asterinas-nvidia-device-dma.patch'
PATHS = [
    '.github/workflows/graphics_gpu.yml',
    'ostd/src/arch/x86/iommu/mod.rs',
    'ostd/src/arch/x86/iommu/registers/mod.rs',
    'ostd/src/arch/x86/kernel/acpi/mod.rs',
    'ostd/src/arch/x86/kernel/acpi/dma_policy.rs',
    'ostd/src/io/io_mem/mod.rs',
    'ostd/src/mm/dma/mod.rs',
    'ostd/src/mm/dma/device.rs',
    'kernel/core/comps/pci/src/lib.rs',
    'kernel/core/comps/pci/src/dma.rs',
    'kernel/core/comps/pci/src/mmio.rs',
    'kernel/core/comps/pci/src/lifecycle_tests.rs',
    'kernel/comps/nvidia-platform/src/device.rs',
    'kernel/comps/nvidia-platform/README.md',
    'tools/graphics/run_cpu_kernel_tests.sh',
]

def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    changes = []
    chunks = []
    for name in sorted(PATHS):
        before = OUT / 'before' / name
        new = (REPO / name).read_bytes()
        if before.is_file():
            old = before.read_bytes()
        else:
            exists = subprocess.run(['git', 'cat-file', '-e', f'HEAD:{name}'], cwd=REPO, capture_output=True)
            if exists.returncode == 0:
                raise RuntimeError('Missing incoming snapshot: ' + name)
            old = b''
        if old == new:
            continue
        if not new.endswith(b'\n'):
            raise RuntimeError('Incomplete text: ' + name)
        chunks.append(f'diff --git a/{name} b/{name}\n')
        if not before.is_file():
            chunks.append('new file mode 100644\n')
        chunks.extend(difflib.unified_diff(
            old.decode().splitlines(True), new.decode().splitlines(True),
            fromfile=f'a/{name}' if before.is_file() else '/dev/null', tofile=f'b/{name}', n=3))
        changes.append(dict(path=name, old_sha256=sha(old), new_sha256=sha(new), new_file=not before.is_file()))
    PATCH.write_text(''.join(chunks))
    with tempfile.TemporaryDirectory(prefix='aster-device-dma-apply-') as temp:
        target = Path(temp)
        for change in changes:
            if change['new_file']:
                continue
            dest = target / change['path']
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes((OUT / 'before' / change['path']).read_bytes())
        subprocess.run(['git', 'apply', '--check', str(PATCH)], cwd=target, check=True)
        subprocess.run(['git', 'apply', str(PATCH)], cwd=target, check=True)
        for change in changes:
            if (target / change['path']).read_bytes() != (REPO / change['path']).read_bytes():
                raise RuntimeError('Round-trip mismatch: ' + change['path'])
    if subprocess.check_output(['git', 'ls-files', '--stage'], cwd=REPO) != (OUT / 'index-before.txt').read_bytes():
        raise RuntimeError('Original Git index changed')
    result = dict(
        patch=str(PATCH), patch_sha256=sha(PATCH.read_bytes()),
        prerequisite='all ordered prior Asterinas patches through asterinas-nvidia-resource-ownership.patch',
        exact_round_trip=True, index_unchanged=True, files=changes,
        authenticated=False, committed=False, pushed=False, remote_actions_run=False,
        hardware_gpu_test=False, rm_gsp_initialized=False, iommu_domain_implemented=False,
        mode='explicit coherent direct DMA only; IOMMU/CVM denied')
    (OUT / 'patch-validation.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))

if __name__ == '__main__':
    main()
