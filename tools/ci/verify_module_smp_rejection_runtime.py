#!/usr/bin/env python3
"""Verify explicit module-mapping rejection evidence on a two-CPU VM only."""
import json
from pathlib import Path
import re
import sys

MARKERS = (
    "CYBERMANJU_MODULE_SMP_REJECTION:topology:PASS cpus=2",
    "CYBERMANJU_MODULE_SMP_REJECTION:denied:PASS cases=3",
    "CYBERMANJU_MODULE_SMP_REJECTION:owner_released:PASS pages=3",
)
SUMMARY = re.compile(r"test result: ok\. 1 passed; 0 failed; [0-9]+ filtered out\.")


def verify(text: str) -> dict:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text).replace("\r", "")
    lines = text.splitlines()
    marker_lines = [line for line in lines if "CYBERMANJU_" in line]
    if len(marker_lines) != 3 or set(marker_lines) != set(MARKERS):
        raise ValueError("Expected exactly the three complete SMP-rejection markers")
    if any(lines.count(marker) != 1 for marker in MARKERS):
        raise ValueError("SMP-rejection marker duplicated")
    summaries = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or not SUMMARY.fullmatch(summaries[0]):
        raise ValueError("Expected one standalone summary: 1 passed; 0 failed")
    text_without_summary = "\n".join(line for line in lines if "test result:" not in line.lower())
    if re.search(r"\b(?:panic|panicked|failure|fail|failed)\b|assertion\b[^\n]*failed\b", text_without_summary, re.I):
        raise ValueError("Runtime log contains panic or failure evidence")
    return {"status": "passed", "scope": "module_mapping_smp_rejection_only",
            "cpus": 2, "rejected_calls": 3, "pages_mapped": False,
            "init_called": False, "smp_mapping_supported": False,
            "production_mapping_available": False}


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(f"Usage: {Path(sys.argv[0]).name} <runtime-log>")
    path = Path(sys.argv[1])
    result = verify(path.read_text(errors="replace"))
    path.with_name("result.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Verified explicit three-call SMP mapping rejection only; no mappings or init.")


if __name__ == "__main__":
    main()
