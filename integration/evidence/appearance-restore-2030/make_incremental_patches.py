#!/usr/bin/env python3
"""Reuse the verified patch checker without regenerating any predecessor artifact."""
from pathlib import Path
import importlib.util
import json
import subprocess

ROOT = Path('/workspace/asterinas-gpui')
OUT = ROOT / 'evidence/appearance-restore-2030'
APP = Path('/workspace/cybermanju-os/cybermanju.github.io')
helper = ROOT / 'evidence/cybermanju-appearance-bridge/make_bridge_patches.py'
spec = importlib.util.spec_from_file_location('previous_patch_helper', helper)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.OUT = OUT


def main():
    gpui_names = [
        'Cargo.lock', 'crates/app/Cargo.toml', 'crates/app/src/config/appearance.rs',
        'scripts/test_appearance_profile.py',
        'docs/src/content/docs/developer/cybermanju-appearance.md',
    ]
    app_files = {
        'src/utils/appearanceProfile.ts': 'appearanceProfile.ts',
        'src/components/SettingsPage.vue': 'SettingsPage.vue',
        'tests/frontend/appearance-profile.test.ts': 'appearance-profile.test.ts',
    }
    doc_names = ['docs/ASTERINAS-PERSONALIZATION-BRIDGE.md', 'docs/CYBERMANJU-OS-PLAN.md', 'TASKS.md', 'worklog.md']
    result = {
        'gpui': module.patch('gpui-appearance-file-admission.patch', ROOT / 'gpui-shell', OUT / 'gpui-before', {p: p for p in gpui_names}),
        'app': module.patch('cybermanju-appearance-restore.patch', APP, OUT / 'app-before', app_files),
        'app_docs': module.patch('cybermanju-appearance-restore-docs.patch', APP, OUT / 'docs-before', {p: p for p in doc_names}),
        'app_baseline_provenance': json.loads((OUT / 'app-before/provenance.json').read_text()),
    }
    before = (OUT / 'gpui-index-before.txt').read_bytes()
    after = subprocess.check_output(['git', 'ls-files', '-s'], cwd=ROOT / 'gpui-shell')
    assert before == after, 'GPUI index was changed'
    result['gpui_index_unchanged'] = True
    a = APP / 'tests/fixtures/appearance-profile-v1.json'
    b = ROOT / 'gpui-shell/crates/app/src/config/appearance/fixtures/appearance-profile-v1.json'
    assert a.read_bytes() == b.read_bytes(), 'Shared v1 fixtures diverged'
    result['shared_fixture_sha256'] = module.sha(a.read_bytes())
    result['scope'] = 'Incremental local source/doc changes after the 20:00 UTC pass; no hardware acceptance or publication'
    (OUT / 'patch-validation.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: result[k]['patch'] for k in ['gpui', 'app', 'app_docs']}, indent=2))


if __name__ == '__main__':
    main()
