// CyberManju OS — Scene (src/utils/scene.ts).
// Scene heuristic matcher — TypeScript twin of `crates/scene`.
//
// Parses the SAME `crates/scene/data/scenes.json` tables the Rust classifier
// uses, with the same normalization, weights and thresholds, so desktop,
// server and WASM agree exactly. Honest by construction: scores come only
// from filename / path / user-tag text (plus known face-cluster membership),
// every score carries its matched keywords, and silence means no signal.

import tablesJson from '../../crates/scene/data/scenes.json'

interface SceneTables {
  parents: Record<string, string>
  categories: {
    id: string
    label: string
    keywords: Record<string, string[]>
  }[]
}

const TABLES = tablesJson as SceneTables

export interface SceneScore {
  category: string
  label: string
  /** 0..1, rounded to 2 decimals. */
  score: number
  hits: string[]
}

export interface SceneInput {
  fileName?: string
  tags?: string[]
  path?: string
  /** The item is already known to depict a person (face cluster hit). */
  hasFaces?: boolean
  mimeType?: string
}

const W_NAME = 1.0
const W_TAG = 0.95
const W_PATH = 0.7
const S_EXACT = 0.9
const S_SUB = 0.55
const S_TINY = 0.4
const MIN_REPORT = 0.3
const MAX_SCORE = 0.97

const LATIN_FOLD: Record<string, string> = {
  à: 'a', á: 'a', â: 'a', ã: 'a', ä: 'a', å: 'a', ā: 'a', ă: 'a', ą: 'a',
  è: 'e', é: 'e', ê: 'e', ë: 'e', ē: 'e', ĕ: 'e', ę: 'e',
  ì: 'i', í: 'i', î: 'i', ï: 'i', ĩ: 'i', ī: 'i',
  ò: 'o', ó: 'o', ô: 'o', õ: 'o', ö: 'o', ø: 'o', ō: 'o',
  ù: 'u', ú: 'u', û: 'u', ü: 'u', ũ: 'u', ū: 'u',
  ý: 'y', ÿ: 'y',
  ç: 'c', ć: 'c', č: 'c',
  ñ: 'n', ń: 'n',
  š: 's', ś: 's',
  ž: 'z', ź: 'z', ż: 'z',
  ß: 's', ł: 'l', æ: 'a', œ: 'o',
}

/** Lowercase + latin diacritic fold (Cyrillic/CJK/kana pass through). */
export function normScene(s: string): string {
  const decomposed = s
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
  return [...decomposed].map(c => LATIN_FOLD[c] ?? c).join('')
}

const SEPARATORS = new Set([
  ' ', '\t', '\n', '\r', '/', '\\', '_', '-', '.', ',',
  ';', ':', '(', ')', '[', ']', "'", '"',
])

export function splitSceneTokens(s: string): string[] {
  const tokens: string[] = []
  let cur = ''
  for (const c of s) {
    if (SEPARATORS.has(c)) {
      if (cur) {
        tokens.push(cur)
        cur = ''
      }
    } else {
      cur += c
    }
  }
  if (cur) tokens.push(cur)
  return tokens
}

function charLen(s: string): number {
  return [...s].length
}

function keywordSignal(keyword: string, tokens: string[], hay: string): number {
  if (!keyword) return 0
  if (keyword.includes(' ')) {
    return hay.includes(keyword) ? S_EXACT : 0
  }
  const kwLen = charLen(keyword)
  let best = 0
  for (const t of tokens) {
    if (t === keyword) return S_EXACT
    if (kwLen === 1) {
      if (t.includes(keyword)) best = Math.max(best, S_TINY)
    } else if (kwLen >= 4 && charLen(t) > kwLen) {
      // Mid-token containment ("cume" in "document") is noise, but
      // edge-aligned forms are real morphology ("praias", "beaches").
      if (t.startsWith(keyword) || t.endsWith(keyword)) {
        best = Math.max(best, S_SUB)
      }
    }
  }
  if (best === 0 && kwLen >= 2 && hay.includes(keyword) && isUnsegmented(hay)) {
    // Unsegmented scripts (CJK strings with no token boundaries at all):
    // raw containment is the only signal available. Latin single tokens
    // already had their edge-aligned chance above — "cume" in "document"
    // must stay silent.
    best = Math.min(S_SUB, S_EXACT - 0.1)
  }
  return best
}

