import unittest
from verify_framebuffer_runtime import verify

SUMMARY = "test result: ok. 1 passed; 0 failed; 0 filtered out."
CONTRACT = "CYBERMANJU_FRAMEBUFFER_CONTRACT:PASS"
BOOT = "CYBERMANJU_FRAMEBUFFER:boot:PASS width=1024 height=768 bpp=32 pitch=4096 bytes=3145728"
IO = "CYBERMANJU_FRAMEBUFFER:io_readback:PASS rows=2 row_bytes=8"

def log(*evidence):
    return "\n".join((*evidence, SUMMARY))

class RuntimeVerifierTests(unittest.TestCase):
    def test_contract_and_ansi_cr(self):
        result = verify(log("\x1b[32m" + CONTRACT + "\x1b[0m").replace("\n", "\r\n"), "contract")
        self.assertEqual(result["kind"], "contract")
        for key in ("hardware_acceleration_verified", "gpui_native_verified", "physical_scanout_verified"):
            self.assertFalse(result[key])

    def test_valid_hardware_exports_measurements(self):
        result = verify(log(BOOT, IO), "hardware")
        self.assertEqual(result["framebuffer"], {"width":1024,"height":768,"bpp":32,"pitch":4096,"bytes":3145728,"rows":2,"row_bytes":8})
        self.assertTrue(all(v is False for k,v in result.items() if k.endswith("_verified")))

    def test_missing_contradictory_duplicate_and_unknown_markers(self):
        for evidence, kind in (((), "contract"), ((CONTRACT, CONTRACT), "contract"), ((CONTRACT, BOOT, IO), "contract"), ((BOOT,), "hardware"), ((BOOT, IO, CONTRACT), "hardware"), ((BOOT, BOOT, IO), "hardware"), ((BOOT, IO, "CYBERMANJU_FRAMEBUFFER:other:PASS"), "hardware")):
            with self.subTest(evidence=evidence):
                with self.assertRaises(ValueError): verify(log(*evidence), kind)

    def test_echo_partial_and_unrecognized_log_lines_rejected(self):
        for text in (log("$ echo " + CONTRACT), log("prefix " + CONTRACT), log(CONTRACT + " trailing"), log(BOOT.replace("width=1024", "width=1024 extra"), IO)):
            with self.subTest(text=text):
                with self.assertRaises(ValueError): verify(text, "contract" if CONTRACT in text else "hardware")

    def test_exactly_one_success_summary_and_failure_rejection(self):
        for text in (log(CONTRACT, SUMMARY), log(CONTRACT, "test result: FAILED. 0 passed; 1 failed;"), log(CONTRACT, "runner failed: 0 failures"), log(CONTRACT, "FAILED assertion panic"), log(CONTRACT, "test result: ok. 1 passed; 1 failed; 0 filtered out.")):
            with self.subTest(text=text):
                with self.assertRaises(ValueError): verify(text, "contract")

    def test_overflow_zero_geometry_rowbytes_and_pitch_mismatch(self):
        invalid = (
            BOOT.replace("width=1024", "width=0"),
            BOOT.replace("width=1024", f"width={2**64}"),
            BOOT.replace("height=768", f"height={2**64}"),
            BOOT.replace("bytes=3145728", f"bytes={2**64}"),
            BOOT.replace("pitch=4096", "pitch=4095"),
            BOOT.replace("bytes=3145728", "bytes=3145727"),
            IO.replace("row_bytes=8", "row_bytes=7"),
        )
        for bad in invalid:
            with self.subTest(bad=bad):
                with self.assertRaises(ValueError): verify(log(bad, IO), "hardware")

if __name__ == "__main__":
    unittest.main()
