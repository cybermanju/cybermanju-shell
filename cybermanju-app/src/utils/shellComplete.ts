// CyberManju OS — ShellComplete (src/utils/shellComplete.ts).
// Tab-completion + history + ghost auto-suggest helpers for the cybsh
// panels. Pure functions (vitest-covered); the components own the server
// round-trip (`completeShellLine`) and the DOM overlay.

export const CYBSH_HISTORY_KEY = 'cybermanju.cybsh.history'
const HISTORY_CAP = 500

/** Index where the word under `caret` starts (whitespace boundary). */
export function wordStart(line: string, caret: number = line.length): number {
  const upto = line.slice(0, caret)
  const m = upto.match(/[^\s]*$/)
  return m ? caret - m[0].length : caret
}

/** The word under `caret` (empty when the caret sits after whitespace). */
export function currentWord(line: string, caret: number = line.length): string {
  return line.slice(wordStart(line, caret), caret)
}

/** True when the caret is still inside the first token (verb position). */
export function isVerbPosition(line: string, caret: number = line.length): boolean {
  return wordStart(line, caret) === 0
}

/** Replace the word under `caret` with `replacement`. */
export function replaceWord(line: string, caret: number, replacement: string): { line: string; caret: number } {
  const start = wordStart(line, caret)
  const next = line.slice(0, start) + replacement + line.slice(caret)
  return { line: next, caret: start + replacement.length }
}

// ─── persistent history ─────────────────────────────────────────────────

export function loadHistory(): string[] {
  try {
    const raw = localStorage.getItem(CYBSH_HISTORY_KEY)
    if (!raw) return []
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.filter((v): v is string => typeof v === 'string').slice(-HISTORY_CAP)
  } catch {
    return []
  }
}

export function saveHistory(entries: string[]): void {
  try {
    localStorage.setItem(CYBSH_HISTORY_KEY, JSON.stringify(entries.slice(-HISTORY_CAP)))
  } catch {
    /* private mode — session history still works */
  }
}

export function pushHistory(entries: string[], line: string): string[] {
  if (entries[entries.length - 1] === line) return entries
  return [...entries, line].slice(-HISTORY_CAP)
}

/**
 * Walk history backwards for the previous entry starting with `prefix`
 * (excluding an exact match), starting *before* `fromIndex`.
 */
export function prevHistoryMatch(entries: string[], prefix: string, fromIndex: number): number {
  for (let i = Math.min(fromIndex, entries.length - 1); i >= 0; i--) {
    if (entries[i] !== prefix && entries[i].startsWith(prefix)) return i
  }
  return -1
}

/**
 * Walk history forwards for the next entry starting with `prefix`
 * (excluding an exact match), starting *after* `fromIndex`.
 */
export function nextHistoryMatch(entries: string[], prefix: string, fromIndex: number): number {
  for (let i = fromIndex; i < entries.length; i++) {
    if (entries[i] !== prefix && entries[i].startsWith(prefix)) return i
  }
  return -1
}

/**
 * Distinct past argument tokens starting with `word` (most recent first).
 * The client-side fallback when the server has no candidates — e.g. file
 * names and flags the user already typed.
 */
