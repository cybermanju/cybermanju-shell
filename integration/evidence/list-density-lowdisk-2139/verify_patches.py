#!/usr/bin/env python3
"""Small source patches only: memory replay + read-only Git checks, no cleanup/builds."""
from pathlib import Path
import difflib,hashlib,json,re,shutil,subprocess
R=Path('/workspace/asterinas-gpui');O=R/'evidence/list-density-lowdisk-2139';G=R/'gpui-shell';A=Path('/workspace/cybermanju-os/cybermanju.github.io')

def replay(before, delta):
    source=before.splitlines(True);output=[];cursor=0;i=2
    while i<len(delta):
        match=re.fullmatch(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@[^\n]*\n',delta[i]);assert match,delta[i]
        start=max(0,int(match[1])-1);assert start>=cursor
        output.extend(source[cursor:start]);cursor=start;old_seen=new_seen=0;i+=1
        while i<len(delta) and not delta[i].startswith('@@ '):
            line=delta[i];assert line[0] in ' +-'
            if line[0] in ' -':
                assert source[cursor]==line[1:],(cursor,source[cursor],line)
                cursor+=1;old_seen+=1
            if line[0] in ' +':output.append(line[1:]);new_seen+=1
            i+=1
        assert old_seen==int(match[2] or 1) and new_seen==int(match[4] or 1)
    output.extend(source[cursor:]);return ''.join(output)

def make(repo,before_root,names,filename,index):
    chunks=[];records=[]
    for name in names:
        old=(before_root/name).read_text();new=(repo/name).read_text()
        assert old.endswith('\n') and new.endswith('\n')
        if old==new:continue
        delta=list(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+name,tofile='b/'+name))
        assert replay(old,delta)==new
        chunks.append('diff --git a/'+name+' b/'+name+'\n');chunks.extend(delta)
        records.append({'path':name,'old_sha256':hashlib.sha256(old.encode()).hexdigest(),'new_sha256':hashlib.sha256(new.encode()).hexdigest(),'memory_replay_equal':True})
    assert records
    patch=O/filename;patch.write_text(''.join(chunks))
    subprocess.run(['git','apply','--reverse','--check',str(patch)],cwd=repo,check=True)
    subprocess.run(['git','diff','--check'],cwd=repo,check=True)
    assert subprocess.check_output(['git','ls-files','-s'],cwd=repo)==index.read_bytes()
    return {'patch':str(patch),'sha256':hashlib.sha256(patch.read_bytes()).hexdigest(),'bytes':patch.stat().st_size,'files':records,'reverse_check':True,'git_index_unchanged':True}

results=[make(G,O/'before',['crates/ui/src/components/list.rs','crates/ui/src/theme/mod.rs','crates/app/src/config/appearance.rs','docs/src/content/docs/developer/cybermanju-appearance.md'],'gpui-shared-list-density-source-only.patch',O/'gpui-index-before.txt'),make(A,O/'app-before',['docs/ASTERINAS-PERSONALIZATION-BRIDGE.md'],'cybermanju-shared-list-density-contract.patch',O/'app-index-before.txt')]
record={'patches':results,'native_patch_order_unchanged':21,'new_native_patch_created':False,'source_only':True,'compiled':False,'rust_tests_executed':0,'new_rust_tests_not_executed':3,'render_tests_executed':0,'no_deletions':True,'no_cache_cleanup':True,'no_build_or_iso_attempt':True,'physical_gpu_acceptance':False,'free_bytes':shutil.disk_usage(R).free}
(O/'patch-validation.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record,indent=2))
