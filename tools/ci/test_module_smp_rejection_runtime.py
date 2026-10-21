import unittest
from pathlib import Path
import json
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).parent))
from verify_module_smp_rejection_runtime import verify


GOOD = """running 1 test
CYBERMANJU_MODULE_SMP_REJECTION:topology:PASS cpus=2
CYBERMANJU_MODULE_SMP_REJECTION:denied:PASS cases=3
CYBERMANJU_MODULE_SMP_REJECTION:owner_released:PASS pages=3
test cybermanju_module_mapping_smp_rejection ... ok

test result: ok. 1 passed; 0 failed; 42 filtered out.
"""


class SmpRejectionRuntimeTests(unittest.TestCase):
    def test_valid_serial_evidence_and_exact_json(self):
        self.assertEqual(verify(GOOD), {
            "status": "passed", "scope": "module_mapping_smp_rejection_only",
            "cpus": 2, "rejected_calls": 3, "pages_mapped": False,
            "init_called": False, "smp_mapping_supported": False,
            "production_mapping_available": False,
        })

    def test_rejects_duplicate_marker(self):
        with self.assertRaises(ValueError):
            verify(GOOD.replace("test cybermanju", "CYBERMANJU_MODULE_SMP_REJECTION:topology:PASS cpus=2\ntest cybermanju"))

    def test_rejects_cpu_one(self):
        with self.assertRaises(ValueError):
            verify(GOOD.replace("cpus=2", "cpus=1"))

    def test_rejects_contradictory_or_echoed_marker(self):
        for line in ("CYBERMANJU_MODULE_SMP_REJECTION:denied:PASS cases=4",
                     "echo CYBERMANJU_MODULE_SMP_REJECTION:denied:PASS cases=3"):
            with self.subTest(line=line), self.assertRaises(ValueError):
                verify(GOOD + line + "\n")

    def test_rejects_bad_summary_and_panic(self):
        for bad in (GOOD.replace("0 failed", "1 failed"), GOOD + "thread panicked\n",
                    GOOD.replace("test result: ok. 1 passed; 0 failed; 42 filtered out.", "test result: ok. 2 passed; 0 failed; 41 filtered out.")):
            with self.subTest(), self.assertRaises(ValueError):
                verify(bad)

    def test_ansi_cr_normalization(self):
        self.assertEqual(verify(GOOD), verify(("\x1b[32m" + GOOD + "\x1b[0m").replace("\n", "\r\n")))

    def test_each_marker_required_and_unknown_rejected(self):
        for marker in GOOD.splitlines():
            if marker.startswith("CYBERMANJU_"):
                with self.subTest(marker=marker), self.assertRaises(ValueError):
                    verify(GOOD.replace(marker + "\n", ""))
        for suffix in ("CYBERMANJU_MODULE_SMP_REJECTION:other:PASS", "FAIL", "runner failed"):
            with self.subTest(suffix=suffix), self.assertRaises(ValueError):
                verify(GOOD + suffix + "\n")

    def test_summary_must_be_unique_complete_and_standalone(self):
        summary = "test result: ok. 1 passed; 0 failed; 42 filtered out."
        for bad in (GOOD + summary, GOOD.replace(summary, "echo " + summary), GOOD.replace(summary, summary + " trailing")):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                verify(bad)

    def test_preflight_preserves_existing_config_and_invalidates_old_success(self):
        runner = Path(__file__).with_name("test_module_staging_runtime.sh")
        for symlink in (False, True):
            with self.subTest(symlink=symlink), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                script = root / "tools/ci/test_module_staging_runtime.sh"
                script.parent.mkdir(parents=True)
                shutil.copyfile(runner, script)
                config = root / "asterinas/ostd/OSDK.toml"
                config.parent.mkdir(parents=True)
                target = config.with_name("preserve.toml") if symlink else config
                target.write_text("# configuração própria\n")
                if symlink:
                    config.symlink_to(target)
                out = root / "out"
                out.mkdir()
                (out / "result.json").write_text('{"status":"passed"}\n')
                result = subprocess.run(["bash", str(script), str(out), "--smp-rejection"], capture_output=True, text=True)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn("Refusing to overwrite", result.stderr)
                self.assertEqual(config.read_text(), "# configuração própria\n")
                self.assertEqual(config.is_symlink(), symlink)
                self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")
                self.assertFalse((out / "runtime.log").exists())


if __name__ == "__main__":
    unittest.main()
