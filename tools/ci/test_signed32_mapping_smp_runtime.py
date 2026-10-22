import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from verify_signed32_mapping_smp_runtime import BEGIN, COMPLETE, verify


class MappingSMPTests(unittest.TestCase):
    def evidence(self):
        return "\n".join([
            BEGIN, COMPLETE,
            "test result: ok. 1 passed; 0 failed; 0 filtered out.", "",
        ])

    def test_positive_scope_and_false_production_invariants(self):
        result = verify(self.evidence())
        self.assertEqual(
            (result["scope"], result["runtime_tests"], result["failed"], result["cpus"],
             result["denied_cases"], result["buffers_preserved"],
             result["raw_compared_bytes"], result["reclaimed_ranges"]),
            ("signed32_mapping_smp_rejection_only", 1, 0, 2, 3, 3, 40960, 1),
        )
        self.assertTrue(result["smp_rejection_verified"])
        for key in ("pages_mapped", "production_mapping_available", "smp_mapping_verified",
                    "module_execution_verified", "linux_modules_executed", "wx_verified"):
            self.assertFalse(result[key])

    def test_two_markers_unique_unprefixed_and_ordered(self):
        good = self.evidence()
        for marker in (BEGIN, COMPLETE):
            for bad in (good.replace(marker + "\n", ""), good + marker + "\n",
                        good.replace(marker, "echo " + marker),
                        good.replace(marker, "prefix " + marker)):
                with self.subTest(marker=marker[:50]), self.assertRaises(ValueError):
                    verify(bad)
        with self.assertRaises(ValueError):
            verify(good.replace(BEGIN + "\n" + COMPLETE, COMPLETE + "\n" + BEGIN))

    def test_claim_counts_summary_panic_and_other_prefix_are_strict(self):
        good = self.evidence()
        for old, new in (("cpus=2", "cpus=3"), ("denied=3", "denied=2"),
                         ("buffers_preserved=3", "buffers_preserved=2"),
                         ("raw_compared=40960", "raw_compared=40959"),
                         ("reclaimed=1", "reclaimed=0"),
                         ("pages_mapped=false", "pages_mapped=true"),
                         ("production=false", "production=true"),
                         ("execution=false", "execution=true")):
            with self.subTest(claim=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))
        for bad in (good.replace("1 passed", "1 failed"), good + "kernel panicked\n",
                    good + "CYBERMANJU_OTHER:thing:PASS\n",
                    good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n"):
            with self.assertRaises(ValueError):
                verify(bad)

    def test_cli_stale_result_invalidation(self):
        script = Path(__file__).with_name("verify_signed32_mapping_smp_runtime.py")
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "runtime.log"
            log.write_text(self.evidence())
            proc = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual(json.loads(log.with_name("result.json").read_text())["scope"],
                             "signed32_mapping_smp_rejection_only")
            log.write_text("panic\n")
            proc = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0)
            self.assertEqual(json.loads(log.with_name("result.json").read_text())["status"],
                             "not_verified")


if __name__ == "__main__":
    unittest.main()
