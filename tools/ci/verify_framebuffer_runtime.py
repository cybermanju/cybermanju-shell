#!/usr/bin/env python3
"""Validate complete framebuffer runtime log evidence; VRAM readback is not scanout."""
import json
import re
import sys
from pathlib import Path

BOOT = re.compile(r"CYBERMANJU_FRAMEBUFFER:boot:PASS width=(\d+) height=(\d+) bpp=(\d+) pitch=(\d+) bytes=(\d+)")
IO = re.compile(r"CYBERMANJU_FRAMEBUFFER:io_readback:PASS rows=(\d+) row_bytes=(\d+)")
CONTRACT = "CYBERMANJU_FRAMEBUFFER_CONTRACT:PASS"
SUMMARY = re.compile(r"test result:\s*ok\.\s*(\d+) passed;\s*(\d+) failed;(?:\s*\d+ filtered out\.)?", re.I)
ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")

def verify(text, kind):
    text = ANSI.sub("", text).replace("\r", "")
    lines = text.splitlines()
    if any(re.match(r"^\s*(?:FAIL(?:ED)?\b|assert(?:ion)? panic\b|runner failed\b)", line, re.I) for line in lines):
        raise ValueError("failure evidence in runtime log")
    summaries = [tuple(map(int, m.groups())) for line in lines if (m := SUMMARY.fullmatch(line.strip()))]
    # Reject summary-like lines even if malformed/embedded, as well as multiple summaries.
    summary_lines = [line for line in lines if "test result:" in line.lower()]
    if len(summaries) != 1 or len(summary_lines) != 1 or summaries[0] != (1, 0):
        raise ValueError("expected exactly one full summary: 1 passed; 0 failed")
    contract_count = sum(line.count(CONTRACT) for line in lines)
    boots = [m for line in lines if (m := BOOT.fullmatch(line.strip()))]
    ios = [m for line in lines if (m := IO.fullmatch(line.strip()))]
    known_marker_lines = [line for line in lines if "CYBERMANJU_FRAMEBUFFER" in line]
    if any(not (line.strip() == CONTRACT or BOOT.fullmatch(line.strip()) or IO.fullmatch(line.strip())) for line in known_marker_lines):
        raise ValueError("unknown or malformed framebuffer marker")
    if kind == "contract":
        if contract_count != 1 or boots or ios:
            raise ValueError("expected exactly one contract marker and no hardware markers")
        scope = "framebuffer_boot_metadata_contract"
    elif kind == "hardware":
        if contract_count or len(boots) != 1 or len(ios) != 1:
            raise ValueError("expected exactly one boot and one io_readback marker")
        w, h, bpp, pitch, nbytes = map(int, boots[0].groups())
        rows, rowbytes = map(int, ios[0].groups())
        umax = 2**64 - 1
        if not (2 <= w <= umax and 2 <= h <= umax and bpp == 32 and pitch <= umax and nbytes <= umax and rows <= umax and rowbytes <= umax):
            raise ValueError("invalid or overflowing framebuffer geometry")
        if pitch < w * 4 or pitch * h > umax or nbytes != pitch * h or rows != 2 or rows > h or rowbytes != 8 or rowbytes > pitch:
            raise ValueError("inconsistent framebuffer geometry/readback")
        scope = "boot_framebuffer_mmio_only"
    else:
        raise ValueError("unknown kind")
    result = {"status": "passed", "kind": kind, "runtime_tests": 1, "failed": 0, "scope": scope}
    result.update(hardware_acceleration_verified=False, gpui_native_verified=False, physical_scanout_verified=False)
    if kind == "hardware":
        result["framebuffer"] = {"width": w, "height": h, "bpp": bpp, "pitch": pitch, "bytes": nbytes, "rows": rows, "row_bytes": rowbytes}
    return result

def main():
    path, kind = Path(sys.argv[1]), sys.argv[2]
    result = verify(path.read_text(errors="replace"), kind)
    dest = Path(sys.argv[3]) if len(sys.argv) > 3 else path.with_name("result.json")
    dest.write_text(json.dumps(result, indent=2) + "\n")
    print("Verified framebuffer runtime evidence.")

if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, IndexError) as exc:
        print(f"Framebuffer result not verified: {exc}", file=sys.stderr)
        raise SystemExit(1)
