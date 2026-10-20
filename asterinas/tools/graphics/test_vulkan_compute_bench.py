#!/usr/bin/env python3
"""Build and test the Vulkan compute benchmark without claiming GPU hardware."""
from __future__ import annotations
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent


class VulkanComputeBenchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.temp = tempfile.TemporaryDirectory(prefix="vulkan-compute-bench-tests-")
        cls.build_dir = Path(cls.temp.name)
        cls.binary = cls.build_dir / "vulkan_compute_bench"
        subprocess.run(
            [str(HERE / "build_vulkan_compute_bench.sh"), str(cls.binary)],
            check=True, capture_output=True, text=True, timeout=120,
        )
        cls.fixture_binary = cls.build_dir / "selection-fixtures"
        subprocess.run(
            ["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-O2",
             str(HERE / "test_vulkan_compute_bench.c"), "-o", str(cls.fixture_binary)],
            check=True, capture_output=True, text=True, timeout=30,
        )

    @classmethod
    def tearDownClass(cls) -> None:
        cls.temp.cleanup()

    def run_bench(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(self.binary), "--shader", str(self.binary) + ".spv", *args],
            capture_output=True, text=True, timeout=45,
        )

    def test_synthetic_device_selection_fixtures(self) -> None:
        completed = subprocess.run([str(self.fixture_binary)], check=True, capture_output=True, text=True)
        self.assertIn("10 synthetic device-selection fixtures passed", completed.stdout)

    def test_rejects_workload_outside_explicit_bounds(self) -> None:
        for args in (("--count", "0"), ("--count", "16777217"),
                     ("--iterations", "101"), ("--count", "1000000", "--iterations", "101")):
            with self.subTest(args=args):
                completed = self.run_bench("--vendor", "any", *args)
                self.assertEqual(completed.returncode, 2)
                self.assertIn("Work bounds exceeded", completed.stderr)
                self.assertEqual(completed.stdout, "")

    def test_rejects_unknown_vendor(self) -> None:
        completed = self.run_bench("--vendor", "nouveau")
        self.assertEqual(completed.returncode, 2)
        self.assertIn("Unsupported vendor filter", completed.stderr)
        self.assertEqual(completed.stdout, "")

    def test_sandbox_cpu_vulkan_is_not_accepted_as_hardware(self) -> None:
        completed = self.run_bench("--vendor", "any", "--count", "64", "--iterations", "1")
        if completed.returncode == 0:
            record = json.loads(completed.stdout)
            self.assertEqual(record["status"], "pass")
            self.assertIn(record["device_type"], ("discrete", "integrated"))
            self.assertEqual(record["count"], 64)
            self.assertEqual(record["dispatches"], 1)
            self.assertEqual(record["verified_outputs"], 64)
            self.assertGreater(record["gpu_elapsed_ns"], 0)
            self.assertGreater(record["host_elapsed_ns"], 0)
        else:
            self.assertEqual(completed.stdout, "", "rejected/software runs must not emit benchmark JSON")
            self.assertTrue(
                "No matching discrete/integrated GPU" in completed.stderr
                or "No GPU matched" in completed.stderr
                or "Vulkan physical devices" in completed.stderr,
                completed.stderr,
            )


if __name__ == "__main__":
    unittest.main()
