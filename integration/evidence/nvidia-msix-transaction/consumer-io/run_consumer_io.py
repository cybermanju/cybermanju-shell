#!/usr/bin/env python3
"""Full-kernel QEMU regression for real NVMe, VirtIO-modern and VirtIO-legacy I/O.

Run only after the parent task's in-progress kernel tests have finished. All
created evidence and disposable raw disks stay under this script's directory.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
HERE = Path(__file__).resolve().parent
RUN_NAME = os.environ.get('CONSUMER_IO_RUN', 'run')
if Path(RUN_NAME).name != RUN_NAME or not RUN_NAME.startswith('run'):
    raise ValueError('CONSUMER_IO_RUN must be a simple run-prefixed directory name')
OUT = HERE / RUN_NAME
DISK_BYTES = 1024 * 1024
BLOCK_BYTES = 4096
SEEDS = {'nvme': 0x23, 'virtio-modern': 0x7B, 'virtio-legacy': 0xD1}


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def checked(command: list[str], **kwargs) -> subprocess.CompletedProcess:
    print('+', ' '.join(map(str, command)), flush=True)
    return subprocess.run(command, check=True, **kwargs)


def build_fixtures(output: Path) -> dict[str, Path]:
    disks = {}
    for label, seed in SEEDS.items():
        path = output / f'{label}.raw'
        # Newly created, disposable writable raw image; unique first-sector pattern.
        payload = bytearray(DISK_BYTES)
        for i in range(BLOCK_BYTES):
            payload[i] = (i * 37 + seed) & 0xFF
        path.write_bytes(payload)
        disks[label] = path
    return disks


def build_initramfs(output: Path) -> tuple[Path, Path]:
    compiler = shutil.which('gcc')
    if not compiler:
        raise RuntimeError('gcc is required to compile the freestanding init')
    executable = output / 'init'
    archive = output / 'consumer-io-initramfs.cpio'
    checked([
        compiler, '-std=c11', '-O2', '-Wall', '-Wextra', '-Werror',
        '-ffreestanding', '-fno-stack-protector', '-fno-pie', '-nostdlib',
        '-static', '-no-pie', '-Wl,-e,_start', str(HERE / 'init.c'), '-o', str(executable),
    ])
    if subprocess.check_output(['file', str(executable)], text=True).find('statically linked') < 0:
        raise RuntimeError('init must be a static, freestanding executable')
    executable.chmod(0o755)

    rootfs = output / 'rootfs'
    rootfs.mkdir()
    (rootfs / 'dev').mkdir()
    shutil.copyfile(executable, rootfs / 'init')
    (rootfs / 'init').chmod(0o755)
    names = sorted(p.relative_to(rootfs).as_posix() for p in rootfs.rglob('*'))
    names = ['.'] + ['./' + name for name in names]
    name_stream = b''.join(os.fsencode(name) + b'\0' for name in names)
    with archive.open('wb') as stream:
        checked(['cpio', '--null', '-o', '--format=newc', '--owner=0:0'],
                cwd=rootfs, input=name_stream, stdout=stream)
    return executable, archive


def qemu_command(iso: Path, disks: dict[str, Path]) -> list[str]:
    return [
        'qemu-system-x86_64', '-machine', 'q35,accel=tcg', '-cpu', 'max',
        '-m', '2048M', '-smp', '2', '-icount', 'shift=3,align=off,sleep=off',
        '-display', 'none', '-monitor', 'none', '-serial', 'stdio', '-no-reboot',
        '-nic', 'none', '-boot', 'order=d',
        '-drive', f'file={disks["nvme"]},if=none,id=consumer_nvme,format=raw,cache=unsafe',
        '-device', 'nvme,drive=consumer_nvme,serial=ASTER-CONSUMER-NVME',
        '-drive', f'file={disks["virtio-modern"]},if=none,id=consumer_vmodern,format=raw,cache=unsafe',
        '-device', 'virtio-blk-pci,drive=consumer_vmodern,disable-legacy=on',
        '-drive', f'file={disks["virtio-legacy"]},if=none,id=consumer_vlegacy,format=raw,cache=unsafe',
        '-device', 'virtio-blk-pci,drive=consumer_vlegacy,disable-modern=on',
        '-device', 'isa-debug-exit,iobase=0xf4,iosize=0x04',
        '-cdrom', str(iso),
    ]


def main() -> int:
    if OUT.exists():
        raise RuntimeError(f'{OUT} already exists; preserve prior evidence and choose a fresh run directory')
    OUT.mkdir()
    disks = build_fixtures(OUT)
    executable, archive = build_initramfs(OUT)
    (OUT / 'fixture-manifest.json').write_text(json.dumps({
        'fixture_bytes': DISK_BYTES,
        'verified_bytes_at_offset_0': BLOCK_BYTES,
        'scratch_bytes_at_offset_4096': BLOCK_BYTES,
        'patterns': {name: {'seed': seed, 'byte_formula': '(i * 37 + seed) & 0xff'}
                     for name, seed in SEEDS.items()},
        'disk_sha256_before_guest': {name: sha256(path) for name, path in disks.items()},
        'init_sha256': sha256(executable),
        'initramfs_sha256': sha256(archive),
    }, indent=2) + '\n')

    config = REPO / 'kernel/OSDK.toml'
    created_config = False
    try:
        # The temporary project config is exclusive and removed even on build failure.
        with config.open('x') as stream:
            created_config = True
            stream.write('[boot]\nmethod = "grub-rescue-iso"\n[grub]\nboot_protocol = "multiboot2"\n[qemu]\nargs = "-nic none"\n')
        env = dict(os.environ)
        env['PATH'] = str(ROOT / 'toolchain/bin') + os.pathsep + env.get('PATH', '')
        env['NVIDIA_SOURCE_DIR'] = str(ROOT / 'vendor-drivers/open-gpu-kernel-modules-615.78.08-tag')
        env['VDSO_LIBRARY_DIR'] = str(ROOT / 'vendor-drivers/linux_vdso')
        env['CARGO_NET_OFFLINE'] = 'true'
        print('Building complete kernel + custom initramfs; storage fixtures are disposable; no GPU/firmware.', flush=True)
        with (OUT / 'kernel-build.log').open('wb') as log:
            checked([
                'cargo', 'osdk', 'build', '--features',
                'nvidia-nvport-selftest,nvidia-pci-diagnostics',
                '--boot-method', 'grub-rescue-iso', '--grub-boot-protocol', 'multiboot2',
                '--strip-elf', '--initramfs', str(archive),
                '--kcmd-args=earlycon console=ttyS0 rdinit=/init',
            ], cwd=REPO / 'kernel', env=env, stdout=log, stderr=subprocess.STDOUT, timeout=1200)
    finally:
        if created_config:
            config.unlink(missing_ok=True)

    bundle = REPO / 'target/osdk/asterinas'
    metadata = tomllib.loads((bundle / 'bundle.toml').read_text())
    image = (bundle / metadata['vm_image']['path']).resolve(strict=True)
    if image.parent != bundle.resolve() or image.suffix != '.iso':
        raise RuntimeError(f'Unexpected OSDK boot image location/type: {image}')
    saved_iso = OUT / 'asterinas-consumer-io.iso'
    shutil.copyfile(image, saved_iso)

    print('Booting full local kernel under TCG; NVMe + VirtIO modern and legacy have raw fixtures.', flush=True)
    serial = OUT / 'serial.log'
    try:
        with serial.open('wb') as log:
            result = subprocess.run(qemu_command(saved_iso, disks), stdin=subprocess.DEVNULL,
                                    stdout=log, stderr=subprocess.STDOUT, timeout=300)
        returncode = result.returncode
        timed_out = False
    except subprocess.TimeoutExpired:
        returncode = None
        timed_out = True
    text = serial.read_text(errors='replace')
    required = [
        'CONSUMER_IO DEVICE PASS: nvme0n1 (NVMe)',
        'CONSUMER_IO DEVICE PASS: vda (VirtIO modern)',
        'CONSUMER_IO DEVICE PASS: vdb (VirtIO legacy)',
        'ASTERINAS_CONSUMER_IO_PASS: NVMe + VirtIO modern + VirtIO legacy real block I/O verified',
    ]
    errors = [f'missing verified-I/O marker: {marker}' for marker in required if marker not in text]
    if 'CONSUMER_IO FAIL:' in text:
        errors.append('guest reported a real block I/O validation failure')
    if timed_out:
        errors.append('QEMU timed out (guest did not power off)')
    elif returncode != 33:
        errors.append(f'expected QEMU isa-debug-exit code 33 after successful poweroff, got {returncode}')
    if 'panic' in text.lower():
        errors.append('kernel panic detected in serial output')
    host_disk_verified = {}
    for name, seed in SEEDS.items():
        raw = disks[name].read_bytes()
        initial = bytes((i * 37 + seed) & 0xff for i in range(BLOCK_BYTES))
        scratch = bytes((i * 17 + (seed ^ 0xa5)) & 0xff for i in range(BLOCK_BYTES))
        valid = raw[:BLOCK_BYTES] == initial and raw[BLOCK_BYTES:2 * BLOCK_BYTES] == scratch
        host_disk_verified[name] = valid
        if not valid:
            errors.append(f'host raw disk did not preserve initial and written scratch bytes: {name}')
    report = {
        'passed': not errors,
        'qemu_exit': returncode,
        'errors': errors,
        'scope': 'Full Asterinas kernel; real block reads and writes under QEMU TCG; not GPU/firmware/performance evidence',
        'hardware_gpu': False,
        'gpu_firmware_loaded': False,
        'devices': {'nvme': '/dev/nvme0n1', 'virtio_modern': '/dev/vda', 'virtio_legacy': '/dev/vdb'},
        'verified': '4096 known patterned bytes at offset 0 plus 4096-byte scratch write/read at offset 4096 per device',
        'iso_sha256': sha256(saved_iso),
        'initramfs_sha256': sha256(archive),
        'serial_sha256': sha256(serial),
        'host_raw_disk_bytes_verified': host_disk_verified,
        'disks_sha256_after_guest': {name: sha256(path) for name, path in disks.items()},
    }
    (OUT / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2), flush=True)
    if errors:
        print(f'Failure evidence: {serial}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f'CONSUMER_IO RUNNER FAIL: {exc}', file=sys.stderr, flush=True)
        raise SystemExit(1)
