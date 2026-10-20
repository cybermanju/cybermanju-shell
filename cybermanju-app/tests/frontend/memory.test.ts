// CyberManju OS — Memory.test (tests/frontend/memory.test.ts).
// Semantic memory — TS mirror of `crates/agent/src/memory.rs`.
// The scoring contract is shared across the FFI: same token rule, same
// keyword formula, same budgets. If these drift, browser recall and native
// recall rank the same store differently.
import { describe, expect, it } from 'vitest'
import {
  chunkMemoryText,
  cosine,
  keywordOverlap,
  LocalMemoryStore,
  MEMORY_MIN_SCORE,
  MEMORY_NUDGE_TURNS,
  MEMORY_RECALL_BUDGET_CHARS,
  MEMORY_TEXT_CAP_CHARS,
  memoryTokens,
  rankMemories,
  renderRecallBlock,
  shouldNudgeMemory,
  type MemoryStorage,
} from '../../src/utils/memory'
import type { AgentMemory } from '../../src/types'

const row = (id: string, text: string, embedding: number[] = [], uses = 0): AgentMemory => ({
  id,
  configId: 'cfg',
  text,
  embedding,
  dims: embedding.length,
  origin: 'remember',
  sessionId: null,
  uses,
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
})

describe('cosine', () => {
  it('scores identity/orthogonal/mismatch like the Rust core', () => {
    expect(cosine([1, 0], [1, 0])).toBeCloseTo(1, 6)
    expect(Math.abs(cosine([1, 0], [0, 1]))).toBeLessThan(1e-6)
    expect(cosine([1], [1, 2])).toBe(0)
    expect(cosine([], [])).toBe(0)
    expect(cosine([0, 0], [1, 1])).toBe(0)
  })
})

describe('tokens + overlap (FFI contract: lowercase, alnum, len >= 3)', () => {
  it('skips short words on both sides', () => {
    expect(memoryTokens('go to the ok store')).toEqual(['the', 'store'])
    expect(keywordOverlap('red blue green', 'RED boat')).toBe(1)
  })

  it('counts distinct query tokens only', () => {
    expect(keywordOverlap('postgres postgres pooling', 'postgres pooling guide')).toBe(2)
  })
})

describe('chunkMemoryText', () => {
  it('packs paragraphs and hard-splits monsters', () => {
    expect(chunkMemoryText('aaa\n\nbbb\n\nccc', 64)).toEqual(['aaa\n\nbbb\n\nccc'])
    // The floor is 64 chars (smaller chunks are useless) — split with real sizes.
    const paras = ['a'.repeat(40), 'b'.repeat(40), 'c'.repeat(40)].join('\n\n')
    expect(chunkMemoryText(paras, 64)).toHaveLength(3)
    const pieces = chunkMemoryText('x'.repeat(200), 64)
    expect(pieces.length).toBeGreaterThanOrEqual(3)
    expect(pieces.every(p => [...p].length <= 64)).toBe(true)
  })
})

describe('rankMemories (parity with Rust recall_rank)', () => {
  it('ranks vector hits above keyword hits', () => {
    const rows = [row('kw', 'the deployment uses kubernetes clusters'), row('vec', 'something unrelated', [1, 0])]
    const hits = rankMemories([1, 0], 'deployment kubernetes', rows, 3)
    expect(hits).toHaveLength(2)
    expect(hits[0].id).toBe('vec')
    expect(hits[0].score).toBeGreaterThanOrEqual(MEMORY_MIN_SCORE)
    expect(hits[1].score).toBeLessThan(0.7)
  })

  it('falls back to keywords on dim mismatch', () => {
    const hits = rankMemories([0.1, 0.2], 'postgres pooling', [row('a', 'postgres connection pooling', [0.1, 0.2, 0.3])], 3)
    expect(hits).toHaveLength(1)
    expect(hits[0].id).toBe('a')
  })

  it('drops noise and breaks ties by uses', () => {
    const rows = [
      row('old', 'postgres pooling'),
      { ...row('used', 'postgres pooling guide'), uses: 9 },
      { ...row('noise', 'completely different topic here'), uses: 99 },
    ]
    const hits = rankMemories(null, 'postgres pooling pain', rows, 3)
    expect(hits.map(h => h.id)).not.toContain('noise')
    expect(hits[0].id).toBe('used')
  })

  it('needs minimum overlap to fire', () => {
    expect(rankMemories(null, 'xyzzy frobnicate', [row('a', 'postgres pooling')], 3)).toEqual([])
  })
})

describe('renderRecallBlock', () => {
  it('is empty when nothing hit and bounded otherwise', () => {
    expect(renderRecallBlock([])).toBe('')
    const block = renderRecallBlock(
      [{ id: 'a', text: 'x'.repeat(500), score: 0.9, origin: 'remember', sessionId: null, updatedAt: 't' }],
      100,
    )
    expect(block).toContain('recalled memories')
    expect(block).toContain('truncated at 100 chars')
    expect([...block].length).toBeLessThan(MEMORY_RECALL_BUDGET_CHARS)
  })
})

describe('shouldNudgeMemory', () => {
  it('fires once for long memory-less runs', () => {
    expect(shouldNudgeMemory(8, false)).toBe(true)
    expect(shouldNudgeMemory(25, false)).toBe(true)
    expect(shouldNudgeMemory(7, false)).toBe(false)
    expect(shouldNudgeMemory(25, true)).toBe(false)
    expect(shouldNudgeMemory(Number.NaN, false)).toBe(false)
    expect(MEMORY_NUDGE_TURNS).toBe(8)
  })
})

describe('LocalMemoryStore', () => {
  const memStorage = (): MemoryStorage => {
    let rows: AgentMemory[] = []
    return { load: () => rows, save: all => { rows = all } }
  }

  it('remembers, recalls, and deletes within a config scope', () => {
    const store = new LocalMemoryStore(memStorage())
    const saved = store.remember('cfg', 'the staging deploy needs the vpn profile')
    expect(saved).not.toBeNull()
    expect(store.recall('cfg', 'staging deploy vpn')).toHaveLength(1)
    expect(store.recall('other', 'staging deploy vpn')).toEqual([])
    store.remove(saved!.id)
    expect(store.recall('cfg', 'staging deploy vpn')).toEqual([])
  })

  it('rejects empty text and caps notes', () => {
    const store = new LocalMemoryStore(memStorage())
    expect(store.remember('cfg', '   ')).toBeNull()
    const big = store.remember('cfg', 'y'.repeat(5000))
    expect([...big!.text].length).toBe(MEMORY_TEXT_CAP_CHARS)
  })

  it('ranks imported vectors even with zero keyword overlap', () => {
    const hits = rankMemories([1, 0], 'deployment notes', [row('v', 'unrelated note', [1, 0])], 3)
    expect(hits).toHaveLength(1)
    expect(hits[0].id).toBe('v')
    expect(hits[0].score).toBeCloseTo(1, 5)
  })
})
