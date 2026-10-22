import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_prepared_module_runtime import SCENARIOS, relative32_numeric_placement_window, verify


class PreparedRuntimeEvidenceTests(unittest.TestCase):
    def evidence(self):
        return (
            "\n".join(
                [
                    "CYBERMANJU_PREPARED_RUNTIME:roundtrip:PASS sections=13 span=40960 payload=1721 relocations=6 pages=10",
                    "CYBERMANJU_PREPARED_RUNTIME:rejected_budgets_and_mismatched_layout:PASS",
                    "CYBERMANJU_PREPARED_RUNTIME:reserved_layout:PASS base=0xffffc00000000000 reserved=40960 span=40960 sections=13 payload=1721 relocations=6 pages=10",
                    "CYBERMANJU_PREPARED_RUNTIME:no_init_or_exit_called:PASS",
                    "CYBERMANJU_PREPARED_RUNTIME:reserved_owner_lifecycle:PASS cases=3",
                    "CYBERMANJU_PREPARED_RUNTIME:scoped_rw_mapping:PASS pages=10",
                    "CYBERMANJU_PREPARED_RUNTIME:prepared_transaction:PASS rejection_cases=7 range_overflow=true reservation_released=true",
                    "CYBERMANJU_PREPARED_RUNTIME:relative32_reach:PASS types=2 boundary_cases=8 missing_exports=1 far_exports=2",
                    "CYBERMANJU_PREPARED_RUNTIME:reach_diagnostics:PASS overflow_sites=4 missing_sites=1 budget_no_site=1 real_code_exports=2",
                    "CYBERMANJU_PREPARED_RUNTIME:signed32_absolute:PASS accepted=2 rejected=3 kernel_export_numeric=true execution=false",
                    "CYBERMANJU_PREPARED_RUNTIME:relative32_placement_window:PASS kinds=2 rejected_current_base=2 candidate_preflight=2 reserved_near_text=false execution=false",
                    'CYBERMANJU_PREPARED_RUNTIME:relative32_import_collection:PASS kinds=2 collected_imports=2 candidate_preflight=2 reserved_near_text=false execution=false',
                    "CYBERMANJU_PREPARED_RUNTIME:relative32_defined_collection:PASS types=2 section_relative=2 absolute=2 kernel_address_numeric=true reserved_near_text=false execution=false",
                    "test result: ok. 1 passed; 0 failed; 301 filtered out.",
                ]
            )
            + "\n"
        )

    def test_import_collection_marker_exact_and_mandatory(self):
        marker = 'CYBERMANJU_PREPARED_RUNTIME:relative32_import_collection:PASS kinds=2 collected_imports=2 candidate_preflight=2 reserved_near_text=false execution=false'
        good = self.evidence()
        variants = [good.replace(marker + "\n", ""), good + marker + "\n",
                    good.replace(marker, "echo " + marker)]
        for original, changed in [("kinds=2", "kinds=3"),
                                  ("collected_imports=2", "collected_imports=0"),
                                  ("candidate_preflight=2", "candidate_preflight=1"),
                                  ("reserved_near_text=false", "reserved_near_text=true"),
                                  ("execution=false", "execution=true")]:
            variants.append(good.replace(marker, marker.replace(original, changed)))
        for text in variants:
            with self.assertRaises(ValueError):
                verify(text)
        self.assertTrue(verify(good)["relative32_import_constraints_verified"])

    def test_defined_symbol_collection_marker_exact_and_mandatory(self):
        marker = "CYBERMANJU_PREPARED_RUNTIME:relative32_defined_collection:PASS types=2 section_relative=2 absolute=2 kernel_address_numeric=true reserved_near_text=false execution=false"
        good = self.evidence()
        bad = [good.replace(marker + "\n", ""), good + marker + "\n",
               good.replace(marker, "echo " + marker)]
        for old, new in [("types=2", "types=1"), ("section_relative=2", "section_relative=1"),
                         ("absolute=2", "absolute=1"),
                         ("kernel_address_numeric=true", "kernel_address_numeric=false"),
                         ("reserved_near_text=false", "reserved_near_text=true"),
                         ("execution=false", "execution=true")]:
            bad.append(good.replace(marker, marker.replace(old, new)))
        for text in bad:
            with self.subTest(marker=text[-180:]), self.assertRaises(ValueError):
                verify(text)
        result = verify(good)
        self.assertTrue(result["relative32_defined_symbol_constraints_verified"])
        self.assertEqual(result["relative32_defined_section_relative_cases"], 2)
        self.assertEqual(result["relative32_defined_absolute_cases"], 2)
        for flag in ("near_text_allocation_available", "module_execution_verified", "linux_modules_executed", "wx_verified"):
            self.assertFalse(result[flag])

    def test_complete_evidence_and_result_contract(self):
        result = verify(self.evidence())
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["runtime_tests"], 1)
        self.assertEqual(result["scope"], "prepared_module_scoped_rw_readback_only")
        self.assertTrue(result["joint_owner_lifecycle_verified"])
        self.assertEqual(result["owner_lifecycle_cases"], 3)
        self.assertTrue(result["reserved_prepare_transaction_verified"])
        self.assertEqual(result["transaction_rejection_cases"], 7)
        self.assertTrue(result["reserved_base_relocation_overflow_rejected"])
        self.assertTrue(result["fixture_staged"])
        self.assertFalse(result["linux_modules_executed"])
        self.assertFalse(result["init_called"])
        self.assertTrue(result["pages_mapped"])
        self.assertFalse(result["mapping_persists_after_read"])
        self.assertTrue(result["scoped_mapping_readback_verified"])
        self.assertFalse(result["new_alias_execute_flag"])
        self.assertFalse(result["global_nx_verified"])
        self.assertEqual(result["tlb_scope"], "current_cpu_scoped")
        self.assertEqual(result["mapping_scope"], "ktest_single_cpu_only")
        self.assertFalse(result["production_mapping_available"])
        self.assertFalse(result["wx_verified"])
        self.assertTrue(result["reservation_held_during_staging"])
        self.assertTrue(result["signed32_absolute_sign_extension_verified"])
        self.assertTrue(result["signed32_kernel_export_numeric_only"])
        self.assertFalse(result["module_execution_verified"])
        self.assertTrue(result["relative32_numeric_placement_window_verified"])
        self.assertTrue(result["relative32_defined_symbol_constraints_verified"])
        self.assertFalse(result["near_text_allocation_available"])
        self.assertEqual(result["reserved_layout"]["base"], "0xffffc00000000000")
        self.assertNotIn("linux_modules_loaded", result)
        for field in ("sections", "span", "payload", "relocations", "pages"):
            self.assertIsInstance(result[field], int)

    def test_ansi_and_c0_prefixes_are_accepted(self):
        decorated = "\x03\x1b[32m" + self.evidence().replace(
            "ok.", "\x1b[32mok\x1b[0m."
        )
        self.assertEqual(verify(decorated)["status"], "passed")

    def test_crlf_is_accepted_but_isolated_cr_is_rejected(self):
        self.assertEqual(verify(self.evidence().replace("\n", "\r\n"))["status"], "passed")
        with self.assertRaises(ValueError):
            verify(self.evidence().replace("prepared_transaction", "prepared\r_transaction"))

    def test_reach_diagnostics_marker_is_exact_and_mandatory(self):
        marker = "CYBERMANJU_PREPARED_RUNTIME:reach_diagnostics:PASS overflow_sites=4 missing_sites=1 budget_no_site=1 real_code_exports=2"
        variants = [self.evidence().replace(marker + "\n", ""), self.evidence() + marker + "\n"]
        variants += [self.evidence().replace(a, b) for a, b in [("overflow_sites=4", "overflow_sites=3"), ("missing_sites=1", "missing_sites=0"), ("budget_no_site=1", "budget_no_site=0"), ("real_code_exports=2", "real_code_exports=0"), ("reach_diagnostics:PASS", "reach_diagnostics:FAIL")]]
        for bad in variants:
            with self.subTest(log=bad), self.assertRaises(ValueError):
                verify(bad)
        result = verify(self.evidence())
        self.assertTrue(result["reach_diagnostics_verified"])
        self.assertEqual(result["real_code_addresses_tested"], 2)
        self.assertFalse(result["kernel_linux_export_lookup_available"])
        self.assertTrue(result["test_export_bindings_are_synthetic"])

    def test_relative_reach_marker_is_exact_and_mandatory(self):
        marker = "CYBERMANJU_PREPARED_RUNTIME:relative32_reach:PASS types=2 boundary_cases=8 missing_exports=1 far_exports=2"
        for bad in [self.evidence().replace(marker + "\n", ""), self.evidence() + marker + "\n"] + [self.evidence().replace(a, b) for a, b in [("types=2", "types=1"), ("boundary_cases=8", "boundary_cases=7"), ("missing_exports=1", "missing_exports=0"), ("far_exports=2", "far_exports=1"), ("relative32_reach", "unknown_relative_reach")]]:
            with self.subTest(log=bad), self.assertRaises(ValueError):
                verify(bad)
        result = verify(self.evidence())
        self.assertTrue(result["relative32_reach_preflight_verified"])
        self.assertTrue(result["exports_are_synthetic"])
        for flag in ("linux_modules_executed", "init_called", "production_mapping_available", "wx_verified"):
            self.assertFalse(result[flag])

    def test_transaction_marker_is_exact_and_mandatory(self):
        marker = "CYBERMANJU_PREPARED_RUNTIME:prepared_transaction:PASS rejection_cases=7 range_overflow=true reservation_released=true"
        for bad in (
            self.evidence().replace(marker + "\n", ""),
            self.evidence().replace("rejection_cases=7", "rejection_cases=6"),
            self.evidence().replace("range_overflow=true", "range_overflow=false"),
            self.evidence().replace("reservation_released=true", "reservation_released=false"),
            self.evidence() + "CYBERMANJU_PREPARED_RUNTIME:prepared_transaction:PASS rejection_cases=7 range_overflow=true reservation_released=true\n",
            self.evidence() + "CYBERMANJU_PREPARED_RUNTIME:unknown:PASS\n",
            self.evidence() + "CYBERMANJU_PREPARED_RUNTIME:prepared_transaction:FAIL rejection_cases=7 range_overflow=true reservation_released=true\n",
        ):
            with self.subTest(bad=bad[-110:]), self.assertRaises(ValueError):
                verify(bad)

    def test_signed32_absolute_marker_is_exact_and_mandatory(self):
        marker = "CYBERMANJU_PREPARED_RUNTIME:signed32_absolute:PASS accepted=2 rejected=3 kernel_export_numeric=true execution=false"
        variants = [self.evidence().replace(marker + "\n", ""), self.evidence() + marker + "\n"]
        variants += [self.evidence().replace(a, b) for a, b in [
            ("accepted=2", "accepted=1"), ("rejected=3", "rejected=2"),
            ("kernel_export_numeric=true", "kernel_export_numeric=false"),
            ("execution=false", "execution=true"),
            ("signed32_absolute:PASS", "signed32_absolute:FAIL"),
        ]]
        for bad in variants:
            with self.subTest(log=bad[-120:]), self.assertRaises(ValueError):
                verify(bad)

    def test_relative32_placement_marker_is_exact_and_numeric_only(self):
        marker = "CYBERMANJU_PREPARED_RUNTIME:relative32_placement_window:PASS kinds=2 rejected_current_base=2 candidate_preflight=2 reserved_near_text=false execution=false"
        bad = [self.evidence().replace(marker + "\n", ""), self.evidence() + marker + "\n"]
        for old, new in [("kinds=2", "kinds=1"), ("rejected_current_base=2", "rejected_current_base=1"), ("candidate_preflight=2", "candidate_preflight=1"), ("reserved_near_text=false", "reserved_near_text=true"), ("execution=false", "execution=true"), ("relative32_placement_window:PASS", "relative32_placement_window:FAIL")]:
            bad.append(self.evidence().replace(old, new))
        for text in bad:
            with self.subTest(text=text[-140:]), self.assertRaises(ValueError):
                verify(text)

    def test_numeric_window_excludes_vmalloc(self):
        low, high, candidate = relative32_numeric_placement_window(0xffff80007ffe2321)
        self.assertLessEqual(low, candidate)
        self.assertLessEqual(candidate, high)
        self.assertFalse(0xffffc00000000000 <= candidate < 0xffffe00000000000)

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
            self.evidence().replace("reserved_owner_lifecycle:PASS cases=3", "reserved_owner_lifecycle:PASS cases=2"),
            self.evidence().replace("reserved_owner_lifecycle:PASS cases=3", "prefix CYBERMANJU_PREPARED_RUNTIME:reserved_owner_lifecycle:PASS cases=3"),
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

    def test_scoped_mapping_page_count_is_strict_and_matches_reservation(self):
        base = self.evidence()
        cases = [
            base.replace("scoped_rw_mapping:PASS pages=10", "scoped_rw_mapping:PASS pages=0"),
            base.replace("scoped_rw_mapping:PASS pages=10", "scoped_rw_mapping:PASS pages=1025"),
            base.replace("scoped_rw_mapping:PASS pages=10", "scoped_rw_mapping:PASS pages=11"),
            base.replace("scoped_rw_mapping:PASS pages=10", "scoped_rw_mapping:PASS pages=nope"),
        ]
        for bad in cases:
            with self.subTest(bad=bad.splitlines()[5]), self.assertRaises(ValueError):
                verify(bad)

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
