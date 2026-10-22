#!/usr/bin/env python3
"""Verify single-CPU ktest scoped RW readback, never persistent mapping or execution."""
import json
from pathlib import Path
import re
import sys

SCENARIOS = ("roundtrip", "rejected_budgets_and_mismatched_layout", "reserved_layout", "no_init_or_exit_called", "reserved_owner_lifecycle", "scoped_rw_mapping", "prepared_transaction", "relative32_reach", "reach_diagnostics", "signed32_absolute", "relative32_placement_window", "relative32_import_collection", "relative32_defined_collection", "requested_layout", "relative32_reservation")
PREFIX = "CYBERMANJU_PREPARED_RUNTIME:"
COUNTS = r"sections=(\d+) span=(\d+) payload=(\d+) relocations=(\d+) pages=(\d+)"
ROUNDTRIP = re.compile(re.escape(PREFIX + "roundtrip:PASS ") + COUNTS)
RESERVED = re.compile(re.escape(PREFIX + "reserved_layout:PASS ") + r"base=(0x[0-9a-fA-F]+) reserved=(\d+) span=(\d+) sections=(\d+) payload=(\d+) relocations=(\d+) pages=(\d+)")
PREPARED_TRANSACTION = PREFIX + "prepared_transaction:PASS rejection_cases=7 range_overflow=true reservation_released=true"
RELATIVE32_REACH = PREFIX + "relative32_reach:PASS types=2 boundary_cases=8 missing_exports=1 far_exports=2"
REACH_DIAGNOSTICS = PREFIX + "reach_diagnostics:PASS overflow_sites=4 missing_sites=1 budget_no_site=1 real_code_exports=2"
SIGNED32_ABSOLUTE = PREFIX + "signed32_absolute:PASS accepted=2 rejected=3 kernel_export_numeric=true execution=false"
RELATIVE32_PLACEMENT_WINDOW = PREFIX + "relative32_placement_window:PASS kinds=2 rejected_current_base=2 candidate_preflight=2 reserved_near_text=false execution=false"
RELATIVE32_IMPORT_COLLECTION = 'CYBERMANJU_PREPARED_RUNTIME:relative32_import_collection:PASS kinds=2 collected_imports=2 candidate_preflight=2 reserved_near_text=false execution=false'
RELATIVE32_DEFINED_COLLECTION = PREFIX + "relative32_defined_collection:PASS types=2 section_relative=2 absolute=2 kernel_address_numeric=true reserved_near_text=false execution=false"
REQUESTED_LAYOUT = re.compile(re.escape(PREFIX + "requested_layout:PASS ") + r"base=(0x[0-9a-fA-F]+) reserved=(\d+) span=(\d+) pages=(\d+) invalid_cases=(\d+) conflict_cases=(\d+) rollback_cases=(\d+) exact_reclaim=(true|false) mapping=(true|false) execution=(true|false)")
RELATIVE32_RESERVATION = "CYBERMANJU_PREPARED_RUNTIME:relative32_reservation:PASS kinds=2 occupied_candidates=2 staged_cases=2 rejection_cases=8 reclaimed_cases=4 attempts_bounded=true reserved_near_text=false mapping=false execution=false"


def relative32_numeric_placement_window(target: int, field_offset: int = 0) -> tuple[int, int, int]:
    """Return an aligned numeric candidate interval for signed PC32/PLT32 only.

    This is arithmetic evidence, not an allocation, mapping, loader or execution.
    The VMALLOC numeric window is deliberately excluded.
    """
    signed_min, signed_max = -(1 << 31), (1 << 31) - 1
    # Candidate placement base must be close enough to target+addend relative to P.
    low = max(0xffff800000000000, target + field_offset - signed_max)
    high = min(0xffffffffffff0000, target + field_offset - signed_min)
    vmalloc_low, vmalloc_high = 0xffffc00000000000, 0xffffe00000000000
    if low < vmalloc_high and high >= vmalloc_low:
        if low >= vmalloc_low:
            low = vmalloc_high
        else:
            high = vmalloc_low - 1
    low = (low + 4095) & ~4095
    high &= ~4095
    if low > high:
        raise ValueError("No non-VMALLOC numeric PC32 placement window")
    candidate = min(max((target + 2048) & ~4095, low), high)
    return low, high, candidate


