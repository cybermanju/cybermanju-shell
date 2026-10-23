import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from verify_elf_ro_lease_smp_runtime import verify

class SmpRuntimeTests(unittest.TestCase):
    def evidence(self, pages=10):
        acquired = pages * 4
        return "\n".join([f"CYBERMANJU_ELF_RO_LEASE_SMP:begin:PASS cpus=2 kinds=4 pages={pages}", f"CYBERMANJU_ELF_RO_LEASE_SMP:denied:PASS attempts=4 reclaims=4 acquired={acquired} dropped={acquired} live=0 irq_preserved=true", "CYBERMANJU_ELF_RO_LEASE_SMP:complete:PASS elf_handoff=false plan_applied=false module_execution=false physical_wx=false smp_mapping=false write_fault_probed=false", "test result: ok. 1 passed; 0 failed; 1 filtered out.", ""])

    def test_dynamic_results_and_scope(self):
        for pages in (1, 10, 1024):
            r = verify(self.evidence(pages))
            self.assertEqual(r["handles_acquired"], pages * 4)
            self.assertEqual(r["scope"], "smp_rejection_only")
            for k in ("elf_handoff", "plan_applied", "module_execution", "physical_wx", "smp_mapping", "write_fault_probed", "linux_full_abi_verified", "production_qualification"):
                self.assertFalse(r[k])

    def test_mutations_and_marker_discipline(self):
        good = self.evidence()
        for old, new in (("cpus=2", "cpus=1"), ("pages=10", "pages=0"), ("pages=10", "pages=1025"), ("attempts=4", "attempts=3"), ("reclaims=4", "reclaims=3"), ("acquired=40", "acquired=39"), ("dropped=40", "dropped=39"), ("live=0", "live=1"), ("irq_preserved=true", "irq_preserved=false"), ("smp_mapping=false", "smp_mapping=true"), ("elf_handoff=false", "elf_handoff=true")):
            with self.subTest(old=old), self.assertRaises(ValueError): verify(good.replace(old, new, 1))
        for bad in (good.replace("begin:PASS", "begin:FAIL"), good.replace("\ntest result", "\nCYBERMANJU_ELF_RO_LEASE_SMP:denied:PASS attempts=4 reclaims=4 acquired=40 dropped=40 live=0 irq_preserved=true\ntest result"), good + "CYBERMANJU_X:bad\n", good.replace("1 passed", "0 passed"), good.replace("1 passed", "2 passed"), good + "test result: ok. 1 passed; 0 failed; 0 filtered out.\n", good + "kernel panicked\n", good + "assertion failed\n"):
            with self.assertRaises(ValueError): verify(bad)
        normalized = "\x1b[32m" + good.replace("\n", "\r\n") + "\x1b[0m"
        self.assertEqual(verify(normalized)["status"], "passed")

    def test_cli_clears_stale_result(self):
        script = Path(__file__).with_name("verify_elf_ro_lease_smp_runtime.py")
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
