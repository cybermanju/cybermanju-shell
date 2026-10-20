// CyberManju — approval diff view (AGENT-REVIEW P2.1).
//
// Pure unified-diff helper for the `edit` approval card: what the model
// wants to replace (pre) vs what it wants to write (post). Line-based LCS
// on a capped window — good enough for a human decision, never shipped to
// the provider. Side-effect free and unit-tested.

export interface DiffLine {
  kind: 'ctx' | 'del' | 'add'
  text: string
}

export interface ApprovalDiff {
  lines: DiffLine[]
  truncated: boolean
  oldLines: number
  newLines: number
}

const MAX_LINES = 120
const MAX_CHARS = 12_000

function splitCapped(text: string): { lines: string[]; truncated: boolean } {
  const src = String(text ?? '')
  const body = src.length > MAX_CHARS ? src.slice(0, MAX_CHARS) : src
  return { lines: body.split('\n').slice(0, MAX_LINES), truncated: src.length > MAX_CHARS }
}

/**
 * Minimal unified diff between `oldBlock` and `newBlock`.
 * 2 lines of context around each hunk; empty input → empty diff.
 */
export function diffBlocks(oldBlock: string, newBlock: string): ApprovalDiff {
  const a = splitCapped(oldBlock)
  const b = splitCapped(newBlock)
  const oldLines = String(oldBlock ?? '').split('\n').length
  const newLines = String(newBlock ?? '').split('\n').length
  const truncated = a.truncated || b.truncated
  const A = a.lines
  const B = b.lines
  if (A.join('\n') === B.join('\n')) {
    return { lines: A.map(text => ({ kind: 'ctx' as const, text })), truncated, oldLines, newLines }
  }
  // LCS table on the capped window.
  const n = A.length
  const m = B.length
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0))
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = A[i] === B[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1])
    }
  }
  const raw: DiffLine[] = []
  let i = 0
  let j = 0
  while (i < n && j < m) {
    if (A[i] === B[j]) {
      raw.push({ kind: 'ctx', text: A[i] })
      i++
      j++
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      raw.push({ kind: 'del', text: A[i++] })
    } else {
      raw.push({ kind: 'add', text: B[j++] })
    }
  }
  while (i < n) raw.push({ kind: 'del', text: A[i++] })
  while (j < m) raw.push({ kind: 'add', text: B[j++] })
  // Collapse long context runs to 2 lines around changes.
  const out: DiffLine[] = []
  const isChange = (l: DiffLine): boolean => l.kind !== 'ctx'
  let k = 0
  while (k < raw.length) {
    if (!isChange(raw[k])) {
      let run = 0
      while (k + run < raw.length && !isChange(raw[k + run])) run++
      const prevChange = out.length > 0 && isChange(out[out.length - 1])
      const nextChange = raw.slice(k + run).some(isChange)
      if (!prevChange && !nextChange) {
        k += run
        continue
      }
      if (run > 4 && (prevChange || nextChange)) {
        if (prevChange) {
          out.push(raw[k], raw[k + 1])
        }
        out.push({ kind: 'ctx', text: `… ${run - (prevChange ? 2 : 0) - (nextChange ? 2 : 0)} unchanged lines …` })
        if (nextChange) {
          out.push(raw[k + run - 2], raw[k + run - 1])
        }
        k += run
        continue
      }
      for (let t = 0; t < run; t++) out.push(raw[k + t])
      k += run
      continue
    }
    out.push(raw[k++])
  }
  return { lines: out, truncated, oldLines, newLines }
}

/** True when an approval input carries an edit worth diffing. */
export function editBlocksOf(input: unknown): { oldBlock: string; newBlock: string } | null {
  const obj = (input ?? {}) as Record<string, unknown>
  if (typeof obj.old_block !== 'string' || typeof obj.new_block !== 'string') return null
  if (!obj.old_block && !obj.new_block) return null
  return { oldBlock: obj.old_block, newBlock: obj.new_block }
}
