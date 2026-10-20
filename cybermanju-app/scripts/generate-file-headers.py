#!/usr/bin/env python3
# CyberManju OS — file header generator (comment-triage bulk pass).
# Role: write a merged top-file header per file from heuristic context
#   (symbols + imports + contract signals), so knowledge moves out of
#   per-line narration into one stable block.
# Exports: --dry-run (print), --apply (rewrite), [file...] (default: all flagged).
# Integrates: reads git ls-files; symbol/import extraction mirrors
#   scripts/collect-file-context.mjs heuristics; authoritative structure stays
#   with the Rust tree-sitter engine in CI code-intel.
# Contracts: never deletes code or load-bearing inline (SAFETY, error prefixes,
#   budgets, engine labels); only inserts/prepends/expands the top block.

import re
import subprocess
import sys
import pathlib

DIR_ROLES = [
    ('crates/agent', 'agent loop, permissions, memory + provider failover'),
    ('crates/os/', 'desktop OS surface: shell, tasks, compute dispatch'),
    ('crates/os-wasm', 'browser volume backend (OPFS/localStorage) for Pages'),
    ('crates/disk', 'virtual disk: superblock, allocator, catalog, volumes'),
    ('crates/erasure', 'Reed-Solomon erasure coding (k+m striped placement)'),
    ('crates/web', 'REST dashboard (:3456): routes, auth, security headers'),
    ('crates/sync', 'sync pipeline: scan→compress→encrypt→upload→verify'),
    ('crates/db', 'redb store (cybermanju.db): tables + sessions/memories'),
    ('crates/types', 'shared schemas: files, sync, agent, schedules'),
    ('crates/crypto', 'ChaCha20Poly1305 + ML-KEM/DSS + Argon2id + BLAKE3'),
    ('crates/compression', 'LZ4→ZSTD-15→Brotli-11 triple cascade + .cyb3'),
    ('crates/search', 'Tantivy BM25 full-text index + faceted search'),
    ('crates/faces', 'face detection (onnx/heuristic-v2/none, engine-labeled)'),
    ('crates/scene', 'scene classifier: 17 categories x 10 langs, BM25-lite'),
    ('src-tauri/src/commands', 'Tauri IPC commands (19 modules, 130 handlers)'),
    ('src-tauri/src/tree_sitter', 'tree-sitter code intel (6 grammars + heuristic)'),
    ('src-tauri/src', 'Tauri desktop shell: lib, state, dashboard bridge'),
    ('docker/server', 'Docker/ZimaOS server entrypoint + REST mount'),
    ('src/composables', 'Vue shared state: transports, wasm backend, vault'),
    ('src/components', 'Vue desktop UI: windows, panels, overlays'),
    ('src/components/ui', 'Vue UI primitives'),
    ('src/utils', 'pure TS helpers (unit-tested, transport-agnostic)'),
    ('src/stores', 'Pinia app store'),
    ('src/ui', 'shell theme/layout tokens'),
    ('cli/cmd', 'cybsh CLI command'),
    ('cli/internal/client', 'CLI REST client'),
    ('cli/internal/config', 'CLI config'),
    ('cli/internal/supa', 'CLI Supabase helper'),
    ('cli/internal/ui', 'CLI terminal UI'),
    ('cli/internal/util', 'CLI utilities'),
    ('crates/tests', 'workspace integration tests'),
    ('tests/frontend', 'Vitest frontend suite'),
    ('scripts', 'ops script (CI/local runbook)'),
]

CONTRACT_RES = [
    'SAFETY', 'INVARIANT', 'integrity:', 'auth:', 'rate_limited:',
    'not_found:', 'unsupported:', 'too_large:', 'conflict:', 'disk_full:',
    'network:', 'engine', 'AGENT-', 'MEMORY_RECALL_BUDGET', 'BUDGET',
    'STATIC_WRITE_LIMIT', 'MAX_TS_',
]

