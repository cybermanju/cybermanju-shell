#!/usr/bin/env python3
"""Verify prepared bytes at a reserved base, not module mapping or execution."""
import json
from pathlib import Path
import re
import sys

SCENARIOS = ("roundtrip", "rejected_budgets_and_mismatched_layout", "reserved_layout", "no_init_or_exit_called")
PREFIX = "CYBERMANJU_PREPARED_RUNTIME:"
COUNTS = r"sections=(\d+) span=(\d+) payload=(\d+) relocations=(\d+) pages=(\d+)"
ROUNDTRIP = re.compile(re.escape(PREFIX + "roundtrip:PASS ") + COUNTS)
RESERVED = re.compile(re.escape(PREFIX + "reserved_layout:PASS ") + r"base=(0x[0-9a-fA-F]+) reserved=(\d+) span=(\d+) sections=(\d+) payload=(\d+) relocations=(\d+) pages=(\d+)")


def _strip_terminal_controls(text: str) -> str:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    return re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]", "", text).replace("\r", "")


def _counts_valid(sections, span, payload, relocations, pages):
    return (0 < sections <= 4096 and 0 < pages <= 4096
            and span == pages * 4096 and 0 < payload <= span
            and 0 < relocations <= 65536)


def verify(text: str) -> dict:
    text = _strip_terminal_controls(text)
    lines = text.splitlines()
    markers = [line for line in lines if "CYBERMANJU_" in line]
    if len(markers) != len(SCENARIOS):
        raise ValueError("Unexpected or contradictory runtime marker")
    simple = {PREFIX + name + ":PASS" for name in (SCENARIOS[1], SCENARIOS[3])}
    rounds = [m for line in markers if (m := ROUNDTRIP.fullmatch(line))]
    reserved = [m for line in markers if (m := RESERVED.fullmatch(line))]
    if len(rounds) != 1 or len(reserved) != 1 or set(markers) != simple | {rounds[0].group(), reserved[0].group()}:
        raise ValueError("Expected exactly one complete marker for every scenario")
    sections, span, payload, relocations, pages = map(int, rounds[0].groups())
    if not _counts_valid(sections, span, payload, relocations, pages):
        raise ValueError("Invalid numeric-layout counts or budgets")
    base_text, *counts = reserved[0].groups()
    base = int(base_text, 16)
    envelope, actual_span, actual_sections, actual_payload, actual_relocations, actual_pages = map(int, counts)
    if not _counts_valid(actual_sections, actual_span, actual_payload, actual_relocations, actual_pages):
        raise ValueError("Invalid reserved-layout counts or budgets")
    if not (0xffff800000000000 <= base < base + envelope <= 0xffffffffffff0000
            and base % 4096 == 0 and envelope % 4096 == 0
            and 0 < actual_span <= envelope <= 4 * 1024 * 1024):
        raise ValueError("Invalid reserved canonical range or envelope")
    if (actual_sections, actual_payload, actual_relocations) != (sections, payload, relocations):
        raise ValueError("Reserved layout does not describe the same fixture contents")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not re.fullmatch(r"test result: ok\. 1 passed; 0 failed; \d+ filtered out\.", summaries[0]):
        raise ValueError("Missing unique focused summary: 1 passed, 0 failed")
    if re.search(r"\b(?:panic|panicked)\b|assertion\b[^\n]*failed\b|runner failed\b|test result:\s*FAILED|\b[1-9]\d* failed;", text, re.I):
        raise ValueError("Runner reported failure or panic")
    return {
        "status": "passed", "runtime_tests": 1,
        "scope": "prepared_module_reserved_base_staging_only",
        "fixture_staged": True, "linux_modules_executed": False, "init_called": False,
        "pages_mapped": False, "wx_verified": False,
        "reservation_held_during_staging": True,
        "sections": sections, "span": span, "payload": payload,
        "relocations": relocations, "pages": pages,
        "reserved_layout": {"base": hex(base), "reserved_bytes": envelope,
                            "span": actual_span, "sections": actual_sections,
                            "payload": actual_payload, "relocations": actual_relocations,
                            "pages": actual_pages},
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    result = verify(path.read_text(errors="replace"))
    path.with_name("result.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Verified prepared-module bytes at reserved base; no mappings or module execution.")


if __name__ == "__main__":
    main()
