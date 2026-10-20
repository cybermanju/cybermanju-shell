#!/usr/bin/env python3
"""Build incremental source patches from preserved pre-edit files; never stage or commit."""
from pathlib import Path
import difflib
import hashlib
import json
import subprocess
import tempfile

ROOT = Path('/workspace/asterinas-gpui')
OUT = ROOT / 'evidence/cybermanju-appearance-bridge'
GPUI = ROOT / 'gpui-shell'
APP = Path('/workspace/cybermanju-os/cybermanju.github.io')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def patch(name, root, before, mapping):
    chunks = []
    records = []
    with tempfile.TemporaryDirectory(prefix='cybermanju-profile-patch-') as directory:
        tree = Path(directory)
        for relative, old_relative in mapping.items():
            dest = root / relative
            new = dest.read_bytes()
            old_path = before / old_relative if old_relative is not None else None
            old = old_path.read_bytes() if old_path is not None else None
            if old == new:
                continue
            if old is not None:
                previous = tree / relative
                previous.parent.mkdir(parents=True, exist_ok=True)
                previous.write_bytes(old)
            chunks.append(f'diff --git a/{relative} b/{relative}\n')
            if old is None:
                chunks.append('new file mode 100644\n')
            chunks.extend(difflib.unified_diff(
                old.decode().splitlines(keepends=True) if old is not None else [],
                new.decode().splitlines(keepends=True),
                fromfile=f'a/{relative}' if old is not None else '/dev/null',
                tofile=f'b/{relative}',
            ))
            records.append({'path': relative, 'before_sha256': sha(old) if old is not None else None,
                            'after_sha256': sha(new)})
        target = OUT / name
        target.write_text(''.join(chunks))
        subprocess.run(['git', 'apply', '--check', '--whitespace=error-all', str(target)], cwd=tree, check=True)
        subprocess.run(['git', 'apply', '--whitespace=error-all', str(target)], cwd=tree, check=True)
        for record in records:
            assert (tree / record['path']).read_bytes() == (root / record['path']).read_bytes(), record['path']
    return {'patch': str(target), 'patch_sha256': sha(target.read_bytes()), 'applies_and_matches_final_bytes': True,
            'files': records}


def main():
    gpui_files = {p: p for p in [
        'Cargo.lock', 'crates/app/Cargo.toml', 'crates/app/src/args.rs', 'crates/app/src/main.rs',
        'crates/app/src/config/mod.rs', 'crates/app/src/ipc/service.rs',
    ]}
    gpui_files.update({p: None for p in [
        'crates/app/src/config/appearance.rs',
        'crates/app/src/config/appearance/fixtures/appearance-profile-v1.json',
        'scripts/test_appearance_profile.py',
        'docs/src/content/docs/developer/cybermanju-appearance.md',
    ]})
    app_files = {
        'src/components/SettingsPage.vue': 'SettingsPage.vue',
        'src/composables/useCustomWallpaper.ts': 'useCustomWallpaper.ts',
        'src/utils/appearanceProfile.ts': None,
        'tests/frontend/appearance-profile.test.ts': None,
        'tests/fixtures/appearance-profile-v1.json': None,
    }
    result = {
        'gpui': patch('gpui-cybermanju-appearance.patch', GPUI, OUT / 'gpui-before', gpui_files),
        'app_source': patch('cybermanju-appearance-export.patch', APP, OUT / 'app-before', app_files),
        'app_documentation_not_in_source_patch': [
            'docs/ASTERINAS-PERSONALIZATION-BRIDGE.md', 'docs/CYBERMANJU-OS-PLAN.md', 'TASKS.md', 'worklog.md',
        ],
    }
    a = (APP / 'tests/fixtures/appearance-profile-v1.json').read_bytes()
    b = (GPUI / 'crates/app/src/config/appearance/fixtures/appearance-profile-v1.json').read_bytes()
    assert a == b, 'The shared fixtures diverged'
    result['shared_fixture_sha256'] = sha(a)
    before_index = (OUT / 'gpui-index-before.txt').read_bytes()
    after_index = subprocess.check_output(['git', 'ls-files', '-s'], cwd=GPUI)
    assert before_index == after_index, 'GPUI index changed during the implementation'
    result['gpui_index_unchanged'] = True
    result['scope'] = 'Local source patches only; no native desktop/GPU acceptance and no publication'
    (OUT / 'patch-validation.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: value['patch'] for key, value in result.items() if isinstance(value, dict)}, indent=2))


if __name__ == '__main__':
    main()
