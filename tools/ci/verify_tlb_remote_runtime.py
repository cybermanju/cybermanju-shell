#!/usr/bin/env python3
"""Validate serial acknowledgements for remote TLB dispatch only."""
import json
from pathlib import Path
import re
import sys

MARKERS = (
    re.compile(r"CYBERMANJU_TLB_REMOTE:topology:PASS cpus=2 source=([01]) target=([01])\Z"),
    "CYBERMANJU_TLB_REMOTE:dispatch:PASS cases=4 remote_batches=4 local_batches=1 global_batches=2",
    "CYBERMANJU_TLB_REMOTE:lifecycle:PASS empty_cases=2 sync_reused=true maps_installed=false init_called=false",
)
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; \d+ filtered out\.")


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    if re.search(r"[\x00-\x08\x0b-\x1f\x7f]", text):
        raise ValueError("Unexpected control character in runtime evidence")
    lines = text.splitlines()
    topology = [line for line in lines if line.startswith("CYBERMANJU_TLB_REMOTE:topology:")]
    if len(topology) != 1 or not (match := MARKERS[0].fullmatch(topology[0])):
        raise ValueError("Expected exactly one complete TLB remote topology marker")
    source, target = map(int, match.groups())
    if source == target:
        raise ValueError("Source and target CPUs must be distinct")
    for marker in MARKERS[1:]:
        if lines.count(marker) != 1:
            raise ValueError("Expected exactly one complete marker for each TLB remote scenario")
    relevant = [line for line in lines if "CYBERMANJU_" in line]
    if len(relevant) != 3 or set(relevant) != {topology[0], *MARKERS[1:]}:
        raise ValueError("Unexpected, echoed, mixed or contradictory runtime marker")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]):
        raise ValueError("Expected one standalone summary: 1 passed, 0 failed")
    without_summary = "\n".join(line for line in lines if "test result:" not in line.lower())
    if re.search(r"\b(?:panic|panicked|fail|failed|failure)\b", without_summary, re.I):
        raise ValueError("Runner reported failure or panic")
    return {
        "status": "passed", "scope": "remote_tlb_dispatch_ack_only", "cpus": 2,
        "source": source, "target": target, "dispatch_cases": 4,
        "remote_batches": 4, "local_batches": 1, "global_batches": 2,
        "empty_cases": 2, "remote_callback_observed": True,
        "remote_translation_coherence_verified": False,
        "frames_reclaimed_verified": False, "smp_mapping_supported": False,
        "linux_modules_executed": False, "init_called": False, "pages_mapped": False,
    }


def main() -> None:
    path = Path(sys.argv[1])
    result = verify(path.read_text(errors="replace"))
    path.with_name("result.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Verified remote TLB dispatch acknowledgements only; no hardware shootdown coherence claimed.")


if __name__ == "__main__":
    main()
