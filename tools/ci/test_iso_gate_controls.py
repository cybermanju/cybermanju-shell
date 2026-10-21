"""Functional tests for the shell gate, without building an ISO or starting QEMU."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class IsoGateControlTests(unittest.TestCase):
    def run_gate(self, phase, status, *, image=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "tools/ci").mkdir(parents=True)
            (root / "asterinas/target/nixos").mkdir(parents=True)
            (root / "bin").mkdir()
            script = root / "tools/ci/test-iso.sh"
            shutil.copyfile(Path(__file__).with_name("test-iso.sh"), script)
            if image:
                (root / "asterinas/target/nixos/asterinas.img").write_bytes(b"disk")

            (root / "bin/timeout").write_text(
                '#!/bin/bash\nshift 3\nexec "$@"\n'
            )
            (root / "bin/python3").write_text(
                '#!/bin/bash\n'
                'if [[ "$1" == *vm_settings.py ]]; then\n'
                '    printf "export ISO_VM_LIVE_TIMEOUT=33m\\n"\n'
                '    printf "export ISO_VM_INSTALL_TIMEOUT=97m\\n"\n'
                '    exit 0\n'
                'else\n'
                '    # Reproduce the raw terminal log: OSC metadata precedes the marker.\n'
                '    printf "\\033]3008;id=systemd\\033\\\\CYBERMANJU_LIVE_INSTALLER_READY\\r\\n" > qemu.log\n'
                '    if [[ "$2" == install ]]; then\n'
                '      printf "Congratulations! Asterinas NixOS has been installed successfully!\\r\\n" >> qemu.log\n'
                '    fi\n'
                '    exit "$HARNESS_STATUS"\n'
                'fi\n'
                'exit 99\n'
            )
            for executable in (root / "bin").iterdir():
                executable.chmod(0o755)
            env = dict(os.environ, PATH=f"{root / 'bin'}:{os.environ['PATH']}",
                       HARNESS_STATUS=str(status))
            return subprocess.run(
                ["bash", str(script), phase], env=env,
                text=True, capture_output=True, check=False,
            )

    def test_live_osc_prefixed_marker_and_successful_harness_pass(self):
        result = self.run_gate("live-usb", 0)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PASS: full ISO live-usb gate.", result.stdout)

    def test_live_marker_does_not_override_failed_harness(self):
        result = self.run_gate("live-usb", 1)
        self.assertNotEqual(result.returncode, 0)

    def test_install_requires_image_even_if_harness_succeeds(self):
        result = self.run_gate("install", 0, image=False)
        self.assertNotEqual(result.returncode, 0)

    def test_install_with_image_and_successful_harness_passes(self):
        result = self.run_gate("install", 0, image=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PASS: full ISO install gate.", result.stdout)


if __name__ == "__main__":
    unittest.main()
