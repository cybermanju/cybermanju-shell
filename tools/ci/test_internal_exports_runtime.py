import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from verify_internal_exports_runtime import ADMISSION, BEGIN, COMPLETE, FAULT, OWNER, PAGE, POOL_START, RESOLVER, verify


class InternalExportsTests(unittest.TestCase):
    def evidence(self):
        base = POOL_START + 16 * PAGE
        self.cases = [f"CYBERMANJU_INTERNAL_EXPORTS:staged:PASS kind={kind} base={base:#x} reserved=40960 "
                      f"span=40960 pages=10 payload=1721 relas=6 symbol=0xffffffff88010000 field={base+32:#x} "
                      "compared=40960 registry_match=true" for kind in (2, 4)]
        self.budget = "CYBERMANJU_INTERNAL_EXPORTS:budget:PASS rejected=5 early_precedence=2 reclaimed=8 exact_limit_staged=1 pages=10 chunks=10 compared=40960 pages_mapped=false"
        return "\n".join([BEGIN, ADMISSION, *self.cases, RESOLVER, OWNER, self.budget, FAULT, COMPLETE, "test result: ok. 1 passed; 0 failed; 4 filtered out.", ""])

    def test_valid_registry_is_not_linux_abi(self):
        r = verify(self.evidence())
        self.assertEqual((r["internal_registry_entries"], r["denied_name_cases"], r["missing_import_cases"], r["staged_cases"], r["available_range_checks"]), (1, 5, 2, 2, 4))
        self.assertEqual(r["internal_registry_symbol"], "cybermanju_ostd_num_cpus")
        self.assertEqual(r["registry_function_abi"], "rust_internal")
        self.assertEqual(r["registry_function_signature"], "fn() -> usize")
        self.assertEqual(r["linux_admission_policy"], "reject_only")
        self.assertEqual((r["linux_admitted_exports"], r["linux_abi_rejection_cases"], r["linux_unknown_name_rejection_cases"]), (0, 1, 5))
        self.assertTrue(r["export_descriptor_verified"])
        self.assertTrue(r["linux_admission_rejection_verified"])
        self.assertTrue(r["policy_symbol_resolver_verified"])
        self.assertFalse(r["policy_numeric_fallback_available"])
        self.assertEqual((r["policy_abi_rejected_prepare_cases"], r["policy_unknown_rejected_prepare_cases"],
                          r["policy_preflight_rejected_cases"], r["policy_local_only_prepared_cases"]), (2, 2, 2, 1))
        self.assertTrue(r["typed_policy_owner_verified"])
        self.assertEqual((r["policy_owner_ctor_abi_rejections"], r["policy_owner_ctor_unknown_rejections"],
                          r["policy_owner_prepare_abi_rejections"], r["policy_owner_occupied_precedence_cases"],
                          r["policy_owner_reclaimed_ranges"], r["policy_owner_local_only_staged_cases"],
                          r["policy_owner_compared_bytes"]), (2, 2, 2, 2, 7, 1, 40960))
        self.assertFalse(r["policy_owner_numeric_downgrade_available"])
        for key in ("internal_registry_lookup_verified", "full_span_readback_verified", "fixture_imports_renamed_for_internal_registry"):
            self.assertTrue(r[key])
        for key in ("kernel_linux_export_lookup_available", "linux_abi_verified", "pages_mapped", "production_mapping_available",
                    "module_execution_verified", "linux_modules_executed", "init_called", "wx_verified", "smp_mapping_verified"):
            self.assertFalse(r[key])

    def test_unique_complete_ordered_markers(self):
        good = self.evidence()
        for marker in [BEGIN, ADMISSION, *self.cases, RESOLVER, OWNER, self.budget, FAULT, COMPLETE]:
            for bad in (good.replace(marker + "\n", ""), good + marker + "\n", good.replace(marker, "echo " + marker)):
                with self.assertRaises(ValueError): verify(bad)
        for bad in (good + "CYBERMANJU_OTHER:PASS\n", good.replace("kind=4", "kind=2"),
                    good.replace(COMPLETE + "\n", "") + COMPLETE + "\n",
                    good.replace(self.cases[0], "SWAP").replace(self.cases[1], self.cases[0]).replace("SWAP", self.cases[1])):
            with self.assertRaises(ValueError): verify(bad)

    def test_counts_and_capabilities_are_exact(self):
        good = self.evidence()
        for old, new in [("cpus=1", "cpus=2"), ("entries=1", "entries=2"), ("staged=2", "staged=1"),
                         ("denied_names=5", "denied_names=4"), ("missing_imports=2", "missing_imports=1"),
                         ("available_checks=4", "available_checks=3"), ("registry_match=true", "registry_match=false"),
                         ("compared_full_span=true", "compared_full_span=false"), ("pages_mapped=false", "pages_mapped=true"),
                         ("linux_abi=false", "linux_abi=true"), ("execution=false", "execution=true")]:
            with self.subTest(old=old), self.assertRaises(ValueError): verify(good.replace(old, new))

    def test_admission_requires_all_rejection_claims(self):
        good = self.evidence()
        for old, new in [("described=1", "described=0"), ("rust_internal=1", "rust_internal=0"),
                         ("rejected_abi=1", "rejected_abi=0"), ("rejected_names=5", "rejected_names=4"),
                         ("numeric_export_preserved=true", "numeric_export_preserved=false"),
                         ("no_admitted_linux_exports=true", "no_admitted_linux_exports=false"),
                         ("abi_rejection=true", "abi_rejection=false")]:
            with self.subTest(old=old), self.assertRaises(ValueError): verify(good.replace(old, new))

    def test_admission_must_precede_staging(self):
        good = self.evidence()
        for index in (0, 3, 4):
            lines = good.splitlines()
            lines.remove(ADMISSION)
            lines.insert(index, ADMISSION)
            with self.subTest(index=index), self.assertRaises(ValueError): verify("\n".join(lines))

    def test_old_numeric_only_evidence_cannot_prove_admission(self):
        old = self.evidence().replace(ADMISSION + "\n", "").replace(" abi_rejection=true", "")
        with self.assertRaises(ValueError): verify(old)

    def test_admission_metadata_does_not_promote_other_capabilities(self):
        r = verify(self.evidence())
        self.assertTrue(r["linux_admission_rejection_verified"])
        for key in ("linux_abi_verified", "module_execution_verified", "linux_modules_executed",
                    "kernel_linux_export_lookup_available", "production_mapping_available", "wx_verified", "smp_mapping_verified"):
            self.assertIs(r[key], False)

    def test_provider_claims_require_rejections_and_no_fallback(self):
        good = self.evidence()
        for old, new in [("checked_abi_rejections=2", "checked_abi_rejections=1"),
                         ("checked_unknown_rejections=2", "checked_unknown_rejections=0"),
                         ("preflight_rejections=2", "preflight_rejections=1"),
                         ("local_only_prepared=1", "local_only_prepared=0"),
                         ("numeric_fallback=false", "numeric_fallback=true"),
                         ("policy_resolver=true", "policy_resolver=false")]:
            with self.subTest(old=old), self.assertRaises(ValueError): verify(good.replace(old, new))

    def test_provider_marker_must_follow_staging_before_cleanup(self):
        good = self.evidence()
        for index in (0, 1, 2, 3, 5, 6):
            lines = good.splitlines()
            lines.remove(RESOLVER)
            lines.insert(index, RESOLVER)
            with self.subTest(index=index), self.assertRaises(ValueError): verify("\n".join(lines))

    def test_old_admission_only_evidence_does_not_prove_provider(self):
        old = self.evidence().replace(RESOLVER + "\n", "").replace(" policy_resolver=true", "")
        with self.assertRaises(ValueError): verify(old)

    def test_checked_resolution_does_not_admit_linux_or_execute(self):
        r = verify(self.evidence())
        self.assertTrue(r["policy_symbol_resolver_verified"])
        self.assertEqual(r["linux_admitted_exports"], 0)
        for key in ("linux_abi_verified", "linux_modules_executed", "module_execution_verified", "init_called",
                    "production_mapping_available", "smp_mapping_verified", "wx_verified", "policy_numeric_fallback_available"):
            self.assertIs(r[key], False)

    def test_owner_requires_all_transactional_proofs(self):
        good = self.evidence()
        for old, new in [("ctor_abi_rejections=2", "ctor_abi_rejections=1"),
                         ("ctor_unknown_rejections=2", "ctor_unknown_rejections=0"),
                         ("prepare_abi_rejections=2", "prepare_abi_rejections=0"),
                         ("occupied_precedence=2", "occupied_precedence=0"),
                         ("reclaimed_ranges=7", "reclaimed_ranges=6"),
                         ("local_only_staged=1", "local_only_staged=0"),
                         ("typed_policy=true", "typed_policy=false"), ("policy_owner=true", "policy_owner=false")]:
            with self.subTest(old=old), self.assertRaises(ValueError): verify(good.replace(old, new))

    def test_owner_must_follow_provider_before_cleanup(self):
        good = self.evidence()
        for index in (0, 1, 2, 3, 4, 6, 7):
            lines = good.splitlines()
            lines.remove(OWNER)
            lines.insert(index, OWNER)
            with self.subTest(index=index), self.assertRaises(ValueError): verify("\n".join(lines))

    def test_provider_only_evidence_cannot_prove_owner(self):
        old = self.evidence().replace(OWNER + "\n", "").replace(" policy_owner=true", "")
        with self.assertRaises(ValueError): verify(old)

    def test_typed_owner_span_must_match_numeric_fixture(self):
        good = self.evidence()
        for case in self.cases:
            good = good.replace(case, case.replace("span=40960", "span=8192").replace("pages=10", "pages=2").replace("compared=40960", "compared=8192"))
        with self.assertRaisesRegex(ValueError, "Typed owner"): verify(good)

    def test_typed_policy_does_not_promote_execution_or_global_admission(self):
        r = verify(self.evidence())
        self.assertTrue(r["typed_policy_owner_verified"])
        self.assertEqual(r["linux_admitted_exports"], 0)
        for key in ("linux_abi_verified", "module_execution_verified", "linux_modules_executed", "init_called",
                    "production_mapping_available", "wx_verified", "smp_mapping_verified", "policy_owner_numeric_downgrade_available"):
            self.assertIs(r[key], False)

    def test_budget_metadata_and_physical_failure_scope(self):
        r = verify(self.evidence())
        self.assertTrue(r["policy_budget_checks_verified"])
        self.assertEqual((r["policy_budget_rejected_cases"], r["policy_budget_early_precedence_cases"],
                          r["policy_budget_reclaimed_ranges"], r["policy_budget_exact_limit_staged_cases"],
                          r["policy_budget_page_limit"], r["policy_budget_chunk_limit"],
                          r["policy_budget_compared_bytes"]), (5, 2, 8, 1, 10, 10, 40960))
        self.assertEqual(r["linux_admitted_exports"], 0)
        for key in ("linux_abi_verified",
                    "module_execution_verified", "production_mapping_available", "smp_mapping_verified", "wx_verified"):
            self.assertIs(r[key], False)

    def test_budget_claims_are_complete(self):
        good = self.evidence()
        for old, new in [("rejected=5", "rejected=4"), ("early_precedence=2", "early_precedence=1"),
                         ("reclaimed=8", "reclaimed=7"), ("exact_limit_staged=1", "exact_limit_staged=0"),
                         ("budget_checks=true", "budget_checks=false")]:
            with self.subTest(old=old), self.assertRaises(ValueError): verify(good.replace(old, new))

    def test_budget_must_follow_owner_before_cleanup(self):
        good = self.evidence()
        for index in (0, 1, 2, 3, 4, 5, 7, 8):
            lines = good.splitlines()
            lines.remove(self.budget)
            lines.insert(index, self.budget)
            with self.subTest(index=index), self.assertRaises(ValueError): verify("\n".join(lines))

    def test_budget_chunk_limit_must_fit_fixture_bound(self):
        good = self.evidence()
        for value in (0, 1025):
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "chunk budget"):
                verify(good.replace("chunks=10", f"chunks={value}"))
        for value in (1, 1024):
            self.assertEqual(verify(good.replace("chunks=10", f"chunks={value}"))["policy_budget_chunk_limit"], value)

    def test_old_owner_evidence_does_not_prove_budgets(self):
        old = self.evidence().replace(self.budget + "\n", "").replace(" budget_checks=true", "")
        with self.assertRaises(ValueError): verify(old)

    def test_fault_cleanup_is_not_actual_oom_or_execution(self):
        r = verify(self.evidence())
        for key in ("policy_fault_cleanup_verified", "fault_backend_real_frames", "physical_allocator_failure_injected", "frame_copy_failure_injected"):
            self.assertIs(r[key], True)
        self.assertEqual((r["policy_fault_allocation_cases"], r["policy_fault_copy_cases"],
                          r["policy_fault_acquired_handles"], r["policy_fault_dropped_handles"],
                          r["policy_fault_successful_writes"], r["policy_fault_reclaimed_ranges"]), (3, 2, 30, 30, 1, 5))
        self.assertEqual(r["linux_admitted_exports"], 0)
        for key in ("actual_oom_observed", "hardware_write_failure_observed", "production_fault_hook_available",
                    "linux_abi_verified", "linux_modules_executed", "pages_mapped", "production_mapping_available", "smp_mapping_verified", "wx_verified"):
            self.assertIs(r[key], False)

    def test_fault_counts_and_scope_are_exact(self):
        good = self.evidence()
        for old, new in [("alloc_cases=3", "alloc_cases=2"), ("copy_cases=2", "copy_cases=0"),
                         ("acquired=30", "acquired=29"), ("dropped=30", "dropped=29"),
                         ("writes=1", "writes=0"), ("reclaimed=5", "reclaimed=4"),
                         ("real_frames=true", "real_frames=false"), ("production_hook=false", "production_hook=true"),
                         ("actual_oom=false", "actual_oom=true"), ("fault_cleanup=true", "fault_cleanup=false")]:
            with self.subTest(old=old), self.assertRaises(ValueError): verify(good.replace(old, new))

    def test_fault_marker_must_follow_budgets_before_cleanup(self):
        good = self.evidence()
        for index in (0, 1, 2, 3, 4, 5, 6, 8, 9):
            lines = good.splitlines()
            lines.remove(FAULT)
            lines.insert(index, FAULT)
            with self.subTest(index=index), self.assertRaises(ValueError): verify("\n".join(lines))

    def test_old_budget_evidence_does_not_prove_fault_cleanup(self):
        old = self.evidence().replace(FAULT + "\n", "").replace(" fault_cleanup=true", "")
        with self.assertRaises(ValueError): verify(old)

    def test_geometry_and_full_readback(self):
        good = self.evidence()
        base = POOL_START + 16 * PAGE
        for old, new in [(f"base={base:#x}", f"base={base+PAGE:#x}"), ("reserved=40960", "reserved=4096"),
                         ("reserved=40960", "reserved=4198400"), ("span=40960", "span=40959"), ("pages=10", "pages=9"),
                         ("payload=1721", "payload=40961"), ("relas=6", "relas=0"), ("relas=6", "relas=65537"),
                         ("compared=40960", "compared=40959"), (f"field={base+32:#x}", f"field={base-1:#x}"),
                         (f"field={base+32:#x}", f"field={base+40958:#x}")]:
            with self.subTest(old=old), self.assertRaises(ValueError): verify(good.replace(old, new))
        with self.assertRaises(ValueError):
            verify(good.replace(self.cases[1], self.cases[1].replace("payload=1721", "payload=1722")))

    def test_code_identity_must_match(self):
        good = self.evidence()
        for symbol in ("0x0", "0xffffffffc0000000", "0x1000"):
            with self.assertRaises(ValueError): verify(good.replace("symbol=0xffffffff88010000", "symbol=" + symbol))
        with self.assertRaises(ValueError):
            verify(good.replace(self.cases[1], self.cases[1].replace("symbol=0xffffffff88010000", "symbol=0xffffffff88020000")))

    def test_summary_and_serial_failures(self):
        good = self.evidence()
        self.assertEqual(verify("\x1b[32m" + good.replace("\n", "\r\n") + "\x1b[0m")["status"], "passed")
        for bad in (good.replace("1 passed", "0 passed"), good + "kernel panicked\n", good + "assertion failed\n",
                    good + "test result: ok. 1 passed; 0 failed; 4 filtered out.\n", "Finished dev profile\n"):
            with self.assertRaises(ValueError): verify(bad)

    def test_cli_invalidates_stale_success(self):
        script = Path(__file__).with_name("verify_internal_exports_runtime.py")
        with tempfile.TemporaryDirectory() as d:
            log = Path(d) / "runtime.log"
            log.write_text(self.evidence())
            r = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
            log.write_text("build only\n")
            r = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertNotEqual(r.returncode, 0)
            self.assertEqual(json.loads(log.with_name("result.json").read_text())["status"], "not_verified")

    def test_mode_preflight_and_forwarding(self):
        import os
        import shutil
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            scripts = root / "tools/ci"
            scripts.mkdir(parents=True)
            for name in ("test_prepared_module_runtime.sh", "test_module_staging_runtime.sh"):
                shutil.copyfile(Path(__file__).with_name(name), scripts / name)
            config = root / "asterinas/kernel/libs/linux-module-staging/OSDK.toml"
            config.parent.mkdir(parents=True)
            config.write_text("keep registry config\n")
            fixture = root / "fixture.ko"
            fixture.write_bytes(b"fixture")
            out = root / "out"
            env = {**os.environ, "CYBERMANJU_STAGING_FIXTURE": str(fixture)}
            r = subprocess.run(["bash", str(scripts / "test_module_staging_runtime.sh"), str(out), "--internal-exports"], env=env, capture_output=True, text=True)
            self.assertEqual(r.returncode, 2, r.stderr)
            self.assertIn("Refusing to overwrite", r.stderr)
            self.assertEqual(config.read_text(), "keep registry config\n")
            self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")
            self.assertFalse((out / "osdk-build.log").exists())
            (scripts / "test_linux_module_noimports.sh").write_text('mkdir -p "$1"\nprintf fixture > "$1/cybermanju_noimports_fixture.ko"\n')
            (scripts / "test_module_staging_runtime.sh").write_text('printf "%s\\n" "$2" > "$1/mode.txt"\n')
            r = subprocess.run(["bash", str(scripts / "test_prepared_module_runtime.sh"), str(out), "--internal-exports"], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual((out / "mode.txt").read_text(), "--internal-exports\n")


if __name__ == "__main__":
    unittest.main()
