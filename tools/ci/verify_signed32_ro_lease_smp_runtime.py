#!/usr/bin/env python3
"""Verify native evidence for experimental signed32 read-only lease SMP rejection only."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_SIGNED32_RO_LEASE_SMP:"
BEGIN = PREFIX + "begin:PASS cpus=2"
DENIED = PREFIX + "denied:PASS attempts=2 reclaims=2 acquired=4 dropped=4 live=0 irq_preserved=true"
COMPLETE = PREFIX + "complete:PASS ro_mapping=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false"
EXPECTED = [BEGIN, DENIED, COMPLETE]
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")
FAILURE = re.compile(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", re.I)


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if FAILURE.search(text):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    if markers != EXPECTED:
        raise ValueError("Expected exactly three ordered standalone signed32 read-only lease SMP markers")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) <= lines.index(COMPLETE):
        raise ValueError("Expected one successful native summary after complete")
    return {
        "status": "passed", "architecture": "x86_64", "page_size": 4096,
        "scope": "smp_rejection_only", "boot_cpus": 2, "runtime_tests": 1,
        "denied_attempts": 2, "reclaimed_reservations": 2,
        "handles_acquired": 4, "handles_dropped": 4, "live_handles": 0,
        "irq_state_preserved": True, "ro_mapping": False,
        "module_execution": False, "physical_wx": False, "smp_mapping": False,
        "write_fault_probed": False, "immutable_data": False,
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
    print("Verified experimental signed32 lease SMP rejection only.")


if __name__ == "__main__":
    main()
