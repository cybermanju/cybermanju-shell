"""Unit tests for disk_budget; all filesystem measurements are mocked."""

import io
import os
import stat
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

import disk_budget


class DiskBudgetTests(unittest.TestCase):
    def test_init_saves_device_and_available_baseline(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            workspace = os.path.join(temp_dir, "workspace")
            docker_root = os.path.join(temp_dir, "docker")
            os.mkdir(workspace)
            os.mkdir(docker_root)
            state_file = os.path.join(temp_dir, "state.json")
            stat_result = SimpleNamespace(st_dev=77, st_mode=stat.S_IFDIR)
            with mock.patch.object(disk_budget.os, "stat", return_value=stat_result), mock.patch.object(
                disk_budget, "available_bytes", return_value=9_000
            ):
                state = disk_budget.create_state(
                    state_file, workspace, docker_root, limit_bytes=8_000, reserve_bytes=1_000
                )
            loaded = disk_budget.load_state(state_file)

        self.assertEqual(state["workspace_st_dev"], 77)
        self.assertEqual(state["docker_root_st_dev"], 77)
        self.assertEqual(loaded["workspace_baseline_available_bytes"], 9_000)
        self.assertEqual(loaded["docker_root_baseline_available_bytes"], 9_000)
        self.assertEqual(loaded["baseline_available_bytes"], 9_000)
        self.assertEqual(loaded["limit_bytes"], 8_000)
        self.assertEqual(loaded["reserve_bytes"], 1_000)

    def test_init_refuses_different_filesystems(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            workspace = os.path.join(temp_dir, "workspace")
            docker_root = os.path.join(temp_dir, "docker")
            os.mkdir(workspace)
            os.mkdir(docker_root)
            stat_results = [SimpleNamespace(st_dev=1), SimpleNamespace(st_dev=2)]
            with mock.patch.object(disk_budget.os, "stat", side_effect=stat_results), mock.patch.object(
                disk_budget, "available_bytes"
            ) as measure:
                with self.assertRaisesRegex(ValueError, "different filesystems"):
                    disk_budget.create_state(
                        os.path.join(temp_dir, "state.json"), workspace, docker_root
                    )
            measure.assert_not_called()

    def test_init_refuses_baseline_below_reserve(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            workspace = os.path.join(temp_dir, "workspace")
            docker_root = os.path.join(temp_dir, "docker")
            os.mkdir(workspace)
            os.mkdir(docker_root)
            with mock.patch.object(disk_budget.os, "stat", return_value=SimpleNamespace(st_dev=1)), mock.patch.object(
                disk_budget, "available_bytes", return_value=99
            ):
                with self.assertRaisesRegex(ValueError, "below the required reserve"):
                    disk_budget.create_state(
                        os.path.join(temp_dir, "state.json"), workspace, docker_root,
                        limit_bytes=200, reserve_bytes=100,
                    )

    def test_check_fails_when_allocated_delta_exceeds_limit_minus_reserve(self):
        state = {
            "workspace_path": "/workspace",
            "baseline_available_bytes": 1_000,
            "limit_bytes": 500,
            "reserve_bytes": 100,
        }
        output = io.StringIO()
        errors = io.StringIO()
        with mock.patch.object(disk_budget, "available_bytes", return_value=550), redirect_stdout(output), redirect_stderr(errors):
            result = disk_budget.check_state(state)

        self.assertEqual(result, 1)
        self.assertIn("allocated_delta_bytes=450", output.getvalue())
        self.assertIn("does NOT fit standard 14GB", errors.getvalue())

    def test_check_fails_when_physical_available_is_below_reserve(self):
        state = {
            "workspace_path": "/workspace",
            "baseline_available_bytes": 1_000,
            "limit_bytes": 500,
            "reserve_bytes": 100,
        }
        with mock.patch.object(disk_budget, "available_bytes", return_value=99), redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()) as errors:
            result = disk_budget.check_state(state)

        self.assertEqual(result, 1)
        self.assertIn("does NOT fit standard 14GB", errors.getvalue())

    def test_run_propagates_child_failure(self):
        state = {
            "workspace_path": "/workspace",
            "baseline_available_bytes": 1_000,
            "limit_bytes": 500,
            "reserve_bytes": 100,
        }
        child = mock.Mock()
        child.poll.return_value = 23
        child.wait.return_value = 23
        with mock.patch.object(disk_budget, "usage", return_value={
            "allocated_delta_bytes": 0,
            "available_bytes": 1_000,
            "baseline_available_bytes": 1_000,
            "budget_bytes": 400,
            "reserve_bytes": 100,
            "limit_bytes": 500,
            "used_budget_bytes": 0,
        }), mock.patch.object(disk_budget.subprocess, "Popen", return_value=child) as popen, redirect_stdout(io.StringIO()):
            result = disk_budget.run_command(state, ["false"])

        self.assertEqual(result, 23)
        popen.assert_called_once_with(["false"], start_new_session=True)

    def test_run_stops_process_group_on_budget_breach(self):
        state = {
            "workspace_path": "/workspace",
            "baseline_available_bytes": 1_000,
            "limit_bytes": 500,
            "reserve_bytes": 100,
        }
        snapshots = [
            {"allocated_delta_bytes": 0, "available_bytes": 1_000, "baseline_available_bytes": 1_000, "budget_bytes": 400, "reserve_bytes": 100, "limit_bytes": 500, "used_budget_bytes": 0},
            {"allocated_delta_bytes": 901, "available_bytes": 99, "baseline_available_bytes": 1_000, "budget_bytes": 400, "reserve_bytes": 100, "limit_bytes": 500, "used_budget_bytes": 901},
            {"allocated_delta_bytes": 901, "available_bytes": 99, "baseline_available_bytes": 1_000, "budget_bytes": 400, "reserve_bytes": 100, "limit_bytes": 500, "used_budget_bytes": 901},
        ]
        child = mock.Mock()
        child.pid = 1234
        child.poll.return_value = None
        child.wait.return_value = 0
        errors = io.StringIO()
        with mock.patch.object(disk_budget, "usage", side_effect=snapshots), mock.patch.object(
            disk_budget.subprocess, "Popen", return_value=child
        ), mock.patch.object(disk_budget.time, "sleep"), mock.patch.object(
            disk_budget, "_signal_process_group"
        ) as signal_group, mock.patch.object(disk_budget, "_kill_builder_container"), redirect_stdout(io.StringIO()), redirect_stderr(errors):
            result = disk_budget.run_command(state, ["long-running"])

        self.assertEqual(result, 1)
        signal_group.assert_called_once_with(child, disk_budget.signal.SIGTERM)
        self.assertIn("does NOT fit standard 14GB", errors.getvalue())

    def test_usage_reports_delta_and_budget_after_reserve(self):
        state = {
            "workspace_path": "/workspace",
            "baseline_available_bytes": 10_000,
            "limit_bytes": 8_000,
            "reserve_bytes": 1_000,
        }
        with mock.patch.object(disk_budget, "available_bytes", return_value=7_500):
            snapshot = disk_budget.usage(state)
        self.assertEqual(snapshot["allocated_delta_bytes"], 2_500)
        self.assertEqual(snapshot["budget_bytes"], 7_000)

    def test_initial_checkout_bytes_are_counted(self):
        with tempfile.TemporaryDirectory() as directory:
            with mock.patch.object(disk_budget, "available_bytes", return_value=9000):
                state = disk_budget.create_state(
                    os.path.join(directory, "state.json"), directory, directory,
                    limit_bytes=8000, reserve_bytes=1000, initial_bytes=500,
                )
                snapshot = disk_budget.usage(state)
            self.assertEqual(snapshot["allocated_delta_bytes"], 500)

    def test_filesystem_change_is_rejected(self):
        state = {"workspace_path": "/workspace", "workspace_st_dev": 1,
                 "docker_root": "/docker", "docker_root_st_dev": 1}
        with mock.patch.object(disk_budget.os, "stat", return_value=SimpleNamespace(st_dev=2)):
            with self.assertRaisesRegex(ValueError, "filesystem changed"):
                disk_budget.usage(state)


if __name__ == "__main__":
    unittest.main()
