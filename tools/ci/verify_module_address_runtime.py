#!/usr/bin/env python3
"""Verify OSTD virtual-address reservation runtime evidence, never execution."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_MODULE_ADDRESS:"
CANONICAL_START = 0xFFFF800000000000
CANONICAL_END = 0xFFFFFFFFFFFF0000
NEAR_POOL_START = 0xFFFFFFFF40001000
NEAR_POOL_END = 0xFFFFFFFF7FFFF000
I32_MAX = 0x7FFFFFFF
HEX = r"0x[0-9a-fA-F]+"
KEYS = ("first_start", "first_end", "second_start", "second_end",
        "existing_start", "existing_end", "pool_start", "pool_end")


def _unique_line(text: str, pattern: str, label: str):
    matches = [line for line in text.splitlines() if re.fullmatch(pattern, line)]
    if len(matches) != 1:
        raise ValueError(f"Expected exactly one complete {label} marker")
    return matches[0]


def _overlap(a, b):
    return a[0] < b[1] and b[0] < a[1]


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

    near_pattern = (re.escape(PREFIX + "near_text_reserve:PASS ") +
                    rf"start=({HEX}) end=({HEX}) peer_start=({HEX}) peer_end=({HEX}) "
                    rf"pool_start=({HEX}) pool_end=({HEX}) kernel_start=({HEX}) kernel_end=({HEX}) code=({HEX})")
    near_line = _unique_line(text, near_pattern, "near-text reservation")
    near = [int(x, 16) for x in re.fullmatch(near_pattern, near_line).groups()]
    ns, ne, ps, pe, nps, npe, ks, ke, code = near
    checks_line = _unique_line(text, re.escape(PREFIX + "near_text_checks:PASS invalid_cases=9 conflicts=2 unmapped_pages=5 reclaimed=2 old_pool_rejected=true mapping=false execution=false"), "near-text checks")

    markers = [line for line in text.splitlines() if "CYBERMANJU_" in line]
    expected_markers = {validation, requested_validation, reserve, unmapped, release, requested, near_line, checks_line}
    if len(markers) != 8 or set(markers) != expected_markers:
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
    if first_e-first_s != 8192 or second_e-second_s != 12288 or existing_e-existing_s != 4096:
        raise ValueError("Reservation range has incorrect size")
    ranges = [(first_s, first_e), (second_s, second_e), (existing_s, existing_e)]
    if any(_overlap((a, b), (c, d)) for i, (a, b) in enumerate(ranges) for c, d in ranges[i+1:]):
        raise ValueError("Reservation ranges overlap")
    if (rel_s, rel_e) != (first_s, first_e):
        raise ValueError("Release range does not match first reservation")
    if not (CANONICAL_START <= req_s < req_e <= CANONICAL_END and pool_s <= req_s < req_e <= pool_e):
        raise ValueError("Requested-base range outside canonical pool or invalid/overflowed")
    if req_s % 4096 or req_e % 4096 or req_e-req_s != 8192 or (req_s,req_e)!=(rel_s,rel_e):
        raise ValueError("Requested-base range is not aligned, exactly two pages, or matching release")
    if counts != [2, 2, 2, 1, 1]:
        raise ValueError("Requested-base evidence has incorrect operation counts")

    if (nps, npe) != (NEAR_POOL_START, NEAR_POOL_END):
        raise ValueError("Near-text pool does not match exact required bounds")
    for start, end, name, size in ((ns,ne,"owner",8192),(ps,pe,"peer",12288)):
        if not (NEAR_POOL_START <= start < end <= NEAR_POOL_END) or start % 4096 or end % 4096 or end-start != size:
            raise ValueError(f"Near-text {name} range has invalid bounds, alignment, or size")
    if _overlap((ns,ne),(ps,pe)):
        raise ValueError("Near-text owner and peer overlap")
    if not (0xFFFFFFFF80000000 <= ks < ke <= CANONICAL_END):
        raise ValueError("Kernel image range outside allowed bounds")
    if code < ks or code >= ke:
        raise ValueError("Code address outside kernel image")
    if any(code - start < 0 or code - start > I32_MAX for start in (ns, ps)):
        raise ValueError("Invalid near-text code geometry")
    if _overlap((nps, npe), (pool_s, pool_e)):
        raise ValueError("Near-text and VMALLOC pools overlap")
    kernel = (ks,ke)
    if any(_overlap(r,kernel) for r in ((ns,ne),(ps,pe),(first_s,first_e),(second_s,second_e),(existing_s,existing_e),(req_s,req_e))):
        raise ValueError("Near-text/kernel range overlaps an existing reservation or image")
    return {"status":"passed", "linux_modules_executed":False, "pages_mapped":False,
            "frames_allocated":False, "mapping_available":False, "wx_verified":False,
            "module_execution_verified":False, "near_text_va_reservation_verified":True,
            "near_text_loader_integration_available":False, "near_text_allocation_available":True,
            "near_text_scope":"typed_va_reservation_only", "near_text_ranges":{
                "owner":{"start":hex(ns),"end":hex(ne)}, "peer":{"start":hex(ps),"end":hex(pe)},
                "pool":{"start":hex(nps),"end":hex(npe)}, "kernel":{"start":hex(ks),"end":hex(ke)}, "code":hex(code)},
            "requested_base_reservation_verified":True, "scope":"va_reservations_only",
            "runtime_tests":1,"scenarios":6,"failed":0,
            "ranges":{name:{"start":hex(start),"end":hex(end)} for start,end,name in bounds}}


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    result = verify(path.read_text(errors="replace"))
    path.with_name("result.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Verified OSTD virtual-address reservation evidence; no payload pages mapped.")


if __name__ == "__main__":
    main()
