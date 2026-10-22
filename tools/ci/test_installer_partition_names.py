#!/usr/bin/env python3
"""Regression tests for the installer partition-path helper.

The /dev strings exercised here are synthetic pathname examples only; passing
these tests does not qualify any hardware, establish device existence, or imply
that a particular machine can boot from NVMe (or any other device).

Only the extracted shell function is run, in an isolated Bash subprocess. The
installer itself is never executed and no device is opened, probed, partitioned,
or formatted.
"""

from pathlib import Path
import os
import re
import subprocess
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
TEMPLATE = (
    REPO_ROOT
    / "asterinas/distro/aster_nixos_installer/templates/aster-nixos-install"
)


class InstallerPartitionNameTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = TEMPLATE.read_text(encoding="utf-8")
        match = re.search(
            r"(?ms)^set_partition_devices\(\)\s*\{\n.*?^\}", cls.source
        )
        if not match:
            raise AssertionError("set_partition_devices() function not found in installer template")
        cls.function = match.group(0)

    def test_function_sets_expected_paths_without_running_installer(self):
        examples = {
            "/dev/nvme0n1": ("/dev/nvme0n1p1", "/dev/nvme0n1p2"),
            "/dev/nvme12n34": ("/dev/nvme12n34p1", "/dev/nvme12n34p2"),
            "/dev/mmcblk0": ("/dev/mmcblk0p1", "/dev/mmcblk0p2"),
            "/dev/nbd0": ("/dev/nbd0p1", "/dev/nbd0p2"),
            "/dev/loop0": ("/dev/loop0p1", "/dev/loop0p2"),
            "/dev/sda": ("/dev/sda1", "/dev/sda2"),
            "/dev/vda": ("/dev/vda1", "/dev/vda2"),
            "/dev/xvda": ("/dev/xvda1", "/dev/xvda2"),
        }
        for disk, expected in examples.items():
            with self.subTest(disk=disk):
                env = os.environ.copy()
                env["DISK"] = disk
                env.pop("BASH_ENV", None)
                env.pop("ENV", None)
                completed = subprocess.run(
                    ["bash", "--noprofile", "--norc", "-c", self.function + '\nset_partition_devices\nprintf \'%s\\n%s\\n\' "$BOOT_DEVICE" "$ROOT_DEVICE"'],
                    env=env,
                    check=True,
                    capture_output=True,
                    text=True,
                    timeout=5,
                )
                self.assertEqual(completed.stdout.splitlines(), list(expected))
                self.assertEqual(completed.stderr, "")

    def test_single_call_precedes_boot_device_block_check_and_old_branch_is_gone(self):
        call_lines = [
            index
            for index, line in enumerate(self.source.splitlines())
            if re.fullmatch(r"\s*set_partition_devices\s*", line)
        ]
        self.assertEqual(len(call_lines), 1, "expected exactly one function call")

        lines = self.source.splitlines()
        block_check_lines = [
            index
            for index, line in enumerate(lines)
            if re.search(r"\bif\s+\[\s*!\s+-b\s+\"\$\{BOOT_DEVICE\}\"", line)
        ]
        self.assertTrue(block_check_lines, 'expected the `if [ ! -b "${BOOT_DEVICE}" ]` check')
        self.assertLess(call_lines[0], block_check_lines[0])
        self.assertNotIn('if [ "${DISK#/dev/loop}', self.source)

    def test_cleanup_diagnostics_bracket_expected_commands_in_order(self):
        markers = (
            'nixos-install --root ${BUILD_DIR} --no-root-passwd',
            'echo "Pruning obsolete installed system generations..."',
            '--delete-generations old',
            'echo "Collecting unreachable paths from the installed Nix store..."',
            'nix-store --store "${BUILD_DIR}" --gc',
            'echo "Congratulations! Asterinas NixOS has been installed successfully!"',
        )
        positions = [self.source.find(marker) for marker in markers]
        self.assertTrue(all(position >= 0 for position in positions), "missing install/cleanup marker")
        self.assertEqual(positions, sorted(positions), "install and cleanup commands are out of order")


if __name__ == "__main__":
    unittest.main(verbosity=2)
