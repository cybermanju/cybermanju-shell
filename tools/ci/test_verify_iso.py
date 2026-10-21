"""Regression tests for ISO GRUB payload validation; no ISO build required."""
from pathlib import Path
import subprocess
import tempfile
import unittest


class GrubPayloadTests(unittest.TestCase):
    def run_check(self, references, payloads):
        source = Path(__file__).with_name("verify-iso.sh").read_text()
        start = source.index("while read -r path; do")
        end = source.index("\ntest -s \"$work/iso/nix-store.squashfs\"", start)
        check = source[start:end]
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            config = work / "iso/EFI/BOOT/grub.cfg"
            config.parent.mkdir(parents=True)
            config.write_text(references)
            for payload in payloads:
                target = work / "iso" / payload
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text("payload")
            return subprocess.run(
                ["bash", "-c", 'set -euo pipefail; work=$1; ' + check, "check", directory],
                text=True, capture_output=True, check=False,
            )

    def test_absolute_and_current_root_payloads(self):
        result = self.run_check(
            "linux /boot/kernel console=hvc0\ninitrd /boot/initrd\n"
            "linux ($root)/boot/memtest.bin\n",
            ["boot/kernel", "boot/initrd", "boot/memtest.bin"],
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_current_root_payload_still_fails(self):
        result = self.run_check("linux ($root)/boot/memtest.bin\n", [])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing GRUB boot payload: /boot/memtest.bin", result.stderr)

    def test_missing_absolute_payload_still_fails(self):
        self.assertNotEqual(self.run_check("initrd /boot/missing\n", []).returncode, 0)

    def test_unknown_device_is_not_silently_stripped(self):
        result = self.run_check("linux (hd9)/boot/kernel\n", ["boot/kernel"])
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
