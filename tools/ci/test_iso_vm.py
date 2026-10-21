"""No VM runs: test console output validation and QEMU argument construction."""
import io
import os
import re
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

from iso_vm import Console, run


class DriverAccelerationTests(unittest.TestCase):
    def test_both_acceleration_branches_without_starting_qemu(self):
        for present, expected in ((False, "0"), (True, "1")):
            with self.subTest(kvm_available=present), \
                 mock.patch.dict(os.environ, {}, clear=True), \
                 mock.patch("vm_settings.Path.is_char_device", return_value=present), \
                 mock.patch("vm_settings.os.access", return_value=True), \
                 mock.patch("iso_vm.subprocess.Popen") as popen, \
                 mock.patch("iso_vm.Console"), \
                 mock.patch("iso_vm.timed_phase"), \
                 mock.patch("sys.stdout", new_callable=io.StringIO):
                popen.return_value.wait.return_value = 0
                popen.return_value.poll.return_value = 0
                run("live-usb")
                env = popen.call_args.kwargs["env"]
                self.assertEqual(env["ENABLE_KVM"], expected)
                self.assertEqual(env["QEMU_ACCEL"], "kvm" if present else "tcg")
                self.assertEqual(env["ISO_BOOT_MODE"], "usb")
                self.assertEqual(env["CONSOLE"], "ttyS0")


