import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from verify_module_staging_runtime import TESTS, verify


class RuntimeEvidenceTests(unittest.TestCase):
    def evidence(self):
        return "\n".join(f"CYBERMANJU_STAGING_RUNTIME:{name}:PASS" for name in TESTS) + "\ntest result: ok. 1 passed; 0 failed; 301 filtered out.\n"

    def test_complete_runtime_evidence(self):
        result = verify(self.evidence())
        self.assertEqual(result["runtime_tests"], 1)
        self.assertFalse(result["linux_modules_loaded"])

    def test_serial_ansi_and_control_prefixes(self):
        result = verify("\x03\x1b[32m" + self.evidence().replace("ok.", "\x1b[32mok\x1b[0m."))
        self.assertEqual(result["status"], "passed")

    def test_every_missing_marker_is_rejected(self):
        for name in TESTS:
            with self.subTest(name=name), self.assertRaises(ValueError):
                verify(self.evidence().replace(f"CYBERMANJU_STAGING_RUNTIME:{name}:PASS", ""))

    def test_duplicate_marker_is_rejected(self):
        with self.assertRaises(ValueError):
            verify(self.evidence() + f"CYBERMANJU_STAGING_RUNTIME:{TESTS[0]}:PASS")

    def test_compilation_or_empty_filter_is_not_runtime_success(self):
        for text in ["Finished dev profile", self.evidence().replace("1 passed", "0 passed"), self.evidence().replace("test result: ok.", "test result: FAILED.")]:
            with self.subTest(text=text), self.assertRaises(ValueError):
                verify(text)

    def test_other_runner_failures_are_rejected(self):
        with self.assertRaises(ValueError):
            verify(self.evidence() + "test result: FAILED. 1 passed; 1 failed; 0 filtered out.")

    def test_existing_config_is_preserved_and_stale_success_invalidated(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "tools/ci"
            scripts.mkdir(parents=True)
            script = scripts / "test_module_staging_runtime.sh"
            shutil.copyfile(Path(__file__).with_name(script.name), script)
            config = root / "asterinas/ostd/OSDK.toml"
            config.parent.mkdir(parents=True)
            config.write_text("user configuration must survive\n")
            out = root / "artifacts/runtime"
            out.mkdir(parents=True)
            (out / "result.json").write_text('{"status":"passed"}')
            result = subprocess.run(["bash", str(script), str(out)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(config.read_text(), "user configuration must survive\n")
            self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")
            self.assertFalse((out / "osdk-build.log").exists())


if __name__ == "__main__":
    unittest.main()