def _strip_terminal_controls(text: str) -> str:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    text = re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]", "", text)
    if re.search(r"\r(?!\n)", text):
        raise ValueError("Isolated carriage return in runtime log")
    return text.replace("\r\n", "\n")


def _counts_valid(sections, span, payload, relocations, pages):
    return (0 < sections <= 4096 and 0 < pages <= 4096
            and span == pages * 4096 and 0 < payload <= span
            and 0 < relocations <= 65536)


def verify(text: str) -> dict:
    text = _strip_terminal_controls(text)
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    if len(markers) != len(SCENARIOS):
        raise ValueError("Unexpected or contradictory runtime marker")
    simple = {PREFIX + name + ":PASS" for name in (SCENARIOS[1], SCENARIOS[3])}
    owner_lifecycle = PREFIX + "reserved_owner_lifecycle:PASS cases=3"
    scoped = re.compile(re.escape(PREFIX + "scoped_rw_mapping:PASS pages=") + r"([1-9]\d{0,3})")
    rounds = [m for line in markers if (m := ROUNDTRIP.fullmatch(line))]
    reserved = [m for line in markers if (m := RESERVED.fullmatch(line))]
    scoped_matches = [m for line in markers if (m := scoped.fullmatch(line))]
    requested = [m for line in markers if (m := REQUESTED_LAYOUT.fullmatch(line))]
    if len(rounds) != 1 or len(reserved) != 1 or len(scoped_matches) != 1 or len(requested) != 1 or set(markers) != simple | {owner_lifecycle, rounds[0].group(), reserved[0].group(), scoped_matches[0].group(), requested[0].group(), PREPARED_TRANSACTION, RELATIVE32_REACH, REACH_DIAGNOSTICS, SIGNED32_ABSOLUTE, RELATIVE32_PLACEMENT_WINDOW, RELATIVE32_IMPORT_COLLECTION, RELATIVE32_DEFINED_COLLECTION, RELATIVE32_RESERVATION}:
        raise ValueError("Expected exactly one complete marker for every scenario")
    sections, span, payload, relocations, pages = map(int, rounds[0].groups())
    if not _counts_valid(sections, span, payload, relocations, pages):
        raise ValueError("Invalid numeric-layout counts or budgets")
    base_text, *counts = reserved[0].groups()
    base = int(base_text, 16)
    envelope, actual_span, actual_sections, actual_payload, actual_relocations, actual_pages = map(int, counts)
    if not _counts_valid(actual_sections, actual_span, actual_payload, actual_relocations, actual_pages):
        raise ValueError("Invalid reserved-layout counts or budgets")
    if not (0xffff800000000000 <= base < base + envelope <= 0xffffffffffff0000
            and base % 4096 == 0 and envelope % 4096 == 0
            and 0 < actual_span <= envelope <= 4 * 1024 * 1024):
        raise ValueError("Invalid reserved canonical range or envelope")
    req_base_text, req_reserved, req_span, req_pages, invalid_cases, conflict_cases, rollback_cases, exact_reclaim, req_mapping, req_execution = requested[0].groups()
    req_base = int(req_base_text, 16)
    req_reserved, req_span, req_pages, invalid_cases, conflict_cases, rollback_cases = map(int, (req_reserved, req_span, req_pages, invalid_cases, conflict_cases, rollback_cases))
    vmalloc_low, vmalloc_high = 0xffffc00000000000, 0xffffe00000000000
    req_end = req_base + req_reserved
    if (req_base >= 1 << 64 or req_end > 1 << 64 or req_end <= req_base
            or not (vmalloc_low <= req_base < req_end <= vmalloc_high)
            or req_base % 4096 or req_reserved != envelope or req_reserved <= 0
            or req_reserved > 4 * 1024 * 1024 or req_reserved % 4096
            or req_span <= 0 or req_span > req_reserved or req_span % 4096
            or req_pages != req_span // 4096 or req_pages > 1024
            or (invalid_cases, conflict_cases, rollback_cases) != (8, 2, 3)
            or exact_reclaim != "true" or req_mapping != "false" or req_execution != "false"):
        raise ValueError("Invalid requested-base layout evidence or budgets")
    scoped_pages = int(scoped_matches[0].group(1))
    if scoped_pages > 1024 or scoped_pages != actual_pages:
        raise ValueError("Scoped mapping pages do not match the reserved layout")
    if (actual_sections, actual_payload, actual_relocations) != (sections, payload, relocations):
        raise ValueError("Reserved layout does not describe the same fixture contents")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not re.fullmatch(r"test result: ok\. 1 passed; 0 failed; \d+ filtered out\.", summaries[0]):
        raise ValueError("Missing unique focused summary: 1 passed, 0 failed")
    if re.search(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|runner failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", text, re.I):
        raise ValueError("Runner reported failure or panic")
    return {
        "status": "passed", "runtime_tests": 1,
        "scope": "prepared_module_scoped_rw_readback_only",
        "requested_base_layout_verified": True,
        "requested_base_invalid_cases": 8, "requested_base_conflict_cases": 2,
        "requested_base_rollback_cases": 3, "requested_base_exact_reclaim_verified": True,
        "requested_base_layout": {"base": hex(req_base), "reserved_bytes": req_reserved,
                                  "span": req_span, "pages": req_pages},
        "joint_owner_lifecycle_verified": True, "owner_lifecycle_cases": 3,
        "reserved_prepare_transaction_verified": True,
        "transaction_rejection_cases": 7,
        "reserved_base_relocation_overflow_rejected": True,
        "relative32_reach_preflight_verified": True,
        "relative32_boundary_cases": 8,
        "relative32_far_exports_rejected": 2,
        "exports_are_synthetic": True,
        "test_export_bindings_are_synthetic": True,
        "reach_diagnostics_verified": True,
        "signed32_absolute_sign_extension_verified": True,
        "signed32_kernel_export_numeric_only": True,
        "relative32_numeric_placement_window_verified": True,
        "relative32_import_constraints_verified": True,
        "relative32_defined_symbol_constraints_verified": True,
        "relative32_defined_section_relative_cases": 2,
        "relative32_defined_absolute_cases": 2,
        "relative32_reservation_verified": True,
        "relative32_reservation_kinds": 2,
        "relative32_reserved_staged_cases": 2,
        "relative32_reservation_rejection_cases": 8,
        "relative32_reservation_reclaimed_cases": 4,
        "relative32_candidate_search_bounded": True,
        "near_text_allocation_available": False,
        "module_execution_verified": False,
        "real_code_addresses_tested": 2,
        "kernel_linux_export_lookup_available": False,
        "fixture_staged": True, "linux_modules_executed": False, "init_called": False,
        "pages_mapped": True, "wx_verified": False,
        "mapping_persists_after_read": False,
        "scoped_mapping_readback_verified": True,
        "new_alias_execute_flag": False, "global_nx_verified": False,
        "tlb_scope": "current_cpu_scoped",
        "mapping_scope": "ktest_single_cpu_only",
        "production_mapping_available": False,
        "reservation_held_during_staging": True,
        "sections": sections, "span": span, "payload": payload,
        "relocations": relocations, "pages": pages,
        "reserved_layout": {"base": hex(base), "reserved_bytes": envelope,
                            "span": actual_span, "sections": actual_sections,
                            "payload": actual_payload, "relocations": actual_relocations,
                            "pages": actual_pages},
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    result = verify(path.read_bytes().decode(errors="replace"))
    path.with_name("result.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Verified scoped temporary RW mapping readback; mapping removed before return; no module execution.")


if __name__ == "__main__":
    main()
