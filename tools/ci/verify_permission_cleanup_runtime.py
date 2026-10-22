#!/usr/bin/env python3
"""Verify experimental consuming permission-preflight cleanup evidence only."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_PERMISSION_CLEANUP:"
BEGIN = PREFIX + "begin:PASS cpus=1 denied_cases=2"
DENIED = re.compile(re.escape(PREFIX + "denied:PASS ") +
    r"id=(0|1) budget=([0-9]+) span=([0-9]+) pages=([0-9]+) acquired=([0-9]+) dropped=([0-9]+) mapper_entries=([0-9]+) writes=([0-9]+) reclaimed=true")
SUCCESS = re.compile(re.escape(PREFIX + "success:PASS ") +
    r"budget=([0-9]+) span=([0-9]+) pages=([0-9]+) acquired=([0-9]+) dropped=([0-9]+) mapper_entries=([0-9]+) writes=([0-9]+) bytes=([0-9]+) nonx_pages=([0-9]+) retained=true reclaimed=true")
COMPLETE = re.compile(re.escape(PREFIX + "complete:PASS ") +
    r"denied=2 public_denied=2 public_reclaims=2 recovered=1 mapper_entries=1 acquired=([0-9]+) dropped=([0-9]+) reclaims=5 injected_heap_oom=false module_execution=false physical_wx=false smp_mapping=false")
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")
FAILURE = re.compile(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", re.I)
ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")


def verify(text: str) -> dict:
    text = ANSI.sub("", text).replace("\r", "")
    if FAILURE.search(text):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    markers = [(i, line) for i, line in enumerate(lines) if "CYBERMANJU_" in line]
    if len(markers) != 5 or markers[0][1] != BEGIN:
        raise ValueError("Expected exactly five ordered standalone permission-cleanup markers")
    denied_cases = []
    for expected_id, (idx, line) in enumerate(markers[1:3]):
        m = DENIED.fullmatch(line)
        if not m:
            raise ValueError("Malformed denied marker")
        ident, budget, span, pages, acquired, dropped, entries, writes = map(int, m.groups())
        if ident != expected_id or (budget != 0 if ident == 0 else budget != pages - 1):
            raise ValueError("Denied case id or budget is invalid")
        if span <= 0 or span % 4096 or span > 1024 * 4096 or pages != span // 4096:
            raise ValueError("Invalid permission-cleanup span geometry")
        if acquired != pages or dropped != pages or entries != 0 or not (0 < writes <= span):
            raise ValueError("Denied cleanup observations are inconsistent")
        denied_cases.append(dict(span=span, pages=pages, acquired=acquired, dropped=dropped,
                                 mapper_entries=entries, writes=writes))
    if denied_cases[0]["span"] != denied_cases[1]["span"] or denied_cases[0]["pages"] != denied_cases[1]["pages"]:
        raise ValueError("Denied cases do not share geometry")
    si, sl = markers[3]
    m = SUCCESS.fullmatch(sl)
    if not m:
        raise ValueError("Malformed successful recovery marker")
    budget, span, pages, acquired, dropped, entries, writes, byte_count, nonx = map(int, m.groups())
    if span != denied_cases[0]["span"] or pages != denied_cases[0]["pages"] or budget != pages:
        raise ValueError("Successful recovery geometry or budget disagrees")
    if acquired != pages or dropped != pages or entries != 1 or not (0 < writes <= span) or byte_count != span or nonx != pages:
        raise ValueError("Successful recovery observations are inconsistent")
    ci, cl = markers[4]
    m = COMPLETE.fullmatch(cl)
    total = sum(x["pages"] for x in denied_cases) + pages
    if not m or tuple(map(int, m.groups())) != (total, total):
        raise ValueError("Completion totals disagree with observed case metrics")
    summaries = [(i, line) for i, line in enumerate(lines) if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0][1]) or summaries[0][0] <= ci:
        raise ValueError("Expected one successful native summary after complete")
    denied_total = sum(x["pages"] for x in denied_cases)
    return {
        "status": "passed", "scope": "experimental_consuming_permission_preflight_cleanup",
        "cpus": 1, "runtime_tests": 1, "denied_observed_handoffs": 2, "denied_public_handoffs": 2,
        "successful_recovery_mappings": 1, "mapper_entries": 1,
        "acquired_handles": total, "dropped_handles": total,
        "denied_mapper_entries": 0, "denied_acquired_handles": denied_total,
        "denied_dropped_handles": denied_total, "reclaimed_reservations": 5, "public_api_reclaims": 2,
        "compared_bytes": byte_count, "actual_nonx_pages": nonx,
        "preflight_cleanup_verified": True, "real_heap_oom_injected": False,
        "linux_modules_executed": False, "physical_wx_verified": False,
        "smp_mapping_verified": False, "production_mapping_ready": False,
        "linux_abi_verified": False, "rx_mapping_verified": False,
        "tlb_shootdown_verified": False,
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    output = path.with_name("result.json")
    output.write_text('{"status":"not_verified"}\n')
    try:
        result = verify(path.read_text(errors="replace"))
    except (OSError, ValueError) as exc:
        print(f"Not verified: {exc}", file=sys.stderr)
        raise SystemExit(1)
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified experimental consuming permission-preflight cleanup evidence only.")


if __name__ == "__main__":
    main()
