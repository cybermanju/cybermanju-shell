#!/usr/bin/env node
// CyberManju OS — Wiki symbol deep-view builder (scripts/wiki-symbols.mjs).
// Role: extract the real source block for a symbol (brace/indent matching
//   per language, capped) and assemble an honest auto-explanation from the
//   data that actually exists at build time — doc-comment, file header
//   Role line, graphify symbol edges, contract signals. No LLM, no invented
//   prose: everything quoted or derived is traceable to the repo.
// Exports: extractSymbolCode(text, line, file) → { code, lines, truncated };
//   explainSymbol(entry) → explanation paragraph(s); buildSymbolEntries(...)
//   → per-subject symbol payload entries (code + explanation + connections).
// Integrates: scripts/generate-wiki.mjs calls buildSymbolEntries when
//   graphify's graph.json ran (engine: graphify); without it subjects keep
//   the heuristic top-symbols list and no code payload is emitted.
// Contracts: engine-honest — explanations end with their provenance line
//   ("engine: heuristic — assembled at build time"); code is verbatim
//   source, truncated only with an explicit marker; caps keep each
//   symbols-<id>.json LLM/SPA friendly (< ~120 KB).

const MAX_CODE_LINES = 120;
const MAX_CODE_CHARS = 4000;

// Strip strings + line comments so brace matching isn't fooled by `{` in
// literals. Block comments are left (rare braces inside are acceptable).
function sanitize(line) {
  return line
    .replace(/"(?:[^"\\]|\\.)*"/g, '""')
    .replace(/'(?:[^'\\]|\\.)*'/g, "''")
    .replace(/`(?:[^`\\]|\\.)*`/g, '``')
    .replace(/\/\/.*$/, '')
    .replace(/#(?!!).*$/, ''); // bash/python comments (#! shebang kept)
}

function indentOf(line) {
  const m = /^[ \t]*/.exec(line);
  return m ? m[0].length : 0;
}

/**
 * Extract the source block for a symbol declared near `line` (1-based).
 * Brace languages (rust/ts/js/go/vue/sh) match {} depth; python matches
 * indentation. Falls back to a window from the declaration line.
 */
export function extractSymbolCode(text, line, file = '') {
  const lines = text.split('\n');
  // Graphify file-level nodes point at L1 — start at the first non-blank.
  let start = Math.max(0, Math.min((line || 1) - 1, lines.length - 1));
  while (start < lines.length && !lines[start].trim()) start++;
  if (start >= lines.length) return { code: '', lines: 0, truncated: false };
  // Include the doc-comment block right above the declaration (max 6 lines)
  // so the extracted snippet is self-documenting.
  {
    let up = start;
    let taken = 0;
    while (up > 0 && taken < 6) {
      const t = lines[up - 1].trim();
      if (/^(\/\/\/?|\/\/!|\/\*\*?|\*|#:|##|""")/.test(t) && !/^\/\/\s*=+/.test(t)) {
        up--;
        taken++;
      } else break;
    }
    start = up;
  }
  const isPy = file.endsWith('.py');
  let end;
  if (isPy) {
    const base = indentOf(lines[start]);
    end = start + 1;
    while (end < lines.length) {
      const l = lines[end];
      if (l.trim() && indentOf(l) <= base) break;
      end++;
    }
  } else {
    // Brace matching from the first `{` at/after the declaration. Items
    // with no body (type aliases, constants) take up to the next blank-ish
    // gap of 2+ lines or the next top-level keyword.
    let depth = 0;
    let seenBrace = false;
    end = start;
    for (; end < lines.length; end++) {
      const s = sanitize(lines[end]);
      for (const ch of s) {
        if (ch === '{') {
          depth++;
          seenBrace = true;
        } else if (ch === '}') depth--;
      }
      if (seenBrace && depth <= 0) {
        end++;
        break;
      }
      if (!seenBrace && end > start) {
        const t = lines[end].trim();
        const prevBlank = !lines[end - 1].trim();
        if ((prevBlank && t) || /^(pub |export |async |fn |function |func |def |class |struct |impl |const |let |var |static |type |interface |enum |mod )/.test(t)) {
          break;
        }
      }
    }
    if (end > lines.length) end = lines.length;
  }
  const total = end - start;
  let slice = lines.slice(start, end);
  let truncated = false;
  if (slice.length > MAX_CODE_LINES) {
    slice = slice.slice(0, MAX_CODE_LINES);
    truncated = true;
  }
  let code = slice.join('\n').replace(/\s+$/, '');
  if (code.length > MAX_CODE_CHARS) {
    code = code.slice(0, MAX_CODE_CHARS);
    truncated = true;
  }
  if (truncated) code += `\n… (truncated — full source at ${file}:${line})`;
  return { code, lines: total, truncated };
}

const KIND_NOUN = {
  function: 'function',
  class: 'type / class',
  module: 'module',
  interface: 'interface',
  const: 'constant',
  impl: 'impl block',
  macro: 'macro',
  type: 'type alias',
};

/**
 * Assemble the "how it works" explanation from real build-time data.
 * @param {{name, kind, docs, file, line, headerRole?, connections?, contracts?, degree?}} entry
 */
export function explainSymbol(entry) {
  const parts = [];
  const noun = KIND_NOUN[entry.kind] ?? 'symbol';
  parts.push(`\`${entry.name}\` is a ${noun} declared at ${entry.file}:${entry.line}.`);
  if (entry.docs) parts.push(`Its doc-comment says: “${entry.docs}”`);
  if (entry.headerRole) parts.push(`The file header describes its job as: “${entry.headerRole}”`);
  const conns = entry.connections ?? [];
  if (conns.length) {
    const out = conns.filter((c) => (c.via ?? 'out') === 'out');
    const inc = conns.filter((c) => c.via === 'in');
    const fmt = (c) => `\`${c.name}\` (${(c.confidence ?? 'EXTRACTED').toLowerCase()})`;
    if (out.length) parts.push(`Graph edges show it ${out.map((c) => `${c.relation.replace(/_/g, ' ')} ${fmt(c)}`).join(', ')}.`);
    if (inc.length) parts.push(`It is reached from ${inc.map((c) => `${fmt(c)} (${c.relation.replace(/_/g, ' ')})`).join(', ')}.`);
    if (typeof entry.degree === 'number' && entry.degree > 0) {
      parts.push(`Total graph connectivity: degree ${entry.degree} (weighted, graphify AST).`);
    }
  } else if (typeof entry.degree === 'number' && entry.degree > 0) {
    parts.push(`Graphify ranked it as a hub with weighted degree ${entry.degree}.`);
  }
  if (entry.contracts?.length) {
    parts.push(`Contract signals in its file: ${entry.contracts.slice(0, 6).map((c) => `\`${c}\``).join(', ')}.`);
  }
  parts.push('_Explanation assembled at build time from doc-comments, file headers and graphify edges — engine: heuristic, no LLM._');
  return parts.join(' ');
}

/**
 * Build the per-subject symbol payload: union of god nodes + top symbols +
 * graphify subjectSymbols, each with real extracted code + explanation.
 * @param {{godNodes, top, files, contracts}} collected  subject collector row
 * @param {Array} symbolNodes  graphify subjectSymbols entries for this subject
 * @param {(file: string) => string|null} readFile  source reader (injected)
 */
export function buildSymbolEntries(collected, symbolNodes, readFile) {
  const byKey = new Map(); // `${name}@${file}` → merged entry
  const key = (name, file) => `${name}@${file}`;
  const upsert = (name, file, line, kind, docs, extra = {}) => {
    const k = key(name, file);
    let e = byKey.get(k);
    if (!e) {
      e = { name, file, line: line || 0, kind: kind || 'function', docs: docs || '', connections: [], ...extra };
      byKey.set(k, e);
    } else {
      if (line && !e.line) e.line = line;
      if (kind && e.kind === 'function' && kind !== 'function') e.kind = kind;
      if (docs && !e.docs) e.docs = docs;
      Object.assign(e, extra);
    }
    return e;
  };
  // File header Role line (first merged-header prose line) per file.
  const roleByFile = new Map();
  for (const f of collected.files ?? []) {
    const role = (f.header ?? []).find((h) => /Role:/i.test(h)) ?? (f.header ?? [])[0];
    if (role) roleByFile.set(f.path, role.replace(/^Role:\s*/i, '').trim());
  }
  // 1. graphify symbol nodes (real repo graph) — strongest hubs first.
  for (const n of symbolNodes ?? []) {
    const e = upsert(n.name, n.file, n.line, n.kind ?? 'function', n.docs ?? '', {
      degree: n.degree,
      community: n.community,
      connections: n.neighbors ?? [],
    });
    if (n.degree && (!e.degree || n.degree > e.degree)) e.degree = n.degree;
  }
  // 2. god nodes (may sit outside the graphify subject cap).
  for (const g of collected.godNodes ?? []) {
    const e = upsert(g.name, g.file, g.line, 'function', '', { degree: g.degree, community: g.community });
    if (g.degree && (!e.degree || g.degree > e.degree)) e.degree = g.degree;
  }
  // 3. top symbols from the structure finder (kind + docs truth).
  for (const f of collected.files ?? []) {
    for (const s of (f.symbols ?? []).slice(0, 12)) {
      upsert(s.name, f.path, s.line, s.kind, s.docs ?? '');
    }
  }
  const contractByFile = new Map((collected.contracts ?? []).map(([sig, where]) => [sig, where]));
  const fileContracts = new Map();
  for (const [sig, where] of contractByFile) {
    for (const f of where) {
      if (!fileContracts.has(f)) fileContracts.set(f, []);
      fileContracts.get(f).push(sig);
    }
  }
  const out = [];
  for (const e of byKey.values()) {
    // Real source: read the file, extract the block at the symbol's line.
    let code = '';
    let codeLines = 0;
    const src = readFile(e.file);
    if (src) {
      const ext = extractSymbolCode(src, e.line, e.file);
      code = ext.code;
      codeLines = ext.lines;
    }
    const explanation = explainSymbol({
      ...e,
      headerRole: roleByFile.get(e.file),
      contracts: fileContracts.get(e.file) ?? [],
    });
    out.push({ ...e, code, codeLines, explanation });
  }
  // Payload order: hubs first (most connected = most interesting), cap.
  out.sort((a, b) => (b.degree ?? 0) - (a.degree ?? 0) || a.name.localeCompare(b.name));
  return out.slice(0, 60);
}
