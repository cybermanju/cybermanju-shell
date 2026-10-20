#!/usr/bin/env python3
"""Run actual OSTD kernel regressions on old/new code, restoring sources in finally."""
from pathlib import Path
import hashlib
import json
import os
import re
import subprocess

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
OUT = ROOT / 'evidence/nvidia-irq-callbacks'
FILE = REPO / 'ostd/src/irq/top_half.rs'
CONFIG = REPO / 'ostd/OSDK.toml'


def main():
    original = FILE.read_bytes()
    incoming = (OUT / 'before/ostd/src/irq/top_half.rs').read_bytes()
    marker = b'#[cfg(ktest)]\nmod test {'
    baseline = incoming.split(marker)[0] + marker + original.split(marker)[1]
    env = dict(os.environ, PATH=str(ROOT / 'toolchain/bin') + ':' + os.environ['PATH'], CARGO_NET_OFFLINE='true')
    cases = [
        ('old-zst', baseline, 'zero_sized_callbacks_keep_registration_identity', False, 1),
        ('old-destructor', baseline, 'callback_captures_drop_outside_registry_lock', False, 1),
        ('fixed-ostd-suite', original, '', True, 251),
    ]
    results = []
    created = False
    try:
        with CONFIG.open('x') as stream:
            stream.write('[boot]\nmethod = "grub-rescue-iso"\n[grub]\nboot_protocol = "multiboot2"\n[qemu]\nargs = "-machine q35,accel=tcg -cpu max -m 2048M -smp 2 -display none -monitor none -serial stdio -no-reboot -nic none -device isa-debug-exit,iobase=0xf4,iosize=0x04"\n[test.boot]\nkcmd_args = ["earlycon", "console=ttyS0"]\n')
        created = True
        for name, source, test_filter, expect_pass, count in cases:
            FILE.write_bytes(source)
            print('Running actual kernel: ' + name, flush=True)
            logpath = OUT / (name + '.log')
            with logpath.open('w') as log:
                command = ['cargo', 'osdk', 'test', '--strip-elf'] + ([test_filter] if test_filter else [])
                proc = subprocess.run(command, cwd=REPO / 'ostd', env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, timeout=600)
            text = re.sub(r'\x1b\[[0-9;]*[mK]', '', logpath.read_text())
            passed = proc.returncode == 0 and f'test result: ok. {count} passed; 0 failed;' in text and '[ktest runner] All crates tested.' in text
            failed_test = proc.returncode != 0 and test_filter in text and 'FAILED' in text and 'panicked' in text.lower()
            expected = passed if expect_pass else failed_test
            results.append(dict(name=name, exit=proc.returncode, expected=expected, passed=passed, baseline_failed=failed_test, log=str(logpath)))
            print(results[-1], flush=True)
            if not expected:
                raise RuntimeError('Unexpected actual kernel result: ' + name)
    finally:
        FILE.write_bytes(original)
        if created:
            CONFIG.unlink()
        (OUT / 'regression-results.json').write_text(json.dumps(dict(runs=results, source_restored=FILE.read_bytes() == original, source_sha256=hashlib.sha256(original).hexdigest(), hardware_gpu=False), indent=2) + '\n')


if __name__ == '__main__':
    main()
