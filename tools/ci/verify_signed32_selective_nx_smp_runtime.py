#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Verify two-CPU RW-factory denial, not a selective transition under SMP."""
import json
from pathlib import Path
import re
import sys

P = "CYBERMANJU_SIGNED32_SELECTIVE_NX_SMP:"
EXPECTED = [
    P + "begin:PASS cpus=2",
    P + "denied:PASS attempts=2 reclaims=2 acquired=6 dropped=6 live=0 irq_preserved=true factory_rw_denied_before_conversion=true",
    P + "complete:PASS selective_mapping=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false",
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
        raise ValueError("Expected exact ordered selective NX SMP denial markers")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if (
        len(summaries) != 1
        or not SUMMARY.fullmatch(summaries[0])
        or lines.index(summaries[0]) <= lines.index(EXPECTED[-1])
    ):
        raise ValueError("Expected one successful native summary after complete")
    return {
        "status": "passed", "architecture": "x86_64", "page_size": 4096,
        "scope": "smp_factory_rw_denial_before_selective_conversion_only",
        "boot_cpus": 2, "runtime_tests": 1, "denied_attempts": 2,
        "reclaimed_reservations": 2, "handles_acquired": 6, "handles_dropped": 6,
        "live_handles": 0, "irq_state_preserved": True,
        "factory_rw_denied_before_conversion": True, "selective_mapping": False,
        "module_execution": False, "physical_wx": False, "smp_mapping": False,
        "write_fault_probed": False, "normal_build_api_exists": True,
        "production_qualified": False,
    }


def main():
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    out = path.with_name("result.json")
    out.write_text('{"status":"not_verified","production_qualified":false}\n')
    out.write_text(json.dumps(verify(path.read_text(errors="replace")), indent=2) + "\n")
    print("Verified selective NX SMP factory denial only.")


if __name__ == "__main__":
    main()
