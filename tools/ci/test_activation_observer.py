#!/usr/bin/env python3
"""Host-only tests for activation observation; not evidence of Asterinas guest behavior."""
from __future__ import annotations

import os
import ast
from pathlib import Path
import re
import shlex
import signal
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
OBSERVER = ROOT / "asterinas/distro/etc_nixos/modules/activation-observer.sh"
PATCHER = ROOT / "asterinas/distro/etc_nixos/modules/trace-activation-observer.py"
SHELL = Path("/bin/bash")
SLEEP = Path("/bin/sleep")
START = re.compile(r"^CYBERMANJU_ACTIVATION_PROBE:start pid=(\d+)", re.M)
END = re.compile(r"^CYBERMANJU_ACTIVATION_PROBE:end pid=(\d+) status=(\d+)", re.M)
SAMPLE = re.compile(r"^CYBERMANJU_ACTIVATION_PROBE:sample pid=(\d+) ", re.M)


def observer_cmd(sleep: Path, *args: str) -> list[str]:
    return ["bash", str(OBSERVER), str(sleep), "--", *args]


def proc_state(pid: int) -> str | None:
    """Return Linux process state, treating an absent proc entry as gone."""
    try:
        stat = Path(f"/proc/{pid}/stat").read_text()
        return stat[stat.rfind(")") + 2:].split()[0]
    except (OSError, IndexError):
        return None


