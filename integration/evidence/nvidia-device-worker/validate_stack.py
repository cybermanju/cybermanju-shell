#!/usr/bin/env python3
"""Apply all local patches to archived HEAD without commits, worktrees, or authentication."""
from pathlib import Path
import difflib
import hashlib
import json
import subprocess
import tempfile

ROOT=Path('/workspace/asterinas-gpui')
REPO=ROOT/'asterinas'
OUT=ROOT/'evidence/nvidia-device-worker'
PATCHES=[
    'asterinas-dumb-buffer-layout.patch',
    'asterinas-simpledrm-exact-pixel-format.patch',
    'asterinas-graphics-prerequisite-check.patch',
    'asterinas-vulkan-compute-ci.patch',
    'asterinas-nvidia-nvport-native-ci.patch',
    'asterinas-nvidia-boot-pci-tcg.patch',
    'asterinas-nvidia-rm-os-services.patch',
    'asterinas-nvidia-resource-ownership.patch',
    'asterinas-nvidia-device-dma.patch',
    'asterinas-nvidia-isolated-vtd.patch',
    'asterinas-nvidia-pcie-topology.patch',
    'asterinas-nvidia-multifunction.patch',
    'asterinas-nvidia-irq-callbacks.patch',
    'asterinas-nvidia-msix-transaction.patch',
    'asterinas-irq-retire-storage.patch',
    'asterinas-nvidia-device-worker.patch',
]

def main():
    base=subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO).decode().strip()
    paths,results=set(),[]
    with tempfile.TemporaryDirectory(prefix='aster-multifunction-stack-') as directory:
        temp=Path(directory)
        archive=temp/'base.tar'
        target=temp/'source'
        target.mkdir()
        subprocess.run(['git','archive','--format=tar','--output',str(archive),base],cwd=REPO,check=True)
        subprocess.run(['tar','-xf',str(archive),'-C',str(target)],check=True)
        for name in PATCHES:
            patch=ROOT/'local-prs'/name
            subprocess.run(['git','apply','--check',str(patch)],cwd=target,check=True)
            subprocess.run(['git','apply',str(patch)],cwd=target,check=True)
            stat=subprocess.check_output(['git','apply','--numstat',str(patch)],cwd=target).decode()
            paths.update(line.split('\t',2)[2] for line in stat.splitlines())
            results.append(dict(name=name,sha256=hashlib.sha256(patch.read_bytes()).hexdigest(),applied=True))
        mismatches=[name for name in sorted(paths) if (target/name).read_bytes()!=(REPO/name).read_bytes()]
        for name in mismatches:
            diff=''.join(difflib.unified_diff((target/name).read_text().splitlines(True),(REPO/name).read_text().splitlines(True),fromfile='patched/'+name,tofile='worktree/'+name))
            (OUT/('stack-mismatch-'+name.replace('/','_')+'.diff')).write_text(diff)
        known='kernel/comps/drm/src/ioctl/gem.rs'
        known_preexisting=[]
        if known in mismatches and (target/known).read_text().replace('use super::{dumb_buffer_layout, PAGE_SIZE};','use super::{PAGE_SIZE, dumb_buffer_layout};')==(REPO/known).read_text():
            known_preexisting.append(known)
        unexpected=[name for name in mismatches if name not in known_preexisting]
        if unexpected:
            raise RuntimeError('Unexpected byte mismatch: '+', '.join(unexpected))
    if subprocess.check_output(['git','ls-files','-s'],cwd=REPO)!=(OUT/'index-before.txt').read_bytes():
        raise RuntimeError('Git index changed')
    result=dict(base=base,patches=results,files_compared=len(paths),byte_equal_files=len(paths)-len(mismatches),source_byte_equality=not mismatches,known_preexisting_import_order_only=known_preexisting,unexpected_mismatches=unexpected,original_index_unchanged=True,commits_created=False,remote_actions_run=False,gpu_acceptance=False)
    (OUT/'full-stack-validation.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))

if __name__=='__main__': main()
