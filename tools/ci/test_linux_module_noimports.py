#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Negative preflight checks only: no successful mocked build or module load."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "tools/ci/test_linux_module_noimports.sh"
RELEASE = "6.8.0-147-generic"


class NoImportFixturePreflightTests(unittest.TestCase):
    def test_rejects_invalid_headers_and_does_not_reuse_success_status(self):
        for case in ("missing", "architecture", "release", "symvers"):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                headers = root / "headers"
                generated = headers / "include/generated"
                generated.mkdir(parents=True)
                (headers / ".config").write_text("CONFIG_X86_64=y\n")
                (headers / "Module.symvers").write_text("fixture\n")
                (generated / "utsrelease.h").write_text(f'#define UTS_RELEASE "{RELEASE}"\n')
                if case == "missing":
                    headers = root / "absent"
                elif case == "architecture":
                    (headers / ".config").write_text("CONFIG_X86_64=n\n")
                elif case == "release":
                    (generated / "utsrelease.h").write_text('#define UTS_RELEASE "wrong-release"\n')
                else:
                    (headers / "Module.symvers").unlink()
                out = root / "output"
                out.mkdir()
                (out / "status.txt").write_text("exit_code=0\nloaded=false\n")
                env = {**os.environ, "KERNEL_HEADERS": str(headers), "EXPECTED_KERNEL_RELEASE": RELEASE}
                result = subprocess.run(
                    ["bash", str(SCRIPT), str(out)], cwd=ROOT, env=env,
                    capture_output=True, text=True, check=False, timeout=10,
                )
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                status = (out / "status.txt").read_text()
                self.assertIn("loaded=false\n", status)
                self.assertNotIn("exit_code=0\n", status)
                self.assertNotIn("exit_code=pending\n", status)
                if case == "release":
                    self.assertIn("wrong-release", result.stderr)


if __name__ == "__main__":
    unittest.main()
