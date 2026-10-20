#!/usr/bin/env python3
"""Validate CI-run Asterinas native-GPU evidence in a VM serial log.

This checks a *runner-controlled log chain*, not cryptographic attestation. A
host with control of the runner, VM console, or guest can forge every marker and
benchmark field. In particular, uname reporting Linux is never accepted as
proof that the guest kernel is Linux or Asterinas.
"""
from __future__ import annotations

import argparse
import json
import math
import re
import sys
from pathlib import Path
from typing import Any

MAX_COUNT = 16_777_216
MAX_DISPATCHES = 100
MAX_TOTAL_INVOCATIONS = 100_000_000
EXPECTED_NVIDIA_RELEASE = "615.78.08"
_HEX40 = re.compile(r"^[0-9a-fA-F]{40}$")
_HEX32 = re.compile(r"^[0-9a-fA-F]{32}$")
_BOOT = re.compile(
    r"ASTERINAS_NATIVE_GPU_BOOT\s+commit=([0-9a-fA-F]{40})\s+nonce=([0-9a-fA-F]{32})(?=\s|$)"
)
_RESULT = re.compile(r"ASTERINAS_NATIVE_GPU_RESULT\s+nonce=([0-9a-fA-F]{32})\s+(.+?)\s*$")
_EXIT = re.compile(r"ASTERINAS_NATIVE_GPU_EXIT\s+nonce=([0-9a-fA-F]{32})\s+status=([^\s]+)\s*$")
_SOFTWARE_HINT = re.compile(
    r"(?:llvmpipe|lavapipe|swiftshader|software\s+render|cpu\s+device|\bvirtual\s+gpu\b)",
    re.IGNORECASE,
)
_VERSION = re.compile(r"(?<!\d)(\d{3,4}\.\d{1,3}\.\d{1,3})(?!\d)")


class EvidenceError(ValueError):
    """The serial log does not satisfy the evidence contract."""


def _unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise EvidenceError(f"benchmark JSON has duplicate key: {key}")
        result[key] = value
    return result


def _reject_json_constant(value: str) -> None:
    raise EvidenceError(f"benchmark JSON contains non-standard numeric constant {value}")


def _positive_integer(record: dict[str, Any], key: str) -> int:
    value = record.get(key)
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise EvidenceError(f"{key} must be a positive integer (booleans are not integers here)")
    return value


def _positive_finite_number(record: dict[str, Any], key: str) -> int | float:
    value = record.get(key)
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise EvidenceError(f"{key} must be a finite positive number")
    if not math.isfinite(value) or value <= 0:
        raise EvidenceError(f"{key} must be a finite positive number")
    return value


def _validate_benchmark(raw: str) -> dict[str, Any]:
    try:
        record = json.loads(
            raw,
            object_pairs_hook=_unique_object,
            parse_constant=_reject_json_constant,
        )
    except EvidenceError:
        raise
    except (json.JSONDecodeError, TypeError, ValueError) as exc:
        raise EvidenceError(f"benchmark result is not valid JSON: {exc}") from exc
    if not isinstance(record, dict):
        raise EvidenceError("benchmark result must be a JSON object")

    if record.get("status") != "pass":
        raise EvidenceError(f"benchmark did not pass (status={record.get('status')!r})")
    vendor_id = record.get("vendor_id")
    if not isinstance(vendor_id, str) or vendor_id.lower() != "0x10de":
        raise EvidenceError("vendor_id must identify NVIDIA (0x10de)")
    if record.get("device_type") not in ("discrete", "integrated"):
        raise EvidenceError("device_type must be discrete or integrated; software/other devices are rejected")
    if record.get("driver_id_name") != "nvidia-proprietary":
        raise EvidenceError("driver_id_name must be nvidia-proprietary")

    metadata = " ".join(
        str(record.get(key, "")) for key in ("device", "driver", "driver_info")
    )
    if _SOFTWARE_HINT.search(metadata):
        raise EvidenceError("software/virtual GPU indicators are not accepted")

    # The native port requires matching firmware/userspace. Unknown version is
    # not a pass: require the dotted release exposed in NVIDIA driver metadata.
    # Do not reinterpret uname or invent a version from a missing numeric field.
    driver_info = record.get("driver_info", "")
    driver_name = record.get("driver", "")
    if not isinstance(driver_info, str) or not isinstance(driver_name, str):
        raise EvidenceError("driver and driver_info must be strings when present")
    disclosed_versions = _VERSION.findall(driver_info + " " + driver_name)
    if EXPECTED_NVIDIA_RELEASE not in disclosed_versions:
        raise EvidenceError(
            f"driver metadata discloses {', '.join(disclosed_versions)}, not expected NVIDIA release "
            f"{EXPECTED_NVIDIA_RELEASE}"
        )

    count = _positive_integer(record, "count")
    dispatches = _positive_integer(record, "dispatches")
    verified = _positive_integer(record, "verified_outputs")
    if count > MAX_COUNT:
        raise EvidenceError(f"count exceeds benchmark limit {MAX_COUNT}")
    if dispatches > MAX_DISPATCHES:
        raise EvidenceError(f"dispatches exceeds benchmark limit {MAX_DISPATCHES}")
    if count * dispatches > MAX_TOTAL_INVOCATIONS:
        raise EvidenceError(f"count*dispatches exceeds benchmark limit {MAX_TOTAL_INVOCATIONS}")
    if verified != count:
        raise EvidenceError("verified_outputs must equal count")

    _positive_finite_number(record, "gpu_elapsed_ns")
    _positive_finite_number(record, "host_elapsed_ns")
    return record


