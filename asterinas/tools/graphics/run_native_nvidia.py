#!/usr/bin/env python3
"""Build current Asterinas, boot it with preauthorized VFIO devices, validate guest compute.

Requires an administrator-provisioned, isolated runner and NVIDIA-matched guest
rootfs. Never binds PCI devices, downloads firmware, or runs compute on Linux.
The serial/nonce chain is evidence from a trusted runner, not remote attestation.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import selectors
import shutil
import subprocess
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parents[2]
BDF = re.compile(r"[0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7]")


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def checked_devices(text, sysfs=Path("/sys/bus/pci/devices")):
    devices = text.split(",")
    if not devices or len(devices) != len(set(devices)):
        raise ValueError("VFIO device list must be nonempty and unique")
    found_gpu = False
    for bdf in devices:
        if BDF.fullmatch(bdf) is None:
            raise ValueError("Invalid full-domain PCI BDF")
        device = sysfs / bdf
        if (device / "driver").resolve().name != "vfio-pci":
            raise ValueError(f"{bdf} must already be bound to vfio-pci by the administrator")
        group = device / "iommu_group"
        if not group.exists():
            raise ValueError(f"{bdf} has no IOMMU group; unsafe no-IOMMU mode is not supported")
        for peer in (group / "devices").iterdir():
            if (peer / "driver").exists() and (peer / "driver").resolve().name != "vfio-pci":
                raise ValueError(f"IOMMU peer {peer.name} remains attached to a host driver")
        found_gpu |= (device / "vendor").read_text().strip() == "0x10de" and (device / "class").read_text().strip().startswith("0x03")
    if not found_gpu:
        raise ValueError("No prebound NVIDIA display/3D controller selected")
    return devices


def capture_guest(command, serial, nonce, timeout):
    """Stop only the QEMU process we created, on terminal guest marker or timeout."""
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
    pending = b""
    terminal = f"ASTERINAS_NATIVE_GPU_EXIT nonce={nonce} status=".encode()
    deadline = time.monotonic() + timeout
    try:
        with selectors.DefaultSelector() as selector, serial.open("wb") as output:
            selector.register(process.stdout, selectors.EVENT_READ)
            while time.monotonic() < deadline:
                events = selector.select(min(1, max(0, deadline - time.monotonic())))
                if not events:
                    if process.poll() is not None:
                        break
                    continue
                chunk = os.read(process.stdout.fileno(), 65536)
                if not chunk:
                    break
                output.write(chunk)
                output.flush()
                pending += chunk
                lines = pending.split(b"\n")
                pending = lines.pop()
                if any(line.rstrip(b"\r").startswith(terminal) for line in lines):
                    return
            raise RuntimeError("Guest exited or timed out without a complete terminal marker")
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        process.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rootfs", required=True, type=Path, help="Prepared guest tree with /bin/sh, mount/cat/poweroff, Vulkan loader, matching NVIDIA userspace and GSP firmware")
    parser.add_argument("--vfio-devices", required=True, help="Comma-separated prebound full PCI BDFs")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--timeout", type=int, default=300)
    args = parser.parse_args()
    if not 1 <= args.timeout <= 900:
        parser.error("timeout must be 1..900 seconds")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if not os.access("/dev/kvm", os.R_OK | os.W_OK):
        raise RuntimeError("Accessible /dev/kvm is required; no CPU/software fallback")
    devices = checked_devices(args.vfio_devices)
    source_root = args.rootfs.resolve(strict=True)
    if not (source_root / "bin/sh").exists():
        raise ValueError("Guest rootfs must supply /bin/sh")
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Invalid source commit")
    if subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=normal"], cwd=ROOT):
        raise ValueError("Native evidence requires an exact clean dispatched checkout")
    nonce = secrets.token_hex(16)
    env = dict(os.environ, ASTER_NATIVE_GPU_CI_COMMIT=commit, ASTER_NATIVE_GPU_CI_NONCE=nonce)
    with tempfile.TemporaryDirectory(prefix="aster-native-gpu-") as temporary:
        rootfs = Path(temporary) / "rootfs"
        shutil.copytree(source_root, rootfs, symlinks=True)
        binary = rootfs / "aster-native-gpu-bench"
        shader = rootfs / "aster-native-gpu.spv"
        init = rootfs / "aster-native-gpu-init"
        for path in (binary, shader, init):
            if path.exists() or path.is_symlink():
                raise ValueError(f"Reserved rootfs entry already exists: {path.name}")
        subprocess.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-O2", str(ROOT / "tools/graphics/vulkan_compute_bench.c"), "-lvulkan", "-o", str(binary)], check=True)
        subprocess.run(["glslangValidator", "-V", "--target-env", "vulkan1.2", str(ROOT / "tools/graphics/vector.comp"), "-o", str(shader)], check=True)
        init.write_text(f'''#!/bin/sh
set -u
export PATH=/bin:/usr/bin:/sbin:/usr/sbin
mount -t proc proc /proc || exit 1
mount -t sysfs sysfs /sys || exit 1
mount -t devtmpfs devtmpfs /dev || exit 1
/aster-native-gpu-bench --shader /aster-native-gpu.spv --vendor nvidia > /aster-native-gpu-result.json
status=$?
if [ "$status" -eq 0 ]; then
    printf 'ASTERINAS_NATIVE_GPU_RESULT nonce={nonce} '
    cat /aster-native-gpu-result.json
fi
printf 'ASTERINAS_NATIVE_GPU_EXIT nonce={nonce} status=%s\\n' "$status"
poweroff -f
''')
        init.chmod(0o755)
        archive = output / "native-initramfs.cpio"
        names = subprocess.check_output(["find", ".", "-print0"], cwd=rootfs)
        with archive.open("wb") as stream:
            subprocess.run(["cpio", "--null", "-o", "--format=newc", "--owner=0:0"], cwd=rootfs, input=names, stdout=stream, check=True)
        command = ["cargo", "osdk", "build", "--features", "native-gpu-ci", "--boot-method", "grub-rescue-iso", "--grub-boot-protocol", "multiboot2", "--strip-elf", "--initramfs", str(archive), "--kcmd-args=earlycon console=ttyS0 rdinit=/bin/sh", "--init-args=/aster-native-gpu-init"]
        # OSDK appends CLI init arguments to manifest defaults. The workspace
        # defaults include interactive `sh -l`, which must not precede our script.
        # A nearest-package manifest is independent of the workspace manifest.
        config = ROOT / "kernel/OSDK.toml"
        with config.open("x") as stream:
            stream.write('[boot]\nmethod = "grub-rescue-iso"\n[grub]\nboot_protocol = "multiboot2"\n[qemu]\nargs = "-nic none"\n')
        try:
            with (output / "kernel-build.log").open("w") as log:
                subprocess.run(command, cwd=ROOT / "kernel", env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=1800)
        finally:
            config.unlink()
        bundle = ROOT / "target/osdk/asterinas"
        manifest = tomllib.loads((bundle / "bundle.toml").read_text())
        image = (bundle / manifest["vm_image"]["path"]).resolve(strict=True)
        if image.parent != bundle.resolve() or image.suffix != ".iso":
            raise ValueError("Unexpected OSDK boot image")
        saved_image = output / "asterinas-native.iso"
        shutil.copyfile(image, saved_image)
        provenance = {"commit": commit, "nonce": nonce, "kernel_image_sha256": sha256(saved_image), "initramfs_sha256": sha256(archive), "benchmark_sha256": sha256(binary), "shader_sha256": sha256(shader), "vfio_devices": devices, "nvidia_source": "1a82fbfda8f321b056eb3568850f9316c5e465c5", "test_scope": "Asterinas-native NVIDIA Vulkan compute; not GPUI/Wayland acceptance"}
        (output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
        serial = output / "guest-serial.log"
        qemu = ["qemu-system-x86_64", "-machine", "q35,accel=kvm,kernel-irqchip=split", "-cpu", "host", "-m", "4096", "-smp", "4", "-display", "none", "-monitor", "none", "-serial", "stdio", "-no-reboot", "-nic", "none", "-cdrom", str(saved_image)]
        for device in devices:
            qemu.extend(["-device", f"vfio-pci,host={device}"])
        try:
            capture_guest(qemu, serial, nonce, args.timeout)
        finally:
            subprocess.run(["python3", str(ROOT / "tools/graphics/validate_native_gpu_evidence.py"), "--serial", str(serial), "--commit", commit, "--nonce", nonce, "--output", str(output / "native-result.json")], check=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        raise SystemExit(f"Asterinas native GPU run failed: {error}") from error
