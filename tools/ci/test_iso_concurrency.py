"""Regression tests for ISO workflow concurrency policy."""

from pathlib import Path
import re
import unittest


WORKFLOW = Path(__file__).resolve().parents[2] / ".github" / "workflows" / "iso.yml"


class IsoConcurrencyTests(unittest.TestCase):
    def test_group_is_scoped_to_github_ref(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertRegex(text, r"(?m)^concurrency:\s*\n\s+group:\s*iso-\$\{\{\s*github\.ref\s*\}\}\s*$")

    def test_in_progress_runs_are_not_canceled(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertRegex(text, r"(?ms)^concurrency:\s*\n(?:(?!^jobs:).)*?^\s+cancel-in-progress:\s*false\s*$")


if __name__ == "__main__":
    unittest.main()
