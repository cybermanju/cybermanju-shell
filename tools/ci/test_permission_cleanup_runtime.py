import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("verify_permission_cleanup_runtime.py")
spec = importlib.util.spec_from_file_location("cleanup", SCRIPT)
cleanup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cleanup)


def log(pages=2):
    span = pages * 4096
    total = pages * 3
    return "\n".join([
        "CYBERMANJU_PERMISSION_CLEANUP:begin:PASS cpus=1 denied_cases=2",
        f"CYBERMANJU_PERMISSION_CLEANUP:denied:PASS id=0 budget=0 span={span} pages={pages} acquired={pages} dropped={pages} mapper_entries=0 writes=1 reclaimed=true",
        f"CYBERMANJU_PERMISSION_CLEANUP:denied:PASS id=1 budget={pages-1} span={span} pages={pages} acquired={pages} dropped={pages} mapper_entries=0 writes=2 reclaimed=true",
        f"CYBERMANJU_PERMISSION_CLEANUP:success:PASS budget={pages} span={span} pages={pages} acquired={pages} dropped={pages} mapper_entries=1 writes=3 bytes={span} nonx_pages={pages} retained=true reclaimed=true",
        f"CYBERMANJU_PERMISSION_CLEANUP:complete:PASS denied=2 public_denied=2 public_reclaims=2 recovered=1 mapper_entries=1 acquired={total} dropped={total} reclaims=5 injected_heap_oom=false module_execution=false physical_wx=false smp_mapping=false",
        "test result: ok. 1 passed; 0 failed; 0 filtered out.",
    ])


class CleanupVerifierTests(unittest.TestCase):
    def test_valid_log_with_ansi(self):
        result = cleanup.verify("\x1b[32m" + log() + "\x1b[0m")
        self.assertEqual(result["status"], "passed")
        self.assertTrue(result["preflight_cleanup_verified"])

    def test_public_denials_and_reclaims_are_required(self):
        for old,new in [("public_denied=2","public_denied=1"),("public_reclaims=2","public_reclaims=1"),("reclaims=5","reclaims=3")]:
            with self.subTest(old=old), self.assertRaises(ValueError): cleanup.verify(log().replace(old,new))

    def test_missing_marker_rejected(self):
        with self.assertRaises(ValueError): cleanup.verify(log().replace("id=0 ", "id=9 "))

    def test_marker_order_rejected(self):
        lines = log().splitlines()
        lines[1], lines[2] = lines[2], lines[1]
        with self.assertRaises(ValueError): cleanup.verify("\n".join(lines))

    def test_foreign_duplicate_and_panic_rejected(self):
        for altered in (log() + "\nCYBERMANJU_FOREIGN:PASS", log().replace("id=0", "id=0") + "\n" + log().splitlines()[1], log() + "\npanicked"):
            with self.subTest(altered=altered[-40:]), self.assertRaises(ValueError): cleanup.verify(altered)

    def test_summary_and_completion_totals_are_strict(self):
        with self.assertRaises(ValueError): cleanup.verify(log().replace("1 passed; 0 failed", "2 passed; 0 failed"))
        with self.assertRaises(ValueError): cleanup.verify(log().replace("acquired=6 dropped=6", "acquired=30 dropped=30"))

    def test_budget_mapping_drop_and_count_mutations(self):
        variants = [log().replace("id=0 budget=0", "id=0 budget=1"),
                    log().replace("mapper_entries=0 writes=1", "mapper_entries=1 writes=1"),
                    log().replace("acquired=2 dropped=2", "acquired=2 dropped=1", 1),
                    log().replace("acquired=6 dropped=6", "acquired=5 dropped=6")]
        for text in variants:
            with self.subTest(text=text), self.assertRaises(ValueError): cleanup.verify(text)

    def test_success_byte_nonx_and_refusal_mutations(self):
        for old, new in (("bytes=8192", "bytes=4096"), ("nonx_pages=2", "nonx_pages=1"),
                         ("physical_wx=false", "physical_wx=true"),
                         ("injected_heap_oom=false", "injected_heap_oom=true"),
                         ("module_execution=false", "module_execution=true"),
                         ("smp_mapping=false", "smp_mapping=true")):
            with self.subTest(change=(old, new)), self.assertRaises(ValueError): cleanup.verify(log().replace(old, new))

    def test_alternate_geometry_computes_totals_not_literal_thirty(self):
        result = cleanup.verify(log(12))
        self.assertEqual(result["acquired_handles"], 36)
        self.assertEqual(result["dropped_handles"], 36)
        self.assertEqual(result["compared_bytes"], 12 * 4096)

    def test_cli_clears_old_passed_result_before_missing_log_error(self):
        with tempfile.TemporaryDirectory() as directory:
            missing = Path(directory) / "missing.log"
            output = Path(directory) / "result.json"
            output.write_text('{"status":"passed"}\n')
            proc = subprocess.run([sys.executable, str(SCRIPT), str(missing)], capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0)
            self.assertEqual(json.loads(output.read_text()), {"status": "not_verified"})

    def test_cli_invalid_log_remains_not_verified(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "runtime.log"
            path.write_text(log().replace("budget=0", "budget=5", 1))
            proc = subprocess.run([sys.executable, str(SCRIPT), str(path)], capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0)
            self.assertEqual(json.loads((Path(directory) / "result.json").read_text())["status"], "not_verified")


if __name__ == "__main__":
    unittest.main()
