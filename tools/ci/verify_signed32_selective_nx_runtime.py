#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Verify exact native evidence for selective signed32 NX alias mapping."""
import json
from pathlib import Path
import re
import sys

P = "CYBERMANJU_SIGNED32_SELECTIVE_NX:"
EXPECTED = [
    P + "begin:PASS cpus=1",
    P + "sealed:PASS span=12288 pages=3 envelope_pages=4 pte_pages=3 frame_matches=3 ro_nonx=2 rw_nonx=1 supervisor=3 writeback=3 global=3 slack_absent=1 bytes=12288 mask_retained=true irq_preserved=true",
    P + "faults:PASS cases=3 prefix_pages=3 reclaims=3 acquired=9 dropped=9 live=0",
    P + "validation:PASS bounds=3 output_preserved=true irq_preserved=true malformed_masks=3 cleanup=true",
    P + "extremes:PASS cases=2 all_ro=3 all_rw=3 bytes=24576 irq_preserved=true",
    P + "complete:PASS protected_mappings=3 total_reclaims=9 acquired=27 dropped=27 live=0 elf_plan_applied=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false",
]
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")
FAILURE = re.compile(
    r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;",
    re.I,
)


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if FAILURE.search(text):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    marks = [line for line in lines if "CYBERMANJU_" in line]
    if marks != EXPECTED:
        raise ValueError("Expected exact ordered selective NX runtime markers and all counts/claims")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if (
        len(summaries) != 1
        or not SUMMARY.fullmatch(summaries[0])
        or lines.index(summaries[0]) <= lines.index(EXPECTED[-1])
    ):
        raise ValueError("Expected one successful native summary after complete")
    return {
        "status": "passed", "architecture": "x86_64", "page_size": 4096,
        "scope": "single_cpu_selective_ro_rw_nonexec_alias", "boot_cpus": 1,
        "runtime_tests": 1, "span_bytes": 12288, "pages": 3, "envelope_pages": 4,
        "pte_pages": 3, "frame_matches": 3, "read_only_nonexec_pages": 2,
        "rw_nonexec_pages": 1, "supervisor_pages": 3, "writeback_pages": 3,
        "global_pages": 3, "slack_absent": 1, "mask": [True, False, True],
        "mask_retained": True, "fault_cases": 3, "fault_prefix_pages": 3,
        "fault_reclaims": 3, "fault_handles_acquired": 9, "fault_handles_dropped": 9,
        "handles_acquired": 27, "handles_dropped": 27, "live_handles": 0,
        "bounds_rejections": 3, "output_preserved": True,
        "malformed_masks_rejected": 3, "cleanup": True, "irq_state_preserved": True,
        "boundary_cases": 2, "readback_bytes": 12288 + 24576,
        "protected_mappings": 3, "total_reclaims": 9,
        "elf_plan_applied": False, "module_execution": False, "physical_wx": False,
        "smp_mapping": False, "write_fault_probed": False,
        "normal_build_api_exists": True, "production_qualified": False,
    }


def main():
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    out = path.with_name("result.json")
    out.write_text('{"status":"not_verified","production_qualified":false}\n')
    out.write_text(json.dumps(verify(path.read_text(errors="replace")), indent=2) + "\n")
    print("Verified selective signed32 NX alias evidence only.")


if __name__ == "__main__":
    main()
