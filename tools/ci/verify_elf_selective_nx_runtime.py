#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Exact native evidence of ELF-derived NX projection, not executable loading."""
import json
from pathlib import Path
import re
import sys

P = "CYBERMANJU_ELF_SELECTIVE_NX:"
TOTAL_HANDLES = 4 * 10 + 3 * 10 + 4 * 10 + 2 * 3
READ_BYTES = 4 * 10 * 4096 + 8 + 3 * 4
EXPECTED = [
    P + "begin:PASS cpus=1",
    P + "faults:PASS cases=3 prefix=7 acquired=30 dropped=30 reclaims=3 irq_preserved=true",
    P + "budgets:PASS public=2 observed=2 mapper_entries=0 acquired=40 dropped=40 reclaims=4 irq_preserved=true",
    P + "gaps:PASS public=1 observed=1 inaccessible_pages=1 mapper_entries=0 acquired=6 dropped=6 reclaims=2 irq_preserved=true",
    *[
        P + f"case:PASS kind={kind} pages=10 intended_ro=4 intended_rw=4 intended_rx=2 ro_nonx=6 rw_nonx=4 pte_pages=10 frame_matches=10 supervisor=10 writeback=10 global=10 plan_retained=true mask_oracle=true irq_preserved=true"
        for kind in (1, 2, 4, 11)
    ],
    P + f"complete:PASS kinds=4 bytes={READ_BYTES} bounds=8 fault_cases=3 fault_prefix=7 budget_public=2 budget_observed=2 gap_public=1 gap_observed=1 reclaims=13 acquired={TOTAL_HANDLES} dropped={TOTAL_HANDLES} live=0 nx_projection_applied=true elf_rx_applied=false gap_mapping_supported=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false production_qualified=false",
]
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; \d+ filtered out\.")
FAILURE = re.compile(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", re.I)


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if FAILURE.search(text):
        raise ValueError("Native test failed")
    lines = text.splitlines()
    if [line for line in lines if "CYBERMANJU_" in line] != EXPECTED:
        raise ValueError("Expected exact ELF selective NX evidence: cases, gap preflight, counts and claims")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) <= lines.index(EXPECTED[-1]):
        raise ValueError("Expected one successful native summary after complete")
    return {
        "status": "passed", "architecture": "x86_64", "page_size": 4096,
        "scope": "ELF selective RO/RW non-executable alias projection", "boot_cpus": 1,
        "runtime_tests": 1, "cases": 4, "relocation_kinds": [1, 2, 4, 11],
        "intended_permissions_per_case": {"ro": 4, "rw": 4, "rx": 2},
        "applied_permissions_per_case": {"ro_nonx": 6, "rw_nonx": 4},
        "pte_observations": 4 * 10, "physical_frame_matches": 4 * 10,
        "readback_bytes": READ_BYTES, "relocation_oracle": True,
        "mask_oracle": True, "plan_retained": True, "irq_state_preserved": True,
        "bounds_rejections": 8, "fault_cases": 3, "fault_prefix_entries": 7,
        "fault_handles_acquired": 30, "fault_handles_dropped": 30,
        "public_budget_rejections": 2, "observed_budget_rejections": 2, "budget_mapper_entries": 0,
        "gap_public_rejections": 1, "gap_observed_rejections": 1,
        "inaccessible_pages_in_gap_fixture": 1, "gap_mapper_entries": 0,
        "reclaimed_reservations": 13, "handles_acquired": TOTAL_HANDLES,
        "handles_dropped": TOTAL_HANDLES, "live_handles": 0,
        "nx_projection_applied": True, "elf_rx_applied": False, "gap_mapping_supported": False,
        "module_execution": False, "physical_wx": False, "smp_mapping": False,
        "write_fault_probed": False, "production_qualified": False,
    }


def main():
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    out = path.with_name("result.json")
    out.write_text('{"status":"not_verified","production_qualified":false}\n')
    out.write_text(json.dumps(verify(path.read_text(errors="replace")), indent=2) + "\n")


if __name__ == "__main__":
    main()
