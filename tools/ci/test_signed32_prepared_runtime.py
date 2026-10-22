import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from verify_signed32_prepared_runtime import (
    BEGIN, COMPLETE, MAPPING, PAGE, POOL_END, signed32_fits, verify,
)


class PreparedRuntimeTests(unittest.TestCase):
    def evidence(self):
        base = POOL_END - 64 * PAGE
        cases = []
        for kind in (11, 2, 4):
            symbol = base + PAGE if kind == 11 else 0xFFFFFFFF88010000
            cases.append(
                f"CYBERMANJU_SIGNED32_PREPARED:staged:PASS kind={kind} "
                f"base={base:#x} reserved=40960 span=40960 pages=10 "
                f"payload=1721 relas=6 symbol={symbol:#x} "
                f"field={base + 32:#x} compared=40960 "
                f"self_defined={str(kind == 11).lower()}"
            )
        return "\n".join([
            BEGIN, *cases, MAPPING, COMPLETE,
            "test result: ok. 1 passed; 0 failed; 2 filtered out.", "",
        ]), cases

    def test_valid_scope_and_signed32_boundaries(self):
        good, _ = self.evidence()
        result = verify(good)
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["scope"], "signed32_arena_temporary_nonexecuting_mapping")
        self.assertEqual((result["staged_cases"], result["transaction_rejection_cases"],
                          result["exact_reclaim_cases"], result["cross_pool_rejection_cases"]),
                         (3, 6, 9, 2))
        self.assertEqual((result["temporary_mapping_full_span_cases"],
                          result["temporary_mapping_partial_cases"],
                          result["temporary_mapping_rejected_cases"],
                          result["temporary_mapping_compared_bytes"],
                          result["temporary_mapping_unmapped_scopes"]), (3, 6, 6, 122928, 9))
        for key in ("signed32_self_defined_staging_verified",
                    "relative32_kernel_numeric_alias_verified",
                    "signed32_arena_preparation_available",
                    "legacy_lower_arena_policy_preserved",
                    "temporary_mapping_verified", "pages_mapped", "pte_rw_verified",
                    "pte_supervisor_verified", "pte_nonexec_verified",
                    "mapping_absent_after_scopes", "test_export_bindings_are_synthetic"):
            self.assertTrue(result[key])
        for key in ("kernel_linux_export_lookup_available", "production_mapping_available",
                    "module_execution_verified", "linux_modules_executed", "init_called",
                    "wx_verified", "smp_mapping_verified"):
            self.assertFalse(result[key])
        for value in (0, 0x7FFFFFFF, 0xFFFFFFFF80000000, 0xFFFFFFFFFFFFFFFF):
            self.assertTrue(signed32_fits(value))
        for value in (0x80000000, 0xFFFFFFFF7FFFFFFF, 0x10000000000000000, -1):
            self.assertFalse(signed32_fits(value))

    def test_markers_unique_complete_unprefixed_and_ordered(self):
        good, cases = self.evidence()
        for marker in (BEGIN, *cases, MAPPING, COMPLETE):
            variants = (good.replace(marker + "\n", ""), good + marker + "\n",
                        good.replace(marker, "echo " + marker),
                        good.replace(marker, "prefix " + marker))
            for bad in variants:
                with self.subTest(marker=marker[:65]), self.assertRaises(ValueError):
                    verify(bad)
        swaps = good.replace(cases[0], "SWAP").replace(cases[1], cases[0]).replace("SWAP", cases[1])
        for bad in (good.replace("kind=4", "kind=2"), good + "CYBERMANJU_OTHER:PASS\n",
                    good.replace(COMPLETE + "\n", "") + COMPLETE + "\n", swaps,
                    good.replace(MAPPING, "prefix " + MAPPING)):
            with self.assertRaises(ValueError):
                verify(bad)

    def test_counts_and_all_capability_claims_are_strict(self):
        good, _ = self.evidence()
        changes = (
            ("cpus=1", "cpus=2"), ("kinds=3", "kinds=2"), ("rejected=6", "rejected=5"),
            ("cross_pool_rejected=2", "cross_pool_rejected=1"), ("full_span_cases=3", "full_span_cases=2"),
            ("partial_cases=6", "partial_cases=5"), ("scopes_unmapped=9", "scopes_unmapped=8"),
            ("pte_rw=true", "pte_rw=false"), ("pte_supervisor=true", "pte_supervisor=false"),
            ("pte_nonexec=true", "pte_nonexec=false"), ("temporary=true", "temporary=false"),
            ("production=false", "production=true"), ("execution=false", "execution=true"),
            ("staged=3", "staged=2"), ("reclaimed=9", "reclaimed=8"),
            ("compared_full_span=true", "compared_full_span=false"),
            ("old_arena_unchanged=true", "old_arena_unchanged=false"),
            ("pages_mapped=true", "pages_mapped=false"),
            ("temporary_mapping=true", "temporary_mapping=false"),
            ("all_unmapped=true", "all_unmapped=false"),
            ("linux_export_lookup=false", "linux_export_lookup=true"),
        )
        for old, new in changes:
            with self.subTest(claim=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))

    def test_mapping_total_tracks_each_geometry(self):
        good, _ = self.evidence()
        mutated = good.replace("span=40960", "span=36864")
        mutated = mutated.replace("pages=10", "pages=9")
        mutated = mutated.replace("compared=40960", "compared=36864")
        with self.assertRaisesRegex(ValueError, "readback count"):
            verify(mutated)

    def test_geometry_field_and_complete_span(self):
        good, cases = self.evidence()
        base = POOL_END - 64 * PAGE
        changes = (
            (f"base={base:#x}", f"base={base + 1:#x}"),
            (f"base={base:#x}", "base=0xffffffff7ffbf000"),
            ("reserved=40960", "reserved=4096"), ("reserved=40960", "reserved=4198400"),
            ("span=40960", "span=40959"), ("pages=10", "pages=9"),
            ("payload=1721", "payload=40961"), ("relas=6", "relas=0"),
            ("relas=6", "relas=65537"), ("compared=40960", "compared=40959"),
            (f"field={base + 32:#x}", f"field={base - 1:#x}"),
            (f"field={base + 32:#x}", f"field={base + 40958:#x}"),
        )
        for old, new in changes:
            with self.subTest(change=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))
        with self.assertRaises(ValueError):
            verify(good.replace(cases[2], cases[2].replace("payload=1721", "payload=1722")))

    def test_local_symbol_and_relative_numeric_alias(self):
        good, _ = self.evidence()
        base = POOL_END - 64 * PAGE
        mutations = (
            ("self_defined=true", "self_defined=false"),
            ("self_defined=false", "self_defined=true"),
            (f"symbol={base + PAGE:#x}", "symbol=0xffffffff88010000"),
            ("symbol=0xffffffff88010000", "symbol=0x0"),
            ("symbol=0xffffffff88010000", "symbol=0xffffffffc0000000"),
        )
        for old, new in mutations:
            with self.subTest(change=old), self.assertRaises(ValueError):
                verify(good.replace(old, new))
        third = next(line for line in self.evidence()[1] if "kind=4 " in line)
        with self.assertRaises(ValueError):
            verify(good.replace(third, third.replace("symbol=0xffffffff88010000",
                                                       "symbol=0xffffffff88020000")))

    def test_summary_serial_and_failure_evidence(self):
        good, _ = self.evidence()
        decorated = "\x1b[32m" + good.replace("\n", "\r\n") + "\x1b[0m"
        self.assertEqual(verify(decorated)["status"], "passed")
        for bad in (good.replace("1 passed", "0 passed"), good + "kernel panicked\n",
                    good + "assertion failed\n",
                    good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n",
                    "Finished dev profile\n"):
            with self.assertRaises(ValueError):
                verify(bad)

    def test_legacy_transcript_is_rejected(self):
        good, _ = self.evidence()
        legacy = good.replace(MAPPING + "\n", "")
        legacy = legacy.replace("pages_mapped=true temporary_mapping=true all_unmapped=true",
                                "pages_mapped=false")
        with self.assertRaises(ValueError):
            verify(legacy)

    def test_cli_stale_result_invalidation(self):
        script = Path(__file__).with_name("verify_signed32_prepared_runtime.py")
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "runtime.log"
            good, _ = self.evidence()
            log.write_text(good)
            proc = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertIn("temporary non-executable mapping", proc.stdout)
            self.assertEqual(json.loads(log.with_name("result.json").read_text())["scope"],
                             "signed32_arena_temporary_nonexecuting_mapping")
            log.write_text("old preparation-only transcript\n")
            proc = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0)
            self.assertEqual(json.loads(log.with_name("result.json").read_text())["status"],
                             "not_verified")

    def test_wrapper_preflight_preserves_config_and_forwards_both_modes(self):
        original = Path(__file__).resolve().parents[2]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            shutil = __import__("shutil")
            shutil.copytree(original / "tools/ci", root / "tools/ci",
                            ignore=shutil.ignore_patterns("*.pyc"))
            (root / "asterinas/osdk").mkdir(parents=True)
            config = root / "asterinas/osdk/OSDK.toml"
            config.write_text("user-owned config\n")
            for mode in ("--signed32-prepared", "--signed32-mapping-smp"):
                output = root / "out"
                output.mkdir(exist_ok=True)
                fixture = root / "fixture-dir"
                fixture.mkdir(exist_ok=True)
                (fixture / "cybermanju_noimports_fixture.ko").write_text("fixture")
                config.write_text("user-owned config\n")
                # The runtime wrapper receives fixture and mode through shell scripts, without Cargo/QEMU.
                noimports = root / "tools/ci/test_linux_module_noimports.sh"
                noimports.write_text('#!/usr/bin/env bash\nmkdir -p "$1"\nprintf fixture > "$1/cybermanju_noimports_fixture.ko"\n')
                staging = root / "tools/ci/test_module_staging_runtime.sh"
                staging.write_text('#!/usr/bin/env bash\nprintf "%s\\n%s\\n" "$2" "$CYBERMANJU_STAGING_FIXTURE" > "$1/forwarded.txt"\n')
                for script in (noimports, staging):
                    script.chmod(0o755)
                proc = subprocess.run(["bash", str(root / "tools/ci/test_prepared_module_runtime.sh"),
                                       str(output), mode], capture_output=True, text=True)
                self.assertEqual(proc.returncode, 0, proc.stderr)
                self.assertEqual(config.read_text(), "user-owned config\n")
                forwarded = (output / "forwarded.txt").read_text().splitlines()
                self.assertEqual(forwarded[0], mode)
                self.assertEqual(forwarded[1], str(output / "fixture/cybermanju_noimports_fixture.ko"))


if __name__ == "__main__":
    unittest.main()
