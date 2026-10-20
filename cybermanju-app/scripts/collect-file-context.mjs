#!/usr/bin/env node
// CyberManju OS — File context collector (header automation).
// Role: heuristic symbol+import+comment extractor, one JSON per file for the AI merger.
// Exports: collectFileContext(path) -> {symbols, imports, inlineComments, header}.
// Integrates: mirrors `cybermanju-types::code::extract_symbols_heuristic` fast path
//   (qualifier stripping, Go receivers, bash defs, additive multi-line doc
//   hoisting); the Rust
//   tree-sitter engine (`src-tauri/src/tree_sitter`, real grammars for
//   rust/python/js/ts/go/bash) is authoritative in CI when cargo runs — this
//   script is the cargo-free local/CI-meta equivalent so `meta-check` stays fast.
// Contracts: read-only, never rewrites source; stdout JSON only.

import { readFileSync } from 'node:fs';

// Keyword tables matched AFTER qualifier stripping (mirrors code.rs).
const KEYWORDS = {
  rust: ['fn ', 'mod ', 'struct ', 'enum ', 'trait ', 'type ', 'const ', 'static ', 'impl ', 'macro_rules! '],
  python: ['def ', 'class '],
  typescript: ['function ', 'const ', 'let ', 'var ', 'class ', 'interface ', 'enum ', 'type ', 'namespace '],
  javascript: ['function ', 'const ', 'let ', 'var ', 'class ', 'interface ', 'enum ', 'type ', 'namespace '],
  vue: ['function ', 'const ', 'let ', 'var ', 'class ', 'interface ', 'enum ', 'type ', 'namespace '],
  svelte: ['function ', 'const ', 'let ', 'var ', 'class ', 'interface ', 'enum ', 'type ', 'namespace '],
  go: ['func ', 'type '],
};

function langOf(filePath) {
  if (filePath.endsWith('.rs')) return 'rust';
  if (filePath.endsWith('.py')) return 'python';
  if (filePath.endsWith('.go')) return 'go';
  if (filePath.endsWith('.sh')) return 'bash';
  if (filePath.endsWith('.vue') || filePath.endsWith('.svelte')) return 'vue';
  if (filePath.endsWith('.ts') || filePath.endsWith('.tsx') || filePath.endsWith('.mjs')) return 'typescript';
  if (filePath.endsWith('.js') || filePath.endsWith('.jsx')) return 'javascript';
  return 'unknown';
}

function stripQualifiers(s, lang) {
  for (;;) {
    let next = null;
    if (lang === 'rust') {
      for (const q of ['pub ', 'pub(crate) ', 'pub(super) ', 'unsafe ', 'async ', 'extern ']) {
        if (s.startsWith(q)) { next = s.slice(q.length); break; }
      }
      if (next === null && s.startsWith('pub(in ')) {
        const c = s.indexOf(')');
        if (c !== -1) next = s.slice(c + 1);
      }
    } else if (lang === 'typescript' || lang === 'javascript' || lang === 'vue' || lang === 'svelte') {
      for (const q of ['export ', 'export default ', 'default ', 'declare ', 'abstract ', 'async ']) {
        if (s.startsWith(q)) { next = s.slice(q.length); break; }
      }
    } else if (lang === 'python') {
      if (s.startsWith('async ')) next = s.slice(6);
    } else if (lang === 'bash' || lang === 'unknown') {
      if (s.startsWith('function ')) next = s.slice(9);
    }
    if (next === null) return s;
    s = next.trimStart();
  }
}

function stripGoReceiver(s) {
  s = s.trimStart();
  if (!s.startsWith('(')) return s;
  let depth = 0;
  for (let i = 0; i < s.length; i++) {
    if (s[i] === '(') depth++;
    else if (s[i] === ')') {
      depth--;
      if (depth === 0) return s.slice(i + 1).trimStart();
    }
  }
  return s;
}

