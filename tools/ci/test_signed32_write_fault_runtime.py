# SPDX-License-Identifier: MPL-2.0
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from verify_signed32_write_fault_runtime import EXPECTED, verify


class Signed32WriteFaultGateTests(unittest.TestCase):
    def evidence(self):
        return "\n".join([*EXPECTED, "test result: ok. 1 passed; 0 failed; 18 filtered out.", ""])

    def test_valid_hardware_counts_and_limits(self):
        result = verify(self.evidence())
        self.assertEqual((result["hardware_write_faults"], result["rw_control_stores"], result["fault_error_code"]), (4, 5, 3))
        self.assertEqual(result["fault_offsets"], [0, 4095, 8192, 12287])
        self.assertEqual((result["handles_acquired"], result["handles_dropped"], result["reclaimed_reservations"]), (3, 3, 1))
        self.assertEqual(result["synthetic_context_rejections"], 12)
        self.assertTrue(result["recovery_ktest_only"])
        self.assertTrue(result["linear_rw_alias"])
        for claim in ("physical_wx", "rx_enabled", "elf_execution", "smp_mapping", "production_qualified", "production_hook", "immutable_data", "cr0_wp_modified"):
            self.assertFalse(result[claim])

    def test_fault_and_cleanup_mutations(self):
        good = self.evidence()
        for old, new in (("rejected=12", "rejected=11"), ("faults=4", "faults=3"),
                         ("traps=1", "traps=0"), ("error_code=3", "error_code=2"),
                         ("cr2_matches=true", "cr2_matches=false"), ("unchanged=true", "unchanged=false"),
                         ("wp_enabled=true", "wp_enabled=false"), ("rw_stores=5", "rw_stores=4"),
                         ("dropped=3", "dropped=2"), ("pte_absent=4", "pte_absent=3"),
                         ("caller_irq_disabled=true", "caller_irq_disabled=false")):
            with self.subTest(mutation=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))

    def test_capability_escalations(self):
        good = self.evidence()
        for name in ("physical_wx", "rx_enabled", "elf_execution", "smp_mapping", "production_qualified", "production_hook", "immutable_data"):
            with self.subTest(claim=name), self.assertRaises(ValueError):
                verify(good.replace(f"{name}=false", f"{name}=true"))
        with self.assertRaises(ValueError):
            verify(good.replace("recovery_ktest_only=true", "recovery_ktest_only=false"))

    def test_marker_order_and_failure_summary(self):
        good = self.evidence()
        for bad in (good.replace(EXPECTED[4] + "\n", ""), good + EXPECTED[4] + "\n",
                    good.replace(EXPECTED[4], "echo " + EXPECTED[4]),
                    good + "kernel panicked\n", good + "assertion failed\n",
                    good.replace("1 passed", "2 passed"),
                    good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n"):
            with self.subTest(), self.assertRaises(ValueError):
                verify(bad)

    def test_cli_invalidates_old_success(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "runtime.log"
            path.write_text("not native evidence\n")
            out = path.with_name("result.json")
            out.write_text('{"status":"passed"}\n')
            process = subprocess.run([sys.executable, str(Path(__file__).with_name("verify_signed32_write_fault_runtime.py")), str(path)], capture_output=True)
            self.assertNotEqual(process.returncode, 0)
            self.assertEqual(json.loads(out.read_text())["status"], "not_verified")


if __name__ == "__main__":
    unittest.main()
