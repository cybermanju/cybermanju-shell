import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("verify_c_shim_import_runtime", HERE / "verify_c_shim_import_runtime.py")
verify_module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verify_module)


def fixture(span=40960):
    rows = [
        "CYBERMANJU_C_SHIM_IMPORT:begin:PASS cpus=1 kinds=4",
        *(f"CYBERMANJU_C_SHIM_IMPORT:case:PASS kind={kind} span={span} bytes={span} relocation_bytes={width} retained=true reclaimed=true" for kind, width in [(1, 8), (2, 4), (4, 4), (11, 4)]),
        "CYBERMANJU_C_SHIM_IMPORT:policy:PASS default_denied=1 unknown_denied=3 rust_denied=1 current_bytes_denied=1 budgets_denied=2 backend_entries=0 reclaimed=8",
        f"CYBERMANJU_C_SHIM_IMPORT:faults:PASS cases=2 entries=2 acquired={span // 4096 + 2} dropped={span // 4096 + 2} writes=1 reclaimed=2",
        "CYBERMANJU_C_SHIM_IMPORT:complete:PASS elf_imports_prepared=true init_called=false modules_executed=0 production_mapping=false wx=false smp_mapping=false",
        "test result: ok. 1 passed; 0 failed; 8 filtered out.",
    ]
    return "\n".join(rows) + "\n"


class VerifyCShimImportRuntimeTests(unittest.TestCase):
    def rejects(self, text):
        with self.assertRaises(ValueError):
            verify_module.verify(text)

    def test_positive_ansi_cr_and_alternate_spans(self):
        for span in (4096, 65536, 4 * 1024 * 1024):
            text = fixture(span).replace("CYBERMANJU_C_SHIM_IMPORT:begin", "\x1b[32mCYBERMANJU_C_SHIM_IMPORT:begin\x1b[0m").replace("\n", "\r\n")
            result = verify_module.verify(text)
            self.assertEqual(result["span_bytes"], span)
            self.assertEqual(result["compared_bytes"], span * 4)

    def test_missing_duplicate_reordered_and_foreign_markers(self):
        base = fixture()
        rows = base.splitlines()
        self.rejects("\n".join(rows[1:]) + "\n")
        self.rejects(base.replace("test result", "CYBERMANJU_C_SHIM_IMPORT:extra test result"))
        reordered = rows[:]
        reordered[1], reordered[2] = reordered[2], reordered[1]
        self.rejects("\n".join(reordered))
        self.rejects(base.replace("CYBERMANJU_C_SHIM_IMPORT:policy", "CYBERMANJU_OTHER:policy"))

    def test_all_fixed_counts_and_flags_are_exact(self):
        base = fixture()
        variants = [
            ("cpus=1", "cpus=2"), ("kinds=4", "kinds=5"),
            ("default_denied=1", "default_denied=2"), ("unknown_denied=3", "unknown_denied=2"),
            ("rust_denied=1", "rust_denied=2"), ("current_bytes_denied=1", "current_bytes_denied=2"),
            ("budgets_denied=2", "budgets_denied=3"), ("backend_entries=0", "backend_entries=1"),
            ("reclaimed=8", "reclaimed=7"), ("entries=2", "entries=3"),
            ("writes=1", "writes=2"), ("elf_imports_prepared=true", "elf_imports_prepared=false"),
            ("init_called=false", "init_called=true"), ("modules_executed=0", "modules_executed=1"),
            ("production_mapping=false", "production_mapping=true"), ("wx=false", "wx=true"),
            ("smp_mapping=false", "smp_mapping=true"), ("retained=true", "retained=false"),
            ("reclaimed=true", "reclaimed=false"),
        ]
        for old, new in variants:
            with self.subTest(old=old):
                self.rejects(base.replace(old, new, 1))

    def test_case_geometry_bytes_span_and_relocation_fields(self):
        for old, new in [("bytes=40960", "bytes=40959"), ("span=40960", "span=40961"),
                         ("span=40960", "span=0"), ("span=40960", "span=4198400"),
                         ("span=40960", "span=18446744073709551616"), ("relocation_bytes=8", "relocation_bytes=4")]:
            self.rejects(fixture().replace(old, new, 1))
        self.rejects(fixture(4096).replace("span=4096 bytes=4096", "span=8192 bytes=8192", 1))

    def test_fault_handle_counts_must_match_page_count(self):
        for key, val in [("acquired=12", "acquired=11"), ("dropped=12", "dropped=13")]:
            self.rejects(fixture().replace(key, val))

    def test_summary_malformed_duplicate_prefixed_and_before_complete(self):
        base = fixture()
        self.rejects(base.replace("test result: ok.", "test result: FAILED"))
        self.rejects(base.replace("test result: ok.", "x test result: ok."))
        self.rejects(base + "test result: ok. 1 passed; 0 failed; 8 filtered out.\n")
        rows = base.splitlines()
        rows[-1], rows[-2] = rows[-2], rows[-1]
        self.rejects("\n".join(rows))

    def test_panic_and_assertion_failure_rejected(self):
        self.rejects(fixture() + "thread panic occurred\n")
        self.rejects(fixture() + "assertion failed: no\n")

    def test_cli_success_and_stale_output_rejected_before_read(self):
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "runtime.log"
            log.write_text(fixture())
            proc = subprocess.run([sys.executable, str(HERE / "verify_c_shim_import_runtime.py"), str(log)], capture_output=True, text=True)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            result = json.loads((Path(tmp) / "result.json").read_text())
            self.assertEqual(result["status"], "passed")
            self.assertNotIn("production_linux_import_admission", result)
            log.write_text("bad\n")
            proc = subprocess.run([sys.executable, str(HERE / "verify_c_shim_import_runtime.py"), str(log)], capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0)
            self.assertEqual(json.loads((Path(tmp) / "result.json").read_text())["status"], "not_verified")

    def test_cli_missing_file_leaves_not_verified(self):
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / "absent.log"
            proc = subprocess.run([sys.executable, str(HERE / "verify_c_shim_import_runtime.py"), str(log)], capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0)
            self.assertEqual(json.loads((Path(tmp) / "result.json").read_text())["status"], "not_verified")


if __name__ == "__main__":
    unittest.main()
