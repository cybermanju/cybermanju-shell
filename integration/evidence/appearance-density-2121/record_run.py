#!/usr/bin/env python3
"""Preserve completed 21:21 implementation evidence without altering Git state."""
from pathlib import Path
import hashlib,json,re,subprocess
R=Path('/workspace/asterinas-gpui');O=R/'evidence/appearance-density-2121';N=R/'evidence/native-2121'
rows={};skips=[]
for path in sorted((N/'native-suites').glob('*.log')):
    text=re.sub(r'\x1b\[[0-9;]*[mK]','',path.read_text())
    matches=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) filtered out',text)
    assert len(matches)==1,(path,matches)
    passed,failed,filtered=map(int,matches[0]);assert passed>0 and failed==filtered==0
    rows[path.name]=passed
    skips += [line for line in text.splitlines() if '[skipped:' in line]
assert len(rows)==5 and sum(rows.values())==315 and len(skips)==3
result={'suites_reported_passed':rows,'reported_passed':sum(rows.values()),'failed':0,'explicit_skips':skips,'qemu_nvme_post_irq_boundary_tested':True,'nvidia_device_exercised':False,'physical_gpu_acceptance':False,'completion_job':'job_JqeN9UAm'}
(N/'test-summary.json').write_text(json.dumps(result,indent=2)+'\n')
marker='## Parent validation after safe cache recovery'
report=N/'REPORT.md'
if marker not in report.read_text():
    with report.open('a') as f:f.write('\n\n'+marker+'\n\nAfter GPUI compilation completed, the parent removed only idle reproducible GPUI incremental caches and recovered ~1.2 GiB. With the existing local OSDK/toolchain, offline full native validation then passed all five suites: 315 reported passed, zero failures, three explicit VMX/VMX-EPT skips. Logs are in `native-suites/`, summary in `test-summary.json`. The actual QEMU NVMe cycle verified callback retirement before the drain and MSI-X enabled/function-masked at that boundary. This supersedes the initial disk-blocked status, not its historical record. It remains CPU/QEMU PCI lifecycle evidence, not NVIDIA hardware or GPU acceptance. All 20 canonical patches applied in order; 98/99 affected files match byte-for-byte and the sole difference is the previously documented DRM import-order formatting. No unexpected mismatch or Git index change.\n')
notes=R/'local-prs/PR_NOTES.md';header='## 2026-10-20 21:21 UTC continuation — post-IRQ drain and actual launcher density'
entry='''

'''+header+'''

**Native patch20** is `asterinas-nvidia-postirq-drain.patch`, after patch19 `asterinas-nvidia-interrupt-worker.patch`. PCI MSI-X now supports a fallible post-IRQ drain: actual caller-provided source quiescence → BME/PBA checks → synchronous callback retirement → admitted deferred-work drain while MSI-X remains masked/owned → state restoration. NVIDIA's owner drains its real worker at that boundary, retaining retry semantics. The old `shutdown` API remains compatible. This does not implement NVIDIA register masking, RM/GSP or a hardware GPU probe.

After critical disk pressure initially blocked native validation, only idle GPUI incremental compilation cache entries were removed after its compilers stopped; sources, binaries, test binaries, dependencies, evidence and the unrelated Android build were preserved. Parent then ran the actual OSDK/QEMU TCG matrix against patch20: **315 reported passed**, zero failures, **three explicit VMX/VMX-EPT skips**. QEMU NVMe verified callback retirement and enabled/function-masked MSI-X at the new drain boundary. Evidence: `evidence/native-2121/native-suites/`, `test-summary.json` and the appended REPORT.md. Ordered replay of all20 patches matches 98/99 source files; only the pre-existing DRM import-order difference remains, with no unexpected mismatch/index change. These are PCI/CPU prerequisites, NOT GPU benchmarks.

**Real personalization implementation:** `settings.density` now changes nine actual native launcher query/body/footer semantic spacing consumers. Compact=0.9 and comfortable=1.0 reuse the app's existing unit semantics; default native spacing stays unchanged without import. Density is a runtime Theme overlay: it survives palette/native-theme reload, accepted atomic/inotify saves, invalid-save retention and recovery, but is not persisted into native theme files. Native typography, icon sizes, explicit hit-target dimensions, bar/compositor geometry are not scaled. The report says **launcher spacing only**, not whole-desktop density. No frontend source, schema, dependency or app preference-store changes.

Final GPUI source passed **205 Rust + 36 Python** tests, real binary build and all-target Clippy; app version/header gates passed. Frontend and rendered host-window suites were not rerun: earlier host color captures do not validate new density visually. `evidence/appearance-density-2121/` contains preimages, exact incremental patches, binary hash, tests, source references, safe cache-recovery record and continuation notes. GPUI patch applies on the previous live-file baseline; do not substitute the unchanged published `/workspace/cybermanju-shell` snapshot.

Hardware again has no display-class PCI GPU, `/dev/dri`, `/dev/nvidia*` or `/dev/kvm`. Full native NVIDIA RM/GSP/Vulkan, AMD drivers, Asterinas Wayland/GPUI/Tauri runtime, app automatic transport, wallpapers/media, desktop shell style/docking, full density/accessibility/restart and correct physical-GPU benchmarks remain open. No credentials/authentication, staging, commits, push/PR/publication or remote Actions occurred. The next concrete native edge is activation after an unused transaction's failed post-IRQ drain: it must not accept an empty retired IRQ set; do not infer that the successful patch20 matrix covers this new retry case.
'''
if header not in notes.read_text():
    with notes.open('a') as f:f.write(entry)
A=Path('/workspace/cybermanju-os/cybermanju.github.io')
old=(O/'app-before/worklog.md').read_bytes();now=(A/'worklog.md').read_bytes()
assert now.startswith(old),'Prior worklog history changed rather than being appended'
subprocess.run(['python3',str(O/'make_patches.py')],check=True)
files=[p for p in O.rglob('*') if p.is_file() and p.name!='delivery-manifest.json']+[notes,R/'local-prs/asterinas-nvidia-postirq-drain.patch',N/'test-summary.json',N/'REPORT.md']
manifest={'gpui_rust_passed':205,'python_passed':36,'native_reported_passed':315,'explicit_native_skips':3,'native_gpu_acceptance':False,'files':[{'path':str(p),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in files]}
assert all(x['bytes']>0 for x in manifest['files'])
(O/'delivery-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps({'rust':205,'python':36,'native_reported':315,'native_skips':3,'notes':str(notes),'manifest':str(O/'delivery-manifest.json')},indent=2))
