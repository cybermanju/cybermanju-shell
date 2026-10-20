#!/usr/bin/env python3
"""Build/boot actual Asterinas and compare full firmware bytes read on guest CPU.

This intentionally supplies no GPU and never starts firmware on a GPU. A passing
result certifies only boot archive service, C ABI leases, real clock/task service,
and static /init execution. It is not GPU/RM/GSP/GPUI acceptance.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tomllib

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
OUT = ROOT / 'evidence/nvport-lifecycle/real-firmware-boot'
RELEASE = '615.78.08'


def sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    source = json.loads((OUT.parent / 'firmware-provenance.json').read_text())
    if source['release'] != RELEASE or source['installer_executed'] or source['gpu_firmware_executed']:
        raise RuntimeError('Unexpected firmware provenance')
    rootfs = OUT / 'rootfs'
    rootfs.mkdir(exist_ok=False)
    firmware = rootfs / f'lib/firmware/nvidia/{RELEASE}'
    firmware.mkdir(parents=True)
    shutil.copyfile(ROOT / 'evidence/nvport/cpu-rootfs/init', rootfs / 'init')
    (rootfs / 'init').chmod(0o755)
    helper = OUT / 'fnv1a64'
    subprocess.run(['cc', '-std=c11', '-Wall', '-Wextra', '-Werror', '-O2', str(OUT.parent / 'fnv1a64.c'), '-o', str(helper)], check=True)
    expected = {}
    for item in source['files']:
        image = Path(item['path'])
        if sha256(image) != item['sha256'] or image.stat().st_size != item['size']:
            raise RuntimeError('Extracted image no longer matches recorded provenance')
        name = image.name
        shutil.copyfile(image, firmware / name)
        checksum = subprocess.check_output([str(helper), str(image)], text=True).strip()
        expected[f'nvidia/{RELEASE}/{name}'] = {'size': item['size'], 'fnv1a64': checksum, 'sha256': item['sha256']}
    archive = OUT / 'real-firmware-initramfs.cpio'
    names = subprocess.check_output(['find', '.', '-print0'], cwd=rootfs)
    with archive.open('wb') as stream:
        subprocess.run(['cpio', '--null', '-o', '--format=newc', '--owner=0:0'], cwd=rootfs, input=names, stdout=stream, check=True)
    env = os.environ.copy()
    env['PATH'] = str(ROOT / 'toolchain/bin') + os.pathsep + env['PATH']
    env['NVIDIA_SOURCE_DIR'] = str(ROOT / 'vendor-drivers/open-gpu-kernel-modules-615.78.08-tag')
    env['VDSO_LIBRARY_DIR'] = str(ROOT / 'vendor-drivers/linux_vdso')
    config = REPO / 'kernel/OSDK.toml'
    with config.open('x') as stream:
        stream.write('[boot]\nmethod = "grub-rescue-iso"\n[grub]\nboot_protocol = "multiboot2"\n[qemu]\nargs = "-nic none"\n')
    try:
        print('Building Asterinas with actual release-matched firmware archive; no GPU', flush=True)
        with (OUT / 'kernel-build.log').open('w') as log:
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
        raise RuntimeError('Unexpected boot bundle')
    saved_image = OUT / 'asterinas-real-firmware.iso'
    shutil.copyfile(image, saved_image)
    print('Booting real guest with TCG, no GPU/KVM/network/passthrough', flush=True)
    with (OUT / 'serial.log').open('wb') as log:
        result = subprocess.run(['qemu-system-x86_64', '-machine', 'q35,accel=tcg', '-cpu', 'max', '-m', '2048M', '-smp', '2',
                                 '-icount', 'shift=3,align=off,sleep=off',
                                 '-display', 'none', '-monitor', 'none', '-serial', 'stdio', '-no-reboot', '-nic', 'none',
                                 '-device', 'isa-debug-exit,iobase=0xf4,iosize=0x04', '-cdrom', str(saved_image)],
                                stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, timeout=300)
    text = (OUT / 'serial.log').read_text(errors='strict')
    observed = {}
    for name, length, checksum in re.findall(r'NVIDIA RM firmware acquired from guest boot archive: (\S+) bytes=(\d+) cpu_fnv1a64=([0-9a-f]{16}) \(GSP not started\)', text):
        if name in observed:
            raise RuntimeError('Duplicate firmware output marker')
        observed[name] = {'size': int(length), 'fnv1a64': checksum}
    errors = []
    if result.returncode != 33: errors.append(f'Unexpected QEMU debug-exit status {result.returncode}')
    if re.search(r'\bpanic(?:ked)?\b', text, re.I): errors.append('Guest panicked')
    if 'ASTERINAS_CPU_USERSPACE_SMOKE passed (not a GPU test)' not in text: errors.append('Real static /init did not complete')
    if 'NVIDIA NVport CPU memory self-test passed (not a GPU test)' not in text: errors.append('Native memory self-test missing')
    if 'NVIDIA NVport runtime: real clock/thread/yield round trip;' not in text: errors.append('Real C clock/thread/yield service missing')
    for name, item in expected.items():
        if observed.get(name) != {'size': item['size'], 'fnv1a64': item['fnv1a64']}:
            errors.append(f'Missing or incorrect full-byte CPU firmware readback for {name}')
    if set(observed) != set(expected): errors.append('Unexpected firmware inventory')
    report = {'pass': not errors, 'scope': 'CPU-only actual Asterinas firmware-file service, C ABI lease/readback/release, runtime and /init boot; not GPU acceptance',
              'emulated_cpu_time_model': 'QEMU TCG icount shift=3,align=off,sleep=off; not wall-clock/performance evidence',
              'qemu_exit': result.returncode, 'hardware_gpu': False, 'rm_gsp_initialized': False, 'firmware_executed_on_gpu': False,
              'image_sha256': sha256(saved_image), 'archive_sha256': sha256(archive), 'serial_sha256': sha256(OUT / 'serial.log'),
              'official_nvidia_source_commit': '1a82fbfda8f321b056eb3568850f9316c5e465c5',
              'firmware_installer_download_complete': source['installer_download_complete'],
              'firmware_archive_integrity_checked': source['embedded_md5_corruption_check'],
              'firmware_vendor_signature_verified': source['vendor_signature_verified'],
              'firmware_provenance_sha256': sha256(OUT.parent / 'firmware-provenance.json'),
              'firmware_expected': expected, 'firmware_observed': observed, 'errors': errors}
    (OUT / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2), flush=True)
    if errors: raise RuntimeError('CPU firmware boot validation failed')


if __name__ == '__main__':
    main()
