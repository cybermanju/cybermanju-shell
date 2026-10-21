#!/usr/bin/env python3
"""Verify serial evidence from the focused OSTD staging tests, not .ko loading."""
import json
from pathlib import Path
import re
import sys

TESTS = (
    "real_wrapper_roundtrip_zero_gaps_tail_and_drop",
    "empty_and_invalid_budget",
    "allocation_failure_drops_all_acquired_real_frames",
    "copy_failure_drops_every_frame_acquired_before_copy",
    "metadata_drop_counter_stays_zero_while_owned_then_counts_all_frames",
)


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    for test in TESTS:
        marker = f"CYBERMANJU_STAGING_RUNTIME:{test}:PASS"
        if text.count(marker) != 1:
            raise ValueError(f"Expected exactly one runtime marker: {test}")
    if not re.search(r"test result:\s*ok\.\s*1 passed;\s*0 failed;\s*\d+ filtered out\.", text):
        raise ValueError("Missing focused runner summary: 1 passed, 0 failed")
    if re.search(r"test result:\s*FAILED|\b[1-9]\d* failed;", text):
        raise ValueError("Runner reported failures")
    return {"status": "passed", "runtime_tests": 1, "scenarios": len(TESTS), "failed": 0,
            "scope": "ostd_frame_staging_only", "linux_modules_loaded": False}


def main() -> None:
    path = Path(sys.argv[1])
    result = verify(path.read_text(errors="replace"))
    path.with_name("result.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Verified 1 OSTD staging runtime test with 5 scenarios; no Linux module was loaded or executed.")


if __name__ == "__main__":
    main()
