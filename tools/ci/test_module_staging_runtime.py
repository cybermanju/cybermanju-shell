import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from verify_module_staging_runtime import TESTS, verify
from verify_module_address_runtime import verify as verify_address


class RuntimeEvidenceTests(unittest.TestCase):
    def evidence(self):
        return "\n".join(f"CYBERMANJU_STAGING_RUNTIME:{name}:PASS" for name in TESTS) + "\ntest result: ok. 1 passed; 0 failed; 301 filtered out.\n"

    def test_complete_runtime_evidence(self):
        result = verify(self.evidence())
        self.assertEqual(result["runtime_tests"], 1)
        self.assertFalse(result["linux_modules_loaded"])

    def test_serial_ansi_and_control_prefixes(self):
        result = verify("\x03\x1b[32m" + self.evidence().replace("ok.", "\x1b[32mok\x1b[0m."))
        self.assertEqual(result["status"], "passed")

    def test_every_missing_marker_is_rejected(self):
        for name in TESTS:
            with self.subTest(name=name), self.assertRaises(ValueError):
                verify(self.evidence().replace(f"CYBERMANJU_STAGING_RUNTIME:{name}:PASS", ""))

    def test_duplicate_marker_is_rejected(self):
        with self.assertRaises(ValueError):
            verify(self.evidence() + f"CYBERMANJU_STAGING_RUNTIME:{TESTS[0]}:PASS")

    def test_prefixed_marker_is_rejected(self):
        with self.assertRaises(ValueError):
            verify(self.evidence().replace(
                f"CYBERMANJU_STAGING_RUNTIME:{TESTS[-1]}:PASS",
                f"prefix CYBERMANJU_STAGING_RUNTIME:{TESTS[-1]}:PASS",
            ))

    def test_compilation_or_empty_filter_is_not_runtime_success(self):
        for text in ["Finished dev profile", self.evidence().replace("1 passed", "0 passed"), self.evidence().replace("test result: ok.", "test result: FAILED.")]:
            with self.subTest(text=text), self.assertRaises(ValueError):
                verify(text)

    def test_other_runner_failures_are_rejected(self):
        with self.assertRaises(ValueError):
            verify(self.evidence() + "test result: FAILED. 1 passed; 1 failed; 0 filtered out.")

    def test_extra_mixed_or_contradictory_markers_and_panic_rejected(self):
        for extra in ["CYBERMANJU_STAGING_RUNTIME:unknown:PASS\n",
                      "CYBERMANJU_STAGING_RUNTIME:reserved_owner_lifecycle:FAIL\n",
                      "CYBERMANJU_MODULE_ADDRESS:validation:PASS cases=6\n",
                      "panic: frame ownership lost\n", "assertion failed: reservation overlap\n"]:
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                verify(self.evidence() + extra)

    def test_only_one_standalone_runner_summary_is_accepted(self):
        valid = self.evidence()
        summary = valid.splitlines()[-1]
        for bad in [valid + summary + "\n", valid.replace(summary, "echo " + summary)]:
            with self.subTest(bad=bad[-100:]), self.assertRaises(ValueError):
                verify(bad)

    def test_existing_config_is_preserved_and_stale_success_invalidated(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "tools/ci"
            scripts.mkdir(parents=True)
            script = scripts / "test_module_staging_runtime.sh"
            shutil.copyfile(Path(__file__).with_name(script.name), script)
            config = root / "asterinas/ostd/OSDK.toml"
            config.parent.mkdir(parents=True)
            config.write_text("user configuration must survive\n")
            out = root / "artifacts/runtime"
            out.mkdir(parents=True)
            (out / "result.json").write_text('{"status":"passed"}')
            result = subprocess.run(["bash", str(script), str(out)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(config.read_text(), "user configuration must survive\n")
            self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")
            self.assertFalse((out / "osdk-build.log").exists())


class AddressReservationTests(unittest.TestCase):
    def setUp(self):
        self.pool = 0xffffc00000000000
        self.first = (self.pool + 0x1000, self.pool + 0x3000)
        self.second = (self.pool + 0x4000, self.pool + 0x7000)
        self.existing = (self.pool + 0x8000, self.pool + 0x9000)

    def evidence(self, first=None, second=None, existing=None, pool=None, release=None):
        first, second, existing = first or self.first, second or self.second, existing or self.existing
        pool = pool or (self.pool, self.pool + 0x10000)
        release = release or first
        return "\n".join([
            "CYBERMANJU_MODULE_ADDRESS:validation:PASS cases=6",
            "CYBERMANJU_MODULE_ADDRESS:reserve:PASS " + " ".join(
                f"{key}=0x{value:x}" for key, value in zip(
                    ["first_start", "first_end", "second_start", "second_end", "existing_start", "existing_end", "pool_start", "pool_end"],
                    [*first, *second, *existing, *pool])),
            "CYBERMANJU_MODULE_ADDRESS:unmapped:PASS pages=6",
            f"CYBERMANJU_MODULE_ADDRESS:release:PASS start=0x{release[0]:x} end=0x{release[1]:x}",
            "test result: ok. 1 passed; 0 failed; 7 filtered out.", ""])

    def test_complete_evidence_and_exact_scope(self):
        result = verify_address(self.evidence())
        self.assertEqual((result["runtime_tests"], result["scenarios"], result["failed"]), (1, 4, 0))
        self.assertFalse(result["pages_mapped"])
        self.assertEqual(result["scope"], "vmalloc_reservation_only")
        self.assertEqual(result["ranges"]["first"], {"start": hex(self.first[0]), "end": hex(self.first[1])})
        for value in result["ranges"].values():
            self.assertTrue(value["start"].startswith("0x") and value["end"].startswith("0x"))

    def test_serial_ansi_cr_and_hex_normalization(self):
        evidence = "\x1b[32m" + self.evidence().replace("\n", "\r\n") + "\x1b[0m"
        self.assertEqual(verify_address(evidence)["status"], "passed")

    def test_extra_malformed_echoed_mixed_markers_and_embedded_summary_rejected(self):
        valid = self.evidence()
        extras = [
            "echo CYBERMANJU_MODULE_ADDRESS:validation:PASS cases=6\n",
            "CYBERMANJU_MODULE_ADDRESS:validation:PASS cases=6 trailing\n",
            "CYBERMANJU_MODULE_ADDRESS:unknown:PASS\n",
            "CYBERMANJU_STAGING_RUNTIME:copy:PASS\n",
            "prefix test result: ok. 1 passed; 0 failed; 0 filtered out.\n",
            "assertion failed: reservation overlap\n",
        ]
        for extra in extras:
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                verify_address(valid + extra)

    def test_required_markers_are_complete_unique_and_no_echo_or_malformed(self):
        evidence = self.evidence()
        for marker in ["validation:PASS cases=6", "reserve:PASS", "unmapped:PASS pages=6", "release:PASS"]:
            line = next(line for line in evidence.splitlines() if marker in line)
            for altered in [evidence.replace(line, ""), evidence + line + "\n", evidence.replace(line, "echo " + line)]:
                with self.subTest(marker=marker, altered=altered[-100:]), self.assertRaises(ValueError):
                    verify_address(altered)
        with self.assertRaises(ValueError):
            verify_address(evidence.replace("cases=6", "cases=6 extra"))

    def test_summary_panic_failure_and_zero_tests_rejected(self):
        for bad in [self.evidence().replace("1 passed", "0 passed"), self.evidence()+"test result: ok. 1 passed; 0 failed; 0 filtered out.\n", self.evidence()+"kernel panic\n"]:
            with self.subTest(bad=bad[-80:]), self.assertRaises(ValueError):
                verify_address(bad)

    def test_alignment_sizes_pool_bounds_canonical_and_overflow(self):
        cases = [
            self.evidence(first=(self.first[0]+1, self.first[1]+1)),
            self.evidence(first=(self.first[0], self.first[0]+0x1000)),
            self.evidence(pool=(self.pool, 0x1000)),
            self.evidence(pool=(0xffff7ffffffff000, self.pool+0x10000)),
            self.evidence(pool=(self.pool, 0x10000000000000000)),
            self.evidence(pool=(self.pool+1, self.pool+0x10000)),
        ]
        for bad in cases:
            with self.subTest(bad=bad.splitlines()[1]), self.assertRaises(ValueError):
                verify_address(bad)

    def test_overlapping_ranges_or_nonmatching_release_rejected(self):
        overlap = self.evidence(second=(self.first[1]-0x1000, self.first[1]+0x2000))
        release = self.evidence(release=(self.first[0]+0x1000, self.first[1]+0x1000))
        for bad in (overlap, release):
            with self.assertRaises(ValueError):
                verify_address(bad)

    def test_address_mode_preflight_preserves_config_and_invalidates_result(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "tools/ci"
            scripts.mkdir(parents=True)
            script = scripts / "test_module_staging_runtime.sh"
            shutil.copyfile(Path(__file__).with_name(script.name), script)
            config = root / "asterinas/ostd/OSDK.toml"
            config.parent.mkdir(parents=True)
            config.write_text("keep me\n")
            out = root / "artifacts/address"
            out.mkdir(parents=True)
            (out / "result.json").write_text('{"status":"passed"}')
            result = subprocess.run(["bash", str(script), str(out), "--address-reservation"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(config.read_text(), "keep me\n")
            self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")
            self.assertFalse((out / "osdk-build.log").exists())


if __name__ == "__main__":
    unittest.main()
