// CyberManju OS — Wiki knowledge-graph aggregator test (tests/frontend/wiki-graph.test.ts).
// Role: pin the graphify graph.json → subject-level fold: provenance split
//   (EXTRACTED never merges with INFERRED), symmetric cross-subject edges,
//   god-node ranking, dominant communities, all-pairs module paths, and the
//   symbol-level neighbor fold (subjectSymbols) the SPA deep view consumes.
// Exports: subject matching, edge aggregation, god node, community, path cases.
// Integrates: scripts/wiki-graph.mjs (pure module, no fs/network).
// Contracts: engine stays "graphify"; unmapped files contribute no edges;
//   same-subject edges never surface as cross-subject links.
import { describe, expect, it } from 'vitest'
import { aggregateGraph, subjectOfFile } from '../../scripts/wiki-graph.mjs'

const SUBJECTS = [
  { id: 'alpha', paths: ['src/alpha/'] },
  { id: 'beta', paths: ['src/beta/'] },
  { id: 'gamma', paths: ['src/gamma/'] },
]

const node = (id: string, file: string, community: number, label = id) => ({
  id, label, source_file: file, source_location: 'L10', community,
})

describe('subjectOfFile', () => {
  it('matches exact paths and prefixes', () => {
    expect(subjectOfFile('src/alpha/', SUBJECTS)).toBe('alpha')
    expect(subjectOfFile('src/alpha/mod.rs', SUBJECTS)).toBe('alpha')
    expect(subjectOfFile('src/other/mod.rs', SUBJECTS)).toBe(null)
  })
})

describe('aggregateGraph', () => {
  const graph = {
    built_at_commit: 'abc1234',
    nodes: [
      node('a1', 'src/alpha/one.rs', 7),
      node('a2', 'src/alpha/two.rs', 7),
      node('b1', 'src/beta/one.rs', 9),
      node('c1', 'src/gamma/one.rs', 9),
      node('out', 'src/elsewhere/x.rs', 1),
    ],
    links: [
      // alpha → beta: two EXTRACTED calls + one INFERRED reference.
      { source: 'a1', target: 'b1', relation: 'calls', confidence: 'EXTRACTED', weight: 2 },
      { source: 'a2', target: 'b1', relation: 'calls', confidence: 'EXTRACTED', weight: 1 },
      { source: 'a1', target: 'b1', relation: 'references', confidence: 'INFERRED', weight: 1 },
      // alpha → gamma via beta (path material).
      { source: 'b1', target: 'c1', relation: 'imports', confidence: 'EXTRACTED', weight: 5 },
      // Same-subject edge: must NOT become a cross-subject link.
      { source: 'a1', target: 'a2', relation: 'calls', confidence: 'EXTRACTED', weight: 9 },
      // Unmapped endpoint: must NOT count anywhere cross-subject.
      { source: 'a1', target: 'out', relation: 'calls', confidence: 'EXTRACTED', weight: 3 },
    ],
  }

  const kg = aggregateGraph(graph, SUBJECTS)

  it('labels the engine honestly and keeps provenance split', () => {
    expect(kg.engine).toBe('graphify')
    expect(kg.commit).toBe('abc1234')
    // All 6 edges counted globally; 4 of them cross-subject-mapped
    // (3× alpha↔beta + 1× beta↔gamma; intra-alpha and the unmapped
    // endpoint don't count).
    expect(kg.stats.edges).toBe(6)
    expect(kg.stats.mapped).toBe(4)
    expect(kg.stats.extracted).toBe(5)
    expect(kg.stats.inferred).toBe(1)
  })

  it('aggregates symmetric cross-subject edges strongest-first', () => {
    const alpha = kg.subjectEdges.get('alpha')!
    // Only the direct alpha↔beta edge; gamma is reachable via beta (paths).
    expect(alpha.map((e) => e.to)).toEqual(['beta'])
    const beta = alpha[0]
    expect(beta.weight).toBe(4) // 2 + 1 + 1
    expect(beta.extracted).toBe(2) // 2 EXTRACTED edge objects
    expect(beta.inferred).toBe(1)
    expect(beta.relations).toEqual({ calls: 2, references: 1 })
    expect(beta.sample).toContain('calls')
    // Mirror side sees the same numbers.
    const mirror = kg.subjectEdges.get('beta')!.find((e) => e.to === 'alpha')!
    expect(mirror.weight).toBe(4)
    expect(mirror.extracted).toBe(2)
    expect(mirror.inferred).toBe(1)
  })

  it('ranks god nodes by weighted degree, only inside subjects', () => {
    const ids = kg.godNodes.map((g) => g.name)
    expect(ids).not.toContain('out') // unmapped file: not a subject god
    // a1: 2+1+1 (alpha↔beta) + 9 (intra-alpha) + 3 (unmapped) = 15.
    expect(kg.godNodes[0].name).toBe('a1')
    expect(kg.godNodes[0].degree).toBe(15)
    expect(kg.godNodes[0].subject).toBe('alpha')
    expect(kg.subjectGods.get('beta')!.map((g) => g.name)).toEqual(['b1'])
  })

  it('assigns the dominant community per subject and groups them', () => {
    expect(kg.subjectCommunity.get('alpha')).toEqual({ id: 7, nodes: 2 })
    expect(kg.subjectCommunity.get('beta')!.id).toBe(9)
    const group = kg.communities.find((c) => c.id === 9)!
    expect(group.subjects.sort()).toEqual(['beta', 'gamma'])
    expect(group.size).toBe(2)
  })

  it('computes shortest module paths (and skips other components)', () => {
    expect(kg.paths['alpha>beta']).toEqual(['alpha', 'beta'])
    expect(kg.paths['alpha>gamma']).toEqual(['alpha', 'beta', 'gamma'])
    expect(kg.paths['gamma>alpha']).toEqual(['gamma', 'beta', 'alpha'])
    expect(kg.paths['alpha>alpha']).toBeUndefined()
  })

  it('folds symbol-level neighbors per subject (real repo edges)', () => {
    const alpha = kg.subjectSymbols.get('alpha')!
    expect(alpha.map((s) => s.name)).toEqual(['a1', 'a2']) // degree order
    const a1 = alpha[0]
    expect(a1.degree).toBe(15)
    expect(a1.file).toBe('src/alpha/one.rs')
    // a1's neighbors: b1 via the EXTRACTED calls (w2), the INFERRED
    // references (w1) and the unmapped edge (w3) — strongest first, cap 6.
    const nb = a1.neighbors.find((n) => n.name === 'b1')!
    expect(nb.relation).toBe('calls')
    expect(nb.confidence).toBe('EXTRACTED')
    expect(nb.weight).toBe(2)
    expect(nb.via).toBe('out')
    // Intra-subject neighbor a2 keeps direction out (a1 calls a2).
    const intra = a1.neighbors.find((n) => n.name === 'a2')!
    expect(intra.via).toBe('out')
    // Reverse edges surface with via 'in' (b1 is reached from a1/a2).
    const b1 = kg.subjectSymbols.get('beta')![0]
    const fromA1 = b1.neighbors.find((n) => n.name === 'a1' && n.via === 'in')!
    expect(fromA1.confidence).toBe('EXTRACTED')
    // Unmapped node 'out' contributes neighbors but is never a subject symbol.
    expect(kg.subjectSymbols.get('gamma')!.map((s) => s.name)).toEqual(['c1'])
    for (const list of kg.subjectSymbols.values()) {
      for (const s of list) expect(s.name).not.toBe('out')
    }
  })
})
