import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from verify_signed32_mapping_fault_runtime import (
    BEGIN,
    COMPLETE,
    ROLLBACK,
    verify,
)


class MappingFaultRuntimeTests(unittest.TestCase):
    def evidence(self):
        return "\n".join(
            [
                BEGIN,
                ROLLBACK,
                COMPLETE,
                "test result: ok. 1 passed; 0 failed; 2 filtered out.",
                "",
            ]
        )

    def test_scope_metrics_and_capability_limits(self):
        result = verify(self.evidence())
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["scope"], "signed32_mapping_fault_injection_only")
        self.assertEqual(
            (
                result["runtime_tests"],
                result["failed"],
                result["cpus"],
                result["mapping_failure_cases"],
                result["installed_prefix_pages"],
                result["buffers_preserved"],
                result["foreign_rejected_cases"],
                result["recovery_compared_bytes"],
                result["reclaimed_ranges"],
            ),
            (1, 0, 1, 3, 3, 3, 1, 12288, 1),
        )
        for key in (
            "foreign_pte_preserved",
            "slack_unmapped_verified",
            "mapping_absent_after_scopes",
            "temporary_mapping_verified",
        ):
            self.assertTrue(result[key])
        for key in (
            "production_mapping_available",
            "linux_abi_verified",
            "wx_verified",
            "smp_mapping_verified",
            "linux_modules_executed",
            "module_execution_verified",
            "init_called",
        ):
            self.assertFalse(result[key])

    def test_markers_are_exact_unique_unprefixed_and_ordered(self):
        good = self.evidence()
        markers = (BEGIN, ROLLBACK, COMPLETE)
        for marker in markers:
            variants = (
                good.replace(marker + "\n", ""),
                good + marker + "\n",
                good.replace(marker, "echo " + marker),
                good.replace(marker, "prefix " + marker),
            )
            for bad in variants:
                with self.subTest(marker=marker[:60]), self.assertRaises(ValueError):
                    verify(bad)
        reordered = good.replace(
            BEGIN + "\n" + ROLLBACK,
            ROLLBACK + "\n" + BEGIN,
        )
        with self.assertRaises(ValueError):
            verify(reordered)
        with self.assertRaises(ValueError):
            verify(good + "CYBERMANJU_OTHER:marker:PASS\n")

    def test_mutated_claims_and_false_capabilities_are_rejected(self):
        good = self.evidence()
        mutations = (
            ("cpus=1", "cpus=2"),
            ("pages=3", "pages=4"),
            ("envelope_pages=4", "envelope_pages=3"),
            ("failures=3", "failures=2"),
            ("installed_prefix_pages=3", "installed_prefix_pages=2"),
            ("buffers_preserved=3", "buffers_preserved=2"),
            ("foreign_rejected=1", "foreign_rejected=0"),
            ("foreign_preserved=true", "foreign_preserved=false"),
            ("slack_unmapped=true", "slack_unmapped=false"),
            ("recovered_bytes=12288", "recovered_bytes=12287"),
            ("reclaimed=1", "reclaimed=0"),
            ("all_unmapped=true", "all_unmapped=false"),
            ("production=false", "production=true"),
            ("execution=false", "execution=true"),
        )
        for old, new in mutations:
            with self.subTest(claim=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))

    def test_summary_failure_and_ansi_cr_handling(self):
        good = self.evidence()
        decorated = "\x1b[32m" + good.replace("\n", "\r\n") + "\x1b[0m"
        self.assertEqual(verify(decorated)["status"], "passed")
        for bad in (
            good.replace("1 passed", "1 failed"),
            good + "kernel panicked\n",
            good + "assertion failed\n",
            good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n",
            good.replace("test result:", "prefix test result:"),
            good.replace(COMPLETE + "\n", "test result: ok. 1 passed; 0 failed; 0 filtered out.\n" + COMPLETE + "\n"),
        ):
            with self.subTest(transcript=bad[-60:]), self.assertRaises(ValueError):
                verify(bad)

    def test_cli_invalidates_stale_success_before_failed_verification(self):
        script = Path(__file__).with_name(
            "verify_signed32_mapping_fault_runtime.py"
        )
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "runtime.log"
            output = log.with_name("result.json")
            log.write_text(self.evidence())
            proc = subprocess.run(
                [sys.executable, str(script), str(log)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual(
                json.loads(output.read_text())["scope"],
                "signed32_mapping_fault_injection_only",
            )

            log.write_text("incomplete evidence\n")
            proc = subprocess.run(
                [sys.executable, str(script), str(log)],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(proc.returncode, 0)
            self.assertEqual(json.loads(output.read_text())["status"], "not_verified")

            log.unlink()
            proc = subprocess.run(
                [sys.executable, str(script), str(log)],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(proc.returncode, 0)
            self.assertEqual(json.loads(output.read_text())["status"], "not_verified")


if __name__ == "__main__":
    unittest.main()
