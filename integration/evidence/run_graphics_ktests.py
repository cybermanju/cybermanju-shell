#!/usr/bin/env python3
"""Run focused Asterinas graphics ktests in bounded QEMU TCG sessions."""
import argparse
import os
from pathlib import Path
import subprocess

CONFIG = '''[boot]
method = "vmm-direct"
[grub]
boot_protocol = "linux"
[qemu]
args = "-machine q35,accel=tcg -cpu max -m 2048M -smp 1 -nographic -monitor none -serial stdio -no-reboot -nic none -device isa-debug-exit,iobase=0xf4,iosize=0x04"
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout", type=int, default=300)
    args = parser.parse_args()
    root = args.root.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env["PATH"] = str(root.parent / "toolchain/bin") + os.pathsep + env["PATH"]
    env["VDSO_LIBRARY_DIR"] = str(root.parent / "vendor-drivers/linux_vdso")
    failed = False
    for crate, test_filter in (("drm", "dumb_buffer"), ("simpledrm", "")):
        directory = root / "kernel/comps" / crate
        config = directory / "OSDK.toml"
        created = False
        try:
            with config.open("x") as handle:
                handle.write(CONFIG)
            created = True
            command = ["timeout", "--kill-after=10", str(args.timeout), "cargo", "osdk", "test", "--linux-x86-legacy-boot", "--strip-elf"]
            if test_filter:
                command.append(test_filter)
            print(f"Running {crate}: {' '.join(command)}", flush=True)
            with (args.output / f"{crate}-ktest.log").open("w") as log:
                result = subprocess.run(command, cwd=directory, env=env, stdout=log, stderr=subprocess.STDOUT, check=False)
            (args.output / f"{crate}-ktest.exit").write_text(str(result.returncode) + "\n")
            print(f"{crate}: exit={result.returncode}; see {args.output / (crate + '-ktest.log')}", flush=True)
            failed |= result.returncode != 0
        finally:
            if created:
                config.unlink()
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
