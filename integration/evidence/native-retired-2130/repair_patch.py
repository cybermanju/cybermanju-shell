#!/usr/bin/env python3
"""Correct patch preimage formatting; do not edit tested kernel sources."""
from pathlib import Path
import difflib,hashlib,json,re,shutil,subprocess,tempfile
R=Path('/workspace/asterinas-gpui');E=R/'evidence/native-retired-2130';P=R/'local-prs/asterinas-msix-retired-activation.patch';S=R/'asterinas'
initial=E/'initial-formatted-preimage.patch'
if not initial.exists():shutil.copy2(P,initial)
paths=['kernel/core/comps/pci/src/msix_transaction.rs','kernel/core/comps/pci/src/msix_tests.rs']
with tempfile.TemporaryDirectory(prefix='msix-canonical-preimage-') as directory:
    base=Path(directory)
    for name in paths:
        dest=base/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(S/name,dest)
    subprocess.run(['git','apply','--reverse',str(initial)],cwd=base,check=True)
    target=base/paths[0];text=target.read_text()
    pattern=r'if self\.closed\s*\|\| self\.activated\s*\|\| self\.irqs\.iter\(\)\.any\(IrqLine::is_empty\)\s*\{'
    text,count=re.subn(pattern,'if self.closed || self.activated || self.irqs.iter().any(IrqLine::is_empty) {',text,count=1)
    assert count==1
    target.write_text(text)
    chunks=[]
    for name in paths:
        chunks.append(f'diff --git a/{name} b/{name}\n')
        chunks.extend(difflib.unified_diff((base/name).read_text().splitlines(True),(S/name).read_text().splitlines(True),fromfile='a/'+name,tofile='b/'+name))
    P.write_text(''.join(chunks))
    subprocess.run(['git','apply','--check',str(P)],cwd=base,check=True)
    subprocess.run(['git','apply',str(P)],cwd=base,check=True)
    for name in paths:assert (base/name).read_bytes()==(S/name).read_bytes()
result={'corrected_patch':str(P),'sha256':hashlib.sha256(P.read_bytes()).hexdigest(),'bytes':P.stat().st_size,'tested_kernel_sources_changed':False,'reason':'Original reconstructed baseline retained multiline predicate formatting; canonical patch20 predicate is single-line. Initial artifact is retained, not canonical.'}
(E/'patch-preimage-repair.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
