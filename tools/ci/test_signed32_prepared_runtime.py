import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from verify_signed32_prepared_runtime import BEGIN, COMPLETE, PAGE, POOL_END, signed32_fits, verify


class Signed32PreparedTests(unittest.TestCase):
    def evidence(self):
        base = POOL_END - 64 * PAGE
        self.cases = []
        for kind in (11, 2, 4):
            symbol = base + PAGE if kind == 11 else 0xFFFFFFFF88010000
            self.cases.append(f"CYBERMANJU_SIGNED32_PREPARED:staged:PASS kind={kind} base={base:#x} "
                              f"reserved=40960 span=40960 pages=10 payload=1721 relas=6 symbol={symbol:#x} "
                              f"field={base + 32:#x} compared=40960 self_defined={str(kind == 11).lower()}")
        return "\n".join([BEGIN, *self.cases, COMPLETE, "test result: ok. 1 passed; 0 failed; 2 filtered out.", ""])

    def test_valid_scope_and_signed32_boundaries(self):
        result = verify(self.evidence())
        self.assertEqual(result["status"], "passed")
        self.assertEqual((result["staged_cases"], result["transaction_rejection_cases"], result["exact_reclaim_cases"]), (3, 6, 9))
        self.assertEqual(result["cross_pool_rejection_cases"], 2)
        for key in ("signed32_self_defined_staging_verified", "relative32_kernel_numeric_alias_verified",
                    "signed32_arena_preparation_available", "legacy_lower_arena_policy_preserved"):
            self.assertTrue(result[key])
        for key in ("kernel_linux_export_lookup_available", "pages_mapped", "production_mapping_available",
                    "module_execution_verified", "linux_modules_executed", "init_called", "wx_verified", "smp_mapping_verified"):
            self.assertFalse(result[key])
        for value in (0, 0x7FFFFFFF, 0xFFFFFFFF80000000, 0xFFFFFFFFFFFFFFFF):
            self.assertTrue(signed32_fits(value))
        for value in (0x80000000, 0xFFFFFFFF7FFFFFFF, 0x10000000000000000, -1):
            self.assertFalse(signed32_fits(value))

    def test_unique_complete_unprefixed_ordered_markers(self):
        good = self.evidence()
        for marker in [BEGIN, *self.cases, COMPLETE]:
            for bad in (good.replace(marker + "\n", ""), good + marker + "\n", good.replace(marker, "echo " + marker)):
                with self.subTest(marker=marker[:75]), self.assertRaises(ValueError):
                    verify(bad)
        for bad in (good.replace("kind=4", "kind=2"), good + "CYBERMANJU_OTHER:PASS\n",
                    good.replace(COMPLETE + "\n", "") + COMPLETE + "\n",
                    good.replace(self.cases[0], "SWAP").replace(self.cases[1], self.cases[0]).replace("SWAP", self.cases[1])):
            with self.assertRaises(ValueError):
                verify(bad)

    def test_counts_and_capability_claims(self):
        good = self.evidence()
        for old, new in [("cpus=1", "cpus=2"), ("kinds=3", "kinds=2"), ("staged=3", "staged=2"),
                         ("rejected=6", "rejected=5"), ("reclaimed=9", "reclaimed=8"),
                         ("cross_pool_rejected=2", "cross_pool_rejected=1"), ("compared_full_span=true", "compared_full_span=false"),
                         ("old_arena_unchanged=true", "old_arena_unchanged=false"), ("pages_mapped=false", "pages_mapped=true"),
                         ("linux_export_lookup=false", "linux_export_lookup=true"), ("execution=false", "execution=true")]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))

    def test_geometry_field_and_complete_span(self):
        good = self.evidence()
        base = POOL_END - 64 * PAGE
        for old, new in [(f"base={base:#x}", f"base={base+1:#x}"), (f"base={base:#x}", "base=0xffffffff7ffbf000"),
                         ("reserved=40960", "reserved=4096"), ("reserved=40960", "reserved=4198400"),
                         ("span=40960", "span=40959"), ("pages=10", "pages=9"), ("payload=1721", "payload=40961"),
                         ("relas=6", "relas=0"), ("relas=6", "relas=65537"), ("compared=40960", "compared=40959"),
                         (f"field={base+32:#x}", f"field={base-1:#x}"), (f"field={base+32:#x}", f"field={base+40958:#x}")]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))
        with self.assertRaises(ValueError):
            verify(good.replace(self.cases[2], self.cases[2].replace("payload=1721", "payload=1722")))

    def test_local_symbol_and_relative_alias_cannot_be_confused(self):
        good = self.evidence()
        base = POOL_END - 64 * PAGE
        mutations = [("self_defined=true", "self_defined=false"), ("self_defined=false", "self_defined=true"),
                     (f"symbol={base+PAGE:#x}", "symbol=0xffffffff88010000"),
                     ("symbol=0xffffffff88010000", "symbol=0x0"),
                     ("symbol=0xffffffff88010000", "symbol=0xffffffffc0000000")]
        for old, new in mutations:
            with self.subTest(old=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))
        with self.assertRaises(ValueError):
            verify(good.replace(self.cases[2], self.cases[2].replace("symbol=0xffffffff88010000", "symbol=0xffffffff88020000")))

    def test_summary_serial_and_failure_evidence(self):
        good = self.evidence()
        self.assertEqual(verify("\x1b[32m" + good.replace("\n", "\r\n") + "\x1b[0m")["status"], "passed")
        for bad in (good.replace("1 passed", "0 passed"), good + "kernel panicked\n", good + "assertion failed\n",
                    good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n", "Finished dev profile\n"):
            with self.assertRaises(ValueError):
                verify(bad)

    def test_cli_invalidates_stale_pass(self):
        script = Path(__file__).with_name("verify_signed32_prepared_runtime.py")
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "runtime.log"
            log.write_text(self.evidence())
            result = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            log.write_text("Finished dev profile\n")
            result = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(json.loads(log.with_name("result.json").read_text())["status"], "not_verified")

    def test_mode_preflight_and_wrapper_forwarding(self):
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
            config.write_text("keep signed32 config\n")
            fixture = root / "fixture.ko"
            fixture.write_bytes(b"fixture")
            out = root / "out"
            env = {**os.environ, "CYBERMANJU_STAGING_FIXTURE": str(fixture)}
            result = subprocess.run(["bash", str(scripts / "test_module_staging_runtime.sh"), str(out), "--signed32-prepared"], env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn("Refusing to overwrite", result.stderr)
            self.assertEqual(config.read_text(), "keep signed32 config\n")
            self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")
            self.assertFalse((out / "osdk-build.log").exists())
            (scripts / "test_linux_module_noimports.sh").write_text('mkdir -p "$1"\nprintf fixture > "$1/cybermanju_noimports_fixture.ko"\n')
            (scripts / "test_module_staging_runtime.sh").write_text('printf "%s\\n" "$2" > "$1/mode.txt"\n')
            result = subprocess.run(["bash", str(scripts / "test_prepared_module_runtime.sh"), str(out), "--signed32-prepared"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((out / "mode.txt").read_text(), "--signed32-prepared\n")


if __name__ == "__main__":
    unittest.main()
