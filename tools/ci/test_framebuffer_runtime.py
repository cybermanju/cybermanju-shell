import unittest
from verify_framebuffer_runtime import verify

SUMMARY = "test result: ok. 1 passed; 0 failed; 0 filtered out."
CONTRACT = "CYBERMANJU_FRAMEBUFFER_CONTRACT:PASS"
BOOT = "CYBERMANJU_FRAMEBUFFER:boot:PASS width=1024 height=768 bpp=32 pitch=4096 bytes=3145728"
IO = "CYBERMANJU_FRAMEBUFFER:io_readback:PASS rows=2 row_bytes=8"
BOUNDS = "CYBERMANJU_FRAMEBUFFER:surface_bounds:PASS cases=6 bytes=24"
CONSOLE = "CYBERMANJU_FRAMEBUFFER:console_flush:PASS cases=3 bytes=576 guard_bytes=8"
SURFACE_VIEW = "CYBERMANJU_FRAMEBUFFER:surface_view:PASS cases=4 bytes=24 guard_bytes=8"
HARDWARE = (BOOT, IO, BOUNDS, CONSOLE, SURFACE_VIEW)


def log(*evidence):
    return "\n".join((*evidence, SUMMARY))


class RuntimeVerifierTests(unittest.TestCase):
    def test_contract_and_ansi_cr(self):
        result = verify(log("\x1b[32m" + CONTRACT + "\x1b[0m").replace("\n", "\r\n"), "contract")
        self.assertEqual(result["kind"], "contract")
        for key in ("hardware_acceleration_verified", "gpui_native_verified", "physical_scanout_verified"):
            self.assertFalse(result[key])
        self.assertNotIn("surface_bounds", result)
        self.assertNotIn("console_flush", result)

    def test_valid_hardware_exports_measurements(self):
        result = verify(log(*HARDWARE), "hardware")
        self.assertEqual(result["framebuffer"], {"width": 1024, "height": 768, "bpp": 32, "pitch": 4096, "bytes": 3145728, "rows": 2, "row_bytes": 8})
        self.assertEqual(result["surface_bounds"], {"cases": 6, "bytes": 24, "synthetic_subsurface": True})
        self.assertEqual(result["console_flush"], {"cases": 3, "bytes": 576, "guard_bytes": 8, "synthetic_subsurface": True})
        self.assertEqual((result["surface_view_cases"], result["surface_view_bytes"], result["surface_view_guard_bytes"]), (4, 24, 8))
        self.assertTrue(result["surface_view_verified"])
        for key in ("fbdev_userspace_verified", "mmap_tail_isolated"):
            self.assertFalse(result[key])
        self.assertTrue(all(v is False for k, v in result.items() if k.endswith("_verified") and k != "surface_view_verified"))

    def test_surface_view_marker_required_exact_and_not_echoed(self):
        invalid = (
            tuple(m for m in HARDWARE if m != SURFACE_VIEW),
            (*HARDWARE, SURFACE_VIEW),
            (*HARDWARE[:-1], "$ echo " + SURFACE_VIEW),
            (*HARDWARE[:-1], SURFACE_VIEW.replace("cases=4", "cases=3")),
            (*HARDWARE[:-1], SURFACE_VIEW.replace("bytes=24", "bytes=25")),
            (*HARDWARE[:-1], SURFACE_VIEW.replace("guard_bytes=8", "guard_bytes=0")),
            (*HARDWARE[:-1], "prefix " + SURFACE_VIEW),
        )
        for evidence in invalid:
            with self.subTest(evidence=evidence), self.assertRaises(ValueError):
                verify(log(*evidence), "hardware")

    def test_missing_contradictory_duplicate_and_unknown_markers(self):
        invalid = [
            ((), "contract"), ((CONTRACT, CONTRACT), "contract"),
            ((CONTRACT, *HARDWARE), "contract"), ((CONTRACT, BOUNDS), "contract"),
            ((CONTRACT, CONSOLE), "contract"), ((CONTRACT, SURFACE_VIEW), "contract"),
            ((*HARDWARE, CONTRACT), "hardware"),
            ((*HARDWARE, "CYBERMANJU_FRAMEBUFFER:other:PASS"), "hardware"),
        ]
        for marker in HARDWARE:
            invalid.append((tuple(m for m in HARDWARE if m != marker), "hardware"))
            invalid.append(((*HARDWARE, marker), "hardware"))
        for evidence, kind in invalid:
            with self.subTest(evidence=evidence, kind=kind), self.assertRaises(ValueError):
                verify(log(*evidence), kind)

    def test_echo_partial_and_unrecognized_log_lines_rejected(self):
        for text in (log("$ echo " + CONTRACT), log("prefix " + CONTRACT), log(CONTRACT + " trailing")):
            with self.subTest(text=text), self.assertRaises(ValueError):
                verify(text, "contract")
        for marker in ("prefix " + BOUNDS, BOUNDS + " trailing", BOUNDS.replace("cases=6", "cases=5"), BOUNDS.replace("bytes=24", "bytes=25")):
            with self.subTest(marker=marker), self.assertRaises(ValueError):
                verify(log(BOOT, IO, marker, CONSOLE, SURFACE_VIEW), "hardware")
        with self.assertRaises(ValueError):
            verify(log(BOOT.replace("width=1024", "width=1024 extra"), IO, BOUNDS, CONSOLE, SURFACE_VIEW), "hardware")

    def test_console_marker_is_complete_and_exact(self):
        invalid = (
            "prefix " + CONSOLE, "$ echo " + CONSOLE, CONSOLE + " trailing",
            CONSOLE.replace("cases=3", "cases=2"),
            CONSOLE.replace("bytes=576", "bytes=4096"),
            CONSOLE.replace("guard_bytes=8", "guard_bytes=0"),
        )
        for marker in invalid:
            with self.subTest(marker=marker), self.assertRaises(ValueError):
                verify(log(BOOT, IO, BOUNDS, marker, SURFACE_VIEW), "hardware")

    def test_exactly_one_success_summary_and_failure_rejection(self):
        invalid = (SUMMARY, "test result: FAILED. 0 passed; 1 failed;", "runner failed: 0 failures", "FAILED assertion panic", "test result: ok. 1 passed; 1 failed; 0 filtered out.", "[caught panic] Panicked at source.rs:5", "Panicked at source.rs:5", "assertion failed: logical span")
        for failure in invalid:
            for kind, evidence in (("contract", (CONTRACT,)), ("hardware", HARDWARE)):
                with self.subTest(failure=failure, kind=kind), self.assertRaises(ValueError):
                    verify(log(*evidence, failure), kind)

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
                verify(log(boot, IO, BOUNDS, CONSOLE, SURFACE_VIEW), "hardware")
        with self.assertRaises(ValueError):
            verify(log(BOOT, IO.replace("row_bytes=8", "row_bytes=7"), BOUNDS, CONSOLE, SURFACE_VIEW), "hardware")


if __name__ == "__main__":
    unittest.main()
