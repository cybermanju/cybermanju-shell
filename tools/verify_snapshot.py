#!/usr/bin/env python3
"""Verifica os arquivos do snapshot inicial; não valida suporte a hardware."""
import hashlib
import json
import os
from pathlib import Path
import sys

root = Path(__file__).resolve().parents[1]
manifest = json.loads((root / 'integration/snapshot/manifest.json').read_text())
errors = []
for relative, expected in manifest['files'].items():
    path = root / relative
    if expected['kind'] == 'symlink':
        if not path.is_symlink() or os.readlink(path) != expected['target']:
            errors.append(relative)
    elif not path.is_file() or path.is_symlink():
        errors.append(relative)
    elif (hashlib.sha256(path.read_bytes()).hexdigest() != expected['sha256']
          or bool(path.stat().st_mode & 0o111) != expected['executable']):
        errors.append(relative)
if errors:
    print('Arquivos ausentes ou diferentes do snapshot inicial:')
    print('\n'.join(errors))
    sys.exit(1)
print(f"OK: {len(manifest['files'])} arquivos/links do snapshot inicial preservados.")
print('Esta verificação não cobre arquivos adicionais nem certifica integração GPUI/Asterinas.')
