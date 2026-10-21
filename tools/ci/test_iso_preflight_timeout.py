"""Regression coverage for the ISO runner preflight timeout and guardrails."""
from pathlib import Path
import re
import unittest

WORKFLOW = Path(__file__).resolve().parents[2] / ".github" / "workflows" / "iso.yml"


class IsoPreflightTimeoutTests(unittest.TestCase):
    def test_preflight_has_bounded_timeout_and_keeps_capacity_guardrails(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        step = re.search(
            r"(?ms)^      - name: Reclaim unused ephemeral SDKs, measure capacity and select KVM or TCG\n(?P<body>.*?)(?=^      - name: )",
            text,
        )
        self.assertIsNotNone(step, "preflight workflow step must exist")
        self.assertRegex(step.group("body"), r"(?m)^        timeout-minutes: 10$")
        self.assertRegex(text, r"(?m)^    timeout-minutes: 358$")
        self.assertRegex(text, r"(?m)^      ISO_DISK_RESERVE_BYTES: '1073741824'$")
        self.assertIn("--limit-from-available-space", step.group("body"))
        self.assertIn("--reserve-bytes \"$ISO_DISK_RESERVE_BYTES\"", step.group("body"))
        self.assertIn("docker info --format", step.group("body"))
        self.assertIn("vm_settings.py shell", step.group("body"))


if __name__ == "__main__":
    unittest.main()
