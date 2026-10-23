"""Host console fixtures only: observations are never installation success."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from iso_vm import (Console, DIAGNOSTIC_LINE_LIMIT, OBSERVER_PREFIX,
                    parse_installer_process_sample, run)
from test_installer_snapshot import empty_console, BEGIN, END, SUCCESS
from vm_settings import TIMEOUTS


def fixture():
    process = dict(pid=1234, ppid=1, comm="installer", state="S", utime_ticks=2,
                   stime_ticks=3, starttime=100, argv="nixos-install --root /mnt/test",
                   argv_truncated=False, wchan="do_wait", io={"read_bytes": 7}, unavailable=[])
    return dict(version=1, event="sample", phase="install-system", parent_pid=1234,
                sample=1, elapsed_seconds=0.0, processes=[process])


def encode(sample):
    return OBSERVER_PREFIX + json.dumps(sample, ensure_ascii=True).encode() + b"\r\n"


class InstallerProcessSnapshotTests(unittest.TestCase):
    def test_exact_prefix_and_bounded_schema(self):
        good = encode(fixture()).rstrip(b"\r\n")
        self.assertEqual(parse_installer_process_sample(good), fixture())
        for bad in (b"echo " + good, b" " + good, good + b" extra", b"copying path '/nix/store/install-process-observer.py'", OBSERVER_PREFIX + b"{}", OBSERVER_PREFIX + b"[]", OBSERVER_PREFIX + b"{" + b"x" * DIAGNOSTIC_LINE_LIMIT, OBSERVER_PREFIX + b"[" * 2000 + b"]" * 2000):
            with self.subTest():
                self.assertIsNone(parse_installer_process_sample(bad))

    def test_rejects_invalid_observation_types_and_claims(self):
        base = fixture()
        mutations = [("version", True), ("phase", "build-system"), ("event", "end"),
                     ("parent_pid", 999), ("parent_pid", True), ("sample", 0),
                     ("sample", 241), ("elapsed_seconds", float("nan")),
                     ("elapsed_seconds", -1), ("processes", []),
                     ("processes", base["processes"] * 13), ("installation_verified", True)]
        for key, value in mutations:
            bad = copy.deepcopy(base); bad[key] = value
            with self.subTest(key=key):
                self.assertIsNone(parse_installer_process_sample(encode(bad).strip()))
        for field, value in (("pid", False), ("argv", "x" * 4097), ("state", "not a state"),
                             ("argv_truncated", 1), ("utime_ticks", -1),
                             ("io", {"secret": "not allowed"}), ("unavailable", ["fake"]),
                             ("installed_boot_verified", True)):
            bad = fixture(); bad["processes"][0][field] = value
            with self.subTest(field=field):
                self.assertIsNone(parse_installer_process_sample(encode(bad).strip()))

    def test_every_chunk_boundary_preserves_explicit_sample(self):
        data = b"\x1b[32m" + encode(fixture()) + b"\x1b[0m"
        for boundary in range(len(data) + 1):
            console = empty_console()
            console.append_output(data[:boundary]); console.append_output(data[boundary:])
            self.assertEqual(console.last_installer_process_sample, fixture())
            self.assertEqual(console.installer_process_samples_seen, 1)
        console = empty_console()
        console.append_output(b"\x1b]title;" + encode(fixture()) + b"\x07")
        self.assertIsNone(getattr(console, "last_installer_process_sample", None))

    def test_sample_survives_eviction_and_heartbeat_does_not_fake_other_output(self):
        console = empty_console()
        with mock.patch("iso_vm.time.monotonic", return_value=10):
            console.append_output(BEGIN)
        with mock.patch("iso_vm.time.monotonic", return_value=20):
            console.append_output(encode(fixture()))
        with tempfile.TemporaryDirectory() as directory, mock.patch("iso_vm.time.monotonic", return_value=30):
            target = Path(directory) / "snapshot.json"
            console.write_failure_snapshot(target, TimeoutError("host fixture"))
            evidence = json.loads(target.read_text())
        self.assertEqual(evidence["seconds_since_output"], 10)
        self.assertEqual(evidence["seconds_since_non_observer_line"], 20)
        self.assertEqual(evidence["last_observed_installer_process_sample"], fixture())
        self.assertEqual(evidence["installer_process_samples_seen"], 1)
        self.assertFalse(evidence["process_observations_are_installation_success"])
        self.assertFalse(evidence["installation_verified"])
        self.assertFalse(evidence["installed_boot_verified"])
        console.append_output(b"x" * (1024 * 1024 + 64) + b"\n")
        self.assertEqual(console.last_installer_process_sample, fixture())

    def test_clipped_line_cannot_manufacture_an_anchored_marker(self):
        sample = fixture()
        data = json.dumps(sample).encode()
        padded = OBSERVER_PREFIX + b" " * (DIAGNOSTIC_LINE_LIMIT - len(OBSERVER_PREFIX) - len(data)) + data
        console = empty_console()
        console.append_output(b"x" + padded)
        console.append_output(b"\n")
        self.assertIsNone(getattr(console, "last_installer_process_sample", None))

    def test_no_sample_invented_from_store_paths_or_missing_observer(self):
        console = empty_console()
        console.append_output(b"copying /nix/store/install-process-observer.py and setup-etc.pl\n")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "snapshot.json"
            console.write_failure_snapshot(target, TimeoutError("fixture"))
            evidence = json.loads(target.read_text())
        self.assertIsNone(evidence["last_observed_installer_process_sample"])
        self.assertEqual(evidence["installer_process_samples_seen"], 0)

    def test_sample_does_not_replace_success_or_launder_phase_failure(self):
        console = empty_console(); console.append_output(encode(fixture()))
        with self.assertRaises(TimeoutError):
            console.expect(SUCCESS, 0, reject_installer_failure=True)
        console.append_output(END + encode(fixture()) + b"Congratulations! Asterinas NixOS has been installed successfully!\n")
        with self.assertRaisesRegex(RuntimeError, "exit 37"):
            console.expect(SUCCESS, 0, reject_installer_failure=True)
        self.assertEqual(TIMEOUTS["ISO_VM_INSTALL_SECONDS"], 3600)


if __name__ == "__main__":
    unittest.main()
