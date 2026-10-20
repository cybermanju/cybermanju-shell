#!/usr/bin/env python3
"""Executable startup regressions; no compositor or physical GPU is simulated.

Set GPUI_SHELL_TEST_BINARY to a built gpuishell executable. Tests skip if absent.
Each test owns a private runtime/config directory; no live shell is contacted.
"""
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import unittest


BINARY = Path(os.environ.get(
    "GPUI_SHELL_TEST_BINARY",
    str(Path(__file__).resolve().parents[1] / "target/debug/gpuishell"),
)).resolve()


@unittest.skipUnless(BINARY.is_file(), "build gpuishell or set GPUI_SHELL_TEST_BINARY")
class SessionStartupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="gpui-startup-test-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.runtime = self.base / "runtime"
        self.runtime.mkdir(mode=0o700)
        self.env = dict(os.environ)
        for key in (
            "WAYLAND_DISPLAY", "NIRI_SOCKET", "NIRI_SOCKET_PATH", "DISPLAY",
            "HYPRLAND_INSTANCE_SIGNATURE", "MANGO_SOCKET", "MANGO_SOCKET_PATH",
            "GPUI_SHELL_FRAME_TELEMETRY", "GPUI_SHELL_FRAME_PROBE_SECONDS",
        ):
            self.env.pop(key, None)
        self.env.update({
            "XDG_RUNTIME_DIR": str(self.runtime),
            "XDG_CONFIG_HOME": str(self.base / "config"),
            "XDG_CACHE_HOME": str(self.base / "cache"),
            "XDG_DATA_HOME": str(self.base / "data"),
            "XDG_STATE_HOME": str(self.base / "state"),
        })

    def run_shell(self, *args):
        return subprocess.run(
            [str(BINARY), *args], env=self.env, capture_output=True,
            text=True, timeout=15,
        )

    def assert_socket_cleaned(self):
        self.assertFalse((self.runtime / "gpuishell.sock").exists())

    def test_validate_does_not_require_wayland(self):
        result = self.run_shell("--validate")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assert_socket_cleaned()

    def test_primary_without_wayland_exits_cleanly_and_removes_socket(self):
        result = self.run_shell()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("requires WAYLAND_DISPLAY", result.stderr)
        self.assertNotIn("panicked", result.stderr)
        self.assert_socket_cleaned()

    def test_empty_wayland_display_is_not_a_session(self):
        self.env["WAYLAND_DISPLAY"] = ""
        result = self.run_shell()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("requires WAYLAND_DISPLAY", result.stderr)
        self.assert_socket_cleaned()

    def test_missing_supported_compositor_exits_without_abort(self):
        self.env["WAYLAND_DISPLAY"] = "wayland-nonexistent-test"
        result = self.run_shell()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("could not connect to a supported compositor", result.stderr)
        self.assertNotIn("panicked", result.stderr)
        self.assert_socket_cleaned()

    def test_secondary_ipc_only_invocation_does_not_require_wayland(self):
        # A private IPC receiver fixture, not a fake graphical/session pass.
        received = []
        failures = []
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
            listener.bind(str(self.runtime / "gpuishell.sock"))
            listener.listen(1)
            listener.settimeout(10)

            def accept():
                try:
                    connection, _ = listener.accept()
                    with connection:
                        connection.settimeout(5)
                        received.append(connection.recv(4096))
                except Exception as error:
                    failures.append(error)

            thread = threading.Thread(target=accept, daemon=True)
            thread.start()
            result = self.run_shell("--input", "startup-test")
            thread.join(timeout=10)
            self.assertFalse(thread.is_alive(), "IPC receiver did not finish")
            self.assertFalse(failures, failures)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue(received and received[0], "secondary did not send IPC")
            self.assertTrue((self.runtime / "gpuishell.sock").exists())


if __name__ == "__main__":
    unittest.main()
