#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Verify actual supervisor byte-store faults, not executable module support."""
import json
from pathlib import Path
import re
import sys

P = "CYBERMANJU_SIGNED32_WRITE_FAULT:"
EXPECTED = [
    P + "begin:PASS cpus=1",
    P + "guard:PASS rejected=12 production_hook=false exact_site=true exact_cr2=true exact_error=true supervisor_only=true single_cpu=true",
    P + "rw_before:PASS stores=3 traps=0 wp_enabled=true irq_preserved=true",
    P + "sealed:PASS pte_pages=3 frame_matches=3 ro_nonx=2 rw_nonx=1 supervisor=3 writeback=3 global=3 slack_absent=1",
    *[
        P + f"fault:PASS offset={offset} traps=1 error_code=3 cr2_matches=true unchanged=true wp_enabled=true caller_irq_disabled={str(i % 2 == 1).lower()} irq_preserved=true"
        for i, offset in enumerate((0, 4095, 8192, 12287))
    ],
    P + "rw_after:PASS stores=2 traps=0 wp_enabled=true irq_preserved=true",
    P + "linear:PASS bytes=1 changed_readback=true linear_rw_alias=true immutable_data=false",
    P + "complete:PASS faults=4 rw_stores=5 bytes=12288 acquired=3 dropped=3 live=0 reclaims=1 pte_absent=4 irq_preserved=true write_fault_probed=true recovery_ktest_only=true physical_wx=false rx_enabled=false elf_execution=false smp_mapping=false production_qualified=false",
]
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; \d+ filtered out\.")
FAILURE = re.compile(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", re.I)


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    lines = text.splitlines()
    if FAILURE.search(text):
        raise ValueError("Native write-fault test failed")
    if [line for line in lines if "CYBERMANJU_" in line] != EXPECTED:
        raise ValueError("Expected exact signed32 hardware write-fault evidence and capability limits")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) <= lines.index(EXPECTED[-1]):
        raise ValueError("Expected one successful native summary after complete")
    return {
        "status": "passed", "scope": "ktest-only single-CPU data-alias supervisor write denial",
        "architecture": "x86_64", "page_size": 4096, "boot_cpus": 1, "runtime_tests": 1,
        "synthetic_context_rejections": 12, "hardware_write_faults": 4,
        "fault_offsets": [0, 4095, 8192, 12287], "fault_error_code": 3,
        "fault_address_verified": True, "faulting_data_unchanged": True,
        "cr0_wp_already_enabled": True, "cr0_wp_modified": False,
        "rw_control_stores": 5, "caller_irq_states": ["enabled", "disabled"],
        "irq_state_preserved": True, "pte_pages": 3, "physical_frame_matches": 3,
        "ro_nonx": 2, "rw_nonx": 1, "full_span_readback_bytes": 12288,
        "linear_rw_alias": True, "linear_alias_mutation_observed": True, "immutable_data": False,
        "handles_acquired": 3, "handles_dropped": 3, "live_handles": 0,
        "reclaimed_reservations": 1, "pte_absent_after_drop": 4,
        "write_fault_probed": True, "recovery_ktest_only": True, "production_hook": False,
        "physical_wx": False, "rx_enabled": False, "elf_execution": False,
        "smp_mapping": False, "production_qualified": False,
    }


def main():
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    out = path.with_name("result.json")
    out.write_text('{"status":"not_verified","elf_execution":false}\n')
    out.write_text(json.dumps(verify(path.read_text(errors="replace")), indent=2) + "\n")


if __name__ == "__main__":
    main()
