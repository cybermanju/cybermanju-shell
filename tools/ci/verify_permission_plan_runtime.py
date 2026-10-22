#!/usr/bin/env python3
"""Verify experimental ELF page-permission preflight metadata evidence only."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_PERMISSION_PLAN:"
BEGIN = PREFIX + "begin:PASS cpus=1 kinds=4"
KINDS = [1, 2, 4, 11]
CASE = re.compile(re.escape(PREFIX + "case:PASS ") +
    r"kind=(1|2|4|11) span=([0-9]+) pages=([0-9]+) ro=([0-9]+) rw=([0-9]+) rx=([0-9]+) gap=([0-9]+) actual_nonx=([0-9]+) bytes=([0-9]+) retained=true reclaimed=true")
COMPLETE = (PREFIX + "complete:PASS cases=4 conflicts_denied=3 budgets_denied=4 "
            "reclaims=4 metadata_only=true rx_applied=false module_execution=false "
            "physical_wx=false smp_mapping=false")
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")
FAILURE = re.compile(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", re.I)


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if FAILURE.search(text):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    expected_markers = 6
    if len(markers) != expected_markers or markers[0] != BEGIN or markers[-1] != COMPLETE:
        raise ValueError("Expected exactly six ordered standalone permission-plan markers")
    cases = []
    for expected, line in zip(KINDS, markers[1:5]):
        match = CASE.fullmatch(line)
        if not match:
            raise ValueError("Malformed permission-plan case marker")
        kind, span, pages, ro, rw, rx, gap, actual_nonx, byte_count = map(int, match.groups())
        if kind != expected:
            raise ValueError("Permission-plan cases are not in canonical kind order")
        if span <= 0 or span % 4096 or span > 1024 * 4096:
            raise ValueError("Invalid permission-plan span")
        page_count = span // 4096
        if pages != page_count or min(ro, rw, rx) <= 0 or gap < 0:
            raise ValueError("Invalid permission class geometry")
        if ro + rw + rx + gap != pages or actual_nonx != pages or byte_count != span:
            raise ValueError("Permission-plan metrics disagree with span geometry")
        cases.append({"kind": kind, "span": span, "pages": pages, "ro": ro,
                      "rw": rw, "rx": rx, "gap": gap, "actual_nonx": actual_nonx,
                      "bytes": byte_count})
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) <= lines.index(COMPLETE):
        raise ValueError("Expected one successful native summary after complete")
    total = lambda field: sum(case[field] for case in cases)
    return {
        "status": "passed", "scope": "experimental_elf_page_permission_preflight_only",
        "cpus": 1, "runtime_tests": 1, "planned_cases": 4, "kinds": KINDS,
        "planned_pages": total("pages"), "intended_ro_pages": total("ro"),
        "intended_rw_pages": total("rw"), "intended_rx_pages": total("rx"),
        "intended_gap_pages": total("gap"), "actual_nonx_pages": total("actual_nonx"),
        "compared_bytes": total("bytes"), "conflicts_denied": 3,
        "budget_rejections": 4, "reclaimed_reservations": 4,
        "page_permission_preflight_verified": True, "permissions_applied": False,
        "rx_mapping_verified": False, "physical_wx_verified": False,
        "linux_modules_executed": False, "linux_abi_verified": False,
        "smp_mapping_verified": False, "production_mapping_ready": False,
        "tlb_shootdown_verified": False,
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    output = path.with_name("result.json")
    output.write_text('{"status":"not_verified"}\n')
    result = verify(path.read_text(errors="replace"))
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified experimental ELF permission-plan preflight evidence only.")


if __name__ == "__main__":
    main()
