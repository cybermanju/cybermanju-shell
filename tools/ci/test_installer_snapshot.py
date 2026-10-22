"""Installer diagnostics only: real shell pipes and fixtures, never a VM or disk."""
import io
import json
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest import mock

from iso_vm import Console, parse_installer_phase, run
from vm_settings import TIMEOUTS

SUCCESS = rb"^Congratulations! Asterinas NixOS has been installed successfully!\r?$"
BEGIN = b"CYBERMANJU_INSTALL_PHASE:begin phase=install-system\r\n"
END = b"CYBERMANJU_INSTALL_PHASE:end phase=install-system status=37\r\n"


def empty_console():
    console = Console.__new__(Console)
    console.buffer, console.control_state = b"", "text"
    console.process = mock.Mock()
    console.process.poll.return_value = None
    return console


class InstallerSnapshotTests(unittest.TestCase):
    def test_phase_protocol_exactness_and_status_range(self):
        self.assertEqual(parse_installer_phase(BEGIN.rstrip(b"\r\n")),
                         {"name": "install-system", "event": "begin", "exit_status": None})
        self.assertEqual(parse_installer_phase(END.rstrip(b"\r\n")),
                         {"name": "install-system", "event": "end", "exit_status": 37})
        for line in (b"echo " + BEGIN.strip(), b" " + BEGIN.strip(), BEGIN.strip() + b" extra",
                     b"CYBERMANJU_INSTALL_PHASE:begin phase=unknown",
                     b"CYBERMANJU_INSTALL_PHASE:begin phase=install-system status=0",
                     b"CYBERMANJU_INSTALL_PHASE:end phase=install-system",
                     b"CYBERMANJU_INSTALL_PHASE:end phase=install-system status=-1",
                     b"CYBERMANJU_INSTALL_PHASE:end phase=install-system status=256"):
            with self.subTest(line=line):
                self.assertIsNone(parse_installer_phase(line))

    def test_ansi_and_every_chunk_boundary_preserve_real_phase_not_osc_payload(self):
        data = b"\x1b]0;" + END + b"\x07\x1b[32m" + BEGIN + b"\x1b[0m"
        for boundary in range(len(data) + 1):
            console = empty_console()
            console.append_output(data[:boundary])
            console.append_output(data[boundary:])
            self.assertEqual(console.last_installer_phase["event"], "begin")
            self.assertIsNone(getattr(console, "first_installer_failure", None))

    def test_phase_survives_buffer_eviction_and_snapshot_is_bounded_diagnostic(self):
        console = empty_console()
        raw = BEGIN + b"copying nix paths\n" + b"x" * (1024 * 1024 + 64) + b"\n"
        with mock.patch("iso_vm.time.monotonic", return_value=100):
            console.append_output(raw)
        self.assertNotIn(BEGIN.strip(), console.buffer)
        with tempfile.TemporaryDirectory() as temp, mock.patch("iso_vm.time.monotonic", return_value=105):
            target = Path(temp) / "failure.json"
            console.write_failure_snapshot(target, TimeoutError("fixture timeout"))
            result = json.loads(target.read_text())
        self.assertEqual(result["status"], "diagnostic_only")
        self.assertEqual(result["last_observed_installer_phase"]["name"], "install-system")
        self.assertEqual(result["observed_output_bytes"], len(raw))
        self.assertEqual(result["seconds_since_output"], 5)
        self.assertIsNone(result["launcher_returncode"])
        self.assertLessEqual(len(result["serial_tail"]), 8192)
        self.assertFalse(result["installation_verified"])
        self.assertFalse(result["installed_boot_verified"])

    def test_legacy_command_listing_and_nix_copy_are_not_phase_evidence(self):
        console = empty_console()
        console.append_output(b"/run/current-system/sw/bin/nix-build\n"
                              b"/run/current-system/sw/bin/nixos-install\n"
                              b"copying path '/nix/store/example-systemd' to 'local'...\n")
        self.assertIsNone(getattr(console, "last_installer_phase", None))
        self.assertIsNone(getattr(console, "first_installer_failure", None))

    def test_empty_snapshot_has_no_invented_output_or_phase(self):
        with tempfile.TemporaryDirectory() as temp:
            target = Path(temp) / "failure.json"
            empty_console().write_failure_snapshot(target, RuntimeError("fixture EOF"))
            result = json.loads(target.read_text())
        self.assertIsNone(result["seconds_since_output"])
        self.assertIsNone(result["last_observed_installer_phase"])
        self.assertEqual(result["observed_output_bytes"], 0)

    def test_phase_zero_is_not_installation_success_and_failure_cannot_be_laundered(self):
        console = empty_console()
        console.append_output(BEGIN + END.replace(b"37", b"0"))
        with self.assertRaises(TimeoutError):
            console.expect(SUCCESS, 0, reject_installer_failure=True)
        console.append_output(END + END.replace(b"37", b"0") +
                              b"Congratulations! Asterinas NixOS has been installed successfully!\r\n")
        with self.assertRaisesRegex(RuntimeError, r"install-system failed \(exit 37\)"):
            console.expect(SUCCESS, 0, reject_installer_failure=True)
        # Other checked commands retain their existing protocol/default behavior.
        self.assertIsNotNone(console.expect(SUCCESS, 0))

    def test_real_shell_wrapper_exit_fails_immediately_and_snapshot_keeps_status(self):
        template = (Path(__file__).resolve().parents[2] /
                    "asterinas/distro/aster_nixos_installer/templates/aster-nixos-install").read_text()
        helper = re.search(r"(?ms)^run_install_phase\(\)\s*\{\n.*?^\}", template).group(0)
        process = subprocess.Popen(["bash", "--noprofile", "--norc"], stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        console = Console(process)
        try:
            console.send(helper)
            console.send("run_install_phase install-system bash -c 'printf partial-progress >&2; exit 37'")
            with mock.patch("iso_vm.sys.stdout"), self.assertRaisesRegex(RuntimeError, "exit 37") as failure:
                console.expect(SUCCESS, 2, reject_installer_failure=True)
            with tempfile.TemporaryDirectory() as temp:
                target = Path(temp) / "failure.json"
                console.write_failure_snapshot(target, failure.exception)
                result = json.loads(target.read_text())
            self.assertEqual(result["first_observed_installer_failure"]["exit_status"], 37)
            self.assertFalse(result["installation_verified"])
        finally:
            process.stdin.close()
            process.wait(timeout=2)
            console.selector.close()
            process.stdout.close()

    def test_run_preserves_timeout_if_snapshot_cannot_be_written(self):
        original = TimeoutError("original installation deadline")
        with mock.patch("iso_vm.settings", return_value=({"QEMU_ACCEL": "tcg", "ENABLE_KVM": "0"}, "fixture")), \
             mock.patch("iso_vm.subprocess.Popen") as popen, \
             mock.patch("iso_vm.Console") as console_type, \
             mock.patch("iso_vm.timed_phase"), \
             mock.patch("iso_vm.sys.stdout", new_callable=io.StringIO), \
             mock.patch("iso_vm.sys.stderr", new_callable=io.StringIO):
            popen.return_value.poll.return_value = 0
            console = console_type.return_value
            console.expect.side_effect = [None, original]
            console.write_failure_snapshot.side_effect = OSError("unwritable diagnostic path")
            with self.assertRaises(TimeoutError) as failure:
                run("install")
            self.assertIs(failure.exception, original)
            console.write_failure_snapshot.assert_called_once()
            self.assertEqual(console.write_failure_snapshot.call_args.args[0].name, "iso-install-failure-snapshot.json")
            self.assertEqual(console.expect.call_args.args, (SUCCESS, TIMEOUTS["ISO_VM_INSTALL_SECONDS"]))
            self.assertTrue(console.expect.call_args.kwargs["reject_installer_failure"])
            self.assertFalse(any("BOOTX64.EFI" in call.args[0] for call in console.checked.call_args_list))

    def test_success_still_requires_original_marker_efi_checks_and_shutdown(self):
        with mock.patch("iso_vm.settings", return_value=({"QEMU_ACCEL": "tcg", "ENABLE_KVM": "0"}, "fixture")), \
             mock.patch("iso_vm.subprocess.Popen") as popen, \
             mock.patch("iso_vm.Console") as console_type, \
             mock.patch("iso_vm.timed_phase"), \
             mock.patch("iso_vm.sys.stdout", new_callable=io.StringIO):
            popen.return_value.poll.return_value = 0
            popen.return_value.wait.return_value = 0
            run("install")
            console = console_type.return_value
            console.write_failure_snapshot.assert_not_called()
            self.assertIn(SUCCESS, [call.args[0] for call in console.expect.call_args_list])
            self.assertTrue(any("BOOTX64.EFI" in call.args[0] for call in console.checked.call_args_list))
            self.assertIn(mock.call("poweroff"), console.send.call_args_list)
            popen.return_value.wait.assert_called_once()


if __name__ == "__main__":
    unittest.main()
