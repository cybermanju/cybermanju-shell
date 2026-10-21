"""Safety regressions for ephemeral-runner cleanup; never invoke its delete path."""
import os
from pathlib import Path
import re
import subprocess
import unittest


class RunnerPreparationTests(unittest.TestCase):
    script = Path(__file__).with_name('prepare-iso-runner.sh')

    def test_cleanup_refuses_local_environment_before_running_any_commands(self):
        env = dict(os.environ, GITHUB_ACTIONS='false', RUNNER_ENVIRONMENT='local',
                   RUNNER_OS='Linux', GITHUB_WORKSPACE='/workspace', PATH='/nonexistent')
        result = subprocess.run(['/bin/bash', str(self.script)], env=env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn('restricted to ephemeral GitHub-hosted', result.stderr)
        self.assertNotIn('command not found', result.stderr)

    def test_cleanup_refuses_self_hosted_environment(self):
        env = dict(os.environ, GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='self-hosted',
                   RUNNER_OS='Linux', GITHUB_WORKSPACE='/home/runner/work/repo/repo',
                   PATH='/nonexistent')
        result = subprocess.run(['/bin/bash', str(self.script)], env=env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn('restricted to ephemeral GitHub-hosted', result.stderr)

    def test_delete_targets_are_exactly_the_three_unused_sdks(self):
        script = self.script.read_text()
        targets = re.search(r'for sdk in ([^;]+); do', script).group(1).split()
        self.assertEqual(targets, ['/usr/share/dotnet', '/usr/local/lib/android', '/opt/ghc'])
        deletes = [line.strip() for line in script.splitlines() if line.strip().startswith('rm ')]
        self.assertEqual(deletes, ['rm -rf --one-file-system -- "$sdk"'])
        self.assertIn('! -L "$sdk"', script)
        self.assertIn('mountpoint -q "$sdk"', script)
        self.assertIn('$(realpath "$sdk") == "$sdk"', script)

    def test_workflow_cleans_before_measuring_actual_capacity(self):
        workflow = (self.script.parents[2] / '.github/workflows/iso.yml').read_text()
        self.assertLess(workflow.index('bash tools/ci/prepare-iso-runner.sh'),
                        workflow.index('disk_budget.py init'))
        self.assertIn('--limit-from-available-space', workflow)
        self.assertNotIn('ISO_DISK_LIMIT_BYTES:', workflow)
        self.assertIn("ISO_DISK_RESERVE_BYTES: '1073741824'", workflow)


if __name__ == '__main__':
    unittest.main()
