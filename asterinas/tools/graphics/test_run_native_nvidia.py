#!/usr/bin/env python3
"""Synthetic orchestration tests; no GPU, PCI binding, or native boot is performed."""
import os
from pathlib import Path
import sys
import tempfile
import unittest

from run_native_nvidia import capture_guest, checked_devices


class NativeRunnerTests(unittest.TestCase):
    def create_pci_fixture(self, root, *, driver="vfio-pci", vendor="0x10de", pci_class="0x030000"):
        device = root / "0000:01:00.0"
        device.mkdir()
        (device / "vendor").write_text(vendor)
        (device / "class").write_text(pci_class)
        target = root / "drivers" / driver
        target.mkdir(parents=True)
        (device / "driver").symlink_to(target, target_is_directory=True)
        group = root / "groups" / "7"
        (group / "devices").mkdir(parents=True)
        (device / "iommu_group").symlink_to(group, target_is_directory=True)
        (group / "devices" / device.name).symlink_to(device, target_is_directory=True)
        return device, group

    def test_prebound_nvidia_fixture_is_selected_without_writes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            device, _ = self.create_pci_fixture(root)
            before = os.readlink(device / "driver")
            self.assertEqual(checked_devices(device.name, root), [device.name])
            self.assertEqual(os.readlink(device / "driver"), before)

    def test_rejects_invalid_or_duplicate_bdfs(self):
        for value in ("", "01:00.0", "0000:01:00.0,0000:01:00.0", "$(command)"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                checked_devices(value, Path("/nonexistent-fixture"))

    def test_rejects_host_driver_and_non_nvidia(self):
        for kwargs in ({"driver": "nvidia"}, {"vendor": "0x1002"}, {"pci_class": "0x040300"}):
            with self.subTest(kwargs=kwargs), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                device, _ = self.create_pci_fixture(root, **kwargs)
                with self.assertRaises(ValueError):
                    checked_devices(device.name, root)

    def test_rejects_missing_iommu_and_host_bound_peer(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            device, group = self.create_pci_fixture(root)
            peer = root / "0000:01:00.1"
            peer.mkdir()
            host_driver = root / "drivers/snd_hda_intel"
            host_driver.mkdir()
            (peer / "driver").symlink_to(host_driver, target_is_directory=True)
            (group / "devices" / peer.name).symlink_to(peer, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "host driver"):
                checked_devices(device.name, root)
            (device / "iommu_group").unlink()
            with self.assertRaisesRegex(ValueError, "IOMMU"):
                checked_devices(device.name, root)

    def test_captures_complete_synthetic_serial_terminal_line(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "serial.log"
            nonce = "0" * 32
            line = f"ASTERINAS_NATIVE_GPU_EXIT nonce={nonce} status=1"
            # This deliberately failing guest marker tests capture only, not acceptance.
            capture_guest([sys.executable, "-c", f"print({line!r}, flush=True)"], output, nonce, 3)
            self.assertEqual(output.read_text().strip(), line)

    def test_missing_terminal_marker_is_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "serial.log"
            with self.assertRaisesRegex(RuntimeError, "without a complete terminal marker"):
                capture_guest([sys.executable, "-c", "print('synthetic boot failure')"], output, "0" * 32, 3)
            self.assertIn("synthetic boot failure", output.read_text())


if __name__ == "__main__":
    unittest.main()
