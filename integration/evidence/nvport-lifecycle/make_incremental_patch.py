#!/usr/bin/env python3
"""Package this continuation relative to its preserved starting source tree.

No Git index changes, commits, authentication or network operations are used.
"""
from pathlib import Path
import difflib
import json
import shutil
import subprocess
import tempfile

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
EVIDENCE = ROOT / 'evidence/nvport-lifecycle'
BASELINE = EVIDENCE / 'before'
PATCH = ROOT / 'local-prs/asterinas-nvidia-rm-os-services.patch'
FIXED = [
    'Cargo.lock', 'kernel/Cargo.toml', 'kernel/src/lib.rs', 'kernel/src/nvidia_boot.rs',
    'ostd/src/task/atomic_mode.rs', '.github/workflows/graphics_gpu.yml',
    'tools/graphics/run_cpu_kernel_tests.sh', 'tools/graphics/run_native_nvidia.py',
    'tools/graphics/run_nvport_format_tests.sh',
]


def old_bytes(relative):
    saved = BASELINE / relative
    if saved.is_file():
        return saved.read_bytes(), 'saved-starting-tree'
    result = subprocess.run(['git', 'show', f'HEAD:{relative}'], cwd=REPO, capture_output=True)
    if result.returncode == 0:
        return result.stdout, 'unchanged-tracked-base'
    return b'', 'new-file'


def main():
    paths = set(FIXED)
    for prefix in ['ostd/libs/nvidia-nvport', 'kernel/comps/nvidia-platform']:
        for file in (REPO / prefix).rglob('*'):
            if file.is_file() and not any(part in ('target', '__pycache__') for part in file.parts):
                if file.name == 'OSDK.toml':
                    raise RuntimeError('A native test is still using a temporary package manifest')
                paths.add(file.relative_to(REPO).as_posix())
    parts = []
    changed = []
    for relative in sorted(paths):
        old, origin = old_bytes(relative)
        new = (REPO / relative).read_bytes()
        if old == new:
            continue
        old_text, new_text = old.decode(), new.decode()
        if not new_text.endswith('\n'):
            raise RuntimeError(f'Missing final newline: {relative}')
        parts.append(f'diff --git a/{relative} b/{relative}\n')
        if origin == 'new-file':
            parts.append('new file mode 100644\n')
        parts.extend(difflib.unified_diff(old_text.splitlines(keepends=True), new_text.splitlines(keepends=True),
                                         fromfile='/dev/null' if origin == 'new-file' else f'a/{relative}',
                                         tofile=f'b/{relative}', n=3))
        changed.append({'path': relative, 'baseline': origin, 'old_bytes': len(old), 'new_bytes': len(new)})
    PATCH.write_text(''.join(parts))
    with tempfile.TemporaryDirectory(prefix='aster-nvidia-services-') as directory:
        destination = Path(directory)
        for item in changed:
            data, origin = old_bytes(item['path'])
            if origin == 'new-file':
                continue
            target = destination / item['path']
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        subprocess.run(['git', 'apply', '--check', str(PATCH)], cwd=destination, check=True)
        subprocess.run(['git', 'apply', str(PATCH)], cwd=destination, check=True)
        for item in changed:
            if (destination / item['path']).read_bytes() != (REPO / item['path']).read_bytes():
                raise RuntimeError('Patch round-trip mismatch: ' + item['path'])
    current = subprocess.check_output(['git', 'ls-files', '--stage'], cwd=REPO)
    if current != (EVIDENCE / 'index-before.txt').read_bytes():
        raise RuntimeError('Git index differs from the preserved pre-task state')
    report = {'patch': str(PATCH), 'apply_check': True, 'apply_round_trip_exact': True,
              'git_index_unchanged': True, 'files': changed,
              'prerequisite': 'all prior ordered Asterinas patches through asterinas-nvidia-boot-pci-tcg.patch',
              'commit_created': False, 'pushed': False, 'authenticated': False}
    (EVIDENCE / 'patch-validation.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
