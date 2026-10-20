#!/usr/bin/env python3
"""Drive the real ISO over hvc0; no direct kernel boot or test-only ISO.

The live installer kernel is Linux. Only the subsequently installed system is
Asterinas. Installation is online and uses the configured Nix binary caches.
"""
import argparse
import os
from pathlib import Path
import re
import selectors
import signal
import subprocess
import sys
import time
import uuid

from vm_settings import TIMEOUTS, settings


class Console:
    def __init__(self, process):
        self.process = process
        self.buffer = b""
        self.selector = selectors.DefaultSelector()
        self.selector.register(process.stdout, selectors.EVENT_READ)

    def expect(self, pattern, seconds):
        deadline = time.monotonic() + seconds
        while True:
            match = re.search(pattern, self.buffer, re.MULTILINE)
            if match:
                self.buffer = self.buffer[match.end():]
                return
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(f"VM console timed out waiting for {pattern!r}")
            if not self.selector.select(min(1, remaining)):
                continue
            data = os.read(self.process.stdout.fileno(), 65536)
            if not data:
                raise RuntimeError(f"VM exited before expected output: {pattern!r}")
            sys.stdout.buffer.write(data)
            sys.stdout.buffer.flush()
            self.buffer = (self.buffer + data)[-1024 * 1024:]

    def send(self, command):
        self.process.stdin.write((command + "\n").encode())
        self.process.stdin.flush()

    def checked(self, command, seconds=120):
        # Match a complete line, never the shell's echoed command or stale logs.
        token = "CI_" + uuid.uuid4().hex
        self.send(f"{command} && printf '\\n%s%s\\n' '{token[:10]}' '{token[10:]}'")
        self.expect(rb"^" + token.encode() + rb"\r?$", seconds)


def run(phase):
    root = Path(__file__).resolve().parents[2] / "asterinas"
    selected, reason = settings()
    print(f"QEMU acceleration: {selected['QEMU_ACCEL']} ({reason})", flush=True)
    env = dict(os.environ, **selected, CONSOLE="hvc0", QEMU_DISPLAY="none",
               MEM="4G", SMP="2", OVMF="on", BOOT_METHOD="", ISO_BOOT_MODE="cdrom")
    if phase == "live-usb":
        env["ISO_BOOT_MODE"] = "usb"
    process = subprocess.Popen(
        ["bash", "tools/nixos/run.sh", "iso"], cwd=root, env=env,
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        start_new_session=True,
    )
    console = Console(process)
    try:
        console.expect(rb"^CYBERMANJU_LIVE_INSTALLER_READY\r?$", TIMEOUTS["ISO_VM_BOOT_SECONDS"])
        console.send("stty -echo; set +o emacs +o vi")
        console.checked(
            'test "$(uname -s)" = Linux && '
            'command -v aster-nixos-install && command -v nix-build && '
            'command -v nixos-install && test -s /root/configuration.nix && '
            'test -b /dev/vda && test ! -b /dev/vda1',
            TIMEOUTS["ISO_VM_COMMAND_SECONDS"],
        )
        if phase == "install":
            # This formats only the newly created disposable guest disk.
            # The same published ISO remains interactive on all real hardware.
            console.send(
                "aster-nixos-install --config /root/configuration.nix "
                "--disk /dev/vda --force-format-disk"
            )
            console.expect(
                rb"^Congratulations! Asterinas NixOS has been installed successfully!\r?$",
                TIMEOUTS["ISO_VM_INSTALL_SECONDS"],
            )
            console.checked(
                'test -b /dev/vda1 && test -b /dev/vda2 && '
                'mkdir -p /tmp/ci-esp && mount -r /dev/vda1 /tmp/ci-esp && '
                'test -s /tmp/ci-esp/EFI/BOOT/BOOTX64.EFI && '
                'test -s /tmp/ci-esp/grub/grub.cfg && '
                'grep -q asterinas /tmp/ci-esp/grub/grub.cfg && '
                'umount /tmp/ci-esp',
                TIMEOUTS["ISO_VM_COMMAND_SECONDS"],
            )
        console.checked("sync", TIMEOUTS["ISO_VM_COMMAND_SECONDS"])
        console.send("poweroff")
        # Drain output until EOF and require real shutdown, not just a marker.
        try:
            console.expect(rb"(?!)", TIMEOUTS["ISO_VM_SHUTDOWN_SECONDS"])
        except RuntimeError:
            pass
        code = process.wait(timeout=TIMEOUTS["ISO_VM_PROCESS_WAIT_SECONDS"])
        if code != 0:
            raise RuntimeError(f"VM shutdown returned {code}")
        print(f"PASS: {phase} using the full interactive ISO (UEFI, {selected['QEMU_ACCEL']}).", flush=True)
    finally:
        console.selector.close()
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
        process.stdin.close()
        process.stdout.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=["live-usb", "install"])
    args = parser.parse_args()
    try:
        run(args.phase)
    except (OSError, RuntimeError, TimeoutError, subprocess.TimeoutExpired) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        sys.exit(1)
