#!/usr/bin/env python3
"""Generate an exact local incremental patch over the preserved starting tree."""
from pathlib import Path
import difflib
import hashlib
import json
import subprocess
import tempfile

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
OUT = ROOT / 'evidence/nvidia-ownership'
PATCH = ROOT / 'local-prs/asterinas-nvidia-resource-ownership.patch'
PATHS = [
    '.github/workflows/graphics_gpu.yml',
    'kernel/core/comps/pci/src/lib.rs',
    'kernel/core/comps/pci/src/bus.rs',
    'kernel/core/comps/pci/src/common_device.rs',
    'kernel/core/comps/pci/src/cfg_space.rs',
    'kernel/core/comps/pci/src/capability/mod.rs',
    'kernel/core/comps/pci/src/reservation.rs',
    'kernel/core/comps/pci/src/mmio.rs',
    'kernel/core/comps/pci/src/lifecycle_tests.rs',
    'kernel/comps/nvidia-platform/src/lib.rs',
    'kernel/comps/nvidia-platform/src/device.rs',
    'kernel/comps/nvidia-platform/README.md',
    'ostd/src/io/io_mem/mod.rs',
    'ostd/src/io/io_mem/allocator.rs',
    'ostd/src/irq/mod.rs',
    'tools/graphics/run_cpu_kernel_tests.sh',
]

def main():
    changes = []
    chunks = []
    for name in sorted(PATHS):
        before = OUT / 'before' / name
        new = (REPO / name).read_bytes()
        if before.is_file():
            old = before.read_bytes()
        else:
            exists_at_base = subprocess.run(['git', 'cat-file', '-e', f'HEAD:{name}'], cwd=REPO, capture_output=True)
            if exists_at_base.returncode == 0:
                raise RuntimeError('Missing saved starting-tree file: ' + name)
            old = b''
        if old == new:
            continue
        if not new.endswith(b'\n'):
            raise RuntimeError('Incomplete POSIX text: ' + name)
        chunks.append(f'diff --git a/{name} b/{name}\n')
        if not before.is_file():
            chunks.append('new file mode 100644\n')
        chunks.extend(difflib.unified_diff(old.decode().splitlines(True), new.decode().splitlines(True),
                      fromfile=f'a/{name}' if before.is_file() else '/dev/null', tofile=f'b/{name}', n=3))
        changes.append(dict(path=name, old_sha256=hashlib.sha256(old).hexdigest(),
                            new_sha256=hashlib.sha256(new).hexdigest(), new_file=not before.is_file()))
    PATCH.write_text(''.join(chunks))
    with tempfile.TemporaryDirectory(prefix='aster-nvidia-resource-apply-') as temp:
        target = Path(temp)
        for change in changes:
            if change['new_file']:
                continue
            file = target / change['path']
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes((OUT / 'before' / change['path']).read_bytes())
        subprocess.run(['git', 'apply', '--check', str(PATCH)], cwd=target, check=True)
        subprocess.run(['git', 'apply', str(PATCH)], cwd=target, check=True)
        for change in changes:
            if (target / change['path']).read_bytes() != (REPO / change['path']).read_bytes():
                raise RuntimeError('Round-trip differs: ' + change['path'])
    if subprocess.check_output(['git', 'ls-files', '--stage'], cwd=REPO) != (OUT / 'index-before.txt').read_bytes():
        raise RuntimeError('Original Git index changed')
    result = dict(patch=str(PATCH), patch_sha256=hashlib.sha256(PATCH.read_bytes()).hexdigest(),
                  prerequisite='all ordered previous Asterinas patches through asterinas-nvidia-rm-os-services.patch',
                  exact_round_trip=True, index_unchanged=True, files=changes,
                  authenticated=False, committed=False, pushed=False, remote_actions_run=False,
                  hardware_gpu_test=False, rm_gsp_initialized=False)
    (OUT / 'patch-validation.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))

if __name__ == '__main__':
    main()
