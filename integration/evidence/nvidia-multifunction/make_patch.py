#!/usr/bin/env python3
"""Create the local incremental patch; never modify Git's index or create commits."""
from pathlib import Path
import difflib
import hashlib
import json
import subprocess
import tempfile

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
OUT = ROOT / 'evidence/nvidia-multifunction'
PATCH = ROOT / 'local-prs/asterinas-nvidia-multifunction.patch'
NAMES = [
    'kernel/core/comps/pci/src/bus.rs',
    'kernel/core/comps/pci/src/reservation.rs',
    'kernel/core/comps/pci/src/mmio.rs',
    'kernel/core/comps/pci/src/topology.rs',
    'kernel/core/comps/pci/src/dma.rs',
    'kernel/core/comps/pci/src/lib.rs',
    'kernel/core/comps/pci/src/multifunction_tests.rs',
    'ostd/src/arch/x86/iommu/dma_remapping/isolated.rs',
    'ostd/src/mm/dma/isolated.rs',
    'ostd/src/irq/top_half.rs',
    'tools/graphics/run_cpu_kernel_tests.sh',
    'tools/graphics/NATIVE_NVIDIA.md',
]

def sha(data):
    return hashlib.sha256(data).hexdigest()

def main():
    chunks, changes = [], []
    for name in NAMES:
        path, before = REPO / name, OUT / 'before' / name
        existed = before.is_file()
        old = before.read_bytes() if existed else b''
        new = path.read_bytes()
        if old == new:
            continue
        if not new.endswith(b'\n'):
            raise RuntimeError('Incomplete file: ' + name)
        chunks.append(f'diff --git a/{name} b/{name}\n')
        if not existed:
            chunks.append('new file mode 100644\n')
        chunks.extend(difflib.unified_diff(old.decode().splitlines(True),new.decode().splitlines(True),
            fromfile=f'a/{name}' if existed else '/dev/null',tofile=f'b/{name}',n=3))
        changes.append(dict(path=name,old_sha256=sha(old),new_sha256=sha(new),new_file=not existed))
    PATCH.write_text(''.join(chunks))
    with tempfile.TemporaryDirectory(prefix='aster-multifunction-apply-') as directory:
        target = Path(directory)
        for change in changes:
            if change['new_file']:
                continue
            name = change['path']
            dest = target / name
            dest.parent.mkdir(parents=True,exist_ok=True)
            dest.write_bytes((OUT / 'before' / name).read_bytes())
        subprocess.run(['git','apply','--check',str(PATCH)],cwd=target,check=True)
        subprocess.run(['git','apply',str(PATCH)],cwd=target,check=True)
        for change in changes:
            name = change['path']
            if (target/name).read_bytes() != (REPO/name).read_bytes():
                raise RuntimeError('Patch round trip differs: ' + name)
    index = subprocess.check_output(['git','ls-files','-s'],cwd=REPO)
    if index != (OUT/'index-before.txt').read_bytes():
        raise RuntimeError('Original Git index changed')
    result = dict(patch=str(PATCH),patch_sha256=sha(PATCH.read_bytes()),files=changes,
        prerequisite='All eleven Asterinas patches through asterinas-nvidia-pcie-topology.patch',
        exact_incremental_round_trip=True,index_unchanged=True,authenticated=False,
        committed=False,pushed=False,remote_actions_run=False,hardware_gpu_test=False,
        rm_gsp_initialized=False,gpu_firmware_loaded=False)
    (OUT/'patch-validation.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))

if __name__ == '__main__':
    main()
