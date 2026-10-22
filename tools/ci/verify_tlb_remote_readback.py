#!/usr/bin/env python3
"""Validate narrowly scoped remote TLB byte-readback serial evidence."""
import json
from pathlib import Path
import re
import sys

TOPOLOGY = re.compile(r"CYBERMANJU_TLB_READBACK:topology:PASS cpus=2 source=([01]) target=([01])\Z")
MARKERS = (
    "CYBERMANJU_TLB_READBACK:bytes:PASS old=225 new=66 bytes=8 pge=true global=true retained_frames=2",
    "CYBERMANJU_TLB_READBACK:cleanup:PASS unmapped=true slot_reset=true pge_restored=true init_called=false",
)
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; \d+ filtered out\.\Z")
ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")


def verify(text: str) -> dict:
    text = ANSI.sub("", text)
    text = text.replace("\r", "")
    if re.search(r"[\x00-\x08\x0b-\x1f\x7f]", text):
        raise ValueError("Unexpected control character in runtime evidence")
    lines = text.split("\n")
    tops = [line for line in lines if line.startswith("CYBERMANJU_TLB_READBACK:topology:")]
    if len(tops) != 1 or not (match := TOPOLOGY.fullmatch(tops[0])):
        raise ValueError("Expected exactly one complete remote TLB topology marker")
    source, target = map(int, match.groups())
    if source == target:
        raise ValueError("Source and target CPUs must be distinct")
    for marker in MARKERS:
        if lines.count(marker) != 1:
            raise ValueError("Expected exactly one complete marker for each readback scenario")
    relevant = [line for line in lines if "CYBERMANJU_" in line]
    if len(relevant) != 3 or set(relevant) != {tops[0], *MARKERS}:
        raise ValueError("Unexpected, echoed, mixed or contradictory runtime marker")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]):
        raise ValueError("Expected one standalone summary: 1 passed, 0 failed")
    without_summary = "\n".join(line for line in lines if "test result:" not in line.lower())
    if re.search(r"\b(?:panic|panicked|fail|failed|failure)\b", without_summary, re.I):
        raise ValueError("Runner reported failure or panic")
    return {
        "status": "passed", "scope": "remote_tlb_byte_readback_only", "cpus": 2,
        "source": source, "target": target, "old_byte": 225, "new_byte": 66,
        "bytes": 8, "retained_frames": 2, "remote_bytes_observed": True,
        "mappings_removed": True, "remote_pge_restored": True,
        "negative_control_discriminates": False,
        "universal_coherence_verified": False, "frames_reclaimed_verified": False,
        "smp_mapping_supported": False, "linux_modules_executed": False, "init_called": False,
    }


def main() -> None:
    path = Path(sys.argv[1])
    result = verify(path.read_bytes().decode(errors="replace"))
    path.with_name("result.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Verified remote TLB byte readback only; no universal coherence or frame reclamation claimed.")


if __name__ == "__main__":
    main()
