"""Unit tests use temporary test-only fixtures; no host devices are fabricated."""

from __future__ import annotations

import json
import os
import stat
import subprocess
import tempfile
import unittest
from types import SimpleNamespace
from pathlib import Path
from unittest import mock

import check_graphics


def vulkan_profiles_fixture(device_type: str, device_name: str) -> dict[str, object]:
    """Build the minimal shape found in vulkaninfo's Vulkan Profiles JSON."""
    return {
        "$schema": "https://schema.khronos.org/vulkan/profiles-0.8-latest.json",
        "capabilities": {
            "device": {
                "properties": {
                    "VkPhysicalDeviceProperties": {
                        "deviceType": device_type,
                        "deviceName": device_name,
                    }
                }
            }
        },
        "profiles": {},
    }


class GraphicsDiagnosticTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory(prefix="graphics-test-")
        self.addCleanup(self.temporary_directory.cleanup)
        self.fixture_directory = Path(self.temporary_directory.name)
        # These are explicit fixture files, consumed only by mocked command runners.
        self.cpu_json_fixture = self.fixture_directory / "cpu-vulkan-profiles-fixture.json"
        self.cpu_json_fixture.write_text(
            json.dumps(vulkan_profiles_fixture("VK_PHYSICAL_DEVICE_TYPE_CPU", "fixture llvmpipe")),
            encoding="utf-8",
        )
        self.gpu_json_fixture = self.fixture_directory / "gpu-vulkan-profiles-fixture.json"
        self.gpu_json_fixture.write_text(
            json.dumps(vulkan_profiles_fixture("VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU", "fixture GPU")),
            encoding="utf-8",
        )
        self.wayland_fixture = self.fixture_directory / "wayland-info-output-fixture.txt"
        self.wayland_fixture.write_text(
            "interface: 'wl_compositor', version: 6, name: 1\n"
            "interface: 'zwlr_layer_shell_v1', version: 4, name: 23\n",
            encoding="utf-8",
        )

    def _vulkan_runner(self, indices: str, fixture_for_index: dict[int, Path]):
        def run(command: list[str], *, cwd: str | None = None):
            if command == ["vulkaninfo", "--summary"]:
                summary = "Devices:\n========\n" + "".join(
                    f"GPU{index}:\napiVersion = 1.3\n" for index in map(int, indices.split(","))
                )
                return subprocess.CompletedProcess(command, 0, summary, "")
            index = int(command[1].split("=", 1)[1])
            source = fixture_for_index[index]
            (Path(cwd) / f"vulkan-device-{index}-fixture.json").write_text(
                source.read_text(encoding="utf-8"), encoding="utf-8"
            )
            return subprocess.CompletedProcess(command, 0, "", "")

        return run

    def test_vulkan_inspects_each_indexed_json_device(self) -> None:
        with mock.patch.object(
            check_graphics,
            "_run",
            side_effect=self._vulkan_runner("0,1", {0: self.cpu_json_fixture, 1: self.gpu_json_fixture}),
        ) as runner:
            result = check_graphics.check_vulkan()
        self.assertTrue(result.passed, result.detail)
        self.assertIn("fixture GPU", result.detail)
        self.assertIn("fixture llvmpipe", result.detail)
        self.assertEqual(runner.call_count, 3)

    def test_cpu_only_vulkan_does_not_pass(self) -> None:
        with mock.patch.object(
            check_graphics,
            "_run",
            side_effect=self._vulkan_runner("0", {0: self.cpu_json_fixture}),
        ):
            result = check_graphics.check_vulkan()
        self.assertFalse(result.passed)
        self.assertIn("Only CPU-type", result.detail)
        self.assertIn("llvmpipe", result.detail)

    def test_unrecognized_device_type_does_not_pass(self) -> None:
        other_fixture = self.fixture_directory / "other-vulkan-profiles-fixture.json"
        other_fixture.write_text(
            json.dumps(vulkan_profiles_fixture("VK_PHYSICAL_DEVICE_TYPE_OTHER", "fixture unknown")),
            encoding="utf-8",
        )
        with mock.patch.object(
            check_graphics,
            "_run",
            side_effect=self._vulkan_runner("0", {0: other_fixture}),
        ):
            result = check_graphics.check_vulkan()
        self.assertFalse(result.passed)
        self.assertIn("No recognized non-CPU GPU type", result.detail)

    def test_vulkan_missing_command_fails_actionably(self) -> None:
        with mock.patch.object(check_graphics, "_run", side_effect=FileNotFoundError()):
            result = check_graphics.check_vulkan()
        self.assertFalse(result.passed)
        self.assertIn("vulkaninfo is not installed", result.detail)
        self.assertIn("Install", result.action)

    def test_drm_requires_accessible_character_node_fixtures_of_each_kind(self) -> None:
        drm_fixture_directory = self.fixture_directory / "dri-fixture"
        drm_fixture_directory.mkdir()
        primary_fixture = drm_fixture_directory / "card3"
        render_fixture = drm_fixture_directory / "renderD129"
        ignored_fixture = drm_fixture_directory / "controlD64"
        for fixture in (primary_fixture, render_fixture, ignored_fixture):
            fixture.touch()

        original_stat = Path.stat

        def fixture_stat(path: Path, *args, **kwargs):
            result = original_stat(path, *args, **kwargs)
            if path in (primary_fixture, render_fixture):
                return SimpleNamespace(st_mode=stat.S_IFCHR | 0o660)
            return result

        with (
            mock.patch.object(check_graphics, "DRM_DIRECTORY", drm_fixture_directory),
            mock.patch.object(Path, "stat", autospec=True, side_effect=fixture_stat),
            mock.patch.object(check_graphics.os, "access", return_value=True),
        ):
            result = check_graphics.check_drm_nodes()
        self.assertTrue(result.passed, result.detail)
        self.assertIn("card3", result.detail)
        self.assertIn("renderD129", result.detail)
        self.assertNotIn("controlD64", result.detail)

    def test_drm_regular_file_is_not_a_device_node(self) -> None:
        drm_fixture_directory = self.fixture_directory / "dri-regular-fixture"
        drm_fixture_directory.mkdir()
        (drm_fixture_directory / "card0").touch()
        (drm_fixture_directory / "renderD128").touch()
        with mock.patch.object(check_graphics, "DRM_DIRECTORY", drm_fixture_directory):
            result = check_graphics.check_drm_nodes()
        self.assertFalse(result.passed)
        self.assertIn("No accessible DRM character-device nodes", result.detail)

    def test_wayland_global_is_parsed_from_mocked_real_format_output(self) -> None:
        output = self.wayland_fixture.read_text(encoding="utf-8")
        with (
            mock.patch.dict(os.environ, {"XDG_RUNTIME_DIR": str(self.fixture_directory), "WAYLAND_DISPLAY": "fixture-wayland"}),
            mock.patch.object(check_graphics.shutil, "which", return_value="/usr/bin/wayland-info"),
            mock.patch.object(
                check_graphics,
                "_run",
                return_value=subprocess.CompletedProcess(["wayland-info"], 0, output, ""),
            ) as runner,
        ):
            result = check_graphics.check_layer_shell()
        self.assertTrue(result.passed, result.detail)
        self.assertIn("version 4", result.detail)
        runner.assert_called_once_with(["wayland-info"])

    def test_wayland_connection_without_layer_shell_fails(self) -> None:
        with (
            mock.patch.dict(os.environ, {"XDG_RUNTIME_DIR": str(self.fixture_directory)}),
            mock.patch.object(check_graphics.shutil, "which", return_value="/usr/bin/wayland-info"),
            mock.patch.object(
                check_graphics,
                "_run",
                return_value=subprocess.CompletedProcess(
                    ["wayland-info"], 0, "interface: 'wl_compositor', version: 6, name: 1\n", ""
                ),
            ),
        ):
            result = check_graphics.check_layer_shell()
        self.assertFalse(result.passed)
        self.assertIn("did not report the zwlr_layer_shell_v1", result.detail)

    def test_main_returns_nonzero_when_a_required_check_fails(self) -> None:
        passed = check_graphics.Check("ok", True, "fixture")
        failed = check_graphics.Check("required", False, "fixture failure", "fixture action")
        with (
            mock.patch.object(check_graphics, "check_drm_nodes", return_value=passed),
            mock.patch.object(check_graphics, "check_vulkan", return_value=failed),
            mock.patch.object(check_graphics, "check_layer_shell", return_value=passed),
        ):
            self.assertEqual(check_graphics.main(), 1)


if __name__ == "__main__":
    unittest.main()