SYMBOL_RES = [
    (re.compile(r'^\s*pub\s+(?:async\s+)?fn\s+([A-Za-z0-9_]+)'), None),
    (re.compile(r'^\s*pub\s+struct\s+([A-Za-z0-9_]+)'), None),
    (re.compile(r'^\s*pub\s+enum\s+([A-Za-z0-9_]+)'), None),
    (re.compile(r'^\s*pub\s+trait\s+([A-Za-z0-9_]+)'), None),
    (re.compile(r'^\s*pub\s+mod\s+([A-Za-z0-9_]+)'), None),
    (re.compile(r'^\s*pub\s+(?:const|static)\s+([A-Za-z0-9_]+)'), None),
    (re.compile(r'^\s*export\s+(?:async\s+)?function\s+([A-Za-z0-9_]+)'), None),
    (re.compile(r'^\s*export\s+(?:default\s+)?(?:class|interface|type|const|enum)\s+([A-Za-z0-9_]+)'), None),
    (re.compile(r'^\s*export\s+const\s+([A-Za-z0-9_]+)'), None),
    (re.compile(r'^\s*func\s+(?:\([^)]*\)\s*)?([A-Z][A-Za-z0-9_]*)'), None),
    (re.compile(r'^\s*type\s+([A-Za-z0-9_]+)\s+(?:struct|interface)'), None),
    (re.compile(r'^\s*([a-zA-Z0-9_:.~-]+)\(\)\s*\{?'), 'sh'),
]

IMPORT_RES = [
    re.compile(r'^\s*use\s+([^;]+);'),
    re.compile(r'''^\s*import\s+(?:.*?\s+from\s+)?['"]([^'"]+)['"]'''),
    re.compile(r'^\s*from\s+(\S+)\s+import\s+'),
]


def role_for(path):
    for prefix, role in DIR_ROLES:
        if path.startswith(prefix):
            return role
    if path.endswith('.vue'):
        return 'Vue view/component (desktop shell UI)'
    if path.endswith('.sh'):
        return 'ops script (CI/local runbook)'
    return 'module (see exports below)'


def mod_label(path):
    base = pathlib.Path(path).stem
    # turn file-stem into a readable module name
    name = re.sub(r'[-_]+', ' ', base).strip()
    return name[:1].upper() + name[1:] if name else base


def analyze(path, text):
    lines = text.splitlines()
    symbols, imports, contracts = [], [], []
    for line in lines:
        # Go parenthesized import blocks: `"github.com/x/y"` / stdlib names.
        if path.endswith('.go'):
            m = re.match(r'^\s*(?:[\w.]+\s+)?\"([^\"]+)\"', line)
            if m:
                imports.append(m.group(1).strip())
        for rx, only in SYMBOL_RES:
            if only == 'sh' and not path.endswith('.sh'):
                continue
            m = rx.match(line)
            if m:
                symbols.append(m.group(1))
                break
        for rx in IMPORT_RES:
            m = rx.match(line)
            if m:
                imports.append(m.group(1).strip())
                break
        s = line.strip()
        if s.startswith('//') or s.startswith('#') or s.startswith('*'):
            for sig in CONTRACT_RES:
                if sig in line:
                    contracts.append(sig.rstrip(':'))
                    break
    # dedupe, keep order
    def dedup(seq):
        seen, out = set(), []
        for x in seq:
            if x not in seen:
                seen.add(x)
                out.append(x)
        return out
    return dedup(symbols), dedup(imports), dedup(contracts)


def short_imports(imports):
    out = []
    for imp in imports:
        imp = imp.strip()
        if imp.split('::')[0].split()[0] in ('crate', 'super', 'self', 'std'):
            continue
        if imp.startswith('cybermanju'):
            out.append(imp.split('::')[0].split()[0])
        elif imp.startswith('@/') or imp.startswith('./') or imp.startswith('../'):
            out.append(imp.split('/')[0] + '/…')
        elif imp.startswith('.'):
            out.append(imp)
        elif '/' in imp and not imp.startswith(('crate', 'super', 'self')):
            out.append(imp.split('/')[0])
        else:
            out.append(imp.split('::')[0].split()[0])
    seen, res = set(), []
    for x in out:
        if x and x not in seen:
            seen.add(x)
            res.append(x)
    return res[:6]


def build_header(path, text):
    symbols, imports, contracts = analyze(path, text)
    role = role_for(path)
    label = mod_label(path)
    exp = ', '.join(symbols[:8]) if symbols else 'module root (see items below)'
    integ = ', '.join(short_imports(imports)) if imports else 'standalone (no imports)'
    if contracts:
        cont = 'signals: ' + ', '.join(contracts[:5])
    else:
        cont = 'none special (pure logic; errors bubble to caller)'
    if path.endswith('.sh'):
        m = '#'
    else:
        m = '//'
    lines = [
        f'{m} CyberManju OS — {label} ({path}).',
        f'{m} Role: {role}.',
        f'{m} Exports: {exp}.',
        f'{m} Integrates: {integ}.',
        f'{m} Contracts: {cont}.',
    ]
    return '\n'.join(lines) + '\n', symbols, imports


