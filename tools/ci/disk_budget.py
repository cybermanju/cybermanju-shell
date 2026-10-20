#!/usr/bin/env python3
"""Best-effort disk budget guard for ISO builds.

This samples filesystem availability; it is not a hard quota, and a process can
consume disk between samples. Initialize against the workspace and Docker root
on the same filesystem so the baseline delta accounts for both.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

STANDARD_LIMIT_BYTES = 14_000_000_000
DEFAULT_RESERVE_BYTES = 1_073_741_824
SAMPLE_INTERVAL_SECONDS = 1.0
TERMINATE_GRACE_SECONDS = 5.0


def available_bytes(path: str) -> int:
    """Return space available to an unprivileged process on path's filesystem."""
    stats = os.statvfs(path)
    return stats.f_bavail * stats.f_frsize


def create_state(
    state_path: str,
    workspace_path: str,
    docker_root: str,
    limit_bytes: int = STANDARD_LIMIT_BYTES,
    reserve_bytes: int = DEFAULT_RESERVE_BYTES,
    initial_bytes: int = 0,
) -> dict[str, Any]:
    if limit_bytes <= 0:
        raise ValueError("--limit-bytes must be greater than zero")
    if initial_bytes < 0:
        raise ValueError("--initial-bytes cannot be negative")
    if reserve_bytes < 0:
        raise ValueError("--reserve-bytes cannot be negative")
    if reserve_bytes >= limit_bytes:
        raise ValueError("--reserve-bytes must be smaller than --limit-bytes")

    workspace_stat = os.stat(workspace_path)
    docker_stat = os.stat(docker_root)
    if workspace_stat.st_dev != docker_stat.st_dev:
        raise ValueError(
            "workspace and Docker root are on different filesystems; "
            "refusing to undercount disk use"
        )
    workspace_baseline = available_bytes(workspace_path)
    docker_baseline = available_bytes(docker_root)
    if min(workspace_baseline, docker_baseline) < reserve_bytes:
        baseline = min(workspace_baseline, docker_baseline)
        raise ValueError(
            f"available space ({baseline} bytes) is below the required "
            f"reserve ({reserve_bytes} bytes)"
        )

    state: dict[str, Any] = {
        "workspace_path": os.path.abspath(workspace_path),
        "docker_root": os.path.abspath(docker_root),
        "workspace_st_dev": workspace_stat.st_dev,
        "docker_root_st_dev": docker_stat.st_dev,
        "workspace_baseline_available_bytes": workspace_baseline,
        "docker_root_baseline_available_bytes": docker_baseline,
        "initial_bytes": initial_bytes,
        "baseline_available_bytes": workspace_baseline + initial_bytes,
        "limit_bytes": limit_bytes,
        "reserve_bytes": reserve_bytes,
    }
    destination = Path(state_path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    return state


def load_state(state_path: str) -> dict[str, Any]:
    with open(state_path, encoding="utf-8") as state_file:
        state = json.load(state_file)
    required = {
        "workspace_path",
        "docker_root",
        "workspace_st_dev",
        "docker_root_st_dev",
        "workspace_baseline_available_bytes",
        "docker_root_baseline_available_bytes",
        "baseline_available_bytes",
        "limit_bytes",
        "reserve_bytes",
    }
    missing = required.difference(state)
    if missing:
        raise ValueError(f"invalid state file; missing: {', '.join(sorted(missing))}")
    return state


def usage(state: dict[str, Any]) -> dict[str, int]:
    # Loaded CLI states always have device keys. Minimal unit-test snapshots do not.
    if "workspace_st_dev" in state:
        for path_key, device_key in (("workspace_path", "workspace_st_dev"),
                                     ("docker_root", "docker_root_st_dev")):
            if os.stat(state[path_key]).st_dev != state[device_key]:
                raise ValueError("filesystem changed since disk-budget initialization")
    current = available_bytes(state["workspace_path"])
    baseline = int(state["baseline_available_bytes"])
    reserve = int(state["reserve_bytes"])
    limit = int(state["limit_bytes"])
    allocated = baseline - current
    budget = limit - reserve
    return {
        "allocated_delta_bytes": allocated,
        "available_bytes": current,
        "baseline_available_bytes": baseline,
        "budget_bytes": budget,
        "reserve_bytes": reserve,
        "limit_bytes": limit,
        "used_budget_bytes": allocated,
    }


def format_usage(snapshot: dict[str, int]) -> str:
    return " ".join(f"{key}={value}" for key, value in snapshot.items())


def violation(snapshot: dict[str, int]) -> str | None:
    if snapshot["available_bytes"] < snapshot["reserve_bytes"]:
        return (
            f"physical available space {snapshot['available_bytes']} bytes is "
            f"below reserve {snapshot['reserve_bytes']} bytes"
        )
    if snapshot["allocated_delta_bytes"] > snapshot["budget_bytes"]:
        return (
            f"allocated delta {snapshot['allocated_delta_bytes']} bytes exceeds "
            f"budget {snapshot['budget_bytes']} bytes"
        )
    return None


def report_snapshot(snapshot: dict[str, int]) -> None:
    print(format_usage(snapshot), flush=True)


def budget_error(reason: str, snapshot: dict[str, int]) -> None:
    print(
        "::error::Disk budget exceeded: this workload does NOT fit standard "
        f"14GB ({reason}); {format_usage(snapshot)}",
        file=sys.stderr,
        flush=True,
    )


def _signal_process_group(process: subprocess.Popen[Any], signum: int) -> None:
    try:
        os.killpg(process.pid, signum)
    except ProcessLookupError:
        pass
    except OSError as error:
        print(f"disk_budget: could not signal process group: {error}", file=sys.stderr)


def _kill_builder_container() -> None:
    docker = shutil.which("docker")
    if not docker:
        return
    try:
        result = subprocess.run(
            [docker, "kill", "iso-builder"],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=15,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        print(f"disk_budget: docker kill iso-builder failed: {error}", file=sys.stderr)
        return
    for stream_name, output in (("stdout", result.stdout), ("stderr", result.stderr)):
        if output:
            print(f"docker kill iso-builder {stream_name}: {output.rstrip()}", file=sys.stderr)
    print(f"docker kill iso-builder exit_code={result.returncode}", file=sys.stderr)


def _stop_process_group(process: subprocess.Popen[Any]) -> None:
    _signal_process_group(process, signal.SIGTERM)
    _kill_builder_container()
    try:
        process.wait(timeout=TERMINATE_GRACE_SECONDS)
    except subprocess.TimeoutExpired:
        _signal_process_group(process, signal.SIGKILL)
        try:
            process.wait(timeout=TERMINATE_GRACE_SECONDS)
        except subprocess.TimeoutExpired:
            print("disk_budget: child did not exit after SIGKILL", file=sys.stderr)


def run_command(state: dict[str, Any], command: list[str]) -> int:
    if not command:
        raise ValueError("run requires a command after --")

    before = usage(state)
    report_snapshot(before)
    reason = violation(before)
    if reason:
        budget_error(reason, before)
        return 1

    process = subprocess.Popen(command, start_new_session=True)
    breach: tuple[str, dict[str, int]] | None = None
    try:
        while process.poll() is None:
            time.sleep(SAMPLE_INTERVAL_SECONDS)
            sample = usage(state)
            report_snapshot(sample)
            reason = violation(sample)
            if reason:
                breach = (reason, sample)
                _stop_process_group(process)
                break
    except (KeyboardInterrupt, OSError, ValueError):
        _stop_process_group(process)
        raise

    after = usage(state)
    report_snapshot(after)
    if breach is None:
        reason = violation(after)
        if reason:
            breach = (reason, after)
            _stop_process_group(process)
    if breach is not None:
        budget_error(*breach)
        return 1

    return process.wait()


def check_state(state: dict[str, Any]) -> int:
    snapshot = usage(state)
    report_snapshot(snapshot)
    reason = violation(snapshot)
    if reason:
        budget_error(reason, snapshot)
        return 1
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="action", required=True)

    init_parser = subparsers.add_parser("init", help="record the disk budget baseline")
    init_parser.add_argument("state", help="state JSON path")
    init_parser.add_argument("--path", required=True, help="workspace path")
    init_parser.add_argument("--docker-root", required=True, help="Docker data root path")
    init_parser.add_argument("--limit-bytes", type=int, default=STANDARD_LIMIT_BYTES)
    init_parser.add_argument("--reserve-bytes", type=int, default=DEFAULT_RESERVE_BYTES)
    init_parser.add_argument("--initial-bytes", type=int, default=0,
                             help="already allocated checkout bytes to include in budget")

    run_parser = subparsers.add_parser("run", help="run a command under the sampled budget guard")
    run_parser.add_argument("state", help="state JSON path")
    run_parser.add_argument("command", nargs=argparse.REMAINDER, help="command, usually after --")

    check_parser = subparsers.add_parser("check", help="print current usage and check the budget")
    check_parser.add_argument("state", help="state JSON path")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        if args.action == "init":
            state = create_state(
                args.state,
                args.path,
                args.docker_root,
                args.limit_bytes,
                args.reserve_bytes,
                args.initial_bytes,
            )
            print(
                "initialized disk budget: "
                f"workspace_st_dev={state['workspace_st_dev']} "
                f"docker_root_st_dev={state['docker_root_st_dev']} "
                f"baseline_available_bytes={state['baseline_available_bytes']} "
                f"limit_bytes={state['limit_bytes']} reserve_bytes={state['reserve_bytes']}"
            )
            return 0
        state = load_state(args.state)
        if args.action == "check":
            return check_state(state)
        command = list(args.command)
        if command and command[0] == "--":
            command = command[1:]
        return run_command(state, command)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"disk_budget: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
