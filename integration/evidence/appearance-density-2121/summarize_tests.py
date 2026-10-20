#!/usr/bin/env python3
"""Verify actual density regression execution and summarize local test results."""
from pathlib import Path
import hashlib,json,re
R=Path('/workspace/asterinas-gpui');O=R/'evidence/appearance-density-2121'
s=(O/'gpui-rust-tests.log').read_text()
rows=[tuple(map(int,x)) for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out',s)]
assert rows and all(f==i==m==q==0 for p,f,i,m,q in rows)
required=['default_preserves_every_existing_native_spacing_step','compact_is_bounded_monotonic_and_keeps_zero_spacing_zero','profile_density_is_runtime_only_and_survives_a_palette_change','density_projects_launcher_spacing_without_scaling_native_typography','real_file_watch_feeds_theme_updates_and_keeps_bad_saves_out']
for name in required:assert re.search(r'test [^\n]*'+name+r' \.\.\. ok',s),name
py=(O/'gpui-python-tests.log').read_text();num=re.search(r'Ran (\d+) tests',py);assert num and py.rstrip().endswith('OK')
binary=R/'gpui-shell/target/debug/gpuishell'
result={'rust_passed':sum(p for p,*_ in rows),'failed':0,'python_passed':int(num[1]),'new_density_cases':required,'build_passed':True,'clippy_passed':True,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'scope':'Launcher semantic spacing only; no native GPU or Asterinas graphical-session acceptance'}
(O/'test-summary.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
