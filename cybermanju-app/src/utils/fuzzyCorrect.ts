// CyberManju OS — FuzzyCorrect (src/utils/fuzzyCorrect.ts).
// Shared string-similarity helpers (mirrors crates/os shell.rs `distance`
// so the frontend corrects the same way the server suggests).

/** Levenshtein distance over chars (small inputs only). */
export function levenshtein(a: string, b: string): number {
  const ac = [...a]
  const bc = [...b]
  let prev: number[] = Array.from({ length: bc.length + 1 }, (_, j) => j)
  let cur: number[] = new Array<number>(bc.length + 1).fill(0)
  for (let i = 1; i <= ac.length; i++) {
    cur[0] = i
    for (let j = 1; j <= bc.length; j++) {
      const cost = ac[i - 1] === bc[j - 1] ? 0 : 1
      cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost)
    }
    const tmp = prev
    prev = cur
    cur = tmp
  }
  return prev[bc.length]
}

/**
 * Jaro–Winkler similarity in [0,1]. Prefix-friendly, so it ranks
 * `lss→ls` and truncated speech transcripts well where raw Levenshtein
 * ties. Case-insensitive by default.
 */
export function jaroWinkler(a: string, b: string, caseSensitive = false): number {
  const s1 = caseSensitive ? a : a.toLowerCase()
  const s2 = caseSensitive ? b : b.toLowerCase()
  if (s1 === s2) return 1
  const l1 = s1.length
  const l2 = s2.length
  if (l1 === 0 || l2 === 0) return 0
  const window = Math.max(Math.floor(Math.max(l1, l2) / 2) - 1, 0)
  const m1 = new Array<boolean>(l1).fill(false)
  const m2 = new Array<boolean>(l2).fill(false)
  let matches = 0
  for (let i = 0; i < l1; i++) {
    const lo = Math.max(0, i - window)
    const hi = Math.min(l2 - 1, i + window)
    for (let j = lo; j <= hi; j++) {
      if (!m2[j] && s1[i] === s2[j]) {
        m1[i] = true
        m2[j] = true
        matches++
        break
      }
    }
  }
  if (matches === 0) return 0
  let t = 0
  let k = 0
  for (let i = 0; i < l1; i++) {
    if (!m1[i]) continue
    while (!m2[k]) k++
    if (s1[i] !== s2[k]) t++
    k++
  }
  t = Math.floor(t / 2)
  const jaro = (matches / l1 + matches / l2 + (matches - t) / matches) / 3
  let prefix = 0
  for (let i = 0; i < Math.min(4, l1, l2); i++) {
    if (s1[i] === s2[i]) prefix++
    else break
  }
  return jaro + prefix * 0.1 * (1 - jaro)
}

export interface BestMatch {
  match: string
  distance: number
  score: number
}

/**
 * Best dictionary neighbour for `word`. Same acceptance rule as the
 * Rust `did_you_mean` helper (`d*3 <= max(len,3)`), ranked by
 * Jaro–Winkler so `lss→ls` beats `lss→ps`. Returns null when nothing
 * is close enough — callers must show the raw word instead of guessing.
 */
export function bestMatch(word: string, candidates: readonly string[]): BestMatch | null {
  const w = word.toLowerCase()
  let best: BestMatch | null = null
  for (const c of candidates) {
    const d = levenshtein(w, c.toLowerCase())
    if (d * 3 > Math.max(w.length, c.length, 3)) continue
    const score = jaroWinkler(w, c)
    if (!best || score > best.score || (score === best.score && d < best.distance)) {
      best = { match: c, distance: d, score }
    }
  }
  return best
}
