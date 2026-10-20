"""Exercise the CI gate without building an ISO or starting QEMU."""
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest


class IsoGateTests(unittest.TestCase):
    def run_gate(self, phase, scenario, *, disk=False, stale=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "tools/ci").mkdir(parents=True)
            (root / "asterinas/target/nixos").mkdir(parents=True)
            (root / "bin").mkdir()
            script = root / "tools/ci/test-iso.sh"
            shutil.copyfile(Path(__file__).with_name("test-iso.sh"), script)
            shutil.copyfile(Path(__file__).with_name("vm_settings.py"), script.with_name("vm_settings.py"))
            if disk:
                (root / "asterinas/target/nixos/asterinas.img").write_text("disk")
            if stale:
                (root / "asterinas/qemu.log").write_text(
                    "Congratulations! Asterinas NixOS has been installed successfully!\n"
                )
            (root / "bin/timeout").write_text(
                '#!/bin/bash\n'
                'if [[ "$SCENARIO" == timeout ]]; then exit 124; fi\n'
                'printf "MOCK_TIMEOUT=%s\\n" "$3"\n'
                'shift 3\nexec "$@"\n'
            )
            (root / "bin/make").write_text(
                '#!/bin/bash\n'
                'printf "MOCK_ARGS=%s\\n" "$*"\n'
                'printf "MOCK_ENABLE_KVM=%s\\n" "$ENABLE_KVM"\n'
                'if [[ "$SCENARIO" == fail ]]; then exit 7; fi\n'
                'if [[ "$SCENARIO" == success ]]; then\n'
                '  printf "Congratulations! Asterinas NixOS has been installed successfully!\\r\\n" > qemu.log\n'
                '  printf "CYBERMANJU_LIVE_INSTALLER_READY\\r\\n" >> qemu.log\n'
                '  printf disk > target/nixos/asterinas.img\n'
                'fi\n'
                'exit 0\n'
            )
            (root / "bin/python3").write_text(
                '#!/bin/bash\n'
                'if [[ "$1" == */vm_settings.py ]]; then\n'
                '  if [[ "$SCENARIO" == settings-fail ]]; then exit 6; fi\n'
                f'  exec {shlex.quote(sys.executable)} "$@"\n'
                'fi\n'
                f'exec {shlex.quote(str(root / "bin/make"))} "$@"\n'
            )
            for executable in (root / "bin").iterdir():
                executable.chmod(0o755)
            env = dict(os.environ, PATH=f"{root / 'bin'}:{os.environ['PATH']}",
                       SCENARIO=scenario, ENABLE_KVM="0")
            result = subprocess.run(
                ["bash", str(script), phase], env=env,
                text=True, capture_output=True, check=False,
            )
            return result

    def test_successful_install_accepts_serial_crlf(self):
        result = self.run_gate("install", "success")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_zero_exit_without_completion_marker_fails(self):
        self.assertNotEqual(self.run_gate("install", "empty").returncode, 0)

    def test_stale_completion_marker_cannot_pass(self):
        self.assertNotEqual(self.run_gate("install", "empty", stale=True, disk=True).returncode, 0)

    def test_nonzero_install_is_propagated(self):
        self.assertEqual(self.run_gate("install", "fail").returncode, 7)

    def test_install_timeout_is_propagated(self):
        self.assertEqual(self.run_gate("install", "timeout").returncode, 124)

    def test_installed_system_requires_disk(self):
        self.assertNotEqual(self.run_gate("boot", "empty").returncode, 0)

    def test_installed_system_harness_success(self):
        self.assertEqual(self.run_gate("boot", "empty", disk=True).returncode, 0)

    def test_installed_system_harness_failure(self):
        self.assertEqual(self.run_gate("boot", "fail", disk=True).returncode, 7)

    def test_installed_system_timeout(self):
        self.assertEqual(self.run_gate("boot", "timeout", disk=True).returncode, 124)

    def test_live_usb_success(self):
        self.assertEqual(self.run_gate("live-usb", "success").returncode, 0)

    def test_live_usb_missing_marker_fails(self):
        self.assertNotEqual(self.run_gate("live-usb", "empty").returncode, 0)

    def test_live_usb_failure_is_propagated(self):
        self.assertEqual(self.run_gate("live-usb", "fail").returncode, 7)

    def test_invalid_phase_is_rejected(self):
        self.assertEqual(self.run_gate("invalid", "empty").returncode, 2)

    def test_selector_failure_is_not_swallowed(self):
        self.assertEqual(self.run_gate("install", "settings-fail").returncode, 6)

    def test_live_usb_has_tcg_outer_timeout(self):
        result = self.run_gate("live-usb", "success")
        self.assertIn("MOCK_TIMEOUT=33m", result.stdout)

    def test_installer_has_tcg_outer_timeout(self):
        result = self.run_gate("install", "success")
        self.assertIn("MOCK_TIMEOUT=97m", result.stdout)

    def test_installed_harness_receives_tcg_and_extended_deadline(self):
        result = self.run_gate("boot", "empty", disk=True)
        self.assertIn("MOCK_TIMEOUT=55m", result.stdout)
        self.assertIn("ENABLE_KVM=0", result.stdout)
        self.assertIn("NIXOS_TEST_TIMEOUT=40min", result.stdout)


if __name__ == "__main__":
    unittest.main()
