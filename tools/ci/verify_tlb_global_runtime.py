#!/usr/bin/env python3
"""Validate serial evidence for local global-TLB invalidation only."""
import json
from pathlib import Path
import re
import sys

MARKERS = (
    "CYBERMANJU_TLB_GLOBAL:encoding:PASS cases=4",
    "CYBERMANJU_TLB_GLOBAL:aggregation:PASS cases=8",
    "CYBERMANJU_TLB_GLOBAL:stale_readback:PASS cases=7 cpus=1",
    "CYBERMANJU_TLB_GLOBAL:cpu_state:PASS pge_cases=2 irq_checks=3",
)
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; \d+ filtered out\.")


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    text = text.replace("\r", "")
    lines = text.splitlines()
    if any(lines.count(marker) != 1 for marker in MARKERS):
        raise ValueError("Expected exactly one complete marker for each TLB-global scenario")
    relevant = [line for line in lines if "CYBERMANJU_" in line]
    if len(relevant) != len(MARKERS) or set(relevant) != set(MARKERS):
        raise ValueError("Unexpected, echoed, mixed or contradictory runtime marker")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]):
        raise ValueError("Expected one standalone summary: 1 passed, 0 failed")
    without_summary = "\n".join(line for line in lines if "test result:" not in line.lower())
    if re.search(r"\b(?:panic|panicked|fail|failed|failure)\b", without_summary, re.I):
        raise ValueError("Runner reported failure or panic")
    return {
        "status": "passed",
        "scope": "local_global_tlb_invalidation_only",
        "cpus": 1,
        "encoding_cases": 4,
        "aggregation_cases": 8,
        "stale_readback_cases": 7,
        "pge_state_cases": 2,
        "irq_state_checks": 3,
        "remote_shootdown_verified": False,
        "linux_modules_executed": False,
        "init_called": False,
        "smp_mapping_supported": False,
    }


def main() -> None:
    path = Path(sys.argv[1])
    result = verify(path.read_text(errors="replace"))
    path.with_name("result.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Verified local global-TLB invalidation runtime evidence; no remote shootdown or Linux module execution claimed.")


if __name__ == "__main__":
    main()
