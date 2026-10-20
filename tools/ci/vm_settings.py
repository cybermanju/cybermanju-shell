#!/usr/bin/env python3
"""Choose usable KVM or explicit TCG; provide shared, TCG-sized CI deadlines.

Device presence/access is not a successful KVM or QEMU probe. Actual boot/install
acceptance still requires a remote run. No virtualization is started here.
"""
import argparse
import json
import os
from pathlib import Path
import shlex
import sys

# Inner deadlines fit within the shell timeouts, which in turn fit within the
# workflow's 35/100/60-minute steps and 350-minute hosted job limit.
TIMEOUTS = {
    "ISO_VM_BOOT_SECONDS": 1200,
    "ISO_VM_COMMAND_SECONDS": 180,
    "ISO_VM_INSTALL_SECONDS": 3600,
    "ISO_VM_SHUTDOWN_SECONDS": 300,
    "ISO_VM_PROCESS_WAIT_SECONDS": 30,
    "ISO_VM_LIVE_TIMEOUT": "33m",
    "ISO_VM_INSTALL_TIMEOUT": "97m",
    "ISO_VM_BOOT_TIMEOUT": "55m",
    "NIXOS_TEST_TIMEOUT": "40min",
}


def select_acceleration(environ=None):
    environ = os.environ if environ is None else environ
    if environ.get("ENABLE_KVM") == "0":
        return "tcg", "software emulation requested by ENABLE_KVM=0"
    try:
        present = Path("/dev/kvm").is_char_device()
    except OSError:
        present = False
    if not present:
        return "tcg", "/dev/kvm is absent or is not a character device"
    if not os.access("/dev/kvm", os.R_OK | os.W_OK):
        return "tcg", "/dev/kvm is not readable and writable"
    return "kvm", "/dev/kvm is present and readable/writable (not a VM probe)"


def settings(environ=None):
    acceleration, reason = select_acceleration(environ)
    return {
        "QEMU_ACCEL": acceleration,
        "ENABLE_KVM": "1" if acceleration == "kvm" else "0",
        **{key: str(value) for key, value in TIMEOUTS.items()},
    }, reason


def record(path, values, reason, phase):
    destination = Path(path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps({
        "phase": phase, "acceleration": values["QEMU_ACCEL"], "reason": reason,
        "timeouts": TIMEOUTS,
    }, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("format", choices=["shell"])
    parser.add_argument("--record")
    parser.add_argument("--phase", default="preflight")
    args = parser.parse_args()
    values, reason = settings()
    print(f"QEMU acceleration: {values['QEMU_ACCEL']} ({reason}); TCG-sized deadlines", file=sys.stderr)
    if args.record:
        record(args.record, values, reason, args.phase)
    for key, value in values.items():
        print(f"export {key}={shlex.quote(value)}")


if __name__ == "__main__":
    main()
