#!/usr/bin/env python3
"""Run the NVport CPU memory self-test in a bounded, software-emulated QEMU VM.

This is strictly a CPU-only self-test; it is not a GPU test and does not certify
that the kernel completes a full boot after the marker.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import subprocess
import time

MARKER = b"NVIDIA NVport CPU memory self-test passed (not a GPU test)"
SCOPE = "CPU-only NVIDIA NVport memory self-test; not a GPU test or full-boot certification"
PANIC = re.compile(rb"(?:kernel\s+)?panic(?:\b|:)|panicked at", re.IGNORECASE)
MAX_TIMEOUT_SECONDS = 900


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def qemu_command(image):
    """Build a deliberately restricted QEMU command: TCG, no network or passthrough."""
    return [
        "qemu-system-x86_64",
        "-machine", "q35,accel=tcg",
        "-cpu", "max",
        "-m", "2048",
        "-smp", "2",
        "-display", "none",
        "-monitor", "none",
        "-serial", "stdio",
        "-no-reboot",
        "-nic", "none",
        "-cdrom", str(image),
    ]


def _stop(process):
    if process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()


def capture_qemu(command, serial_path, stderr_path, timeout):
    """Capture serial and QEMU diagnostics; accept only an exact complete marker line."""
    process = subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        bufsize=0,
    )
    streams = {process.stdout: "serial", process.stderr: "stderr"}
    pending = b""
    serial_bytes = 0
    deadline = time.monotonic() + timeout
    try:
        with selectors.DefaultSelector() as selector, serial_path.open("wb") as serial, stderr_path.open("wb") as stderr:
            for stream, kind in streams.items():
                selector.register(stream, selectors.EVENT_READ, kind)
            while selector.get_map():
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise RuntimeError("QEMU timed out before the complete NVport marker")
                events = selector.select(min(0.25, remaining))
                if not events and process.poll() is not None:
                    # Continue until both pipes reach EOF, preserving all captured output.
                    continue
                for key, _ in events:
                    stream = key.fileobj
                    data = os.read(stream.fileno(), 65536)
                    if not data:
                        selector.unregister(stream)
                        continue
                    if key.data == "stderr":
                        stderr.write(data)
                        stderr.flush()
                        continue
                    serial.write(data)
                    serial.flush()
                    serial_bytes += len(data)
                    pending += data
                    lines = pending.split(b"\n")
                    pending = lines.pop()
                    for line in lines:
                        line = line[:-1] if line.endswith(b"\r") else line
                        if PANIC.search(line):
                            raise RuntimeError("Kernel panic appeared on serial before the NVport marker")
                        if line == MARKER:
                            return
                    if PANIC.search(pending):
                        raise RuntimeError("Kernel panic appeared on serial before the NVport marker")
            if pending:
                final_line = pending[:-1] if pending.endswith(b"\r") else pending
                if PANIC.search(final_line):
                    raise RuntimeError("Kernel panic appeared on serial before the NVport marker")
                if final_line == MARKER:
                    return
            if serial_bytes == 0:
                raise RuntimeError("QEMU exited with empty serial output before the NVport marker")
            raise RuntimeError("QEMU process exited before the complete NVport marker")
    finally:
        _stop(process)
        for stream in streams:
            stream.close()


def run_smoke(image_arg, output_arg, timeout):
    output = Path(output_arg).resolve()
    output.mkdir(parents=True, exist_ok=True)
    serial_path = output / "serial.log"
    stderr_path = output / "qemu-stderr.log"
    result_path = output / "result.json"
    result = {
        "scope": SCOPE,
        "image_sha256": None,
        "log": serial_path.name,
        "qemu_stderr_log": stderr_path.name,
        "pass": False,
    }
    try:
        image = Path(image_arg).resolve(strict=True)
        if not image.is_file():
            raise ValueError("--image must name a regular ISO image file")
        result["image_sha256"] = sha256(image)
        capture_qemu(qemu_command(image), serial_path, stderr_path, timeout)
        result["pass"] = True
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        result["error"] = str(error)
    finally:
        # Keep the evidence shape stable even when image validation/QEMU startup fails.
        serial_path.touch(exist_ok=True)
        stderr_path.touch(exist_ok=True)
        result_path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True, type=Path, help="Caller-built bootable ISO image")
    parser.add_argument("--output", required=True, type=Path, help="Directory for result and QEMU logs")
    parser.add_argument("--timeout", required=True, type=int, help="Maximum QEMU runtime in seconds (1..900)")
    args = parser.parse_args(argv)
    if not 1 <= args.timeout <= MAX_TIMEOUT_SECONDS:
        parser.error(f"--timeout must be 1..{MAX_TIMEOUT_SECONDS} seconds")
    result = run_smoke(args.image, args.output, args.timeout)
    if not result["pass"]:
        print(f"NVport CPU smoke failed: {result['error']}")
        return 1
    print(f"NVport CPU smoke passed; result: {(args.output / 'result.json').resolve()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