export function historyTokenHints(entries: string[], word: string): string[] {
  if (!word) return []
  const out: string[] = []
  const seen = new Set<string>()
  for (let i = entries.length - 1; i >= 0; i--) {
    for (const tok of entries[i].split(/\s+/)) {
      const clean = tok.replace(/^["']|["']$/g, '')
      if (clean.length > word.length && clean.startsWith(word) && !seen.has(clean)) {
        seen.add(clean)
        out.push(clean)
      }
    }
    if (out.length >= 12) break
  }
  return out
}

/**
 * Ghost auto-suggest: the remainder of the best full-line candidate, or ''
 * when there is nothing to offer. History wins over the static table.
 */
export function ghostSuffix(line: string, history: string[], tableHits: string[]): string {
  if (!line) return ''
  for (let i = history.length - 1; i >= 0; i--) {
    if (history[i].length > line.length && history[i].startsWith(line)) {
      return history[i].slice(line.length)
    }
  }
  const hit = tableHits.find((h) => h.length > line.length && h.startsWith(line))
  return hit ? hit.slice(line.length) : ''
}

// ─── unix terminal prompt + line editing ──────────────────────────────
// Pure helpers for the TerminalPanel's Unix-emulator chrome. The server
// prompt stays `cybsh> ` on the wire (`ShellResult.prompt`); what the user
// *sees* is a bash-style `user@host:cwd$` prompt with exit-status colour.

export const TERM_USER = 'cybsh'
export const TERM_HOST = 'os'

/** Shorten a volume cwd for the prompt (`/a/b/c` → `~/…` never; keep it literal, cap the tail). */
export function shortenCwd(cwd: string, max = 32): string {
  if (!cwd) return '/'
  if (cwd.length <= max) return cwd
  const parts = cwd.split('/').filter(Boolean)
  let tail = ''
  for (let i = parts.length - 1; i >= 0; i--) {
    const next = `/${parts[i]}${tail}`
    if (next.length > max - 1) break
    tail = next
  }
  return `…${tail || `/${parts[parts.length - 1]}`}`
}

/** Rendered prompt text, e.g. `cybsh@os:/providers/docs$`. The `$` turns `#`-ish red on failure via `ok`. */
export function formatPrompt(cwd: string, ok = true): string {
  void ok
  return `${TERM_USER}@${TERM_HOST}:${shortenCwd(cwd)}`
}

/** True when server output carries a clear-screen sequence (`clear` verb). */
export function containsClearSequence(output: string): boolean {
  return output.includes('\x1b[2J') || output.includes('\x1b[H') || output.includes('\x1b[2J\x1b[H')
}

/** Strip clear-screen / cursor-home sequences so they never render as glyphs. */
export function stripClearSequences(output: string): string {
  return output.replace(/\x1b\[(2J|H|3J)/g, '')
}

/**
 * Bash-style history expansion for `!!`, `!n`, `!-n`, `!prefix` and `!$`.
 * Returns `{ line }` untouched when there is nothing to expand, or
 * `{ error }` when the reference resolves to nothing (like bash's
 * `event not found`).
 */
export function expandHistory(line: string, history: string[]): { line: string; error?: string; expanded?: boolean } {
  const trimmed = line.trim()
  if (!trimmed.startsWith('!') || trimmed.startsWith('!=')) return { line }
  if (trimmed === '!!') {
    const last = history[history.length - 1]
    if (!last) return { line, error: 'event not found: !! (no history yet)' }
    return { line: last, expanded: true }
  }
  let m = /^!-(\d+)$/.exec(trimmed)
  if (m) {
    const n = Number(m[1])
    const hit = history[history.length - n]
    if (!hit) return { line, error: `event not found: !-${n}` }
    return { line: hit, expanded: true }
  }
  m = /^!(\d+)$/.exec(trimmed)
  if (m) {
    const n = Number(m[1])
    if (n < 1 || n > history.length) return { line, error: `event not found: !${n}` }
    return { line: history[n - 1], expanded: true }
  }
  if (trimmed === '!$') {
    const last = history[history.length - 1]
    if (!last) return { line, error: 'event not found: !$ (no history yet)' }
    const toks = last.trim().split(/\s+/)
    return { line: toks[toks.length - 1], expanded: true }
  }
  m = /^!(\S+)$/.exec(trimmed)
  if (m) {
    const prefix = m[1]
    for (let i = history.length - 1; i >= 0; i--) {
      if (history[i].startsWith(prefix)) return { line: history[i], expanded: true }
    }
    return { line, error: `event not found: !${prefix}` }
  }
  return { line }
}

/**
 * Track a volume `cd` client-side so the prompt updates without an extra
 * `pwd` round-trip. Returns the new cwd, or `null` when the line is not a
 * plain `cd <path>` (the server stays authoritative — refresh from `pwd`
 * after anything else).
 */
export function trackCd(line: string, cwd: string): string | null {
  const m = /^\s*cd(?:\s+(?:"([^"]+)"|'([^']+)'|(\S+)))?\s*$/.exec(line)
  if (!m) return null
  const raw = m[1] ?? m[2] ?? m[3] ?? '/'
  if (raw === '-') return null // `cd -` needs server state — fall back to pwd
  if (raw.startsWith('/')) return raw === '/' ? '/' : `/${raw.split('/').filter(Boolean).join('/')}`
  const parts = cwd.split('/').filter(Boolean)
  for (const seg of raw.split('/')) {
    if (!seg || seg === '.') continue
    if (seg === '..') parts.pop()
    else parts.push(seg)
  }
  return `/${parts.join('/')}`
}

// ─── tab completion tables (offline-capable) ────────────────────────────
// Mirrors the server command table (`crates/os/src/shell.rs`
// `command_table()` + `completions()` sub-commands) so Tab always has
// verb candidates — even when the `os_complete` round-trip fails
// (offline, wasm still loading, swallowed transport error). The server
// stays authoritative when reachable; these are merged in, never alone.

/** Every cybsh verb, in help order (mirror of `command_table()`). */
export const CYBSH_VERBS: readonly string[] = [
  'help', 'history', 'clear', 'version', 'echo',
  'ls', 'cd', 'pwd', 'cat', 'cp', 'mv', 'rm', 'mkdir', 'touch',
  'stat', 'du', 'df', 'mount', 'umount', 'disk',
  'providers', 'quota', 'oauth', 'sync', 'scrub', 'repair', 'gc', 'lease',
  'ps', 'top', 'kill', 'jobs', 'compute', 'workers',
  'keygen', 'encrypt', 'decrypt', 'compress', 'decompress',
  'search', 'grep', 'find', 'head', 'tail', 'wc', 'write', 'edit',
  'ai', 'run', 'cron', 'theme', 'ui',
]

/** Sub-commands + flags the server completes (mirror of `completions()`). */
export const CYBSH_SUBS: readonly string[] = [
  'disk create', 'disk attach', 'disk detach', 'disk resize', 'disk list', 'disk check',
  'ai ask', 'ai init', 'ai status', 'ai abort', 'ai sessions',
  'oauth status', 'oauth start',
  'sync start', 'sync status', 'sync list', 'sync cancel', 'sync move',
  'compute run', 'lease status',
  'cron ls', 'cron add', 'cron rm', 'cron run', 'cron enable', 'cron disable', 'cron history',
  'history clear',
  'ui theme', 'ui accent', 'ui density', 'ui glass', 'ui motion', 'ui glow',
  'ui vars', 'ui palette', 'ui get', 'theme get',
  'run --dry', 'run --lint', 'run --fmt', 'run --json', 'run --record', 'run --replay',
]

/** Verbs + sub-commands starting with `prefix` (sorted, deduped). */
export function localCompletions(prefix: string): string[] {
  if (!prefix) return [...CYBSH_VERBS]
  const out: string[] = []
  for (const c of [...CYBSH_VERBS, ...CYBSH_SUBS]) {
    if (c.startsWith(prefix) && !out.includes(c)) out.push(c)
  }
  return out.sort()
}

/** Longest common prefix of the candidates (readline-style Tab extension). */
export function commonPrefix(cands: string[]): string {
  if (cands.length === 0) return ''
  let prefix = cands[0]
  for (let i = 1; i < cands.length; i++) {
    let j = 0
    while (j < prefix.length && prefix[j] === cands[i][j]) j++
    prefix = prefix.slice(0, j)
    if (!prefix) break
  }
  return prefix
}

/** Split a completion word into the directory to list + the fragment. */
export function splitCompletionWord(word: string): { rawDir: string; frag: string } {
  const idx = word.lastIndexOf('/')
  if (idx < 0) return { rawDir: '', frag: word }
  return { rawDir: word.slice(0, idx) || '/', frag: word.slice(idx + 1) }
}

function normalizeParts(parts: string[]): string[] {
  const out: string[] = []
  for (const seg of parts) {
    if (!seg || seg === '.') continue
    if (seg === '..') out.pop()
    else out.push(seg)
  }
  return out
}

/** Resolve a completion dir against the volume cwd (`''` → cwd). */
export function resolveCompletionDir(rawDir: string, cwd: string): string {
  if (!rawDir) return cwd || '/'
  if (rawDir.startsWith('/')) {
    const parts = normalizeParts(rawDir.split('/'))
    return `/${parts.join('/')}`
  }
  const base = (cwd || '/').split('/').filter(Boolean)
  const parts = normalizeParts([...base, ...rawDir.split('/')])
  return `/${parts.join('/')}`
}

/** First verb of a shell line (`cd /do` → `cd`), for verb-aware completion. */
export function completionVerb(prefix: string): string {
  const first = prefix.trimStart().split(/\s+/)[0]
  return first ?? ''
}

/** True when the verb only accepts directories (`cd <Tab>` lists folders). */
export function wantsDirsOnly(prefix: string): boolean {
  return completionVerb(prefix) === 'cd'
}

/** Quote a file name for re-insertion when it needs it. */
export function quoteCompletionName(name: string): string {
  if (/[\s"'\\]/.test(name)) return `"${name.replace(/"/g, '\\"')}"`
  return name
}

/** Split rendered text into URL / path / plain runs for clickable output. */
export interface LinkPart {
  text: string
  url?: boolean
  path?: boolean
}

export function linkifyLine(text: string): LinkPart[] {
  const out: LinkPart[] = []
  const re = /(https?:\/\/[^\s<>"']+|\/(?:[\w.~+:@-]+(?:\/[\w.~+:@-]+)*)\/?)/g
  let cursor = 0
  let match: RegExpExecArray | null
  while ((match = re.exec(text)) !== null) {
    if (match.index > cursor) out.push({ text: text.slice(cursor, match.index) })
    const token = match[0]
    if (token.startsWith('http')) {
      // Trim trailing punctuation bash would not include in the URL.
      const clean = token.replace(/[.,;:!?)\]]+$/, '')
      out.push({ text: clean, url: true })
      const rest = token.slice(clean.length)
      if (rest) out.push({ text: rest })
    } else if (token.length > 1) {
      out.push({ text: token, path: true })
    } else {
      out.push({ text: token })
    }
    cursor = match.index + match[0].length
  }
  if (cursor < text.length) out.push({ text: text.slice(cursor) })
  if (!out.length) out.push({ text })
  return out
}
