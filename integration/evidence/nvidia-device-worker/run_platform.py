#!/usr/bin/env python3
"""Native OSTD kernel validation; never GPU acceptance or synthetic task execution."""
from pathlib import Path
import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
OUT = ROOT / 'evidence/nvidia-device-worker'
PACKAGE = REPO / 'kernel/comps/nvidia-platform'
SOURCE = PACKAGE / 'src/worker.rs'
CONFIG = PACKAGE / 'OSDK.toml'


def main():
    negative = '--negative' in sys.argv
    original = SOURCE.read_bytes()
    env = dict(os.environ, PATH=str(ROOT / 'toolchain/bin') + ':' + os.environ['PATH'], NVIDIA_SOURCE_DIR=str(ROOT / 'vendor-drivers/open-gpu-kernel-modules-615.78.08-tag'), CARGO_NET_OFFLINE='true')
    stages = [('fixed-platform', original, [])]
    if negative:
        text = original.decode()
        old = 'if state.pending {\n                        state.pending = false;\n                        Some(true)\n                    } else if state.closing {\n                        Some(false)'
        new = 'if state.closing {\n                        Some(false)\n                    } else if state.pending {\n                        state.pending = false;\n                        Some(true)'
        assert text.count(old) == 1, 'Baseline splice must match exactly'
        stages = [('reject-lost-accepted-work', text.replace(old, new).encode(), ['dropping_owner_closes_without_waiting_and_task_still_drains']), ('fixed-platform', original, [])]
    results = []
    created = False
    try:
        with CONFIG.open('x') as config:
            config.write('[boot]\nmethod = "grub-rescue-iso"\n[grub]\nboot_protocol = "multiboot2"\n[qemu]\nargs = "-machine q35,accel=tcg -cpu max -m 2048M -smp 2 -display none -monitor none -serial stdio -no-reboot -nic none -device isa-debug-exit,iobase=0xf4,iosize=0x04"\n[test.boot]\nkcmd_args = ["earlycon", "console=ttyS0"]\n')
        created = True
        for name, source, filters in stages:
            print('Executing ' + name + ' in actual OSTD kernel', flush=True)
            SOURCE.write_bytes(source)
            logpath = OUT / (name + '.log')
            with logpath.open('w') as log:
                process = subprocess.run(['cargo', 'osdk', 'test', *filters, '--strip-elf', '--features', 'nvport-runtime'], cwd=PACKAGE, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, timeout=180)
            text = re.sub(r'\x1b\[[0-9;]*[mK]', '', logpath.read_text())
            counts = re.findall(r'test result: ok\. ([1-9][0-9]*) passed; 0 failed;', text)
            success = process.returncode == 0 and bool(counts) and '[ktest runner] All crates tested.' in text
            is_negative = bool(filters)
            if is_negative:
                expected = process.returncode != 0 and filters[0] in text and 'FAILED' in text and 'panicked' in text.lower() and 'right: 2' in text
            else:
                expected = success and counts == ['10'] and 'actual APIC timer IRQ to sleepable task handoff passed' in text and 'accepted-work drain and capture destruction passed' in text
            result = dict(name=name, exit=process.returncode, expected=expected, kernel_passed=success, counts=[int(n) for n in counts], log=str(logpath))
            results.append(result)
            print(result, flush=True)
            if not expected:
                raise RuntimeError('Unexpected actual-kernel result: ' + name)
    finally:
        SOURCE.write_bytes(original)
        if created:
            CONFIG.unlink()
        (OUT / ('negative-results.json' if negative else 'platform-results.json')).write_text(json.dumps(dict(runs=results, source_restored=SOURCE.read_bytes() == original, source_sha256=hashlib.sha256(original).hexdigest(), hardware_gpu_acceptance=False, rm_initialized=False, remote_actions_run=False), indent=2) + '\n')


if __name__ == '__main__':
    main()
