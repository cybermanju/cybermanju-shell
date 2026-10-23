import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from verify_elf_ro_lease_runtime import verify

class RuntimeTests(unittest.TestCase):
    def evidence(self, pages=10):
        p = pages * 4096; env = min(pages + 2, 1024) * 4096
        cases = []
        for kind, reloc in ((1, 8), (2, 4), (4, 4), (11, 4)):
            cases.append(f"CYBERMANJU_ELF_RO_LEASE:case:PASS kind={kind} span={p} envelope={env} pages={pages} bytes={p+reloc} relocation_bytes={reloc} frame_matches={pages} ro_nonx={pages} rw_nonx=0 planned_pages={pages} planned_rx={min(2, pages)} acquired={pages} dropped={pages} retained=true layout_preserved=true plan_preserved=true reclaimed=true")
        total = 11 * pages
        return "\n".join(["CYBERMANJU_ELF_RO_LEASE:begin:PASS cpus=1 kinds=4", *cases, f"CYBERMANJU_ELF_RO_LEASE:faults:PASS cases=3 prefix_pages={pages+1} reclaims=3 acquired={3*pages} dropped={3*pages} live=0", f"CYBERMANJU_ELF_RO_LEASE:budgets:PASS cases=2 public_cases=2 zero_budget=0 short_budget={pages-1} mapper_entries=0 reclaims=4 acquired={4*pages} dropped={4*pages} live=0", f"CYBERMANJU_ELF_RO_LEASE:complete:PASS mappings=4 bounds_rejected=8 reclaims=11 acquired={total} dropped={total} live=0 elf_handoff=true plan_applied=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false", "test result: ok. 1 passed; 0 failed; 2 filtered out.", ""])

    def test_positive_and_dynamic_totals(self):
        for pages in (1, 10, 1024):
            r = verify(self.evidence(pages))
            self.assertEqual(r["handles_acquired"], 11 * pages)
            self.assertEqual(r["total_mapped_read_bytes"], 4 * pages * 4096 + 20)
            self.assertEqual(r["scope"], "single_cpu_elf_uniform_read_only_nonexec_alias")
            for key in ("physical_wx", "smp_mapping", "write_fault_probed", "plan_applied", "immutable_data", "linux_full_abi_verified", "production_qualification"):
                self.assertFalse(r[key])

    def test_mutations_and_structure(self):
        good = self.evidence()
        changes = [("span=40960", "span=40961"), ("envelope=49152", "envelope=2048"), ("pages=10", "pages=9"), ("bytes=40968", "bytes=40967"), ("relocation_bytes=8", "relocation_bytes=4"), ("frame_matches=10", "frame_matches=9"), ("ro_nonx=10", "ro_nonx=9"), ("rw_nonx=0", "rw_nonx=1"), ("planned_pages=10", "planned_pages=9"), ("planned_rx=2", "planned_rx=0"), ("acquired=10", "acquired=9"), ("dropped=10", "dropped=9"), ("retained=true", "retained=false"), ("layout_preserved=true", "layout_preserved=false"), ("plan_preserved=true", "plan_preserved=false"), ("reclaimed=true", "reclaimed=false"), ("prefix_pages=11", "prefix_pages=10"), ("short_budget=9", "short_budget=8"), ("acquired=30", "acquired=29"), ("reclaims=11", "reclaims=8"), ("public_cases=2", "public_cases=1"), ("reclaims=4 acquired=40", "reclaims=3 acquired=40"), ("physical_wx=false", "physical_wx=true")]
        for old, new in changes:
            with self.subTest(old=old), self.assertRaises(ValueError): verify(good.replace(old, new, 1))
        pages_mixed = good.replace("pages=10", "pages=11", 1)
        with self.assertRaises(ValueError): verify(pages_mixed)
        for bad in (good.replace("\ntest result", "\nCYBERMANJU_UNKNOWN:unexpected\ntest result"), good + "CYBERMANJU_OTHER:x\n", good.replace("test result: ok. 1 passed; 0 failed", "test result: ok. 1 passed; 1 failed"), good.replace("test result: ok. 1 passed", "test result: ok. 2 passed"), good.replace("test result: ok. 1 passed; 0 failed; 2 filtered out.", "test result: ok. 1 passed; 0 failed; 2 filtered out.\ntest result: ok. 1 passed; 0 failed; 2 filtered out."), good + "kernel panicked\n", good + "assertion failed\n", good.replace("\n", "\r\n").replace("begin:PASS", "\x1b[32mbegin:PASS\x1b[0m")):
            if bad.startswith("\x1b") or "\r\n" in bad:
                self.assertEqual(verify(bad)["status"], "passed")
            else:
                with self.assertRaises(ValueError): verify(bad)

    def test_cli_clears_stale_result(self):
        script = Path(__file__).with_name("verify_elf_ro_lease_runtime.py")
        with tempfile.TemporaryDirectory() as d:
            log = Path(d) / "runtime.log"; out = Path(d) / "result.json"
            log.write_text(self.evidence())
            p = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertEqual(p.returncode, 0, p.stderr); self.assertEqual(json.loads(out.read_text())["status"], "passed")
            log.write_text("bad\n"); p = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertNotEqual(p.returncode, 0); self.assertEqual(json.loads(out.read_text()), {"status": "not_verified"})
            log.unlink(); p = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
            self.assertNotEqual(p.returncode, 0); self.assertEqual(json.loads(out.read_text()), {"status": "not_verified"})

if __name__ == "__main__": unittest.main()
