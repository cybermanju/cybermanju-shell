#!/usr/bin/env python3
"""Drive the real Linux installer ISO over ttyS0; no direct kernel boot or test-only ISO.

The live installer kernel is Linux. Only the subsequently installed system is
Asterinas. Installation is online and uses the configured Nix binary caches.
"""
import argparse
import json
import math
import os
from pathlib import Path
import re
import selectors
import shlex
import signal
import subprocess
import sys
import time
import uuid

from vm_settings import TIMEOUTS, settings
from boot_metrics import timed_phase


INSTALLER_PHASES = rb"(?:build-system|install-system|prune-generations|collect-garbage)"


def parse_installer_phase(line):
    begin = re.fullmatch(rb"CYBERMANJU_INSTALL_PHASE:begin phase=(" + INSTALLER_PHASES + rb")", line)
    if begin:
        return {"name": begin[1].decode("ascii"), "event": "begin", "exit_status": None}
    end = re.fullmatch(rb"CYBERMANJU_INSTALL_PHASE:end phase=(" + INSTALLER_PHASES + rb") status=([0-9]{1,3})", line)
    if end and int(end[2]) <= 255:
        return {"name": end[1].decode("ascii"), "event": "end", "exit_status": int(end[2])}
    return None


OBSERVER_PREFIX = b"CYBERMANJU_INSTALL_OBSERVER:"
DIAGNOSTIC_LINE_LIMIT = 128 * 1024


def parse_installer_process_sample(line):
    """Accept bounded explicit process observations, never Nix copy-path text."""
    if not line.startswith(OBSERVER_PREFIX) or len(line) > DIAGNOSTIC_LINE_LIMIT:
        return None
    try:
        sample = json.loads(line[len(OBSERVER_PREFIX):].decode("ascii"))
        keys = {"version", "event", "phase", "parent_pid", "sample", "elapsed_seconds", "processes"}
        if not isinstance(sample, dict) or set(sample) != keys:
            return None
        if type(sample["version"]) is not int or sample["version"] != 1 or sample["event"] != "sample" or sample["phase"] != "install-system":
            return None
        if type(sample["parent_pid"]) is not int or not 0 < sample["parent_pid"] < 2**31:
            return None
        if type(sample["sample"]) is not int or not 1 <= sample["sample"] <= 240:
            return None
        elapsed = sample["elapsed_seconds"]
        if type(elapsed) not in (int, float) or not math.isfinite(elapsed) or elapsed < 0:
            return None
        processes = sample["processes"]
        if not isinstance(processes, list) or not 1 <= len(processes) <= 12:
            return None
        process_keys = {"pid", "ppid", "comm", "state", "utime_ticks", "stime_ticks", "starttime", "argv", "argv_truncated", "wchan", "io", "unavailable"}
        pids = set()
        for process in processes:
            if not isinstance(process, dict) or set(process) != process_keys:
                return None
            pid = process["pid"]
            if type(pid) is not int or not 0 < pid < 2**31 or pid in pids:
                return None
            pids.add(pid)
            for field in ("ppid", "utime_ticks", "stime_ticks", "starttime"):
                value = process[field]
                if value is not None and (type(value) is not int or not 0 <= value < 2**64):
                    return None
            for field, limit in (("comm", 256), ("state", 1), ("argv", 4096), ("wchan", 512)):
                value = process[field]
                if value is not None and (not isinstance(value, str) or len(value) > limit):
                    return None
            if type(process["argv_truncated"]) is not bool:
                return None
            unavailable = process["unavailable"]
            if not isinstance(unavailable, list) or len(unavailable) > 4 or any(v not in ("stat", "cmdline", "wchan", "io", "identity_changed") for v in unavailable):
                return None
            io = process["io"]
            if io is not None:
                if not isinstance(io, dict) or not set(io) <= {"rchar", "wchar", "syscr", "syscw", "read_bytes", "write_bytes", "cancelled_write_bytes"}:
                    return None
                if any(v is not None and (type(v) is not int or not 0 <= v < 2**64) for v in io.values()):
                    return None
        return sample if sample["parent_pid"] in pids else None
    except (ValueError, TypeError, OverflowError, RecursionError):
        return None


