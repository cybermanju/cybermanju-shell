#!/usr/bin/env python3
"""Real QEMU kernel old/new comparisons; restore source exactly even after failure."""
from pathlib import Path
import os, subprocess, re, json, hashlib
ROOT=Path('/workspace/asterinas-gpui'); REPO=ROOT/'asterinas'; OUT=ROOT/'evidence/nvidia-msix-transaction'
FILE=REPO/'kernel/core/comps/pci/src/capability/msix.rs'
CONFIG=REPO/'kernel/core/comps/pci/OSDK.toml'

def main():
    original=FILE.read_bytes(); text=original.decode()
    old_bme=text.replace('| Command::INTERRUPT_DISABLE.bits(),','| (Command::INTERRUPT_DISABLE | Command::BUS_MASTER).bits(),',1).encode()
    old_index=text.replace('self.irqs.get_mut(index)?.as_mut()','self.irqs[index].as_mut()',1).encode()
    if old_bme==original or old_index==original: raise RuntimeError('Missing mutation context')
    cases=[('old-implicit-bme',old_bme,False,'MSI-X acquisition must not authorize DMA'),
           ('old-index-panic',old_index,False,'index out of bounds'),('fixed-legacy',original,True,'')]
    env=dict(os.environ,PATH=str(ROOT/'toolchain/bin')+':'+os.environ['PATH'],CARGO_NET_OFFLINE='true')
    created=False;results=[]
    try:
        with CONFIG.open('x') as f:
            f.write('[boot]\nmethod = "grub-rescue-iso"\n[grub]\nboot_protocol = "multiboot2"\n[qemu]\nargs = "-machine q35,accel=tcg -cpu max -m 2048M -smp 2 -display none -monitor none -serial stdio -no-reboot -nic none -device pcie-root-port,id=rp-msix-legacy,addr=0d.0,chassis=13,hotplug=off -device nvme,bus=rp-msix-legacy,addr=00.0,serial=ASTER-MSIX-LEGACY -device isa-debug-exit,iobase=0xf4,iosize=0x04"\n[test.boot]\nkcmd_args = ["earlycon", "console=ttyS0"]\n')
        created=True
        for name,source,expect_pass,needle in cases:
            FILE.write_bytes(source); print('Real guest comparison: '+name,flush=True)
            logpath=OUT/(name+'.log')
            with logpath.open('w') as log:
                p=subprocess.run(['cargo','osdk','test','--strip-elf','qemu_legacy_msix_never_grants_dma_and_checks_indices'],cwd=CONFIG.parent,env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT,timeout=600)
            text=re.sub(r'\x1b\[[0-9;]*[mK]','',logpath.read_text())
            passed=p.returncode==0 and 'test result: ok. 1 passed; 0 failed;' in text and '[ktest runner] All crates tested.' in text
            failed=p.returncode!=0 and 'FAILED' in text and 'panicked' in text.lower() and needle in text
            expected=passed if expect_pass else failed
            result=dict(name=name,exit=p.returncode,expected=expected,passed=passed,baseline_failed=failed,log=str(logpath))
            results.append(result); print(json.dumps(result),flush=True)
            if not expected: raise RuntimeError('Unexpected baseline result '+name)
    finally:
        FILE.write_bytes(original)
        if created: CONFIG.unlink()
        (OUT/'regression-results.json').write_text(json.dumps(dict(runs=results,source_restored=FILE.read_bytes()==original,source_sha256=hashlib.sha256(original).hexdigest(),hardware_gpu=False),indent=2)+'\n')
if __name__=='__main__':main()
