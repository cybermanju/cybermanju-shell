#!/usr/bin/env python3
"""Verify experimental single-CPU ELF signed32 persistent RW/non-executable lease evidence."""
import json
from pathlib import Path
import re
import sys

PREFIX = "CYBERMANJU_ELF_LEASE:"
BEGIN = PREFIX + "begin:PASS cpus=1 kinds=4"
CASE = re.compile(re.escape(PREFIX + "case:PASS ") +
    r"kind=(1|2|4|11) span=([0-9]+) envelope=([0-9]+) pages=([0-9]+) bytes=([0-9]+) relocation_bytes=(8|4) frame_matches=([0-9]+) nonx_pages=([0-9]+) slack_absent=([0-9]+) retained=true layout_preserved=true reclaimed=true")
KINDS = [1, 2, 4, 11]
RELOC = {1: 8, 2: 4, 4: 4, 11: 4}
FAULTS = PREFIX + "faults:PASS cases=3 prefix_pages=3 reclaims=3"
COMPLETE = PREFIX + "complete:PASS mappings=4 bounds_rejected=8 reclaims=7 elf_handoff=true module_execution=false physical_wx=false smp_mapping=false"
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")
FAILURE = re.compile(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", re.I)


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if FAILURE.search(text):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    if len(markers) != 7 or markers[0] != BEGIN or markers[5] != FAULTS or markers[6] != COMPLETE:
        raise ValueError("Expected exactly seven ordered standalone ELF lease markers")
    cases, envelopes, spans = [], set(), []
    for expected, line in zip(KINDS, markers[1:5]):
        m = CASE.fullmatch(line)
        if not m:
            raise ValueError("Malformed ELF lease case marker")
        kind, span, envelope, pages, byte_count, reloc, frames, nonx, slack = map(int, m.groups())
        if kind != expected or reloc != RELOC[kind]:
            raise ValueError("ELF case order or relocation width is invalid")
        if span <= 0 or span % 4096 or span > 1024 * 4096 or envelope < span or envelope % 4096 or envelope > 1024 * 4096:
            raise ValueError("Invalid span or envelope geometry")
        page_count = span // 4096
        if pages != page_count or byte_count != span + reloc or frames != page_count or nonx != page_count or slack != (envelope - span) // 4096:
            raise ValueError("ELF lease metrics disagree with geometry")
        spans.append(span); envelopes.add(envelope)
        cases.append({"kind": kind, "span": span, "envelope": envelope, "pages": pages, "bytes": byte_count, "slack": slack})
    if len(envelopes) != 1:
        raise ValueError("All cases must use the same envelope")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]) or lines.index(summaries[0]) <= lines.index(COMPLETE):
        raise ValueError("Expected one successful native summary after complete")
    return {
        "status": "passed", "scope": "experimental_elf_signed32_persistent_rw_nonx_single_cpu",
        "cpus": 1, "runtime_tests": 1, "mapped_elf_cases": 4, "kinds": KINDS,
        "mapped_spans": spans, "total_mapped_read_bytes": sum(c["bytes"] for c in cases),
        "relocation_bytes_checked": 20, "frame_identity_pages": sum(c["pages"] for c in cases),
        "nonx_pages_verified": sum(c["pages"] for c in cases), "slack_pages_absent": sum(c["slack"] for c in cases),
        "bounds_rejections": 8, "mapping_fault_cases": 3, "installed_fault_prefix_pages": 3,
        "reclaimed_reservations": 7, "elf_handoff_verified": True, "policy_opt_in_c_shims": True,
        "linux_modules_executed": False, "linux_abi_verified": False, "physical_wx_verified": False,
        "smp_mapping_verified": False, "production_mapping_ready": False, "tlb_shootdown_verified": False,
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1]); output = path.with_name("result.json")
    output.write_text('{"status":"not_verified"}\n')
    result = verify(path.read_text(errors="replace"))
    output.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified experimental ELF lease evidence only.")

if __name__ == "__main__":
    main()