class Console:
    def __init__(self, process):
        self.process = process
        self.buffer = b""
        self.control_state = "text"
        self.received_bytes = 0
        self.last_output_at = None
        self.phase_line_tail = b""
        self.last_installer_phase = None
        self.first_installer_failure = None
        self.last_installer_process_sample = None
        self.installer_process_samples_seen = 0
        self.last_non_observer_line_at = None
        self.selector = selectors.DefaultSelector()
        self.selector.register(process.stdout, selectors.EVENT_READ)

    def append_output(self, data):
        if data:
            self.received_bytes = getattr(self, "received_bytes", 0) + len(data)
            self.last_output_at = time.monotonic()
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
        # Keep structured phase evidence even when verbose Nix output evicts it
        # from the bounded console buffer. Parse complete visible lines only.
        phase_lines = (getattr(self, "phase_line_tail", b"") + visible).split(b"\n")
        tail = phase_lines.pop()
        was_truncated = getattr(self, "phase_line_truncated", False)
        self.phase_line_tail = tail[-DIAGNOSTIC_LINE_LIMIT:]
        self.phase_line_truncated = len(tail) > DIAGNOSTIC_LINE_LIMIT or (was_truncated and not phase_lines)
        for index, line in enumerate(phase_lines):
            if index == 0 and was_truncated:
                continue  # Lost line origin must not create an anchored marker.
            line = line.rstrip(b"\r")
            if line and not line.startswith(OBSERVER_PREFIX):
                self.last_non_observer_line_at = time.monotonic()
            observed = parse_installer_process_sample(line)
            if observed is not None:
                self.last_installer_process_sample = observed
                self.installer_process_samples_seen = getattr(self, "installer_process_samples_seen", 0) + 1
            phase = parse_installer_phase(line)
            if phase is not None:
                self.last_installer_phase = phase
                if phase["event"] == "end" and phase["exit_status"] != 0 and getattr(self, "first_installer_failure", None) is None:
                    self.first_installer_failure = phase
        self.buffer = (self.buffer + visible)[-1024 * 1024:]

    def expect(self, pattern, seconds, *, reject_installer_failure=False):
        deadline = time.monotonic() + seconds
        while True:
            failure = getattr(self, "first_installer_failure", None)
            if reject_installer_failure and failure is not None:
                raise RuntimeError(f"Installer phase {failure['name']} failed (exit {failure['exit_status']})")
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

    def write_failure_snapshot(self, path, error):
        last_output = getattr(self, "last_output_at", None)
        last_non_observer = getattr(self, "last_non_observer_line_at", None)
        payload = {
            "schema_version": 1,
            "status": "diagnostic_only",
            "error_type": type(error).__name__,
            "error": str(error),
            "launcher_returncode": self.process.poll(),
            "observed_output_bytes": getattr(self, "received_bytes", 0),
            "seconds_since_output": None if last_output is None else max(0, time.monotonic() - last_output),
            "last_observed_installer_phase": getattr(self, "last_installer_phase", None),
            "first_observed_installer_failure": getattr(self, "first_installer_failure", None),
            "last_observed_installer_process_sample": getattr(self, "last_installer_process_sample", None),
            "installer_process_samples_seen": getattr(self, "installer_process_samples_seen", 0),
            "seconds_since_non_observer_line": None if last_non_observer is None else max(0, time.monotonic() - last_non_observer),
            "process_observations_are_installation_success": False,
            "serial_tail": self.buffer[-8192:].decode("utf-8", errors="replace"),
            "installation_verified": False,
            "installed_boot_verified": False,
        }
        path = Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(payload, indent=2) + "\n")

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


def activation_trace_config_command(source="/root/configuration.nix", destination="/root/ci-configuration.nix"):
    # Embed the original module expression, not an absolute /root import that
    # would break future evaluation after the installer copies it to /etc/nixos.
    # Both files stay in the same directory, preserving relative Nix imports.
    if Path(source).parent != Path(destination).parent or source == destination:
        raise ValueError("CI config must be a distinct file in the original directory")
    src, dst = shlex.quote(source), shlex.quote(destination)
    return (f"test -s {src} && ( set -C; {{ "
            "printf '%s\\n' '{ imports = [ ('; "
            f"cat {src}; "
            "printf '\\n%s\\n' ') ]; aster_nixos.trace-activation = true; }'; "
            f"}} > {dst} ) && nix-instantiate --parse {dst} >/dev/null")


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
            with timed_phase(artifacts, f"{phase}-diagnostic-config", acceleration):
                console.checked(activation_trace_config_command(), TIMEOUTS["ISO_VM_COMMAND_SECONDS"])
            with timed_phase(artifacts, f"{phase}-install", acceleration):
                try:
                    console.send(
                        "CYBERMANJU_INSTALL_OBSERVER=1 aster-nixos-install --config /root/ci-configuration.nix "
                        "--disk /dev/vda --force-format-disk"
                    )
                    console.expect(
                        rb"^Congratulations! Asterinas NixOS has been installed successfully!\r?$",
                        TIMEOUTS["ISO_VM_INSTALL_SECONDS"],
                        reject_installer_failure=True,
                    )
                except (TimeoutError, RuntimeError, OSError) as error:
                    # Diagnostic only: retain the original exception/gates. Do
                    # not send commands, interrupt the guest or extend its wait.
                    try:
                        console.write_failure_snapshot(artifacts / "iso-install-failure-snapshot.json", error)
                    except OSError as diagnostic_error:
                        print(f"Could not save installation snapshot: {diagnostic_error}", file=sys.stderr)
                    raise
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
