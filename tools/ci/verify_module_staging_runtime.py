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
    "reserved_owner_lifecycle",
)


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    text = re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]", "", text).replace("\r", "")
    lines = text.splitlines()
    for test in TESTS:
        marker = f"CYBERMANJU_STAGING_RUNTIME:{test}:PASS"
        if lines.count(marker) != 1:
            raise ValueError(f"Expected exactly one runtime marker: {test}")
    markers = [line for line in lines if "CYBERMANJU_" in line]
    expected = {f"CYBERMANJU_STAGING_RUNTIME:{test}:PASS" for test in TESTS}
    if len(markers) != len(TESTS) or set(markers) != expected:
        raise ValueError("Unexpected, mixed or contradictory runtime marker")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not re.fullmatch(r"test result: ok\. 1 passed; 0 failed; \d+ filtered out\.", summaries[0]):
        raise ValueError("Missing unique standalone summary: 1 passed, 0 failed")
    if re.search(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|runner failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", text, re.I):
        raise ValueError("Runner reported failures or panic")
    return {"status": "passed", "runtime_tests": 1, "scenarios": len(TESTS), "failed": 0,
            "scope": "ostd_frame_staging_only", "linux_modules_loaded": False}


def main() -> None:
    path = Path(sys.argv[1])
    result = verify(path.read_text(errors="replace"))
    path.with_name("result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(f"Verified 1 OSTD staging runtime test with {len(TESTS)} scenarios; no Linux module was loaded or executed.")


if __name__ == "__main__":
    main()
