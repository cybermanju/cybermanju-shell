import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from verify_signed32_search_runtime import BEGIN, COMPLETE, PAGE, POOL_START, verify


class Signed32SearchTests(unittest.TestCase):
    def evidence(self):
        first = POOL_START
        selected = first + PAGE
        self.cases = []
        for kind in (11, 2, 4):
            symbol = selected + PAGE if kind == 11 else 0xFFFFFFFF88010000
            self.cases.append(f"CYBERMANJU_SIGNED32_SEARCH:staged:PASS kind={kind} first={first:#x} selected={selected:#x} "
                              f"reserved=40960 span=40960 pages=10 payload=1721 relas=6 symbol={symbol:#x} "
                              f"field={selected + 32:#x} compared=40960 self_defined={str(kind == 11).lower()}")
        return "\n".join([BEGIN, *self.cases, COMPLETE, "test result: ok. 1 passed; 0 failed; 3 filtered out.", ""])

    def test_valid_scope_and_budgets(self):
        r = verify(self.evidence())
        self.assertEqual(r["status"], "passed")
        self.assertEqual((r["staged_cases"], r["one_candidate_prefix_exhaustions"], r["rejected_input_cases"], r["available_range_checks"]), (3, 3, 9, 12))
        self.assertEqual((r["selected_case_candidate_budget"], r["supported_candidate_budget_max"]), (2, 64))
        for key in ("signed32_candidate_search_verified", "bounded_prefix_verified", "caller_window_clipping_verified",
                    "occupied_first_candidate_skipped", "full_span_readback_verified"):
            self.assertTrue(r[key])
        for key in ("pages_mapped", "production_mapping_available", "module_execution_verified", "linux_modules_executed",
                    "init_called", "wx_verified", "smp_mapping_verified", "kernel_linux_export_lookup_available"):
            self.assertFalse(r[key])

    def test_markers_must_be_unique_unprefixed_complete_and_ordered(self):
        good = self.evidence()
        for marker in [BEGIN, *self.cases, COMPLETE]:
            for bad in (good.replace(marker + "\n", ""), good + marker + "\n", good.replace(marker, "echo " + marker)):
                with self.subTest(marker=marker[:80]), self.assertRaises(ValueError):
                    verify(bad)
        for bad in (good.replace("kind=4", "kind=2"), good + "CYBERMANJU_OTHER:PASS\n",
                    good.replace(COMPLETE + "\n", "") + COMPLETE + "\n",
                    good.replace(self.cases[0], "SWAP").replace(self.cases[1], self.cases[0]).replace("SWAP", self.cases[1])):
            with self.assertRaises(ValueError):
                verify(bad)

    def test_case_counts_and_unearned_capabilities(self):
        good = self.evidence()
        for old, new in [("cpus=1", "cpus=2"), ("kinds=3", "kinds=2"), ("staged=3", "staged=2"),
                         ("prefix_exhaustions=3", "prefix_exhaustions=2"), ("rejected=9", "rejected=8"),
                         ("available_checks=12", "available_checks=11"), ("compared_full_span=true", "compared_full_span=false"),
                         ("pages_mapped=false", "pages_mapped=true"), ("linux_export_lookup=false", "linux_export_lookup=true"),
                         ("execution=false", "execution=true")]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))

    def test_second_candidate_and_envelope_are_not_optional(self):
        good = self.evidence()
        selected = POOL_START + PAGE
        for old, new in [(f"selected={selected:#x}", f"selected={POOL_START:#x}"),
                         (f"selected={selected:#x}", f"selected={selected+PAGE:#x}"),
                         (f"first={POOL_START:#x}", f"first={POOL_START-PAGE:#x}"),
                         ("reserved=40960", "reserved=4096"), ("reserved=40960", "reserved=4198400"),
                         ("span=40960", "span=40959"), ("pages=10", "pages=9"), ("payload=1721", "payload=40961"),
                         ("relas=6", "relas=0"), ("relas=6", "relas=65537"), ("compared=40960", "compared=40959"),
                         (f"field={selected+32:#x}", f"field={selected-1:#x}"),
                         (f"field={selected+32:#x}", f"field={selected+40958:#x}")]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))
        with self.assertRaises(ValueError):
            verify(good.replace(self.cases[2], self.cases[2].replace("payload=1721", "payload=1722")))

    def test_local_symbol_and_code_alias_cannot_be_substituted(self):
        good = self.evidence()
        selected = POOL_START + PAGE
        for old, new in [("self_defined=true", "self_defined=false"), ("self_defined=false", "self_defined=true"),
                         (f"symbol={selected+PAGE:#x}", "symbol=0xffffffff88010000"),
                         ("symbol=0xffffffff88010000", "symbol=0x0"),
                         ("symbol=0xffffffff88010000", "symbol=0xffffffffc0000000")]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))
        with self.assertRaises(ValueError):
            verify(good.replace(self.cases[2], self.cases[2].replace("symbol=0xffffffff88010000", "symbol=0xffffffff88020000")))

    def test_summary_serial_and_failure_detection(self):
        good = self.evidence()
        self.assertEqual(verify("\x1b[32m" + good.replace("\n", "\r\n") + "\x1b[0m")["status"], "passed")
        for bad in (good.replace("1 passed", "0 passed"), good + "kernel panicked\n", good + "assertion failed\n",
                    good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n", "Finished dev profile\n"):
            with self.assertRaises(ValueError):
                verify(bad)

    def test_cli_invalidates_stale_success(self):
        script = Path(__file__).with_name("verify_signed32_search_runtime.py")
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "runtime.log"
            log.write_text(self.evidence())
            result = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            log.write_text("Finished dev profile\n")
            result = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(json.loads(log.with_name("result.json").read_text())["status"], "not_verified")

    def test_mode_recognized_before_build_and_forwarded_by_wrapper(self):
        import os
        import shutil
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "tools/ci"
            scripts.mkdir(parents=True)
            for name in ("test_prepared_module_runtime.sh", "test_module_staging_runtime.sh"):
                shutil.copyfile(Path(__file__).with_name(name), scripts / name)
            config = root / "asterinas/kernel/libs/linux-module-staging/OSDK.toml"
            config.parent.mkdir(parents=True)
            config.write_text("keep search config\n")
            fixture = root / "fixture.ko"
            fixture.write_bytes(b"fixture")
            out = root / "out"
            env = {**os.environ, "CYBERMANJU_STAGING_FIXTURE": str(fixture)}
            result = subprocess.run(["bash", str(scripts / "test_module_staging_runtime.sh"), str(out), "--signed32-search"], env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn("Refusing to overwrite", result.stderr)
            self.assertEqual(config.read_text(), "keep search config\n")
            self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")
            self.assertFalse((out / "osdk-build.log").exists())
            (scripts / "test_linux_module_noimports.sh").write_text('mkdir -p "$1"\nprintf fixture > "$1/cybermanju_noimports_fixture.ko"\n')
            (scripts / "test_module_staging_runtime.sh").write_text('printf "%s\\n" "$2" > "$1/mode.txt"\n')
            result = subprocess.run(["bash", str(scripts / "test_prepared_module_runtime.sh"), str(out), "--signed32-search"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((out / "mode.txt").read_text(), "--signed32-search\n")


if __name__ == "__main__":
    unittest.main()
