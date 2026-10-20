#!/usr/bin/env python3
"""Validate local incremental changes; never stage, commit or authenticate."""
from pathlib import Path
import importlib.util
import json
import subprocess

ROOT = Path('/workspace/asterinas-gpui')
OUT = ROOT / 'evidence/cybermanju-live-profile'
GPUI = ROOT / 'gpui-shell'
APP = Path('/workspace/cybermanju-os/cybermanju.github.io')


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    helper = load('bridge_patch_checker', ROOT / 'evidence/cybermanju-appearance-bridge/make_bridge_patches.py')
    helper.OUT = OUT
    names = [
        'crates/app/src/args.rs', 'crates/app/src/main.rs',
        'crates/app/src/config/mod.rs', 'crates/app/src/config/appearance.rs',
        'crates/app/src/ipc/service.rs', 'crates/services/src/notification/mod.rs',
        'crates/services/src/watcher.rs', 'scripts/test_appearance_profile.py',
        'docs/src/content/docs/developer/cybermanju-appearance.md',
    ]
    mapping = {p: p for p in names}
    mapping['crates/app/src/config/appearance_live.rs'] = None
    docs = ['docs/ASTERINAS-PERSONALIZATION-BRIDGE.md', 'docs/CYBERMANJU-OS-PLAN.md', 'TASKS.md', 'worklog.md']
    result = {
        'gpui': helper.patch('gpui-cybermanju-live-profile.patch', GPUI, OUT / 'before', mapping),
        'app_docs': helper.patch('cybermanju-live-profile-docs.patch', APP, OUT / 'app-docs-before', {p: p for p in docs}),
    }
    assert subprocess.check_output(['git', 'ls-files', '-s'], cwd=GPUI) == (OUT / 'index-before.txt').read_bytes()
    result['gpui_index_unchanged'] = True
    result['no_dependency_or_lockfile_changes'] = True
    result['scope'] = 'Local explicit file-to-native-theme implementation; not full app transport or target graphics acceptance'
    (OUT / 'patch-validation.json').write_text(json.dumps(result, indent=2) + '\n')

    stack = load('native_stack_checker', ROOT / 'evidence/nvidia-device-worker/validate_stack.py')
    stack.OUT = OUT / 'native-stack'
    stack.OUT.mkdir(exist_ok=True)
    (stack.OUT / 'index-before.txt').write_bytes(subprocess.check_output(['git', 'ls-files', '-s'], cwd=ROOT / 'asterinas'))
    stack.PATCHES += [
        'asterinas-nvidia-worker-flush.patch',
        'asterinas-nvidia-irq-worker-teardown.patch',
        'asterinas-nvidia-interrupt-worker.patch',
    ]
    stack.main()
    print('Source/doc patches replay exactly; native stack validated in order without regenerating predecessors.')


if __name__ == '__main__':
    main()
