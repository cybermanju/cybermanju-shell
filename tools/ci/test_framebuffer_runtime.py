import unittest
from verify_framebuffer_runtime import verify

SUMMARY = "test result: ok. 1 passed; 0 failed; 0 filtered out."
CONTRACT = "CYBERMANJU_FRAMEBUFFER_CONTRACT:PASS"
BOOT = "CYBERMANJU_FRAMEBUFFER:boot:PASS width=1024 height=768 bpp=32 pitch=4096 bytes=3145728"
IO = "CYBERMANJU_FRAMEBUFFER:io_readback:PASS rows=2 row_bytes=8"
BOUNDS = "CYBERMANJU_FRAMEBUFFER:surface_bounds:PASS cases=6 bytes=24"


def log(*evidence):
    return "\n".join((*evidence, SUMMARY))


class RuntimeVerifierTests(unittest.TestCase):
    def test_contract_and_ansi_cr(self):
        result = verify(log("\x1b[32m" + CONTRACT + "\x1b[0m").replace("\n", "\r\n"), "contract")
        self.assertEqual(result["kind"], "contract")
        for key in ("hardware_acceleration_verified", "gpui_native_verified", "physical_scanout_verified"):
            self.assertFalse(result[key])
        self.assertNotIn("surface_bounds", result)

    def test_valid_hardware_exports_measurements(self):
        result = verify(log(BOOT, IO, BOUNDS), "hardware")
        self.assertEqual(result["framebuffer"], {"width": 1024, "height": 768, "bpp": 32, "pitch": 4096, "bytes": 3145728, "rows": 2, "row_bytes": 8})
        self.assertEqual(result["surface_bounds"], {"cases": 6, "bytes": 24, "synthetic_subsurface": True})
        self.assertTrue(all(v is False for k, v in result.items() if k.endswith("_verified")))

    def test_missing_contradictory_duplicate_and_unknown_markers(self):
        invalid = (
            ((), "contract"), ((CONTRACT, CONTRACT), "contract"),
            ((CONTRACT, BOOT, IO, BOUNDS), "contract"), ((CONTRACT, BOUNDS), "contract"),
            ((BOOT,), "hardware"), ((BOOT, IO), "hardware"),
            ((BOOT, BOUNDS), "hardware"), ((IO, BOUNDS), "hardware"),
            ((BOOT, IO, BOUNDS, CONTRACT), "hardware"),
            ((BOOT, BOOT, IO, BOUNDS), "hardware"),
            ((BOOT, IO, IO, BOUNDS), "hardware"),
            ((BOOT, IO, BOUNDS, BOUNDS), "hardware"),
            ((BOOT, IO, BOUNDS, "CYBERMANJU_FRAMEBUFFER:other:PASS"), "hardware"),
        )
        for evidence, kind in invalid:
            with self.subTest(evidence=evidence):
                with self.assertRaises(ValueError):
                    verify(log(*evidence), kind)

    def test_echo_partial_and_unrecognized_log_lines_rejected(self):
        for text in (log("$ echo " + CONTRACT), log("prefix " + CONTRACT), log(CONTRACT + " trailing")):
            with self.subTest(text=text), self.assertRaises(ValueError):
                verify(text, "contract")
        for marker in ("prefix " + BOUNDS, BOUNDS + " trailing", BOUNDS.replace("cases=6", "cases=5"), BOUNDS.replace("bytes=24", "bytes=25")):
            with self.subTest(marker=marker), self.assertRaises(ValueError):
                verify(log(BOOT, IO, marker), "hardware")
        with self.assertRaises(ValueError):
            verify(log(BOOT.replace("width=1024", "width=1024 extra"), IO, BOUNDS), "hardware")

    def test_exactly_one_success_summary_and_failure_rejection(self):
        invalid = (SUMMARY, "test result: FAILED. 0 passed; 1 failed;", "runner failed: 0 failures", "FAILED assertion panic", "test result: ok. 1 passed; 1 failed; 0 filtered out.", "[caught panic] Panicked at source.rs:5", "Panicked at source.rs:5", "assertion failed: logical span")
        for failure in invalid:
            with self.subTest(failure=failure), self.assertRaises(ValueError):
                verify(log(CONTRACT, failure), "contract")

    def test_overflow_zero_geometry_rowbytes_and_pitch_mismatch(self):
        invalid_boots = (
            BOOT.replace("width=1024", "width=0"),
            BOOT.replace("width=1024", f"width={2**64}"),
            BOOT.replace("height=768", f"height={2**64}"),
            BOOT.replace("bytes=3145728", f"bytes={2**64}"),
            BOOT.replace("pitch=4096", "pitch=4095"),
            BOOT.replace("bytes=3145728", "bytes=3145727"),
        )
        for boot in invalid_boots:
            with self.subTest(boot=boot), self.assertRaises(ValueError):
                verify(log(boot, IO, BOUNDS), "hardware")
        with self.assertRaises(ValueError):
            verify(log(BOOT, IO.replace("row_bytes=8", "row_bytes=7"), BOUNDS), "hardware")


if __name__ == "__main__":
    unittest.main()
