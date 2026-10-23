#!/usr/bin/env python3
"""Validate bounded native evidence for the experimental ELF RO lease runtime."""
import json
from pathlib import Path
import re
import sys

P = "CYBERMANJU_ELF_RO_LEASE:"
KINDS = [1, 2, 4, 11]
RELOC = {1: 8, 2: 4, 4: 4, 11: 4}
CASE = re.compile(re.escape(P + "case:PASS ") + r"kind=(\d+) span=(\d+) envelope=(\d+) pages=(\d+) bytes=(\d+) relocation_bytes=(\d+) frame_matches=(\d+) ro_nonx=(\d+) rw_nonx=(\d+) planned_pages=(\d+) planned_rx=(\d+) acquired=(\d+) dropped=(\d+) retained=(true|false) layout_preserved=(true|false) plan_preserved=(true|false) reclaimed=(true|false)")
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; \d+ filtered out\.")
FAILURE = re.compile(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", re.I)

def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if FAILURE.search(text):
        raise ValueError("Native runner reported failure")
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    if len(markers) != 8:
        raise ValueError("Expected exactly eight standalone RO lease markers")
    if markers[0] != P + "begin:PASS cpus=1 kinds=4":
        raise ValueError("Invalid begin marker")
    cases = []
    for kind_expected, line in zip(KINDS, markers[1:5]):
        m = CASE.fullmatch(line)
        if not m:
            raise ValueError("Malformed RO lease case marker")
        vals = m.groups()
        kind, span, envelope, pages, byte_count, reloc, frames, ro, rw, planned, rx, acquired, dropped = map(int, vals[:13])
        flags = vals[13:]
        if kind != kind_expected or reloc != RELOC[kind]:
            raise ValueError("Case kind order or relocation width invalid")
        if not 1 <= pages <= 1024 or span != pages * 4096 or envelope < span or envelope % 4096 or envelope > 1024 * 4096:
            raise ValueError("Invalid observed span/envelope geometry")
        if byte_count != span + reloc or frames != pages or ro != pages or rw != 0 or planned != pages or not 0 < rx <= pages or acquired != pages or dropped != pages:
            raise ValueError("Case metrics inconsistent with observed geometry")
        if flags != ("true", "true", "true", "true"):
            raise ValueError("Case retention/layout/reclaim claim invalid")
        cases.append({"kind": kind, "span": span, "envelope": envelope, "pages": pages, "bytes": byte_count, "relocation_bytes": reloc, "frame_matches": frames, "ro_nonx": ro, "rw_nonx": rw, "planned_pages": planned, "planned_rx": rx, "acquired": acquired, "dropped": dropped, "retained": True, "layout_preserved": True, "plan_preserved": True, "reclaimed": True})
    if len({c["pages"] for c in cases}) != 1:
        raise ValueError("All clone cases must have equal page geometry")
    pages = cases[0]["pages"]
    if markers[5] != f"{P}faults:PASS cases=3 prefix_pages={pages + 1} reclaims=3 acquired={3 * pages} dropped={3 * pages} live=0":
        raise ValueError("Fault/budget prefix counters inconsistent")
    if markers[6] != f"{P}budgets:PASS cases=2 public_cases=2 zero_budget=0 short_budget={pages - 1} mapper_entries=0 reclaims=4 acquired={4 * pages} dropped={4 * pages} live=0":
        raise ValueError("Budget counters inconsistent")
    total = sum(c["pages"] for c in cases) + 7 * pages
    expected = f"{P}complete:PASS mappings=4 bounds_rejected=8 reclaims=11 acquired={total} dropped={total} live=0 elf_handoff=true plan_applied=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false"
    if markers[7] != expected:
        raise ValueError("Completion counters/capability claims invalid")
    summaries = [(i, line) for i, line in enumerate(lines) if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0][1]) or summaries[0][0] <= lines.index(markers[7]):
        raise ValueError("Expected one successful native summary after complete")
    return {"status": "passed", "arch": "x86_64", "scope": "single_cpu_elf_uniform_read_only_nonexec_alias", "cpus": 1, "mapped_elf_cases": 4, "cases": cases, "mapped_spans": [c["span"] for c in cases], "total_mapped_read_bytes": sum(c["bytes"] for c in cases), "relocation_bytes_checked": sum(c["relocation_bytes"] for c in cases), "handles_acquired": total, "handles_dropped": total, "reclaims": 11, "faults": {"cases": 3, "prefix_pages": pages + 1, "reclaims": 3, "acquired": 3 * pages, "dropped": 3 * pages, "live": 0}, "budgets": {"cases": 2, "zero_budget": 0, "short_budget": pages - 1, "public_cases": 2, "mapper_entries": 0, "reclaims": 4, "acquired": 4 * pages, "dropped": 4 * pages, "live": 0}, "elf_handoff": True, "layout_preserved": True, "planned_metadata_preserved": True, "plan_applied": False, "module_execution": False, "physical_wx": False, "smp_mapping": False, "write_fault_probed": False, "immutable_data": False, "all_ptes_read_only_nx_including_intended_rw_data": True, "linux_full_abi_verified": False, "production_qualification": False}

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
    print("Verified experimental ELF RO lease evidence only.")

if __name__ == "__main__":
    main()
