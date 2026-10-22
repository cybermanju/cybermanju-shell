import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).parent))
from verify_tlb_remote_readback import MARKERS, verify

TOPO = "CYBERMANJU_TLB_READBACK:topology:PASS cpus=2 source=0 target=1"
GOOD = "\n".join((TOPO, *MARKERS, "test cybermanju_tlb_remote_readback_runtime ... ok", "", "test result: ok. 1 passed; 0 failed; 47 filtered out.", ""))
EXPECTED = {
    "status": "passed", "scope": "remote_tlb_byte_readback_only", "cpus": 2,
    "source": 0, "target": 1, "old_byte": 225, "new_byte": 66, "bytes": 8,
    "retained_frames": 2, "remote_bytes_observed": True, "mappings_removed": True,
    "remote_pge_restored": True, "negative_control_discriminates": False,
    "universal_coherence_verified": False, "frames_reclaimed_verified": False,
    "smp_mapping_supported": False, "linux_modules_executed": False, "init_called": False,
}


class TlbRemoteReadbackTests(unittest.TestCase):
    def test_valid_evidence_and_exact_json_claims(self):
        self.assertEqual(verify(GOOD), EXPECTED)

    def test_ansi_crlf_and_reverse_ids(self):
        self.assertEqual(verify(("\x1b[32m" + GOOD + "\x1b[0m").replace("\n", "\r\n"))["status"], "passed")
        result = verify(GOOD.replace("source=0 target=1", "source=1 target=0"))
        self.assertEqual((result["source"], result["target"]), (1, 0))

    def test_carriage_return_does_not_create_a_standalone_marker(self):
        bad = GOOD.replace(TOPO, "echo\r" + TOPO)
        with self.assertRaises(ValueError):
            verify(bad)
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "runtime.log"
            path.write_bytes(bad.encode())
            proc = subprocess.run([sys.executable, str(Path(__file__).with_name("verify_tlb_remote_readback.py")), str(path)], capture_output=True, text=True)
            self.assertNotEqual(proc.returncode, 0)

    def test_counts_claims_and_distinct_cpu_ids_are_exact(self):
        for old, new in (("cpus=2", "cpus=1"), ("old=225", "old=224"), ("new=66", "new=67"),
                         ("bytes=8", "bytes=7"), ("retained_frames=2", "retained_frames=1"),
                         ("pge=true", "pge=false"), ("global=true", "global=false"),
                         ("unmapped=true", "unmapped=false"), ("slot_reset=true", "slot_reset=false"),
                         ("pge_restored=true", "pge_restored=false"), ("init_called=false", "init_called=true")):
            with self.subTest(field=old), self.assertRaises(ValueError):
                verify(GOOD.replace(old, new))
        for ids in ("0 0", "1 1", "0 2", "2 0", "-1 0"):
            src, dst = ids.split()
            with self.subTest(ids=ids), self.assertRaises(ValueError):
                verify(GOOD.replace("source=0 target=1", f"source={src} target={dst}"))

    def test_markers_missing_duplicate_echoed_corrupt_or_unknown_are_rejected(self):
        for marker in (TOPO, *MARKERS):
            for bad in (GOOD.replace(marker, ""), GOOD + marker + "\n", GOOD.replace(marker, "echo " + marker),
                        GOOD.replace(marker, marker.replace("PASS", "FAIL")), GOOD.replace(marker, marker + " extra=yes")):
                with self.subTest(marker=marker, bad=bad[-80:]), self.assertRaises(ValueError):
                    verify(bad)
        for extra in ("CYBERMANJU_TLB_READBACK:unknown:PASS", "CYBERMANJU_OTHER:thing:PASS"):
            with self.assertRaises(ValueError):
                verify(GOOD + extra + "\n")

    def test_summary_failures_control_bytes_and_extra_output_rejected(self):
        for bad in (GOOD.replace("1 passed", "2 passed"), GOOD.replace("0 failed", "1 failed"),
                    GOOD + "test result: ok. 1 passed; 0 failed; 1 filtered out.\n", GOOD + "panic\n",
                    GOOD + "FAIL\n", "\x00" + GOOD, "\x0b" + GOOD, "\x0c" + GOOD,
                    "\x1c" + GOOD, "\x7f" + GOOD):
            with self.subTest(bad=repr(bad[-80:])), self.assertRaises(ValueError):
                verify(bad)

    def test_preflight_preserves_config_invalidates_stale_result_and_rejects_modes(self):
        runner = Path(__file__).with_name("test_module_staging_runtime.sh")
        for args in (("--tlb-remote-readback",), ("--tlb-remote-readback", "--tlb-global")):
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                scripts = root / "tools/ci"; scripts.mkdir(parents=True)
                shutil.copyfile(runner, scripts / runner.name)
                config = root / "asterinas/ostd/OSDK.toml"; config.parent.mkdir(parents=True)
                config.write_text("user config\n")
                out = root / "out"; out.mkdir(); (out / "result.json").write_text('{"status":"passed"}')
                proc = subprocess.run(["bash", str(scripts / runner.name), str(out), *args], capture_output=True, text=True)
                self.assertEqual(proc.returncode, 2)
                self.assertEqual(config.read_text(), "user config\n")
                self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")
                self.assertFalse((out / "osdk-build.log").exists())

    def test_mutual_exclusion_with_other_modes(self):
        runner = Path(__file__).with_name("test_module_staging_runtime.sh")
        for args in (("--tlb-remote-readback", "--tlb-remote"), ("--tlb-remote", "--tlb-remote-readback"),
                     ("--tlb-remote-readback", "--smp-rejection")):
            with tempfile.TemporaryDirectory() as tmp:
                out = Path(tmp) / "out"; out.mkdir(); (out / "result.json").write_text('{"status":"passed"}')
                proc = subprocess.run(["bash", str(runner), str(out), *args], capture_output=True, text=True)
                self.assertEqual(proc.returncode, 2)
                self.assertIn("mutually exclusive", proc.stderr)
                self.assertEqual(json.loads((out / "result.json").read_text())["status"], "not_verified")


if __name__ == "__main__":
    unittest.main()
