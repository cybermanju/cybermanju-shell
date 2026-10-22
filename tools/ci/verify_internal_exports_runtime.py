#!/usr/bin/env python3
"""Require the internal OSTD registry and data staging, never Linux ABI admission."""
import json
from pathlib import Path
import re
import sys

from verify_signed32_prepared_runtime import KERNEL_BASE, PAGE, POOL_START, POOL_END, SUMMARY

PREFIX = "CYBERMANJU_INTERNAL_EXPORTS:"
BEGIN = PREFIX + "begin:PASS cpus=1 entries=1"
ADMISSION = PREFIX + ("admission:PASS described=1 rust_internal=1 rejected_abi=1 rejected_names=5 "
                      "numeric_export_preserved=true no_admitted_linux_exports=true")
RESOLVER = PREFIX + ("resolver:PASS checked_abi_rejections=2 checked_unknown_rejections=2 "
                     "preflight_rejections=2 local_only_prepared=1 numeric_fallback=false")
COMPLETE = PREFIX + ("complete:PASS staged=2 denied_names=5 missing_imports=2 available_checks=4 "
                     "compared_full_span=true pages_mapped=false linux_abi=false execution=false abi_rejection=true policy_resolver=true")
STAGED = re.compile(re.escape(PREFIX + "staged:PASS ") +
                    r"kind=([0-9]+) base=(0x[0-9a-fA-F]+) reserved=([0-9]+) span=([0-9]+) "
                    r"pages=([0-9]+) payload=([0-9]+) relas=([0-9]+) symbol=(0x[0-9a-fA-F]+) "
                    r"field=(0x[0-9a-fA-F]+) compared=([0-9]+) registry_match=true")


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if re.search(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", text, re.I):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    staged = [m for line in markers if (m := STAGED.fullmatch(line))]
    if (len(markers) != 6 or markers.count(BEGIN) != 1 or markers.count(COMPLETE) != 1
            or markers.count(ADMISSION) != 1 or markers[1] != ADMISSION
            or markers.count(RESOLVER) != 1 or markers[-2] != RESOLVER
            or len(staged) != 2 or [int(m[1]) for m in staged] != [2, 4]
            or set(markers) != {BEGIN, ADMISSION, RESOLVER, COMPLETE, *(m[0] for m in staged)}
            or markers[0] != BEGIN or markers[-1] != COMPLETE):
        raise ValueError("Expected unique ordered registry/ABI-rejection/PC32/PLT32/provider/cleanup markers")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) < lines.index(COMPLETE):
        raise ValueError("Expected one successful native summary after cleanup")
    cases = []
    for m in staged:
        kind, base, reserved, span, pages, payload, relas, symbol, field, compared = (
            int(m[i], 16 if i in (2, 8, 9) else 10) for i in range(1, 11))
        if not (base == POOL_START + 16 * PAGE and POOL_START <= base < base + reserved <= POOL_END
                and reserved % PAGE == 0 and span % PAGE == 0 and 0 < span <= reserved <= 1024 * PAGE
                and pages == span // PAGE and 0 < payload <= span and 0 < relas <= 65_536
                and base <= field and field + 4 <= base + span and compared == span
                and KERNEL_BASE <= symbol < KERNEL_BASE + (1 << 30)
                and -0x80000000 <= symbol - field <= 0x7FFFFFFF):
            raise ValueError("Invalid registry-bound relative relocation or full-span geometry")
        cases.append({"kind": kind, "base": hex(base), "reserved_bytes": reserved, "span": span,
                      "pages": pages, "payload": payload, "relocations": relas, "symbol": hex(symbol),
                      "field": hex(field), "compared": compared, "registry_match": True})
    if any(cases[0][key] != cases[1][key] for key in ("base", "reserved_bytes", "span", "pages", "payload", "relocations", "symbol")):
        raise ValueError("Both fixture mutations must share registry address and geometry")
    return {
        "status": "passed", "scope": "internal_ostd_export_registry_data_only",
        "runtime_tests": 1, "failed": 0, "internal_registry_lookup_verified": True,
        "internal_registry_entries": 1, "denied_name_cases": 5, "missing_import_cases": 2,
        "staged_cases": 2, "available_range_checks": 4, "full_span_readback_verified": True,
        "fixture_imports_renamed_for_internal_registry": True,
        "internal_registry_symbol": "cybermanju_ostd_num_cpus", "registry_function_abi": "rust_internal",
        "registry_function_signature": "fn() -> usize", "export_descriptor_verified": True,
        "linux_admission_policy": "reject_only", "linux_admission_rejection_verified": True,
        "linux_admitted_exports": 0, "linux_abi_rejection_cases": 1, "linux_unknown_name_rejection_cases": 5,
        "policy_symbol_resolver_verified": True, "policy_numeric_fallback_available": False,
        "policy_abi_rejected_prepare_cases": 2, "policy_unknown_rejected_prepare_cases": 2,
        "policy_preflight_rejected_cases": 2, "policy_local_only_prepared_cases": 1,
        "kernel_linux_export_lookup_available": False, "linux_abi_verified": False,
        "pages_mapped": False, "production_mapping_available": False, "module_execution_verified": False,
        "linux_modules_executed": False, "init_called": False, "wx_verified": False, "smp_mapping_verified": False,
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
    print("Verified real OSTD internal registry and complete staged bytes; no Linux ABI or execution.")


if __name__ == "__main__":
    main()
