#!/usr/bin/env python3
"""Summarize GPUI shell renderer and frame telemetry records from a log.

Frame telemetry measures bar render-entry observations, not presented frames.
This tool intentionally never treats those observations as physical FPS or as
proof that the selected renderer is hardware accelerated.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
from pathlib import Path
from typing import Any

RENDERER_MARKER = "GPUI_SHELL_RENDERER "
TELEMETRY_MARKER = "GPUI_SHELL_FRAME_TELEMETRY "
ADAPTER_MARKER = "Selected GPU adapter:"
ADAPTER_RE = re.compile(
    r'Selected GPU adapter:\s*("(?:\\.|[^"\\])*")\s*\((Vulkan|Gl)\)\s*$'
)
ANSI_RE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
CLASSIFICATIONS = {"software_cpu", "virtual_or_remote", "unverified_non_cpu", "unknown"}
PERCENTILES = ("p50", "p95", "p99", "max")


class AnalysisError(ValueError):
    """The log is missing required records or contains invalid telemetry."""


def _record_json(line: str, marker: str, line_number: int) -> dict[str, Any]:
    payload = line.split(marker, 1)[1].strip()
    try:
        value = json.loads(payload)
    except (json.JSONDecodeError, UnicodeError) as exc:
        raise AnalysisError(f"line {line_number}: malformed JSON after {marker.strip()}: {exc}") from exc
    if not isinstance(value, dict):
        raise AnalysisError(f"line {line_number}: {marker.strip()} record must be a JSON object")
    return value


def _finite_number(value: Any, label: str, *, allow_zero: bool = True) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise AnalysisError(f"{label} must be a finite number")
    try:
        number = float(value)
    except (OverflowError, ValueError) as exc:
        raise AnalysisError(f"{label} must be a finite number") from exc
    if not math.isfinite(number) or number < 0 or (not allow_zero and number == 0):
        qualifier = "positive and finite" if not allow_zero else "finite and non-negative"
        raise AnalysisError(f"{label} must be {qualifier}")
    return number


def _nonnegative_int(value: Any, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise AnalysisError(f"{label} must be a non-negative integer")
    return value


def _validate_distribution(value: Any, label: str, *, positive: bool = False) -> dict[str, float] | None:
    if value is None:
        return None
    if not isinstance(value, dict):
        raise AnalysisError(f"{label} must be null or a percentile object")
    result = {
        key: _finite_number(value.get(key), f"{label}.{key}", allow_zero=not positive)
        for key in PERCENTILES
    }
    if not result["p50"] <= result["p95"] <= result["p99"] <= result["max"]:
        raise AnalysisError(f"{label} percentiles must be non-decreasing (p50 <= p95 <= p99 <= max)")
    return result


def _validate_telemetry(record: dict[str, Any], line_number: int) -> dict[str, Any]:
    prefix = f"line {line_number} telemetry"
    if record.get("schema") != 1:
        raise AnalysisError(f"{prefix}: unsupported or missing schema (expected 1)")
    if record.get("surface") != "bar" or record.get("metric") != "bar_render_entry":
        raise AnalysisError(f"{prefix}: expected surface=bar and metric=bar_render_entry")
    if not isinstance(record.get("window"), str) or not record["window"]:
        raise AnalysisError(f"{prefix}: missing window identity")
    mode = record.get("mode")
    probe_seconds = record.get("probe_seconds")
    probe_finished = record.get("probe_finished")
    if mode == "passive":
        if probe_seconds is not None or probe_finished is not False:
            raise AnalysisError(f"{prefix}: passive mode requires probe_seconds=null and probe_finished=false")
    elif mode == "bounded_redraw_probe":
        if (isinstance(probe_seconds, bool) or not isinstance(probe_seconds, int)
                or not 1 <= probe_seconds <= 60):
            raise AnalysisError(f"{prefix}: bounded_redraw_probe requires probe_seconds from 1 through 60")
        if not isinstance(probe_finished, bool):
            raise AnalysisError(f"{prefix}: probe_finished must be a boolean")
    else:
        raise AnalysisError(f"{prefix}: mode must be passive or bounded_redraw_probe")
    if record.get("includes_idle_gaps") is not True:
        raise AnalysisError(f"{prefix}: includes_idle_gaps must be true")
    if record.get("physical_presentation_fps", "missing") is not None:
        raise AnalysisError(f"{prefix}: physical_presentation_fps must be null")
    if record.get("hardware_acceleration_verified") is not False:
        raise AnalysisError(f"{prefix}: hardware_acceleration_verified must be false")

    elapsed = _finite_number(record.get("elapsed_seconds"), f"{prefix}.elapsed_seconds")
    entries = _nonnegative_int(record.get("sample_entries"), f"{prefix}.sample_entries")
    intervals = _nonnegative_int(record.get("sample_intervals"), f"{prefix}.sample_intervals")
    dropped = _nonnegative_int(record.get("dropped_samples"), f"{prefix}.dropped_samples")
    invalid = _nonnegative_int(record.get("invalid_intervals"), f"{prefix}.invalid_intervals")
    if entries == 0:
        raise AnalysisError(f"{prefix}: telemetry observation has no samples")
    if intervals > entries:
        raise AnalysisError(f"{prefix}: sample_intervals cannot exceed sample_entries")

    rate = record.get("render_entry_rate_hz")
    if intervals == 0:
        if rate is not None:
            raise AnalysisError(f"{prefix}.render_entry_rate_hz must be null when there are no valid intervals")
    else:
        rate = _finite_number(rate, f"{prefix}.render_entry_rate_hz", allow_zero=False)

    interval_distribution = _validate_distribution(
        record.get("render_entry_interval_ms"), f"{prefix}.render_entry_interval_ms", positive=True
    )
    build_distribution = _validate_distribution(
        record.get("view_build_wall_ms"), f"{prefix}.view_build_wall_ms"
    )
    if (interval_distribution is None) != (intervals == 0):
        raise AnalysisError(f"{prefix}: interval percentiles must be present exactly when valid intervals exist")
    if build_distribution is None:
        raise AnalysisError(f"{prefix}: view-build percentiles are required for non-empty samples")

    # Return source fields as emitted, after validating every timing value.
    return {
        "window": record["window"],
        "mode": mode,
        "probe_seconds": probe_seconds,
        "probe_finished": probe_finished,
        "elapsed_seconds": elapsed,
        "sample_entries": entries,
        "sample_intervals": intervals,
        "dropped_samples": dropped,
        "invalid_intervals": invalid,
        # This is a render-entry statistic only, never presentation FPS.
        "render_entry_rate_hz": rate,
        "render_entry_interval_ms": interval_distribution,
        "view_build_wall_ms": build_distribution,
        "includes_idle_gaps": True,
        "physical_presentation_fps": None,
        "hardware_acceleration_verified": False,
    }


def _normalized_device_name(name: str) -> str:
    return " ".join(name.split()).casefold()


def _renderer_classification(record: dict[str, Any], line_number: int) -> tuple[str, dict[str, Any] | None, str]:
    prefix = f"line {line_number} renderer"
    if record.get("schema") != 1:
        raise AnalysisError(f"line {line_number}: unsupported or missing renderer schema (expected 1)")
    if record.get("api") != "wgpu":
        raise AnalysisError(f"line {line_number}: renderer api must be wgpu")
    if not isinstance(record.get("window"), str) or not record["window"]:
        raise AnalysisError(f"{prefix}: missing window identity")
    if record.get("backend") is not None:
        raise AnalysisError(f"{prefix}: backend must be null when it is not exposed by GpuSpecs")
    if not isinstance(record.get("backend_source"), str) or not record["backend_source"].strip():
        raise AnalysisError(f"{prefix}: missing backend_source")
    if record.get("physical_presentation_timestamps_available") is not False:
        raise AnalysisError(f"{prefix}: physical_presentation_timestamps_available must be false")
    presentation_fields = (
        "presentation_feedback_status", "presentation_feedback_reason", "host_monitor_scanout_verified"
    )
    if any(field in record for field in presentation_fields):
        if record.get("presentation_feedback_status") != "unavailable_locked_gpui":
            raise AnalysisError(f"{prefix}: presentation_feedback_status must be unavailable_locked_gpui")
        if record.get("host_monitor_scanout_verified") is not False:
            raise AnalysisError(f"{prefix}: host_monitor_scanout_verified must be false")
        reason = record.get("presentation_feedback_reason")
        if not isinstance(reason, str) or not reason.strip():
            raise AnalysisError(f"{prefix}: missing presentation_feedback_reason")
    classification = record.get("classification")
    if not isinstance(classification, str) or classification not in CLASSIFICATIONS:
        raise AnalysisError(f"line {line_number}: missing or unknown renderer classification")
    device = record.get("device")
    if device is not None and not isinstance(device, dict):
        raise AnalysisError(f"{prefix}: device specs must be an object or null")
    device_name = ""
    if isinstance(device, dict):
        candidate = device.get("device_name")
        if candidate is not None and not isinstance(candidate, str):
            raise AnalysisError(f"{prefix}: device_name must be a string")
        device_name = candidate or ""
    if record.get("hardware_acceleration_verified") is not False:
        raise AnalysisError(f"{prefix}: hardware_acceleration_verified must be false")
    return classification, device, device_name


def _adapter_records(lines: list[str]) -> list[dict[str, str]]:
    adapters: list[dict[str, str]] = []
    for line_number, original in enumerate(lines, 1):
        line = ANSI_RE.sub("", original).rstrip("\r\n")
        if ADAPTER_MARKER not in line:
            continue
        match = ADAPTER_RE.search(line)
        if not match:
            raise AnalysisError(f"line {line_number}: malformed Selected GPU adapter record")
        try:
            name = json.loads(match.group(1))
        except json.JSONDecodeError as exc:
            raise AnalysisError(f"line {line_number}: malformed quoted adapter name: {exc}") from exc
        adapters.append({"device": name, "backend": match.group(2)})
    if not adapters:
        raise AnalysisError("missing upstream Selected GPU adapter record")
    return adapters


def analyze_text(text: str) -> dict[str, Any]:
    """Parse and validate one complete GPUI log into a JSON-serializable summary."""
    lines = text.splitlines()
    renderer_records: list[tuple[dict[str, Any], int]] = []
    telemetry_records: list[tuple[dict[str, Any], int]] = []
    for line_number, original in enumerate(lines, 1):
        line = ANSI_RE.sub("", original)
        if RENDERER_MARKER in line:
            renderer_records.append((_record_json(line, RENDERER_MARKER, line_number), line_number))
        if TELEMETRY_MARKER in line:
            telemetry_records.append((_record_json(line, TELEMETRY_MARKER, line_number), line_number))
    if not renderer_records:
        raise AnalysisError("missing GPUI_SHELL_RENDERER record")
    if not telemetry_records:
        raise AnalysisError("missing GPUI_SHELL_FRAME_TELEMETRY record; no telemetry observations")

    adapters = _adapter_records(lines)
    parsed_renderers = [
        (record, line_number, *_renderer_classification(record, line_number))
        for record, line_number in renderer_records
    ]
    adapters_by_name: dict[str, dict[tuple[str, str], dict[str, str]]] = {}
    for adapter in adapters:
        key = _normalized_device_name(adapter["device"])
        adapters_by_name.setdefault(key, {})[(key, adapter["backend"])] = adapter

    renderer_by_window: dict[str, dict[str, Any]] = {}
    for parsed in parsed_renderers:
        record, line_number, classification, device, device_name = parsed
        normalized_name = _normalized_device_name(device_name)
        matches = adapters_by_name.get(normalized_name, {}) if normalized_name else {}
        if not matches:
            names = [item["device"] for item in adapters]
            raise AnalysisError(
                f"line {line_number}: no upstream Selected GPU adapter device name exactly matches "
                f"renderer device_name {device_name!r}; upstream adapters: {names!r}"
            )
        if len(matches) != 1:
            backends = sorted({item["backend"] for item in matches.values()})
            raise AnalysisError(
                f"line {line_number}: ambiguous upstream adapter for renderer device_name "
                f"{device_name!r} across backends {backends!r}"
            )
        adapter = next(iter(matches.values()))
        window = record["window"]
        identity = {
            "window": window,
            "specs": device,
            "device_name": device_name,
            "classification": classification,
            "backend": adapter["backend"],
            "backend_source": record["backend_source"],
            "upstream_selected_adapter": adapter,
            "backend_device_match": True,
            "hardware_acceleration_verified": False,
            "presentation_feedback_status": record.get("presentation_feedback_status", "not_recorded"),
            "presentation_feedback_reason": record.get("presentation_feedback_reason"),
            "host_monitor_scanout_verified": False,
        }
        previous = renderer_by_window.get(window)
        if previous is not None and previous != identity:
            raise AnalysisError(f"line {line_number}: conflicting renderer identities for window {window!r}")
        renderer_by_window[window] = identity

    observations = [
        _validate_telemetry(record, line_number)
        for record, line_number in telemetry_records
    ]
    for observation in observations:
        if observation["window"] not in renderer_by_window:
            raise AnalysisError(
                f"telemetry window {observation['window']!r} has no matching GPUI_SHELL_RENDERER record"
            )

    reasons = {
        "software_cpu": "The selected renderer is classified as software CPU rendering; it is not physical-GPU evidence.",
        "virtual_or_remote": "The selected renderer is virtual or remote; it is not physical-GPU evidence.",
        "unknown": "The renderer type is unknown; physical-GPU acceleration cannot be established.",
        "unverified_non_cpu": (
            "The device is not identified as CPU software, but GpuSpecs exposes neither adapter type nor PCI identity; "
            "this classification cannot certify a physical GPU or hardware acceleration."
        ),
    }
    limitations = [
        "GPUI bar_render_entry records measure render-entry observations, not completed GPU work, compositor presentation, or scanout.",
        "render_entry_rate_hz is a render-entry statistic and is not presentation FPS.",
        "view_build_wall_ms is elapsed wall time for element-tree construction, not thread CPU time.",
        "Physical presentation timestamps are unavailable; physical_presentation_fps is null.",
        "hardware_acceleration_verified is false: GpuSpecs has no adapter type or PCI identity, so hardware acceleration is not verified.",
    ]
    identities = list(renderer_by_window.values())
    distinct_identities = {
        json.dumps({key: value for key, value in identity.items() if key != "window"}, sort_keys=True)
        for identity in identities
    }
    consistent_identity = len(distinct_identities) == 1
    classifications = {identity["classification"] for identity in identities}
    classification = next(iter(classifications)) if len(classifications) == 1 else "mixed"
    result = {
        "renderer_by_window": identities,
        "telemetry": {"observation_count": len(observations), "observations": observations},
        "physical_presentation_fps": None,
        "hardware_acceleration_verified": False,
        "physical_gpu_certification": {
            "verified": False,
            "classification": classification,
            "reason": (reasons[classification] if classification in reasons else
                       "Renderer identities differ across windows; no singular renderer classification applies."),
        },
        "not_hardware_performance_evidence": True,
        "limitations": limitations,
    }
    if consistent_identity:
        identity = identities[0]
        result["renderer"] = {key: value for key, value in identity.items() if key != "window"}
        result["physical_gpu_certification"]["classification"] = identity["classification"]
        result["physical_gpu_certification"]["reason"] = reasons[identity["classification"]]
    return result


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log_path", type=Path, help="path to the GPUI application log")
    parser.add_argument(
        "--require-physical-gpu",
        action="store_true",
        help="fail unless physical GPU acceleration is verified (these records cannot verify it)",
    )
    args = parser.parse_args(argv)
    try:
        text = args.log_path.read_text(encoding="utf-8")
        summary = analyze_text(text)
    except (OSError, UnicodeError, AnalysisError) as exc:
        print(f"frame telemetry analysis failed: {exc}", file=sys.stderr)
        return 2

    if args.require_physical_gpu:
        summary["physical_gpu_certification"]["strict_requirement_met"] = False
        summary["physical_gpu_certification"]["strict_requirement_reason"] = (
            "Physical GPU acceleration cannot be certified from this log: GpuSpecs lacks adapter type and PCI identity; "
            "software_cpu, virtual_or_remote, and unknown devices are rejected outright, and unverified_non_cpu is "
            "also insufficient. There is no override."
        )
    print(json.dumps(summary, ensure_ascii=False, indent=2, allow_nan=False))
    if args.require_physical_gpu:
        print("physical GPU requirement not met: hardware acceleration is not verified", file=sys.stderr)
        return 3
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
