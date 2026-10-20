#!/usr/bin/env python3
"""Synthetic runner tests; no kernel build, GPU, or real QEMU boot is performed."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest import mock

from run_nvport_smoke import MARKER, SCOPE, capture_qemu, qemu_command, run_smoke


class NvportSmokeRunnerTests(unittest.TestCase):
    def run_capture(self, code, timeout=3):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        serial = root / "serial.log"
        stderr = root / "qemu-stderr.log"
        capture_qemu([sys.executable, "-u", "-c", code], serial, stderr, timeout)
        return serial.read_bytes(), stderr.read_bytes()

    def test_qemu_command_is_tcg_only_without_network_or_passthrough(self):
        command = qemu_command(Path("/tmp/kernel.iso"))
        self.assertIn("q35,accel=tcg", command)
        # QEMU rejects -accel together with -machine accel=, even for TCG.
        self.assertNotIn("-accel", command)
        self.assertIn("-nic", command)
        self.assertIn("none", command)
        self.assertNotIn("kvm", " ".join(command).lower())
        self.assertFalse(any("vfio" in item.lower() or "host=" in item.lower() for item in command))
        self.assertEqual(command[-2:], ["-cdrom", "/tmp/kernel.iso"])

    def test_accepts_exact_complete_marker_and_terminates_qemu_immediately(self):
        start = time.monotonic()
        serial, _ = self.run_capture(
            f"import sys,time; print({MARKER.decode()!r}, flush=True); time.sleep(30)",
            timeout=4,
        )
        self.assertEqual(serial.decode().strip(), MARKER.decode())
        self.assertLess(time.monotonic() - start, 3)

    def test_rejects_marker_with_prefix_or_suffix(self):
        for line in (b"noise " + MARKER, MARKER + b" extra"):
            code = f"import sys; sys.stdout.buffer.write({line + bytes([10])!r}); sys.stdout.flush()"
            with self.subTest(line=line), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                with self.assertRaisesRegex(RuntimeError, "process exited"):
                    capture_qemu([sys.executable, "-u", "-c", code], root / "serial.log", root / "stderr.log", 3)

    def test_rejects_panic_before_marker_even_if_marker_follows(self):
        code = f"import sys; sys.stdout.buffer.write(b'Kernel panic: framebuffer\\n' + {MARKER + bytes([10])!r}); sys.stdout.flush()"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(RuntimeError, "panic"):
                capture_qemu([sys.executable, "-u", "-c", code], root / "serial.log", root / "stderr.log", 3)

    def test_rejects_empty_serial_and_process_exit_without_marker(self):
        for code, expected in (("pass", "empty serial"), ("print('boot ended')", "process exited")):
            with self.subTest(code=code), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                with self.assertRaisesRegex(RuntimeError, expected):
                    capture_qemu([sys.executable, "-u", "-c", code], root / "serial.log", root / "stderr.log", 3)

    def test_rejects_timeout_and_preserves_serial(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            code = "import sys,time; print('booting', flush=True); time.sleep(30)"
            with self.assertRaisesRegex(RuntimeError, "timed out"):
                capture_qemu([sys.executable, "-u", "-c", code], root / "serial.log", root / "stderr.log", 1)
            self.assertIn("booting", (root / "serial.log").read_text())

    def test_result_json_records_cpu_only_scope_hash_logs_and_pass(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = root / "kernel.iso"
            image.write_bytes(b"synthetic ISO")
            output = root / "out"
            with mock.patch("run_nvport_smoke.capture_qemu") as capture:
                result = run_smoke(image, output, 10)
            capture.assert_called_once()
            self.assertTrue(result["pass"])
            self.assertIn("CPU-only", result["scope"])
            self.assertIn("not a GPU test", result["scope"])
            self.assertEqual(result["image_sha256"], hashlib.sha256(b"synthetic ISO").hexdigest())
            saved = json.loads((output / "result.json").read_text())
            self.assertEqual(saved, result)
            self.assertEqual(result["log"], "serial.log")
            self.assertEqual(result["scope"], SCOPE)

    def test_failure_result_is_written_and_never_passes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = root / "kernel.iso"
            image.write_bytes(b"synthetic ISO")
            output = root / "out"
            with mock.patch("run_nvport_smoke.capture_qemu", side_effect=RuntimeError("synthetic panic")):
                result = run_smoke(image, output, 10)
            self.assertFalse(result["pass"])
            self.assertEqual(result["error"], "synthetic panic")
            self.assertTrue((output / "serial.log").exists())
            self.assertTrue((output / "qemu-stderr.log").exists())
            self.assertFalse(json.loads((output / "result.json").read_text())["pass"])


if __name__ == "__main__":
    unittest.main()
