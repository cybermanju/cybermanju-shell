import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from verify_tlb_global_runtime import MARKERS, verify


class TlbGlobalRuntimeEvidenceTests(unittest.TestCase):
    def evidence(self):
        return "\n".join((*MARKERS, "test result: ok. 1 passed; 0 failed; 301 filtered out.", ""))

    def test_valid_evidence_and_exact_json_claims(self):
        self.assertEqual(verify(self.evidence()), {
            "status": "passed", "scope": "local_global_tlb_invalidation_only", "cpus": 1,
            "encoding_cases": 4, "aggregation_cases": 8, "stale_readback_cases": 7,
            "remote_shootdown_verified": False, "linux_modules_executed": False,
            "init_called": False, "smp_mapping_supported": False,
        })

    def test_ansi_and_crlf_are_normalized(self):
        evidence = "\x1b[32m" + self.evidence().replace("\n", "\r\n") + "\x1b[0m"
        self.assertEqual(verify(evidence)["status"], "passed")

    def test_each_required_marker_is_unique(self):
        for marker in MARKERS:
            line = marker + "\n"
            for text in (self.evidence().replace(line, ""), self.evidence() + line,
                         self.evidence().replace(line, "echo " + line)):
                with self.subTest(marker=marker, text=text[-100:]), self.assertRaises(ValueError):
                    verify(text)

    def test_cpu_two_is_rejected(self):
        with self.assertRaises(ValueError):
            verify(self.evidence().replace("cases=7 cpus=1", "cases=7 cpus=2"))

    def test_wrong_case_counts_or_extra_fields_are_rejected(self):
        for bad in (self.evidence().replace("cases=4", "cases=5"),
                    self.evidence().replace("cases=8", "cases=9"),
                    self.evidence().replace("cases=7 cpus=1", "cases=7 cpus=1 extra=yes")):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                verify(bad)

    def test_unknown_mixed_or_contradictory_marker_is_rejected(self):
        for marker in ("CYBERMANJU_TLB_GLOBAL:unknown:PASS", "CYBERMANJU_TLB_GLOBAL:encoding:FAIL cases=4",
                       "CYBERMANJU_OTHER:thing:PASS"):
            with self.subTest(marker=marker), self.assertRaises(ValueError):
                verify(self.evidence() + marker + "\n")

    def test_missing_or_corrupt_summaries_are_rejected(self):
        summary = "test result: ok. 1 passed; 0 failed; 301 filtered out.\n"
        for bad in (self.evidence().replace(summary, ""), self.evidence() + summary,
                    self.evidence().replace(summary, "echo " + summary),
                    self.evidence().replace("1 passed; 0 failed", "0 passed; 1 failed")):
            with self.subTest(bad=bad[-100:]), self.assertRaises(ValueError):
                verify(bad)

    def test_panic_and_failure_text_are_rejected(self):
        for bad in (self.evidence() + "kernel panic\n", self.evidence() + "panic: stale translation\n",
                    self.evidence() + "assertion failed: invalidation\n", self.evidence() + "FAIL\n", self.evidence() + "FAILED\n"):
            with self.subTest(bad=bad[-80:]), self.assertRaises(ValueError):
                verify(bad)

    def test_unexpected_control_characters_do_not_repair_markers(self):
        with self.assertRaises(ValueError):
            verify(self.evidence().replace("encoding:PASS", "encoding:\x00PASS"))

    def test_tlb_mode_preflight_preserves_config_and_invalidates_old_success(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "tools/ci"
            scripts.mkdir(parents=True)
            script = scripts / "test_module_staging_runtime.sh"
            shutil.copyfile(Path(__file__).with_name(script.name), script)
            config = root / "asterinas/ostd/OSDK.toml"
            config.parent.mkdir(parents=True)
            config.write_text("user config\n")
            out = root / "artifacts/tlb"
            out.mkdir(parents=True)
            (out / "result.json").write_text('{"status":"passed"}')
            result = subprocess.run(["bash", str(script), str(out), "--tlb-global"], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(config.read_text(), "user config\n")
            self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")
            self.assertFalse((out / "osdk-build.log").exists())


if __name__ == "__main__":
    unittest.main()
