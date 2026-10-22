#!/usr/bin/env python3
"""Verify the deliberately narrow direct-fileops fbdev runtime evidence."""
import json
import re
import sys
from pathlib import Path

SURFACE = "CYBERMANJU_FBDEV:surface:PASS bytes=24 pitch=12 smem_len=24 mapping_larger=true"
FILEOPS = "CYBERMANJU_FBDEV:fileops:PASS read_cases=4 write_cases=4 guard_bytes=8 cursors=true"
SCOPE = "CYBERMANJU_FBDEV:scope:PASS direct_fileops=true userspace_syscall=false mmap_tested=false fbdev_init_called=false"
UNALIGNED_FILEOPS = "CYBERMANJU_FBDEV:unaligned_fileops:PASS cases=4 bytes=24 mmap_rejected=true"
SUMMARY = re.compile(r"test result:\s*ok\.\s*(\d+) passed;\s*(\d+) failed;(?:\s*\d+ filtered out\.)?", re.I)
ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
MARKERS = (SURFACE, FILEOPS, SCOPE, UNALIGNED_FILEOPS)


def verify(text, kind="fbdev"):
    if kind != "fbdev":
        raise ValueError("unknown kind")
    text = ANSI.sub("", text)
    # Permit CRLF only. A bare CR may conceal a prior marker, so reject it.
    if "\r" in text.replace("\r\n", ""):
        raise ValueError("bare carriage return in runtime log")
    lines = text.replace("\r\n", "\n").split("\n")
    if any("test result:" not in line.lower() and re.search(r"(?:\bFAIL(?:ED)?\b|panic|panicked|assertion.*failed|runner failed)", line, re.I) for line in lines):
        raise ValueError("failure evidence in runtime log")
    summaries = [m.groups() for line in lines if (m := SUMMARY.fullmatch(line.strip()))]
    if len(summaries) != 1 or summaries[0] != ("1", "0") or sum("test result:" in x.lower() for x in lines) != 1:
        raise ValueError("expected exactly one test summary: 1 passed; 0 failed")
    marker_lines = [line for line in lines if "CYBERMANJU_FBDEV" in line]
    if len(marker_lines) != 4 or any(line not in MARKERS for line in marker_lines):
        raise ValueError("unknown, malformed, echoed or duplicate fbdev marker")
    if any(marker_lines.count(m) != 1 for m in MARKERS):
        raise ValueError("expected exactly one of each fbdev marker")
    return {
        "status": "passed", "scope": "fbdev_direct_fileops_only",
        "direct_fileops_verified": True, "userspace_syscall_verified": False,
        "mmap_tail_isolated": False, "fbdev_init_called": False,
        "unaligned_fileops_verified": True, "unaligned_mmap_admission_rejected": True,
        "mmap_userspace_verified": False,
        "linux_modules_executed": False,
        "surface": {"bytes": 24, "pitch": 12, "smem_len": 24, "mapping_larger": True},
        "read_cases": 4, "write_cases": 4, "guard_bytes": 8,
    }


def main():
    path, kind = Path(sys.argv[1]), sys.argv[2]
    result = verify(path.read_bytes().decode("utf-8"), kind)
    dest = Path(sys.argv[3]) if len(sys.argv) > 3 else path.with_name("result.json")
    dest.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified fbdev direct-fileops runtime evidence.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, IndexError, UnicodeError) as exc:
        print(f"fbdev result not verified: {exc}", file=sys.stderr)
        raise SystemExit(1)
