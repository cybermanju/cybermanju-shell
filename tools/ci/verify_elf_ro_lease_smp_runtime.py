#!/usr/bin/env python3
"""Validate bounded native evidence for SMP rejection of the ELF RO lease."""
import json
from pathlib import Path
import re
import sys

P = "CYBERMANJU_ELF_RO_LEASE_SMP:"
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; \d+ filtered out\.")
FAILURE = re.compile(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", re.I)

def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if FAILURE.search(text):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    if len(markers) != 3:
        raise ValueError("Expected exactly three standalone SMP markers")
    begin = re.fullmatch(re.escape(P + "begin:PASS cpus=2 kinds=4 pages=") + r"(\d+)", markers[0])
    denied = re.fullmatch(re.escape(P + "denied:PASS attempts=4 reclaims=4 acquired=") + r"(\d+) dropped=(\d+) live=0 irq_preserved=true", markers[1])
    if not begin or not denied:
        raise ValueError("Malformed or reordered SMP markers")
    pages, = map(int, begin.groups())
    acquired, dropped = map(int, denied.groups())
    if not 1 <= pages <= 1024 or acquired != 4 * pages or dropped != acquired:
        raise ValueError("SMP page/handle totals inconsistent")
    if markers[2] != P + "complete:PASS elf_handoff=false plan_applied=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false":
        raise ValueError("SMP completion capability claims invalid")
    summaries = [(i, line) for i, line in enumerate(lines) if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0][1]) or summaries[0][0] <= lines.index(markers[2]):
        raise ValueError("Expected one successful native summary after complete")
    return {"status": "passed", "arch": "x86_64", "scope": "smp_rejection_only", "cpus": 2, "pages": pages, "attempts": 4, "reclaims": 4, "handles_acquired": acquired, "handles_dropped": dropped, "live_handles": 0, "irq_preserved": True, "elf_handoff": False, "plan_applied": False, "module_execution": False, "physical_wx": False, "smp_mapping": False, "write_fault_probed": False, "linux_full_abi_verified": False, "production_qualification": False}

def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1]); output = path.with_name("result.json")
    output.write_text('{"status":"not_verified"}\n')
    try:
        result = verify(path.read_text(errors="replace"))
    except Exception as exc:
        print(f"Verification failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified SMP rejection evidence only.")

if __name__ == "__main__":
    main()