def leading_block(lines, is_sh, filename=''):
    i = 0
    if lines and lines[0].startswith('#!'):
        i = 1
    # Vue SFCs: the `//` header lives after the `<script>` opener.
    if filename.endswith('.vue') and i < len(lines) and lines[i].lstrip().startswith('<script'):
        i += 1
    while i < len(lines) and lines[i].strip() == '':
        i += 1
    start = i
    while i < len(lines):
        t = lines[i].strip()
        if is_sh:
            if t.startswith('#'):
                i += 1
                continue
            break
        if (t.startswith('//') or t.startswith('/*') or t.startswith('*')
                or t.startswith('<!--')):
            i += 1
            continue
        break
    return start, i


def needs_header(path, text):
    lines = text.splitlines()
    is_sh = path.endswith('.sh')
    s, e = leading_block(lines, is_sh, path)
    block = lines[s:e]
    if not block:
        return 'MISSING'
    joined = '\n'.join(block)
    if not re.search(r'CyberManju(\s+OS)?\s*[—\-–:]', joined, re.IGNORECASE):
        return 'NONAME'
    if len(block) < 2:
        return 'THIN'
    return 'OK'


def apply_header(path, text):
    header, _, _ = build_header(path, text)
    lines = text.splitlines(keepends=True)
    kind = needs_header(path, text)
    is_sh = path.endswith('.sh')
    is_vue_sfc = path.endswith('.vue')

    def split_lines():
        return [ln.rstrip('\n') for ln in lines]

    if kind in ('MISSING', 'NONAME', 'THIN'):
        raw = split_lines()
        if is_vue_sfc and raw and raw[0].strip().startswith('<script'):
            # insert // header after the <script> opener so SFC stays valid
            opener = lines[0]
            body = ''.join(lines[1:])
            return opener + header + body, kind
        if is_vue_sfc and raw and raw[0].strip().startswith('<'):
            html = header.replace('//', '<!--').replace('\n', '\n').rstrip('\n')
            # convert each line to an HTML comment line
            html_lines = []
            for ln in header.splitlines():
                html_lines.append(ln.replace('//', '<!--', 1) + ' -->')
            html_block = '\n'.join(html_lines) + '\n'
            return html_block + ''.join(lines), kind
        if kind == 'THIN':
            # single-line named header: expand in place, keep its line 1.
            raw = split_lines()
            s, e = leading_block(raw, is_sh, path)
            first = raw[s] + '\n'
            rest = header.splitlines()[1:]
            expanded = first + ''.join(ln + '\n' for ln in rest)
            out = lines[:s] + [expanded] + lines[e:]
            return ''.join(out), kind
        s, _ = leading_block(raw, is_sh, path)
        shebang = ''
        rest_start = 0
        if raw and raw[0].startswith('#!'):
            shebang = lines[0]
            rest_start = 1
            # recompute block after shebang using raw list
        s, _ = leading_block(raw, is_sh, path)
        if kind == 'NONAME':
            # keep existing descriptive comment; name line goes first
            name_line = header.splitlines()[0] + '\n'
            head = ([shebang] if shebang else []) + [name_line]
            return ''.join(head) + ''.join(lines[(1 if shebang else 0):]), kind
        # MISSING or THIN: full template block, preserving old block below for THIN
        head = ([shebang] if shebang else []) + [header]
        return ''.join(head) + ''.join(lines[(1 if shebang else 0):]), kind
    return text, 'OK'


def main():
    args = sys.argv[1:]
    apply = '--apply' in args
    files = [a for a in args if not a.startswith('--')]
    if not files:
        tracked = subprocess.check_output(['git', 'ls-files'], text=True).splitlines()
        exts = ('.rs', '.ts', '.vue', '.js', '.mjs', '.sh', '.go')
        files = [f for f in tracked if f.endswith(exts)
                 and not f.startswith('target/') and '/pkg/' not in f
                 and 'tsbuildinfo' not in f
                 and f not in ('scripts/collect-file-context.mjs',
                               'scripts/check-file-headers.sh',
                               'scripts/generate-file-headers.py')]
    changed, kinds = 0, {}
    for f in files:
        p = pathlib.Path(f)
        if not p.exists():
            continue
        text = p.read_text(errors='ignore')
        kind = needs_header(f, text)
        kinds[kind] = kinds.get(kind, 0) + 1
        if kind == 'OK':
            continue
        new_text, _ = apply_header(f, text)
        if new_text != text:
            if apply:
                p.write_text(new_text)
            changed += 1
            if not apply:
                print(f'{kind} {f}')
    print(f'kinds={kinds} would_change={changed} apply={apply}')


if __name__ == '__main__':
    main()
