#!/usr/bin/env python3
"""Report GPUI shell graphics prerequisites visible to this process."""

from __future__ import annotations

import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


DRM_DIRECTORY = Path("/dev/dri")
LAYER_SHELL_INTERFACE = "zwlr_layer_shell_v1"
NON_CPU_GPU_TYPES = {
    "VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU",
    "VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU",
    "VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU",
}
COMMAND_TIMEOUT_SECONDS = 60


@dataclass(frozen=True)
class Check:
    name: str
    passed: bool
    detail: str
    action: str = ""


def _command_error(command: list[str], error: OSError | subprocess.TimeoutExpired) -> str:
    rendered = " ".join(command)
    if isinstance(error, FileNotFoundError):
        return f"{command[0]} is not installed or not on PATH."
    if isinstance(error, subprocess.TimeoutExpired):
        return f"{rendered} timed out after {COMMAND_TIMEOUT_SECONDS} seconds."
    return f"Could not run {rendered}: {error}."


def _run(command: list[str], *, cwd: str | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        check=False,
        capture_output=True,
        text=True,
        timeout=COMMAND_TIMEOUT_SECONDS,
        cwd=cwd,
    )


def check_drm_nodes() -> Check:
    """Require accessible character-device primary and render nodes."""
    if not DRM_DIRECTORY.is_dir():
        return Check(
            "DRM nodes",
            False,
            f"{DRM_DIRECTORY} is absent or not a directory.",
            "Expose the host's real DRM device nodes (usually /dev/dri/cardN and renderDNNN) to this session.",
        )

    primary: list[str] = []
    render: list[str] = []
    try:
        entries = sorted(DRM_DIRECTORY.iterdir(), key=lambda path: path.name)
    except OSError as error:
        return Check(
            "DRM nodes",
            False,
            f"Cannot inspect {DRM_DIRECTORY}: {error}.",
            "Check device-directory permissions and whether the DRM devices are exposed.",
        )

    for path in entries:
        if re.fullmatch(r"card\d+", path.name):
            destination = primary
        elif re.fullmatch(r"renderD\d+", path.name):
            destination = render
        else:
            continue
        try:
            mode = path.stat(follow_symlinks=False).st_mode
        except OSError:
            continue
        if stat.S_ISCHR(mode) and os.access(path, os.R_OK | os.W_OK):
            destination.append(str(path))

    missing = []
    if not primary:
        missing.append("an accessible primary cardN node")
    if not render:
        missing.append("an accessible renderDNNN node")
    if missing:
        found = []
        if primary:
            found.append("primary: " + ", ".join(primary))
        if render:
            found.append("render: " + ", ".join(render))
        detail = "; ".join(found) if found else "No accessible DRM character-device nodes found."
        return Check(
            "DRM nodes",
            False,
            detail + "; missing " + " and ".join(missing) + ".",
            "Expose the real DRM nodes to this session and grant the user read/write access (commonly via the video/render groups).",
        )

    return Check(
        "DRM nodes",
        True,
        f"primary: {', '.join(primary)}; render: {', '.join(render)}.",
    )


def _physical_device_properties(document: Any) -> tuple[str, str]:
    """Extract VkPhysicalDeviceProperties from vulkaninfo's Profiles JSON."""
    try:
        properties = document["capabilities"]["device"]["properties"][
            "VkPhysicalDeviceProperties"
        ]
        device_type = properties["deviceType"]
        device_name = properties["deviceName"]
    except (KeyError, TypeError) as error:
        raise ValueError(
            "missing capabilities.device.properties.VkPhysicalDeviceProperties fields"
        ) from error
    if not isinstance(device_type, str) or not isinstance(device_name, str):
        raise ValueError("deviceType and deviceName must be strings")
    return device_type, device_name


def _inspect_vulkan_json(device_index: int) -> tuple[str, str, str]:
    """Ask vulkaninfo for one indexed device and parse its documented JSON file."""
    command = ["vulkaninfo", f"--json={device_index}"]
    with tempfile.TemporaryDirectory(prefix="gpui-vulkaninfo-") as output_directory:
        try:
            completed = _run(command, cwd=output_directory)
        except (OSError, subprocess.TimeoutExpired) as error:
            return "", "", _command_error(command, error)
        if completed.returncode != 0:
            message = (completed.stderr or completed.stdout).strip().replace("\n", " ")
            return "", "", f"vulkaninfo --json={device_index} exited {completed.returncode}: {message[:300]}"
        outputs = list(Path(output_directory).glob("*.json"))
        if len(outputs) != 1:
            return "", "", (
                f"vulkaninfo --json={device_index} exited successfully but produced "
                f"{len(outputs)} JSON files (expected one)."
            )
        try:
            document = json.loads(outputs[0].read_text(encoding="utf-8"))
            device_type, device_name = _physical_device_properties(document)
        except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as error:
            return "", "", f"Cannot read device {device_index} Vulkan Profiles JSON: {error}."
        return device_type, device_name, ""


