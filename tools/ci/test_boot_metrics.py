import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from boot_metrics import timed_phase


class BootMetricsTests(unittest.TestCase):
    def test_success_writes_serializable_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            with timed_phase(directory, "live-checks", "kvm"):
                pass
            path = Path(directory) / "boot-metrics-live-checks.json"
            record = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual(record["phase"], "live-checks")
            self.assertEqual(record["status"], "success")
            self.assertEqual(record["acceleration"], "kvm")
            self.assertGreaterEqual(record["elapsed_seconds"], 0)

    def test_failure_writes_error_and_preserves_original_exception(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "original problem"):
                with timed_phase(directory, "install", "tcg"):
                    raise ValueError("original problem")
            record = json.loads(
                (Path(directory) / "boot-metrics-install.json").read_text(encoding="utf-8")
            )
            self.assertEqual(record["status"], "failure")
            self.assertEqual(record["acceleration"], "tcg")
            self.assertIn("ValueError: original problem", record["error"])


if __name__ == "__main__":
    unittest.main()
