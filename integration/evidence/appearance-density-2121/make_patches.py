#!/usr/bin/env python3
"""Replay the local density delta; never stage or publish."""
from pathlib import Path
import importlib.util,json,subprocess
R=Path('/workspace/asterinas-gpui');O=R/'evidence/appearance-density-2121';G=R/'gpui-shell';A=Path('/workspace/cybermanju-os/cybermanju.github.io')
spec=importlib.util.spec_from_file_location('patch_helper',R/'evidence/cybermanju-appearance-bridge/make_bridge_patches.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);m.OUT=O
names=['crates/ui/src/ui.rs','crates/ui/src/theme/mod.rs','crates/ui/src/patterns/launcher.rs','crates/app/src/config/appearance.rs','crates/app/src/config/appearance_live.rs','docs/src/content/docs/developer/cybermanju-appearance.md']
files={p:p for p in names};files['crates/ui/src/theme/density.rs']=None
v={'gpui':m.patch('gpui-cybermanju-launcher-density.patch',G,O/'before',files),'app_docs':m.patch('cybermanju-launcher-density-docs.patch',A,O/'app-before',{p:p for p in ['docs/ASTERINAS-PERSONALIZATION-BRIDGE.md','worklog.md']})}
assert subprocess.check_output(['git','ls-files','-s'],cwd=G)==(O/'index-before.txt').read_bytes()
assert subprocess.check_output(['git','ls-files','-s'],cwd=A)==(O/'app-index-before.txt').read_bytes()
v.update(indexes_unchanged=True,commits_or_publication=False,physical_gpu_acceptance=False,scope='Launcher semantic spacing only; no frontend source or preference schema changes')
(O/'patch-validation.json').write_text(json.dumps(v,indent=2)+'\n')
print('Incremental patches replay byte-for-byte; both indexes unchanged.')
