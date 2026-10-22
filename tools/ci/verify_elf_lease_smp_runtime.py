#!/usr/bin/env python3
"""Verify experimental ELF persistent-lease SMP denial evidence only."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_ELF_LEASE_SMP:"
BEGIN = PREFIX + "begin:PASS cpus=2 kinds=4"
DENIED = PREFIX + "denied:PASS attempts=4 reclaims=4"
COMPLETE = PREFIX + "complete:PASS elf_handoff=false module_execution=false physical_wx=false smp_mapping=false"
EXPECTED = [BEGIN, DENIED, COMPLETE]
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")
FAILURE = re.compile(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", re.I)

def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if FAILURE.search(text): raise ValueError("Native runner reported failure")
    lines = text.splitlines(); markers = [line for line in lines if "CYBERMANJU_" in line]
    if markers != EXPECTED: raise ValueError("Expected exactly three ordered standalone ELF lease SMP markers")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) <= lines.index(COMPLETE):
        raise ValueError("Expected one successful native summary after complete")
    return {"status":"passed", "scope":"experimental_elf_persistent_lease_smp_denial_only", "cpus":2,
            "runtime_tests":1, "denied_attempts":4, "reclaimed_reservations":4,
            "elf_handoff_verified":False, "persistent_mapping_available":False,
            "linux_modules_executed":False, "linux_abi_verified":False, "physical_wx_verified":False,
            "smp_mapping_verified":False, "production_mapping_ready":False, "tlb_shootdown_verified":False}

def main() -> None:
    if len(sys.argv) != 2: raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path=Path(sys.argv[1]); output=path.with_name("result.json")
    output.write_text('{"status":"not_verified"}\n')
    result=verify(path.read_text(errors="replace")); output.write_text(json.dumps(result,indent=2)+"\n")
    print("Verified experimental ELF lease SMP denial only.")

if __name__ == "__main__": main()
