"""Metatests for the explicit Linux C ABI runtime evidence verifier."""
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import verify_linux_c_abi_runtime as verifier


GOOD = "\n".join(verifier.MARKERS + [
    "test result: ok. 1 passed; 0 failed; 8 filtered out.",
]) + "\n"


class LinuxCAbiRuntimeTests(unittest.TestCase):
    def test_accepts_exact_evidence_and_reports_narrow_scope(self):
        result = verifier.verify(GOOD)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["scope"], "explicit_memcmp_c_abi_probe_only")
        self.assertEqual((result["runtime_tests"], result["failed"], result["cpus"]), (1, 0, 1))
        self.assertEqual((result["c_cases"], result["c_calls"], result["rust_calls"]), (16, 16, 16))
        self.assertTrue(result["buffers_preserved"])
        self.assertEqual((result["explicit_c_shim_names"], result["rust_internal_exports_denied"], result["unknown_names_denied"]), (1, 1, 3))
        for key in ("c_abi_subset_verified",):
            self.assertTrue(result[key])
        for key in ("elf_imports_verified", "linux_abi_verified", "production_linux_import_admission",
                    "production_mapping_available", "module_execution_verified", "linux_modules_executed",
                    "init_called", "wx_verified", "smp_mapping_verified"):
            self.assertFalse(result[key])

    def test_strips_ansi_and_carriage_returns(self):
        self.assertEqual(verifier.verify("\x1b[32m" + GOOD.replace("\n", "\r\n") + "\x1b[0m")["c_calls"], 16)

    def test_rejects_marker_mutation_duplicate_reorder_and_echo(self):
        variants = [
            GOOD.replace("cases=16", "cases=15"),
            GOOD.replace(verifier.MARKERS[1], verifier.MARKERS[1] + "\n" + verifier.MARKERS[1]),
            "\n".join([verifier.MARKERS[1], verifier.MARKERS[0], *verifier.MARKERS[2:]]) + "\ntest result: ok. 1 passed; 0 failed; 0 filtered out.\n",
            GOOD.replace(verifier.MARKERS[0], "echo " + verifier.MARKERS[0]),
        ]
        for sample in variants:
            with self.subTest(sample=sample[:90]), self.assertRaises(ValueError):
                verifier.verify(sample)

    def test_rejects_foreign_prefix_and_missing_or_duplicate_markers(self):
        variants = [
            GOOD.replace(verifier.MARKERS[2] + "\n", "CYBERMANJU_OTHER:PASS\n" + verifier.MARKERS[2] + "\n"),
            "\n".join(verifier.MARKERS[:-1]) + "\ntest result: ok. 1 passed; 0 failed; 0 filtered out.\n",
            GOOD.replace(verifier.MARKERS[0] + "\n", ""),
        ]
        for sample in variants:
            with self.subTest(sample=sample[:90]), self.assertRaises(ValueError):
                verifier.verify(sample)

    def test_rejects_failure_panic_assertion_and_mutation_flag(self):
        variants = [
            GOOD + "thread panicked at native runner\n",
            GOOD + "assertion check failed\n",
            GOOD + "test result: FAILED\n",
            GOOD + "mutations=true\n",
        ]
        for sample in variants:
            with self.subTest(sample=sample[-50:]), self.assertRaises(ValueError):
                verifier.verify(sample)

    def test_requires_one_exact_standalone_summary_after_complete(self):
        variants = [
            "\n".join(verifier.MARKERS) + "\ntest result: ok. 1 passed; 0 failed; 0 filtered out. extra\n",
            GOOD + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n",
            "test result: ok. 1 passed; 0 failed; 0 filtered out.\n" + GOOD,
        ]
        for sample in variants:
            with self.subTest(sample=sample[:110]), self.assertRaises(ValueError):
                verifier.verify(sample)

    def test_cli_invalidates_stale_result_before_missing_input_read(self):
        with tempfile.TemporaryDirectory() as directory:
            missing = Path(directory) / "absent.log"
            result_path = missing.with_name("result.json")
            result_path.write_text('{"status":"passed","stale":true}\n')
            proc = subprocess.run([sys.executable, str(Path(verifier.__file__)), str(missing)],
                                  capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0)
            state = json.loads(result_path.read_text())
            self.assertEqual(state, {"status": "not_verified", "linux_abi_verified": False})


if __name__ == "__main__":
    unittest.main()
