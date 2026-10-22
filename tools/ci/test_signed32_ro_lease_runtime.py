import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from verify_signed32_ro_lease_runtime import BEGIN, SEALED, ALIAS, FAULTS, VALIDATION, REMAP, COMPLETE, verify


class Signed32ROLeaseRuntimeTests(unittest.TestCase):
    def evidence(self):
        return "\n".join([BEGIN, SEALED, ALIAS, FAULTS, VALIDATION, REMAP, COMPLETE,
                          "test result: ok. 1 passed; 0 failed; 3 filtered out.", ""])

    def test_metrics_and_capability_limits(self):
        result = verify(self.evidence())
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["architecture"], "x86_64")
        self.assertEqual(result["page_size"], 4096)
        self.assertEqual(result["scope"], "single_cpu_read_only_nonexec_alias")
        self.assertEqual(result["boot_cpus"], 1)
        self.assertEqual(result["span_bytes"], result["sealed_bytes"])
        self.assertEqual(result["alias_bytes"], 32)
        self.assertEqual((result["pages"], result["envelope_pages"], result["pte_pages"]), (2, 3, 2))
        self.assertEqual((result["fault_reclaims"], result["remap_reclaims"], result["total_reclaims"]), (3, 2, 5))
        self.assertEqual((result["handles_acquired"], result["handles_dropped"], result["live_handles"]), (6, 6, 0))
        self.assertTrue(result["read_only_nonexec_pages"] > 0)
        self.assertEqual(result["rw_nonexec_pages"], 0)
        for field in ("immutable_data", "module_execution", "physical_wx", "smp_mapping", "write_fault_probed", "production_qualified"):
            self.assertFalse(result[field], field)
        self.assertTrue(result["normal_build_api_exists"])

    def test_exact_markers_mutations_summary_and_normalization(self):
        good = self.evidence()
        for marker in (BEGIN, SEALED, ALIAS, FAULTS, VALIDATION, REMAP, COMPLETE):
            for bad in (good.replace(marker + "\n", ""), good + marker + "\n",
                        good.replace(marker, "echo " + marker), good.replace(marker, "x " + marker)):
                with self.subTest(marker=marker[:48]), self.assertRaises(ValueError):
                    verify(bad)
        for old, new in (("cpus=1", "cpus=2"), ("span=8192", "span=8191"),
                         ("pages=2", "pages=3"), ("envelope_pages=3", "envelope_pages=2"),
                         ("before_rw=2", "before_rw=1"), ("pte_pages=2", "pte_pages=1"),
                         ("ro_nonx=2", "ro_nonx=1"), ("rw_nonx=0", "rw_nonx=1"),
                         ("bytes=32", "bytes=31"), ("immutable_data=false", "immutable_data=true"),
                         ("live=0", "live=1"), ("bounds=3", "bounds=2"),
                         ("output_preserved=true", "output_preserved=false"),
                         ("frame_changes=2", "frame_changes=1"), ("reclaims=3", "reclaims=2"),
                         ("total_reclaims=5", "total_reclaims=4"),
                         ("module_execution=false", "module_execution=true"),
                         ("physical_wx=false", "physical_wx=true"),
                         ("smp_mapping=false", "smp_mapping=true"),
                         ("write_fault_probed=false", "write_fault_probed=true")):
            with self.subTest(claim=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))
        self.assertEqual(verify("\x1b[32m" + good.replace("\n", "\r\n") + "\x1b[0m")["status"], "passed")
        for bad in (good.replace("1 passed", "1 failed"), good + "kernel panicked\n",
                    good + "assertion failed\n", good + "CYBERMANJU_OTHER:x\n",
                    good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n",
                    good.replace(COMPLETE + "\n", "test result: ok. 1 passed; 0 failed; 0 filtered out.\n" + COMPLETE + "\n")):
            with self.subTest(bad=bad[-50:]), self.assertRaises(ValueError):
                verify(bad)

    def test_cli_success_and_invalidates_stale_result_on_bad_or_missing_log(self):
        script = Path(__file__).with_name("verify_signed32_ro_lease_runtime.py")
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "runtime.log"
            output = log.with_name("result.json")
            log.write_text(self.evidence())
            proc = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual(json.loads(output.read_text())["scope"], "single_cpu_read_only_nonexec_alias")
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
