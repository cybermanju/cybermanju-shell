#!/usr/bin/env python3
"""Verify native evidence for an experimental single-CPU read-only alias lease."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_SIGNED32_RO_LEASE:"
BEGIN = PREFIX + "begin:PASS cpus=1"
SEALED = PREFIX + ("sealed:PASS span=8192 pages=2 envelope_pages=3 before_rw=2 pte_pages=2 "
    "frame_matches=2 ro_nonx=2 rw_nonx=0 supervisor=2 writeback=2 global=2 "
    "slack_absent=1 bytes=8192 irq_preserved=true exclusive=true")
ALIAS = PREFIX + "alias:PASS bytes=32 linear_rw_alias=true changed_readback=true immutable_data=false"
FAULTS = PREFIX + "faults:PASS cases=3 prefix_pages=3 reclaims=3 acquired=6 dropped=6 live=0"
VALIDATION = PREFIX + "validation:PASS bounds=3 output_preserved=true irq_preserved=true"
REMAP = PREFIX + "remap:PASS bytes=8192 frame_changes=2 reclaims=2 factory_ro=true"
COMPLETE = PREFIX + ("complete:PASS protected_mappings=2 total_reclaims=5 module_execution=false "
    "physical_wx=false smp_mapping=false write_fault_probed=false")
EXPECTED = [BEGIN, SEALED, ALIAS, FAULTS, VALIDATION, REMAP, COMPLETE]
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")
FAILURE = re.compile(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", re.I)


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if FAILURE.search(text):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    if markers != EXPECTED:
        raise ValueError("Expected exactly seven ordered standalone signed32 read-only lease markers")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) <= lines.index(COMPLETE):
        raise ValueError("Expected one successful native summary after complete")
    return {
        "status": "passed", "architecture": "x86_64", "page_size": 4096,
        "scope": "single_cpu_read_only_nonexec_alias", "boot_cpus": 1,
        "runtime_tests": 1, "span_bytes": 8192, "pages": 2, "envelope_pages": 3,
        "before_rw_pages": 2, "pte_pages": 2, "frame_matches": 2,
        "read_only_nonexec_pages": 2, "rw_nonexec_pages": 0,
        "supervisor_pages": 2, "writeback_pages": 2, "global_pages": 2,
        "slack_absent": 1, "sealed_bytes": 8192, "alias_bytes": 32,
        "linear_rw_alias": True, "changed_readback": True, "immutable_data": False,
        "fault_cases": 3, "installed_fault_prefix_pages": 3,
        "fault_reclaims": 3, "handles_acquired": 6, "handles_dropped": 6,
        "live_handles": 0, "bounds_rejections": 3, "output_preserved": True,
        "remapped_frame_changes": 2, "remap_reclaims": 2,
        "protected_mappings": 2, "total_reclaims": 5, "irq_state_preserved": True,
        "exclusive": True, "factory_read_only": True,
        "module_execution": False, "physical_wx": False, "smp_mapping": False,
        "write_fault_probed": False,
        "normal_build_api_exists": True, "production_qualified": False,
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    output = path.with_name("result.json")
    output.write_text('{"status":"not_verified","production_qualified":false}\n')
    result = verify(path.read_text(errors="replace"))
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified experimental single-CPU read-only non-executable alias evidence only.")


if __name__ == "__main__":
    main()
