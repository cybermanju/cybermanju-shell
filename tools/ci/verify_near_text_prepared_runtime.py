#!/usr/bin/env python3
"""Data-only ELF preparation in the dedicated arena, never module execution."""
import json
from pathlib import Path
import re
import sys

PAGE = 4096
KERNEL_BASE = 0xFFFFFFFF80000000
POOL_START = KERNEL_BASE - (1 << 30) + PAGE
POOL_END = KERNEL_BASE - PAGE
PREFIX = "CYBERMANJU_NEAR_TEXT_PREPARED:"
BEGIN = PREFIX + "begin:PASS cpus=1 kinds=2 rejected=6"
COMPLETE = PREFIX + ("complete:PASS staged=2 rejected=6 reclaimed=8 compared_full_span=true "
                     "actual_kernel_address=true linux_export_lookup=false pages_mapped=false execution=false")
STAGED = re.compile(re.escape(PREFIX + "staged:PASS ") +
                    r"kind=([0-9]+) base=(0x[0-9a-fA-F]+) reserved=([0-9]+) span=([0-9]+) "
                    r"pages=([0-9]+) payload=([0-9]+) relas=([0-9]+) export=(0x[0-9a-fA-F]+) compared=([0-9]+)")
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if re.search(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", text, re.I):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    staged = [m for line in markers if (m := STAGED.fullmatch(line))]
    if (len(markers) != 4 or markers.count(BEGIN) != 1 or markers.count(COMPLETE) != 1
            or len(staged) != 2 or [int(m[1]) for m in staged] != [2, 4]
            or set(markers) != {BEGIN, COMPLETE, *(m[0] for m in staged)}
            or markers[0] != BEGIN or markers[-1] != COMPLETE):
        raise ValueError("Expected complete unique begin/PC32/PLT32/final markers in order")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) < lines.index(COMPLETE):
        raise ValueError("Expected one successful native summary after cleanup")
    cases = []
    for m in staged:
        kind, base, reserved, span, pages, payload, relas, export, compared = (
            int(m[i], 16 if i in (2, 8) else 10) for i in range(1, 10))
        if not (base == POOL_END - 64 * PAGE and POOL_START <= base < base + reserved <= POOL_END
                and base % PAGE == 0 and reserved % PAGE == 0 and span % PAGE == 0
                and 0 < span <= reserved <= 1024 * PAGE and pages == span // PAGE
                and 0 < payload <= span and 0 < relas <= 65_536 and compared == span):
            raise ValueError("Invalid envelope, page/payload/relocation or full-readback geometry")
        if not (KERNEL_BASE <= export < KERNEL_BASE + (1 << 30) and abs(export - base) <= 0x7FFFFFFF):
            raise ValueError("Expected numerically reachable actual kernel-code address")
        cases.append({"kind": kind, "base": hex(base), "reserved_bytes": reserved, "span": span,
                      "pages": pages, "payload": payload, "relocations": relas, "export": hex(export),
                      "compared": compared})
    if any(cases[0][key] != cases[1][key] for key in ("base", "reserved_bytes", "span", "pages", "payload", "relocations", "export")):
        raise ValueError("PC32 and PLT32 must use the same fixture geometry and code-address alias")
    return {
        "status": "passed", "scope": "near_text_data_only_preparation",
        "runtime_tests": 1, "failed": 0, "near_text_data_staging_verified": True,
        "full_relocation_preflight_verified": True, "self_signed32_arena_overflow_rejected": True,
        "near_text_staged_cases": 2, "transaction_rejection_cases": 6, "exact_reclaim_cases": 8,
        "actual_kernel_address_numeric_only": True, "kernel_linux_export_lookup_available": False,
        "test_export_bindings_are_synthetic": True, "reservation_held_during_staging": True,
        "near_text_preparation_available": True, "pages_mapped": False,
        "production_mapping_available": False, "module_execution_verified": False,
        "linux_modules_executed": False, "init_called": False, "wx_verified": False,
        "smp_mapping_verified": False, "cases": cases,
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    output = path.with_name("result.json")
    output.write_text('{"status":"not_verified","linux_modules_executed":false}\n')
    result = verify(path.read_text(errors="replace"))
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified near-text ELF data staging and rollback; no VA mapping or module execution.")


if __name__ == "__main__":
    main()
