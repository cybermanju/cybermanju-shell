#!/usr/bin/env python3
"""Verify real foreign-PTE rejection/rollback, never module execution or SMP."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_NEAR_TEXT_COLLISION:"
PAGE = 4096
POOL_START = 0xFFFFFFFF80000000 - (1 << 30) + PAGE
POOL_END = 0xFFFFFFFF80000000 - PAGE
BEGIN = PREFIX + "begin:PASS cpus=1 cases=3 span_pages=3 fixture_frames=1"
COMPLETE = (PREFIX + "complete:PASS cases=3 denied=6 preserved=6 reclaimed=3 "
            "seeded_pages=3 fixture_frames=1 va_dereferences=0 "
            "fault_injection_mapping=true production_mapping=false execution=false")
CASE = re.compile(re.escape(PREFIX + "case:PASS ") +
                  r"index=([0-9]+) start=(0x[0-9a-fA-F]+) end=(0x[0-9a-fA-F]+) "
                  r"foreign=(0x[0-9a-fA-F]+) denied=2 preserved=2 reclaimed=true")
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if re.search(r"\b(?:panic|panicked|failure)\b|test result:\s*FAILED\b|assertion\b[^\n]*failed\b", text, re.I):
        raise ValueError("Runtime contains panic or failure evidence")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    cases = [match for line in markers if (match := CASE.fullmatch(line))]
    if (len(markers) != 5 or markers.count(BEGIN) != 1 or markers.count(COMPLETE) != 1
            or len(cases) != 3 or {int(m[1]) for m in cases} != {0, 1, 2}
            or set(markers) != {BEGIN, COMPLETE, *(m[0] for m in cases)}):
        raise ValueError("Expected one complete begin, three unique cases and one complete final marker")
    # The protocol is ordered, including the native success summary after cleanup.
    if markers[0] != BEGIN or markers[-1] != COMPLETE or [int(m[1]) for m in cases] != [0, 1, 2]:
        raise ValueError("Adversarial cases missing or out of order")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]):
        raise ValueError("Expected exactly one native summary: 1 passed; 0 failed")
    if lines.index(summaries[0]) < lines.index(COMPLETE):
        raise ValueError("Native summary must follow completed cleanup")
    ranges = []
    for match in cases:
        index = int(match[1])
        start, end, foreign = (int(match[n], 16) for n in (2, 3, 4))
        if not (POOL_START <= start < end <= POOL_END):
            raise ValueError("Foreign-PTE fixture outside dedicated canonical pool")
        if start != POOL_END - 8 * PAGE or end != start + 3 * PAGE:
            raise ValueError("Fixture is not the required bounded three-page candidate")
        if start % PAGE or end % PAGE or foreign % PAGE or foreign != start + index * PAGE:
            raise ValueError("Foreign-PTE slot must cover start, middle and end pages exactly")
        ranges.append({"index": index, "start": hex(start), "end": hex(end), "foreign": hex(foreign)})
    return {
        "status": "passed", "scope": "ktest_foreign_pte_fault_injection_only",
        "runtime_tests": 1, "scenarios": 3, "failed": 0,
        "foreign_pte_rejection_verified": True,
        "post_audit_claim_rollback_verified": True,
        "foreign_pte_preservation_verified": True,
        "denied_claims": 6, "preserved_snapshots": 6, "exact_reclaims": 3,
        "fault_injection_mapping_performed": True,
        "fixture_frames_allocated": 1, "injected_mapping_cases": 3,
        "reservation_frames_allocated": False,
        "va_dereferences": 0, "production_mapping_available": False,
        "near_text_loader_integration_available": False,
        "linux_modules_executed": False, "module_execution_verified": False,
        "smp_mapping_verified": False, "wx_verified": False, "case_ranges": ranges,
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    output = path.with_name("result.json")
    output.write_text(json.dumps({"status": "not_verified", "module_execution_verified": False}) + "\n")
    result = verify(path.read_text(errors="replace"))
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified foreign-PTE fault injection and claim rollback; no production mapping or execution.")


if __name__ == "__main__":
    main()
