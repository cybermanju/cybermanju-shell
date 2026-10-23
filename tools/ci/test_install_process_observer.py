"""Behavioral regression tests for the opt-in install process observer (Linux only)."""
import json
import importlib.util
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
OBSERVER = ROOT / "asterinas/distro/aster_nixos_installer/templates/install-process-observer.py"
INSTALLER = ROOT / "asterinas/distro/aster_nixos_installer/templates/aster-nixos-install"
PREFIX = "CYBERMANJU_INSTALL_OBSERVER:"
PHASE_PREFIX = "CYBERMANJU_INSTALL_PHASE:"


def records(text):
    result = []
    for line in text.splitlines():
        if line.startswith(PREFIX):
            result.append(json.loads(line[len(PREFIX):]))
    return result


def run_install_phase(phase, command, *args, env=None, stdin=None, timeout=8, errexit=False):
    """Run the actual shell function from the installer with real placeholder paths."""
    text = INSTALLER.read_text()
    match = re.search(r"(?ms)^run_install_phase\(\) \{.*?^\}", text)
    if not match:
        raise AssertionError("run_install_phase function was not found in installer template")
    function = match.group(0).replace("@aster-observer-python@", sys.executable)
    function = function.replace("@aster-install-observer@", str(OBSERVER.resolve()))
    script = function + ("\nset -e\n" if errexit else "\n") + 'run_install_phase "$@"\n'
    if errexit:
        script += "printf SHOULD_NOT_RUN\n"
    with tempfile.TemporaryDirectory(prefix="install-observer-test-") as td:
        envmap = os.environ.copy()
        envmap["TMPDIR"] = td
        envmap["CYBERMANJU_INSTALL_OBSERVER"] = "0"
        if env:
            envmap.update({str(k): str(v) for k, v in env.items()})
        result = subprocess.run(["/bin/bash", "-c", script, "observer-test", phase, command, *map(str, args)],
                                input=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                text=True, env=envmap, timeout=timeout)
        leftovers = list(Path(td).iterdir())
        return result, leftovers


