#!/usr/bin/env python3
"""Execute actual OSTD tests in an independent source/target snapshot."""
from pathlib import Path
import hashlib
import json
import os
import re
import subprocess

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas-irq-retire-local'
OUT = ROOT / 'evidence/nvidia-msix-run-1900'
SOURCE = REPO / 'ostd/src/irq/top_half.rs'
CONFIG = REPO / 'ostd/OSDK.toml'
TEST = 'disconnect_releases_callback_storage_before_retiring_with_clone'


def main():
    fixed = SOURCE.read_bytes()
    # The baseline preserves the incoming clear() cleanup operation. The private
    # helper is retained so the same kernel test can inspect its capacity invariant.
    needle = b'drop(core::mem::take(&mut self.callbacks));'
    assert fixed.count(needle) == 1
    baseline = fixed.replace(needle, b'self.callbacks.clear();')
    env = dict(os.environ, PATH=str(REPO / 'osdk/target/debug') + ':' + str(ROOT / 'toolchain/bin') + ':' + os.environ['PATH'], CARGO_NET_OFFLINE='true')
    results = []
    created = False
    try:
        with CONFIG.open('x') as stream:
            stream.write('[boot]\nmethod = "grub-rescue-iso"\n[grub]\nboot_protocol = "multiboot2"\n[qemu]\nargs = "-machine q35,accel=tcg -cpu max -m 2048M -smp 2 -display none -monitor none -serial stdio -no-reboot -nic none -device isa-debug-exit,iobase=0xf4,iosize=0x04"\n[test.boot]\nkcmd_args = ["earlycon", "console=ttyS0"]\n')
        created = True
        for name, content, args in [('legacy-clear-operation', baseline, [TEST]), ('fixed-full-ostd', fixed, [])]:
            SOURCE.write_bytes(content)
            print('Executing isolated actual kernel: ' + name, flush=True)
            logpath = OUT / (name + '.log')
            with logpath.open('w') as log:
                process = subprocess.run(['cargo', 'osdk', 'test', '--strip-elf', *args], cwd=REPO / 'ostd', env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, timeout=600)
            text = re.sub(r'\x1b\[[0-9;]*[mK]', '', logpath.read_text())
            counts = re.findall(r'test result: ok\. ([1-9][0-9]*) passed; 0 failed;', text)
            passed = process.returncode == 0 and bool(counts) and '[ktest runner] All crates tested.' in text and TEST in text
            generated = (REPO / 'target/osdk/ostd-test-base/Cargo.toml').read_text()
            if str(REPO / 'osdk') not in generated:
                raise RuntimeError('OSDK generated dependencies outside the isolated source')
            failed_capacity = process.returncode != 0 and TEST in text and 'FAILED' in text and 'panicked' in text.lower() and 'right: 0' in text
            expected = failed_capacity if args else passed
            results.append(dict(name=name, exit=process.returncode, expected=expected, kernel_passed=passed, baseline_assertion=failed_capacity, reported_counts=[int(x) for x in counts], explicit_skips=re.findall(r'\[skipped:[^\]]+\]', text), log=str(logpath)))
            print(results[-1], flush=True)
            if not expected:
                raise RuntimeError('Unexpected real kernel result: ' + name)
    finally:
        SOURCE.write_bytes(fixed)
        if created:
            CONFIG.unlink()
        (OUT / 'retire-storage-results.json').write_text(json.dumps(dict(runs=results, source_restored=SOURCE.read_bytes() == fixed, source_sha256=hashlib.sha256(fixed).hexdigest(), isolated_source=str(REPO), baseline_note='Same incoming clear() operation, extracted into the private cleanup helper for the capacity regression; not a byte-identical old source.', hardware_gpu=False, shared_tree_modified=False), indent=2) + '\n')


if __name__ == '__main__':
    main()
