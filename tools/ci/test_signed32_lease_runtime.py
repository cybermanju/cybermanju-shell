import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from verify_signed32_lease_runtime import BEGIN, PERSISTENT, FAULTS, VALIDATION, REMAP, COMPLETE, verify


class Signed32LeaseRuntimeTests(unittest.TestCase):
    def evidence(self):
        return "\n".join([BEGIN, PERSISTENT, FAULTS, VALIDATION, REMAP, COMPLETE,
                          "test result: ok. 1 passed; 0 failed; 3 filtered out.", ""])

    def test_metrics_and_capability_limits(self):
        result = verify(self.evidence())
        expected = {
            "status": "passed", "scope": "experimental_signed32_rw_nonx_persistent_lease_single_cpu",
            "cpus": 1, "runtime_tests": 1, "span_bytes": 8192, "pages": 2,
            "envelope_pages": 3, "read_bytes": 16404, "pte_pages": 2,
            "frame_matches": 2, "rw_nonx_pages": 2, "supervisor_pages": 2,
            "writeback_pages": 2, "global_pages": 2, "slack_absent": 1,
            "persistent_mappings": 2, "read_bounds_rejections": 3,
            "invalid_join_rejections": 2, "collision_rejections": 1,
            "foreign_mapping_preserved": True, "fault_cases": 3,
            "installed_fault_prefix_pages": 3, "remapped_frame_changes": 2,
            "reclaimed_reservations": 8, "irq_state_preserved": True,
            "experimental_persistent_mapping_available": True,
            "linux_modules_executed": False, "linux_abi_verified": False,
            "physical_wx_verified": False, "smp_mapping_verified": False,
            "tlb_shootdown_verified": False, "production_mapping_ready": False,
        }
        self.assertEqual(result, expected)

    def test_exact_markers_mutations_summary_and_normalization(self):
        good = self.evidence()
        for marker in (BEGIN, PERSISTENT, FAULTS, VALIDATION, REMAP, COMPLETE):
            for bad in (good.replace(marker + "\n", ""), good + marker + "\n",
                        good.replace(marker, "echo " + marker), good.replace(marker, "x " + marker)):
                with self.subTest(marker=marker[:55]), self.assertRaises(ValueError): verify(bad)
        for old, new in (("cpus=1", "cpus=2"), ("span=8192", "span=8191"),
                         ("pages=2", "pages=3"), ("envelope_pages=3", "envelope_pages=2"),
                         ("bytes=8212", "bytes=8211"), ("reads=3", "reads=2"),
                         ("rw_nonx=2", "rw_nonx=1"), ("bounds=3", "bounds=2"),
                         ("invalid_joins=2", "invalid_joins=1"), ("collision=1", "collision=0"),
                         ("frame_changes=2", "frame_changes=1"), ("reclaims=8", "reclaims=7"),
                         ("physical_wx=false", "physical_wx=true"),
                         ("smp_mapping=false", "smp_mapping=true")):
            with self.subTest(claim=old), self.assertRaises(ValueError): verify(good.replace(old, new))
        decorated = "\x1b[32m" + good.replace("\n", "\r\n") + "\x1b[0m"
        self.assertEqual(verify(decorated)["status"], "passed")
        for bad in (good.replace("1 passed", "1 failed"), good + "kernel panicked\n",
                    good + "assertion failed\n", good + "CYBERMANJU_OTHER:x\n",
                    good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n",
                    good.replace(COMPLETE + "\n", "test result: ok. 1 passed; 0 failed; 0 filtered out.\n" + COMPLETE + "\n")):
            with self.subTest(bad=bad[-50:]), self.assertRaises(ValueError): verify(bad)

    def test_cli_writes_success_and_invalidates_stale_result_missing_log(self):
        script = Path(__file__).with_name("verify_signed32_lease_runtime.py")
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "runtime.log"
            output = log.with_name("result.json")
            log.write_text(self.evidence())
            proc = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual(json.loads(output.read_text())["scope"], "experimental_signed32_rw_nonx_persistent_lease_single_cpu")
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
