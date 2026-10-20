// CyberManju OS — Agent diff.test (tests/frontend/agent-diff.test.ts).
// Approval diff view — unified pre/post rendering for edit approvals.
import { describe, expect, it } from 'vitest'
import { diffBlocks, editBlocksOf } from '../../src/utils/agentDiff'

describe('diffBlocks', () => {
  it('marks identical blocks as pure context', () => {
    const d = diffBlocks('a\nb', 'a\nb')
    expect(d.lines.every(l => l.kind === 'ctx')).toBe(true)
  })

  it('emits del/add rows for a changed line', () => {
    const d = diffBlocks('a\nold\nc', 'a\nnew\nc')
    expect(d.lines.some(l => l.kind === 'del' && l.text === 'old')).toBe(true)
    expect(d.lines.some(l => l.kind === 'add' && l.text === 'new')).toBe(true)
    expect(d.lines.some(l => l.kind === 'ctx' && l.text === 'a')).toBe(true)
  })

  it('reports line counts', () => {
    const d = diffBlocks('a', 'a\nb')
    expect(d.oldLines).toBe(1)
    expect(d.newLines).toBe(2)
  })

  it('collapses long unchanged runs', () => {
    const old = Array.from({ length: 30 }, (_, i) => `line ${i}`).join('\n')
    const next = old.replace('line 15', 'CHANGED')
    const d = diffBlocks(old, next)
    expect(d.lines.length).toBeLessThan(30)
    expect(d.lines.some(l => l.text.includes('unchanged lines'))).toBe(true)
  })
})

describe('editBlocksOf', () => {
  it('extracts edit blocks from approval input', () => {
    expect(editBlocksOf({ old_block: 'a', new_block: 'b' })).toEqual({ oldBlock: 'a', newBlock: 'b' })
  })

  it('returns null for non-edit tools', () => {
    expect(editBlocksOf({ path: '/x' })).toBeNull()
    expect(editBlocksOf({ old_block: '', new_block: '' })).toBeNull()
    expect(editBlocksOf(null)).toBeNull()
  })
})
