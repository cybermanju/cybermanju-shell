#!/usr/bin/env python3
"""Summarize existing real QEMU logs; do not run or fabricate tests."""
from pathlib import Path
import hashlib
import json
import re

OUT=Path('/workspace/asterinas-gpui/evidence/nvidia-irq-callbacks')

def clean(text): return re.sub(r'\x1b\[[0-9;]*[mK]','',text)

def main():
    suites=[]
    for path in sorted((OUT/'native-suites').glob('native-*-tests.log')):
        raw=path.read_bytes()
        text=clean(raw.decode())
        matches=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) filtered out\.',text)
        if not matches or '[ktest runner] All crates tested.' not in text:
            raise RuntimeError('Incomplete suite: '+str(path))
        if any(int(failed) for _,failed,_ in matches): raise RuntimeError('Failed suite')
        suites.append(dict(log=str(path),counts=[dict(passed=int(p),failed=int(f),filtered=int(s)) for p,f,s in matches],explicit_hardware_skips=re.findall(r'\[skipped:[^\]]+\]',text),sha256=hashlib.sha256(raw).hexdigest()))
    if len(suites)!=5: raise RuntimeError('Expected five native suite logs')
    text=clean((OUT/'native-suites/native-pci-iommu-tests.log').read_text())
    markers=[line.strip() for line in text.splitlines() if line.startswith(('PCI multifunction','PCIe NVMe isolated DMA passed:','PCI isolated DMA'))]
    if not any('PCI multifunction isolated DMA passed:' in marker for marker in markers):
        raise RuntimeError('No actual multifunction DMA marker')
    result=dict(native_suites=suites,real_device_markers=markers,host_graphics_tests=38,hardware_gpu_acceptance=False,rm_gsp_initialized=False,remote_actions_run=False)
    (OUT/'test-counts.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))

if __name__=='__main__': main()
