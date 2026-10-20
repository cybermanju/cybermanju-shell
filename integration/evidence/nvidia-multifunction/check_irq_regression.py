#!/usr/bin/env python3
"""Exercise the real kernel test with old/new IRQ drop ordering; always restore source."""
from pathlib import Path
import hashlib
import json
import os
import re
import subprocess

ROOT=Path('/workspace/asterinas-gpui')
REPO=ROOT/'asterinas'
OUT=ROOT/'evidence/nvidia-multifunction/irq-regression'
FILE=REPO/'ostd/src/irq/top_half.rs'

def main():
    OUT.mkdir(parents=True,exist_ok=False)
    original=FILE.read_text()
    start=original.index('impl Drop for IrqLine {')
    end=original.index('\nstruct Inner {',start)
    baseline=original[:start]+original[end:]
    config=REPO/'ostd/OSDK.toml'
    env=dict(os.environ,PATH=str(ROOT/'toolchain/bin')+':'+os.environ['PATH'],CARGO_NET_OFFLINE='true')
    results=[]
    try:
        with config.open('x') as f:
            f.write('[boot]\nmethod = "grub-rescue-iso"\n[grub]\nboot_protocol = "multiboot2"\n[qemu]\nargs = "-machine q35,accel=tcg -cpu max -m 2048M -smp 2 -display none -monitor none -serial stdio -no-reboot -nic none -device isa-debug-exit,iobase=0xf4,iosize=0x04"\n[test.boot]\nkcmd_args = ["earlycon", "console=ttyS0"]\n')
        for name,source in [('old-order',baseline),('fixed-order',original)]:
            FILE.write_text(source)
            print('Testing IRQ '+name,flush=True)
            with (OUT/f'{name}.log').open('w') as log:
                p=subprocess.run(['cargo','osdk','test','--strip-elf','callbacks_are_dropped_before_irq_is_released'],cwd=REPO/'ostd',env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT,timeout=600)
            text=re.sub(r'\x1b\[[0-9;]*[mK]', '', (OUT/f'{name}.log').read_text())
            passed='test result: ok. 1 passed; 0 failed;' in text and '[ktest runner] All crates tested.' in text and p.returncode==0
            rejected='callbacks_are_dropped_before_irq_is_released' in text and 'FAILED' in text and p.returncode!=0
            expected=passed if name=='fixed-order' else rejected
            results.append(dict(mode=name,exit=p.returncode,expected=expected,test_passed=passed,baseline_failed=rejected))
            print(results[-1],flush=True)
            if not expected:
                raise RuntimeError('Unexpected regression result: '+name)
    finally:
        FILE.write_text(original)
        if config.exists(): config.unlink()
        (OUT/'result.json').write_text(json.dumps(dict(runs=results,source_restored=FILE.read_text()==original,source_sha256=hashlib.sha256(original.encode()).hexdigest()),indent=2)+'\n')

if __name__=='__main__': main()
