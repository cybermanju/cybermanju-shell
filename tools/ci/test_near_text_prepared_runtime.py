import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from verify_near_text_prepared_runtime import BEGIN, COMPLETE, PAGE, POOL_END, verify


class NearTextPreparedTests(unittest.TestCase):
    def evidence(self):
        base = POOL_END - 64 * PAGE
        self.cases = [f"CYBERMANJU_NEAR_TEXT_PREPARED:staged:PASS kind={kind} base={base:#x} "
                      "reserved=40960 span=40960 pages=10 payload=1721 relas=6 "
                      "export=0xffffffff88010000 compared=40960" for kind in (2, 4)]
        return "\n".join([BEGIN, *self.cases, COMPLETE, "test result: ok. 1 passed; 0 failed; 1 filtered out.", ""])

    def test_valid_data_only_scope(self):
        result = verify(self.evidence())
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["scope"], "near_text_data_only_preparation")
        self.assertEqual((result["near_text_staged_cases"], result["transaction_rejection_cases"], result["exact_reclaim_cases"]), (2, 6, 8))
        for key in ("near_text_data_staging_verified", "full_relocation_preflight_verified", "self_signed32_arena_overflow_rejected",
                    "near_text_preparation_available", "actual_kernel_address_numeric_only", "test_export_bindings_are_synthetic"):
            self.assertTrue(result[key])
        for key in ("kernel_linux_export_lookup_available", "pages_mapped", "production_mapping_available", "module_execution_verified",
                    "linux_modules_executed", "init_called", "wx_verified", "smp_mapping_verified"):
            self.assertFalse(result[key])

    def test_missing_duplicate_echoed_unknown_and_reordered_markers(self):
        good = self.evidence()
        for marker in [BEGIN, *self.cases, COMPLETE]:
            for bad in (good.replace(marker + "\n", ""), good + marker + "\n", good.replace(marker, "echo " + marker)):
                with self.subTest(marker=marker[:65]), self.assertRaises(ValueError):
                    verify(bad)
        bads = [good + "CYBERMANJU_OTHER:PASS\n", good.replace("kind=4", "kind=2"),
                good.replace(COMPLETE + "\n", "") + COMPLETE + "\n",
                good.replace(self.cases[0], "SWAP").replace(self.cases[1], self.cases[0]).replace("SWAP", self.cases[1])]
        for bad in bads:
            with self.assertRaises(ValueError):
                verify(bad)

    def test_counts_and_unearned_capability_flags_fail_closed(self):
        good = self.evidence()
        for old, new in [("cpus=1", "cpus=2"), ("kinds=2", "kinds=1"), ("staged=2", "staged=1"),
                         ("rejected=6", "rejected=5"), ("reclaimed=8", "reclaimed=7"),
                         ("compared_full_span=true", "compared_full_span=false"), ("actual_kernel_address=true", "actual_kernel_address=false"),
                         ("linux_export_lookup=false", "linux_export_lookup=true"), ("pages_mapped=false", "pages_mapped=true"),
                         ("execution=false", "execution=true")]:
            with self.subTest(old=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))

    def test_geometry_readback_and_numeric_code_address(self):
        good = self.evidence()
        base = POOL_END - 64 * PAGE
        for old, new in [(f"base={base:#x}", f"base={base+1:#x}"), (f"base={base:#x}", "base=0x1000"),
                         ("reserved=40960", "reserved=4096"), ("reserved=40960", "reserved=4198400"),
                         ("span=40960", "span=40959"), ("pages=10", "pages=9"), ("payload=1721", "payload=0"),
                         ("payload=1721", "payload=40961"), ("relas=6", "relas=0"), ("relas=6", "relas=65537"),
                         ("compared=40960", "compared=40959"), ("export=0xffffffff88010000", "export=0x0"),
                         ("export=0xffffffff88010000", "export=0xffffffffc0000000")]:
            with self.subTest(old=old, new=new), self.assertRaises(ValueError):
                verify(good.replace(old, new))
        with self.assertRaises(ValueError):
            verify(good.replace(self.cases[1], self.cases[1].replace("payload=1721", "payload=1722")))

    def test_summary_panic_and_serial_normalization(self):
        good = self.evidence()
        self.assertEqual(verify("\x1b[32m" + good.replace("\n", "\r\n") + "\x1b[0m")["status"], "passed")
        for bad in [good.replace("1 passed", "0 passed"), good + "kernel panicked\n", good + "assertion failed\n",
                    good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n", "Finished dev profile\n"]:
            with self.assertRaises(ValueError):
                verify(bad)

    def test_cli_invalidates_old_success_before_failed_revalidation(self):
        script = Path(__file__).with_name("verify_near_text_prepared_runtime.py")
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "runtime.log"
            log.write_text(self.evidence())
            result = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            log.write_text("Finished dev profile\n")
            result = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(json.loads(log.with_name("result.json").read_text())["status"], "not_verified")

    def test_modes_fail_before_build_and_invalidate_stale_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "tools/ci"
            scripts.mkdir(parents=True)
            for name in ("test_prepared_module_runtime.sh", "test_module_staging_runtime.sh"):
                shutil.copyfile(Path(__file__).with_name(name), scripts / name)
            config = root / "asterinas/kernel/libs/linux-module-staging/OSDK.toml"
            config.parent.mkdir(parents=True)
            config.write_text("keep me\n")
            fixture = root / "fixture.ko"
            fixture.write_bytes(b"fixture")
            import os
            env = {**os.environ, "CYBERMANJU_STAGING_FIXTURE": str(fixture)}
            commands = [("test_module_staging_runtime.sh", "--near-text-prepared"),
                        ("test_module_staging_runtime.sh", "--near-text-prepared", "--prepared-module"),
                        ("test_prepared_module_runtime.sh", "--unknown"),
                        ("test_prepared_module_runtime.sh", "--near-text-prepared", "--prepared-module")]
            for args in commands:
                out = root / "artifacts/test"
                out.mkdir(parents=True, exist_ok=True)
                (out / "result.json").write_text('{"status":"passed"}')
                result = subprocess.run(["bash", str(scripts / args[0]), str(out), *args[1:]], env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode, 2, f"{args!r}: {result.stderr} {result.stdout}")
                self.assertEqual(config.read_text(), "keep me\n")
                self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")
                self.assertFalse((out / "osdk-build.log").exists())
                self.assertFalse((out / "fixture-build.log").exists())


if __name__ == "__main__":
    unittest.main()
