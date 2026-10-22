#!/usr/bin/env python3
"""Verify OSTD virtual-address reservation runtime evidence, never execution."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_MODULE_ADDRESS:"
CANONICAL_START = 0xFFFF800000000000
CANONICAL_END = 0xFFFFFFFFFFFF0000  # exclusive
HEX = r"0x[0-9a-fA-F]+"
KEYS = ("first_start", "first_end", "second_start", "second_end",
        "existing_start", "existing_end", "pool_start", "pool_end")


def _unique_line(text: str, pattern: str, label: str):
    matches = [line for line in text.splitlines() if re.fullmatch(pattern, line)]
    if len(matches) != 1:
        raise ValueError(f"Expected exactly one complete {label} marker")
    return matches[0]


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if re.search(r"\b(?:panicked|panic)\b|\bfailure\b|test result:\s*FAILED\b|assertion\b[^\n]*failed\b", text, re.I):
        raise ValueError("Runtime log contains panic or failure evidence")
    validation = _unique_line(text, re.escape(PREFIX + "validation:PASS cases=6"), "validation")
    requested_validation = _unique_line(text, re.escape(PREFIX + "requested_validation:PASS cases=8"), "requested validation")
    reserve_pattern = re.escape(PREFIX + "reserve:PASS ") + " ".join(rf"{key}=({HEX})" for key in KEYS)
    reserve = _unique_line(text, reserve_pattern, "reserve")
    values = [int(value, 16) for value in re.fullmatch(reserve_pattern, reserve).groups()]
    first_s, first_e, second_s, second_e, existing_s, existing_e, pool_s, pool_e = values
    unmapped = _unique_line(text, re.escape(PREFIX + "unmapped:PASS pages=6"), "unmapped")
    release_pattern = re.escape(PREFIX + "release:PASS ") + rf"start=({HEX}) end=({HEX})"
    release = _unique_line(text, release_pattern, "release")
    rel_s, rel_e = (int(x, 16) for x in re.fullmatch(release_pattern, release).groups())
    requested_pattern = re.escape(PREFIX + "requested_base:PASS ") + rf"start=({HEX}) end=({HEX}) pages=([0-9]+) conflicts=([0-9]+) releases=([0-9]+) conventional_exclusion=([0-9]+) unmapped=([0-9]+)"
    requested = _unique_line(text, requested_pattern, "requested-base reservation")
    req_s, req_e, *counts = re.fullmatch(requested_pattern, requested).groups()
    req_s, req_e = int(req_s, 16), int(req_e, 16)
    counts = [int(value) for value in counts]
    markers = [line for line in text.splitlines() if "CYBERMANJU_" in line]
    expected_markers = {validation, requested_validation, reserve, unmapped, release, requested}
    if len(markers) != 6 or set(markers) != expected_markers:
        raise ValueError("Unexpected or contradictory runtime marker")
    summary = [line for line in text.splitlines() if re.fullmatch(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.", line)]
    summary_lines = [line for line in text.splitlines() if "test result:" in line.lower()]
    if len(summary) != 1 or len(summary_lines) != 1:
        raise ValueError("Expected exactly one summary: 1 passed; 0 failed")
    bounds = [(pool_s, pool_e, "pool"), (first_s, first_e, "first"), (second_s, second_e, "second"), (existing_s, existing_e, "existing")]
    if not CANONICAL_START <= pool_s < pool_e <= CANONICAL_END:
        raise ValueError("Pool is outside canonical high-half range")
    for start, end, name in bounds:
        if not (CANONICAL_START <= start < end <= CANONICAL_END and pool_s <= start < end <= pool_e):
            raise ValueError(f"{name} range outside canonical pool or invalid/overflowed")
        if start % 4096 or end % 4096:
            raise ValueError(f"{name} range is not 4 KiB aligned")
    if first_e - first_s != 8192 or second_e - second_s != 12288 or existing_e - existing_s != 4096:
        raise ValueError("Reservation range has incorrect size")
    ranges = [(first_s, first_e), (second_s, second_e), (existing_s, existing_e)]
    if any(a < d and c < b for i, (a, b) in enumerate(ranges) for c, d in ranges[i+1:]):
        raise ValueError("Reservation ranges overlap")
    if (rel_s, rel_e) != (first_s, first_e):
        raise ValueError("Release range does not match first reservation")
    if not (CANONICAL_START <= req_s < req_e <= CANONICAL_END and pool_s <= req_s < req_e <= pool_e):
        raise ValueError("Requested-base range outside canonical pool or invalid/overflowed")
    if req_s % 4096 or req_e % 4096 or req_e - req_s != 8192:
        raise ValueError("Requested-base range is not aligned or exactly two pages")
    if (req_s, req_e) != (rel_s, rel_e):
        raise ValueError("Requested-base range does not match original release range")
    if counts != [2, 2, 2, 1, 1]:
        raise ValueError("Requested-base evidence has incorrect operation counts")
    return {"status": "passed", "linux_modules_executed": False, "pages_mapped": False,
            "frames_allocated": False, "mapping_available": False,
            "wx_verified": False, "module_execution_verified": False,
            "near_text_allocation_available": False,
            "requested_base_reservation_verified": True,
            "scope": "vmalloc_reservation_only", "runtime_tests": 1,
            "scenarios": 4, "failed": 0,
            "ranges": {name: {"start": hex(start), "end": hex(end)} for start, end, name in bounds}}


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    result = verify(path.read_text(errors="replace"))
    path.with_name("result.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Verified OSTD virtual-address reservation evidence; no payload pages mapped.")


if __name__ == "__main__":
    main()
