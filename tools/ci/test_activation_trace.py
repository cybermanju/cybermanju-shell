#!/usr/bin/env python3
"""Host tests for opt-in activation diagnostics, not evidence of installed boot."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from iso_vm import activation_trace_config_command

ROOT = Path(__file__).resolve().parents[2]
TRACER = ROOT / "asterinas/distro/etc_nixos/modules/trace-activation.sh"


class ActivationTraceTests(unittest.TestCase):
    def trace(self, path):
        return subprocess.run(["bash", str(TRACER), str(path)], capture_output=True, text=True)

    def test_tracing_preserves_output_exit_and_symlink_target(self):
        with tempfile.TemporaryDirectory(prefix="activation space ") as temp:
            root = Path(temp)
            original = root / "immutable activate"
            content = "#!/bin/bash\nprintf 'activation-body\\n'\nexit 17\n"
            original.write_text(content)
            original.chmod(0o555)
            output = root / "activate"
            output.symlink_to(original)
            self.assertEqual(self.trace(output).returncode, 0)
            self.assertFalse(output.is_symlink())
            self.assertEqual(original.read_text(), content)
            self.assertEqual(output.stat().st_mode & 0o777, 0o555)
            result = subprocess.run([str(output)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 17)
            self.assertEqual(result.stdout, "activation-body\n")
            self.assertIn("+CYBERMANJU_ACTIVATION:", result.stderr)
            self.assertNotIn("PASS:", result.stdout + result.stderr)

    def test_shebang_errexit_is_not_relaxed(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "activate"
            path.write_text("#!/bin/bash -e\nfalse\necho MUST_NOT_RUN\n")
            path.chmod(0o755)
            self.assertEqual(self.trace(path).returncode, 0)
            self.assertEqual(path.read_text().splitlines()[0], "#!/bin/bash -e")
            result = subprocess.run([str(path)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn("MUST_NOT_RUN", result.stdout + result.stderr)

    def test_bad_syntax_leaves_original_and_no_temporary_file(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "activate"
            content = "#!/bin/bash\nif then\n"
            path.write_text(content)
            path.chmod(0o755)
            self.assertNotEqual(self.trace(path).returncode, 0)
            self.assertEqual(path.read_text(), content)
            self.assertEqual(list(Path(temp).glob("activate.trace.*")), [])

    def test_missing_or_nonbash_script_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "activate"
            self.assertNotEqual(self.trace(path).returncode, 0)
            path.write_text("#!/bin/sh\necho unchanged\n")
            path.chmod(0o755)
            self.assertNotEqual(self.trace(path).returncode, 0)
            self.assertNotIn("set -x", path.read_text())

    def test_ci_wrapper_embeds_original_and_does_not_overwrite(self):
        with tempfile.TemporaryDirectory(prefix="ci config ") as temp:
            root = Path(temp)
            source = root / "configuration.nix"
            target = root / "ci-configuration.nix"
            content = '{ config, ... }: { imports = [ ./aster_configuration.nix ]; } # last comment'
            source.write_text(content)
            parser = root / "nix-instantiate"
            # Stub checks command plumbing only; real Nix parsing is separate.
            parser.write_text('#!/bin/sh\ntest "$1" = --parse && test -s "$2"\n')
            parser.chmod(0o755)
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}")
            command = activation_trace_config_command(str(source), str(target))
            result = subprocess.run(["bash", "-c", command], env=env, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            wrapper = target.read_text()
            self.assertIn(content + "\n)", wrapper)
            self.assertIn("aster_nixos.trace-activation = true", wrapper)
            self.assertNotIn(str(source), wrapper)
            self.assertEqual(source.read_text(), content)
            result = subprocess.run(["bash", "-c", command], env=env, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(target.read_text(), wrapper)

    def test_wrapper_requires_same_directory_and_distinct_source(self):
        for source, target in [("/one/config.nix", "/two/config.nix"), ("/same/config.nix", "/same/config.nix")]:
            with self.subTest(source=source), self.assertRaises(ValueError):
                activation_trace_config_command(source, target)

    def test_default_off_and_builder_order_are_explicit(self):
        options = (ROOT / "asterinas/distro/aster_nixos_installer/templates/aster_configuration.nix").read_text()
        option = options.split("trace-activation = lib.mkOption {", 1)[1].split("};", 1)[0]
        self.assertIn("default = false;", option)
        self.assertIn("lib.types.bool", option)
        core = (ROOT / "asterinas/distro/etc_nixos/modules/core.nix").read_text()
        self.assertIn("lib.mkAfter (lib.optionalString config.aster_nixos.trace-activation", core)
        self.assertIn('${pkgs.runtimeShell} ${./trace-activation.sh} "$out/activate"', core)
        self.assertNotIn("trace-activation = true", core)

    def test_existing_installed_login_hello_and_shutdown_gates_remain(self):
        script = (ROOT / "tools/ci/test-iso.sh").read_text()
        self.assertIn("NIXOS_TEST_SUITE=hello", script)
        self.assertIn('(( status == 0 )) || exit "$status"', script)
        framework = ROOT / "asterinas/test/nixos/common/framework/src"
        text = "\n".join(p.read_text() for p in framework.rglob("*.rs"))
        self.assertIn("root@asterinas:", text)
        self.assertIn("Hello Asterinas!", (ROOT / "asterinas/test/nixos/tests/hello/src/main.rs").read_text())


if __name__ == "__main__":
    unittest.main()
