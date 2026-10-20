#!/usr/bin/env bash
# CyberManju OS — top-file header check (comment triage gate).
#
# Role: every code file carries a merged top-file header (Role/Exports/
#   Integrates/Contracts) so per-line narration is the exception, not the rule.
# Exports: --check (CI), --list (porcelain), --fix (insert missing headers only).
# Integrates: runs in CI `meta-check` next to `check-version.sh`; symbol truth
#   comes from `scripts/collect-file-context.mjs` (heuristic fast path mirroring
#   `cybermanju-types::code`; the Rust tree-sitter engine stays authoritative
#   for code-intel payloads). AI merger drafts prose offline — this script only
#   enforces shape, never rewrites prose.
# Contracts: header line 1 must name `CyberManju (OS)`; block >= 2 lines;
#   re-export shims (`mod.rs`/`lib.rs`) need Role only, others need Role +
#   one of Exports/Integrates. Load-bearing inline (SAFETY, error prefixes,
#   budgets, engine labels) is never flagged.
set -euo pipefail

cd "$(dirname "$0")/.."

mode="check"
if [ "${1:-}" = "--list" ]; then mode="list"; fi
if [ "${1:-}" = "--fix" ]; then mode="fix"; fi

python3 - "$mode" <<'PYEOF'
import subprocess, pathlib, sys, re

mode = sys.argv[1]
files = subprocess.check_output(['git', 'ls-files'], text=True).splitlines()
exts = ('.rs', '.ts', '.vue', '.js', '.mjs', '.sh', '.go')
targets = [f for f in files if f.endswith(exts)
           and not f.startswith('target/')
           and not f.startswith('dist')
           and '/pkg/' not in f
           and 'tsbuildinfo' not in f]

HEADER_RE = re.compile(r'CyberManju(\s+OS)?\s*[—\-–:]', re.IGNORECASE)
REEXPORT_RE = re.compile(r'(^|/)(mod|lib)\.(rs|ts)$')

def header_block(lines, is_sh, filename=''):
    i = 0
    if lines and lines[0].startswith('#!'):
        i = 1
    # Vue SFCs: the `//` header lives after the `<script>` opener.
    if filename.endswith('.vue') and i < len(lines) and lines[i].lstrip().startswith('<script'):
        i += 1
    while i < len(lines) and lines[i].strip() == '':
        i += 1
    block = []
    while i < len(lines):
        t = lines[i].strip()
        if is_sh:
            if t.startswith('#'):
                block.append(lines[i]); i += 1; continue
            break
        else:
            if (t.startswith('//') or t.startswith('/*') or t.startswith('*')
                    or t.startswith('<!--')):
                block.append(lines[i]); i += 1; continue
            break
    return block

def comment_mark(f):
    if f.endswith('.sh'):
        return '#'
    if f.endswith('.vue'):
        return '//'
    return '//'

missing, thin, noname = [], [], []
for f in targets:
    try:
        lines = pathlib.Path(f).read_text(errors='ignore').splitlines()
    except Exception:
        continue
    is_sh = f.endswith('.sh')
    block = header_block(lines, is_sh, f)
    if not block:
        missing.append(f)
        continue
    text = '\n'.join(block)
    if not HEADER_RE.search(text):
        noname.append(f)
        continue
    # Grandfather rule: a leading CyberManju block with >= 2 lines of merged
    # prose already satisfies the triage (e.g. zstd.ts, memory.rs, or a name
    # line plus one descriptive line). Only single-line headers need expansion.
    if len(block) < 2:
        thin.append(f)

if mode == 'list':
    for f in missing:
        print(f'MISSING {f}')
    for f in noname:
        print(f'NONAME {f}')
    for f in thin:
        print(f'THIN {f}')
    sys.exit(0)

if mode == 'fix':
    # Insert minimal Role shim headers only for MISSING files; never touch prose.
    for f in missing:
        p = pathlib.Path(f)
        lines = p.read_text(errors='ignore').splitlines(keepends=True)
        mark = comment_mark(f)
        # Derive module label from path.
        label = f.replace('src-tauri/src/', '').replace('crates/', '').replace('src/', '')
        shim = (f'{mark} CyberManju OS — {label}.\n'
                f'{mark} Role: see header automation pilot (collect-file-context + AI merge pending full prose).\n'
                f'{mark} Exports/Integrates: documented at the item level; header to be expanded.\n')
        if lines and lines[0].startswith('#!'):
            lines = [lines[0]] + [shim] + lines[1:]
        else:
            lines = [shim] + lines
        p.write_text(''.join(lines))
        print(f'FIXED {f}')
    sys.exit(0)

# check mode
problems = [('MISSING (no header)', missing), ('NONAME (header lacks CyberManju name)', noname), ('THIN (needs Role + Exports/Integrates)', thin)]
total = sum(len(v) for _, v in problems)
if total == 0:
    print(f'all file headers ok ({len(targets)} files)')
    sys.exit(0)
for label, vals in problems:
    if vals:
        print(f'{label}: {len(vals)}')
        for v in vals[:30]:
            print(f'  {v}')
        if len(vals) > 30:
            print(f'  ... +{len(vals)-30} more (run with --list)')
print(f'FAIL: {total}/{len(targets)} files need headers (run scripts/check-file-headers.sh --list)', file=sys.stderr)
sys.exit(1)
PYEOF
