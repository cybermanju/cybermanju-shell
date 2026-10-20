// CyberManju OS — Wiki symbol deep-view builder test (tests/frontend/wiki-symbols.test.ts).
// Role: pin extractSymbolCode (brace/indent block extraction, caps, honest
//   truncation markers), explainSymbol (data-only prose + provenance line)
//   and buildSymbolEntries (god ∪ top ∪ graphify merge, degree ordering).
// Exports: extraction, explanation, and entry-merge cases.
// Integrates: scripts/wiki-symbols.mjs (pure module, fs-free — readers are
//   injected).
// Contracts: code is verbatim source; truncation always carries a marker;
//   explanations never invent facts and end with the engine line.
import { describe, expect, it } from 'vitest'
import { buildSymbolEntries, explainSymbol, extractSymbolCode } from '../../scripts/wiki-symbols.mjs'

const RUST = `// CyberManju OS — demo (src/demo.rs).
// Role: demo file for extraction tests.

/// Hash-anchor check.
pub fn verify_anchor(text: &str, anchor: &str) -> bool {
    if anchor.len() < 16 {
        return false;
    }
    text.contains(anchor)
}

pub struct Vault {
    pub root: String,
}
`

const TS = `export function greet(name: string): string {
  return \`hello \${name}\`;
}

const unused = 1;
`

const PY = `def scan(path):
    out = []
    for f in path.iterdir():
        out.append(f)
    return out
`

describe('extractSymbolCode', () => {
  it('extracts a rust fn block by brace matching', () => {
    const r = extractSymbolCode(RUST, 5, 'src/demo.rs')
    expect(r.code).toContain('pub fn verify_anchor')
    expect(r.code).toContain('text.contains(anchor)')
    expect(r.code.trimEnd().endsWith('}')).toBe(true)
    expect(r.truncated).toBe(false)
  })

  it('extracts a typescript function block', () => {
    const r = extractSymbolCode(TS, 1, 'src/x.ts')
    expect(r.code).toContain('export function greet')
    expect(r.code).toContain('return `hello')
    expect(r.truncated).toBe(false)
  })

  it('extracts python by indentation', () => {
    const r = extractSymbolCode(PY, 1, 'scripts/x.py')
    expect(r.code).toContain('def scan(path):')
    expect(r.code).toContain('return out')
    expect(r.code).not.toContain('def other')
  })

  it('truncates oversized blocks with an honest marker', () => {
    const big = 'pub fn huge() {\n' + '    let x = 1;\n'.repeat(200) + '}\n'
    const r = extractSymbolCode(big, 1, 'src/big.rs')
    expect(r.truncated).toBe(true)
    expect(r.code).toContain('… (truncated — full source at src/big.rs:1)')
  })

  it('handles file-level L1 pointers (first non-blank)', () => {
    const r = extractSymbolCode('\n\n' + TS, 1, 'src/x.ts')
    expect(r.code).toContain('export function greet')
  })
})

describe('explainSymbol', () => {
  it('assembles only real data and ends with the engine line', () => {
    const text = explainSymbol({
      name: 'verify_anchor',
      kind: 'function',
      docs: 'Hash-anchor check.',
      file: 'src/demo.rs',
      line: 6,
      headerRole: 'demo file for extraction tests.',
      connections: [
        { name: 'blake3_hash', file: 'src/h.rs', line: 3, relation: 'calls', confidence: 'EXTRACTED', weight: 2, via: 'out' },
        { name: 'edit_trailer', file: 'src/e.rs', line: 9, relation: 'calls', confidence: 'EXTRACTED', weight: 1, via: 'in' },
      ],
      contracts: ['integrity:'],
      degree: 12,
    })
    expect(text).toContain('`verify_anchor` is a function declared at src/demo.rs:6')
    expect(text).toContain('Hash-anchor check.')
    expect(text).toContain('calls `blake3_hash` (extracted)')
    expect(text).toContain('reached from `edit_trailer` (extracted)')
    expect(text).toContain('degree 12')
    expect(text).toContain('`integrity:`')
    expect(text).toContain('engine: heuristic')
  })

  it('degrades honestly with no docs and no connections', () => {
    const text = explainSymbol({ name: 'x', kind: 'const', docs: '', file: 'a.ts', line: 1 })
    expect(text).toContain('constant declared at a.ts:1')
    expect(text).toContain('engine: heuristic')
  })
})

describe('buildSymbolEntries', () => {
  it('merges god ∪ top ∪ graphify symbols with real code, degree-first', () => {
    const collected = {
      godNodes: [{ name: 'vault_write', file: 'src/demo.rs', line: 12, degree: 20, community: 3 }],
      files: [
        {
          path: 'src/demo.rs',
          header: ['Role: demo file for extraction tests.'],
          symbols: [
            { name: 'verify_anchor', kind: 'function', docs: 'Hash-anchor check.', line: 5 },
            { name: 'Vault', kind: 'class', docs: '', line: 12 },
          ],
        },
      ],
      contracts: [['integrity:', ['src/demo.rs']]],
    }
    const symbolNodes = [
      {
        id: 'n1',
        name: 'verify_anchor',
        file: 'src/demo.rs',
        line: 5,
        degree: 30,
        community: 3,
        neighbors: [
          { name: 'vault_write', file: 'src/demo.rs', line: 12, relation: 'calls', confidence: 'EXTRACTED', weight: 2, via: 'out' },
        ],
      },
    ]
    const readFile = (f: string) => (f === 'src/demo.rs' ? RUST : null)
    const out = buildSymbolEntries(collected, symbolNodes, readFile)
    const names = out.map((e: { name: string }) => e.name)
    // Degree order: verify_anchor (30) before vault_write (20).
    expect(names[0]).toBe('verify_anchor')
    expect(names).toContain('vault_write')
    expect(names).toContain('Vault')
    const va = out[0]
    expect(va.code).toContain('pub fn verify_anchor')
    expect(va.docs).toBe('Hash-anchor check.')
    expect(va.connections[0].name).toBe('vault_write')
    expect(va.explanation).toContain('engine: heuristic')
    // god-only symbol gets extracted code too (line 14 → struct block).
    const vw = out.find((e: { name: string }) => e.name === 'vault_write')
    expect(vw.code).toContain('pub struct Vault')
  })

  it('caps the payload at 60 entries', () => {
    const many = Array.from({ length: 80 }, (_, i) => ({
      id: `n${i}`, name: `sym${i}`, file: 'src/demo.rs', line: 6, degree: 100 - i, community: 1, neighbors: [],
    }))
    const out = buildSymbolEntries({ godNodes: [], files: [], contracts: [] }, many, () => null)
    expect(out.length).toBe(60)
    expect(out[0].name).toBe('sym0')
  })
})
