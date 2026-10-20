// CyberManju OS — wiki graph HTML builder tests (tests/frontend/wiki-graph-html.test.ts).
// Role: pin the symbol-level graph.html builder so the 2D view never
//   regresses to "Community {n}" meta-circles — every node label must be a
//   real symbol name or subject title, and docs must ride the payload.
// Integrates: imports scripts/wiki-graph-html.mjs (pure, node-env safe).
// Contracts: vitest node env; no DOM, no network.

import { describe, expect, it } from 'vitest';
import { buildGraphHtml, pickGraphSymbols } from '../../scripts/wiki-graph-html.mjs';

const SUBJECTS = [
  {
    id: 'agent',
    title: 'Agent System',
    tag: 'loop',
    color: '#a855f7',
    shape: 'knot',
    glyph: '🤖',
    paths: ['crates/agent/'],
    blurb: 'The agentic loop.',
  },
  {
    id: 'crypto',
    title: 'Crypto',
    tag: 'PQ',
    color: '#ef4444',
    shape: 'diamond',
    glyph: '💎',
    paths: ['crates/crypto/'],
    blurb: 'Post-quantum crypto.',
  },
];

function makeCollected() {
  return [
    {
      sub: SUBJECTS[0],
      files: [
        {
          path: 'crates/agent/src/loop.rs',
          header: ['Agent loop'],
          symbols: [
            { name: 'run_agent', kind: 'function', docs: 'Runs the agent loop.', line: 10 },
            { name: 'AgentConfig', kind: 'struct', docs: '', line: 40 },
          ],
          imports: [],
        },
      ],
      fileCount: 3,
      symbolCount: 2,
      contracts: [],
      links: ['crypto'],
      edges: [
        {
          to: 'crypto',
          weight: 5,
          extracted: 3,
          inferred: 2,
          relations: { calls: 3, imports: 2 },
          sample: 'calls encrypt → crypto',
        },
      ],
      godNodes: [{ name: 'run_agent', degree: 42, file: 'crates/agent/src/loop.rs', line: 10 }],
    },
    {
      sub: SUBJECTS[1],
      files: [
        {
          path: 'crates/crypto/src/lib.rs',
          header: ['Crypto'],
          symbols: [{ name: 'encrypt', kind: 'function', docs: 'ChaCha20 encrypt.', line: 5 }],
          imports: [],
        },
      ],
      fileCount: 2,
      symbolCount: 1,
      contracts: [],
      links: ['agent'],
      edges: [],
      godNodes: [],
    },
  ];
}

function makeKg() {
  const subjectGods = new Map([
    ['agent', [{ name: 'run_agent', degree: 42, file: 'crates/agent/src/loop.rs', line: 10, subject: 'agent', community: 3 }]],
    ['crypto', [{ name: 'encrypt', degree: 18, file: 'crates/crypto/src/lib.rs', line: 5, subject: 'crypto', community: 7 }]],
  ]);
  const subjectSymbols = new Map([
    [
      'agent',
      [
        {
          id: 'a1',
          name: 'run_agent',
          file: 'crates/agent/src/loop.rs',
          line: 10,
          degree: 42,
          community: 3,
          neighbors: [
            { name: 'encrypt', file: 'crates/crypto/src/lib.rs', line: 5, relation: 'calls', confidence: 'EXTRACTED', weight: 3, via: 'out' },
          ],
        },
        {
          id: 'a2',
          name: 'AgentConfig',
          file: 'crates/agent/src/loop.rs',
          line: 40,
          degree: 8,
          community: 3,
          neighbors: [],
        },
      ],
    ],
    [
      'crypto',
      [
        {
          id: 'c1',
          name: 'encrypt',
          file: 'crates/crypto/src/lib.rs',
          line: 5,
          degree: 18,
          community: 7,
          neighbors: [
            { name: 'run_agent', file: 'crates/agent/src/loop.rs', line: 10, relation: 'calls', confidence: 'EXTRACTED', weight: 3, via: 'in' },
          ],
        },
      ],
    ],
  ]);
  const subjectCommunity = new Map([
    ['agent', { id: 3, nodes: 2 }],
    ['crypto', { id: 7, nodes: 1 }],
  ]);
  return {
    engine: 'graphify',
    commit: null,
    stats: { nodes: 3, edges: 2, mapped: 1, extracted: 2, inferred: 0, communities: 2, subjects: 2 },
    subjectGods,
    subjectSymbols,
    subjectCommunity,
    godNodes: [],
    communities: [],
    paths: {},
  };
}

