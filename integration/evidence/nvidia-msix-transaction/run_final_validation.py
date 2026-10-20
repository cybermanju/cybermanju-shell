#!/usr/bin/env python3
"""Validate the local 14+15 patch stack without commits, pushes or remote Actions."""
from pathlib import Path
import os,subprocess,json
ROOT=Path('/workspace/asterinas-gpui');REPO=ROOT/'asterinas';OUT=ROOT/'evidence/nvidia-msix-transaction'
env=dict(os.environ,PATH=str(ROOT/'toolchain/bin')+':'+os.environ['PATH'],NVIDIA_SOURCE_DIR=str(ROOT/'vendor-drivers/open-gpu-kernel-modules-615.78.08-tag'),VDSO_LIBRARY_DIR=str(ROOT/'vendor-drivers/linux_vdso'),CARGO_NET_OFFLINE='true')
results=[]
def phase(name,command,timeout=1200,extra=None):
    print('PHASE: '+name,flush=True)
    e=env| (extra or {})
    path=OUT/('final-'+name+'.log')
    with path.open('w') as log:
        p=subprocess.run(command,cwd=REPO,env=e,stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT,timeout=timeout)
    results.append(dict(name=name,exit=p.returncode,log=str(path)))
    (OUT/'final-validation-phases.json').write_text(json.dumps(dict(phases=results,gpu_acceptance=False,remote_actions_run=False),indent=2)+'\n')
    if p.returncode:raise RuntimeError(name+' failed; inspect '+str(path))
    print('PASS: '+name,flush=True)

def main():
    phase('native-suites',['bash','tools/graphics/run_cpu_kernel_tests.sh',str(OUT/'native-suites-storage-final')])
    phase('test-counts',['python3',str(OUT/'summarize_tests.py')])
    phase('negative-regressions',['python3',str(OUT/'check_regressions.py')])
    phase('consumer-io',['python3',str(OUT/'consumer-io/run_consumer_io.py')],extra={'CONSUMER_IO_RUN':'run-final'})
    phase('full-cpu-boots',['python3',str(OUT/'run_cpu_boot.py')])
    phase('host-graphics',['python3','-m','unittest','discover','-s','tools/graphics','-v'])
    phase('patch-round-trip',['python3',str(OUT/'make_patch.py')])
    phase('all-patches',['python3',str(OUT/'validate_stack.py')])
    phase('workflow-lint',[str(ROOT/'toolchain/bin/actionlint'),'.github/workflows/graphics_gpu.yml'])
    phase('diff-check',['git','diff','--check'])
    print('All final validation phases passed; GPU acceptance remains false.',flush=True)
if __name__=='__main__':main()
