#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Verify prepared ELF factory denial on two CPUs, not an SMP mapping."""
import json
from pathlib import Path
import re
import sys
from verify_elf_selective_nx_runtime import FAILURE, SUMMARY

EXPECTED = [
    "CYBERMANJU_ELF_SELECTIVE_NX_SMP:begin:PASS cpus=2",
    "CYBERMANJU_ELF_SELECTIVE_NX_SMP:complete:PASS attempts=1 reclaims=1 acquired=10 dropped=10 live=0 irq_preserved=true factory_rw_denied_before_conversion=true selective_mapping=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false production_qualified=false",
]


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    lines = text.splitlines()
    if FAILURE.search(text) or [line for line in lines if "CYBERMANJU_" in line] != EXPECTED:
        raise ValueError("Expected exact ELF selective SMP factory-denial evidence")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) <= lines.index(EXPECTED[-1]):
        raise ValueError("Expected one successful native summary after complete")
    return {
        "status": "passed", "architecture": "x86_64", "page_size": 4096,
        "scope": "SMP factory denial before selective conversion", "boot_cpus": 2,
        "runtime_tests": 1, "attempts": 1, "reclaimed_reservations": 1,
        "handles_acquired": 10, "handles_dropped": 10, "live_handles": 0,
        "irq_state_preserved": True, "factory_rw_denied_before_conversion": True,
        "selective_mapping": False, "module_execution": False, "physical_wx": False,
        "smp_mapping": False, "write_fault_probed": False, "production_qualified": False,
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
