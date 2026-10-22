import json
import subprocess
import sys
import tempfile
from pathlib import Path
import unittest
from verify_framebuffer_offset_init import MARKERS, SUMMARY, verify


def log(*items, summary=SUMMARY, crlf=False):
    text = "\n".join((*items, summary)) + "\n"
    return text.replace("\n", "\r\n") if crlf else text


class OffsetInitVerifierTests(unittest.TestCase):
    def test_runner_and_ci_wire_the_isolated_mode_without_dropping_gates(self):
        root = Path(__file__).resolve().parents[2]
        runner = (root / "tools/ci/test_framebuffer_runtime.sh").read_text()
        workflow = (root / ".github/workflows/checks.yml").read_text()
        for required in (
            "offset-init) CRATE=aster-framebuffer",
            "PATHCRATE=kernel/core/comps/framebuffer",
            "FILTER=cybermanju_framebuffer_offset_init_runtime",
            "framebuffer-runtime.OSDK.toml",
            "verify_framebuffer_offset_init.py",
            "timeout --signal=TERM --kill-after=15s 600",
        ):
            self.assertIn(required, runner)
        self.assertIn("artifacts/framebuffer-offset-init --kind offset-init", workflow)
        self.assertIn("artifacts/framebuffer-offset-init/*.json", workflow)
        for gate in ("--kind contract", "--kind hardware", "--kind fbdev"):
            self.assertIn(gate, workflow)

    def test_valid_exact_markers_and_json_scope(self):
        result = verify(log(*MARKERS, crlf=True))
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["scope"], "framebuffer_synthetic_descriptor_real_acquisition")
        for key in ("production_constructor_verified", "actual_mmio_acquisition_verified", "reacquisition_verified", "bytes_restored", "last_view_retains_reservation_verified", "synthetic_descriptor"):
            self.assertIs(result[key], True)
        for key in ("bootloader_unaligned_framebuffer_verified", "userspace_syscall_verified", "mmap_userspace_verified", "gpui_native_verified", "linux_modules_executed"):
            self.assertIs(result[key], False)
        self.assertEqual(json.loads(json.dumps(result)), result)
        colored = SUMMARY.replace("ok", "\x1b[32mok\x1b[39m")
        self.assertEqual(verify(log(*MARKERS, summary=colored)), result)

    def test_rejects_missing_duplicate_echo_corruption_extra(self):
        cases = [
            MARKERS[:-1],
            (*MARKERS, MARKERS[0]),
            (*MARKERS[:-1], "$ echo " + MARKERS[-1]),
            (*MARKERS[:-1], MARKERS[-1].replace("false", "true")),
            (*MARKERS, "CYBERMANJU_FB_OFFSET_INIT:extra:PASS"),
            (*MARKERS, "CYBERMANJU_FRAMEBUFFER:io_readback:PASS rows=2 row_bytes=8"),
        ]
        for evidence in cases:
            with self.subTest(evidence=evidence), self.assertRaises(ValueError):
                verify(log(*evidence))

    def test_rejects_summary_failures_panics_cr_and_other_kind(self):
        for summary in ("test result: ok. 2 passed; 0 failed", "test result: FAILED", "test result: ok. 1 passed; 1 failed; 0 filtered out"):
            with self.subTest(summary=summary), self.assertRaises(ValueError):
                verify(log(*MARKERS, summary=summary))
        for extra in ("FAIL", "assertion failed at runtime", "thread panicked at runtime", "test result: ok. 1 passed; 1 failed; 0 filtered out", "noise\rwithout LF"):
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                verify(log(*MARKERS, extra))
        with self.assertRaises(ValueError):
            verify(log(*MARKERS), "hardware")

    def test_osdk_filtered_counts_are_not_cargo_test_counts(self):
        for filtered in (0, 4, 142):
            self.assertEqual(verify(log(*MARKERS, summary=f"test result: ok. 1 passed; 0 failed; {filtered} filtered out."))["status"], "passed")
        with self.assertRaises(ValueError):
            verify(log(*MARKERS, summary="test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"))

    def test_cli_rejects_invalid_utf8_and_never_writes_success(self):
        with tempfile.TemporaryDirectory() as temp:
            source = Path(temp) / "runtime.log"
            target = Path(temp) / "result.json"
            source.write_bytes(log(*MARKERS).encode() + b"\xff")
            result = subprocess.run([sys.executable, str(Path(__file__).with_name("verify_framebuffer_offset_init.py")), str(source), "offset-init", str(target)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(target.exists())

    def test_rejects_contradictory_fail_marker(self):
        with self.assertRaises(ValueError):
            verify(log(*MARKERS, "test cybermanju_framebuffer_offset_init_runtime ... FAILED"))


if __name__ == "__main__":
    unittest.main()
