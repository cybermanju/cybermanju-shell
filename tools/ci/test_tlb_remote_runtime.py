import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).parent))
from verify_tlb_remote_runtime import MARKERS, verify

TOPO = "CYBERMANJU_TLB_REMOTE:topology:PASS cpus=2 source=0 target=1"
GOOD = "\n".join((TOPO, MARKERS[1], MARKERS[2], "test cybermanju_tlb_remote_runtime ... ok", "", "test result: ok. 1 passed; 0 failed; 47 filtered out.", ""))


class TlbRemoteRuntimeTests(unittest.TestCase):
    def test_valid_evidence_and_exact_claims(self):
        result = verify(GOOD)
        self.assertEqual(result["scope"], "remote_tlb_dispatch_ack_only")
        self.assertEqual((result["source"], result["target"]), (0, 1))
        for claim in ("remote_translation_coherence_verified", "frames_reclaimed_verified", "smp_mapping_supported", "linux_modules_executed", "init_called", "pages_mapped"):
            self.assertIs(result[claim], False)

    def test_ansi_crlf_and_reverse_ids(self):
        self.assertEqual(verify(("\x1b[32m" + GOOD + "\x1b[0m").replace("\n", "\r\n"))["status"], "passed")
        self.assertEqual(verify(GOOD.replace("source=0 target=1", "source=1 target=0"))["source"], 1)

    def test_unexpected_controls_are_not_line_separators(self):
        for control in ("\x00", "\x0b", "\x0c", "\x1c", "\x7f"):
            with self.subTest(control=repr(control)), self.assertRaises(ValueError):
                verify(control + GOOD)

    def test_counts_and_claims_are_exact(self):
        for old, new in (("cpus=2", "cpus=1"), ("cases=4", "cases=3"), ("remote_batches=4", "remote_batches=3"), ("local_batches=1", "local_batches=0"), ("global_batches=2", "global_batches=1"), ("empty_cases=2", "empty_cases=1"), ("sync_reused=true", "sync_reused=false"), ("maps_installed=false", "maps_installed=true"), ("init_called=false", "init_called=true")):
            with self.subTest(field=old), self.assertRaises(ValueError):
                verify(GOOD.replace(old, new))

    def test_required_marker_each_corrupted_duplicated_missing_echoed(self):
        for marker in (TOPO, *MARKERS[1:]):
            for bad in (GOOD.replace(marker, ""), GOOD + marker + "\n", GOOD.replace(marker, "echo " + marker), GOOD.replace(marker, marker.replace("PASS", "FAIL")), GOOD.replace(marker, marker + " extra=yes")):
                with self.subTest(marker=marker, bad=bad[-90:]), self.assertRaises(ValueError):
                    verify(bad)

    def test_cpu_ids_distinct_and_bounded(self):
        for ids in ("0 0", "1 1", "0 2", "2 0", "-1 0"):
            src, dst = ids.split()
            with self.subTest(ids=ids), self.assertRaises(ValueError):
                verify(GOOD.replace("source=0 target=1", f"source={src} target={dst}"))

    def test_summary_failures_markers_and_claim_text_rejected(self):
        for bad in (GOOD.replace("1 passed", "2 passed"), GOOD.replace("0 failed", "1 failed"), GOOD + "test result: ok. 1 passed; 0 failed; 1 filtered out.\n", GOOD + "panic\n", GOOD + "FAIL\n", GOOD + "CYBERMANJU_TLB_REMOTE:other:PASS\n", GOOD + "CYBERMANJU_OTHER:x:PASS\n"):
            with self.subTest(bad=bad[-100:]), self.assertRaises(ValueError):
                verify(bad)

    def test_preflight_existing_config_preserves_and_invalidates_result(self):
        runner = Path(__file__).with_name("test_module_staging_runtime.sh")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); scripts = root / "tools/ci"; scripts.mkdir(parents=True)
            shutil.copyfile(runner, scripts / runner.name)
            config = root / "asterinas/ostd/OSDK.toml"; config.parent.mkdir(parents=True); config.write_text("user config\n")
            out = root / "out"; out.mkdir(); (out / "result.json").write_text('{"status":"passed"}')
            proc = subprocess.run(["bash", str(scripts / runner.name), str(out), "--tlb-remote"], capture_output=True, text=True)
            self.assertEqual(proc.returncode, 2); self.assertEqual(config.read_text(), "user config\n")
            self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")
            self.assertFalse((out / "osdk-build.log").exists())

    def test_runner_rejects_multiple_modes_including_tlb_global(self):
        runner = Path(__file__).with_name("test_module_staging_runtime.sh")
        for first, second in (("--tlb-remote", "--tlb-global"), ("--tlb-global", "--tlb-remote"), ("--tlb-remote", "--smp-rejection")):
            with tempfile.TemporaryDirectory() as tmp:
                out = Path(tmp) / "out"
                out.mkdir()
                (out / "result.json").write_text('{"status":"passed"}')
                proc = subprocess.run(["bash", str(runner), str(out), first, second], capture_output=True, text=True)
                self.assertEqual(proc.returncode, 2)
                self.assertIn("mutually exclusive", proc.stderr)
                self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")


if __name__ == "__main__":
    unittest.main()
