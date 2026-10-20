#!/usr/bin/env python3
"""Finalize the real retired-vector regression evidence and continuation state."""
from pathlib import Path
import datetime,glob,hashlib,json,re,shutil,subprocess
R=Path('/workspace/asterinas-gpui');O=R/'evidence/native-retired-2130';S=R/'asterinas';G=R/'gpui-shell';A=Path('/workspace/cybermanju-os/cybermanju.github.io')
rows={};skips=[]
for path in sorted((O/'native-suites').glob('*.log')):
    text=re.sub(r'\x1b\[[0-9;]*[mK]','',path.read_text())
    assert '[ktest runner] All crates tested.' in text,path
    m=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) filtered out',text)
    assert len(m)==1,(path,m)
    passed,failed,filtered=map(int,m[0]);assert passed and failed==filtered==0
    rows[path.name]=passed;skips += [line for line in text.splitlines() if '[skipped:' in line]
assert len(rows)==5 and sum(rows.values())==315 and len(skips)==3
red=(O/'negative-old-activation.log').read_text();assert '12 passed; 1 failed' in red and 'Err(AccessDenied)' in red
summary={'suites_reported_passed':rows,'reported_passed':sum(rows.values()),'failed':0,'explicit_skips':skips,'negative_test_summary':'12 passed, 1 failed with old activation predicate','negative_outer_cargo_exit':'not reliably captured; do not claim a measured process exit','real_qemu_nvme_error_and_retry_path':True,'nvidia_hardware_exercised':False,'physical_gpu_acceptance':False}
(O/'test-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
patch=R/'local-prs/asterinas-msix-retired-activation.patch';digest=hashlib.sha256(patch.read_bytes()).hexdigest()
stack=json.loads((O/'stack-validation/full-stack-validation.json').read_text());assert len(stack['patches'])==21 and not stack['unexpected_mismatches'] and stack['original_index_unchanged']
assert stack['patches'][-1]['sha256']==digest
report=O/'REPORT.md';heading='## Canonical preimage correction and ordered-stack verification'
if heading not in report.read_text():
    with report.open('a') as f:f.write('\n\n'+heading+'\n\nThe first patch replayed against a reconstructed, multiline-formatted baseline, but failed against canonical patch20, whose old predicate is single-line. Only patch metadata/preimage formatting was corrected; no tested kernel source changed. The initial artifact remains `initial-formatted-preimage.patch`, and `repair_patch.py` reproduces the correction. Use the canonical patch at '+str(patch)+'; current SHA-256 **'+digest+'** ('+str(patch.stat().st_size)+' bytes), superseding the initial digest above. All21 patches now apply in order. The sole final-source difference is the previously documented DRM import-order formatting; no unexpected mismatch or index change. Fixed native matrix results remain valid because tested source bytes were unchanged.\n')
notes=R/'local-prs/PR_NOTES.md';heading='## 2026-10-20 21:30 UTC continuation — reject activation after IRQ retirement'
entry='''

'''+heading+'''

**Implemented native patch21:** `asterinas-msix-retired-activation.patch`, directly after patch20 `asterinas-nvidia-postirq-drain.patch`; preserve all predecessors. A prepared but never-activated MSI-X transaction can retire every callback and then fail its fallible deferred drain. Its old activation predicate accepted the now-empty IRQ set because the “any empty handler” iteration was vacuously false. `PciMsixGuard::activate` now fails closed with `AccessDenied` when no owned vectors remain, retaining the masked transaction for a real shutdown retry rather than reporting successful activation.

**Genuine regression:** the dedicated idle QEMU NVMe controller verifies RDY=0 and PCI BME-off in quiesce callbacks. The test registers an actual IRQ capture, injects a drain error, verifies capture retirement, zero live vectors and enabled/function-masked MSI-X, requires activation denial, then retries shutdown and verifies exact PCI/MSI-X restoration. With the old predicate, the actual KTest run reported **12 passed / 1 failed**, observing `Ok(())` instead of `Err(AccessDenied)`; its outer Cargo exit was not reliably captured, so do not invent that status. Fixed source passed the actual five-suite QEMU/OSTD matrix: **315 reported passed, zero failures**, with **three explicit VMX/VMX-EPT skips**, and final runner markers in every suite. This is a real PCI lifecycle/error-path test, not NVIDIA hardware or GPU benchmark success.

`evidence/native-retired-2130/` holds the negative log/status, fixed native suite logs, source report, corrected patch preimage metadata, ordered-stack results and final hashes. An initial artifact reconstructed the old predicate in multiline formatting and therefore failed canonical ordered replay; it is preserved as `initial-formatted-preimage.patch`, NOT the canonical deliverable. Only patch context metadata was repaired; the tested source did not change. **All21 canonical patches now apply in order**, with no unexpected source mismatch or Git index change; the one old DRM import-order difference remains documented.

The completed 21:21 GPUI density source remains unchanged: **205 Rust + 36 Python** tests, real binary build and all-target Clippy passed. This scheduled continuation changed no GPUI/app source. Native launcher semantic spacing is only a partial density bridge; new rendered-density, frontend, Asterinas runtime and GPU tests were not performed. Full app automatic transport, wallpaper/media, shell style/docking/accessibility, native AMD/NVIDIA RM/GSP/Vulkan and Asterinas Wayland/Tauri/GPUI remain open.

Hardware still exposes no `/dev/dri`, `/dev/nvidia*` or `/dev/kvm`; no physical-GPU benchmark has passed and Linux parity/full GPU integration is NOT accepted. Keep monitoring scarce shared disk capacity before compiling: only idle reproducible caches may be removed, never sources/evidence/binaries or unrelated Android artifacts. No credentials/authentication, staging, commits, push/PR/publication or remote Actions were used. The published `/workspace/cybermanju-shell` remains an unchanged initial snapshot. Continue from patch21, not either preserved initial artifact.
'''
if heading not in notes.read_text():
    with notes.open('a') as f:f.write(entry)
# Cross-check final code/artifact and prior successful GPUI binary without rerunning completed checks.
gpui=json.loads((R/'evidence/appearance-density-2121/test-summary.json').read_text());assert gpui['rust_passed']==205 and gpui['python_passed']==36
assert hashlib.sha256((G/'target/debug/gpuishell').read_bytes()).hexdigest()==gpui['binary_sha256']
for repo in [S,G,A]:subprocess.run(['git','diff','--check'],cwd=repo,check=True)
for repo,previous in [(G,R/'evidence/appearance-density-2121/index-before.txt'),(A,R/'evidence/appearance-density-2121/app-index-before.txt')]:assert subprocess.check_output(['git','ls-files','-s'],cwd=repo)==previous.read_bytes()
nodes={kind:glob.glob('/dev/'+kind+'*') for kind in ['dri','nvidia','kvm']}
files=[p for p in O.rglob('*') if p.is_file() and p.name!='delivery-manifest.json']+[notes,patch,R/'local-prs/asterinas-nvidia-postirq-drain.patch',R/'evidence/appearance-density-2121/gpui-cybermanju-launcher-density.patch',R/'evidence/appearance-density-2121/test-summary.json',G/'crates/ui/src/theme/density.rs',A/'docs/ASTERINAS-PERSONALIZATION-BRIDGE.md',A/'worklog.md']
manifest={'completed_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'native_reported_passed':315,'native_explicit_skips':3,'prior_gpui_rust_passed':205,'prior_python_passed':36,'canonical_stack_count':21,'unexpected_stack_mismatches':stack['unexpected_mismatches'],'whole_gpu_or_app_integration':False,'physical_gpu_acceptance':False,'device_nodes':nodes,'free_workspace_bytes':shutil.disk_usage(R).free,'older_manifests_are_checkpoint_snapshots':True,'files':[{'path':str(p),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in files]}
assert notes.stat().st_size>0 and patch.stat().st_size>0
(O/'delivery-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps({'notes':str(notes),'canonical_patch':str(patch),'patch_sha256':digest,'native_reported_passed':315,'explicit_skips':3,'canonical_stack_count':21,'physical_gpu_acceptance':False,'manifest':str(O/'delivery-manifest.json')},indent=2))
