#!/usr/bin/env python3
"""Verify signed32 temporary mapping fault-injection evidence only."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_SIGNED32_MAPPING_FAULT:"
BEGIN = PREFIX + "begin:PASS cpus=1 pages=3 envelope_pages=4"
ROLLBACK = PREFIX + (
    "rollback:PASS failures=3 installed_prefix_pages=3 buffers_preserved=3 "
    "foreign_rejected=1 foreign_preserved=true slack_unmapped=true"
)
COMPLETE = PREFIX + (
    "complete:PASS recovered_bytes=12288 reclaimed=1 all_unmapped=true "
    "production=false execution=false"
)
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if re.search(
        r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|"
        r"test result:\s*FAILED|\b[1-9]\d* failed;",
        text,
        re.I,
    ):
        raise ValueError("Native runner reported failure")

    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    if markers != [BEGIN, ROLLBACK, COMPLETE]:
        raise ValueError("Expected exactly three unique ordered mapping-fault markers")

    summaries = [line for line in lines if "test result:" in line.lower()]
    if (
        len(summaries) != 1
        or not SUMMARY.fullmatch(summaries[0])
        or lines.index(summaries[0]) < lines.index(COMPLETE)
    ):
        raise ValueError("Expected one standalone successful native summary after evidence")

    return {
        "status": "passed",
        "scope": "signed32_mapping_fault_injection_only",
        "runtime_tests": 1,
        "failed": 0,
        "cpus": 1,
        "mapping_failure_cases": 3,
        "installed_prefix_pages": 3,
        "buffers_preserved": 3,
        "foreign_rejected_cases": 1,
        "foreign_pte_preserved": True,
        "slack_unmapped_verified": True,
        "recovery_compared_bytes": 12288,
        "reclaimed_ranges": 1,
        "mapping_absent_after_scopes": True,
        "temporary_mapping_verified": True,
        "production_mapping_available": False,
        "linux_abi_verified": False,
        "wx_verified": False,
        "smp_mapping_verified": False,
        "linux_modules_executed": False,
        "module_execution_verified": False,
        "init_called": False,
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    output = path.with_name("result.json")
    output.write_text('{"status":"not_verified","linux_modules_executed":false}\n')
    result = verify(path.read_text(errors="replace"))
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified signed32 mapping fault-injection rollback and recovery only.")


if __name__ == "__main__":
    main()