class ConsoleTests(unittest.TestCase):
    def test_checked_command_uses_whole_line_and_split_nonce(self):
        console = Console.__new__(Console)
        console.send = mock.Mock()
        console.expect = mock.Mock(return_value=re.match(rb"([0-9]+)", b"0"))
        with mock.patch("iso_vm.uuid.uuid4") as token:
            token.return_value.hex = "a" * 32
            console.checked("test -s /root/configuration.nix")
        command = console.send.call_args.args[0]
        self.assertIn("( test -s /root/configuration.nix ); ci_status=$?;", command)
        self.assertNotIn("CI_" + "a" * 32, command)
        pattern = console.expect.call_args.args[0]
        self.assertIsNone(re.search(pattern, command.encode(), re.MULTILINE))
        self.assertIsNotNone(re.search(pattern, b"CI_" + b"a" * 32 + b":0\r\n", re.MULTILINE))

    def test_checked_command_reports_nonzero_exit_without_timeout(self):
        console = Console.__new__(Console)
        console.send = mock.Mock()
        console.expect = mock.Mock(return_value=re.match(rb"([0-9]+)", b"7"))
        with self.assertRaisesRegex(RuntimeError, r"Guest command failed \(exit 7\)"):
            console.checked("false")

    def test_checked_command_real_pipe_success_and_failure(self):
        # Exercise the protocol against a real shell, not mocked expect output.
        process = subprocess.Popen(["bash", "--noprofile", "--norc"],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT)
        console = Console(process)
        try:
            console.checked("true", seconds=2)
            for command in ("false", "exit 9", "test -f /nonexistent-ci-file"):
                with self.subTest(command=command), self.assertRaises(RuntimeError):
                    console.checked(command, seconds=2)
            console.checked("true", seconds=2)
        finally:
            process.stdin.close()
            process.wait(timeout=2)
            console.selector.close()
            process.stdout.close()

    def test_console_does_not_accept_substring_or_stale_echo(self):
        pattern = rb"^CYBERMANJU_LIVE_INSTALLER_READY\r?$"
        self.assertIsNone(re.search(pattern, b"echo CYBERMANJU_LIVE_INSTALLER_READY\n", re.MULTILINE))
        self.assertIsNotNone(re.search(pattern, b"CYBERMANJU_LIVE_INSTALLER_READY\r\n", re.MULTILINE))

    def test_systemd_login_osc_and_csi_at_every_read_boundary(self):
        # OSC 3008 is the actual shape emitted by systemd login in the failed VM.
        output = (b"login\r\n\x1b]3008;start=token;comm=login;type=session\x1b\\"
                  b"CYBERMANJU_LIVE_INSTALLER_READY\r\n\x1b[?2004h")
        for boundary in range(len(output) + 1):
            with self.subTest(boundary=boundary):
                console = Console.__new__(Console)
                console.buffer, console.control_state = b"", "text"
                console.append_output(output[:boundary])
                console.append_output(output[boundary:])
                self.assertEqual(console.buffer,
                                 b"login\r\nCYBERMANJU_LIVE_INSTALLER_READY\r\n")
                self.assertIsNotNone(console.expect(
                    rb"^CYBERMANJU_LIVE_INSTALLER_READY\r?$", seconds=0))

    def test_ansi_never_exposes_metadata_or_turns_echo_into_marker(self):
        console = Console.__new__(Console)
        console.buffer, console.control_state = b"", "text"
        # Hidden OSC payload must not become a marker, even one with newlines.
        output = (b"\x1b]0;\nCYBERMANJU_LIVE_INSTALLER_READY\n\x07"
                  b"\x1b[31mecho CYBERMANJU_LIVE_INSTALLER_READY\x1b[0m\r\n")
        for byte in output:
            console.append_output(bytes([byte]))
        self.assertEqual(console.buffer, b"echo CYBERMANJU_LIVE_INSTALLER_READY\r\n")
        self.assertIsNone(re.search(rb"^CYBERMANJU_LIVE_INSTALLER_READY\r?$",
                                   console.buffer, re.MULTILINE))

    def test_expect_reads_raw_ansi_pipe_and_preserves_log(self):
        output = (b"\x1b]3008;comm=login\x1b\\"
                  b"CYBERMANJU_LIVE_INSTALLER_READY\r\n")
        process = subprocess.Popen(
            ["python3", "-c", "import os; os.write(1, " + repr(output) + ")"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        console = Console(process)
        try:
            with mock.patch("iso_vm.sys.stdout") as logged:
                console.expect(rb"^CYBERMANJU_LIVE_INSTALLER_READY\r?$", seconds=2)
            self.assertEqual(b"".join(call.args[0]
                                     for call in logged.buffer.write.call_args_list), output)
        finally:
            process.stdin.close()
            process.wait(timeout=2)
            console.selector.close()
            process.stdout.close()


class LiveConsoleConfigTests(unittest.TestCase):
    def test_live_iso_selects_uart_but_preserves_legacy_auto_install(self):
        source = (Path(__file__).resolve().parents[2]
                  / "asterinas/distro/iso_image/default.nix").read_text()
        self.assertRegex(source, r'boot\.initrd\.kernelModules\s*=\s*\[[^\]]*"virtio_console"')
        self.assertIn('(if autoInstall then "console=hvc0" else "console=ttyS0,115200n8")', source)
        self.assertIn("/dev/ttyS0|/dev/hvc0)", source)
        self.assertIn('services.getty.autologinUser = pkgs.lib.mkForce "root";', source)


class RunScriptTests(unittest.TestCase):
    def run_script(self, mode, *, kvm_available=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "tools/nixos").mkdir(parents=True)
            (root / "target/nixos/iso_image/iso").mkdir(parents=True)
            source = Path(__file__).resolve().parents[2] / "asterinas/tools/nixos/run.sh"
            shutil.copyfile(source, root / "tools/nixos/run.sh")
            (root / "target/nixos/iso_image/iso/test.iso").write_bytes(b"not a boot image")
            arguments = root / "arguments.txt"
            qemu = root / "fake-qemu"
            qemu.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$ARGUMENTS"\nexit "${QEMU_STATUS:-0}"\n')
            args = root / "tools/qemu_args.sh"
            shutil.copyfile(source.parent.parent / "qemu_args.sh", args)
            qemu.chmod(0o755)
            args.chmod(0o755)
            env = dict(os.environ, QEMU_BIN=str(qemu), ARGUMENTS=str(arguments),
                       ISO_BOOT_MODE=mode, NIXOS_DISK_SIZE_IN_MB="8", ENABLE_KVM="0", NETDEV="none")
            if kvm_available is not None:
                # Bash's [ builtin is mocked only for the three /dev/kvm checks.
                # No real KVM device is opened and QEMU is still a tiny fake.
                probe = root / "mock-device-checks.sh"
                probe.write_text(
                    'function [ {\n'
                    '  if [[ $# == 3 && "$2" == /dev/kvm && '
                    '( "$1" == -c || "$1" == -r || "$1" == -w ) ]]; then\n'
                    '    [[ "$MOCK_KVM_AVAILABLE" == 1 ]]; return $?\n'
                    '  fi\n'
                    '  builtin [ "$@"\n'
                    '}\n'
                )
                env.update(BASH_ENV=str(probe), ENABLE_KVM="1",
                           MOCK_KVM_AVAILABLE="1" if kvm_available else "0")
            result = subprocess.run(["bash", str(root / "tools/nixos/run.sh"), "iso"],
                                    env=env, capture_output=True, text=True, check=False)
            image = root / "target/nixos/asterinas.img"
            stats = image.stat()
            return result, arguments.read_text() if arguments.exists() else "", stats

    def test_cdrom_path_preserves_capacity_but_is_sparse(self):
        result, arguments, stats = self.run_script("cdrom")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("-cdrom\n", arguments)
        self.assertIn("-bios\n", arguments)
        self.assertIn("/OVMF.fd", arguments)
        self.assertNotIn("id=x0", arguments)
        self.assertNotIn("nvme0n1", arguments)
        self.assertEqual(arguments.count("virtio-blk-pci"), 1)
        self.assertIn("-accel\ntcg,thread=multi", arguments)
        self.assertIn("QEMU acceleration: tcg", result.stdout)
        self.assertIn("-vmx", arguments)
        self.assertEqual(stats.st_size, 8 * 1024 * 1024)
        self.assertLess(stats.st_blocks * 512, 1024 * 1024)

    def test_usb_path_is_read_only_actual_usb_not_cdrom(self):
        result, arguments, _ = self.run_script("usb")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("usb-storage,bus=iso-xhci.0", arguments)
        self.assertIn("readonly=on", arguments)
        self.assertNotIn("-cdrom", arguments)

    def test_unknown_media_mode_fails_before_qemu(self):
        result, arguments, _ = self.run_script("invalid")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(arguments, "")

    def test_mocked_accessible_kvm_selects_explicit_kvm_backend(self):
        result, arguments, _ = self.run_script("cdrom", kvm_available=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("-accel\nkvm", arguments)
        self.assertIn("QEMU acceleration: kvm", result.stdout)
        self.assertIn("+vmx", arguments)

    def test_mocked_missing_kvm_falls_back_despite_enable_request(self):
        result, arguments, _ = self.run_script("cdrom", kvm_available=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("-accel\ntcg,thread=multi", arguments)
        self.assertIn("QEMU acceleration: tcg", result.stdout)
        self.assertIn("-vmx", arguments)

    def test_uart_console_routes_stdin_and_stdout_to_captured_mux(self):
        script = Path(__file__).resolve().parents[2] / "asterinas/tools/qemu_args.sh"
        result = subprocess.run(["bash", str(script), "nixos"],
                                env=dict(os.environ, CONSOLE="ttyS0", NETDEV="none"),
                                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("-serial chardev:mux", result.stdout)
        self.assertNotIn("-device virtconsole", result.stdout)

    def test_asterinas_default_console_remains_virtio(self):
        script = Path(__file__).resolve().parents[2] / "asterinas/tools/qemu_args.sh"
        env = dict(os.environ, NETDEV="none")
        env.pop("CONSOLE", None)
        result = subprocess.run(["bash", str(script), "nixos"], env=env,
                                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("-device virtconsole,chardev=mux", result.stdout)
        self.assertIn("-serial file:qemu-serial.log", result.stdout)

    def test_cpu_flags_follow_acceleration_without_running_qemu(self):
        script = Path(__file__).resolve().parents[2] / "asterinas/tools/qemu_args.sh"
        for enabled, expected in (("0", "-vmx"), ("1", "+vmx")):
            result = subprocess.run(["bash", str(script), "nixos"],
                                    env=dict(os.environ, ENABLE_KVM=enabled, NETDEV="none"),
                                    capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn(expected, result.stdout)


if __name__ == "__main__":
    unittest.main()
