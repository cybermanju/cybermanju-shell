#!/usr/bin/env python3
"""Bounded, read-only Linux process observer for an installer wrapper.

--proc-root is solely a host-side testing hook for fake proc fixtures; it does
not indicate or configure a boot environment. No environment variable changes it.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import time

PREFIX = "CYBERMANJU_INSTALL_OBSERVER:"
VERSION = 1
PHASE = "install-system"
MAX_SAMPLES = 240
MAX_PROCESSES = 12
MAX_DEPTH = 8
MAX_FILE = 32768
MAX_COUNTER = (1 << 64) - 1
MAX_PID = (1 << 31) - 1
IO_FIELDS = ("rchar", "wchar", "syscr", "syscw", "read_bytes", "write_bytes", "cancelled_write_bytes")


def _read_bounded(path: str, limit: int = MAX_FILE) -> bytes:
    with open(path, "rb") as stream:
        return stream.read(limit + 1)[:limit]


def parse_stat(text: str) -> dict:
    """Parse /proc/PID/stat robustly (comm may contain spaces and parentheses)."""
    left = text.find("(")
    right = text.rfind(")")
    if left < 0 or right <= left:
        raise ValueError("malformed stat")
    pid_text = text[:left].strip()
    fields = text[right + 1:].split()
    # Fields after comm begin with stat field 3; starttime is field 22.
    if len(fields) < 20:
        raise ValueError("short stat")
    result = {"pid": int(pid_text), "comm": text[left + 1:right][:256], "state": fields[0],
              "ppid": int(fields[1]), "utime_ticks": int(fields[11]),
              "stime_ticks": int(fields[12]), "starttime": int(fields[19])}
    if not (0 < result["pid"] <= MAX_PID and 0 <= result["ppid"] <= MAX_PID):
        raise ValueError("invalid process identity")
    if len(result["state"]) != 1 or any(not 0 <= result[k] <= MAX_COUNTER
                                       for k in ("utime_ticks", "stime_ticks", "starttime")):
        raise ValueError("invalid stat counters")
    return result


def _json_safe(raw: bytes, limit: int) -> tuple[str, bool]:
    truncated = len(raw) > limit
    piece = raw[:limit]
    # Escape embedded controls through JSON serialization; preserve invalid UTF-8.
    return piece.decode("utf-8", errors="backslashreplace"), truncated


def read_process(pid: int, proc_root: str = "/proc") -> dict:
    """Read one process observationally; inaccessible or racing fields are null."""
    item = {"pid": pid, "ppid": None, "comm": None, "state": None,
            "utime_ticks": None, "stime_ticks": None, "starttime": None,
            "argv": None, "argv_truncated": False, "wchan": None, "io": None,
            "unavailable": []}
    base = os.path.join(proc_root, str(pid))
    try:
        stat = parse_stat(_read_bounded(os.path.join(base, "stat"), 8192).decode("utf-8", "replace"))
        if stat["pid"] != pid:
            raise ValueError("stat identity mismatch")
        item.update(stat)
    except (OSError, ValueError, UnicodeError):
        item["unavailable"].append("stat")
    try:
        raw = _read_bounded(os.path.join(base, "cmdline"), 513)
        # Include a byte beyond 512 in the truncation determination.
        item["argv"], item["argv_truncated"] = _json_safe(raw, 512)
        if raw.endswith(b"\0"):
            item["argv"] = item["argv"].rstrip("\0").replace("\0", " ")
    except OSError:
        item["unavailable"].append("cmdline")
    try:
        raw = _read_bounded(os.path.join(base, "wchan"), 121)
        item["wchan"] = raw[:120].decode("utf-8", "backslashreplace").strip()
    except OSError:
        item["unavailable"].append("wchan")
    try:
        raw = _read_bounded(os.path.join(base, "io"), 4096).decode("ascii", "replace")
        values = {}
        for line in raw.splitlines():
            parts = line.split()
            if len(parts) == 2 and parts[0].rstrip(":") in IO_FIELDS:
                try:
                    value = int(parts[1])
                    if 0 <= value <= MAX_COUNTER:
                        values[parts[0].rstrip(":")] = value
                except ValueError:
                    pass
        item["io"] = {key: values.get(key) for key in IO_FIELDS}
    except OSError:
        item["unavailable"].append("io")
    if item["starttime"] is not None:
        if _parent_start(pid, proc_root) != item["starttime"]:
            # Never join one process's stat to another process after PID reuse.
            return {**{key: None for key in item}, "pid": pid,
                    "argv_truncated": False, "unavailable": ["identity_changed"]}
    return item


def _children(pid: int, proc_root: str) -> list[int]:
    try:
        data = _read_bounded(os.path.join(proc_root, str(pid), "task", str(pid), "children"), 4096)
        return [int(x) for x in data.split() if x.isdigit() and 0 < int(x) <= MAX_PID]
    except (OSError, ValueError):
        return []


def collect_sample(parent_pid: int, proc_root: str = "/proc", observer_pid: int | None = None, max_processes: int = MAX_PROCESSES) -> list[dict]:
    """Collect parent and bounded descendants without scanning /proc globally."""
    observer_pid = os.getpid() if observer_pid is None else observer_pid
    limit = min(MAX_PROCESSES, max(1, max_processes))
    ordered: list[int] = []
    queue: list[tuple[int, int]] = [(parent_pid, 0)]
    seen = {observer_pid}
    while queue and len(ordered) < limit:
        pid, depth = queue.pop(0)
        if pid in seen:
            continue
        seen.add(pid)
        ordered.append(pid)
        if depth < MAX_DEPTH:
            for child in _children(pid, proc_root):
                if child not in seen and len(ordered) + len(queue) < limit:
                    queue.append((child, depth + 1))
    # The traversal does not descend into the observer at all: it and its
    # subtree are pruned by `seen` before enqueue/child discovery.
    return [read_process(pid, proc_root) for pid in ordered][:limit]


def _emit(event: str, parent_pid: int, sample: int, elapsed: float, **extra) -> None:
    record = {"version": VERSION, "event": event, "phase": PHASE,
              "parent_pid": parent_pid, "sample": sample,
              "elapsed_seconds": round(elapsed, 3)}
    record.update(extra)
    try:
        sys.stderr.write("\n" + PREFIX + json.dumps(record, ensure_ascii=True, separators=(",", ":")) + "\n")
        sys.stderr.flush()
    except (OSError, ValueError):
        # A closed diagnostics pipe must never affect the observed installer.
        pass


def _parent_start(pid: int, root: str) -> int | None:
    try:
        stat = parse_stat(_read_bounded(os.path.join(root, str(pid), "stat"), 8192).decode("utf-8", "replace"))
        if stat["pid"] != pid or stat["state"] in ("Z", "X", "x"):
            return None
        return stat["starttime"]
    except (OSError, ValueError, UnicodeError):
        return None


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="bounded read-only Linux /proc observer")
    parser.add_argument("--parent", required=True, type=int)
    parser.add_argument("--stop-file", required=True)
    parser.add_argument("--phase", required=True, choices=[PHASE])
    parser.add_argument("--interval", type=float, default=30.0)
    parser.add_argument("--samples", type=int, default=120)
    parser.add_argument("--max-processes", type=int, default=12)
    parser.add_argument("--proc-root", default="/proc", help="host-only fake proc fixture testing hook; not a boot indicator")
    args = parser.parse_args(argv)
    if not 0 < args.parent <= MAX_PID or args.parent == os.getpid() or not (0.01 <= args.interval <= 300) or not (1 <= args.samples <= MAX_SAMPLES) or not (1 <= args.max_processes <= MAX_PROCESSES):
        parser.error("invalid bounded observer arguments")
    root = os.path.realpath(args.proc_root)
    initial = _parent_start(args.parent, root)
    if initial is None:
        _emit("end", args.parent, 0, 0.0, reason="parent_exited_or_unavailable")
        return 0
    started = time.monotonic()
    count = 0
    _emit("start", args.parent, 0, 0.0)
    reason = "sample_limit"
    deadline = started
    try:
        while count < args.samples:
            if os.path.exists(args.stop_file):
                reason = "stop_requested"
                break
            current = _parent_start(args.parent, root)
            if current is None or current != initial:
                reason = "parent_exited_or_unavailable"
                break
            count += 1
            _emit("sample", args.parent, count, time.monotonic() - started,
                  processes=collect_sample(args.parent, root, max_processes=args.max_processes))
            if count >= args.samples:
                reason = "sample_limit"
                break
            deadline += args.interval
            # Poll stop and parent identity frequently while respecting interval.
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    break
                if os.path.exists(args.stop_file):
                    reason = "stop_requested"
                    raise StopIteration
                current = _parent_start(args.parent, root)
                if current is None or current != initial:
                    reason = "parent_exited_or_unavailable"
                    raise StopIteration
                time.sleep(min(0.2, remaining))
    except StopIteration:
        pass
    except Exception as exc:
        # Diagnostic only; never signal or otherwise touch the target.
        _emit("end", args.parent, count, time.monotonic() - started,
              reason="observer_error", diagnostic=type(exc).__name__[:80])
        return 0
    _emit("end", args.parent, count, time.monotonic() - started, reason=reason)
    return 0


if __name__ == "__main__":
    main()
