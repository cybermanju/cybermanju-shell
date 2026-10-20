#!/usr/bin/env python3
"""Validate the exact canonical twenty-one-patch stack without commits or authentication."""
from pathlib import Path
import importlib.util,json,subprocess
R=Path('/workspace/asterinas-gpui');O=R/'evidence/native-retired-2130/stack-validation';O.mkdir(parents=True,exist_ok=True)
spec=importlib.util.spec_from_file_location('stack',R/'evidence/nvidia-device-worker/validate_stack.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);m.OUT=O
prior=json.loads((R/'evidence/native-2121/stack-validation/full-stack-validation.json').read_text());m.PATCHES=[x['name'] for x in prior['patches']]+['asterinas-msix-retired-activation.patch']
index=O/'index-before.txt'
if not index.exists():index.write_bytes(subprocess.check_output(['git','ls-files','-s'],cwd=R/'asterinas'))
m.main()