@unittest.skipUnless(sys.platform.startswith("linux"), "Linux /proc required")
class InstallProcessObserverTests(unittest.TestCase):
    def invoke(self, *args, timeout=4):
        return subprocess.run([sys.executable, str(OBSERVER), *map(str, args)],
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              text=True, timeout=timeout)

    def test_cli_invalid_bounds(self):
        for args in (("--interval", "0.001"), ("--interval", "301"),
                     ("--samples", "0"), ("--samples", "241"),
                     ("--max-processes", "0"), ("--max-processes", "13")):
            with self.subTest(args=args):
                r = self.invoke("--parent", os.getpid(), "--stop-file", "/tmp/x",
                                "--phase", "install-system", *args)
                self.assertNotEqual(r.returncode, 0)

    def test_real_parent_and_child_are_sampled(self):
        with tempfile.TemporaryDirectory() as td:
            stop = Path(td) / "stop"
            child = subprocess.Popen(["/bin/sleep", "2"])
            try:
                p = subprocess.Popen([sys.executable, str(OBSERVER), "--parent", str(os.getpid()),
                                      "--stop-file", str(stop), "--phase", "install-system",
                                      "--interval", ".02", "--samples", "2"],
                                     stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
                err = p.communicate(timeout=2)[1]
                ev = records(err)
                self.assertEqual(p.returncode, 0)
                self.assertTrue(any(x.get("event") == "sample" for x in ev), ev)
                procs = [q for x in ev if x.get("event") == "sample" for q in x.get("processes", [])]
                self.assertTrue(any(q.get("pid") == os.getpid() for q in procs), procs)
                self.assertTrue(any(q.get("pid") == child.pid for q in procs), procs)
                self.assertTrue(all("state" in q and "wchan" in q for q in procs))
            finally:
                child.terminate()
                child.wait(timeout=2)

    def test_fake_proc_bounds_tricky_comm_and_no_environment_access(self):
        with tempfile.TemporaryDirectory() as td:
            proc = Path(td) / "proc"
            proc.mkdir()
            stop = Path(td) / "stop"

            def add(pid, ppid, comm, children="", argv=b"a\x00b\x00"):
                d = proc / str(pid)
                d.mkdir()
                # starttime is field 22: index 19 among the fields after comm.
                fields = ["S", str(ppid)] + ["0"] * 17 + ["100"]
                (d / "stat").write_text(f"{pid} ({comm}) " + " ".join(fields) + "\n")
                (d / "status").write_text(f"Name:\t{comm}\nState:\tS (sleeping)\n")
                (d / "cmdline").write_bytes(argv)
                (d / "io").write_text("read_bytes: 7\nwrite_bytes: 9\n")
                (d / "wchan").write_text("wait_event\n")
                (d / "task" / str(pid)).mkdir(parents=True)
                (d / "task" / str(pid) / "children").write_text(children)

            add(101, 1, "parent", "102", b"parent\x00--secret\x00")
            add(102, 101, "name with ) tricky", "103", b"x" * 800)
            add(103, 102, "deep", "101")  # cycle
            add(104, 101, "observer", "105")
            add(105, 104, "observer-child")
            (proc / "101" / "environ").write_text("DO_NOT_READ")
            r = self.invoke("--parent", 101, "--stop-file", stop, "--phase", "install-system",
                            "--interval", ".01", "--samples", "1", "--max-processes", "12",
                            "--proc-root", proc)
            self.assertEqual(r.returncode, 0, r.stderr)
            ev = records(r.stderr)
            self.assertTrue(ev)
            items = [q for e in ev for q in e.get("processes", [])]
            pids = [q["pid"] for q in items]
            self.assertEqual(len(pids), len(set(pids)))
            self.assertLessEqual(len(items), 12)
            self.assertTrue({101, 102, 103}.issubset(pids), items)
            self.assertNotIn(104, pids)
            self.assertNotIn(105, pids)
            longarg = next(q for q in items if q["pid"] == 102)
            self.assertTrue(longarg["argv_truncated"], longarg)
            self.assertEqual(len(longarg["argv"].encode()), 512)
            self.assertNotIn("DO_NOT_READ", r.stderr)

    def test_proc_depth_observer_subtree_identity_and_allowed_reads(self):
        spec = importlib.util.spec_from_file_location("install_process_observer_unit", OBSERVER)
        observer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(observer)
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            for pid in range(101, 122):
                base = proc / str(pid); base.mkdir()
                fields = ["S", str(pid - 1)] + ["0"] * 17 + ["100"]
                (base / "stat").write_text(f"{pid} (p ) tricky) " + " ".join(fields))
                (base / "cmdline").write_bytes(b"p\0--arg\0")
                (base / "wchan").write_text("do_wait")
                (base / "io").write_text("read_bytes: 7\nwrite_bytes: 9\n")
                children = base / "task" / str(pid); children.mkdir(parents=True)
                (children / "children").write_text(str(pid + 1) if pid < 121 else "101")
                (base / "environ").write_text("MUST_NOT_BE_READ")
            sample = observer.collect_sample(101, str(proc), observer_pid=999999)
            self.assertEqual([p["pid"] for p in sample], list(range(101, 110)))
            sample = observer.collect_sample(101, str(proc), observer_pid=105)
            self.assertEqual([p["pid"] for p in sample], list(range(101, 105)))
            (proc / "101/task/101/children").write_text(" ".join(map(str, range(102, 122))))
            self.assertEqual(len(observer.collect_sample(101, str(proc), observer_pid=999999)), 12)
            reads = []
            original = observer._read_bounded
            def recorded(path, limit=32768):
                reads.append(Path(path).name)
                return original(path, limit)
            with mock.patch.object(observer, "_read_bounded", side_effect=recorded):
                value = observer.read_process(101, str(proc))
            self.assertTrue(reads)
            self.assertTrue(set(reads) <= {"stat", "cmdline", "wchan", "io"}, reads)
            self.assertEqual(value["starttime"], 100)
            with mock.patch.object(observer, "_parent_start", return_value=101):
                changed = observer.read_process(101, str(proc))
            self.assertEqual(changed["unavailable"], ["identity_changed"])
            self.assertIsNone(changed["argv"])
            self.assertIsNone(changed["io"])

    def test_default_off_preserves_stdio_and_argv_and_emits_only_phase_lines(self):
        r, td = run_install_phase("install-system", "/bin/bash", "-c",
                                  'printf "OUT:%s\\n" "$1"; read -r line; printf "IN:%s\\n" "$line"',
                                  "cmd-name", "arg value", stdin="pipe-value\n")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout, "OUT:arg value\nIN:pipe-value\n")
        self.assertEqual([line for line in r.stderr.splitlines() if line], [
            f"{PHASE_PREFIX}begin phase=install-system",
            f"{PHASE_PREFIX}end phase=install-system status=0"])
        self.assertEqual(td, [])

    def test_optin_observes_shell_and_command_preserves_status_and_stdio(self):
        # The first output line records the outer Bash PID from inside the command.
        payload = 'printf "PARENT:%s CHILD:%s ARGV:%s\\n" "$PPID" "$$" "$1"; read -r x; printf "READ:%s\\n" "$x"; sleep .15; exit 37'
        # bash -c treats its first following argument as $0.
        r, td = run_install_phase("install-system", "/bin/bash", "-c", payload, "cmd-name", "payload-arg",
                                  stdin="stdin-payload\n", env={"CYBERMANJU_INSTALL_OBSERVER": "1",
                                  "CYBERMANJU_INSTALL_OBSERVER_INTERVAL": ".01",
                                  "CYBERMANJU_INSTALL_OBSERVER_SAMPLES": "240"})
        self.assertEqual(r.returncode, 37, r.stderr)
        parentline = next(line for line in r.stdout.splitlines() if line.startswith("PARENT:"))
        parentpid = int(parentline.split()[0].split(":")[1])
        childpid = int(parentline.split()[1].split(":")[1])
        self.assertIn("ARGV:payload-arg", parentline)
        self.assertIn("READ:stdin-payload", r.stdout)
        phase_lines = [line for line in r.stderr.splitlines() if line.startswith(PHASE_PREFIX)]
        self.assertEqual(phase_lines[0], f"{PHASE_PREFIX}begin phase=install-system")
        self.assertEqual(phase_lines[-1], f"{PHASE_PREFIX}end phase=install-system status=37")
        ev = records(r.stderr)
        samples = [e for e in ev if e.get("event") == "sample"]
        self.assertTrue(samples, r.stderr)
        self.assertTrue(all(e.get("parent_pid") == parentpid for e in samples), samples)
        procs = [p for e in samples for p in e.get("processes", [])]
        self.assertTrue(any(p.get("pid") == parentpid for p in procs), procs)
        self.assertTrue(any(p.get("pid") == childpid for p in procs), procs)
        self.assertFalse(any("install-process-observer.py" in (p.get("argv") or "") for p in procs), procs)
        self.assertEqual(td, [])

    def test_optin_failure_preserves_errexit_before_next_command(self):
        result, leftovers = run_install_phase("install-system", "/bin/bash", "-c", "sleep .1; exit 37",
            env={"CYBERMANJU_INSTALL_OBSERVER": "1", "CYBERMANJU_INSTALL_OBSERVER_INTERVAL": ".01"}, errexit=True)
        self.assertEqual(result.returncode, 37, result.stderr)
        self.assertNotIn("SHOULD_NOT_RUN", result.stdout)
        self.assertIn("end phase=install-system status=37", result.stderr)
        self.assertEqual(leftovers, [])

    def test_build_phase_does_not_start_observer(self):
        r, td = run_install_phase("build-system", "/bin/true", env={"CYBERMANJU_INSTALL_OBSERVER": "1",
                                                                      "CYBERMANJU_INSTALL_OBSERVER_INTERVAL": ".01"})
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(records(r.stderr), [])
        self.assertEqual([line for line in r.stderr.splitlines() if line], [f"{PHASE_PREFIX}begin phase=build-system",
                                                 f"{PHASE_PREFIX}end phase=build-system status=0"])
        self.assertEqual(td, [])

    def test_invalid_observer_interval_preserves_target_status(self):
        r, td = run_install_phase("install-system", "/bin/bash", "-c", "exit 37",
                                  env={"CYBERMANJU_INSTALL_OBSERVER": "1",
                                       "CYBERMANJU_INSTALL_OBSERVER_INTERVAL": "0"})
        self.assertEqual(r.returncode, 37, r.stderr)
        self.assertIn("invalid bounded observer arguments", r.stderr)
        self.assertEqual(r.stderr.splitlines()[-1], f"{PHASE_PREFIX}end phase=install-system status=37")
        self.assertEqual(td, [])

    def test_target_sigterm_status_is_preserved_by_wrapper(self):
        for enabled in ("0", "1"):
            with self.subTest(observer=enabled):
                r, td = run_install_phase("install-system", "/bin/bash", "-c", "sleep .1; kill -TERM $$",
                                          env={"CYBERMANJU_INSTALL_OBSERVER": enabled,
                                               "CYBERMANJU_INSTALL_OBSERVER_INTERVAL": ".01"})
                self.assertEqual(r.returncode, 128 + signal.SIGTERM, r.stderr)
                self.assertEqual(r.stderr.splitlines()[-1], f"{PHASE_PREFIX}end phase=install-system status=143")
                self.assertEqual(td, [])

    def test_sample_limit_does_not_terminate_target(self):
        started = time.monotonic()
        r, td = run_install_phase("install-system", "/bin/sleep", ".25",
                                  env={"CYBERMANJU_INSTALL_OBSERVER": "1",
                                       "CYBERMANJU_INSTALL_OBSERVER_INTERVAL": ".01",
                                       "CYBERMANJU_INSTALL_OBSERVER_SAMPLES": "1"}, timeout=4)
        elapsed = time.monotonic() - started
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertGreaterEqual(elapsed, .2, "target did not finish naturally")
        self.assertTrue(any(e.get("event") == "sample" for e in records(r.stderr)), r.stderr)
        self.assertEqual(td, [])

    def test_observer_stops_and_reaps_promptly_after_target_finishes(self):
        started = time.monotonic()
        r, td = run_install_phase("install-system", "/bin/true",
                                  env={"CYBERMANJU_INSTALL_OBSERVER": "1",
                                       "CYBERMANJU_INSTALL_OBSERVER_INTERVAL": ".01",
                                       "CYBERMANJU_INSTALL_OBSERVER_SAMPLES": "240"}, timeout=3)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertLess(time.monotonic() - started, 2)
        self.assertEqual(td, [])

    def test_missing_proc_files_are_nonfatal(self):
        with tempfile.TemporaryDirectory() as td:
            proc = Path(td) / "proc"
            proc.mkdir()
            r = self.invoke("--parent", 999999, "--stop-file", Path(td)/"stop", "--phase",
                            "install-system", "--samples", "1", "--proc-root", proc)
            self.assertEqual(r.returncode, 0, r.stderr)


if __name__ == "__main__":
    unittest.main()
