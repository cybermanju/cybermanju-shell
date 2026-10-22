#!/usr/bin/env python3
"""Prove temporary non-executable mapping in a signed-32-compatible VA arena."""
import json
from pathlib import Path
import re
import sys

PAGE = 4096
KERNEL_BASE = 0xFFFFFFFF80000000
POOL_START = KERNEL_BASE + (1 << 30) + PAGE
POOL_END = KERNEL_BASE + (3 << 29) - PAGE
PREFIX = "CYBERMANJU_SIGNED32_PREPARED:"
BEGIN = PREFIX + "begin:PASS cpus=1 kinds=3 rejected=6 cross_pool_rejected=2"
MAPPING = PREFIX + "mapping:PASS full_span_cases=3 partial_cases=6 rejected=6 compared=122928 scopes_unmapped=9 pte_rw=true pte_supervisor=true pte_nonexec=true temporary=true production=false execution=false"
COMPLETE = PREFIX + ("complete:PASS staged=3 rejected=6 reclaimed=9 compared_full_span=true "
                     "old_arena_unchanged=true pages_mapped=true temporary_mapping=true all_unmapped=true linux_export_lookup=false execution=false")
STAGED = re.compile(re.escape(PREFIX + "staged:PASS ") +
                    r"kind=([0-9]+) base=(0x[0-9a-fA-F]+) reserved=([0-9]+) span=([0-9]+) "
                    r"pages=([0-9]+) payload=([0-9]+) relas=([0-9]+) symbol=(0x[0-9a-fA-F]+) "
                    r"field=(0x[0-9a-fA-F]+) compared=([0-9]+) self_defined=(true|false)")
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")


def signed32_fits(value: int) -> bool:
    low = value & 0xFFFFFFFF
    return (low | (0xFFFFFFFF00000000 if low & 0x80000000 else 0)) == value


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if re.search(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", text, re.I):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    staged = [m for line in markers if (m := STAGED.fullmatch(line))]
    if (len(markers) != 6 or markers.count(BEGIN) != 1 or markers.count(MAPPING) != 1 or markers.count(COMPLETE) != 1
            or len(staged) != 3 or [int(m[1]) for m in staged] != [11, 2, 4]
            or set(markers) != {BEGIN, MAPPING, COMPLETE, *(m[0] for m in staged)}
            or markers[0] != BEGIN or markers[-2] != MAPPING or markers[-1] != COMPLETE):
        raise ValueError("Expected unique ordered begin/R32S/PC32/PLT32/mapping/final markers")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) < lines.index(COMPLETE):
        raise ValueError("Expected one successful native summary after cleanup")
    cases = []
    for m in staged:
        kind, base, reserved, span, pages, payload, relas, symbol, field, compared = (int(m[i], 16 if i in (2, 8, 9) else 10) for i in range(1, 11))
        self_defined = m[11] == "true"
        if not (base == POOL_END - 64 * PAGE and POOL_START <= base < base + reserved <= POOL_END and base % PAGE == 0 and reserved % PAGE == 0 and span % PAGE == 0 and 0 < span <= reserved <= 1024 * PAGE and pages == span // PAGE and 0 < payload <= span and 0 < relas <= 65_536 and compared == span and base <= field and field + 4 <= base + span):
            raise ValueError("Invalid arena envelope, field, budgets or full-readback geometry")
        if not (signed32_fits(base) and signed32_fits(base + reserved - 1)):
            raise ValueError("Reservation endpoints must be signed-32 representable")
        if kind == 11:
            if not (self_defined and base <= symbol < base + span and signed32_fits(symbol)):
                raise ValueError("R32S must refer to a representable defined module symbol")
        elif self_defined or not (KERNEL_BASE <= symbol < KERNEL_BASE + (1 << 30) and -0x80000000 <= symbol - field <= 0x7FFFFFFF):
            raise ValueError("PC32/PLT32 must reach the actual kernel-address numeric alias")
        cases.append({"kind": kind, "base": hex(base), "reserved_bytes": reserved, "span": span, "pages": pages, "payload": payload, "relocations": relas, "symbol": hex(symbol), "field": hex(field), "compared": compared, "self_defined": self_defined})
    if any(cases[0][key] != c[key] for c in cases[1:] for key in ("base", "reserved_bytes", "span", "pages", "payload", "relocations")):
        raise ValueError("All three mutations must retain fixture geometry")
    if cases[1]["symbol"] != cases[2]["symbol"]:
        raise ValueError("Relative cases must use the same real code-address alias")
    if sum(case["span"] + 16 for case in cases) != 122928:
        raise ValueError("Temporary mapped readback count must match full spans and partial reads")
    return {"status": "passed", "scope": "signed32_arena_temporary_nonexecuting_mapping", "runtime_tests": 1,
            "failed": 0, "signed32_self_defined_staging_verified": True, "relative32_kernel_numeric_alias_verified": True,
            "signed32_arena_preparation_available": True, "legacy_lower_arena_policy_preserved": True,
            "cross_pool_rejection_cases": 2, "staged_cases": 3, "transaction_rejection_cases": 6, "exact_reclaim_cases": 9,
            "test_export_bindings_are_synthetic": True, "kernel_linux_export_lookup_available": False,
            "reservation_held_during_staging": True, "pages_mapped": True, "temporary_mapping_verified": True,
            "temporary_mapping_full_span_cases": 3, "temporary_mapping_partial_cases": 6,
            "temporary_mapping_rejected_cases": 6, "temporary_mapping_compared_bytes": 122928,
            "temporary_mapping_unmapped_scopes": 9, "pte_rw_verified": True, "pte_supervisor_verified": True,
            "pte_nonexec_verified": True, "mapping_absent_after_scopes": True,
            "production_mapping_available": False, "module_execution_verified": False, "linux_modules_executed": False,
            "init_called": False, "wx_verified": False, "smp_mapping_verified": False, "cases": cases}


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    output = path.with_name("result.json")
    output.write_text('{"status":"not_verified","linux_modules_executed":false}\n')
    result = verify(path.read_text(errors="replace"))
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified signed32-arena temporary non-executable mapping; production mapping and execution unavailable.")


if __name__ == "__main__":
    main()
