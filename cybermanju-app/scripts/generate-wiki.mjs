#!/usr/bin/env node
// CyberManju OS — wiki generator (Pages wiki + llms.txt).
// Role: scan the tree with the improved structure finder, group files into
//   subjects, and emit LLM artifacts (llms.txt, per-subject .txt/.json) plus
//   static per-subject .html, the subjects.json payload the Three.js SPA
//   fetches at runtime, and the per-subject FORM assignment (shape/glyph/
//   shapeWhy — a distinct, function-themed 3D component per module, never a
//   bare cube/circle) that both the SPA and the injected graph.html script
//   consume.
// Exports: SUBJECTS table (id/title/tag/color/shape/glyph/shapeWhy/paths);
//   main `--out <dir>` (default dist-wasm) — importable without side effects
//   (CLI guard), so tests can pin the form table.
// Integrates: reads `scripts/collect-file-context.mjs` (heuristic fast path;
//   the Rust tree-sitter engine stays authoritative for code-intel payloads)
//   and `scripts/wiki-graph.mjs` (graphify graph.json → subject edges/god
//   nodes/communities/paths, when present — `npm run wiki:graph` or CI's
//   `graphify extract . --code-only` produces it) plus `scripts/wiki-symbols.mjs`
//   (per-subject symbols-<id>.json: real source blocks + auto explanations)
//   and `scripts/wiki-graph-html.mjs` (symbol-level interactive graph.html —
//   real symbol/module nodes with docs, never "Community {n}" meta-circles);
//   `wiki-src/shapes.ts` owns the matching 3D/clip forms.
//   Output lands in dist-wasm/{llms.txt,wiki/} so the Pages artifact serves it
//   with zero extra deploy steps. `npm run wiki` chains this + the vite build.
// Contracts: read-only w.r.t. source; only writes under --out (created with
//   mkdir -p). Caps keep every .txt LLM-friendly (< ~40 KB).
//   Knowledge-graph enrichment is OPTIONAL: no graphify-out/graph.json means
//   the heuristic link miner runs and every artifact reports engine
//   "heuristic" — never fabricated EXTRACTED/INFERRED provenance.
//   Every artifact stamps the build commit + live-update rules (CI
//   regenerates them each push — curl consumers re-fetch, don't cache).

import { execSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { collectFileContext } from './collect-file-context.mjs';
import { aggregateGraph } from './wiki-graph.mjs';
import { buildSymbolEntries } from './wiki-symbols.mjs';
import { buildGraphHtml } from './wiki-graph-html.mjs';

const SITE = 'https://cybermanju.github.io';
const VERSION = JSON.parse(readFileSync('package.json', 'utf8')).version;
// Build provenance every artifact stamps: consumers pin/clobber-cache on it,
// and it proves which commit a curl'd .txt describes.
const GIT_COMMIT = (() => {
  try { return execSync('git rev-parse --short=12 HEAD', { encoding: 'utf8' }).trim(); } catch { return null; }
})();

// id, title, tagline, blurb, accent color, path prefixes.
export const SUBJECTS = [
  { id: 'overview', title: 'Overview', tag: 'start here: identity, stack, transports', color: '#c084fc', shape: 'facet', glyph: '🌐', shapeWhy: 'one faceted crystal — the whole system seen as a single object',
    paths: ['src/main.ts', 'src-tauri/src/main.rs', 'src-tauri/src/lib.rs', 'docker/server/src/main.rs'],
    blurb: 'CyberManju OS is a quantum-resistant encrypted file manager plus a decentralized OS layer. One Vue 3 + Pinia frontend runs over three transports — Tauri IPC on desktop, REST against the :3456 dashboard, and a WASM volume on GitHub Pages — while 15 Rust crates own agent, sync, crypto, compression, disk, search and OS services.' },
  { id: 'agent', title: 'Agent System', tag: 'loop, permissions, memory, failover', color: '#a855f7', shape: 'knot', glyph: '🤖', shapeWhy: 'the agentic loop woven into a torus knot — config → providers → edit → stream',
    paths: ['crates/agent/', 'crates/types/src/agent.rs'],
    blurb: 'The agentic loop (config → providers → MCP → edit → redact → stream) with opencode-style allow/ask/deny permissions, two-layer memory (transcripts in agent_sessions plus semantic rows in agent_memories, 2200-char recall budget), and provider failover chains that continue the same transcript on terminal auth/rate-limit/network errors.' },
  { id: 'sync', title: 'Sync Pipeline', tag: 'scan, erasure, backends, repair', color: '#ff4d6d', shape: 'orbit', glyph: '🔄', shapeWhy: 'an open ring — the scan → compress → encrypt → upload → verify cycle',
    paths: ['crates/sync/', 'crates/erasure/'],
    blurb: 'Async sync jobs (202 + poll, ≤10 min parked asks) run scan → compress → encrypt → upload → verify → link → clean, with Reed-Solomon k+m striped placement, quota/rate-limit/retry guards, and scrub/repair/gc/lease workers. Failures keep the local original and record a WARNING — never silent loss.' },
  { id: 'crypto', title: 'Crypto', tag: 'ML-KEM, ChaCha20, Argon2id, BLAKE3', color: '#ef4444', shape: 'diamond', glyph: '💎', shapeWhy: 'an octahedral key diamond — post-quantum hardness, facet = KEM/MAU/sign layers',
    paths: ['crates/crypto/'],
    blurb: 'Hybrid post-quantum encryption: ML-KEM-1024 key encapsulation plus X25519, ChaCha20Poly1305 data encryption, ML-DSA/SLH-DSA signatures, Argon2id key derivation and BLAKE3 hash-anchored edits. Secrets are sealed per config (hasKey only, never echoed).' },
  { id: 'compression', title: 'Compression', tag: 'LZ4, ZSTD-15, Brotli-11, .cyb3', color: '#e879f9', shape: 'funnel', glyph: '🗜️', shapeWhy: 'a funnel — LZ4 → ZSTD → Brotli cascade narrowing bytes to a .cyb3 payload',
    paths: ['crates/compression/'],
    blurb: 'The TripleCompressor cascade (LZ4 → ZSTD-15 → Brotli-11) with per-layer stats and .cyb3 payloads. Incompressible input short-circuits after LZ4 (ratio > 0.98). The browser mirrors the chain with zstd-wasm so Pages reads and writes the same frames.' },
  { id: 'disk', title: 'Virtual Disk', tag: 'volumes, catalog, allocator', color: '#a855f7', shape: 'platter', glyph: '💾', shapeWhy: 'a 12-sided platter — the single-file virtual disk and its block allocator',
    paths: ['crates/disk/'],
    blurb: 'A single-file virtual disk with superblock, block allocator, file catalog and mountable volumes — the durable home for the vault, the redb database and sync state, shared by desktop, server and the browser volume.' },
  { id: 'search', title: 'Search + Scene', tag: 'Tantivy BM25, 17 categories', color: '#f0abfc', shape: 'lens', glyph: '🔍', shapeWhy: 'a magnifier ring + handle — Tantivy BM25 lens over the scene classifier',
    paths: ['crates/search/', 'crates/scene/'],
    blurb: 'Full-text search over a Tantivy BM25 index (filename, content, tags, metadata) with faceted and fuzzy queries, paired with a heuristic scene classifier (17 categories × 10 languages, weighted name/tags/path) that also powers the Pages BM25-lite transport.' },
  { id: 'faces', title: 'Faces', tag: 'onnx, heuristic-v2, none', color: '#fda4af', shape: 'star', glyph: '🎭', shapeWhy: 'a five-point spotlight — detection regions, engine-labeled honestly',
    paths: ['crates/faces/'],
    blurb: 'Face detection with honest engine labels: onnx where the model runs, a labeled skin-segmentation heuristic-v2 fallback, and silence (never fabricated boxes) where neither applies. Every result reports its engine.' },
  { id: 'os-shell', title: 'OS + cybsh', tag: 'shell, tasks, compute, schedule', color: '#ff4d6d', shape: 'chevron', glyph: '⌨️', shapeWhy: 'the cybsh prompt chevron — the shell `>` on every transport',
    paths: ['crates/os/', 'crates/types/src/schedule.rs'],
    blurb: 'The server-side shell (POST /api/os/exec with --json, pipes, history, completion) is identical on desktop, web and Pages via the WASM dispatcher, plus background tasks, compute dispatch and cron-style schedules.' },
  { id: 'web-api', title: 'Web API', tag: '114 routes, auth, security', color: '#c084fc', shape: 'globe', glyph: '🔌', shapeWhy: 'a faceted globe — the :3456 dashboard speaking REST to every client',
    paths: ['crates/web/', 'docker/server/'],
    blurb: 'The :3456 dashboard backend: 114 REST route arms, fail-closed auth with 404-before-401 and role RBAC, sealed secrets, scrubbed device-shell spawn, and the async job contracts (sync start 202, agent prompt 202, approve/abort). Ships as Docker/ZimaOS and CasaOS images.' },
  { id: 'db-types', title: 'Database + Types', tag: 'redb, 17+ tables, schemas', color: '#a855f7', shape: 'hex', glyph: '🗄️', shapeWhy: 'a hex prism — redb tables and the serde schemas every crate agrees on',
    paths: ['crates/db/', 'crates/types/'],
    blurb: 'One redb file (cybermanju.db) with JSON rows across 17+ tables — agent sessions/memories, disks/volumes/block_map, sync state, leases, compute tasks — and the shared serde schemas every crate and the frontend agree on.' },
  { id: 'frontend', title: 'Frontend', tag: 'Vue 3, Pinia, transports', color: '#e879f9', shape: 'pill', glyph: '🧩', shapeWhy: 'a component pill — Vue SFCs and composables snapping together',
    paths: ['src/composables/', 'src/components/', 'src/utils/', 'src/stores/', 'src/ui/', 'src/App.vue', 'src/main.ts'],
    blurb: '44 top-level Vue components plus ui primitives, 28 composables and 28 pure utils over a single Pinia store. The transport layer (tauri IPC / REST / WASM volume, REST-first for os/disk) is a pure, unit-tested decision matrix so the gateway cannot drift per call site.' },
  { id: 'tauri', title: 'Tauri Desktop', tag: '19 modules, 130 handlers', color: '#ef4444', shape: 'shield', glyph: '🛡️', shapeWhy: 'a desktop shield — the Tauri v2 shell guarding IPC + code-intel',
    paths: ['src-tauri/src/'],
    blurb: 'The Tauri v2 desktop shell: 19 command modules with 130 IPC handlers, the incremental tree-sitter code-intel engine (6 real grammars plus heuristic fallback, every result engine-labeled), and the web-dashboard bridge.' },
  { id: 'cli', title: 'CLI (cyb)', tag: 'Go client, every verb', color: '#f5f3ff', shape: 'cursor', glyph: '▶️', shapeWhy: 'the ▶ pointer — one cyb verb per server endpoint',
    paths: ['cli/'],
    blurb: 'The Go cyb CLI: one command per server verb (files, sync, agent, secrets, disks, doctor, setup), a shared REST client with tolerant decoding, terminal UI helpers, and the Supabase OAuth broker for Pages sign-in.' },
  { id: 'wasm', title: 'WASM + Pages', tag: 'OPFS volume, offline-first', color: '#c084fc', shape: 'spindle', glyph: '⚡', shapeWhy: 'an hourglass spindle — the browser volume flipping Pages into a full peer',
    paths: ['crates/os-wasm/', 'src/composables/useWasm', 'wasm-pkg.ts', 'vite.config.wasm.ts', 'vite-plugin-wasm-stub.ts'],
    blurb: 'The browser volume backend (OPFS/localStorage) that makes GitHub Pages a full peer: file pipeline, search, crypto and sync-reader run locally, fetch-free and offline-friendly, with the same error prefixes as the server.' },
  { id: 'ops', title: 'Ops + CI', tag: 'scripts, releases, android', color: '#ff4d6d', shape: 'gear', glyph: '⚙️', shapeWhy: 'a working gear — version checks, the CI matrix and the release atlas',
    paths: ['scripts/', '.github/workflows/', 'docker-compose.yml', 'docs/OPERATIONS.md', 'docs/SECURITY.md', 'docs/ANDROID.md'],
    blurb: 'Everything around the code: version single-source checks, the ~15 min CI matrix (rust, wasm, windows, linux, rpm, flatpak, arch, android, macos, pages), the 8-family release atlas, Android signing runbooks, and the Docker/ZimaOS operations guide.' },
];

const SIGNAL_RE = /SAFETY|INVARIANT|integrity:|auth:|rate_limited:|not_found:|unsupported:|too_large:|conflict:|disk_full:|network:|AGENT-\d|P\d+-\d+|MEMORY_\w+|STATIC_WRITE_LIMIT|MAX_TS_\w+|BUDGET|engine-labeled/g;

function trackedFiles() {
  return execSync('git ls-files', { encoding: 'utf8' }).split('\n').filter(Boolean);
}

function headerText(ctx) {
  return ctx.header
    .map((l) => l.replace(/^(\s*\/\/\/?\s?|\s*#\s?|\s*<!--\s?|\s*\*\s?)/, '').trim())
    .filter(Boolean)
    .slice(0, 8);
}

function collectSubject(sub, files) {
  const matched = files.filter((f) => sub.paths.some((p) => f === p || f.startsWith(p)));
  const fileCtx = [];
  const contractHits = new Map();
  const importCrates = new Set();
  let symbolCount = 0;
  let documentedCount = 0;
  for (const f of matched) {
    if (!/\.(rs|ts|vue|js|mjs|sh|go)$/.test(f)) continue;
    // Skip generated/vendored trees.
    if (f.includes('/pkg/') || f.startsWith('target/') || f.startsWith('dist')) continue;
    let text;
    try { text = readFileSync(f, 'utf8'); } catch { continue; }
    const ctx = collectFileContext(f, text);
    fileCtx.push({ path: f, header: headerText(ctx), symbols: ctx.symbols.slice(0, 60), imports: ctx.imports.slice(0, 12) });
    symbolCount += ctx.symbols.length;
    documentedCount += ctx.symbols.filter((s) => s.docs).length;
    for (const imp of ctx.imports) {
      for (const m of imp.raw.matchAll(/cybermanju[-_]([a-z_]+)/g)) {
        importCrates.add(m[1].replace(/-/g, '_'));
      }
    }
    const sigs = new Set((text.match(SIGNAL_RE) || []).map((s) => s.replace(/:$/, '')));
    for (const s of sigs) {
      if (!contractHits.has(s)) contractHits.set(s, []);
      if (contractHits.get(s).length < 6) contractHits.get(s).push(f);
    }
  }
  // Richest files first (most symbols) for the .txt module section.
  fileCtx.sort((a, b) => b.symbols.length - a.symbols.length);
  return { sub, files: fileCtx, fileCount: fileCtx.length, symbolCount, documentedCount, contracts: [...contractHits.entries()], importCrates: [...importCrates] };
}

// Crate → subject: real dependency edges mined from imports.
const CRATE_SUBJECT = {
  agent: 'agent', sync: 'sync', erasure: 'sync', crypto: 'crypto',
  compression: 'compression', disk: 'disk', web: 'web-api', db: 'db-types',
  types: 'db-types', search: 'search', scene: 'search', faces: 'faces',
  os: 'os-shell', os_wasm: 'wasm',
};

// Inter-subject links: hard import edges first, then reverse "used by"
// edges (so leaf crates like compression still show who consumes them),
// then subjects sharing ≥2 contract signals (shared protocols).
function computeLinks(collected) {
  const sigSets = new Map(collected.map((c) => [c.sub.id, new Set(c.contracts.map(([s]) => s))]));
  const impLinks = new Map();
  for (const c of collected) {
    const linked = [];
    for (const crate of c.importCrates) {
      const target = CRATE_SUBJECT[crate];
      if (target && target !== c.sub.id && !linked.includes(target)) linked.push(target);
    }
    impLinks.set(c.sub.id, linked);
  }
  for (const c of collected) {
    const linked = [...impLinks.get(c.sub.id)];
    // Reverse: whoever imports me.
    for (const o of collected) {
      if (o.sub.id !== c.sub.id && (impLinks.get(o.sub.id) || []).includes(c.sub.id) && !linked.includes(o.sub.id)) {
        linked.push(o.sub.id);
      }
    }
    if (linked.length < 5) {
      const mine = sigSets.get(c.sub.id);
      const scored = collected
        .filter((o) => o.sub.id !== c.sub.id && !linked.includes(o.sub.id))
        .map((o) => ({ id: o.sub.id, shared: [...sigSets.get(o.sub.id)].filter((s) => mine.has(s)).length }))
        .filter((x) => x.shared >= 2)
        .sort((a, b) => b.shared - a.shared);
      for (const s of scored) {
        if (linked.length >= 5) break;
        linked.push(s.id);
      }
    }
    c.links = linked.slice(0, 5);
  }
}

function escHtml(s) {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

// Cross-subject knowledge-graph summary for .txt (engine-labeled: only ever
// claims EXTRACTED/INFERRED provenance when graphify actually ran).
function graphTxt(c) {
  if (c.engine !== 'graphify') {
    return ['## Related modules (engine: heuristic)', '',
      'No graphify graph present at build time — links come from mined crate imports + shared contract signals.', ''];
  }
  const out = ['## Knowledge graph (engine: graphify — EXTRACTED/INFERRED edges)', ''];
  if (c.community) out.push(`- Community: #${c.community.id} (${c.community.nodes} symbols in cluster)`);
  if (c.edges?.length) {
    out.push('- Strongest cross-subject edges:');
    for (const e of c.edges) {
      const rels = Object.entries(e.relations).map(([r, n]) => `${r} ×${n}`).join(', ');
      out.push(`  - → ${e.to} (w${e.weight}; EXTRACTED ${e.extracted} / INFERRED ${e.inferred}${rels ? `; ${rels}` : ''})${e.sample ? ` — e.g. ${e.sample}` : ''}`);
    }
  }
  if (c.godNodes?.length) {
    out.push('- God nodes here (most-connected hubs):');
    for (const g of c.godNodes) out.push(`  - ${g.name} — ${g.file}:${g.line} (degree ${g.degree}, community #${g.community})`);
  }
  out.push('- Query it: `graphify query "<question>" --graph graphify-out/graph.json` · `graphify path A B`', '');
  return out;
}

function subjectTxt(c) {
  const { sub, files, fileCount, symbolCount, documentedCount, contracts } = c;
  const coverage = symbolCount === 0 ? 100 : Math.round((documentedCount / symbolCount) * 100);
  const out = [];
  out.push(`# CyberManju OS wiki — ${sub.title} (v${VERSION})`);
  out.push(`> ${sub.tag}`);
  out.push(`> ${sub.blurb}`);
  if (sub.shape) out.push(`> Form: ${sub.glyph} ${sub.shape} — ${sub.shapeWhy}`);
  out.push(`> App: ${SITE}/wiki/#/ + ${sub.id} | Static: ${SITE}/wiki/${sub.id}.html | JSON: ${SITE}/wiki/${sub.id}.json`);
  out.push(`> Live artifact: CI regenerates this file on every push (build commit ${GIT_COMMIT ?? 'unknown'}). Re-fetch instead of caching; the architecture map + update rules live in ${SITE}/llms.txt.`);
  out.push('');
  out.push(`Files: ${fileCount}  |  Symbols: ${symbolCount} (${documentedCount} documented, ${coverage}%) — gaps: \`node scripts/check-symbol-docs.mjs --dir <path>\``);
  if (c.links && c.links.length) out.push(`Links: ${c.links.join(', ')} (see their .txt for the other side)`);
  if (c.engine === 'graphify') out.push(`Symbol payload: symbols-${sub.id}.json (real source block + auto explanation + graph edges per symbol)`);
  out.push('');
  out.push(...graphTxt(c));
  out.push('## Modules (merged top-file headers)');
  out.push('');
  for (const f of files.slice(0, 40)) {
    out.push(`### ${f.path}`);
    for (const h of f.header) out.push(`  ${h}`);
    out.push('');
  }
  if (files.length > 40) out.push(`… +${files.length - 40} more files (see ${sub.id}.json)`, '');
  out.push('## Symbols (top 150)');
  out.push('');
  let n = 0;
  for (const f of files) {
    for (const s of f.symbols) {
      if (n++ >= 150) break;
      out.push(`- ${s.name} (${s.kind}) — ${f.path}:${s.line}${s.docs ? ` — ${s.docs}` : ''}`);
    }
    if (n >= 150) break;
  }
  out.push('');
  out.push('## Contract signals in this subject');
  out.push('');
  if (contracts.length === 0) out.push('(none — pure logic; errors bubble to callers)');
  for (const [sig, where] of contracts.slice(0, 40)) out.push(`- ${sig} — ${where.join(', ')}`);
  out.push('');
  out.push('## All files');
  out.push('');
  for (const f of files) out.push(`- ${f.path}`);
  const txt = out.join('\n');
  return txt.length > 40_000 ? txt.slice(0, 40_000) + '\n…(truncated at 40 KB; see JSON)\n' : txt + '\n';
}

function subjectJson(c) {
  const { sub, files, fileCount, symbolCount, documentedCount, contracts } = c;
  return {
    id: sub.id, title: sub.title, tag: sub.tag, blurb: sub.blurb, color: sub.color,
    shape: sub.shape ?? null, glyph: sub.glyph ?? null, shapeWhy: sub.shapeWhy ?? null,
    version: VERSION, fileCount, symbolCount, documentedCount,
    docsCoverage: symbolCount === 0 ? 100 : Math.round((documentedCount / symbolCount) * 100),
    engine: c.engine || 'heuristic',
    community: c.community ?? null,
    edges: c.edges || [],
    godNodes: c.godNodes || [],
    links: c.links || [],
    contracts: contracts.slice(0, 40).map(([sig, where]) => ({ signal: sig, files: where })),
    files: files.map((f) => ({ path: f.path, header: f.header, symbols: f.symbols.slice(0, 40), imports: f.imports })),
  };
}

function subjectHtml(c) {
  const { sub, files, fileCount, symbolCount, contracts } = c;
  const topSyms = [];
  for (const f of files) for (const s of f.symbols) { if (topSyms.length < 60) topSyms.push({ ...s, file: f.path }); }
  const rows = files.slice(0, 40).map((f) => `
      <details><summary><code>${escHtml(f.path)}</code> <span class="n">${f.symbols.length} symbols</span></summary>
      ${f.header.map((h) => `<p class="h">${escHtml(h)}</p>`).join('')}
      <p class="syms">${f.symbols.slice(0, 12).map((s) => `<code>${escHtml(s.name)}</code>`).join(' ')}</p>
      </details>`).join('');
  const graphNav = c.engine === 'graphify' ? `<a href="./graph.html">graph</a>` : '';
  const graphSec = c.engine === 'graphify'
    ? `<h2>Knowledge graph <span style="color:var(--mut);font-size:12px">engine: graphify · EXTRACTED/INFERRED</span></h2>
${c.community ? `<p>Community <code>#${c.community.id}</code> (${c.community.nodes} symbols).</p>` : ''}
${(c.edges && c.edges.length) ? `<p class="syms">${c.edges.map((e) => `<code title="${escHtml(e.sample)}">${escHtml(e.to)}</code><span class="n"> w${e.weight} · E${e.extracted}/I${e.inferred}</span>`).join('<br>')}</p>` : ''}
${(c.godNodes && c.godNodes.length) ? `<p class="syms">★ god nodes: ${c.godNodes.map((g) => `<code title="${escHtml(g.file)}:${g.line} · degree ${g.degree}">${escHtml(g.name)}</code>`).join(' ')}</p>` : ''}
<p class="h">Full interactive graph: <a href="./graph.html" style="color:var(--ac)">graph.html</a> · query locally with <code>graphify query "…"</code>.</p>`
    : '';
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>CyberManju OS wiki — ${escHtml(sub.title)}</title>
<meta name="description" content="${escHtml(sub.blurb)}">
<style>
:root{--ac:${sub.color};--bg:#0d0714;--card:#1a0f2e;--tx:#f5f3ff;--mut:#b8a6d9}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--tx);font-family:"Pixelify Sans","Baloo 2",ui-rounded,system-ui,sans-serif;line-height:1.6}
.wrap{max-width:860px;margin:0 auto;padding:32px 20px 64px}
nav{display:flex;gap:12px;flex-wrap:wrap;margin-bottom:24px;font-size:14px}
nav a{color:var(--ac);text-decoration:none;border:2px solid #ffffff22;padding:6px 14px;border-radius:0;box-shadow:3px 3px 0 rgba(0,0,0,.55);font-family:"Silkscreen","Pixelify Sans",monospace;font-size:12px}
.hero{border:2px solid #ffffff1e;border-left:8px solid var(--ac);border-radius:0;box-shadow:5px 5px 0 rgba(0,0,0,.55);padding:24px;background:var(--card)}
.hero h1{margin:0 0 4px;font-size:30px;font-family:"Silkscreen","Pixelify Sans",monospace;line-height:1.3}.tag{color:var(--ac);font-weight:700;font-family:"Silkscreen","Pixelify Sans",monospace;font-size:12px}.blurb{color:var(--mut)}
.stats{display:flex;gap:16px;margin:16px 0;color:var(--mut);font-size:14px}
.form{margin:6px 0 0;color:var(--mut);font-size:13px}.form code{color:var(--ac);background:#ffffff12;padding:1px 6px}
h2{font-family:"Silkscreen","Pixelify Sans",monospace;font-size:15px;letter-spacing:.06em}
details{background:#ffffff08;border:2px solid #ffffff14;border-radius:0;padding:8px 12px;margin:8px 0}
summary{cursor:pointer}summary code{color:var(--ac)}.n{color:var(--mut);font-size:12px}
.h{margin:4px 0;color:var(--mut);font-size:14px}.syms code{background:#ffffff12;border-radius:0;padding:1px 6px;font-size:12px;margin:1px}
ul.links{list-style:none;padding:0;display:flex;gap:10px;flex-wrap:wrap}ul.links a{color:var(--tx);background:#ffffff10;border:2px solid var(--line,#ffffff1f);border-radius:0;box-shadow:3px 3px 0 rgba(0,0,0,.55);padding:6px 12px;text-decoration:none;font-size:13px}
footer{margin-top:32px;color:var(--mut);font-size:13px}
</style></head><body><div class="wrap">
<nav><a href="${SITE}/wiki/">✦ 3D wiki</a>${graphNav}<a href="${SITE}/llms.txt">llms.txt</a><a href="./${sub.id}.txt">.txt</a><a href="./${sub.id}.json">.json</a></nav>
<div class="hero"><div class="tag">${escHtml(sub.tag)}</div><h1>${sub.glyph ? escHtml(sub.glyph) + ' ' : ''}${escHtml(sub.title)}</h1>
<p class="blurb">${escHtml(sub.blurb)}</p>
${sub.shape ? `<p class="form">${sub.glyph} form: <code>${escHtml(sub.shape)}</code> — ${escHtml(sub.shapeWhy)}</p>` : ''}
<div class="stats"><span>${fileCount} files</span><span>${symbolCount} symbols</span><span>v${VERSION}</span></div></div>
${(c.links && c.links.length) ? `<h2>Related modules</h2><p class="syms">${c.links.map((id) => `<a href="./${id}.html" style="color:var(--ac);text-decoration:none;border:2px solid #ffffff22;border-radius:0;box-shadow:2px 2px 0 rgba(0,0,0,.55);padding:2px 10px;margin:2px;display:inline-block">${escHtml(id)}</a>`).join(' ')}</p>` : ''}
<h2>Modules</h2>${rows}
<h2>Top symbols</h2><p class="syms">${topSyms.map((s) => `<code title="${escHtml(s.file)}:${s.line}">${escHtml(s.name)}</code>`).join(' ')}</p>
<h2>Contracts</h2><p>${contracts.length ? contracts.slice(0, 24).map(([s]) => `<code>${escHtml(s)}</code>`).join(' ') : 'none — pure logic'}</p>
${graphSec}
<ul class="links"><li><a href="./${sub.id}.txt">LLM text</a></li><li><a href="./${sub.id}.json">JSON</a></li><li><a href="${SITE}/wiki/#/${sub.id}">Open in 3D wiki</a></li></ul>
<footer>CyberManju OS v${VERSION} — auto-generated wiki. Prefer the <a href="${SITE}/wiki/">interactive 3D wiki</a> for humans, <a href="${SITE}/llms.txt">llms.txt</a> for agents.</footer>
</div></div></body></html>
`;
}

function llmsTxt(collected, kg) {
  const out = [
    '# CyberManju OS',
    `> Quantum-resistant encrypted file manager + decentralized OS layer (v${VERSION}).`,
    '> Interactive wiki for humans: https://cybermanju.github.io/wiki/',
    '> This file is the agent entry point: one line per subject, then fetch the .txt you need.',
    `> Live build: commit ${GIT_COMMIT ?? 'unknown'} — CI regenerates every wiki file on each push (see "Live updates" below).`,
    '',
    ...collected.map((c) => `- [${c.sub.title}](https://cybermanju.github.io/wiki/${c.sub.id}.txt): ${c.sub.tag} — ${c.fileCount} files, ${c.symbolCount} symbols.`),
    '',
  ];
  out.push(
    '## Live updates (read before caching)',
    '- Everything under https://cybermanju.github.io/wiki/ and this file are auto-generated by CI (`.github/workflows/ci.yml`, the "Build wiki" step, chained after the graphify knowledge-graph step) on every push to main/develop, then served statically from GitHub Pages.',
    `- Each file stamps the commit it describes (currently ${GIT_COMMIT ?? 'unknown'}). Do not cache long-term: re-fetch per session, or cache-bust with the commit (…/wiki/agent.txt?v=${GIT_COMMIT ?? 'HEAD'}).`,
    '- The knowledge graph (EXTRACTED/INFERRED edges, god nodes, communities) is rebuilt from source in the same CI run — edge provenance in these files always matches the stamped commit, never a stale local copy.',
    '',
    '## Architecture (one-screen system model)',
    '- Shape: one Vue 3 + Pinia SPA (`src/`) that runs over three transports chosen by `src/composables/useTauri.ts` — Tauri IPC on desktop, REST against the :3456 dashboard (`crates/web` + `docker/server`) on server, and a WASM OPFS volume (`crates/os-wasm`) on GitHub Pages. `os_*`/`disk_*` calls are REST_FIRST; the Go `cli/` speaks the same REST API.',
    '- Request flow: component → composable → transport → handler (a Tauri command module or a `crates/web` route arm) → service crate → persistence (single-file redb `cybermanju.db` or the virtual disk). Errors always carry the fixed prefixes (`auth:` `rate_limited:` `not_found:` `unsupported:` `too_large:` `integrity:` `network:` `disk_full:` `conflict:`) that map to UI hints — never raw messages.',
    '- Service crates and their contracts: `agent` (LLM loop, allow/ask/deny permissions, two-layer memory, provider failover chains) → `web` (fail-closed auth, 404-before-401, RBAC, async jobs: POST → 202 {jobId} + poll, asks auto-deny after ≤10 min) → `sync` (scan → compress → encrypt → upload → verify → link → clean, Reed-Solomon k+m placement, scrub/repair/gc/lease workers; failures keep the local original and record a WARNING) → `disk` (superblock + block allocator + catalog + mountable volumes) → `crypto` (hybrid ML-KEM-1024+X25519 KEM, ChaCha20Poly1305, ML-DSA/SLH-DSA signatures, Argon2id, BLAKE3 hash-anchored edits) → `compression` (LZ4 → ZSTD-15 → Brotli-11 cascade in `.cyb3` frames) → `search`+`scene` (Tantivy BM25 + 17-category heuristic classifier) → `os`/`os-wasm` (the identical `cybsh` shell on server, desktop and in the browser).',
    '- Data truth: `package.json` is the single version source (`scripts/check-version.sh`); every code file carries the merged top-file header; every result that depends on a tool reports its `engine` (`graphify`/`heuristic`, `onnx`/`heuristic-v2`, tree-sitter/heuristic) — unlabeled provenance is a bug.',
    '- Deep dive: fetch each subject .txt linked above; its "Knowledge graph" section gives the cross-subject EXTRACTED/INFERRED edges, god nodes and community, and `graphify query "…" --graph graphify-out/graph.json` answers questions against the same graph locally.',
    '- Symbol deep view: https://cybermanju.github.io/wiki/symbols-<id>.json (engine graphify builds) — per-symbol {code, explanation, connections} with the real source block and graphify neighbors; the 3D wiki fetches it when a module opens.',
    '',
  );
  if (kg) {
    out.push(
      `## Knowledge graph (engine: graphify — ${kg.stats.nodes} nodes, ${kg.stats.edges} edges, EXTRACTED ${kg.stats.extracted} / INFERRED ${kg.stats.inferred})`,
      '- Query it locally: `graphify query "<question>" --graph graphify-out/graph.json`, `graphify path A B`, `graphify explain X`.',
      ...kg.godNodes.slice(0, 8).map((g) => `- God node: ${g.name} (${g.file}:${g.line}, degree ${g.degree})`),
      '',
    );
  }
  out.push(
    '## Conventions (must-read before editing code)',
    '- Transports: tauri IPC (desktop) / REST :3456 (server) / WASM volume (Pages). os_*/disk/* are REST_FIRST.',
    '- Async jobs: 202 + poll; asks park ≤10 min then auto-deny. Error prefixes: auth:/rate_limited:/not_found:/unsupported:(→501)/too_large:/integrity:/network:/disk_full:/conflict:.',
    '- Every code file carries a merged top-file header (Role/Exports/Integrates/Contracts); scripts/check-file-headers.sh enforces it.',
    '- No local cargo/Rust toolchain; Rust validation happens in CI. Local loop: check-version → vue-tsc → npm test.',
  );
  return out.join('\n') + '\n';
}

function main() {
  const outIx = process.argv.indexOf('--out');
  const OUT = outIx !== -1 ? process.argv[outIx + 1] : 'dist-wasm';
  const files = trackedFiles();
  const collected = SUBJECTS.map((sub) => collectSubject(sub, files));
  computeLinks(collected);

  // Optional knowledge-graph pass: fold graphify's graph.json over the
  // heuristic links (EXTRACTED/INFERRED edges, god nodes, communities,
  // module paths). Absent file → heuristic engine label everywhere.
  let kg = null;
  try {
    kg = aggregateGraph(JSON.parse(readFileSync('graphify-out/graph.json', 'utf8')), SUBJECTS);
  } catch {
    kg = null;
  }
  if (kg) {
    for (const c of collected) {
      const edges = kg.subjectEdges.get(c.sub.id) || [];
      // Graph edges win when they exist; subjects the AST found no
      // cross-links for keep the heuristic miner's suggestions.
      if (edges.length) c.links = edges.map((e) => e.to);
      c.engine = 'graphify';
      c.edges = edges;
      c.godNodes = kg.subjectGods.get(c.sub.id) || [];
      c.community = kg.subjectCommunity.get(c.sub.id) ?? null;
    }
  }
  mkdirSync(join(OUT, 'wiki'), { recursive: true });

  // Per-subject symbol deep-view payload (engine graphify only): union of
  // god nodes + top symbols + graphify subject symbols, each with the REAL
  // source block extracted from the repo + an auto explanation. The SPA
  // fetches symbols-<id>.json on demand when a planet is opened.
  const readFileSafe = (f) => {
    try { return readFileSync(f, 'utf8'); } catch { return null; }
  };
  const symbolsUrl = new Map();
  if (kg) {
    for (const c of collected) {
      const entries = buildSymbolEntries(c, kg.subjectSymbols.get(c.sub.id) ?? [], readFileSafe);
      writeFileSync(join(OUT, 'wiki', `symbols-${c.sub.id}.json`), JSON.stringify({ subject: c.sub.id, engine: 'graphify', symbols: entries }) + '\n');
      symbolsUrl.set(c.sub.id, `./symbols-${c.sub.id}.json`);
    }
  }

  writeFileSync(join(OUT, 'llms.txt'), llmsTxt(collected, kg));
  const index = [];
  for (const c of collected) {
    writeFileSync(join(OUT, 'wiki', `${c.sub.id}.txt`), subjectTxt(c));
    const j = subjectJson(c);
    writeFileSync(join(OUT, 'wiki', `${c.sub.id}.json`), JSON.stringify(j, null, 1) + '\n');
    writeFileSync(join(OUT, 'wiki', `${c.sub.id}.html`), subjectHtml(c));
    index.push({ id: c.sub.id, title: c.sub.title, tag: c.sub.tag, blurb: c.sub.blurb, color: c.sub.color,
      shape: c.sub.shape ?? null, glyph: c.sub.glyph ?? null, shapeWhy: c.sub.shapeWhy ?? null,
      paths: c.sub.paths ?? [],
      fileCount: c.fileCount, symbolCount: c.symbolCount,
      engine: c.engine || 'heuristic', community: c.community?.id ?? null,
      symbolsUrl: symbolsUrl.get(c.sub.id) ?? null,
      edges: (c.edges || []).map((e) => ({ to: e.to, weight: e.weight, extracted: e.extracted, inferred: e.inferred, relations: e.relations, sample: e.sample })),
      godNodes: (c.godNodes || []).map((g) => ({ name: g.name, degree: g.degree, file: g.file, line: g.line })),
      links: c.links || [],
      contracts: c.contracts.slice(0, 12).map(([sig]) => sig),
      top: j.files.flatMap((f) => f.symbols.slice(0, 6).map((s) => ({ name: s.name, kind: s.kind, docs: s.docs, file: f.path, line: s.line }))).slice(0, 40) });
  }
  // Graph summary rides along so the SPA can render provenance badges, god
  // nodes, communities and the path tracer without fetching anything else.
  const graphPayload = kg
    ? { engine: kg.engine, commit: kg.commit, stats: kg.stats,
        godNodes: kg.godNodes.slice(0, 15), communities: kg.communities, paths: kg.paths }
    : null;
  writeFileSync(join(OUT, 'wiki', 'subjects.json'), JSON.stringify({ version: VERSION, commit: GIT_COMMIT, graph: graphPayload, subjects: index }, null, 1) + '\n');
  // Symbol-level interactive graph page (engine graphify only). Built from
  // graph.json via scripts/wiki-graph-html.mjs — every node is a real code
  // symbol or subject module (never a "Community {n}" meta-circle), drawn
  // with its module's vis-network silhouette and carrying file:line +
  // doc-comment tooltips. The old path copied graphify's community-hub
  // export; that collapsed the 11k-symbol graph into opaque cluster nodes.
  let graphHtml = false;
  if (kg) {
    writeFileSync(
      join(OUT, 'wiki', 'graph.html'),
      buildGraphHtml({ subjects: SUBJECTS, collected, kg, version: VERSION, commit: GIT_COMMIT }),
    );
    graphHtml = true;
  }
  const kb = (p) => (Buffer.byteLength(readFileSync(p, 'utf8')) / 1024).toFixed(1) + ' KB';
  console.log(`wiki: ${collected.length} subjects → ${OUT}/wiki/ (+ ${OUT}/llms.txt ${kb(join(OUT, 'llms.txt'))})`);
  console.log(`  engine: ${kg ? `graphify (${kg.stats.nodes} nodes, ${kg.stats.edges} edges, EXTRACTED ${kg.stats.extracted} / INFERRED ${kg.stats.inferred}${kg.commit ? `, ${kg.commit.slice(0, 8)}` : ''}${graphHtml ? ', graph.html ✓' : ''})` : 'heuristic (no graphify-out/graph.json — run `npm run wiki:graph` to enrich)'}`);
  for (const c of collected) console.log(`  ${c.sub.id}: ${c.fileCount} files, ${c.symbolCount} symbols (${c.documentedCount} documented), ${kb(join(OUT, 'wiki', c.sub.id + '.txt'))}`);
}

// Run only as a CLI (`node scripts/generate-wiki.mjs`) — importing the
// module (tests pinning the SUBJECTS form table) must not generate.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
