#!/usr/bin/env python3
"""Host-only tests for phase marker behavior in the installer template."""

import pathlib
import re
import subprocess
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
TEMPLATE = ROOT / "asterinas/distro/aster_nixos_installer/templates/aster-nixos-install"


def helper_source():
    source = TEMPLATE.read_text()
    match = re.search(r"(?ms)^run_install_phase\(\) \{\n.*?^\}\n", source)
    if not match:
        raise AssertionError("run_install_phase() helper not found")
    return match.group(0)


class InstallerPhaseTests(unittest.TestCase):
    def run_bash(self, snippet):
        return subprocess.run(
            ["bash", "-c", helper_source() + "\n" + snippet],
            text=True,
            capture_output=True,
            check=False,
        )

    def test_stdout_and_exact_markers(self):
        result = self.run_bash('run_install_phase build-system bash -c \'printf "/nix/store/test\\n"\'')
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "/nix/store/test\n")
        self.assertEqual(
            result.stderr,
            "\nCYBERMANJU_INSTALL_PHASE:begin phase=build-system\n"
            "\nCYBERMANJU_INSTALL_PHASE:end phase=build-system status=0\n",
        )

    def test_failure_status_and_set_e_prevent_next_phase(self):
        result = self.run_bash(
            'set -e\nrun_install_phase install-system bash -c "exit 37"\n'
            'run_install_phase prune-generations bash -c "echo SHOULD_NOT_RUN"'
        )
        self.assertEqual(result.returncode, 37)
        self.assertEqual(result.stdout, "")
        self.assertEqual(
            result.stderr,
            "\nCYBERMANJU_INSTALL_PHASE:begin phase=install-system\n"
            "\nCYBERMANJU_INSTALL_PHASE:end phase=install-system status=37\n",
        )

    def test_argv_with_spaces_is_preserved(self):
        result = self.run_bash(
            "run_install_phase build-system bash -c 'printf \"<%s>\\\\n\" \"$1\"' _ "
            '"argument with spaces"'
        )
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "<argument with spaces>\n")

    def test_failed_closure_assignment_preserves_set_e(self):
        result = self.run_bash(
            'set -e\nSYSTEM_CLOSURE=$(run_install_phase build-system bash -c "exit 37")\n'
            'printf SHOULD_NOT_RUN'
        )
        self.assertEqual(result.returncode, 37)
        self.assertEqual(result.stdout, "")
        self.assertIn("CYBERMANJU_INSTALL_PHASE:end phase=build-system status=37\n", result.stderr)

    def test_template_integrates_all_phases_in_order(self):
        source = TEMPLATE.read_text()
        calls = [
            "run_install_phase build-system nix-build",
            "run_install_phase install-system nixos-install",
            "run_install_phase prune-generations nix-env",
            "run_install_phase collect-garbage nix-store",
        ]
        positions = [source.index(call) for call in calls]
        self.assertEqual(positions, sorted(positions))


if __name__ == "__main__":
    unittest.main()