def validate_serial_log(serial_text: str, commit: str, nonce: str) -> dict[str, Any]:
    """Validate markers and benchmark content; return a JSON-serializable report."""
    if not isinstance(commit, str) or not _HEX40.fullmatch(commit):
        raise EvidenceError("--commit must be exactly 40 hexadecimal characters")
    if not isinstance(nonce, str) or not _HEX32.fullmatch(nonce):
        raise EvidenceError("--nonce must be exactly 32 hexadecimal characters")
    expected_commit = commit.lower()
    expected_nonce = nonce.lower()

    boots: list[tuple[int, str]] = []
    results: list[tuple[int, str]] = []
    exits: list[tuple[int, str]] = []
    for line_number, line in enumerate(serial_text.splitlines(), start=1):
        for match in _BOOT.finditer(line):
            if match.group(2).lower() == expected_nonce:
                boots.append((line_number, match.group(1).lower()))
        for match in _RESULT.finditer(line):
            if match.group(1).lower() == expected_nonce:
                results.append((line_number, match.group(2)))
        for match in _EXIT.finditer(line):
            if match.group(1).lower() == expected_nonce:
                exits.append((line_number, match.group(2)))

    if not boots:
        raise EvidenceError("missing ASTERINAS_NATIVE_GPU_BOOT marker for requested nonce")
    if len(boots) != 1:
        raise EvidenceError("expected exactly one boot marker for requested nonce")
    boot_line, boot_commit = boots[0]
    if boot_commit != expected_commit:
        raise EvidenceError(
            f"boot marker commit mismatch: expected {expected_commit}, got {boot_commit}"
        )

    if not results:
        raise EvidenceError("missing ASTERINAS_NATIVE_GPU_RESULT marker for requested nonce")
    if len(results) != 1:
        raise EvidenceError("expected exactly one benchmark result for requested nonce")
    result_line, json_text = results[0]

    if not exits:
        raise EvidenceError("missing ASTERINAS_NATIVE_GPU_EXIT marker for requested nonce")
    if len(exits) != 1:
        raise EvidenceError("expected exactly one exit marker for requested nonce")
    exit_line, exit_status = exits[0]
    if exit_status != "0":
        raise EvidenceError(f"guest reported nonzero exit status {exit_status!r}")
    if not (boot_line < result_line < exit_line):
        raise EvidenceError("required marker order is boot, benchmark result, then successful exit")

    benchmark = _validate_benchmark(json_text)
    return {
        "status": "validated",
        # This is a scope annotation produced by the validator, not evidence
        # that GPUI or a Wayland compositor was exercised.
        "scope": "Asterinas-native NVIDIA Vulkan compute only; not GPUI/Wayland acceptance",
        "full_gpui_wayland_vulkan_acceptance": False,
        "commit": expected_commit,
        "nonce": expected_nonce,
        "benchmark": benchmark,
        "serial_lines": {"boot": boot_line, "result": result_line, "exit": exit_line},
        "trust_boundary": (
            "Evidence is a runner-controlled serial-log chain, not cryptographic attestation; "
            "a party controlling the runner, VM console, or guest can forge it. uname is not proof."
        ),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True, type=Path, help="VM serial log path")
    parser.add_argument("--commit", required=True, help="expected 40-hex CI build commit")
    parser.add_argument("--nonce", required=True, help="expected 32-hex CI run nonce")
    parser.add_argument("--output", required=True, type=Path, help="write validated evidence JSON here")
    args = parser.parse_args(argv)
    try:
        serial_text = args.serial.read_text(encoding="utf-8", errors="strict")
        report = validate_serial_log(serial_text, args.commit, args.nonce)
        output = json.dumps(report, indent=2, sort_keys=True, allow_nan=False) + "\n"
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(output, encoding="utf-8")
    except (OSError, UnicodeError, EvidenceError) as exc:
        print(f"native GPU evidence rejected: {exc}", file=sys.stderr)
        return 1
    print(f"native GPU evidence validated: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
