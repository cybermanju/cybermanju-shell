import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from verify_near_text_collision_runtime import BEGIN, COMPLETE, PAGE, POOL_END, verify


class NearTextCollisionTests(unittest.TestCase):
    def evidence(self):
        base = POOL_END - 8 * PAGE
        self.cases = [
            f"CYBERMANJU_NEAR_TEXT_COLLISION:case:PASS index={index} "
            f"start={base:#x} end={base + 3 * PAGE:#x} foreign={base + index * PAGE:#x} "
            "denied=2 preserved=2 reclaimed=true" for index in range(3)
        ]
        return "\n".join([BEGIN, *self.cases, COMPLETE,
                           "test result: ok. 1 passed; 0 failed; 264 filtered out.", ""])

    def test_valid_scope_distinguishes_fault_injection_from_reservation_mapping(self):
        result = verify(self.evidence())
        self.assertEqual((result["runtime_tests"], result["scenarios"], result["failed"]), (1, 3, 0))
        self.assertEqual(result["scope"], "ktest_foreign_pte_fault_injection_only")
        self.assertTrue(result["fault_injection_mapping_performed"])
        self.assertEqual(result["fixture_frames_allocated"], 1)
        self.assertEqual(result["injected_mapping_cases"], 3)
        self.assertEqual((result["denied_claims"], result["preserved_snapshots"], result["exact_reclaims"]), (6, 6, 3))
        self.assertEqual(result["va_dereferences"], 0)
        for flag in ("foreign_pte_rejection_verified", "post_audit_claim_rollback_verified", "foreign_pte_preservation_verified"):
            self.assertTrue(result[flag])
        for flag in ("reservation_frames_allocated", "production_mapping_available", "near_text_loader_integration_available",
                     "linux_modules_executed", "module_execution_verified", "smp_mapping_verified", "wx_verified"):
            self.assertFalse(result[flag])
        self.assertEqual([case["index"] for case in result["case_ranges"]], [0, 1, 2])

    def test_each_marker_must_be_complete_unique_and_not_echoed(self):
        good = self.evidence()
        for marker in [BEGIN, *self.cases, COMPLETE]:
            for bad in (good.replace(marker + "\n", ""), good + marker + "\n",
                        good.replace(marker, "echo " + marker), good.replace(marker, marker[:-1])):
                with self.subTest(marker=marker[:70], bad=bad[-100:]), self.assertRaises(ValueError):
                    verify(bad)

    def test_counters_flags_and_unknown_markers_are_rejected(self):
        good = self.evidence()
        mutations = [("cpus=1", "cpus=2"), ("cases=3", "cases=2"), ("span_pages=3", "span_pages=4"),
                     ("fixture_frames=1", "fixture_frames=0"), ("denied=2", "denied=1"),
                     ("preserved=2", "preserved=1"), ("reclaimed=true", "reclaimed=false"),
                     ("denied=6", "denied=5"), ("preserved=6", "preserved=5"), ("reclaimed=3", "reclaimed=2"),
                     ("seeded_pages=3", "seeded_pages=2"), ("va_dereferences=0", "va_dereferences=1"),
                     ("fault_injection_mapping=true", "fault_injection_mapping=false"),
                     ("production_mapping=false", "production_mapping=true"), ("execution=false", "execution=true")]
        for old, new in mutations:
            with self.subTest(old=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))
        with self.assertRaises(ValueError):
            verify(good + "CYBERMANJU_OTHER:PASS\n")

    def test_case_indices_order_and_summary_after_cleanup(self):
        good = self.evidence()
        bad = [good.replace("index=2", "index=1"), good.replace("index=2", "index=3"),
               good.replace(self.cases[0], "SWAP").replace(self.cases[1], self.cases[0]).replace("SWAP", self.cases[1]),
               good.replace(COMPLETE + "\n", "") + COMPLETE + "\n",
               good.replace("1 passed", "0 passed"), good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n"]
        for text in bad:
            with self.subTest(text=text[-100:]), self.assertRaises(ValueError):
                verify(text)

    def test_alignment_span_pool_overflow_and_slot_coverage(self):
        good = self.evidence()
        base = POOL_END - 8 * PAGE
        changes = [(f"start={base:#x}", f"start={base + 1:#x}"),
                   (f"start={base:#x}", "start=0x0"),
                   (f"end={base + 3 * PAGE:#x}", f"end={base + 2 * PAGE:#x}"),
                   (f"end={base + 3 * PAGE:#x}", "end=0x10000000000000000"),
                   (f"foreign={base + PAGE:#x}", f"foreign={base:#x}"),
                   (f"foreign={base + 2 * PAGE:#x}", f"foreign={base + 3 * PAGE:#x}"),
                   (f"foreign={base:#x}", f"foreign={base + 1:#x}")]
        for old, new in changes:
            with self.subTest(old=old, new=new), self.assertRaises(ValueError):
                verify(good.replace(old, new))

    def test_serial_normalization_and_panic_fail_closed(self):
        good = self.evidence()
        self.assertEqual(verify("\x1b[32m" + good.replace("\n", "\r\n") + "\x1b[0m")["status"], "passed")
        for bad in (good + "kernel panic\n", good + "assertion failed: unexpected claim\n",
                    good.replace("test result: ok.", "test result: FAILED."), "Finished dev profile\n"):
            with self.assertRaises(ValueError):
                verify(bad)

    def test_cli_writes_result_only_on_verified_success(self):
        script = Path(__file__).with_name("verify_near_text_collision_runtime.py")
        with tempfile.TemporaryDirectory() as root:
            log = Path(root) / "runtime.log"
            log.write_text(self.evidence())
            good = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertEqual(good.returncode, 0, good.stderr)
            self.assertEqual(json.loads(log.with_name("result.json").read_text())["status"], "passed")
            log.write_text(self.evidence().replace("denied=6", "denied=0"))
            bad = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertNotEqual(bad.returncode, 0)
            stale = json.loads(log.with_name("result.json").read_text())
            self.assertEqual(stale["status"], "not_verified")
            self.assertFalse(stale["module_execution_verified"])


if __name__ == "__main__":
    unittest.main()
