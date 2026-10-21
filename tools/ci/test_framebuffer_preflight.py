#!/usr/bin/env python3
"""Runner preflight only: no compiler, VM or GPU is started by these tests."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("test_framebuffer_runtime.sh")


class FramebufferPreflight(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.script = self.root / "tools/ci/test_framebuffer_runtime.sh"
        self.script.parent.mkdir(parents=True)
        shutil.copyfile(SCRIPT, self.script)
        self.out = self.root / "evidence"
        self.out.mkdir()
        (self.out / "result.json").write_text('{"status":"passed"}\n')

    def run_runner(self, *args):
        result = subprocess.run(["bash", str(self.script), str(self.out), *args],
                                capture_output=True, text=True, timeout=5)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(json.loads((self.out / "result.json").read_text()),
                         {"status": "not_verified"})
        return result

    def test_unknown_mode_invalidates_previous_success(self):
        result = self.run_runner("--kind", "unknown")
        self.assertIn("expected --kind", result.stderr)

    def test_positional_kind_is_not_silently_accepted(self):
        result = self.run_runner("hardware")
        self.assertIn("expected OUT --kind", result.stderr)

    def test_extra_arguments_are_rejected(self):
        self.run_runner("--kind", "hardware", "--unexpected")

    def test_existing_config_preserved_for_both_modes(self):
        for kind, location in (("contract", "ostd"),
                               ("hardware", "kernel/core/comps/framebuffer")):
            with self.subTest(kind=kind):
                config = self.root / "asterinas" / location / "OSDK.toml"
                config.parent.mkdir(parents=True, exist_ok=True)
                config.write_bytes(b"# owner configuration\n")
                result = self.run_runner("--kind", kind)
                self.assertIn("Refusing to overwrite", result.stderr)
                self.assertEqual(config.read_bytes(), b"# owner configuration\n")

    def test_dangling_config_symlink_preserved(self):
        config = self.root / "asterinas/kernel/core/comps/framebuffer/OSDK.toml"
        config.parent.mkdir(parents=True)
        config.symlink_to("owner-missing-config")
        result = self.run_runner("--kind", "hardware")
        self.assertIn("Refusing to overwrite", result.stderr)
        self.assertTrue(config.is_symlink())
        self.assertEqual(config.readlink(), Path("owner-missing-config"))


if __name__ == "__main__":
    unittest.main()