/** CJK-style continuous script — the only case where raw containment is legitimate. */
function isUnsegmented(hay: string): boolean {
  return /[一-鿿぀-ヿ가-힯]/.test(hay)
}

function combine(acc: number, w: number, s: number): number {
  return Math.min(acc + w * s * (1 - acc), MAX_SCORE)
}

function isImageMime(mime: string): boolean {
  const m = mime.toLowerCase()
  return m.startsWith('image/') || m.includes('photo') || m.includes('picture')
}

const round2 = (v: number): number => Math.round(v * 100) / 100

/** Classify evidence → scene scores sorted by confidence (empty = no signal). */
export function classifyScene(input: SceneInput): SceneScore[] {
  const rawName = input.fileName ?? ''
  const stem = rawName.includes('.') ? rawName.slice(0, rawName.lastIndexOf('.')) : rawName
  const nameN = normScene(stem)
  const pathN = normScene(input.path ?? '')
  const tagsN = (input.tags ?? []).map(normScene)

  const nameTokens = splitSceneTokens(nameN)
  const pathTokens = splitSceneTokens(pathN)
  const tagTokens = tagsN.flatMap(splitSceneTokens)
  const tagsHay = tagsN.join(' ')

  const out: SceneScore[] = []
  for (const cat of TABLES.categories) {
    let acc = 0
    const hits: string[] = []
    const seen = new Set<string>()
    const langs = Object.keys(cat.keywords).sort()
    for (const lang of langs) {
      for (const kwRaw of cat.keywords[lang]) {
        const kw = normScene(kwRaw)
        const sName = keywordSignal(kw, nameTokens, nameN)
        const sTag = keywordSignal(kw, tagTokens, tagsHay)
        const sPath = keywordSignal(kw, pathTokens, pathN)
        const s = Math.max(W_NAME * sName, W_TAG * sTag, W_PATH * sPath)
        if (s > 0) {
          acc = combine(acc, 1, s)
          if (!seen.has(kw)) {
            seen.add(kw)
            hits.push(kwRaw)
          }
        }
      }
    }
    if (cat.id === 'human' && input.hasFaces) {
      acc = combine(acc, 1, 0.55)
      hits.push('face-cluster')
    }
    if (acc >= MIN_REPORT) {
      out.push({ category: cat.id, label: cat.label, score: round2(acc), hits })
    }
  }

  for (const s of [...out]) {
    const parent = TABLES.parents[s.category]
    if (parent && round2(s.score * 0.6) >= MIN_REPORT && !out.some(o => o.category === parent)) {
      const cat = TABLES.categories.find(c => c.id === parent)
      if (cat) {
        out.push({
          category: cat.id,
          label: cat.label,
          score: round2(s.score * 0.6),
          hits: [`inferred:${s.category}`],
        })
      }
    }
  }

  if (isImageMime(input.mimeType ?? '')) {
    for (const s of out) s.score = round2(Math.min(s.score + 0.02, MAX_SCORE))
  }

  out.sort((a, b) => b.score - a.score || (a.category < b.category ? -1 : 1))
  return out
}

/** Which scene categories does a free-text query name? (dynamic search) */
export function matchSceneQuery(query: string): SceneScore[] {
  return classifyScene({ fileName: query })
}

/** All category ids + labels (pickers, tests). */
export function sceneCategoryList(): [string, string][] {
  return TABLES.categories.map(c => [c.id, c.label])
}
