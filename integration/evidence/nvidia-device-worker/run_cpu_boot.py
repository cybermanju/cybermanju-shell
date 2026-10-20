#!/usr/bin/env python3
"""Build/boot the local kernel; no GPU firmware, GPU benchmark or timing claims."""
from pathlib import Path
import hashlib
import json
import os
import re
import shutil
import subprocess
import tomllib

ROOT=Path('/workspace/asterinas-gpui')
REPO=ROOT/'asterinas'
OUT=ROOT/'evidence/nvidia-device-worker/cpu-boot'

def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    OUT.mkdir(parents=True,exist_ok=False)
    archive=ROOT/'evidence/nvport/cpu-initramfs.cpio'
    env=dict(os.environ,PATH=str(ROOT/'toolchain/bin')+':'+os.environ['PATH'],
        NVIDIA_SOURCE_DIR=str(ROOT/'vendor-drivers/open-gpu-kernel-modules-615.78.08-tag'),
        VDSO_LIBRARY_DIR=str(ROOT/'vendor-drivers/linux_vdso'),CARGO_NET_OFFLINE='true')
    config=REPO/'kernel/OSDK.toml'
    with config.open('x') as f:
        f.write('[boot]\nmethod = "grub-rescue-iso"\n[grub]\nboot_protocol = "multiboot2"\n[qemu]\nargs = "-nic none"\n')
    try:
        print('Building full local kernel; no GPU firmware in initramfs',flush=True)
        with (OUT/'build.log').open('w') as log:
            subprocess.run(['cargo','osdk','build','--features','nvidia-nvport-selftest,nvidia-pci-diagnostics','--boot-method','grub-rescue-iso','--grub-boot-protocol','multiboot2','--strip-elf','--initramfs',str(archive),'--kcmd-args=earlycon console=ttyS0 rdinit=/init'],cwd=REPO/'kernel',env=env,stdout=log,stderr=subprocess.STDOUT,timeout=1200,check=True)
    finally: config.unlink()
    bundle=REPO/'target/osdk/asterinas'
    metadata=tomllib.loads((bundle/'bundle.toml').read_text())
    image=(bundle/metadata['vm_image']['path']).resolve(strict=True)
    if image.parent!=bundle.resolve() or image.suffix!='.iso':
        raise RuntimeError('Unexpected kernel image path')
    saved=OUT/'asterinas-cpu.iso'
    shutil.copyfile(image,saved)
    modes={'plain':[],'iommu':['-device','intel-iommu,intremap=on'],
        'multifunction-iommu':['-device','intel-iommu,intremap=on',
            '-device','pcie-root-port,id=rp-group,addr=0a.0,chassis=10,hotplug=off',
            '-device','nvme,bus=rp-group,addr=00.0,multifunction=on,serial=ASTER-GROUP-F0',
            '-device','nvme,bus=rp-group,addr=00.1,serial=ASTER-GROUP-F1']}
    for name,extra in modes.items():
        print('Booting '+name+'; TCG functional evidence only',flush=True)
        serial=OUT/f'serial-{name}.log'
        with serial.open('wb') as log:
            p=subprocess.run(['qemu-system-x86_64','-machine','q35,accel=tcg','-cpu','max','-m','2048M','-smp','2','-icount','shift=3,align=off,sleep=off','-display','none','-monitor','none','-serial','stdio','-no-reboot','-nic','none',*extra,'-device','isa-debug-exit,iobase=0xf4,iosize=0x04','-cdrom',str(saved)],stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT,timeout=300)
        text=serial.read_text()
        markers=['ASTERINAS_CPU_USERSPACE_SMOKE passed (not a GPU test)','NVIDIA NVport CPU memory self-test passed (not a GPU test)','NVIDIA NVport runtime: real clock/thread/yield round trip;']
        if name!='plain': markers+=['DMA remapping enabled']
        errors=[f'Missing marker: {marker}' for marker in markers if marker not in text]
        if p.returncode!=33: errors+=[f'Unexpected QEMU debug-exit: {p.returncode}']
        if re.search(r'\bpanic(?:ked)?\b',text,re.I): errors+=['Guest panic']
        result=dict(passed=not errors,mode=name,qemu_exit=p.returncode,errors=errors,scope='Full CPU kernel boot only; instruction-counted TCG is not performance or GPU evidence',hardware_gpu=False,rm_gsp_initialized=False,gpu_firmware_loaded=False,image_sha256=sha(saved),initramfs_sha256=sha(archive),serial_sha256=sha(serial))
        (OUT/f'result-{name}.json').write_text(json.dumps(result,indent=2)+'\n')
        print(json.dumps(result,indent=2),flush=True)
        if errors: raise SystemExit(1)

if __name__=='__main__': main()
