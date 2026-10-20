#!/usr/bin/env python3
"""Append continuation state and hash small deliverables; no deletion/build/test."""
from pathlib import Path
import datetime,hashlib,json,re,shutil
R=Path('/workspace/asterinas-gpui');O=R/'evidence/list-density-lowdisk-2139';N=R/'local-prs/PR_NOTES.md'
header='## 2026-10-20 21:39 UTC continuation — low-disk source-only shared rows'
body=(O/'continuation-notes.md').read_text().split('\n',1)[1];body=re.sub(r'^## ','### ',body,flags=re.MULTILINE)
entry='\n\n'+header+'\n\n**OWNER RESTRICTION: no additional cache/file deletions without explicit approval; no disk-heavy build/ISO attempt at current capacity. The existing NixOS image reportedly reserves 6.44 GB. Source-only work and small consistency checks only.**\n'+body
if header not in N.read_text():
    with N.open('a') as f:f.write(entry)
validation=json.loads((O/'patch-validation.json').read_text());assert validation['compiled'] is False and validation['rust_tests_executed']==0 and validation['no_deletions']
assert len(validation['patches'])==2 and all(x['reverse_check'] and x['git_index_unchanged'] for x in validation['patches'])
for data in validation['patches']:
    p=Path(data['patch']);assert p.is_file() and p.stat().st_size==data['bytes'] and hashlib.sha256(p.read_bytes()).hexdigest()==data['sha256']
# Stream an existing binary read-only; never execute stale tests as new-source evidence.
binary=R/'gpui-shell/target/debug/gpuishell';digest=hashlib.sha256()
with binary.open('rb') as f:
    for block in iter(lambda:f.read(1024*1024),b''):digest.update(block)
old=json.loads((R/'evidence/appearance-density-2121/test-summary.json').read_text());assert digest.hexdigest()==old['binary_sha256']
files=[p for p in O.rglob('*') if p.is_file() and p.name!='delivery-manifest.json']+[N,R/'evidence/native-lowdisk-2139/REVIEW.md',R/'gpui-shell/crates/ui/src/components/list.rs',R/'gpui-shell/crates/app/src/config/appearance.rs',R/'gpui-shell/crates/ui/src/theme/mod.rs']
manifest={'completed_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'native_canonical_patch_count':21,'new_native_patch_created':False,'new_source_compiled':False,'new_tests_executed':0,'new_rust_tests_unexecuted':3,'built_gpui_binary_unchanged':True,'built_binary_sha256':digest.hexdigest(),'no_deletions_or_cleanup':True,'no_build_or_iso':True,'physical_gpu_acceptance':False,'free_bytes':shutil.disk_usage(R).free,'files':[{'path':str(p),'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in files]}
(O/'delivery-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
for p in [N,O/'gpui-shared-list-density-source-only.patch',O/'cybermanju-shared-list-density-contract.patch',O/'delivery-manifest.json']:
    assert p.is_file() and p.stat().st_size>0;print('VERIFIED',p)
print('No builds, tests or deletions; new source uncompiled; prior binary unchanged; native order still21.')
