"""Device availability is mocked. No /dev/kvm opens, QEMU, or real VM probes."""
import contextlib
import io
import json
import os
from pathlib import Path
import re
import tempfile
import unittest
from unittest import mock

import vm_settings
from vm_settings import TIMEOUTS, select_acceleration, settings


class AccelerationTests(unittest.TestCase):
    def test_absent_device_uses_tcg_even_if_kvm_requested(self):
        with mock.patch("vm_settings.Path.is_char_device", return_value=False), \
             mock.patch("vm_settings.os.access") as access:
            values, reason = settings({"ENABLE_KVM": "1"})
        self.assertEqual(values["QEMU_ACCEL"], "tcg")
        self.assertEqual(values["ENABLE_KVM"], "0")
        self.assertIn("absent", reason)
        access.assert_not_called()

    def test_accessible_character_device_uses_kvm(self):
        with mock.patch("vm_settings.Path.is_char_device", return_value=True), \
             mock.patch("vm_settings.os.access", return_value=True) as access:
            values, reason = settings({})
        self.assertEqual(values["QEMU_ACCEL"], "kvm")
        self.assertEqual(values["ENABLE_KVM"], "1")
        self.assertIn("not a VM probe", reason)
        access.assert_called_once_with("/dev/kvm", os.R_OK | os.W_OK)

    def test_inaccessible_device_uses_tcg(self):
        with mock.patch("vm_settings.Path.is_char_device", return_value=True), \
             mock.patch("vm_settings.os.access", return_value=False):
            self.assertEqual(select_acceleration({})[0], "tcg")

    def test_disabled_kvm_stays_tcg_without_probing(self):
        with mock.patch("vm_settings.Path.is_char_device") as probe:
            self.assertEqual(select_acceleration({"ENABLE_KVM": "0"})[0], "tcg")
        probe.assert_not_called()

    def test_device_stat_error_falls_back_to_tcg(self):
        with mock.patch("vm_settings.Path.is_char_device", side_effect=OSError("denied")):
            self.assertEqual(select_acceleration({})[0], "tcg")

    def test_shell_and_record_report_the_selected_acceleration(self):
        for present, expected in ((False, "tcg"), (True, "kvm")):
            with self.subTest(acceleration=expected), tempfile.TemporaryDirectory() as directory:
                record = Path(directory) / "acceleration.json"
                output, errors = io.StringIO(), io.StringIO()
                with mock.patch.dict(os.environ, {}, clear=True), \
                     mock.patch("vm_settings.Path.is_char_device", return_value=present), \
                     mock.patch("vm_settings.os.access", return_value=True), \
                     mock.patch("sys.argv", ["vm_settings.py", "shell", "--record", str(record)]), \
                     contextlib.redirect_stdout(output), contextlib.redirect_stderr(errors):
                    vm_settings.main()
                self.assertIn(f"export QEMU_ACCEL={expected}", output.getvalue())
                self.assertIn(f"QEMU acceleration: {expected}", errors.getvalue())
                self.assertEqual(json.loads(record.read_text())["acceleration"], expected)


class DeadlineTests(unittest.TestCase):
    def test_inner_deadlines_fit_shell_and_workflow_limits(self):
        # Include successful shutdown, process reap and error-cleanup allowance.
        common = (TIMEOUTS["ISO_VM_BOOT_SECONDS"]
                  + TIMEOUTS["ISO_VM_SHUTDOWN_SECONDS"]
                  + TIMEOUTS["ISO_VM_PROCESS_WAIT_SECONDS"] + 20)
        live = common + 2 * TIMEOUTS["ISO_VM_COMMAND_SECONDS"]
        install = common + 3 * TIMEOUTS["ISO_VM_COMMAND_SECONDS"] + TIMEOUTS["ISO_VM_INSTALL_SECONDS"]
        self.assertLess(live, int(TIMEOUTS["ISO_VM_LIVE_TIMEOUT"][:-1]) * 60)
        self.assertLess(install, int(TIMEOUTS["ISO_VM_INSTALL_TIMEOUT"][:-1]) * 60)
        # Allow timeout's 30-second SIGKILL escalation below the Actions step.
        for key, step_minutes in (("ISO_VM_LIVE_TIMEOUT", 35),
                                  ("ISO_VM_INSTALL_TIMEOUT", 100),
                                  ("ISO_VM_BOOT_TIMEOUT", 60)):
            self.assertLess(int(TIMEOUTS[key][:-1]) * 60 + 30, step_minutes * 60)
        self.assertLess(int(TIMEOUTS["NIXOS_TEST_TIMEOUT"][:-3]),
                        int(TIMEOUTS["ISO_VM_BOOT_TIMEOUT"][:-1]))

    def test_combined_step_maxima_are_below_job_and_hosted_limits(self):
        workflow = (Path(__file__).resolve().parents[2] / ".github/workflows/iso.yml").read_text()
        limits = [int(value) for value in re.findall(r"timeout-minutes: (\d+)", workflow)]
        self.assertEqual(limits[0], 350)
        self.assertEqual(sum(limits[1:]), 333)
        self.assertLess(sum(limits[1:]), limits[0])
        self.assertLess(limits[0], 360)


if __name__ == "__main__":
    unittest.main()
