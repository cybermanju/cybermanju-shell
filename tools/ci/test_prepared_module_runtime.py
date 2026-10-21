import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_prepared_module_runtime import SCENARIOS, verify


class PreparedRuntimeEvidenceTests(unittest.TestCase):
    def evidence(self):
        return (
            "\n".join(
                [
                    "CYBERMANJU_PREPARED_RUNTIME:roundtrip:PASS sections=13 span=40960 payload=1721 relocations=6 pages=10",
                    "CYBERMANJU_PREPARED_RUNTIME:rejected_budgets_and_mismatched_layout:PASS",
                    "CYBERMANJU_PREPARED_RUNTIME:reserved_layout:PASS base=0xffffc00000000000 reserved=40960 span=40960 sections=13 payload=1721 relocations=6 pages=10",
                    "CYBERMANJU_PREPARED_RUNTIME:no_init_or_exit_called:PASS",
                    "test result: ok. 1 passed; 0 failed; 301 filtered out.",
                ]
            )
            + "\n"
        )

    def test_complete_evidence_and_result_contract(self):
        result = verify(self.evidence())
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["runtime_tests"], 1)
        self.assertEqual(result["scope"], "prepared_module_reserved_base_staging_only")
        self.assertTrue(result["fixture_staged"])
        self.assertFalse(result["linux_modules_executed"])
        self.assertFalse(result["init_called"])
        self.assertFalse(result["pages_mapped"])
        self.assertFalse(result["wx_verified"])
        self.assertTrue(result["reservation_held_during_staging"])
        self.assertEqual(result["reserved_layout"]["base"], "0xffffc00000000000")
        self.assertNotIn("linux_modules_loaded", result)
        for field in ("sections", "span", "payload", "relocations", "pages"):
            self.assertIsInstance(result[field], int)

    def test_ansi_and_c0_prefixes_are_accepted(self):
        decorated = "\x03\x1b[32m" + self.evidence().replace(
            "ok.", "\x1b[32mok\x1b[0m."
        )
        self.assertEqual(verify(decorated)["status"], "passed")

    def test_each_missing_marker_is_rejected(self):
        for scenario in SCENARIOS:
            marker = f"CYBERMANJU_PREPARED_RUNTIME:{scenario}:PASS"
            with self.subTest(scenario=scenario), self.assertRaises(ValueError):
                verify(self.evidence().replace(marker, "", 1))

    def test_duplicate_markers_are_rejected(self):
        for scenario in SCENARIOS:
            marker = f"CYBERMANJU_PREPARED_RUNTIME:{scenario}:PASS"
            with self.subTest(scenario=scenario), self.assertRaises(ValueError):
                verify(self.evidence() + marker + "\n")

    def test_contradictory_unknown_and_malformed_markers_are_rejected(self):
        examples = (
            self.evidence() + "CYBERMANJU_PREPARED_RUNTIME:roundtrip:FAIL\n",
            self.evidence() + "CYBERMANJU_PREPARED_RUNTIME:unknown:PASS\n",
            self.evidence().replace("no_init_or_exit_called:PASS", "no_init_or_exit_called:PASS_EXTRA"),
            self.evidence().replace("pages=10", "pages=10 unexpected"),
        )
        for text in examples:
            with self.subTest(text=text), self.assertRaises(ValueError):
                verify(text)

    def test_reserved_range_budget_and_fixture_consistency(self):
        text = self.evidence()
        marker = next(line for line in text.splitlines() if "reserved_layout:" in line)
        changes = [
            ("base=0xffffc00000000000", "base=0x10000000"),
            ("base=0xffffc00000000000", "base=0x10000000000000000"),
            ("base=0xffffc00000000000", "base=0xffffffffffff0000"),
            ("base=0xffffc00000000000", "base=0xffffc00000000001"),
            ("reserved=40960", "reserved=40959"),
            ("reserved=40960", "reserved=36864"),
            ("reserved=40960", f"reserved={4*1024*1024+4096}"),
            ("sections=13", "sections=12"),
            ("payload=1721", "payload=1720"),
            ("relocations=6", "relocations=5"),
        ]
        for old, new in changes:
            with self.subTest(new=new), self.assertRaises(ValueError):
                verify(text.replace(marker, marker.replace(old, new)))

    def test_echoed_markers_mixed_logs_and_panic_cannot_qualify(self):
        text = self.evidence()
        marker = text.splitlines()[0]
        cases = [text.replace(marker, "echo " + marker),
                 text + "CYBERMANJU_MODULE_ADDRESS:validation:PASS cases=6\n",
                 text + "[caught panic] Panicked at module.rs:42\n",
                 text + "assertion failed: prepared bytes\n",
                 text + "prefix test result: ok. 1 passed; 0 failed; 0 filtered out.\n"]
        for bad in cases:
            with self.subTest(bad=bad[-100:]), self.assertRaises(ValueError):
                verify(bad)

    def test_multiple_runner_summaries_are_rejected(self):
        for summary in (
            "test result: ok. 1 passed; 0 failed; 0 filtered out.\n",
            "test result: ok. 0 passed; 0 failed; 999 filtered out.\n",
        ):
            with self.subTest(summary=summary), self.assertRaises(ValueError):
                verify(self.evidence() + summary)

    def test_zero_passes_and_runner_failures_are_rejected(self):
        bad = (
            self.evidence().replace("1 passed", "0 passed"),
            self.evidence().replace("test result: ok.", "test result: FAILED."),
            self.evidence() + "test result: FAILED. 1 passed; 1 failed; 0 filtered out.\n",
            self.evidence().replace("0 failed", "2 failed"),
        )
        for text in bad:
            with self.subTest(text=text), self.assertRaises(ValueError):
                verify(text)

    def test_invalid_or_budget_exceeding_counts_are_rejected(self):
        examples = (
            "sections=0 span=4096 payload=1 relocations=1 pages=1",
            "sections=4097 span=4096 payload=1 relocations=1 pages=1",
            "sections=1 span=4096 payload=1 relocations=1 pages=0",
            "sections=1 span=4096 payload=0 relocations=1 pages=1",
            "sections=1 span=4096 payload=4097 relocations=1 pages=1",
            "sections=1 span=4096 payload=1 relocations=0 pages=1",
            "sections=1 span=4096 payload=1 relocations=65537 pages=1",
            "sections=-1 span=4096 payload=1 relocations=1 pages=1",
            "sections=1 span=4097 payload=1 relocations=1 pages=1",
            "sections=1 span=4096 payload=nope relocations=1 pages=1",
        )
        for counts in examples:
            line = "CYBERMANJU_PREPARED_RUNTIME:roundtrip:PASS " + counts
            text = self.evidence().replace(
                "CYBERMANJU_PREPARED_RUNTIME:roundtrip:PASS sections=13 span=40960 payload=1721 relocations=6 pages=10",
                line,
            )
            with self.subTest(counts=counts), self.assertRaises(ValueError):
                verify(text)

    def test_cli_writes_result_json_only_after_success(self):
        script = Path(__file__).with_name("verify_prepared_module_runtime.py")
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "runtime.log"
            result_path = log.with_name("result.json")
            log.write_text(self.evidence())
            passed = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertEqual(passed.returncode, 0, passed.stderr)
            self.assertEqual(json.loads(result_path.read_text())["status"], "passed")
            log.write_text("incomplete output\n")
            failed = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertNotEqual(failed.returncode, 0)
            self.assertEqual(json.loads(result_path.read_text())["status"], "passed")


if __name__ == "__main__":
    unittest.main()
