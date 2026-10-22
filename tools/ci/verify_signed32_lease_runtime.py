#!/usr/bin/env python3
"""Verify experimental single-CPU signed32 persistent-lease runtime evidence only."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_SIGNED32_LEASE:"
BEGIN = PREFIX + "begin:PASS cpus=1"
PERSISTENT = PREFIX + ("persistent:PASS span=8192 pages=2 envelope_pages=3 bytes=8212 reads=3 "
    "pte_pages=2 frame_matches=2 rw_nonx=2 supervisor=2 writeback=2 global=2 "
    "slack_absent=1 irq_preserved=true exclusive=true")
FAULTS = PREFIX + "faults:PASS cases=3 prefix_pages=3 reclaims=3"
VALIDATION = PREFIX + ("validation:PASS bounds=3 invalid_joins=2 collision=1 reclaims=3 "
    "foreign_preserved=true irq_preserved=true")
REMAP = PREFIX + "remap:PASS bytes=8192 frame_changes=2 reclaims=2 irq_preserved=true"
COMPLETE = PREFIX + ("complete:PASS persistent_mappings=2 reclaims=8 module_execution=false "
    "physical_wx=false smp_mapping=false")
EXPECTED = [BEGIN, PERSISTENT, FAULTS, VALIDATION, REMAP, COMPLETE]
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")
FAILURE = re.compile(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", re.I)


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if FAILURE.search(text):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    if markers != EXPECTED:
        raise ValueError("Expected exactly six ordered standalone signed32 lease markers")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) <= lines.index(COMPLETE):
        raise ValueError("Expected one successful native summary after complete")
    return {
        "status": "passed", "scope": "experimental_signed32_rw_nonx_persistent_lease_single_cpu",
        "cpus": 1, "runtime_tests": 1, "span_bytes": 8192, "pages": 2,
        "envelope_pages": 3, "read_bytes": 16404, "pte_pages": 2,
        "frame_matches": 2, "rw_nonx_pages": 2, "supervisor_pages": 2,
        "writeback_pages": 2, "global_pages": 2, "slack_absent": 1,
        "persistent_mappings": 2, "read_bounds_rejections": 3,
        "invalid_join_rejections": 2, "collision_rejections": 1,
        "foreign_mapping_preserved": True, "fault_cases": 3,
        "installed_fault_prefix_pages": 3, "remapped_frame_changes": 2,
        "reclaimed_reservations": 8, "irq_state_preserved": True,
        "experimental_persistent_mapping_available": True,
        "linux_modules_executed": False, "linux_abi_verified": False,
        "physical_wx_verified": False, "smp_mapping_verified": False,
        "tlb_shootdown_verified": False, "production_mapping_ready": False,
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    output = path.with_name("result.json")
    output.write_text('{"status":"not_verified","linux_abi_verified":false}\n')
    result = verify(path.read_text(errors="replace"))
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified experimental single-CPU signed32 persistent lease only.")


if __name__ == "__main__":
    main()
