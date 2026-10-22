import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from verify_elf_lease_runtime import BEGIN, CASE, FAULTS, COMPLETE, verify

class ElfLeaseRuntimeTests(unittest.TestCase):
    def evidence(self):
        cases=[]
        for kind, span, envelope, reloc in ((1,4096,16384,8),(2,8192,16384,4),(4,12288,16384,4),(11,4096,16384,4)):
            pages=span//4096; slack=envelope//4096-pages
            cases.append(f"CYBERMANJU_ELF_LEASE:case:PASS kind={kind} span={span} envelope={envelope} pages={pages} bytes={span+reloc} relocation_bytes={reloc} frame_matches={pages} nonx_pages={pages} slack_absent={slack} retained=true layout_preserved=true reclaimed=true")
        return "\n".join([BEGIN,*cases,FAULTS,COMPLETE,"test result: ok. 1 passed; 0 failed; 2 filtered out.",""])

    def test_metrics_and_capability_limits(self):
        r=verify(self.evidence())
        self.assertEqual(r["mapped_spans"],[4096,8192,12288,4096]); self.assertEqual(r["total_mapped_read_bytes"],28692)
        self.assertEqual(r["relocation_bytes_checked"],20); self.assertEqual(r["frame_identity_pages"],7)
        self.assertTrue(r["elf_handoff_verified"] and r["policy_opt_in_c_shims"])
        for key in ("linux_modules_executed","linux_abi_verified","physical_wx_verified","smp_mapping_verified","production_mapping_ready","tlb_shootdown_verified"): self.assertFalse(r[key])

    def test_markers_order_foreign_and_format(self):
        good=self.evidence()
        for bad in (good.replace(BEGIN+"\n",""),good+BEGIN+"\n",good.replace(FAULTS+"\n","")+FAULTS+"\n",good.replace(BEGIN,"prefix "+BEGIN),good+"CYBERMANJU_OTHER:x\n",good.replace(BEGIN+"\n",FAULTS+"\n"+BEGIN+"\n")):
            with self.subTest():
                with self.assertRaises(ValueError): verify(bad)
        self.assertEqual(verify("\x1b[32m"+good.replace("\n","\r\n")+"\x1b[0m")["status"],"passed")

    def test_geometry_flags_summary_and_failures(self):
        good=self.evidence()
        for old,new in (("span=4096","span=4097"),("envelope=16384","envelope=12288"),("pages=1","pages=2"),("bytes=4104","bytes=4096"),("relocation_bytes=8","relocation_bytes=4"),("frame_matches=1","frame_matches=0"),("nonx_pages=1","nonx_pages=0"),("slack_absent=1","slack_absent=0"),("retained=true","retained=false"),("bounds_rejected=8","bounds_rejected=7"),("1 passed","1 failed")):
            with self.subTest(claim=old),self.assertRaises(ValueError): verify(good.replace(old,new,1))
        for bad in (good+"kernel panicked\n",good+"assertion failed\n",good+"test result: ok. 1 passed; 0 failed; 0 filtered out.\n",good.replace(COMPLETE+"\n","test result: ok. 1 passed; 0 failed; 0 filtered out.\n"+COMPLETE+"\n")):
            with self.assertRaises(ValueError): verify(bad)

    def test_cli_invalidates_stale_result_on_invalid_and_missing_log(self):
        script=Path(__file__).with_name("verify_elf_lease_runtime.py")
        with tempfile.TemporaryDirectory() as d:
            log=Path(d)/"runtime.log"; out=Path(d)/"result.json"; log.write_text(self.evidence())
            p=subprocess.run([sys.executable,str(script),str(log)],capture_output=True,text=True); self.assertEqual(p.returncode,0,p.stderr)
            self.assertEqual(json.loads(out.read_text())["status"],"passed")
            log.write_text("bad\n"); p=subprocess.run([sys.executable,str(script),str(log)],capture_output=True,text=True)
            self.assertNotEqual(p.returncode,0); self.assertEqual(json.loads(out.read_text())["status"],"not_verified")
            log.unlink(); p=subprocess.run([sys.executable,str(script),str(log)],capture_output=True,text=True)
            self.assertNotEqual(p.returncode,0); self.assertEqual(json.loads(out.read_text())["status"],"not_verified")

if __name__ == "__main__": unittest.main()
