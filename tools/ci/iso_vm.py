#!/usr/bin/env python3
"""Drive the real Linux installer ISO over ttyS0; no direct kernel boot or test-only ISO.

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
from boot_metrics import timed_phase


class Console:
    def __init__(self, process):
        self.process = process
        self.buffer = b""
        self.control_state = "text"
        self.selector = selectors.DefaultSelector()
        self.selector.register(process.stdout, selectors.EVENT_READ)

    def append_output(self, data):
        # Keep raw output in the logs, but match terminal-visible text. systemd
        # login emits OSC metadata immediately before the readiness marker.
        # State persists across reads: an escape may be split at any byte.
        visible = bytearray()
        for byte in data:
            if self.control_state == "text":
                if byte == 0x1b:
                    self.control_state = "escape"
                else:
                    visible.append(byte)
            elif self.control_state == "escape":
                if byte == ord("]"):
                    self.control_state = "osc"
                elif byte == ord("["):
                    self.control_state = "csi"
                else:
                    self.control_state = "text"
            elif self.control_state == "csi":
                if 0x40 <= byte <= 0x7e:
                    self.control_state = "text"
            elif self.control_state == "osc":
                if byte == 0x07:
                    self.control_state = "text"
                elif byte == 0x1b:
                    self.control_state = "osc-escape"
            elif self.control_state == "osc-escape":
                if byte in (ord("\\"), 0x07):
                    self.control_state = "text"
                elif byte != 0x1b:
                    self.control_state = "osc"
        self.buffer = (self.buffer + visible)[-1024 * 1024:]

    def expect(self, pattern, seconds):
        deadline = time.monotonic() + seconds
        while True:
            match = re.search(pattern, self.buffer, re.MULTILINE)
            if match:
                self.buffer = self.buffer[match.end():]
                return match
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
            self.append_output(data)

    def send(self, command):
        self.process.stdin.write((command + "\n").encode())
        self.process.stdin.flush()

    def checked(self, command, seconds=120):
        # Match a complete line, never the shell's echoed command or stale logs.
        token = "CI_" + uuid.uuid4().hex
        # Run in a subshell so `exit` and shell options cannot suppress reporting.
        # A failed command should fail now, not look like a missing console later.
        self.send(f"( {command} ); ci_status=$?; "
                  f"printf '\\n%s%s:%s\\n' '{token[:10]}' '{token[10:]}' \"$ci_status\"")
        match = self.expect(rb"^" + token.encode() + rb":([0-9]+)\r?$", seconds)
        status = int(match.group(1))
        if status:
            raise RuntimeError(f"Guest command failed (exit {status}): {command}")


def run(phase):
    root = Path(__file__).resolve().parents[2] / "asterinas"
    artifacts = root.parent / "artifacts"
    selected, reason = settings()
    acceleration = selected["QEMU_ACCEL"]
    print(f"QEMU acceleration: {acceleration} ({reason})", flush=True)
    env = dict(os.environ, **selected, CONSOLE="ttyS0", QEMU_DISPLAY="none",
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
        with timed_phase(artifacts, f"{phase}-iso-boot-readiness", acceleration):
            console.expect(rb"^CYBERMANJU_LIVE_INSTALLER_READY\r?$", TIMEOUTS["ISO_VM_BOOT_SECONDS"])
        console.send("stty -echo; set +o emacs +o vi")
        with timed_phase(artifacts, f"{phase}-live-checks", acceleration):
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
            with timed_phase(artifacts, f"{phase}-install", acceleration):
                console.send(
                    "aster-nixos-install --config /root/configuration.nix "
                    "--disk /dev/vda --force-format-disk"
                )
                console.expect(
                    rb"^Congratulations! Asterinas NixOS has been installed successfully!\r?$",
                    TIMEOUTS["ISO_VM_INSTALL_SECONDS"],
                )
            with timed_phase(artifacts, f"{phase}-verify-install", acceleration):
                console.checked(
                    'test -b /dev/vda1 && test -b /dev/vda2 && '
                    'mkdir -p /tmp/ci-esp && mount -r /dev/vda1 /tmp/ci-esp && '
                    'test -s /tmp/ci-esp/EFI/BOOT/BOOTX64.EFI && '
                    'test -s /tmp/ci-esp/grub/grub.cfg && '
                    'grep -q asterinas /tmp/ci-esp/grub/grub.cfg && '
                    'umount /tmp/ci-esp',
                    TIMEOUTS["ISO_VM_COMMAND_SECONDS"],
                )
        with timed_phase(artifacts, f"{phase}-shutdown", acceleration):
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
        print(f"PASS: {phase} using the full interactive ISO (UEFI, {acceleration}).", flush=True)
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