class ObserverTests(unittest.TestCase):
    def test_stdio_arguments_and_exit_code(self):
        with tempfile.TemporaryDirectory() as td:
            script = Path(td) / "argv.py"
            script.write_text(
                "import sys\n"
                "data=sys.stdin.buffer.read()\n"
                "print(repr(sys.argv[1:]))\n"
                "print('out:'+data.decode(), end='')\n"
                "print('target-stderr', file=sys.stderr)\n"
                "sys.exit(37)\n"
            )
            cmd = observer_cmd(SLEEP, "python3", str(script), "with space", "", "a'b\"c")
            result = subprocess.run(cmd, input="pipe-input\n", capture_output=True, text=True, timeout=3)
            self.assertEqual(result.returncode, 37, result.stderr)
            self.assertEqual(ast.literal_eval(result.stdout.splitlines()[0]), ["with space", "", "a'b\"c"])
            self.assertIn("out:pipe-input\n", result.stdout)
            self.assertIn("target-stderr", result.stderr)
            self.assertRegex(result.stderr, START)
            self.assertRegex(result.stderr, END)
            self.assertLessEqual(len(SAMPLE.findall(result.stderr)), 1)

    def test_short_child_stops_real_five_second_monitor_promptly(self):
        start = time.monotonic()
        result = subprocess.run(observer_cmd(SLEEP, "/bin/true"), capture_output=True, timeout=2)
        elapsed = time.monotonic() - start
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertLess(elapsed, 2)
        self.assertRegex(result.stderr.decode(), END)

    def test_sample_limit_does_not_kill_live_child(self):
        with tempfile.TemporaryDirectory() as td:
            fast_sleep = Path(td) / "fast-sleep"
            fast_sleep.write_text("#!/bin/sh\n/bin/sleep 0.01\n")
            fast_sleep.chmod(0o755)
            start = time.monotonic()
            result = subprocess.run(observer_cmd(fast_sleep, "/bin/sleep", "0.70"),
                                    capture_output=True, timeout=3)
            elapsed = time.monotonic() - start
            text = result.stderr.decode()
            self.assertEqual(result.returncode, 0, text)
            self.assertEqual(len(SAMPLE.findall(text)), 12, text)
            self.assertEqual(text.count("CYBERMANJU_ACTIVATION_PROBE:limit "), 1, text)
            self.assertTrue(any("stat=unavailable" not in line for line in text.splitlines()
                                if line.startswith("CYBERMANJU_ACTIVATION_PROBE:sample ")), text)
            self.assertRegex(text, END)
            self.assertGreaterEqual(elapsed, 0.60, "target appears to have been killed at sample limit")

    def test_missing_target_propagates_127_and_disabled_sleep_execs_directly(self):
        missing = subprocess.run(observer_cmd(SLEEP, "/no/such/activation-target"), capture_output=True, timeout=2)
        self.assertEqual(missing.returncode, 127)
        self.assertRegex(missing.stderr.decode(), END)
        absent_sleep = subprocess.run(observer_cmd(Path("/not/a/real/sleep"), "/bin/sh", "-c", "exit 37"),
                                      capture_output=True, timeout=2)
        self.assertEqual(absent_sleep.returncode, 37)
        self.assertIn(b"disabled", absent_sleep.stderr)
        self.assertNotRegex(absent_sleep.stderr.decode(), START)

    def test_term_forwarding_cleans_wrapper_monitor_and_child(self):
        with tempfile.TemporaryDirectory() as td:
            ready = Path(td) / "ready"
            target = Path(td) / "target.sh"
            target.write_text("#!/bin/bash\ntrap 'exit 0' TERM\necho $BASHPID > \"$1\"\nwhile :; do /bin/sleep 0.05; done\n")
            target.chmod(0o755)
            proc = subprocess.Popen(observer_cmd(SLEEP, str(target), str(ready)),
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            try:
                deadline = time.monotonic() + 2
                while time.monotonic() < deadline and not ready.exists() and proc.poll() is None:
                    time.sleep(0.01)
                self.assertTrue(ready.exists(), "target did not start")
                child_pid = int(ready.read_text().strip())
                proc.send_signal(signal.SIGTERM)
                _, stderr = proc.communicate(timeout=3)
                self.assertEqual(proc.returncode, 0, stderr)  # Target TERM handler intentionally exits 0.
                self.assertTrue(END.search(stderr), stderr)
                deadline = time.monotonic() + 1
                while time.monotonic() < deadline and proc_state(child_pid) not in (None, "Z"):
                    time.sleep(0.02)
                self.assertIn(proc_state(child_pid), (None, "Z"), "forwarded child remains running")
            finally:
                if proc.poll() is None:
                    proc.kill()
                    proc.communicate(timeout=2)
                if ready.exists():
                    try:
                        pid = int(ready.read_text().strip())
                        if proc_state(pid) not in (None, "Z"):
                            os.kill(pid, signal.SIGKILL)
                    except (ValueError, ProcessLookupError, PermissionError):
                        pass


    def test_default_term_preserves_143_and_reaps_owned_jobs(self):
        proc = subprocess.Popen(observer_cmd(SLEEP, "/bin/sleep", "30"),
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        captured = set()
        try:
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline:
                children = Path(f"/proc/{proc.pid}/task/{proc.pid}/children")
                try:
                    direct = [int(x) for x in children.read_text().split()]
                    captured.update(direct)
                    for pid in direct:
                        try:
                            captured.update(int(x) for x in Path(f"/proc/{pid}/task/{pid}/children").read_text().split())
                        except OSError:
                            pass
                    if len(captured) >= 3:
                        break
                except OSError:
                    pass
                time.sleep(0.01)
            self.assertGreaterEqual(len(captured), 3, "command, monitor and monitor sleep were not observed")
            proc.send_signal(signal.SIGTERM)
            _, text = proc.communicate(timeout=3)
            self.assertEqual(proc.returncode, 143, text)
            self.assertIn("status=143", text)
            for pid in captured:
                self.assertIn(proc_state(pid), (None, "Z"), f"owned process {pid} survived")
        finally:
            if proc.poll() is None:
                proc.kill(); proc.communicate(timeout=2)
            for pid in captured:
                if proc_state(pid) not in (None, "Z"):
                    try: os.kill(pid, signal.SIGKILL)
                    except ProcessLookupError: pass


    def test_term_in_spawn_assignment_gaps_does_not_leave_owned_jobs(self):
        source=OBSERVER.read_text()
        for old in ["\nchild=$!\n", "\nmonitor=$!\n"]:
            with self.subTest(gap=old), tempfile.TemporaryDirectory() as td:
                root=Path(td); ready=root/"spawned"; copy=root/"observer.sh"
                assert source.count(old)==1
                injected="\nprintf ready > "+shlex.quote(str(ready))+"\n/bin/sleep 0.2"+old
                copy.write_text(source.replace(old,injected,1))
                proc=subprocess.Popen(["bash",str(copy),str(SLEEP),"--","/bin/sleep","30"],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
                owned=set()
                try:
                    deadline=time.monotonic()+2
                    while time.monotonic()<deadline and not ready.exists(): time.sleep(0.005)
                    self.assertTrue(ready.exists())
                    owned.update(int(x) for x in Path(f"/proc/{proc.pid}/task/{proc.pid}/children").read_text().split())
                    proc.send_signal(signal.SIGTERM)
                    _, text=proc.communicate(timeout=3)
                    self.assertEqual(proc.returncode,143,text)
                    for pid in owned: self.assertIn(proc_state(pid),(None,"Z"))
                finally:
                    if proc.poll() is None: proc.kill(); proc.communicate(timeout=2)
                    for pid in owned:
                        if proc_state(pid) not in (None,"Z"):
                            try: os.kill(pid,signal.SIGKILL)
                            except ProcessLookupError: pass


class PatcherTests(unittest.TestCase):
    def setup_fixture(self, root: Path, source: str):
        """Create literal, executable dependencies and a temporary source file."""
        bindir = root / "bin tools"
        bindir.mkdir()
        perl = bindir / "perl"
        perl.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\"\n")
        perl.chmod(0o755)
        observe = root / "observer with space.sh"
        observe.write_text("#!/bin/bash\nshift 2\nexec \"$@\"\n")
        shell = root / "shell with space"
        shell.write_text("#!/bin/bash\nexec /bin/bash \"$@\"\n")
        shell.chmod(0o755)
        sleep = root / "sleep with space"
        sleep.write_text("#!/bin/sh\nexit 0\n")
        sleep.chmod(0o755)
        src = root / "abc-activate"
        src.write_text(source)
        return src, perl, observe, shell, sleep

    def patch(self, source: Path, observer: Path, shell: Path, sleep: Path):
        return subprocess.run(["python3", str(PATCHER), str(source), str(observer), str(shell), str(sleep)],
                              cwd=ROOT, capture_output=True, text=True, timeout=3)

    def test_single_literal_command_is_wrapped_and_shell_valid(self):
        with tempfile.TemporaryDirectory(prefix="patch paths ") as td:
            root = Path(td)
            perl = root / "perl path/bin"
            perl.mkdir(parents=True)
            perl = perl / "perl"
            perl.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\"\n")
            perl.chmod(0o755)
            observe = root / "observer path/observer's.sh"
            observe.parent.mkdir()
            observe.write_text('#!/bin/bash\nshift 2\nexec "$@"\n')
            shell = root / 'shell "quoted"'
            shell.write_text("#!/bin/bash\nexec /bin/bash \"$@\"\n")
            shell.chmod(0o755)
            sleep = root / "sleep path"
            sleep.write_text("#!/bin/sh\nexit 0\n")
            sleep.chmod(0o755)
            etc = root / "abc-etc/etc"
            etc.mkdir(parents=True)
            setup = root / "abc-setup-etc.pl"
            src = root / "activation"
            src.write_text("#!/bin/bash\n\"" + str(perl) + "\" '" + str(setup) + "' \"" + str(etc) + "\"\n")
            result = self.patch(src, observe, shell, sleep)
            self.assertEqual(result.returncode, 0, result.stderr)
            patched = src.read_text()
            self.assertIn("__cybermanju_activation_observe() {", patched)
            self.assertIn("__cybermanju_activation_observe \"" + str(perl) + "\" '" + str(setup) + "' \"" + str(etc) + "\"", patched)
            self.assertEqual(subprocess.run(["bash", "-n", str(src)], capture_output=True).returncode, 0)
            run = subprocess.run(["bash", str(src)], capture_output=True, text=True, timeout=3)
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertEqual(run.stdout.splitlines(), [str(setup), str(etc)])

    def test_invalid_candidate_forms_leave_original_bytes_unchanged(self):
        cases = {
            "zero": "#!/bin/bash\necho nothing\n",
            "multiple": "#!/bin/bash\n/bin/perl /tmp/a-setup-etc.pl /tmp/abc-etc/etc\n/bin/perl /tmp/b-setup-etc.pl /tmp/abc-etc/etc\n",
            "pipeline": "#!/bin/bash\n/bin/perl /tmp/a-setup-etc.pl /tmp/abc-etc/etc | cat\n",
            "env-prefix": "#!/bin/bash\nenv /bin/perl /tmp/a-setup-etc.pl /tmp/abc-etc/etc\n",
            "substitution": "#!/bin/bash\n/bin/perl \"$(echo /tmp/a-setup-etc.pl)\" /tmp/abc-etc/etc\n",
            "bad-quote": "#!/bin/bash\n/bin/perl '/tmp/a-setup-etc.pl /tmp/abc-etc/etc\n",
            "existing": "#!/bin/bash\n__cybermanju_activation_observe() { :; }\n/bin/perl /tmp/a-setup-etc.pl /tmp/abc-etc/etc\n",
        }
        for label, content in cases.items():
            with self.subTest(case=label), tempfile.TemporaryDirectory() as td:
                root = Path(td)
                src, _perl, observe, shell, sleep = self.setup_fixture(root, content)
                original = src.read_bytes()
                result = self.patch(src, observe, shell, sleep)
                self.assertNotEqual(result.returncode, 0, result.stderr)
                self.assertEqual(src.read_bytes(), original)

    def test_bad_dependency_paths_rejected_without_mutating_source(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            src, _perl, observe, shell, sleep = self.setup_fixture(
                root, "#!/bin/bash\n/bin/perl /tmp/a-setup-etc.pl /tmp/abc-etc/etc\n")
            original = src.read_bytes()
            bad = self.patch(src, observe, root / "missing shell", sleep)
            self.assertNotEqual(bad.returncode, 0)
            self.assertEqual(src.read_bytes(), original)


class FullTracerTests(unittest.TestCase):
    def test_six_arg_trace_preserves_symlink_target_mode_status_and_stdio(self):
        tracer = OBSERVER.with_name("trace-activation.sh")
        with tempfile.TemporaryDirectory(prefix="full trace ") as td:
            root=Path(td); bindir=root/"bin"; bindir.mkdir()
            perl=bindir/"perl"
            perl.write_text("#!/bin/bash\nIFS= read -r data\nprintf 'payload:%s:%s:%s\\n' \"$data\" \"$1\" \"$2\"\nexit 23\n")
            perl.chmod(0o755)
            setup=root/"locked-setup-etc.pl"; etc=root/"locked-etc/etc"
            original=root/"immutable activate"
            content="#!/bin/bash -e\n"+" ".join(shlex.quote(str(p)) for p in (perl,setup,etc))+"\nprintf MUST_NOT_RUN\n"
            original.write_text(content); original.chmod(0o555)
            target=root/"activate"; target.symlink_to(original)
            run=subprocess.run(["bash",str(tracer),str(target),str(OBSERVER),str(SHELL),str(SLEEP),os.sys.executable,str(PATCHER)],capture_output=True,text=True)
            self.assertEqual(run.returncode,0,run.stderr)
            self.assertFalse(target.is_symlink())
            self.assertEqual(original.read_text(),content)
            self.assertEqual(target.stat().st_mode & 0o777,0o555)
            self.assertNotIn(str(PATCHER),target.read_text())
            self.assertNotIn(os.sys.executable,target.read_text())
            result=subprocess.run([str(target)],input="pipe-data\n",capture_output=True,text=True,timeout=3)
            self.assertEqual(result.returncode,23,result.stderr)
            self.assertEqual(result.stdout,f"payload:pipe-data:{setup}:{etc}\n")
            self.assertIn("CYBERMANJU_ACTIVATION_PROBE:end",result.stderr)
            self.assertNotIn("PASS",result.stderr)
            self.assertNotIn("MUST_NOT_RUN",result.stdout)

    def test_unsupported_six_arg_trace_keeps_original_symlink_and_no_temp(self):
        tracer=OBSERVER.with_name("trace-activation.sh")
        with tempfile.TemporaryDirectory() as td:
            root=Path(td); original=root/"store activate"
            content="#!/bin/bash\necho no-supported-setup-command\n"
            original.write_text(content); original.chmod(0o555)
            target=root/"activate"; target.symlink_to(original)
            result=subprocess.run(["bash",str(tracer),str(target),str(OBSERVER),str(SHELL),str(SLEEP),os.sys.executable,str(PATCHER)],capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertTrue(target.is_symlink())
            self.assertEqual(original.read_text(),content)
            self.assertEqual(list(root.glob("activate.trace.*")),[])


if __name__ == "__main__":
    unittest.main(verbosity=2)
