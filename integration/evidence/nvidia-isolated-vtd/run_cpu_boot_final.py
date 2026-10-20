#!/usr/bin/env python3
"""Build/boot the real kernel with device-DMA prerequisite code; never load GPU firmware."""
from pathlib import Path
import hashlib
import json
import os
import re
import shutil
import subprocess
import tomllib

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
OUT = ROOT / 'evidence/nvidia-isolated-vtd/cpu-boot-final'

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    OUT.mkdir(parents=True, exist_ok=False)
    archive = ROOT / 'evidence/nvport/cpu-initramfs.cpio'
    env = os.environ.copy()
    env.update(PATH=str(ROOT / 'toolchain/bin') + ':' + env['PATH'],
               NVIDIA_SOURCE_DIR=str(ROOT / 'vendor-drivers/open-gpu-kernel-modules-615.78.08-tag'),
               VDSO_LIBRARY_DIR=str(ROOT / 'vendor-drivers/linux_vdso'),
               CARGO_NET_OFFLINE='true')
    config = REPO / 'kernel/OSDK.toml'
    with config.open('x') as stream:
        stream.write('[boot]\nmethod = "grub-rescue-iso"\n[grub]\nboot_protocol = "multiboot2"\n[qemu]\nargs = "-nic none"\n')
    try:
        print('Building full kernel; CPU-only initramfs, no GPU firmware', flush=True)
        with (OUT / 'build.log').open('w') as log:
            subprocess.run(['cargo', 'osdk', 'build', '--features', 'nvidia-nvport-selftest,nvidia-pci-diagnostics',
                            '--boot-method', 'grub-rescue-iso', '--grub-boot-protocol', 'multiboot2', '--strip-elf',
                            '--initramfs', str(archive), '--kcmd-args=earlycon console=ttyS0 rdinit=/init'],
                           cwd=REPO / 'kernel', env=env, stdout=log, stderr=subprocess.STDOUT, timeout=1200, check=True)
    finally:
        config.unlink()
    bundle = REPO / 'target/osdk/asterinas'
    metadata = tomllib.loads((bundle / 'bundle.toml').read_text())
    image = (bundle / metadata['vm_image']['path']).resolve(strict=True)
    if image.parent != bundle.resolve() or image.suffix != '.iso':
        raise RuntimeError('Unexpected kernel image path')
    saved = OUT / 'asterinas-cpu.iso'
    shutil.copyfile(image, saved)
    print('Booting full kernel under instruction-counted TCG; no GPU/KVM/passthrough/network', flush=True)
    with (OUT / 'serial.log').open('wb') as log:
        process = subprocess.run(['qemu-system-x86_64', '-machine', 'q35,accel=tcg', '-cpu', 'max',
                                  '-m', '2048M', '-smp', '2', '-icount', 'shift=3,align=off,sleep=off',
                                  '-display', 'none', '-monitor', 'none', '-serial', 'stdio', '-no-reboot', '-nic', 'none',
                                  '-device', 'isa-debug-exit,iobase=0xf4,iosize=0x04', '-cdrom', str(saved)],
                                 stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, timeout=300)
    text = (OUT / 'serial.log').read_text()
    errors = []
    if process.returncode != 33:
        errors.append(f'Unexpected raw QEMU exit: {process.returncode}')
    if re.search(r'\bpanic(?:ked)?\b', text, re.I):
        errors.append('Guest panic')
    for marker in ['ASTERINAS_CPU_USERSPACE_SMOKE passed (not a GPU test)',
                   'NVIDIA NVport CPU memory self-test passed (not a GPU test)',
                   'NVIDIA NVport runtime: real clock/thread/yield round trip;']:
        if marker not in text:
            errors.append('Missing marker: ' + marker)
    result = dict(passed=not errors, scope='CPU-only full-kernel regression, not GPU evidence',
                  qemu_exit=process.returncode, virtual_time='instruction-counted TCG, not performance evidence',
                  hardware_gpu=False, rm_gsp_initialized=False, gpu_firmware_loaded=False,
                  image_sha256=sha(saved), initramfs_sha256=sha(archive), serial_sha256=sha(OUT / 'serial.log'), errors=errors)
    (OUT / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2), flush=True)
    if errors:
        raise SystemExit(1)

if __name__ == '__main__':
    main()

# The same genuine kernel image also boots with QEMU Intel VT-d enabled.
if __name__ == '__main__':
    with (OUT / 'serial-iommu.log').open('wb') as log:
        process = subprocess.run(['qemu-system-x86_64', '-machine', 'q35,accel=tcg', '-cpu', 'max',
            '-m', '2048M', '-smp', '2', '-icount', 'shift=3,align=off,sleep=off',
            '-display', 'none', '-monitor', 'none', '-serial', 'stdio', '-no-reboot', '-nic', 'none',
            '-device', 'intel-iommu,intremap=on',
            '-device', 'isa-debug-exit,iobase=0xf4,iosize=0x04', '-cdrom', str(OUT / 'asterinas-cpu.iso')],
            stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, timeout=300)
    text = (OUT / 'serial-iommu.log').read_text()
    markers = ['DMA remapping enabled', 'ASTERINAS_CPU_USERSPACE_SMOKE passed (not a GPU test)',
               'NVIDIA NVport CPU memory self-test passed (not a GPU test)']
    errors = [f'Missing marker: {m}' for m in markers if m not in text]
    if process.returncode != 33: errors.append(f'Unexpected QEMU exit: {process.returncode}')
    if re.search(r'\bpanic(?:ked)?\b', text, re.I): errors.append('Guest panic')
    result = dict(passed=not errors, qemu_exit=process.returncode, errors=errors,
        scope='Full CPU kernel boot with Intel VT-d, not GPU proof', hardware_gpu=False,
        image_sha256=sha(OUT / 'asterinas-cpu.iso'), serial_sha256=sha(OUT / 'serial-iommu.log'))
    (OUT / 'result-iommu.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2), flush=True)
    if errors: raise SystemExit(1)
