#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Regression tests for the Linux module kbuild preflight checks.

These tests exercise only failing preflight paths with temporary header trees;
they do not stub a successful build or load a module.
"""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "tools" / "ci" / "test_linux_module_kbuild.sh"
EXPECTED_RELEASE = "6.8.0-147-generic"


class LinuxModuleKbuildPreflightTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="linux-module-kbuild-test-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.headers = self.base / "headers"
        self.headers.mkdir()
        self.out = self.base / "absolute-output"
        self.assertTrue(self.out.is_absolute())
        (self.headers / ".config").write_text("CONFIG_X86_64=y\n")
        generated = self.headers / "include" / "generated"
        generated.mkdir(parents=True)
        (generated / "utsrelease.h").write_text(
            f'#define UTS_RELEASE "{EXPECTED_RELEASE}"\n'
        )
        (self.headers / "Module.symvers").write_text("fixture symvers\n")

    def run_preflight(self):
        env = os.environ.copy()
        env["KERNEL_HEADERS"] = str(self.headers)
        env["EXPECTED_KERNEL_RELEASE"] = EXPECTED_RELEASE
        return subprocess.run(
            ["bash", str(SCRIPT), str(self.out)],
            cwd=ROOT,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def assert_fails(self, result):
        self.assertNotEqual(result.returncode, 0, msg=result.stdout + result.stderr)

    def test_missing_headers_fail(self):
        self.headers = self.base / "headers-does-not-exist"
        result = self.run_preflight()
        self.assert_fails(result)

    def test_non_x86_64_config_fails(self):
        (self.headers / ".config").write_text("CONFIG_X86_64=n\n")
        result = self.run_preflight()
        self.assert_fails(result)

    def test_uts_release_mismatch_fails_with_message(self):
        (self.headers / "include" / "generated" / "utsrelease.h").write_text(
            '#define UTS_RELEASE "different-release"\n'
        )
        result = self.run_preflight()
        self.assert_fails(result)
        self.assertIn(
            f"Header release mismatch: expected {EXPECTED_RELEASE}, got different-release",
            result.stderr,
        )

    def test_missing_module_symvers_fails(self):
        (self.headers / "Module.symvers").unlink()
        result = self.run_preflight()
        self.assert_fails(result)


if __name__ == "__main__":
    unittest.main()