function nameToken(raw) {
  const m = raw.split(/[<(:{ =;,]/)[0];
  return (m || '').trim().replace(/[?!]+$/, '');
}

function kindOf(normalized, kw) {
  if (kw === 'trait ') return 'interface';
  if (/(class |struct |enum |interface |trait |protocol |module )/.test(normalized)) return 'class';
  if (kw === 'mod ' || normalized.includes('namespace ')) return 'module';
  if (kw === 'type ') return 'type';
  if (normalized.includes('macro_rules')) return 'macro';
  if (kw === 'const ' || kw === 'static ' || kw === 'let ' || kw === 'var ') return 'const';
  if (kw === 'impl ' || normalized.includes('impl ')) return 'impl';
  return 'function';
}

// Doc-comment lines (`///`, `//!`, `/**`, `##`, `#:`, `"""`). Plain `//`
// narration is not hoisted — same rule as code.rs.
function docText(t) {
  let body = null;
  if (t.startsWith('///') || t.startsWith('//!') || t.startsWith('/**')) body = t.slice(3);
  else if (t.startsWith('##') || t.startsWith('#:')) body = t.slice(2);
  else if (t.startsWith('"""')) body = t.replace(/"/g, '');
  else if (t.startsWith('--')) body = t.slice(2);
  else if (t.startsWith('*') && t.length > 1) body = t.slice(1);
  if (body === null) return null;
  body = body.replace(/^[/\* #!\->:"]+/, '').trim();
  if (!body || body.startsWith('*')) return null;
  return body.slice(0, 200);
}

const BASH_SKIP = new Set(['if', 'for', 'while', 'until', 'case', 'select', 'function', 'echo', 'test']);

function bashSymbol(t) {
  const p = t.indexOf('(');
  if (p === -1) return null;
  const name = t.slice(0, p).trim();
  if (!name || /[\s$\["']/.test(name)) return null;
  if (!/^[A-Za-z_.][A-Za-z0-9_.:\/-]*$/.test(name)) return null;
  if (BASH_SKIP.has(name)) return null;
  return name;
}

const IMPORT_RES = [
  /^\s*use\s+([^;]+);/,
  /^\s*import\s+(?:.*?\s+from\s+)?['"]([^'"]+)['"]/,
  /^\s*from\s+(\S+)\s+import\s+/,
];

// Go parenthesized import blocks: `"fmt"`, `"github.com/x/y"`.
const GO_IMPORT_RES = /^\s*(?:[\w.]+\s+)?"([^"]+)"/;

export function collectFileContext(filePath, text = null) {
  const src = text ?? readFileSync(filePath, 'utf8');
  const lines = src.split('\n');
  const lang = langOf(filePath);
  const kws = KEYWORDS[lang] || KEYWORDS.typescript;
  const symbols = [];
  const imports = [];
  const inlineComments = [];
  // Pending doc-block ({ text, line }); consecutive doc lines accumulate
  // into one space-joined summary (capped at 400 chars). Cleared by blank
  // lines and by code that yields no symbol; attributes, block closers and
  // plain narration are transparent — they slide the window instead of
  // consuming it, so wrapped/annotated items keep their docs.
  let pendingDocs = null; // { text, line }
  const slideWindow = (lineNo) => { if (pendingDocs) pendingDocs.line = lineNo; };

  // Header = leading comment block after optional shebang (+ vue <script>).
  let i = 0;
  if (lines[0]?.startsWith('#!')) i = 1;
  if (filePath.endsWith('.vue') && lines[i]?.trimStart().startsWith('<script')) i++;
  while (i < lines.length && lines[i].trim() === '') i++;
  const headerStart = i;
  while (i < lines.length) {
    const t = lines[i].trim();
    if (t.startsWith('//') || t.startsWith('/*') || t.startsWith('*') || t.startsWith('#') || t.startsWith('<!--')) {
      i++;
    } else break;
  }
  const header = lines.slice(headerStart, i);

  const docsNear = (line) =>
    pendingDocs && line - pendingDocs.line <= 4 ? pendingDocs.text : '';

  lines.forEach((line, idx) => {
    const lineNo = idx + 1;
    if (idx < headerStart + header.length) return;
    const t = line.trim();
    if (!t) { pendingDocs = null; return; }
    const doc = docText(t);
    if (doc !== null) {
      if (pendingDocs && lineNo === pendingDocs.line + 1) {
        pendingDocs = { text: (pendingDocs.text + ' ' + doc).slice(0, 400), line: lineNo };
      } else {
        pendingDocs = { text: doc, line: lineNo };
      }
      return;
    }
    if (t.startsWith('//') || t.startsWith('/*') || t.startsWith('<!--') || (t.startsWith('#') && !t.startsWith('#!'))) {
      if (t.length > 3) inlineComments.push({ line: lineNo, text: t.slice(0, 200) });
      slideWindow(lineNo);
      return;
    }
    if (t.startsWith('#[') || t.startsWith('@')) { slideWindow(lineNo); return; } // attribute: keep docs
    if (t.startsWith('*/') || t === '-->') { slideWindow(lineNo); return; } // block-doc closer: keep docs

    if (lang === 'bash') {
      const name = bashSymbol(stripQualifiers(t, lang));
      if (name) symbols.push({ name, kind: 'function', line: lineNo, docs: docsNear(lineNo), engine: 'heuristic' });
      pendingDocs = null;
      return;
    }

    const normalized = stripQualifiers(t, lang);
    // `const`/`let`/`var`/`static` only count at top level (column 0):
    // indented ones are function bodies, not module structure.
    const indented = /^\s/.test(line);
    for (const kw of kws) {
      if (normalized.startsWith(kw)) {
        if ((kw === 'const ' || kw === 'let ' || kw === 'var ' || kw === 'static ') && indented) break;
        let rest = normalized.slice(kw.length);
        if (lang === 'go' && kw === 'func ') rest = stripGoReceiver(rest);
        let name;
        if (kw === 'impl ') {
          // `Trait for Target` → target; leading `impl<T>` skipped.
          let s2 = rest.trimStart();
          if (s2.startsWith('<')) {
            let depth = 0, cut = -1;
            for (let k = 0; k < s2.length; k++) {
              if (s2[k] === '<') depth++;
              else if (s2[k] === '>') { depth--; if (depth === 0) { cut = k; break; } }
            }
            if (cut !== -1) s2 = s2.slice(cut + 1).trimStart();
          }
          const toks = s2.split(/\s+/);
          const forIx = toks.indexOf('for');
          name = nameToken(forIx !== -1 ? (toks[forIx + 1] || '') : toks[0] || '');
        } else {
          name = nameToken(rest);
        }
        if (name) {
          symbols.push({ name, kind: kindOf(normalized, kw), line: lineNo, docs: docsNear(lineNo), engine: 'heuristic' });
        }
        break;
      }
    }
    for (const re of IMPORT_RES) {
      const m = line.match(re);
      if (m) { imports.push({ raw: m[1].trim().slice(0, 120), line: lineNo }); break; }
    }
    if (lang === 'go') {
      const m = line.match(GO_IMPORT_RES);
      if (m && !/=>/.test(line)) imports.push({ raw: m[1].trim().slice(0, 120), line: lineNo });
    }
    pendingDocs = null;
  });

  return {
    file: filePath,
    header,
    symbols: symbols.slice(0, 200),
    imports: imports.slice(0, 40),
    inlineComments: inlineComments.slice(0, 80),
  };
}

if (process.argv[1]?.endsWith('collect-file-context.mjs')) {
  const files = process.argv.slice(2);
  if (files.length === 0) {
    console.error('usage: collect-file-context.mjs <file...>');
    process.exit(2);
  }
  console.log(JSON.stringify(files.map((f) => collectFileContext(f)), null, 2));
}
