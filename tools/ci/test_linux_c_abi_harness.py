"""Preflight tests only: these fixtures do not build or execute kernel code."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


MODES = ("--linux-c-abi", "--c-shim-import", "--elf-lease", "--elf-lease-smp", "--permission-plan", "--permission-cleanup")


class CAbiHarnessTests(unittest.TestCase):
    def test_existing_config_is_preserved(self):
        for mode in MODES:
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                scripts = root / "tools/ci"
                scripts.mkdir(parents=True)
                script = scripts / "test_module_staging_runtime.sh"
                shutil.copyfile(Path(__file__).with_name(script.name), script)
                config = root / "asterinas/kernel/libs/linux-module-staging/OSDK.toml"
                config.parent.mkdir(parents=True)
                config.write_text("user configuration\n")
                fixture = root / "fixture.ko"
                fixture.write_bytes(b"not a real ELF: preflight only")
                output = root / "out"
                output.mkdir()
                (output / "result.json").write_text('{"status":"passed"}')
                env = dict(os.environ, CYBERMANJU_STAGING_FIXTURE=str(fixture))
                run = subprocess.run(
                    ["bash", str(script), str(output), mode],
                    env=env, capture_output=True, text=True,
                )
                self.assertEqual(run.returncode, 2)
                self.assertIn("Refusing to overwrite", run.stderr)
                self.assertEqual(config.read_text(), "user configuration\n")
                self.assertEqual(json.loads((output / "result.json").read_text())["status"], "not_verified")
                self.assertFalse((output / "osdk-build.log").exists())

    def test_missing_fixture_invalidates_stale_approval(self):
        for mode in MODES:
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                output = Path(directory)
                (output / "result.json").write_text('{"status":"passed"}')
                env = dict(os.environ)
                env.pop("CYBERMANJU_STAGING_FIXTURE", None)
                run = subprocess.run(
                    ["bash", str(Path(__file__).with_name("test_module_staging_runtime.sh")),
                     str(output), mode],
                    env=env, capture_output=True, text=True,
                )
                self.assertEqual(run.returncode, 2)
                self.assertIn("Missing prepared-module fixture", run.stderr)
                self.assertEqual(json.loads((output / "result.json").read_text())["status"], "not_verified")

    def test_wrapper_forwards_mode_and_fixture_without_default_downgrade(self):
        for mode in MODES:
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                scripts = root / "tools/ci"
                scripts.mkdir(parents=True)
                wrapper = scripts / "test_prepared_module_runtime.sh"
                shutil.copyfile(Path(__file__).with_name(wrapper.name), wrapper)
                (scripts / "test_linux_module_noimports.sh").write_text(
                    '#!/usr/bin/env bash\nset -eu\nmkdir -p "$1"\n'
                    'printf "fixture" > "$1/cybermanju_noimports_fixture.ko"\n'
                )
                (scripts / "test_module_staging_runtime.sh").write_text(
                    '#!/usr/bin/env bash\nset -eu\ncase "$2" in --linux-c-abi|--c-shim-import|--elf-lease|--elf-lease-smp|--permission-plan|--permission-cleanup) ;; *) exit 2 ;; esac\n'
                    '[[ -s "$CYBERMANJU_STAGING_FIXTURE" ]]\n'
                    'printf "%s\\n" "$2" > "$1/forwarded-mode.txt"\n'
                )
                output = root / "out"
                run = subprocess.run(
                    ["bash", str(wrapper), str(output), mode],
                    capture_output=True, text=True,
                )
                self.assertEqual(run.returncode, 0, run.stderr)
                self.assertEqual((output / "forwarded-mode.txt").read_text(), mode + "\n")
                self.assertEqual(json.loads((output / "result.json").read_text())["status"], "not_verified")


if __name__ == "__main__":
    unittest.main()