describe('pickGraphSymbols', () => {
  it('prefers god nodes and caps per subject', () => {
    const picked = pickGraphSymbols(makeCollected(), makeKg(), 14);
    expect(picked.get('agent')).toHaveLength(2);
    expect(picked.get('agent')![0].name).toBe('run_agent');
    expect(picked.get('agent')![0].god).toBe(true);
    expect(picked.get('crypto')).toHaveLength(1);
    expect(picked.get('crypto')![0].name).toBe('encrypt');
  });

  it('attaches heuristic docs to matching symbol names', () => {
    const picked = pickGraphSymbols(makeCollected(), makeKg(), 14);
    expect(picked.get('agent')![0].docs).toBe('Runs the agent loop.');
    expect(picked.get('agent')![1].docs).toBe('');
  });

  it('respects the per-subject cap', () => {
    const picked = pickGraphSymbols(makeCollected(), makeKg(), 1);
    expect(picked.get('agent')).toHaveLength(1);
    expect(picked.get('agent')![0].name).toBe('run_agent');
  });
});

describe('buildGraphHtml', () => {
  it('labels every node with a real symbol or subject title — never Community {n}', () => {
    const html = buildGraphHtml({
      subjects: SUBJECTS,
      collected: makeCollected(),
      kg: makeKg(),
      version: '0.0.0-test',
      commit: null,
    });
    expect(html).toContain('Agent System');
    expect(html).toContain('Crypto');
    expect(html).toContain('run_agent');
    expect(html).toContain('encrypt');
    expect(html).not.toMatch(/Community \d+/);
  });

  it('embeds doc-comments in the payload and info panel', () => {
    const html = buildGraphHtml({
      subjects: SUBJECTS,
      collected: makeCollected(),
      kg: makeKg(),
      version: '0.0.0-test',
      commit: null,
    });
    expect(html).toContain('Runs the agent loop.');
    expect(html).toContain('ChaCha20 encrypt.');
    expect(html).toContain('Docs:');
  });

  it('applies per-subject vis-network silhouettes', () => {
    const html = buildGraphHtml({
      subjects: SUBJECTS,
      collected: makeCollected(),
      kg: makeKg(),
      version: '0.0.0-test',
      commit: null,
    });
    // agent shape knot → star; crypto shape diamond → diamond.
    expect(html).toContain('"shape":"star"');
    expect(html).toContain('"shape":"diamond"');
    // Module hubs present.
    expect(html).toContain('"kind":"module"');
    expect(html).toContain('"kind":"god"');
    expect(html).toContain('"kind":"symbol"');
  });

  it('carries EXTRACTED/INFERRED provenance on cross-subject edges', () => {
    const html = buildGraphHtml({
      subjects: SUBJECTS,
      collected: makeCollected(),
      kg: makeKg(),
      version: '0.0.0-test',
      commit: null,
    });
    expect(html).toContain('EXTRACTED 3 / INFERRED 2');
    expect(html).toContain('calls ×3');
  });

  it('is self-contained static HTML with SRI-pinned vis-network', () => {
    const html = buildGraphHtml({
      subjects: SUBJECTS,
      collected: makeCollected(),
      kg: makeKg(),
      version: '0.0.0-test',
      commit: 'abc123',
    });
    expect(html).toContain('<!DOCTYPE html>');
    expect(html).toContain('vis-network@9.1.6');
    expect(html).toContain('integrity="sha384-');
    expect(html).toContain('crossorigin="anonymous"');
    expect(html).toContain('0.0.0-test');
    expect(html).toContain('abc123');
    expect(html).toContain('symbol level');
  });

  it('escapes HTML-special characters in labels and tooltips', () => {
    const subjects = [
      { ...SUBJECTS[0], title: 'A<b>&"x"' },
      SUBJECTS[1],
    ];
    const html = buildGraphHtml({
      subjects,
      collected: makeCollected(),
      kg: makeKg(),
      version: '0.0.0-test',
      commit: null,
    });
    // JSON payload uses \u003c for `<` so a hostile label can never close
    // the <script> tag; the raw angle bracket must not appear unescaped.
    expect(html).toContain('\\u003c');
    expect(html).not.toMatch(/A<b>/);
  });
});
