import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from verify_signed32_lease_smp_runtime import BEGIN, DENIED, COMPLETE, verify


class Signed32LeaseSMPTests(unittest.TestCase):
    def evidence(self):
        return "\n".join([BEGIN, DENIED, COMPLETE,
                          "test result: ok. 1 passed; 0 failed; 0 filtered out.", ""])

    def test_denial_only_metrics_and_limits(self):
        self.assertEqual(verify(self.evidence()), {
            "status": "passed", "scope": "experimental_signed32_persistent_lease_smp_denial_only",
            "cpus": 2, "runtime_tests": 1, "denied_attempts": 2,
            "reclaimed_reservations": 2, "irq_state_preserved": True,
            "persistent_mapping_available": False, "linux_modules_executed": False,
            "linux_abi_verified": False, "physical_wx_verified": False,
            "smp_mapping_verified": False, "tlb_shootdown_verified": False,
            "production_mapping_ready": False,
        })

    def test_markers_claims_summary_and_normalization_are_strict(self):
        good = self.evidence()
        for marker in (BEGIN, DENIED, COMPLETE):
            for bad in (good.replace(marker + "\n", ""), good + marker + "\n",
                        good.replace(marker, "echo " + marker), good.replace(marker, "prefix " + marker)):
                with self.subTest(marker=marker[:50]), self.assertRaises(ValueError): verify(bad)
        for old, new in (("cpus=2", "cpus=1"), ("attempts=2", "attempts=1"),
                         ("reclaims=2", "reclaims=1"), ("irq_preserved=true", "irq_preserved=false"),
                         ("persistent_mapping=false", "persistent_mapping=true"),
                         ("module_execution=false", "module_execution=true"),
                         ("physical_wx=false", "physical_wx=true"), ("smp_mapping=false", "smp_mapping=true")):
            with self.subTest(claim=old), self.assertRaises(ValueError): verify(good.replace(old, new))
        self.assertEqual(verify("\x1b[32m" + good.replace("\n", "\r\n") + "\x1b[0m")["status"], "passed")
        for bad in (good.replace("1 passed", "1 failed"), good + "panic\n",
                    good + "assertion failed\n", good + "CYBERMANJU_OTHER:x\n",
                    good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n",
                    good.replace(COMPLETE + "\n", "test result: ok. 1 passed; 0 failed; 0 filtered out.\n" + COMPLETE + "\n")):
            with self.subTest(bad=bad[-50:]), self.assertRaises(ValueError): verify(bad)

    def test_cli_stale_success_invalidated_on_invalid_or_missing_input(self):
        script = Path(__file__).with_name("verify_signed32_lease_smp_runtime.py")
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "runtime.log"
            output = log.with_name("result.json")
            log.write_text(self.evidence())
            proc = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual(json.loads(output.read_text())["scope"], "experimental_signed32_persistent_lease_smp_denial_only")
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
