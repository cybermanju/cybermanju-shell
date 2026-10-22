#!/usr/bin/env python3
"""Verify SMP mapping rejection and buffer preservation evidence only."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_SIGNED32_MAPPING_SMP:"
BEGIN = PREFIX + "begin:PASS cpus=2"
COMPLETE = PREFIX + "complete:PASS denied=3 buffers_preserved=3 raw_compared=40960 reclaimed=1 pages_mapped=false production=false execution=false"
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if re.search(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", text, re.I):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    if markers != [BEGIN, COMPLETE]:
        raise ValueError("Expected exactly two unique ordered SMP mapping markers")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) < lines.index(COMPLETE):
        raise ValueError("Expected one successful native summary after SMP evidence")
    return {"status": "passed", "scope": "signed32_mapping_smp_rejection_only", "runtime_tests": 1, "failed": 0,
            "smp_rejection_verified": True, "cpus": 2, "denied_cases": 3, "buffers_preserved": 3,
            "raw_compared_bytes": 40960, "reclaimed_ranges": 1, "pages_mapped": False,
            "production_mapping_available": False, "smp_mapping_verified": False,
            "module_execution_verified": False, "linux_modules_executed": False, "wx_verified": False}


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    output = path.with_name("result.json")
    output.write_text('{"status":"not_verified","linux_modules_executed":false}\n')
    result = verify(path.read_text(errors="replace"))
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified signed32 SMP mapping rejection and preserved buffers; mapping/execution unverified.")


if __name__ == "__main__":
    main()
