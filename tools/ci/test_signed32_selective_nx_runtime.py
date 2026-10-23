# SPDX-License-Identifier: MPL-2.0
import unittest

from verify_signed32_selective_nx_runtime import EXPECTED, verify
from verify_signed32_selective_nx_smp_runtime import EXPECTED as SMP_EXPECTED
from verify_signed32_selective_nx_smp_runtime import verify as verify_smp


class SelectiveNxGateTests(unittest.TestCase):
    def evidence(self):
        return "\n".join([*EXPECTED, "test result: ok. 1 passed; 0 failed; 4 filtered out.", ""])

    def test_valid_counts_and_claims(self):
        result = verify(self.evidence())
        self.assertEqual((result["pages"], result["read_only_nonexec_pages"], result["rw_nonexec_pages"]), (3, 2, 1))
        self.assertEqual((result["fault_cases"], result["fault_prefix_pages"], result["fault_reclaims"]), (3, 3, 3))
        self.assertEqual((result["handles_acquired"], result["handles_dropped"], result["live_handles"]), (27, 27, 0))
        self.assertEqual(result["mask"], [True, False, True])
        self.assertEqual((result["total_reclaims"], result["readback_bytes"], result["boundary_cases"]), (9, 36864, 2))
        for claim in ("elf_plan_applied", "module_execution", "physical_wx", "smp_mapping", "write_fault_probed", "production_qualified"):
            self.assertFalse(result[claim])

    def test_mutated_count_and_claim_rejected(self):
        good = self.evidence()
        mutations = (
            ("ro_nonx=2", "ro_nonx=1"), ("rw_nonx=1", "rw_nonx=2"),
            ("frame_matches=3", "frame_matches=2"), ("slack_absent=1", "slack_absent=0"),
            ("prefix_pages=3", "prefix_pages=2"), ("acquired=9", "acquired=8"),
            ("malformed_masks=3", "malformed_masks=2"), ("output_preserved=true", "output_preserved=false"),
            ("mask_retained=true", "mask_retained=false"), ("total_reclaims=9", "total_reclaims=8"),
            ("acquired=27", "acquired=26"), ("elf_plan_applied=false", "elf_plan_applied=true"),
            ("physical_wx=false", "physical_wx=true"), ("smp_mapping=false", "smp_mapping=true"),
            ("write_fault_probed=false", "write_fault_probed=true"),
        )
        for old, new in mutations:
            with self.subTest(mutation=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))

    def test_exact_order_failures_and_summary(self):
        good = self.evidence()
        invalid = (
            good.replace(EXPECTED[0] + "\n", ""), good + EXPECTED[0] + "\n",
            good.replace(EXPECTED[2], "echo " + EXPECTED[2]),
            good.replace("1 passed", "1 failed"),
            good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n",
            good + "kernel panicked\n",
        )
        for bad in invalid:
            with self.subTest(), self.assertRaises(ValueError):
                verify(bad)


class SelectiveNxSmpGateTests(unittest.TestCase):
    def evidence(self):
        return "\n".join([*SMP_EXPECTED, "test result: ok. 1 passed; 0 failed; 4 filtered out.", ""])

    def test_claim_is_only_factory_denial(self):
        result = verify_smp(self.evidence())
        self.assertEqual(result["scope"], "smp_factory_rw_denial_before_selective_conversion_only")
        self.assertEqual((result["denied_attempts"], result["reclaimed_reservations"]), (2, 2))
        self.assertFalse(result["selective_mapping"])
        self.assertFalse(result["production_qualified"])

    def test_count_and_claim_mutations_rejected(self):
        good = self.evidence()
        mutations = (
            ("attempts=2", "attempts=1"), ("reclaims=2", "reclaims=1"),
            ("acquired=6", "acquired=5"),
            ("factory_rw_denied_before_conversion=true", "factory_rw_denied_before_conversion=false"),
            ("selective_mapping=false", "selective_mapping=true"),
            ("physical_wx=false", "physical_wx=true"), ("smp_mapping=false", "smp_mapping=true"),
        )
        for old, new in mutations:
            with self.subTest(mutation=old), self.assertRaises(ValueError):
                verify_smp(good.replace(old, new))


if __name__ == "__main__":
    unittest.main()
