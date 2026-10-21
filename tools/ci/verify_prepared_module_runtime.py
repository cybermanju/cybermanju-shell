#!/usr/bin/env python3
"""Verify prepared-module frame staging evidence, not Linux module execution."""
import json
from pathlib import Path
import re
import sys

SCENARIOS = (
    "roundtrip",
    "rejected_budgets_and_mismatched_layout",
    "no_init_or_exit_called",
)
PREFIX = "CYBERMANJU_PREPARED_RUNTIME:"
ROUNDTRIP = re.compile(
    r"CYBERMANJU_PREPARED_RUNTIME:roundtrip:PASS"
    r"\s+sections=(\d+)\s+span=(\d+)\s+payload=(\d+)"
    r"\s+relocations=(\d+)\s+pages=(\d+)"
)


def _strip_terminal_controls(text: str) -> str:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    return re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]", "", text)


def verify(text: str) -> dict:
    text = _strip_terminal_controls(text)
    for scenario in SCENARIOS:
        marker = f"{PREFIX}{scenario}:PASS"
        if text.count(marker) != 1:
            raise ValueError(f"Expected exactly one runtime marker: {scenario}")

    lines = re.findall(re.escape(PREFIX) + r"[^\r\n]*", text)
    if len(lines) != len(SCENARIOS):
        raise ValueError("Unexpected or contradictory runtime marker")
    for line in lines:
        if line.startswith(f"{PREFIX}roundtrip:"):
            if not ROUNDTRIP.fullmatch(line.strip()):
                raise ValueError("Malformed roundtrip marker")
        elif line.strip() not in {f"{PREFIX}{name}:PASS" for name in SCENARIOS[1:]}:
            raise ValueError("Malformed runtime marker")

    match = ROUNDTRIP.search(text)
    if not match:
        raise ValueError("Missing roundtrip marker counts")
    sections, span, payload, relocations, pages = map(int, match.groups())
    if not (0 < sections <= 4096 and 0 < pages <= 4096):
        raise ValueError("Section/page count outside supported positive budgets")
    if not (0 < span and span == pages * 4096 and 0 < payload <= span):
        raise ValueError("Invalid span/payload/page relationship")
    if not (0 < relocations <= 65536):
        raise ValueError("Relocation count outside supported positive budget")

    summaries = re.findall(r"test result:[^\r\n]*", text)
    if len(summaries) != 1 or not re.fullmatch(
        r"test result:\s*ok\.\s*1 passed;\s*0 failed;\s*\d+ filtered out\.", summaries[0].strip()
    ):
        raise ValueError("Missing focused runner summary: 1 passed, 0 failed")
    if re.search(r"test result:\s*FAILED|\b[1-9]\d* failed;", text):
        raise ValueError("Runner reported failures")
    return {
        "status": "passed",
        "runtime_tests": 1,
        "scope": "prepared_module_frame_staging_only",
        "fixture_staged": True,
        "linux_modules_executed": False,
        "init_called": False,
        "sections": sections,
        "span": span,
        "payload": payload,
        "relocations": relocations,
        "pages": pages,
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    result = verify(path.read_text(errors="replace"))
    path.with_name("result.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Verified prepared-module runtime staging evidence; no Linux module was executed.")


if __name__ == "__main__":
    main()
