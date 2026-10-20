// CyberManju OS — Transfer plan.test (tests/frontend/transfer-plan.test.ts).
// Transfer-board plan model: edges, validation, destination math, layout.
import { describe, expect, it } from 'vitest'
import {
  bytesEqual,
  clampPos,
  destPathFor,
  joinRemote,
  loadNodeLayout,
  makeEdge,
  overWriteCap,
  planKey,
  planSummaryText,
  saveNodeLayout,
  summarizePlan,
  toggleEdgeOp,
  TRANSFER_WRITE_CAP_BYTES,
  validateEdge,
} from '@/utils/transferPlan'

function edge(over: Record<string, unknown> = {}) {
  return makeEdge('e1', {
    sourceName: 'a.txt',
    sourceIsDir: false,
    from: { mountId: 'm1', remotePath: 'docs/a.txt' },
    to: { mountId: 'm2', remotePath: 'backup' },
    ...(over as object),
  } as never)
}

describe('transfer plan', () => {
  it('joins remote segments without doubled slashes', () => {
    expect(joinRemote('a/', '/b', 'c.txt')).toBe('a/b/c.txt')
    expect(joinRemote('', '')).toBe('')
    expect(joinRemote(undefined, 'x')).toBe('x')
  })

  it('resolves the destination as folder + source name', () => {
    expect(destPathFor(edge())).toBe('backup/a.txt')
    expect(destPathFor(edge({ to: { mountId: 'm2', remotePath: '' } }))).toBe('a.txt')
  })

  it('defaults to cp and toggles to mv and back', () => {
    expect(edge().op).toBe('cp')
    expect(toggleEdgeOp(edge()).op).toBe('mv')
    expect(toggleEdgeOp(toggleEdgeOp(edge())).op).toBe('cp')
  })

  it('builds a stable dedupe key', () => {
    expect(planKey(edge())).toBe(planKey(edge()))
    expect(planKey(edge())).not.toBe(planKey(edge({ to: { mountId: 'm3', remotePath: '' } })))
  })

  it('refuses the mount root as a source', () => {
    expect(validateEdge(edge({ from: { mountId: 'm1', remotePath: '' } }))).toMatch(/root/)
  })

  it('refuses same-path plans on one mount', () => {
    const same = edge({
      from: { mountId: 'm1', remotePath: 'docs/a.txt' },
      to: { mountId: 'm1', remotePath: 'docs' },
    })
    expect(validateEdge(same)).toMatch(/same path/)
  })

  it('refuses a folder aimed into its own subtree', () => {
    const intoSelf = edge({
      sourceIsDir: true,
      sourceName: 'photos',
      from: { mountId: 'm1', remotePath: 'docs/photos' },
      to: { mountId: 'm1', remotePath: 'docs/photos/sub' },
    })
    expect(validateEdge(intoSelf)).toMatch(/into itself/)
  })

  it('allows a folder into a sibling folder on the same mount', () => {
    const sibling = edge({
      sourceIsDir: true,
      sourceName: 'photos',
      from: { mountId: 'm1', remotePath: 'docs/photos' },
      to: { mountId: 'm1', remotePath: 'docs/other' },
    })
    expect(validateEdge(sibling)).toBeNull()
  })

  it('allows a folder onto the same path across mounts', () => {
    const cross = edge({
      sourceIsDir: true,
      sourceName: 'photos',
      from: { mountId: 'm1', remotePath: 'docs/photos' },
      to: { mountId: 'm2', remotePath: 'docs' },
    })
    expect(validateEdge(cross)).toBeNull()
  })

  it('allows cross-mount plans', () => {
    expect(validateEdge(edge())).toBeNull()
  })

  it('summarizes copy/move/file/folder counts', () => {
    const s = summarizePlan([
      edge(),
      edge({ sourceIsDir: true, sourceName: 'photos' }),
      { ...edge(), id: 'e3', op: 'mv' },
    ])
    expect(s).toEqual({ edges: 3, cp: 2, mv: 1, files: 2, folders: 1 })
    expect(planSummaryText(s)).toContain('3 planned')
    expect(planSummaryText(s)).toContain('recursive')
  })

  it('describes the empty plan as guidance, not an error', () => {
    expect(planSummaryText(summarizePlan([]))).toMatch(/click a file node/)
  })

  it('clamps dragged nodes inside the canvas', () => {
    expect(clampPos({ x: -5, y: 9999 }, 800, 500)).toEqual({ x: 0, y: 460 })
    expect(clampPos({ x: 100, y: 100 }, 800, 500)).toEqual({ x: 100, y: 100 })
  })

  it('loads an empty layout without a browser and never throws on save', () => {
    expect(loadNodeLayout()).toEqual({})
    expect(() => saveNodeLayout({})).not.toThrow()
  })

  it('pins the write cap at 5 MiB', () => {
    expect(TRANSFER_WRITE_CAP_BYTES).toBe(5 * 1024 * 1024)
    expect(overWriteCap(TRANSFER_WRITE_CAP_BYTES)).toBe(false)
    expect(overWriteCap(TRANSFER_WRITE_CAP_BYTES + 1)).toBe(true)
    expect(overWriteCap(0)).toBe(false)
    expect(overWriteCap(Number.NaN)).toBe(false)
  })

  it('compares bytes for move verification', () => {
    expect(bytesEqual(new Uint8Array([1, 2, 3]), new Uint8Array([1, 2, 3]))).toBe(true)
    expect(bytesEqual(new Uint8Array([1, 2, 3]), new Uint8Array([1, 2, 4]))).toBe(false)
    expect(bytesEqual(new Uint8Array([1, 2]), new Uint8Array([1, 2, 3]))).toBe(false)
    expect(bytesEqual(new Uint8Array([]), new Uint8Array([]))).toBe(true)
  })
})