def check_vulkan() -> Check:
    """Require an enumerated non-CPU GPU device type in actual vulkaninfo JSON."""
    command = ["vulkaninfo", "--summary"]
    try:
        completed = _run(command)
    except (OSError, subprocess.TimeoutExpired) as error:
        return Check(
            "Vulkan device",
            False,
            _command_error(command, error),
            "Install the Vulkan loader utilities (vulkaninfo) and a working Vulkan ICD, then retry.",
        )
    if completed.returncode != 0:
        message = (completed.stderr or completed.stdout).strip().replace("\n", " ")
        return Check(
            "Vulkan device",
            False,
            f"vulkaninfo --summary exited {completed.returncode}: {message[:300] or 'no diagnostic output'}.",
            "Install/configure a Vulkan ICD and ensure this user can access it; then rerun vulkaninfo --summary.",
        )

    indices = [int(match.group(1)) for match in re.finditer(r"^\s*GPU(\d+)\s*:\s*$", completed.stdout, re.MULTILINE)]
    if not indices:
        return Check(
            "Vulkan device",
            False,
            "vulkaninfo --summary reported no indexed physical devices.",
            "Install/configure a Vulkan ICD and verify it with vulkaninfo --summary.",
        )

    devices: list[tuple[int, str, str]] = []
    problems: list[str] = []
    for index in indices:
        device_type, name, error = _inspect_vulkan_json(index)
        if error:
            problems.append(error)
        else:
            if not name:
                problems.append(f"Cannot retrieve device name for Vulkan device {index}.")
            else:
                devices.append((index, device_type, name))

    candidates = [device for device in devices if device[1] in NON_CPU_GPU_TYPES]
    cpu_devices = [device for device in devices if device[1] == "VK_PHYSICAL_DEVICE_TYPE_CPU"]
    unknown_devices = [device for device in devices if device[1] not in NON_CPU_GPU_TYPES and device[1] != "VK_PHYSICAL_DEVICE_TYPE_CPU"]
    if candidates:
        descriptions = [f"GPU{index} {name} ({device_type})" for index, device_type, name in candidates]
        detail = "Non-CPU Vulkan device type observed in JSON: " + "; ".join(descriptions) + "."
        if cpu_devices:
            detail += " CPU/software Vulkan device(s) also present: " + "; ".join(
                f"GPU{index} {name}" for index, _, name in cpu_devices
            ) + "."
        if problems:
            detail += " Some enumerated device JSON could not be inspected: " + " ".join(problems)
        detail += " This is a device-type observation, not certification of hardware acceleration."
        return Check("Vulkan device", True, detail)

    if cpu_devices:
        detail = "Only CPU-type Vulkan device(s) were confirmed: " + "; ".join(
            f"GPU{index} {name}" for index, _, name in cpu_devices
        ) + "."
        detail += " A software Vulkan implementation (for example, llvmpipe) is not a non-CPU GPU candidate."
    elif unknown_devices:
        detail = "No recognized non-CPU GPU type was confirmed; reported device type(s): " + "; ".join(
            f"GPU{index} {name} ({device_type})" for index, device_type, name in unknown_devices
        ) + "."
    else:
        detail = "No Vulkan device JSON could be inspected."
    if problems:
        detail += " Inspection errors: " + " ".join(problems)
    return Check(
        "Vulkan device",
        False,
        detail,
        "Check all device records with vulkaninfo --summary, install/configure the intended non-CPU Vulkan ICD, and rerun this diagnostic.",
    )


def parse_layer_shell_version(output: str) -> int | None:
    """Return the advertised global version from a wayland-info listing."""
    line_pattern = re.compile(
        rf"^\s*interface:\s*['\"]{re.escape(LAYER_SHELL_INTERFACE)}['\"]\s*,"
        r"\s*version:\s*(\d+)\s*,\s*name:\s*\d+\s*$",
        re.MULTILINE,
    )
    match = line_pattern.search(output)
    return int(match.group(1)) if match else None


def check_layer_shell() -> Check:
    """Connect with wayland-info and require the wlr layer-shell global."""
    if not os.environ.get("XDG_RUNTIME_DIR"):
        return Check(
            "Wayland layer-shell",
            False,
            "XDG_RUNTIME_DIR is unset; no active Wayland session can be identified.",
            "Run this check from the GPUI user's active Wayland login session (with XDG_RUNTIME_DIR set).",
        )
    if not shutil.which("wayland-info"):
        return Check(
            "Wayland layer-shell",
            False,
            "wayland-info is not installed or not on PATH.",
            "Install wayland-utils, enter an active Wayland session, and rerun this diagnostic.",
        )

    command = ["wayland-info"]
    try:
        completed = _run(command)
    except (OSError, subprocess.TimeoutExpired) as error:
        return Check(
            "Wayland layer-shell",
            False,
            _command_error(command, error),
            "Check the Wayland socket and session environment (WAYLAND_DISPLAY and XDG_RUNTIME_DIR), then retry.",
        )
    if completed.returncode != 0:
        message = (completed.stderr or completed.stdout).strip().replace("\n", " ")
        return Check(
            "Wayland layer-shell",
            False,
            f"wayland-info could not query the compositor (exit {completed.returncode}): {message[:300] or 'no diagnostic output'}.",
            "Run inside the target user's active Wayland session and check WAYLAND_DISPLAY/XDG_RUNTIME_DIR and socket permissions.",
        )

    version = parse_layer_shell_version(completed.stdout)
    if version is None:
        return Check(
            "Wayland layer-shell",
            False,
            f"wayland-info connected, but did not report the {LAYER_SHELL_INTERFACE} global.",
            "Use a compositor configured with the wlr layer-shell protocol; a working Wayland connection alone is insufficient.",
        )
    return Check(
        "Wayland layer-shell",
        True,
        f"Compositor advertises {LAYER_SHELL_INTERFACE} version {version} in wayland-info output.",
    )


def main() -> int:
    checks = [check_drm_nodes(), check_vulkan(), check_layer_shell()]
    print("GPUI graphics readiness (prerequisite probe; not integration certification)")
    for check in checks:
        print(f"{'PASS' if check.passed else 'FAIL'}  {check.name}: {check.detail}")
        if not check.passed and check.action:
            print(f"      Action: {check.action}")
    failed = [check for check in checks if not check.passed]
    if failed:
        print(f"\nNot ready: {len(failed)} required check(s) failed.")
        return 1
    print("\nAll required prerequisite checks passed. This does not prove GPUI integration or hardware acceleration.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
