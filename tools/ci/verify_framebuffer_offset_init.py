#!/usr/bin/env python3
"""Validate evidence from the isolated synthetic framebuffer offset-init VM."""
import json
import re
import sys
from pathlib import Path

MARKERS = (
    "CYBERMANJU_FB_OFFSET_INIT:acquire:PASS offset=4093 bytes=24 pages=2",
    "CYBERMANJU_FB_OFFSET_INIT:io:PASS guard_bytes=8 crossing_bytes=4",
    "CYBERMANJU_FB_OFFSET_INIT:lifetime:PASS release_reacquire=true restored=true",
    "CYBERMANJU_FB_OFFSET_INIT:scope:PASS synthetic_descriptor=true firmware_unaligned=false userspace=false",
)
SUMMARY = "test result: ok. 1 passed; 0 failed; 4 filtered out."
SUMMARY_PATTERN = re.compile(r"test result:\s*ok\.\s*(\d+) passed;\s*(\d+) failed;(?:\s*\d+ filtered out\.)?", re.I)


def verify(text, kind="offset-init"):
    if kind != "offset-init":
        raise ValueError("offset-init verifier cannot validate another runtime kind")
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    text = text.replace("\r\n", "\n")
    if "\r" in text:
        raise ValueError("isolated carriage return in offset-init log")
    lines = text.splitlines()
    marker_lines = [line for line in lines if "CYBERMANJU_" in line]
    if len(marker_lines) != len(MARKERS) or any(line not in MARKERS for line in marker_lines):
        raise ValueError("unexpected, echoed, mixed-scope or corrupt offset-init marker")
    if any(marker_lines.count(marker) != 1 for marker in MARKERS):
        raise ValueError("each required offset-init marker must occur exactly once")
    summaries = [m.groups() for line in lines if (m := SUMMARY_PATTERN.fullmatch(line.strip()))]
    if len(summaries) != 1 or summaries[0] != ("1", "0") or sum("test result:" in line.lower() for line in lines) != 1:
        raise ValueError("expected exactly one successful isolated Rust test summary")
    if any("test result:" not in line.lower() and re.search(r"(?:\bFAIL(?:ED)?\b|panic|panicked|assertion.*failed|runner failed)", line, re.I) for line in lines):
        raise ValueError("panic or contradictory failure evidence")
    return {
        "status": "passed",
        "scope": "framebuffer_synthetic_descriptor_real_acquisition",
        "production_constructor_verified": True,
        "actual_mmio_acquisition_verified": True,
        "reacquisition_verified": True,
        "bytes_restored": True,
        "last_view_retains_reservation_verified": True,
        "synthetic_descriptor": True,
        "bootloader_unaligned_framebuffer_verified": False,
        "userspace_syscall_verified": False,
        "mmap_userspace_verified": False,
        "gpui_native_verified": False,
        "linux_modules_executed": False,
    }


def main():
    path = Path(sys.argv[1])
    result = verify(path.read_bytes().decode("utf-8"), sys.argv[2] if len(sys.argv) > 2 else "offset-init")
    dest = Path(sys.argv[3]) if len(sys.argv) > 3 else path.with_name("result.json")
    dest.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified framebuffer offset initialization evidence.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, IndexError, UnicodeError) as exc:
        print(f"Framebuffer offset initialization not verified: {exc}", file=sys.stderr)
        raise SystemExit(1)
