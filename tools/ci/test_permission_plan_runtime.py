import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).parent))
from verify_permission_plan_runtime import BEGIN, COMPLETE, KINDS, verify


class PermissionPlanRuntimeTests(unittest.TestCase):
    def evidence(self):
        # General valid geometry, with positive intended permission classes.
        case_lines = []
        for kind in KINDS:
            case_lines.append(
                f"CYBERMANJU_PERMISSION_PLAN:case:PASS kind={kind} span=40960 pages=10 "
                "ro=2 rw=3 rx=2 gap=3 actual_nonx=10 bytes=40960 retained=true reclaimed=true"
            )
        return "\r\n".join([BEGIN, *case_lines, COMPLETE,
                            "test result: ok. 1 passed; 0 failed; 42 filtered out.", ""])

    def test_valid_ansi_evidence_reports_metadata_only_scope(self):
        colored = "\x1b[32m" + self.evidence().replace("\r\n", "\x1b[0m\r\n\x1b[32m") + "\x1b[0m"
        result = verify(colored)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["scope"], "experimental_elf_page_permission_preflight_only")
        self.assertEqual((result["cpus"], result["runtime_tests"], result["planned_cases"]), (1, 1, 4))
        self.assertEqual(result["kinds"], KINDS)
        for field, value in {"planned_pages": 40, "intended_ro_pages": 8,
                             "intended_rw_pages": 12, "intended_rx_pages": 8,
                             "intended_gap_pages": 12, "actual_nonx_pages": 40,
                             "compared_bytes": 163840, "conflicts_denied": 3,
                             "budget_rejections": 4, "reclaimed_reservations": 4}.items():
            self.assertEqual(result[field], value)
        self.assertTrue(result["page_permission_preflight_verified"])
        for flag in ("permissions_applied", "rx_mapping_verified", "physical_wx_verified",
                     "linux_modules_executed", "linux_abi_verified", "smp_mapping_verified",
                     "production_mapping_ready", "tlb_shootdown_verified"):
            self.assertFalse(result[flag])

    def test_protocol_markers_are_exact_standalone_unique_and_ordered(self):
        good = self.evidence()
        marker_lines = [line for line in good.splitlines() if "CYBERMANJU_" in line]
        swapped = good.replace(marker_lines[1] + "\r\n" + marker_lines[2],
                               marker_lines[2] + "\r\n" + marker_lines[1])
        variants = [good.replace(marker_lines[0] + "\r\n", ""),
                    good + marker_lines[0] + "\n",
                    good.replace(marker_lines[1], "echo " + marker_lines[1]),
                    good.replace(marker_lines[1], "OTHER:" + marker_lines[1]),
                    swapped,
                    good + "CYBERMANJU_FOREIGN:pass\n"]
        for text in variants:
            with self.subTest(text=text[-100:]), self.assertRaises(ValueError):
                verify(text)

    def test_all_case_fields_and_capability_claims_fail_closed(self):
        good = self.evidence()
        mutations = [
            ("cpus=1", "cpus=2"), ("kinds=4", "kinds=3"),
            ("kind=1", "kind=2"), ("span=40960", "span=0"),
            ("span=40960", "span=4097"), ("span=40960", "span=4198401"),
            ("pages=10", "pages=9"), ("ro=2", "ro=0"), ("rw=3", "rw=0"),
            ("rx=2", "rx=0"), ("gap=3", "gap=-1"), ("gap=3", "gap=2"),
            ("actual_nonx=10", "actual_nonx=9"), ("bytes=40960", "bytes=40959"),
            ("retained=true", "retained=false"), ("reclaimed=true", "reclaimed=false"),
            ("conflicts_denied=3", "conflicts_denied=2"),
            ("budgets_denied=4", "budgets_denied=3"), ("reclaims=4", "reclaims=3"),
            ("metadata_only=true", "metadata_only=false"), ("rx_applied=false", "rx_applied=true"),
            ("module_execution=false", "module_execution=true"),
            ("physical_wx=false", "physical_wx=true"), ("smp_mapping=false", "smp_mapping=true"),
        ]
        for old, new in mutations:
            with self.subTest(old=old, new=new), self.assertRaises(ValueError):
                verify(good.replace(old, new, 1))
        for extra in ("\npanicked at runtime\n", "\ntest result: FAILED\n",
                      "\ntest result: ok. 0 passed; 1 failed; 0 filtered out.\n",
                      "\nassertion failed: condition\n"):
            with self.subTest(extra=extra), self.assertRaises(ValueError):
                verify(good + extra)

    def test_byte_span_and_class_sum_must_match(self):
        good = self.evidence()
        for old, new in (("span=40960", "span=45056"), ("ro=2 rw=3 rx=2 gap=3", "ro=2 rw=3 rx=2 gap=4"),
                         ("bytes=40960", "bytes=45056"), ("actual_nonx=10", "actual_nonx=11")):
            with self.subTest(old=old), self.assertRaises(ValueError):
                verify(good.replace(old, new, 1))

    def test_summary_must_be_unique_successful_and_after_complete(self):
        good = self.evidence()
        summaries = ["test result: ok. 1 passed; 0 failed; 1 filtered out."]
        invalid = [good.replace("42 filtered out", "1 failed"),
                   good.replace(COMPLETE + "\r\n", summaries[0] + "\r\n" + COMPLETE + "\r\n"),
                   good + summaries[0] + "\n"]
        for text in invalid:
            with self.subTest(text=text[-120:]), self.assertRaises(ValueError):
                verify(text)

    def test_cli_invalidates_stale_success_before_missing_or_invalid_log_read(self):
        script = Path(__file__).with_name("verify_permission_plan_runtime.py")
        with tempfile.TemporaryDirectory() as root:
            missing = Path(root) / "absent.log"
            result_path = missing.with_name("result.json")
            result_path.write_text('{"status":"passed"}\n')
            proc = subprocess.run([sys.executable, str(script), str(missing)], capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0)
            self.assertEqual(json.loads(result_path.read_text()), {"status": "not_verified"})
            missing.write_text(self.evidence().replace("pages=10", "pages=0", 1))
            result_path.write_text('{"status":"passed"}\n')
            proc = subprocess.run([sys.executable, str(script), str(missing)], capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0)
            self.assertEqual(json.loads(result_path.read_text()), {"status": "not_verified"})


if __name__ == "__main__":
    unittest.main()
