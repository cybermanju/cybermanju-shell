import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from verify_elf_lease_smp_runtime import BEGIN, DENIED, COMPLETE, verify

class ElfLeaseSMPTests(unittest.TestCase):
    def evidence(self):
        return "\n".join([BEGIN,DENIED,COMPLETE,"test result: ok. 1 passed; 0 failed; 0 filtered out.",""])

    def test_denial_metrics_and_limits(self):
        r=verify(self.evidence()); self.assertEqual((r["cpus"],r["denied_attempts"],r["reclaimed_reservations"]),(2,4,4))
        self.assertEqual(r["scope"],"experimental_elf_persistent_lease_smp_denial_only")
        for key in ("elf_handoff_verified","persistent_mapping_available","linux_modules_executed","linux_abi_verified","physical_wx_verified","smp_mapping_verified","production_mapping_ready","tlb_shootdown_verified"): self.assertFalse(r[key])

    def test_markers_order_foreign_summary_and_normalization(self):
        good=self.evidence()
        for bad in (good.replace(BEGIN+"\n",""),good+BEGIN+"\n",good.replace(DENIED,"x "+DENIED),good+"CYBERMANJU_OTHER:x\n",good.replace(COMPLETE+"\n","test result: ok. 1 passed; 0 failed; 0 filtered out.\n"+COMPLETE+"\n"),good+"test result: ok. 1 passed; 0 failed; 0 filtered out.\n"):
            with self.assertRaises(ValueError): verify(bad)
        self.assertEqual(verify("\x1b[32m"+good.replace("\n","\r\n")+"\x1b[0m")["status"],"passed")

    def test_claims_and_panics_rejected(self):
        good=self.evidence()
        for old,new in (("cpus=2","cpus=1"),("attempts=4","attempts=3"),("reclaims=4","reclaims=3"),("elf_handoff=false","elf_handoff=true"),("physical_wx=false","physical_wx=true"),("smp_mapping=false","smp_mapping=true")):
            with self.subTest(old=old),self.assertRaises(ValueError): verify(good.replace(old,new))
        for bad in (good+"kernel panicked\n",good+"assertion failed\n",good.replace("1 passed","1 failed")):
            with self.assertRaises(ValueError): verify(bad)

    def test_cli_invalidates_stale_result_on_invalid_and_missing_log(self):
        script=Path(__file__).with_name("verify_elf_lease_smp_runtime.py")
        with tempfile.TemporaryDirectory() as d:
            log=Path(d)/"runtime.log"; out=Path(d)/"result.json"; log.write_text(self.evidence())
            p=subprocess.run([sys.executable,str(script),str(log)],capture_output=True,text=True); self.assertEqual(p.returncode,0,p.stderr)
            self.assertEqual(json.loads(out.read_text())["status"],"passed")
            log.write_text("incomplete\n"); p=subprocess.run([sys.executable,str(script),str(log)],capture_output=True,text=True)
            self.assertNotEqual(p.returncode,0); self.assertEqual(json.loads(out.read_text())["status"],"not_verified")
            log.unlink(); p=subprocess.run([sys.executable,str(script),str(log)],capture_output=True,text=True)
            self.assertNotEqual(p.returncode,0); self.assertEqual(json.loads(out.read_text())["status"],"not_verified")

if __name__ == "__main__": unittest.main()
