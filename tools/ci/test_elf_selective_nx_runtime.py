# SPDX-License-Identifier: MPL-2.0
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from verify_elf_selective_nx_runtime import EXPECTED, verify
from verify_elf_selective_nx_smp_runtime import EXPECTED as SMP_EXPECTED
from verify_elf_selective_nx_smp_runtime import verify as verify_smp


class ElfSelectiveNxGateTests(unittest.TestCase):
    def evidence(self):
        return "\n".join([*EXPECTED, "test result: ok. 1 passed; 0 failed; 5 filtered out.", ""])

    def test_valid_scope_and_totals(self):
        result = verify(self.evidence())
        self.assertEqual((result["cases"], result["readback_bytes"], result["pte_observations"]), (4, 163860, 40))
        self.assertEqual((result["reclaimed_reservations"], result["handles_acquired"], result["handles_dropped"]), (13, 116, 116))
        self.assertEqual(result["gap_mapper_entries"], 0)
        self.assertEqual(result["gap_public_rejections"], 1)
        self.assertEqual(result["gap_observed_rejections"], 1)
        self.assertTrue(result["nx_projection_applied"])
        for claim in ("elf_rx_applied", "gap_mapping_supported", "module_execution", "physical_wx", "smp_mapping", "write_fault_probed", "production_qualified"):
            self.assertFalse(result[claim])

    def test_projection_and_resource_counts_rejected(self):
        good = self.evidence()
        for old, new in (("ro_nonx=6", "ro_nonx=5"), ("rw_nonx=4", "rw_nonx=5"),
                         ("pte_pages=10", "pte_pages=9"), ("frame_matches=10", "frame_matches=9"),
                         ("prefix=7", "prefix=6"), ("mapper_entries=0", "mapper_entries=1"),
                         ("inaccessible_pages=1", "inaccessible_pages=0"), ("reclaims=13", "reclaims=12"),
                         ("acquired=116", "acquired=115"), ("dropped=116", "dropped=115"),
                         ("bytes=163860", "bytes=163859"), ("bounds=8", "bounds=7")):
            with self.subTest(mutation=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))

    def test_claim_escalation_rejected(self):
        good = self.evidence()
        for claim in ("elf_rx_applied", "gap_mapping_supported", "module_execution", "physical_wx", "smp_mapping", "write_fault_probed", "production_qualified"):
            with self.subTest(claim=claim), self.assertRaises(ValueError):
                verify(good.replace(f"{claim}=false", f"{claim}=true"))
        with self.assertRaises(ValueError):
            verify(good.replace("nx_projection_applied=true", "nx_projection_applied=false"))

    def test_order_failures_and_summary_rejected(self):
        good = self.evidence()
        for bad in (good.replace(EXPECTED[3] + "\n", ""), good + EXPECTED[3] + "\n",
                    good.replace(EXPECTED[4], "echo " + EXPECTED[4]),
                    good + "kernel panicked\n", good + "assertion failed\n",
                    good.replace("1 passed", "2 passed"),
                    good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n"):
            with self.subTest(), self.assertRaises(ValueError):
                verify(bad)

    def test_cli_failure_overwrites_stale_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "runtime.log"
            log.write_text("not native evidence\n")
            out = log.with_name("result.json")
            out.write_text('{"status":"passed"}\n')
            process = subprocess.run([sys.executable, str(Path(__file__).with_name("verify_elf_selective_nx_runtime.py")), str(log)], capture_output=True)
            self.assertNotEqual(process.returncode, 0)
            self.assertEqual(json.loads(out.read_text())["status"], "not_verified")


class ElfSelectiveNxSmpGateTests(unittest.TestCase):
    def evidence(self):
        return "\n".join([*SMP_EXPECTED, "test result: ok. 1 passed; 0 failed; 5 filtered out.", ""])

    def test_factory_only_counts(self):
        result = verify_smp(self.evidence())
        self.assertEqual((result["handles_acquired"], result["handles_dropped"], result["reclaimed_reservations"]), (10, 10, 1))
        self.assertFalse(result["selective_mapping"])
        self.assertFalse(result["module_execution"])

    def test_scope_counts_and_failure_mutations(self):
        good = self.evidence()
        for old, new in (("acquired=10", "acquired=9"), ("dropped=10", "dropped=9"),
                         ("selective_mapping=false", "selective_mapping=true"),
                         ("module_execution=false", "module_execution=true"),
                         ("factory_rw_denied_before_conversion=true", "factory_rw_denied_before_conversion=false"),
                         ("irq_preserved=true", "irq_preserved=false")):
            with self.subTest(mutation=old), self.assertRaises(ValueError):
                verify_smp(good.replace(old, new))
        with self.assertRaises(ValueError):
            verify_smp(good + "assertion failed\n")


if __name__ == "__main__":
    unittest.main()
