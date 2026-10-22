#!/usr/bin/env python3
"""Verify explicit C-shim import data-staging runtime evidence only."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_C_SHIM_IMPORT:"
BEGIN = PREFIX + "begin:PASS cpus=1 kinds=4"
CASE = re.compile(re.escape(PREFIX + "case:PASS ") +
                  r"kind=(1|2|4|11) span=([0-9]+) bytes=([0-9]+) relocation_bytes=(8|4) retained=true reclaimed=true")
FIXED_CASES = {1: 8, 2: 4, 4: 4, 11: 4}
POLICY = PREFIX + "policy:PASS default_denied=1 unknown_denied=3 rust_denied=1 current_bytes_denied=1 budgets_denied=2 backend_entries=0 reclaimed=8"
FAULTS = re.compile(re.escape(PREFIX + "faults:PASS cases=2 entries=2 acquired=") + r"([0-9]+) dropped=([0-9]+) writes=1 reclaimed=2")
COMPLETE = PREFIX + "complete:PASS elf_imports_prepared=true init_called=false modules_executed=0 production_mapping=false wx=false smp_mapping=false"
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")
FAILURE = re.compile(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", re.I)


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if FAILURE.search(text):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    marker_lines = [line for line in lines if "CYBERMANJU_" in line]
    expected_kinds = [1, 2, 4, 11]
    matches = [CASE.fullmatch(line) for line in marker_lines]
    if (len(marker_lines) != 8 or marker_lines[0] != BEGIN or
            any(m is None for m in matches[1:5]) or marker_lines[5] != POLICY or
            not FAULTS.fullmatch(marker_lines[6]) or marker_lines[7] != COMPLETE):
        raise ValueError("Expected exactly eight standalone ordered C-shim import markers")
    cases = []
    span = None
    for kind, m in zip(expected_kinds, matches[1:5]):
        actual_kind, case_span, byte_count, relocation_bytes = map(int, m.groups())
        if actual_kind != kind or relocation_bytes != FIXED_CASES[kind]:
            raise ValueError("Case kind order or relocation width is invalid")
        if not (0 < case_span <= 4 * 1024 * 1024 and case_span % 4096 == 0 and byte_count == case_span):
            raise ValueError("Invalid case span/readback geometry")
        if span is None:
            span = case_span
        elif case_span != span:
            raise ValueError("All cases must use the same span")
        cases.append((actual_kind, case_span, byte_count, relocation_bytes))
    fault = FAULTS.fullmatch(marker_lines[6])
    acquired, dropped = map(int, fault.groups())
    pages_plus_handles = span // 4096 + 2
    if acquired != dropped or acquired != pages_plus_handles:
        raise ValueError("Fault handles must match pages plus two and be balanced")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) <= lines.index(COMPLETE):
        raise ValueError("Expected one standalone successful native summary after complete")
    return {
        "status": "passed", "scope": "explicit_c_shim_import_data_staging_only", "runtime_tests": 1,
        "failed": 0, "cpus": 1, "relocation_kinds": [1, 2, 4, 11], "prepared_cases": 4,
        "span_bytes": span, "compared_bytes": 4 * span, "relocation_bytes": 20,
        "default_imports_denied": 1, "unknown_imports_denied": 3, "rust_imports_denied": 1,
        "current_bytes_revalidated": 1, "budget_rejections": 2, "preflight_backend_entries": 0,
        "preflight_reclaims": 8, "fault_cases": 2, "backend_entries": 2,
        "acquired_handles": acquired, "dropped_handles": dropped, "writes_before_failure": 1,
        "fault_reclaims": 2, "success_reclaims": 4, "total_reclaims": 14,
        "elf_imports_prepared": True, "default_linux_import_policy_changed": False,
        "linux_abi_verified": False, "init_called": False, "module_execution_verified": False,
        "linux_modules_executed": False, "production_mapping_available": False,
        "wx_verified": False, "smp_mapping_verified": False,
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    output = path.with_name("result.json")
    output.write_text('{"status":"not_verified","linux_abi_verified":false}\n')
    result = verify(path.read_text(errors="replace"))
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified explicit C-shim import data staging only.")


if __name__ == "__main__":
    main()
