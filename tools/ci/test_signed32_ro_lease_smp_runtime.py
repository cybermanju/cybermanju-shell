import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from verify_signed32_ro_lease_smp_runtime import BEGIN, DENIED, COMPLETE, verify


class Signed32ROLeaseSMPTests(unittest.TestCase):
    def evidence(self):
        return "\n".join([BEGIN, DENIED, COMPLETE,
                          "test result: ok. 1 passed; 0 failed; 0 filtered out.", ""])

    def test_rejection_only_metrics_and_limits(self):
        result = verify(self.evidence())
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["architecture"], "x86_64")
        self.assertEqual(result["page_size"], 4096)
        self.assertEqual(result["scope"], "smp_rejection_only")
        self.assertEqual(result["boot_cpus"], 2)
        self.assertEqual((result["denied_attempts"], result["reclaimed_reservations"]), (2, 2))
        self.assertEqual((result["handles_acquired"], result["handles_dropped"], result["live_handles"]), (4, 4, 0))
        for field in ("ro_mapping", "module_execution", "physical_wx", "smp_mapping",
                      "write_fault_probed", "immutable_data", "production_qualified"):
            self.assertFalse(result[field], field)
        self.assertTrue(result["normal_build_api_exists"])

    def test_exact_markers_claims_summary_and_normalization(self):
        good = self.evidence()
        for marker in (BEGIN, DENIED, COMPLETE):
            for bad in (good.replace(marker + "\n", ""), good + marker + "\n",
                        good.replace(marker, "echo " + marker), good.replace(marker, "prefix " + marker)):
                with self.subTest(marker=marker[:48]), self.assertRaises(ValueError):
                    verify(bad)
        for old, new in (("cpus=2", "cpus=1"), ("attempts=2", "attempts=1"),
                         ("reclaims=2", "reclaims=1"), ("acquired=4", "acquired=3"),
                         ("dropped=4", "dropped=3"), ("live=0", "live=1"),
                         ("irq_preserved=true", "irq_preserved=false"),
                         ("ro_mapping=false", "ro_mapping=true"),
                         ("module_execution=false", "module_execution=true"),
                         ("physical_wx=false", "physical_wx=true"),
                         ("smp_mapping=false", "smp_mapping=true"),
                         ("write_fault_probed=false", "write_fault_probed=true")):
            with self.subTest(claim=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))
        self.assertEqual(verify("\x1b[32m" + good.replace("\n", "\r\n") + "\x1b[0m")["status"], "passed")
        for bad in (good.replace("1 passed", "1 failed"), good + "kernel panicked\n",
                    good + "assertion failed\n", good + "CYBERMANJU_OTHER:x\n",
                    good + "test result: ok. 1 passed; 0 failed; 2 filtered out.\n",
                    good.replace(COMPLETE + "\n", "test result: ok. 1 passed; 0 failed; 0 filtered out.\n" + COMPLETE + "\n")):
            with self.subTest(bad=bad[-50:]), self.assertRaises(ValueError):
                verify(bad)

    def test_cli_success_and_invalidates_stale_result_on_bad_or_missing_log(self):
        script = Path(__file__).with_name("verify_signed32_ro_lease_smp_runtime.py")
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "runtime.log"
            output = log.with_name("result.json")
            log.write_text(self.evidence())
            proc = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual(json.loads(output.read_text())["scope"], "smp_rejection_only")
            log.write_text("incomplete\n")
            proc = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0)
            self.assertEqual(json.loads(output.read_text())["status"], "not_verified")
            log.unlink()
            proc = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0)
            self.assertEqual(json.loads(output.read_text())["status"], "not_verified")


if __name__ == "__main__":
    unittest.main()
