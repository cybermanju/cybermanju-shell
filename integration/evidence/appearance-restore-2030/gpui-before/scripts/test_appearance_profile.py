#!/usr/bin/env python3
"""Exercise the real GPUI binary's cosmetic snapshot boundary, never GPU acceptance."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / 'crates/app/src/config/appearance/fixtures/appearance-profile-v1.json'
BINARY = Path(os.environ.get('GPUI_SHELL_TEST_BINARY', str(ROOT / 'target/debug/gpuishell'))).resolve()


class AppearanceCliTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not BINARY.is_file() or not FIXTURE.is_file():
            raise RuntimeError('A real built shell and the shared app-export fixture are required')
        cls.fixture = json.loads(FIXTURE.read_text())

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='gpui-appearance-cli-')
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.runtime = self.directory / 'runtime'
        self.runtime.mkdir(mode=0o700)
        self.config = self.directory / 'config'
        self.config.mkdir()
        self.profile = self.directory / 'appearance.json'
        self.profile.write_text(json.dumps(self.fixture))
        self.env = dict(os.environ, XDG_RUNTIME_DIR=str(self.runtime), XDG_CONFIG_HOME=str(self.config))
        self.env.pop('WAYLAND_DISPLAY', None)
        self.env.pop('DISPLAY', None)

    def run_shell(self, *args):
        return subprocess.run([str(BINARY), *args], env=self.env, stdin=subprocess.DEVNULL,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=20)

    def test_shared_app_export_validates_without_wayland_or_mutating_preferences(self):
        before = self.profile.read_bytes()
        result = self.run_shell('--validate', '--appearance-profile', str(self.profile))
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn('CyberManju appearance v1:', result.stdout)
        self.assertIn('retained, NOT applied:', result.stdout)
        self.assertIn('No live sync or Asterinas/GPU certification', result.stdout)
        self.assertEqual(before, self.profile.read_bytes())
        self.assertFalse((self.runtime / 'gpuishell.sock').exists())
        self.assertEqual(list(self.config.rglob('*.toml')), [])

    def test_invalid_profiles_fail_before_wayland_or_ipc(self):
        cases = []
        for key, value in [('version', 2), ('serverUrl', 'https://private.invalid')]:
            profile = dict(self.fixture)
            profile[key] = value
            cases.append(profile)
        profile = dict(self.fixture)
        profile['wallpaper'] = {'kind': 'url', 'url': 'https://user:password@example.invalid/picture'}
        cases.append(profile)
        for case in cases:
            with self.subTest(case_keys=list(case)):
                self.profile.write_text(json.dumps(case))
                result = self.run_shell('--appearance-profile', str(self.profile))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('Appearance profile rejected:', result.stdout)
                self.assertNotIn('requires WAYLAND_DISPLAY', result.stdout)
                self.assertNotIn('user:password', result.stdout)
                self.assertFalse((self.runtime / 'gpuishell.sock').exists())

    def test_oversized_file_is_rejected(self):
        self.profile.write_bytes(b' ' * (64 * 1024 + 1))
        result = self.run_shell('--validate', '--appearance-profile', str(self.profile))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('exceeds 64 KiB', result.stdout)

    def test_secondary_profile_refused_without_sending_launcher_or_claiming_import(self):
        path = self.runtime / 'gpuishell.sock'
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as server:
            server.bind(str(path))
            server.listen(2)
            server.settimeout(2)
            result = self.run_shell('--appearance-profile', str(self.profile))
            connection, _ = server.accept()
            with connection:
                connection.settimeout(2)
                self.assertEqual(connection.recv(4096), b'')
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn('NOT applied', result.stdout)
            self.assertTrue(path.exists())

    def test_ordinary_secondary_launcher_still_sends_existing_ipc_command(self):
        path = self.runtime / 'gpuishell.sock'
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as server:
            server.bind(str(path))
            server.listen(2)
            server.settimeout(2)
            result = self.run_shell('--input', 'unchanged launcher')
            connection, _ = server.accept()
            with connection:
                connection.settimeout(2)
                payload = connection.recv(4096)
            self.assertEqual(result.returncode, 0, result.stdout)
            self.assertIn(b'unchanged launcher', payload)
            self.assertTrue(path.exists())

    def test_existing_config_validation_does_not_require_a_profile(self):
        result = self.run_shell('--validate')
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertNotIn('CyberManju appearance v1:', result.stdout)
        self.assertFalse((self.runtime / 'gpuishell.sock').exists())


if __name__ == '__main__':
    unittest.main()
