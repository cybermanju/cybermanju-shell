#!/usr/bin/env python3
"""Synthetic fixtures for the serial evidence validator; these claim no GPU run."""
from __future__ import annotations

import json
import unittest

from validate_native_gpu_evidence import EvidenceError, validate_serial_log

COMMIT = "0123456789abcdef0123456789abcdef01234567"
NONCE = "abcdef0123456789abcdef0123456789"
OTHER_NONCE = "11111111111111111111111111111111"
OTHER_COMMIT = "fedcba9876543210fedcba9876543210fedcba98"


def synthetic_benchmark(**overrides: object) -> dict[str, object]:
    """Return a synthetic JSON record shaped like vulkan_compute_bench.c output."""
    record: dict[str, object] = {
        "status": "pass",
        "device": "Synthetic NVIDIA test fixture",
        "vendor": "NVIDIA",
        "vendor_id": "0x10de",
        "device_type": "discrete",
        "driver": "Synthetic NVIDIA driver",
        "driver_info": "Synthetic NVIDIA 615.78.08 fixture",
        "driver_id": 4,
        "driver_id_name": "nvidia-proprietary",
        "os": "Linux synthetic-fixture 0.0.0 test",
        "count": 64,
        "dispatches": 2,
        "verified_outputs": 64,
        "gpu_elapsed_ns": 17.25,
        "host_elapsed_ns": 90,
    }
    record.update(overrides)
    return record


def synthetic_log(
    *,
    benchmark: dict[str, object] | None = None,
    boot_commit: str = COMMIT,
    boot_nonce: str = NONCE,
    result_nonce: str = NONCE,
    exit_nonce: str = NONCE,
    exit_status: str = "0",
    include_boot: bool = True,
    include_result: bool = True,
    include_exit: bool = True,
) -> str:
    """Construct a clearly synthetic serial transcript."""
    lines: list[str] = ["[synthetic test fixture; not output from a VM]"]
    if include_boot:
        lines.append(f"ASTERINAS_NATIVE_GPU_BOOT commit={boot_commit} nonce={boot_nonce}")
    if include_result:
        result_json = json.dumps(benchmark if benchmark is not None else synthetic_benchmark())
        lines.append(f"ASTERINAS_NATIVE_GPU_RESULT nonce={result_nonce} {result_json}")
    if include_exit:
        lines.append(f"ASTERINAS_NATIVE_GPU_EXIT nonce={exit_nonce} status={exit_status}")
    return "\n".join(lines) + "\n"


class NativeGpuEvidenceTests(unittest.TestCase):
    def assert_rejected(self, transcript: str, message: str | None = None) -> None:
        with self.assertRaises(EvidenceError) as caught:
            validate_serial_log(transcript, COMMIT, NONCE)
        if message is not None:
            self.assertIn(message, str(caught.exception))

    def test_accepts_synthetic_valid_chain_and_linux_uname_is_not_evidence(self) -> None:
        report = validate_serial_log(synthetic_log(), COMMIT, NONCE)
        self.assertEqual(report["status"], "validated")
        self.assertEqual(
            report["scope"],
            "Asterinas-native NVIDIA Vulkan compute only; not GPUI/Wayland acceptance",
        )
        self.assertIs(report["full_gpui_wayland_vulkan_acceptance"], False)
        benchmark = report["benchmark"]
        self.assertEqual(benchmark["os"], "Linux synthetic-fixture 0.0.0 test")
        self.assertIn("not cryptographic attestation", report["trust_boundary"])

    def test_rejects_missing_boot_marker(self) -> None:
        self.assert_rejected(synthetic_log(include_boot=False), "missing ASTERINAS_NATIVE_GPU_BOOT")

    def test_rejects_plain_linux_benchmark_without_native_prefixes(self) -> None:
        linux_only = "Linux host uname output\n" + json.dumps(synthetic_benchmark()) + "\n"
        self.assert_rejected(linux_only, "missing ASTERINAS_NATIVE_GPU_BOOT")

    def test_rejects_wrong_nonce(self) -> None:
        self.assert_rejected(
            synthetic_log(boot_nonce=OTHER_NONCE, result_nonce=OTHER_NONCE, exit_nonce=OTHER_NONCE),
            "missing ASTERINAS_NATIVE_GPU_BOOT",
        )

    def test_rejects_zero_or_nonfinite_timings(self) -> None:
        for field, value, message in (
            ("gpu_elapsed_ns", 0, "gpu_elapsed_ns"),
            ("host_elapsed_ns", float("nan"), "non-standard numeric constant NaN"),
        ):
            with self.subTest(field=field):
                self.assert_rejected(
                    synthetic_log(benchmark=synthetic_benchmark(**{field: value})),
                    message,
                )

    def test_rejects_mismatched_commit(self) -> None:
        self.assert_rejected(synthetic_log(boot_commit=OTHER_COMMIT), "commit mismatch")

    def test_rejects_bad_hardware_identity_and_software(self) -> None:
        for changes, message in (
            ({"vendor_id": "0x1002"}, "vendor_id"),
            ({"device_type": "virtual"}, "device_type"),
            ({"driver_id_name": "mesa-radv"}, "driver_id_name"),
            ({"device": "llvmpipe software adapter"}, "software/virtual"),
        ):
            with self.subTest(changes=changes):
                self.assert_rejected(synthetic_log(benchmark=synthetic_benchmark(**changes)), message)

    def test_rejects_duplicate_result_for_nonce(self) -> None:
        transcript = synthetic_log()
        duplicate = f"ASTERINAS_NATIVE_GPU_RESULT nonce={NONCE} {json.dumps(synthetic_benchmark())}\n"
        transcript = transcript.replace("ASTERINAS_NATIVE_GPU_EXIT", duplicate + "ASTERINAS_NATIVE_GPU_EXIT")
        self.assert_rejected(transcript, "exactly one benchmark result")

    def test_rejects_boot_commit_mismatch_and_bad_work_bounds(self) -> None:
        self.assert_rejected(synthetic_log(boot_commit=OTHER_COMMIT), "commit mismatch")
        self.assert_rejected(
            synthetic_log(benchmark=synthetic_benchmark(count=16_777_217)),
            "count exceeds benchmark limit",
        )

    def test_rejects_wrong_marker_order_and_nonzero_guest_exit(self) -> None:
        normal = synthetic_log()
        lines = normal.splitlines()
        reordered = "\n".join((lines[0], lines[2], lines[1], lines[3]))
        self.assert_rejected(reordered, "marker order")
        self.assert_rejected(synthetic_log(exit_status="1"), "nonzero exit status")

    def test_rejects_unknown_or_mismatched_nvidia_release(self) -> None:
        for info in ("", "release not exposed", "NVIDIA 590.44.01"):
            with self.subTest(driver_info=info):
                self.assert_rejected(
                    synthetic_log(benchmark=synthetic_benchmark(driver_info=info)),
                    "not expected NVIDIA release",
                )

if __name__ == "__main__":
    unittest.main()
