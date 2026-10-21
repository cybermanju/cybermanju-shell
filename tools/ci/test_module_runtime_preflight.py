import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SOURCE_SCRIPT = Path(__file__).with_name("test_module_staging_runtime.sh")


class RuntimePreflightTests(unittest.TestCase):
    def make_root(self, directory):
        root = Path(directory)
        scripts = root / "tools/ci"
        scripts.mkdir(parents=True)
        shutil.copyfile(SOURCE_SCRIPT, scripts / SOURCE_SCRIPT.name)
        return root, scripts / SOURCE_SCRIPT.name

    def run_preflight(self, root, script, args=(), env=None):
        out = root / "artifacts/runtime"
        out.mkdir(parents=True, exist_ok=True)
        (out / "result.json").write_text('{"status":"passed"}\n')
        result = subprocess.run(
            ["bash", str(script), str(out), *args],
            capture_output=True,
            text=True,
            env=env,
        )
        return result, out

    def assert_stale_invalidated(self, out):
        self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")

    def test_unknown_mode_rejected_and_stale_result_invalidated(self):
        with tempfile.TemporaryDirectory() as directory:
            root, script = self.make_root(directory)
            result, out = self.run_preflight(root, script, ["--unknown"])
            self.assertEqual(result.returncode, 2)
            self.assertIn("Unknown runtime mode", result.stderr)
            self.assert_stale_invalidated(out)

    def test_prepared_module_without_fixture_rejected_and_stale_invalidated(self):
        with tempfile.TemporaryDirectory() as directory:
            root, script = self.make_root(directory)
            env = os.environ.copy()
            env.pop("CYBERMANJU_STAGING_FIXTURE", None)
            result, out = self.run_preflight(root, script, ["--prepared-module"], env)
            self.assertEqual(result.returncode, 2)
            self.assertIn("Missing prepared-module fixture", result.stderr)
            self.assert_stale_invalidated(out)

    def test_broken_osdk_config_symlink_is_preserved_and_rejected_before_build(self):
        with tempfile.TemporaryDirectory() as directory:
            root, script = self.make_root(directory)
            config = root / "asterinas/ostd/OSDK.toml"
            config.parent.mkdir(parents=True)
            config.symlink_to(config.parent / "missing-target")
            result, out = self.run_preflight(root, script)
            self.assertEqual(result.returncode, 2)
            self.assertTrue(config.is_symlink())
            self.assertFalse(config.resolve().exists())
            self.assertIn("Refusing to overwrite an existing OSDK.toml", result.stderr)
            self.assert_stale_invalidated(out)
            self.assertFalse((out / "osdk-build.log").exists())

    def test_noclobber_race_preserves_foreign_config_without_runtime(self):
        with tempfile.TemporaryDirectory() as directory:
            root, script = self.make_root(directory)
            config = root / "asterinas/ostd/OSDK.toml"
            config.parent.mkdir(parents=True)
            self.assertFalse(config.exists())
            osdk_dir = root / "asterinas/osdk"
            osdk_dir.mkdir(parents=True)
            fakebin = root / "fakebin"
            fakebin.mkdir()
            marker = root / "fake-cargo-ran"

            cargo = fakebin / "cargo"
            cargo.write_text(
                "#!/bin/sh\n"
                f"touch {str(marker)!r}\n"
                f"printf '%s\\n' 'foreign configuration' > {str(config)!r}\n"
                "exit 0\n"
            )
            cargo.chmod(0o755)
            rustc = fakebin / "rustc"
            rustc.write_text(
                "#!/bin/sh\n"
                "case \"$1\" in\n"
                f"  --print) printf '%s\\n' {str(root / 'fake-sysroot')!r} ;;\n"
                "  -vV) printf '%s\\n' 'rustc 1.0' 'host: fake-host' ;;\n"
                "  *) exit 1 ;;\n"
                "esac\n"
            )
            rustc.chmod(0o755)
            sysroot = root / "fake-sysroot/lib/rustlib/fake-host/bin"
            sysroot.mkdir(parents=True)
            objcopy = sysroot / "llvm-objcopy"
            objcopy.write_text("#!/bin/sh\nexit 0\n")
            objcopy.chmod(0o755)
            for tool in ("qemu-system-x86_64", "grub-mkrescue", "xorriso"):
                stub = fakebin / tool
                stub.write_text("#!/bin/sh\nexit 0\n")
                stub.chmod(0o755)

            env = os.environ.copy()
            env["PATH"] = f"{fakebin}:/usr/bin:/bin"
            result, out = self.run_preflight(root, script, env=env)
            self.assertTrue(marker.exists(), "fake cargo build was not executed")
            self.assertEqual(result.returncode, 2)
            self.assertIn("cannot overwrite", result.stderr.lower())
            self.assertEqual(config.read_text(), "foreign configuration\n")
            self.assert_stale_invalidated(out)
            self.assertFalse((out / "runtime.log").exists(), "runtime/VM must not be launched")


if __name__ == "__main__":
    unittest.main()
