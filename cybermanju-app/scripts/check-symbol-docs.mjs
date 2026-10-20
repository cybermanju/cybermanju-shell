#!/usr/bin/env node
// CyberManju OS — symbol docs auditor (scripts/check-symbol-docs.mjs).
// Role: report symbols the heuristic extractor finds with an empty `docs`
//   field, so missing item docs are visible without failing any gate.
// Exports: `--public-only` (default on), `--all`, `--dir <prefix>`,
//   `--top <n>`, `--json`, `--strict` (exit 1 when anything is missing).
// Integrates: reads `scripts/collect-file-context.mjs` (same extractor the
//   wiki and Rust code-intel use); advisory only — CI `meta-check` stays on
//   top-file headers (`check-file-headers.sh`), never on this.
// Contracts: read-only, never rewrites source; exit 0 unless `--strict` and
//   at least one listed symbol lacks docs. Skips tests, generated trees
//   (`target/`, `/pkg/`, `dist*`) and, in public mode, shell files.
//   Known limit: Go doc convention (`// Name …`) is plain narration to the
//   extractor (same rule as `code.rs`), so `cli/` coverage reads low until
//   its items carry `##`/`#:` lines — treat that subject as informational.

import { execSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { collectFileContext } from './collect-file-context.mjs';

const args = process.argv.slice(2);
const publicOnly = !args.includes('--all');
const strict = args.includes('--strict');
const asJson = args.includes('--json');
const dirIx = args.indexOf('--dir');
const dirFilter = dirIx !== -1 ? args[dirIx + 1] : null;
const topIx = args.indexOf('--top');
const topN = topIx !== -1 ? Number(args[topIx + 1]) || 40 : 40;

const isTest = (f) =>
  f.includes('__tests__') || f.endsWith('.test.ts') || f.endsWith('_test.go') ||
  f.startsWith('tests/') || f.includes('/tests/');

const detailOf = (root, f, line) => {
  try {
    return (readFileSync(`${root}/${f}`, 'utf8').split('\n')[line - 1] || '').trim();
  } catch {
    return '';
  }
};

// Public-API heuristic per language: the items external callers touch.
const isPublic = (f, s, detail) => {
  if (f.endsWith('.rs')) return detail.includes('pub ');
  if (/\.(ts|vue|js|mjs)$/.test(f)) return detail.startsWith('export ');
  if (f.endsWith('.go')) return /^[A-Z]/.test(s.name);
  return false;
};

const root = process.cwd();
const files = execSync('git ls-files', { encoding: 'utf8' })
  .split('\n')
  .filter((f) => /\.(rs|ts|vue|js|mjs|sh|go)$/.test(f))
  .filter((f) => !f.startsWith('target/') && !f.includes('/pkg/') && !f.includes('dist'))
  .filter((f) => !isTest(f))
  .filter((f) => !dirFilter || f === dirFilter || f.startsWith(dirFilter));

let total = 0;
let documented = 0;
const missing = [];
const missingByDir = new Map();

for (const f of files) {
  let ctx;
  try {
    ctx = collectFileContext(`${root}/${f}`);
  } catch {
    continue;
  }
  for (const s of ctx.symbols) {
    if (s.kind === 'impl') continue;
    const detail = detailOf(root, f, s.line);
    // Re-export shims (`pub mod foo;`, with or without a trailing note)
    // carry the top-file header instead of item docs — same exemption as
    // `check-file-headers.sh`.
    const code = detail.split('//')[0].trim();
    if (/^pub mod \w+;$/.test(code)) continue;
    if (publicOnly) {
      if (f.endsWith('.sh')) continue;
      if (!isPublic(f, s, detail)) continue;
    }
    total += 1;
    if (s.docs) {
      documented += 1;
      continue;
    }
    missing.push({ name: s.name, kind: s.kind, file: f, line: s.line, detail: detail.slice(0, 100) });
    const top = f.split('/').slice(0, 2).join('/');
    missingByDir.set(top, (missingByDir.get(top) || 0) + 1);
  }
}

const coverage = total === 0 ? 100 : (documented / total) * 100;

if (asJson) {
  console.log(JSON.stringify({
    mode: publicOnly ? 'public-only' : 'all',
    files: files.length,
    total,
    documented,
    missing: missing.length,
    coverage: Math.round(coverage * 10) / 10,
    missingByDir: Object.fromEntries([...missingByDir.entries()].sort((a, b) => b[1] - a[1])),
    missing: missing.slice(0, topN),
  }, null, 1));
} else {
  console.log(`symbol docs (${publicOnly ? 'public-only' : 'all'}): ${documented}/${total} documented (${coverage.toFixed(1)}%) across ${files.length} files`);
  console.log('missing by dir:');
  for (const [d, n] of [...missingByDir.entries()].sort((a, b) => b[1] - a[1]).slice(0, 20)) {
    console.log(`  ${String(n).padStart(5)}  ${d}`);
  }
  console.log(`first ${Math.min(topN, missing.length)} missing:`);
  for (const m of missing.slice(0, topN)) {
    console.log(`  MISSING ${m.file}:${m.line} ${m.name} (${m.kind}) :: ${m.detail}`);
  }
  if (missing.length > topN) console.log(`  ... +${missing.length - topN} more (use --top N / --json)`);
}

if (strict && missing.length > 0) process.exit(1);
