#!/usr/bin/env python3
"""Require a native bounded signed32 search and complete staged-byte readback."""
import json
from pathlib import Path
import re
import sys

from verify_signed32_prepared_runtime import KERNEL_BASE, PAGE, POOL_END, POOL_START, SUMMARY, signed32_fits

PREFIX = "CYBERMANJU_SIGNED32_SEARCH:"
BEGIN = PREFIX + "begin:PASS cpus=1 kinds=3"
COMPLETE = PREFIX + ("complete:PASS staged=3 prefix_exhaustions=3 rejected=9 available_checks=12 "
                     "compared_full_span=true pages_mapped=false linux_export_lookup=false execution=false")
STAGED = re.compile(re.escape(PREFIX + "staged:PASS ") +
                    r"kind=([0-9]+) first=(0x[0-9a-fA-F]+) selected=(0x[0-9a-fA-F]+) "
                    r"reserved=([0-9]+) span=([0-9]+) pages=([0-9]+) payload=([0-9]+) relas=([0-9]+) "
                    r"symbol=(0x[0-9a-fA-F]+) field=(0x[0-9a-fA-F]+) compared=([0-9]+) self_defined=(true|false)")


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if re.search(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", text, re.I):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    staged = [m for line in markers if (m := STAGED.fullmatch(line))]
    if (len(markers) != 5 or markers.count(BEGIN) != 1 or markers.count(COMPLETE) != 1
            or len(staged) != 3 or [int(m[1]) for m in staged] != [11, 2, 4]
            or set(markers) != {BEGIN, COMPLETE, *(m[0] for m in staged)}
            or markers[0] != BEGIN or markers[-1] != COMPLETE):
        raise ValueError("Expected unique ordered begin/R32S/PC32/PLT32/final markers")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) < lines.index(COMPLETE):
        raise ValueError("Expected one successful native summary after cleanup")
    cases = []
    for m in staged:
        kind, first, selected, reserved, span, pages, payload, relas, symbol, field, compared = (
            int(m[i], 16 if i in (2, 3, 9, 10) else 10) for i in range(1, 12))
        self_defined = m[12] == "true"
        if not (first == POOL_START and selected == first + PAGE
                and POOL_START <= selected < selected + reserved <= POOL_END
                and reserved % PAGE == 0 and span % PAGE == 0
                and 0 < span <= reserved <= 1024 * PAGE and pages == span // PAGE
                and 0 < payload <= span and 0 < relas <= 65_536 and compared == span
                and selected <= field and field + 4 <= selected + span
                and signed32_fits(selected) and signed32_fits(selected + reserved - 1)):
            raise ValueError("Invalid second-candidate or full-span geometry")
        if kind == 11:
            if not (self_defined and selected <= symbol < selected + span and signed32_fits(symbol)):
                raise ValueError("R32S must use the defined local section symbol")
        elif self_defined or not (KERNEL_BASE <= symbol < KERNEL_BASE + (1 << 30) and -0x80000000 <= symbol - field <= 0x7FFFFFFF):
            raise ValueError("Relative relocation must reach the real code-address numeric alias")
        cases.append({"kind": kind, "first_candidate": hex(first), "selected_base": hex(selected),
                      "reserved_bytes": reserved, "span": span, "pages": pages, "payload": payload,
                      "relocations": relas, "symbol": hex(symbol), "field": hex(field),
                      "compared": compared, "self_defined": self_defined})
    if any(cases[0][key] != c[key] for c in cases[1:] for key in ("first_candidate", "selected_base", "reserved_bytes", "span", "pages", "payload", "relocations")):
        raise ValueError("All mutations must retain fixture geometry and search prefix")
    if cases[1]["symbol"] != cases[2]["symbol"]:
        raise ValueError("Relative cases must use the same code-address alias")
    return {
        "status": "passed", "scope": "bounded_signed32_candidate_search_data_only",
        "runtime_tests": 1, "failed": 0, "signed32_candidate_search_verified": True,
        "bounded_prefix_verified": True, "caller_window_clipping_verified": True,
        "occupied_first_candidate_skipped": True, "full_span_readback_verified": True,
        "staged_cases": 3, "one_candidate_prefix_exhaustions": 3,
        "rejected_input_cases": 9, "available_range_checks": 12,
        "selected_case_candidate_budget": 2, "supported_candidate_budget_max": 64,
        "test_export_bindings_are_synthetic": True, "kernel_linux_export_lookup_available": False,
        "pages_mapped": False, "production_mapping_available": False,
        "module_execution_verified": False, "linux_modules_executed": False,
        "init_called": False, "wx_verified": False, "smp_mapping_verified": False,
        "cases": cases,
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    output = path.with_name("result.json")
    output.write_text('{"status":"not_verified","linux_modules_executed":false}\n')
    result = verify(path.read_text(errors="replace"))
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified bounded signed32 candidate search and full staged bytes; no mapping or execution.")


if __name__ == "__main__":
    main()
