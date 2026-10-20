// CyberManju OS — `.cybsh` interpreter twin for the static/Pages transport.
//
// Same language as `crates/os/src/script.rs` (python-style `print` +
// `if/elif/else`, `let` with stripped TS annotations, `for`/`while`,
// inline `$ <cybsh>` / `sh "…"` lines, `js` expression subset, `fetch`,
// `ui`/`theme`, `vars`/`free`/`gc`, plus v2: `args`/`env()` inputs,
// `match`/`ok()`/`err()`/`unwrap()`/`?`, `|>` pipes, `import`, `with`,
// `await`, `fetch … method/headers/as json`, `# schedule:/# on:`
// frontmatter), same budgets. Differences are transport-only and honest:
// - `fetch` really runs here (browser `fetch`, 10 s timeout, 64 KiB cap);
//   the native shell answers `unsupported:` (no HTTP client in that crate).
// - `ui`/`theme` lines execute through the static verbs, whose output
//   carries `ui:` effect lines — harvested here so callers can apply them
//   via `useTheme()`.
// - `sh` lines run through the caller's `execCybsh` (static verbs first,
//   wasm dispatcher fallback), with the same 4-deep `run` nesting guard.
// - `-os` (host filesystem: sdcard on Android) has no browser equivalent —
//   `runStaticCybshLine` refuses it with `unsupported:` before dispatch so a
//   volume answer never masquerades as a host listing. `deny=<verb>`
//   capabilities cover `-os` variants (verb match).
// - `env()` reads `deps.env` (explicit map; falls back to `process.env`
//   under node, absent in browsers) — ambient, unjournaled.

export const CYBSH_SCRIPT_EXT = '.cybsh'
export const CYBSH_MAX_SOURCE_BYTES = 64 * 1024
export const CYBSH_MAX_STMTS = 200
export const CYBSH_MAX_STEPS = 5000
export const CYBSH_MAX_ITERS = 1000
export const CYBSH_MAX_VARS = 64
export const CYBSH_MAX_STR = 16 * 1024
export const CYBSH_MAX_LIST = 1024
export const CYBSH_MAX_OUTPUT_BYTES = 256 * 1024
export const CYBSH_MAX_RUN_DEPTH = 4
export const CYBSH_MAX_FUNCS = 32
export const CYBSH_MAX_CALL_DEPTH = 32
export const CYBSH_MAX_AWAIT_TRIES = 100
export const CYBSH_DEFAULT_AWAIT_TRIES = 10
export const CYBSH_MAX_IMPORT_DEPTH = 4
/** Language version pinned by `# cybsh: 1`. */
export const CYBSH_SCRIPT_VERSION = '1'

/** Statement + expression keywords (completion/LSP vocabulary). */
export const CYBSH_SCRIPT_KEYWORDS = [
  'print', 'let', 'const', 'if', 'elif', 'else', 'for', 'while', 'in', 'try', 'catch',
  'fail', 'def', 'return', 'match', 'ok', 'err', 'with', 'import', 'as', 'await',
  'timeout', 'fetch', 'method', 'headers', 'json', 'and', 'or', 'not', 'true',
  'false', 'null', 'args', 'env',
]

export interface CybshScriptEffect {
  kind: string
  detail: string
}

export interface CybshFrontmatter {
  schedule: string | null
  triggers: string[]
  description: string | null
}

export interface CybshScriptResult {
  output: string
  effects: CybshScriptEffect[]
  vars: number
  caps: {
    active: boolean
    net: string[] | null
    read: string[] | null
    write: string[] | null
    deny: string[]
  } | null
  shCalls: number
  fetchCalls: number
  schedule: string | null
  triggers: string[]
}

export type CybshExecFn = (line: string) => Promise<string> | string

/** One `fetch` request (journal key = `METHOD url`). */
export interface CybshFetchReq {
  url: string
  method: string
  headers: Record<string, string>
  wantJson: boolean
}

export function cybshFetchKey(req: CybshFetchReq): string {
  return `${req.method.toUpperCase()} ${req.url}`
}

export interface CybshScriptDeps {
  execCybsh: CybshExecFn
  /** Browser fetch for `fetch <url>`; absent → honest `unsupported:`. */
  fetchText?: (req: CybshFetchReq) => Promise<string>
  /** Host environment for `env()` (browsers: explicit map; node: `process.env` fallback). */
  env?: Record<string, string>
}

type Value =
  | { t: 'null' }
  | { t: 'bool'; v: boolean }
  | { t: 'num'; v: number }
  | { t: 'str'; v: string }
  | { t: 'list'; v: Value[] }
  // Dict keys stay sorted everywhere (display, iteration, `==`) so runs
  // are deterministic on every transport (Starlark rule).
  | { t: 'dict'; v: Map<string, Value> }

const vNull = (): Value => ({ t: 'null' })
const vBool = (v: boolean): Value => ({ t: 'bool', v })
const vNum = (v: number): Value => ({ t: 'num', v })
const vStr = (v: string): Value => ({ t: 'str', v })
const vList = (v: Value[]): Value => ({ t: 'list', v })
const vDict = (v: Map<string, Value>): Value => ({ t: 'dict', v })

/**
 * FNV-1a/64 over the UTF-8 bytes (non-crypto content tag for replay
 * journals). Same input bytes as the Rust twin, so fingerprints — and
 * therefore journals — are portable across transports. Pinned by the
 * `fnv("a") == af63dc4c8601ec8c` test vector on both sides.
 */
export function cybshFingerprint(source: string): string {
  const bytes = new TextEncoder().encode(source)
  // 64-bit state in two 32-bit halves (offset basis 0xcbf29ce484222325,
  // prime 0x100000001b3). The carry needs the FULL product — splitting
  // floor() across the halves drops sub-integer remainders that still push
  // the total over an integer boundary — so it is computed in exact double
  // arithmetic (< 2^41, well under 2^53) instead of 32-bit halves.
  let hi = 0xcbf29ce4
  let lo = 0x84222325
  for (const b of bytes) {
    lo ^= b
    const carry = Math.floor(((lo >>> 0) * 0x1b3) / 4294967296)
    const newLo = Math.imul(lo, 0x1b3)
    const newHi = (Math.imul(hi, 0x1b3) + Math.imul(lo, 0x100) + carry) | 0
    lo = newLo
    hi = newHi
  }
  const hex = (n: number): string => (n >>> 0).toString(16).padStart(8, '0')
  return hex(hi) + hex(lo)
}

/** First non-blank raw line may pin the language (`# cybsh: 1`). */
export function checkScriptVersion(source: string): void {
  for (const raw of source.split('\n')) {
    const t = raw.trim()
    if (!t) continue
    if (t.startsWith('#')) {
      const decl = t.slice(1).trim()
      const m = decl.match(/^cybsh\s*:\s*(\S+)/) ?? decl.match(/^cybsh\s+(\S+)/)
      if (m?.[1] && m[1] !== CYBSH_SCRIPT_VERSION) {
        throw new Error(
          `unsupported: script pins cybsh v${m[1]} but this shell speaks v${CYBSH_SCRIPT_VERSION}`,
        )
      }
    }
    return
  }
}

export interface CybshCaps {
  active: boolean
  net: string[] | null
  read: string[] | null
  write: string[] | null
  deny: string[]
}

/** Parse `# cap:` lines (`k=v` tokens, Deno-style least privilege). */
export function parseScriptCaps(source: string): CybshCaps {
  const caps: CybshCaps = { active: false, net: null, read: null, write: null, deny: [] }
  for (const raw of source.split('\n').slice(0, 200)) {
    const t = raw.trim()
    const rest = t.startsWith('# cap:') ? t.slice(6) : t.startsWith('#cap:') ? t.slice(5) : null
    if (rest === null) continue
    caps.active = true
    for (const tok of rest.split(/\s+/)) {
      const eq = tok.indexOf('=')
      if (eq < 0) continue
      const items = tok
        .slice(eq + 1)
        .split(',')
        .map((s) => s.trim())
        .filter((s) => s.length > 0)
      const key = tok.slice(0, eq)
      if (key === 'net') caps.net = items
      else if (key === 'read') caps.read = items
      else if (key === 'write') caps.write = items
      else if (key === 'deny') caps.deny.push(...items)
    }
  }
  return caps
}

function fetchHost(url: string): string {
  const after = url.includes('://') ? url.split('://')[1]! : url
  const hostPort = after.split('/')[0]!
  return (hostPort.includes('@') ? hostPort.split('@').pop()! : hostPort).toLowerCase()
}

export function describeScriptCaps(caps: CybshCaps): string {
  const parts: string[] = []
  if (caps.net) parts.push(`net=${caps.net.join(',')}`)
  if (caps.deny.length) parts.push(`deny=${caps.deny.join(',')}`)
  if (!parts.length) parts.push('declared')
  return parts.join(' ')
}

const BARE_VERBS = new Set([
  'help', 'history', 'clear', 'version', 'echo', 'ls', 'cd', 'pwd', 'cat', 'cp', 'mv', 'rm',
  'mkdir', 'touch', 'stat', 'du', 'df', 'mount', 'umount', 'disk', 'providers', 'quota',
  'oauth', 'sync', 'scrub', 'repair', 'gc', 'lease', 'ps', 'top', 'kill', 'jobs', 'compute',
  'workers', 'keygen', 'encrypt', 'decrypt', 'compress', 'decompress', 'search', 'grep',
  'find', 'head', 'tail', 'wc', 'write', 'edit', 'ai', 'theme', 'ui', 'cron', 'backup',
])

interface SrcLine {
  indent: number
  text: string
  lineno: number
}

function validName(name: string): boolean {
  return /^[A-Za-z_][A-Za-z0-9_]{0,63}$/.test(name)
}

function stripComment(body: string): string {
  let quote: string | null = null
  for (let i = 0; i < body.length; i++) {
    const c = body[i]!
    if (quote) {
      if (c === quote) quote = null
      continue
    }
    if (c === '"' || c === "'") {
      quote = c
      continue
    }
    if (c === '#' && (i === 0 || /\s/.test(body[i - 1]!))) return body.slice(0, i).trimEnd()
  }
  return body
}

function splitLines(source: string): SrcLine[] {
  const out: SrcLine[] = []
  source.split('\n').forEach((raw, idx) => {
    let indent = 0
    for (const c of raw) {
      if (c === ' ') indent += 1
      else if (c === '\t') indent += 4
      else break
    }
    const text = stripComment(raw.replace(/^[ \t]+/, '')).trim()
    if (text) out.push({ indent, text, lineno: idx + 1 })
  })
  return out
}

/** Parse + budget-check only (backs `run --dry`). */
export function checkCybshScript(source: string): string {
  if (source.length > CYBSH_MAX_SOURCE_BYTES) {
    throw new Error(`too_large: script is ${source.length} bytes, limit is ${CYBSH_MAX_SOURCE_BYTES}`)
  }
  checkScriptVersion(source)
  const lines = splitLines(source)
  const caps = parseScriptCaps(source)
  const capNote = caps.active ? ` · caps: ${describeScriptCaps(caps)}` : ''
  const fm = parseFrontmatter(source)
  let extra = ''
  if (fm.schedule) extra += ` · schedule: ${fm.schedule}`
  if (fm.triggers.length) extra += ` · on: ${fm.triggers.join(',')}`
  return `dry: ${lines.length} statement(s) parse${capNote}${extra} — nothing executed (use \`run <file.cybsh>\` to execute)`
}

/** Parse `# schedule:`, `# on: a, b`, `# desc:` frontmatter (first 200 lines). */
export function parseFrontmatter(source: string): CybshFrontmatter {
  const fm: CybshFrontmatter = { schedule: null, triggers: [], description: null }
  for (const raw of source.split('\n').slice(0, 200)) {
    const t = raw.trim()
    if (!t.startsWith('#')) continue
    const body = t.slice(1).trim()
    const cut = (prefixes: string[]): string | null => {
      for (const p of prefixes) {
        if (body.startsWith(p)) {
          const v = body.slice(p.length).trim().replace(/^[:\s]+/, '').trim()
          return v
        }
      }
      return null
    }
    const sched = cut(['schedule:', 'schedule '])
    if (sched !== null && fm.schedule === null && sched) fm.schedule = sched
    const on = cut(['on:'])
    if (on !== null) {
      for (const trg of on.split(',').map((s) => s.trim()).filter((s) => s.length > 0)) {
        if (!fm.triggers.includes(trg)) fm.triggers.push(trg)
      }
      continue
    }
    if (body.startsWith('on ') && fm.triggers.length === 0) {
      const v = body.slice(3).trim()
      for (const trg of v.split(',').map((s) => s.trim()).filter((s) => s.length > 0)) {
        if (!fm.triggers.includes(trg)) fm.triggers.push(trg)
      }
      continue
    }
    const desc = cut(['desc:', 'description:'])
    if (desc !== null && fm.description === null && desc) fm.description = desc
  }
  return fm
}

const STATEMENT_KWS = new Set([
  'print', 'let', 'const', 'if', 'elif', 'else', 'for', 'while', 'try',
  'catch', 'fail', 'def', 'return', 'match', 'ok', 'err', 'with',
  'import', 'await', 'fetch', 'js', 'vars', 'free', 'gc',
])

/** Clippy-style static warnings (backs `run --lint`): never executes. */
export function lintCybshScript(source: string): string[] {
  if (source.length > CYBSH_MAX_SOURCE_BYTES) {
    throw new Error(`too_large: script is ${source.length} bytes, limit is ${CYBSH_MAX_SOURCE_BYTES}`)
  }
  checkScriptVersion(source)
  const lines = splitLines(source)
  const warns: string[] = []
  const pinned = source.split('\n').some((l) => {
    const t = l.trim()
    if (!t.startsWith('#')) return false
    const d = t.slice(1).trim()
    return d.startsWith('cybsh:') || d.startsWith('cybsh ')
  })
  if (!pinned) warns.push('hint: no `# cybsh: 1` version pin (add one for reproducibility)')
  const fm = parseFrontmatter(source)
  if (fm.schedule && !fm.triggers.length) {
    warns.push('hint: `# schedule:` without `# on:` — triggers default to manual')
  }
  for (const line of lines) {
    const word = line.text.split(/\s+/, 1)[0]!
    if (BARE_VERBS.has(word) || STATEMENT_KWS.has(word)) continue
    if (splitAssignment(line.text)) continue
    if (/^[$"'\-[{0-9]/.test(line.text) || line.text.startsWith('js') || line.text.startsWith('sh')) continue
    if (/[(\-.|?]/.test(word)) continue
    warns.push(`warn: line ${line.lineno}: unknown verb \`${word}\` — bare lines must be cybsh verbs (try \`sh "…"\`)`)
  }
  if (source.split('\n').some((r) => r.startsWith('\t'))) {
    warns.push('style: tab indent (works as 4 spaces, prefer 2 spaces)')
  }
  const caps = parseScriptCaps(source)
  if (source.split('\n').some((l) => l.trimStart().startsWith('fetch')) && !caps.active) {
    warns.push('hint: `fetch` without `# cap: net=<host>` — ambient now, pinned later')
  }
  if (!warns.length) warns.push('lint: clean — no warnings')
  return warns
}

function isMatchArmLine(text: string): boolean {
  return text === 'else:' || text === 'else' || text === 'ok:' || text === 'err:' || isKw(text, 'ok') || isKw(text, 'err')
}

function isDedentKw(text: string): boolean {
  return text === 'else:' || text === 'else' || text === 'catch' || text === 'catch:' || isKw(text, 'elif') || isKw(text, 'catch') || isMatchArmLine(text)
}

function opensBlock(text: string): boolean {
  const opens =
    isKw(text, 'if') || isKw(text, 'elif') || text === 'else:' || text === 'else' ||
    isKw(text, 'for') || isKw(text, 'while') || text === 'try:' || text === 'try' ||
    isCatch(text) || isKw(text, 'def') || isKw(text, 'match') || isKw(text, 'with') ||
    isMatchArmLine(text)
  return opens && (text.endsWith(':') || text === 'try' || isCatch(text))
}

/** Canonical formatter (backs `run --fmt`): 2-space re-indent, idempotent. */
export function formatCybshScript(source: string): string {
  checkScriptVersion(source)
  const lines = splitLines(source)
  const out: string[] = []
  let stack: number[] = [-1]
  for (const line of lines) {
    const text = line.text
    const dedent = isDedentKw(text)
    while (stack.length > 1 && line.indent <= stack[stack.length - 1]!) stack.pop()
    let depth = stack.length - 1
    if (dedent) depth = Math.max(0, depth - 1)
    if (line.indent === 0) {
      depth = 0
      stack = [-1]
    }
    out.push(`${'  '.repeat(depth)}${text}`)
    if (opensBlock(text)) stack.push(line.indent)
  }
  return out.join('\n') + '\n'
}

function jsonQuote(s: string): string {
  return JSON.stringify(s)
}

function display(v: Value): string {
  switch (v.t) {
    case 'null': return 'null'
    case 'bool': return v.v ? 'true' : 'false'
    case 'num': return Number.isInteger(v.v) && Math.abs(v.v) < 1e15 ? String(v.v) : String(v.v)
    case 'str': return v.v
    case 'list': return `[${v.v.map(display).join(', ')}]`
    case 'dict': {
      const parts: string[] = []
      for (const k of [...v.v.keys()].sort()) parts.push(`${jsonQuote(k)}: ${display(v.v.get(k)!)}`)
      return `{${parts.join(', ')}}`
    }
  }
}

function typeName(v: Value): string {
  return v.t === 'num' ? 'number' : v.t === 'str' ? 'string' : v.t === 'bool' ? 'bool' : v.t
}

function truthy(v: Value): boolean {
  switch (v.t) {
    case 'null': return false
    case 'bool': return v.v
    case 'num': return v.v !== 0 && !Number.isNaN(v.v)
    case 'str': return v.v.length > 0
    case 'list': return v.v.length > 0
    case 'dict': return v.v.size > 0
  }
}

function asNum(v: Value): number | null {
  switch (v.t) {
    case 'num': return v.v
    case 'bool': return v.v ? 1 : 0
    case 'null': return 0
    case 'list': return v.v.length
    case 'dict': return v.v.size
    case 'str': {
      const n = Number(v.v.trim())
      return Number.isNaN(n) ? null : n
    }
  }
}

function trunc(s: string, cap = CYBSH_MAX_STR): string {
  return s.length <= cap ? s : s.slice(0, cap)
}

function truncShow(s: string, cap = 120): string {
  return s.length <= cap ? s : `${s.slice(0, cap)}…`
}

export interface FuncDef {
  params: string[]
  body: SrcLine[]
  definedAt: number
}

class Runner {
  vars = new Map<string, Value>()
  funcs = new Map<string, FuncDef>()
  flow: Value | null = null
  callDepth = 0
  importDepth = 0
  caps: CybshCaps = { active: false, net: null, read: null, write: null, deny: [] }
  chunks: string[] = []
  effects: CybshScriptEffect[] = []
  steps = 0
  truncated = false
  shCalls = 0
  fetchCalls = 0

  constructor(readonly deps: CybshScriptDeps) {}

  emit(s: string): void {
    const cur = this.chunks.join('\n').length
    if (cur >= CYBSH_MAX_OUTPUT_BYTES) {
      this.truncated = true
      return
    }
    let chunk = s
    if (cur + chunk.length + (this.chunks.length ? 1 : 0) > CYBSH_MAX_OUTPUT_BYTES) {
      chunk = chunk.slice(0, Math.max(0, CYBSH_MAX_OUTPUT_BYTES - cur))
      this.truncated = true
    }
    this.chunks.push(chunk)
  }

  bump(): void {
    this.steps += 1
    if (this.steps > CYBSH_MAX_STEPS) {
      throw new Error(`too_large: script exceeded ${CYBSH_MAX_STEPS} steps (possible infinite loop — split it or add a bound)`)
    }
  }

  setVar(name: string, v: Value): void {
    if (!validName(name)) throw new Error(`syntax: bad variable name '${name}' ([A-Za-z_][A-Za-z0-9_]*)`)
    if (!this.vars.has(name) && this.vars.size >= CYBSH_MAX_VARS) {
      throw new Error(`too_large: script holds ${CYBSH_MAX_VARS} variables already (\`free\` one or run \`gc\`)`)
    }
    this.vars.set(name, v)
  }

  harvest(text: string): void {
    for (const raw of text.split('\n')) {
      const line = raw.trim()
      if (!line.startsWith('ui:')) continue
      const rest = line.slice(3).trim()
      const eq = rest.indexOf('=')
      if (eq < 0) continue
      const k = rest.slice(0, eq).trim()
      const val = rest.slice(eq + 1).trim()
      if (k === 'theme' || k === 'accent' || k === 'density' || k === 'glass' || k === 'motion' || k === 'glow' || k === 'accent-for') this.effects.push({ kind: k, detail: val })
    }
  }

  interpolate(src: string): string {
    return src.replace(/\$\{([^}]*)\}/g, (m, name: string) => {
      const v = this.vars.get(String(name).trim())
      return v ? display(v) : m
    })
  }

  async runInline(cmdline: string, lineno: number): Promise<void> {
    const expanded = this.interpolate(cmdline)
    try {
      const text = await this.execChecked(expanded)
      this.harvest(text)
      if (text) this.emit(text)
      this.setVar('_', vStr(trunc(text)))
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      throw new Error(`${msg} (line ${lineno})`)
    }
  }

  /** Inline shell through the capability gate (single choke point). */
  async execChecked(line: string): Promise<string> {
    const verb = line.trim().split(/\s+/, 1)[0]?.toLowerCase() ?? ''
    if (this.caps.active && this.caps.deny.some((d) => d.toLowerCase() === verb)) {
      throw new Error(`denied: \`${verb}\` is refused by this script's capabilities (\`# cap: deny=…\`)`)
    }
    this.shCalls++
    return String(await this.deps.execCybsh(line))
  }

  /** Fetch through the capability gate + host client. */
  async fetchChecked(req: CybshFetchReq, lineno: number): Promise<string> {
    if (this.caps.active) {
      const host = fetchHost(req.url)
      if (!this.caps.net || !this.caps.net.some((h) => h.toLowerCase() === host)) {
        throw new Error(
          this.caps.net
            ? `denied: fetch ${req.url} is outside this script's \`net=\` allowlist`
            : 'denied: fetch needs a `net=<host>` capability (`# cap: net=…`)',
        )
      }
    }
    this.fetchCalls++
    if (!this.deps.fetchText) {
      throw new Error(
        `unsupported: \`fetch ${req.url}\` needs the browser/static transport (this shell has no HTTP client) — run the same \`.cybsh\` on Pages, replay a journal (\`--replay\`), or serve it via \`POST /api/os/exec\` on the dashboard worker`,
      )
    }
    return trunc(await this.deps.fetchText(req))
  }

  /** Shared capture: `--json` output materializes, text stays a string. */
  async inlineValue(cmdline: string, lineno: number): Promise<Value> {
    const expanded = this.interpolate(cmdline)
    try {
      const text = await this.execChecked(expanded)
      this.harvest(text)
      const v = materializeJson(text) ?? vStr(trunc(text))
      this.setVar('_', v)
      return v
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      throw new Error(`${msg} (line ${lineno})`)
    }
  }
}

/** `--json` output becomes structure (objects → Dict sorted, arrays → List). */
function materializeJson(text: string): Value | null {
  let parsed: unknown
  try {
    parsed = JSON.parse(text.trim())
  } catch {
    return null
  }
  return jsonToValue(parsed)
}

function jsonToValue(v: unknown): Value {
  if (v === null) return vNull()
  if (typeof v === 'boolean') return vBool(v)
  if (typeof v === 'number') return vNum(v)
  if (typeof v === 'string') return vStr(trunc(v))
  if (Array.isArray(v)) return vList(v.slice(0, CYBSH_MAX_LIST).map(jsonToValue))
  if (typeof v === 'object') {
    const map = new Map<string, Value>()
    for (const k of Object.keys(v as Record<string, unknown>).sort().slice(0, CYBSH_MAX_LIST)) {
      map.set(k, jsonToValue((v as Record<string, unknown>)[k]))
    }
    return vDict(map)
  }
  return vStr(trunc(String(v)))
}

function isKw(text: string, kw: string): boolean {
  return text === kw || text.startsWith(`${kw} `)
}

function restOf(text: string, kw: string, lineno: number): string {
  const rest = text.slice(kw.length).trim()
  if (!rest) throw new Error(`syntax: line ${lineno}: \`${kw}\` needs an argument`)
  return rest
}

function stripColon(s: string, lineno: number): string {
  const t = s.trim()
  if (!t.endsWith(':')) {
    throw new Error('syntax: line ' + lineno + ': block opener needs a trailing `:` (`if …:`, `for …:`, `while …:`, `else:`)')
  }
  return t.slice(0, -1).trim()
}

function blockIndent(lines: SrcLine[], start: number, parent: number): void {
  if (start >= lines.length || lines[start]!.indent <= parent) {
    const at = lines[start]?.lineno ?? 0
    throw new Error(`syntax: expected an indented block after line ${at}`)
  }
}

function skipBlock(lines: SrcLine[], start: number, parent: number): number {
  let i = start
  while (i < lines.length && lines[i]!.indent > parent) i++
  return i
}

function skipElifChain(lines: SrcLine[], j: number, parent: number): number {
  for (;;) {
    if (j < lines.length && lines[j]!.indent === parent) {
      const t = lines[j]!.text
      if (isKw(t, 'elif') || t === 'else:' || t === 'else') {
        j = skipBlock(lines, j + 1, parent)
        continue
      }
    }
    return j
  }
}

async function runBlock(r: Runner, lines: SrcLine[], start: number, parent: number): Promise<number> {
  let i = start
  let executed = 0
  while (i < lines.length && lines[i]!.indent > parent) {
    if (executed >= CYBSH_MAX_STMTS) {
      throw new Error(`too_large: block exceeds ${CYBSH_MAX_STMTS} statements (line ${lines[i]!.lineno}) — split the script`)
    }
    i = await runStatement(r, lines, i)
    executed++
    // A `return` inside a `def` unwinds every enclosing block.
    if (r.flow !== null) break
  }
  return i
}

// ─── expressions ────────────────────────────────────────────────────────

class Expr {
  pos = 0
  constructor(
    readonly chars: string,
    readonly vars: Map<string, Value>,
    readonly lineno: number,
    readonly env?: Record<string, string>,
  ) {}

  skip(): void {
    while (this.pos < this.chars.length && /\s/.test(this.chars[this.pos]!)) this.pos++
  }

  eatWord(word: string): boolean {
    this.skip()
    if (!this.chars.startsWith(word, this.pos)) return false
    if (/[A-Za-z]/.test(word)) {
      const after = this.chars[this.pos + word.length]
      if (after && /[A-Za-z0-9_]/.test(after)) return false
    }
    this.pos += word.length
    return true
  }

  parsePipe(): Value {
    // `a |> f(b)` is `f(a, b)`; `a |> len` is `len(a)` (Nushell rule over
    // materialized values). Pure builtins only here — `sh`/user-`def` pipes
    // are hoisted one layer up (`evalExprAsync`).
    let left = this.parseOr()
    for (;;) {
      this.skip()
      if (this.chars[this.pos] === '|' && this.chars[this.pos + 1] === '>') {
        this.pos += 2
        left = this.applyPipe(left)
      } else return left
    }
  }

  applyPipe(lhs: Value): Value {
    this.skip()
    const name = this.parseIdent()
    if (!name || !validName(name)) {
      throw new Error(`syntax: line ${this.lineno}: \`|>\` needs a function (\`x |> len\`, \`x |> split(",")\`)`)
    }
    const args: Value[] = [lhs]
    this.skip()
    if (this.chars[this.pos] === '(') {
      this.pos++
      for (;;) {
        this.skip()
        if (this.chars[this.pos] === ')') {
          this.pos++
          break
        }
        if (args.length >= 9) throw new Error(`syntax: line ${this.lineno}: \`${name}\` takes at most 8 arguments`)
        args.push(this.parsePipe())
        this.skip()
        if (this.chars[this.pos] === ',') {
          this.pos++
          continue
        }
        this.skip()
        if (this.chars[this.pos] === ')') {
          this.pos++
          break
        }
        throw new Error(`syntax: line ${this.lineno}: expected \`,\` or \`)\` in \`${name}(…)\``)
      }
    }
    return builtinSync(name, args, this.lineno, { vars: this.vars, env: this.env })
  }

  parseOr(): Value {
    let left = this.parseAnd()
    for (;;) {
      if (this.eatWord('or') || this.eatOp('||')) {
        const right = this.parseAnd()
        left = vBool(truthy(left) || truthy(right))
      } else return left
    }
  }

  parseAnd(): Value {
    let left = this.parseNot()
    for (;;) {
      if (this.eatWord('and') || this.eatOp('&&')) {
        const right = this.parseNot()
        left = vBool(truthy(left) && truthy(right))
      } else return left
    }
  }

  parseNot(): Value {
    if (this.eatWord('not')) return vBool(!truthy(this.parseNot()))
    this.skip()
    if (this.chars[this.pos] === '!' && this.chars[this.pos + 1] !== '=') {
      this.pos++
      return vBool(!truthy(this.parseNot()))
    }
    return this.parseCmp()
  }

  eatOp(op: string): boolean {
    this.skip()
    if (this.chars.startsWith(op, this.pos)) {
      this.pos += op.length
      return true
    }
    return false
  }

  parseCmp(): Value {
    const left = this.parseAdd()
    for (const op of ['==', '!=', '<=', '>=', '<', '>']) {
      if (this.eatOp(op)) {
        const right = this.parseAdd()
        return vBool(compare(left, op, right))
      }
    }
    return left
  }

  parseAdd(): Value {
    let left = this.parseMul()
    for (;;) {
      this.skip()
      const c = this.chars[this.pos]
      if ((c === '+' || c === '-') && !(c === '-' && this.chars[this.pos + 1] === '=')) {
        this.pos++
        const right = this.parseMul()
        left = arith(left, c, right, this.lineno)
      } else return left
    }
  }

  parseMul(): Value {
    let left = this.parseUnary()
    for (;;) {
      this.skip()
      const c = this.chars[this.pos]
      if (c === '*' || c === '/' || c === '%') {
        this.pos++
        const right = this.parseUnary()
        left = arith(left, c as '+' | '-' | '*' | '/' | '%', right, this.lineno)
      } else return left
    }
  }

  parseUnary(): Value {
    this.skip()
    if (this.chars[this.pos] === '-') {
      this.pos++
      const v = this.parseUnary()
      const n = asNum(v)
      if (n === null) throw new Error(`syntax: line ${this.lineno}: \`-\` needs a number`)
      return vNum(-n)
    }
    return this.parsePrimary()
  }

  parsePrimary(): Value {
    this.skip()
    if (this.pos >= this.chars.length) throw new Error(`syntax: line ${this.lineno}: expression ended early`)
    const c = this.chars[this.pos]!
    let base: Value
    if (c === '(') {
      this.pos++
      base = this.parsePipe()
      this.skip()
      if (this.chars[this.pos] !== ')') throw new Error(`syntax: line ${this.lineno}: unclosed \`(\``)
      this.pos++
    } else if (c === '[') {
      base = this.parseList()
    } else if (c === '{') {
      base = this.parseDict()
    } else if (c === '"' || c === "'") {
      base = vStr(this.parseString())
    } else if (/[0-9]/.test(c) || (c === '.' && /[0-9]/.test(this.chars[this.pos + 1] ?? ''))) {
      base = this.parseNumber()
    } else if (/[A-Za-z_]/.test(c)) {
      const ident = this.parseIdent()
      this.skip()
      if (this.chars[this.pos] === '(') {
        base = this.parseCall(ident)
      } else {
        if (ident === 'true') base = vBool(true)
        else if (ident === 'false') base = vBool(false)
        else if (ident === 'null' || ident === 'none' || ident === 'nil') base = vNull()
        else if (ident === 'and' || ident === 'or' || ident === 'not') {
          throw new Error(`syntax: line ${this.lineno}: \`${ident}\` outside an expression`)
        } else {
          const v = this.vars.get(ident)
          if (!v) throw new Error(`not_found: no variable \`${ident}\` (line ${this.lineno})`)
          base = v
        }
      }
    } else {
      throw new Error(`syntax: line ${this.lineno}: unexpected \`${c}\` in expression`)
    }
    // Postfix indexing (`d.key`, `d["k"]`, `l[0]`, `s[0]`) + `?` unwrap
    // (`{ok:true,value:v}?` → `v`; `{ok:false,error:e}?` raises `e`).
    for (;;) {
      this.skip()
      if (this.chars[this.pos] === '.') {
        this.pos++
        const field = this.parseIdent()
        if (!field) throw new Error(`syntax: line ${this.lineno}: \`.\` needs a field name`)
        base = indexField(base, field, this.lineno)
        continue
      }
      if (this.chars[this.pos] === '[') {
        this.pos++
        const key = this.parsePipe()
        this.skip()
        if (this.chars[this.pos] !== ']') {
          throw new Error(`syntax: line ${this.lineno}: unclosed \`[\` in index`)
        }
        this.pos++
        base = indexValue(base, key, this.lineno)
        continue
      }
      if (this.chars[this.pos] === '?') {
        this.pos++
        base = unwrapValue(base, this.lineno)
        continue
      }
      return base
    }
  }

  parseIdent(): string {
    const start = this.pos
    while (this.pos < this.chars.length && /[A-Za-z0-9_]/.test(this.chars[this.pos]!)) this.pos++
    return this.chars.slice(start, this.pos)
  }

  parseString(): string {
    const quote = this.chars[this.pos]!
    this.pos++
    let out = ''
    while (this.pos < this.chars.length) {
      const c = this.chars[this.pos]!
      if (c === '\\' && this.pos + 1 < this.chars.length) {
        const n = this.chars[this.pos + 1]!
        out += n === 'n' ? '\n' : n === 't' ? '\t' : n
        this.pos += 2
        continue
      }
      if (c === quote) {
        this.pos++
        return out.replace(/\$\{([^}]*)\}/g, (m, name: string) => {
          const v = this.vars.get(String(name).trim())
          return v ? display(v) : m
        })
      }
      out += c
      this.pos++
    }
    throw new Error(`syntax: line ${this.lineno}: unclosed string`)
  }

  parseNumber(): Value {
    const start = this.pos
    let dot = false
    while (this.pos < this.chars.length && /[0-9.]/.test(this.chars[this.pos]!)) {
      if (this.chars[this.pos] === '.') {
        if (dot) break
        dot = true
      }
      this.pos++
    }
    const raw = this.chars.slice(start, this.pos)
    const n = Number(raw)
    if (Number.isNaN(n)) throw new Error(`syntax: line ${this.lineno}: bad number \`${raw}\``)
    return vNum(n)
  }

  parseList(): Value {
    this.pos++
    const items: Value[] = []
    for (;;) {
      this.skip()
      if (this.chars[this.pos] === ']') {
        this.pos++
        return vList(items)
      }
      if (items.length >= CYBSH_MAX_LIST) throw new Error(`syntax: line ${this.lineno}: list exceeds ${CYBSH_MAX_LIST} items`)
      items.push(this.parsePipe())
      this.skip()
      if (this.chars[this.pos] === ',') {
        this.pos++
        continue
      }
      this.skip()
      if (this.chars[this.pos] === ']') {
        this.pos++
        return vList(items)
      }
      throw new Error(`syntax: line ${this.lineno}: expected \`,\` or \`]\` in list`)
    }
  }

  parseDict(): Value {
    // `{` already peeked. Keys are string literals or bare idents.
    this.pos++
    const map = new Map<string, Value>()
    for (;;) {
      this.skip()
      if (this.chars[this.pos] === '}') {
        this.pos++
        return vDict(map)
      }
      if (map.size >= CYBSH_MAX_LIST) {
        throw new Error(`syntax: line ${this.lineno}: dict exceeds ${CYBSH_MAX_LIST} keys`)
      }
      this.skip()
      if (this.pos >= this.chars.length) throw new Error(`syntax: line ${this.lineno}: unclosed \`{\``)
      const kc = this.chars[this.pos]!
      let key: string
      if (kc === '"' || kc === "'") key = this.parseString()
      else if (/[A-Za-z_]/.test(kc)) key = this.parseIdent()
      else throw new Error(`syntax: line ${this.lineno}: dict keys are \`"str"\` or bare idents`)
      this.skip()
      if (this.chars[this.pos] !== ':') {
        throw new Error(`syntax: line ${this.lineno}: dict needs \`key: value\` pairs`)
      }
      this.pos++
      const value = this.parsePipe()
      if (value.t === 'str' && value.v.length > CYBSH_MAX_STR) {
        throw new Error(`too_large: line ${this.lineno}: dict value exceeds ${CYBSH_MAX_STR} bytes`)
      }
      map.set(key, value)
      this.skip()
      if (this.chars[this.pos] === ',') {
        this.pos++
        continue
      }
      this.skip()
      if (this.chars[this.pos] === '}') {
        this.pos++
        return vDict(map)
      }
      throw new Error(`syntax: line ${this.lineno}: expected \`,\` or \`}\` in dict`)
    }
  }

  parseCall(name: string): Value {
    this.pos++
    const args: Value[] = []
    for (;;) {
      this.skip()
      if (this.chars[this.pos] === ')') {
        this.pos++
        break
      }
      if (args.length >= 8) throw new Error(`syntax: line ${this.lineno}: \`${name}\` takes at most 8 arguments`)
      args.push(this.parsePipe())
      this.skip()
      if (this.chars[this.pos] === ',') {
        this.pos++
        continue
      }
      this.skip()
      if (this.chars[this.pos] === ')') {
        this.pos++
        break
      }
      throw new Error(`syntax: line ${this.lineno}: expected \`,\` or \`)\` in \`${name}(…)\``)
    }
    return builtinSync(name, args, this.lineno, { vars: this.vars, env: this.env })
  }
}

function compare(left: Value, op: string, right: Value): boolean {
  if (left.t === 'num' && right.t === 'num') {
    const [a, b] = [left.v, right.v]
    switch (op) {
      case '==': return a === b
      case '!=': return a !== b
      case '<': return a < b
      case '<=': return a <= b
      case '>': return a > b
      case '>=': return a >= b
    }
  }
  const [a, b] = [display(left), display(right)]
  switch (op) {
    case '==': return a === b
    case '!=': return a !== b
    case '<': return a < b
    case '<=': return a <= b
    case '>': return a > b
    default: return a >= b
  }
}

function arith(left: Value, op: string, right: Value, lineno: number): Value {
  if (op === '+' && (left.t === 'str' || right.t === 'str')) {
    const s = display(left) + display(right)
    if (s.length > CYBSH_MAX_STR) throw new Error(`too_large: line ${lineno}: string grew past ${CYBSH_MAX_STR} bytes`)
    return vStr(s)
  }
  if (op === '*') {
    const pair: [Value, Value] = left.t === 'str' ? [left, right] : right.t === 'str' ? [right, left] : [left, right]
    if (pair[0].t === 'str' && pair[1].t === 'num') {
      const times = Math.max(0, Math.min(256, Math.floor(pair[1].v)))
      if (pair[0].v.length * times > CYBSH_MAX_STR) {
        throw new Error(`too_large: line ${lineno}: repeat exceeds ${CYBSH_MAX_STR} bytes`)
      }
      return vStr(pair[0].v.repeat(times))
    }
  }
  const a = asNum(left)
  const b = asNum(right)
  if (a === null || b === null) {
    throw new Error(`syntax: line ${lineno}: \`${op}\` needs numbers (got ${typeName(left)} and ${typeName(right)})`)
  }
  switch (op) {
    case '+': return vNum(a + b)
    case '-': return vNum(a - b)
    case '*': return vNum(a * b)
    case '/':
      if (b === 0) throw new Error(`syntax: line ${lineno}: division by zero`)
      return vNum(a / b)
    default:
      if (b === 0) throw new Error(`syntax: line ${lineno}: modulo by zero`)
      return vNum(a % b)
  }
}

/** Sync builtins (`len/int/str/json/split/range`). `sh` needs async exec. */
export interface BuiltinCtx {
  vars?: Map<string, Value>
  env?: Record<string, string>
}

const BUILTIN_HINT = 'len/int/str/json/split/range/sh/set/push/del/keys/values/ok/err/unwrap/is_ok/is_err/env/arg/fingerprint'

/** Classify a Result dict: (isErr, okValue, errText). Plain values are ok. */
function classifyResult(v: Value): { isErr: boolean; value: Value; err: string } {
  if (v.t === 'dict') {
    const flag = v.v.get('ok')
    if (flag?.t === 'bool' && !flag.v) {
      const e = v.v.get('error')
      return { isErr: true, value: vNull(), err: e ? display(e) : 'error' }
    }
    if (flag?.t === 'bool' && flag.v) {
      return { isErr: false, value: v.v.get('value') ?? vNull(), err: '' }
    }
  }
  return { isErr: false, value: v, err: '' }
}

/** Postfix `?` unwrap. */
function unwrapValue(v: Value, lineno: number): Value {
  const c = classifyResult(v)
  if (c.isErr) throw new Error(c.err || `fail: unwrap of err (line ${lineno})`)
  return c.value
}

function builtinSync(name: string, args: Value[], lineno: number, ctx?: BuiltinCtx): Value {
  const need = (min: number, max: number): void => {
    if (args.length < min || args.length > max) {
      throw new Error(`syntax: line ${lineno}: \`${name}\` takes ${min}–${max} argument(s), got ${args.length}`)
    }
  }
  switch (name) {
    case 'len': {
      need(1, 1)
      const a = args[0]!
      if (a.t === 'str') return vNum([...a.v].length)
      if (a.t === 'list') return vNum(a.v.length)
      if (a.t === 'dict') return vNum(a.v.size)
      if (a.t === 'num') return vNum(a.v)
      return vNum(a.t === 'bool' && a.v ? 1 : 0)
    }
    case 'int': {
      need(1, 1)
      const n = asNum(args[0]!)
      if (n === null) throw new Error(`syntax: line ${lineno}: \`int\` needs a number-like value`)
      return vNum(Math.trunc(n))
    }
    case 'str': {
      need(1, 1)
      return vStr(trunc(display(args[0]!)))
    }
    case 'json': {
      need(1, 1)
      const v = materializeJson(display(args[0]!))
      if (v === null) throw new Error(`syntax: line ${lineno}: \`json\` needs valid JSON text`)
      return v
    }
    case 'split': {
      need(1, 2)
      const first = args[0]!
      if (first.t !== 'str') {
        throw new Error(`syntax: line ${lineno}: \`split\` takes a string (got ${typeName(first)})`)
      }
      const text = first.v
      let parts: string[]
      if (args.length === 2) {
        const delim = display(args[1]!)
        parts = delim === '' ? [...text] : text.split(delim)
      } else {
        parts = text.includes('\n') ? text.split('\n') : text.split(/\s+/).filter((s) => s.length > 0)
      }
      if (parts.length > CYBSH_MAX_LIST) {
        throw new Error(`too_large: line ${lineno}: split produced ${parts.length} items (limit ${CYBSH_MAX_LIST})`)
      }
      return vList(parts.map((s) => vStr(s)))
    }
    case 'range': {
      need(1, 2)
      const nums = args.map((a) => asNum(a) ?? 0)
      const [start, end] = nums.length === 2 ? [Math.trunc(nums[0]!), Math.trunc(nums[1]!)] : [0, Math.trunc(nums[0]!)]
      const count = Math.max(0, Math.min(CYBSH_MAX_ITERS, end - start))
      const items: Value[] = []
      for (let i = 0; i < count; i++) items.push(vNum(start + i))
      return vList(items)
    }
    // Functional updaters (values are owned — updates return new values).
    case 'set': {
      need(3, 3)
      const base = args[0]!
      if (base.t !== 'dict') throw new Error(`syntax: line ${lineno}: \`set\` takes a dict (got ${typeName(base)})`)
      const next = new Map(base.v)
      next.set(display(args[1]!), args[2]!)
      if (next.size > CYBSH_MAX_LIST) throw new Error(`too_large: line ${lineno}: dict exceeds ${CYBSH_MAX_LIST} keys`)
      return vDict(next)
    }
    case 'push': {
      need(2, 2)
      const base = args[0]!
      if (base.t !== 'list') throw new Error(`syntax: line ${lineno}: \`push\` takes a list (got ${typeName(base)})`)
      if (base.v.length >= CYBSH_MAX_LIST) {
        throw new Error(`too_large: line ${lineno}: list exceeds ${CYBSH_MAX_LIST} items`)
      }
      return vList([...base.v, args[1]!])
    }
    case 'del': {
      need(2, 2)
      const base = args[0]!
      if (base.t !== 'dict') throw new Error(`syntax: line ${lineno}: \`del\` takes a dict (got ${typeName(base)})`)
      const next = new Map(base.v)
      next.delete(display(args[1]!))
      return vDict(next)
    }
    case 'keys': {
      need(1, 1)
      const base = args[0]!
      if (base.t !== 'dict') throw new Error(`syntax: line ${lineno}: \`keys\` takes a dict (got ${typeName(base)})`)
      return vList([...base.v.keys()].sort().map((k) => vStr(k)))
    }
    case 'values': {
      need(1, 1)
      const base = args[0]!
      if (base.t !== 'dict') throw new Error(`syntax: line ${lineno}: \`values\` takes a dict (got ${typeName(base)})`)
      return vList([...base.v.keys()].sort().map((k) => base.v.get(k)!))
    }
    case 'ok': {
      need(1, 1)
      return vDict(new Map([['ok', vBool(true)], ['value', args[0]!]]))
    }
    case 'err': {
      need(1, 1)
      return vDict(new Map([['ok', vBool(false)], ['error', vStr(display(args[0]!))]]))
    }
    case 'unwrap': {
      need(1, 1)
      return unwrapValue(args[0]!, lineno)
    }
    case 'is_ok': {
      need(1, 1)
      return vBool(!classifyResult(args[0]!).isErr)
    }
    case 'is_err': {
      need(1, 1)
      return vBool(classifyResult(args[0]!).isErr)
    }
    case 'env': {
      need(1, 2)
      const key = display(args[0]!)
      const table = ctx?.env ?? readProcessEnv()
      if (table && key in table) return vStr(trunc(table[key]!))
      if (args.length === 2) return args[1]!
      throw new Error(`not_found: no env \`${key}\` (line ${lineno})`)
    }
    case 'arg': {
      need(1, 2)
      const n = asNum(args[0]!)
      if (n === null) throw new Error(`syntax: line ${lineno}: \`arg\` needs a number index`)
      const list = ctx?.vars?.get('args')
      const items = list?.t === 'list' ? list.v : []
      const i = Math.trunc(n)
      if (i < 0 || i >= items.length) {
        if (args.length === 2) return args[1]!
        throw new Error(`not_found: no arg \`${i}\` (line ${lineno})`)
      }
      return items[i]!
    }
    case 'fingerprint': {
      need(1, 1)
      return vStr(cybshFingerprint(display(args[0]!)))
    }
    case 'sh': {
      // `sh` needs async exec — the async layer hoists it first. A bare
      // `sh` surviving to sync eval means it was piped without parens in a
      // pure (`js`) context.
      throw new Error(`syntax: line ${lineno}: \`sh\` needs the async shell context (use \`sh("…")\` in a statement or \`let x = sh("…")\`)`)
    }
    default:
      throw new Error(`syntax: line ${lineno}: unknown function \`${name}\` (try \`${BUILTIN_HINT}\` or \`def\` it first)`)
  }
}

/** `process.env` under node (tests/SSR); browsers pass `deps.env` instead. */
function readProcessEnv(): Record<string, string> | undefined {
  try {
    const g = globalThis as { process?: { env?: Record<string, string> } }
    return g.process?.env ? { ...g.process.env } as Record<string, string> : undefined
  } catch {
    return undefined
  }
}

function evalExpr(r: Runner, src: string, lineno: number): Value {
  const p = new Expr(src, r.vars, lineno, r.deps.env)
  const v = p.parsePipe()
  p.skip()
  if (p.pos < p.chars.length) {
    throw new Error(`syntax: line ${lineno}: unexpected \`${p.chars.slice(p.pos).trim()}\` in expression`)
  }
  if (v.t === 'str' && v.v.length > CYBSH_MAX_STR) {
    throw new Error(`too_large: line ${lineno}: value exceeds ${CYBSH_MAX_STR} bytes`)
  }
  return v
}

/** Quote/bracket-aware top-level `|>` split. Null when no pipe is present. */
function splitTopPipe(src: string): string[] | null {
  const parts: string[] = []
  let cur = ''
  let quote: string | null = null
  let depth = 0
  for (let i = 0; i < src.length; i++) {
    const c = src[i]!
    if (quote) {
      cur += c
      if (c === quote) quote = null
      continue
    }
    if (c === '"' || c === "'") {
      quote = c
      cur += c
      continue
    }
    if (c === '(' || c === '[' || c === '{') depth++
    else if (c === ')' || c === ']' || c === '}') depth = Math.max(0, depth - 1)
    if (depth === 0 && c === '|' && src[i + 1] === '>') {
      parts.push(cur)
      cur = ''
      i++
      continue
    }
    cur += c
  }
  if (!parts.length) return null
  parts.push(cur)
  return parts
}

/** Parse a pipe segment rhs: `name` or `name(args…)`. */
function parsePipeRhs(seg: string, lineno: number): { name: string; argsSrc: string | null } {
  const t = seg.trim()
  const m = t.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*(\([\s\S]*\))?$/)
  if (!m?.[1]) throw new Error(`syntax: line ${lineno}: \`|>\` needs a function (\`x |> len\`, \`x |> split(",")\`)`)
  const name = m[1]
  const paren = m[2] ?? null
  if (paren !== null) {
    if (!paren.endsWith(')')) throw new Error(`syntax: line ${lineno}: unclosed \`(\` after \`|>\` ${name}`)
    return { name, argsSrc: paren.slice(1, -1) }
  }
  return { name, argsSrc: null }
}

/** Async pipe: `lhs |> f(a)` is `f(lhs, a)`; user-`def`s and `sh` allowed. */
async function evalPipeAsync(r: Runner, segments: string[], lineno: number): Promise<Value> {
  let lhs = await evalExprAsync(r, segments[0]!.trim(), lineno)
  for (const seg of segments.slice(1)) {
    const { name, argsSrc } = parsePipeRhs(seg, lineno)
    const argVals: Value[] = []
    if (argsSrc !== null && argsSrc.trim()) {
      for (const a of splitTopCommas(argsSrc)) {
        if (a) argVals.push(await evalExprAsync(r, a, lineno))
      }
    }
    const full = [lhs, ...argVals]
    if (r.funcs.has(name)) {
      if (full.length !== (r.funcs.get(name)!.params.length)) {
        // callFunc reports arity itself; pad check lives there.
      }
      lhs = await callFunc(r, name, full, lineno)
    } else if (name === 'sh') {
      if (argVals.length > 0) {
        throw new Error(`syntax: line ${lineno}: \`|>\` into \`sh\` takes no extra args (\`cmd |> sh\` runs the command)`)
      }
      lhs = await execShValue(r, lhs, lineno)
    } else {
      lhs = builtinSync(name, full, lineno, { vars: r.vars, env: r.deps.env })
    }
  }
  return lhs
}

/**
 * Expression evaluation with `sh(…)` hoisting: the sync grammar cannot
 * `await`, so each `sh(<expr>)` runs first (innermost-first, left-to-right)
 * and its output is substituted as a string literal. Matches the native
 * shell, where `sh()` takes any expression (`let x = sh("echo " + name)`).
 */
async function evalExprAsync(r: Runner, src: string, lineno: number): Promise<Value> {
  // Pipes first (top-level `|>`; segments recurse, so `sh`/user calls
  // inside any segment hoist naturally), then user `def`s, then `sh(…)`.
  const piped = splitTopPipe(src)
  if (piped) return evalPipeAsync(r, piped, lineno)
  // User `def`s hoist first (bodies run statements, hence async), then
  // `sh(…)` — innermost-first, left-to-right, like evaluation order.
  const call = findFirstUserCall(src, r.funcs)
  if (call) {
    const argVals: Value[] = []
    for (const a of splitTopCommas(call.inner)) {
      if (a) argVals.push(await evalExprAsync(r, a, lineno))
    }
    // `f()` with empty parens: zero args (splitTopCommas yields [] anyway).
    const result = await callFunc(r, call.name, argVals, lineno)
    const replaced = src.slice(0, call.start) + valueToLiteral(result) + src.slice(call.end)
    return evalExprAsync(r, replaced, lineno)
  }
  const found = findFirstShCall(src)
  if (!found) return evalExpr(r, src, lineno)
  const innerVal = await evalExprAsync(r, found.inner, lineno)
  const result = await execShValue(r, innerVal, lineno)
  const replaced = src.slice(0, found.start) + valueToLiteral(result) + src.slice(found.end)
  return evalExprAsync(r, replaced, lineno)
}

/**
 * `sh(<expr>)` through the capability gate, with `--json` output
 * materialized like the native twin (objects → Dict, arrays → List).
 */
async function execShValue(r: Runner, arg: Value, lineno: number): Promise<Value> {
  const expanded = r.interpolate(display(arg))
  try {
    const text = await r.execChecked(expanded)
    r.harvest(text)
    const v = materializeJson(text) ?? vStr(trunc(text))
    r.setVar('_', v)
    return v
  } catch (e) {
    throw new Error(`${e instanceof Error ? e.message : String(e)} (line ${lineno})`)
  }
}

/** First quote-aware `sh(` call with its balanced span. Null when absent. */
function findFirstShCall(src: string): { start: number; end: number; inner: string } | null {
  let quote: string | null = null
  for (let i = 0; i < src.length; i++) {
    const c = src[i]!
    if (quote) {
      if (c === quote) quote = null
      continue
    }
    if (c === '"' || c === "'") {
      quote = c
      continue
    }
    if ((c === 's' && src.startsWith('sh', i)) && !/[A-Za-z0-9_.]/.test(src[i - 1] ?? '')) {
      let j = i + 2
      while (j < src.length && /\s/.test(src[j]!)) j++
      if (src[j] !== '(') continue
      let depth = 0
      let q2: string | null = null
      for (let k = j; k < src.length; k++) {
        const d = src[k]!
        if (q2) {
          if (d === q2) q2 = null
          continue
        }
        if (d === '"' || d === "'") {
          q2 = d
          continue
        }
        if (d === '(') depth++
        else if (d === ')') {
          depth--
          if (depth === 0) return { start: i, end: k + 1, inner: src.slice(j + 1, k) }
        }
      }
      throw new Error('syntax: unclosed `sh(` — expected a matching `)`')
    }
  }
  return null
}

/** `d.field` — dict lookup only (use `len(x)` etc. for the rest). */
function indexField(base: Value, field: string, lineno: number): Value {
  if (base.t === 'dict') {
    const v = base.v.get(field)
    if (v === undefined) throw new Error(`not_found: dict has no field \`${field}\` (line ${lineno})`)
    return v
  }
  throw new Error(`syntax: line ${lineno}: \`.\` indexes dicts — got ${typeName(base)} (try \`len(x)\`)`)
}

/** `d["k"]`, `l[0]`, `s[0]` — dict key, list index, string char. */
function indexValue(base: Value, key: Value, lineno: number): Value {
  if (base.t === 'dict') {
    const v = base.v.get(display(key))
    if (v === undefined) throw new Error(`not_found: dict has no key \`${display(key)}\` (line ${lineno})`)
    return v
  }
  if (base.t === 'list' && key.t === 'num') {
    const i = Math.trunc(key.v)
    if (i < 0 || i >= base.v.length) {
      throw new Error(`not_found: line ${lineno}: index ${i} out of range (len ${base.v.length})`)
    }
    return base.v[i]!
  }
  if (base.t === 'str' && key.t === 'num') {
    const chars = [...base.v]
    const i = Math.trunc(key.v)
    if (i < 0 || i >= chars.length) {
      throw new Error(`not_found: line ${lineno}: index ${i} out of range (len ${chars.length})`)
    }
    return vStr(chars[i]!)
  }
  throw new Error(
    `syntax: line ${lineno}: cannot index ${typeName(base)} with ${typeName(key)} (dicts take keys, lists/strings take numbers)`,
  )
}
/** JS/TS spellings → script grammar (`===`, `&&`, `||`, `!x`). */
function normalizeJs(src: string): string {
  return src.replaceAll('===', '==').replaceAll('!==', '!=').replaceAll('&&', ' and ').replaceAll('||', ' or ')
}

function splitTopCommas(src: string): string[] {
  const parts: string[] = []
  let cur = ''
  let quote: string | null = null
  let depth = 0
  for (let i = 0; i < src.length; i++) {
    const c = src[i]!
    if (quote) {
      cur += c
      if (c === quote) quote = null
      continue
    }
    if (c === '"' || c === "'") {
      quote = c
      cur += c
    } else if (c === '(' || c === '[' || c === '{') {
      depth++
      cur += c
    } else if (c === ')' || c === ']' || c === '}') {
      depth = Math.max(0, depth - 1)
      cur += c
    } else if (c === ',' && depth === 0) {
      parts.push(cur.trim())
      cur = ''
    } else cur += c
  }
  if (cur.trim() || parts.length) parts.push(cur.trim())
  return parts
}

/** `name = expr` / `let name [: type] = expr` / `const …` — null when not an assignment. */
function splitAssignment(text: string): { name: string; expr: string } | null {
  let quote: string | null = null
  let depth = 0
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!
    if (quote) {
      if (c === quote) quote = null
      continue
    }
    if (c === '"' || c === "'") {
      quote = c
      continue
    }
    if (c === '(' || c === '[' || c === '{') depth++
    else if (c === ')' || c === ']' || c === '}') depth = Math.max(0, depth - 1)
    else if (c === '=' && depth === 0) {
      const lhs = text.slice(0, i)
      const rhs = text.slice(i + 1)
      if (rhs.startsWith('=') || lhs.endsWith('!') || lhs.endsWith('<') || lhs.endsWith('>')) return null
      let name = lhs.trim()
      if (name.startsWith('let ')) name = name.slice(4).trim()
      else if (name.startsWith('const ')) name = name.slice(6).trim()
      else if (name.includes(' ')) return null
      const colon = name.indexOf(':')
      if (colon >= 0) name = name.slice(0, colon).trim()
      if (!validName(name) || !rhs.trim()) return null
      return { name, expr: rhs.trim() }
    }
  }
  return null
}

async function evalAssignRhs(r: Runner, src: string, lineno: number): Promise<Value> {
  const t = src.trim()
  if (isKw(t, 'sh')) {
    return r.inlineValue(unquoteArg(restOf(t, 'sh', lineno), lineno), lineno)
  }
  if (t.startsWith('js:')) return evalExpr(r, normalizeJs(t.slice(3).trim()), lineno)
  if (isKw(t, 'fetch')) {
    const rest = restOf(t, 'fetch', lineno)
    const spec = splitFetchFull(rest, lineno)
    const url = display(await evalExprAsync(r, spec.urlSrc, lineno))
    const headers = await evalFetchHeaders(r, spec.headersSrc, lineno)
    const body = await r.fetchChecked({ url, method: spec.method, headers, wantJson: spec.wantJson }, lineno)
    if (spec.wantJson) {
      const v = materializeJson(body)
      if (!v) throw new Error(`invalid: line ${lineno}: \`fetch\` did not return JSON`)
      r.setVar('_', v)
      return v
    }
    const v = vStr(body)
    r.setVar('_', v)
    return v
  }
  return evalExprAsync(r, t, lineno)
}

function unquoteArg(s: string, lineno: number): string {
  const t = s.trim()
  if (t.length >= 2 && ((t.startsWith('"') && t.endsWith('"')) || (t.startsWith("'") && t.endsWith("'")))) {
    return t.slice(1, -1)
  }
  if (t.startsWith('"') || t.startsWith("'")) throw new Error(`syntax: line ${lineno}: unclosed quote in \`${t}\``)
  return t
}

export interface FetchSpec {
  urlSrc: string
  method: string
  headersSrc: string | null
  wantJson: boolean
  asVar: string | null
}

/** Split `head KEYWORD tail` on a top-level keyword (outside quotes/brackets). */
function splitTopKw(src: string, kw: string): [string, string] | null {
  let quote: string | null = null
  let depth = 0
  for (let i = 0; i < src.length; i++) {
    const c = src[i]!
    if (quote) {
      if (c === quote) quote = null
      continue
    }
    if (c === '"' || c === "'") {
      quote = c
      continue
    }
    if (c === '(' || c === '[' || c === '{') depth++
    else if (c === ')' || c === ']' || c === '}') depth = Math.max(0, depth - 1)
    if (depth === 0 && /\s/.test(c)) {
      const tail = src.slice(i).trimStart()
      if (tail === kw || tail.startsWith(`${kw} `)) {
        return [src.slice(0, i).trim(), tail.slice(kw.length).trim()]
      }
    }
  }
  return null
}

/** Trailing `as json name` / `as name` (quote/bracket-aware, last wins). */
function parseFetchAs(rest: string): { head: string; wantJson: boolean; asVar: string | null } | null {
  let quote: string | null = null
  let depth = 0
  let lastAs = -1
  for (let i = 0; i < rest.length; i++) {
    const c = rest[i]!
    if (quote) {
      if (c === quote) quote = null
      continue
    }
    if (c === '"' || c === "'") {
      quote = c
      continue
    }
    if (c === '(' || c === '[' || c === '{') depth++
    else if (c === ')' || c === ']' || c === '}') depth = Math.max(0, depth - 1)
    if (depth === 0 && c === ' ') {
      const tail = rest.slice(i).trimStart()
      if (tail === 'as' || tail.startsWith('as ')) {
        const after = tail.slice(2)
        if (!after || after.startsWith(' ')) lastAs = i
      }
    }
  }
  if (lastAs < 0) return { head: rest.trim(), wantJson: false, asVar: null }
  const head = rest.slice(0, lastAs).trim()
  const tail = rest.slice(lastAs).trimStart().slice(2).trim()
  if (!tail) return null
  if (tail === 'json') return null
  if (tail.startsWith('json ')) {
    const name = tail.slice(4).trim()
    if (!validName(name)) return null
    return { head, wantJson: true, asVar: name }
  }
  if (!validName(tail)) return null
  return { head, wantJson: false, asVar: tail }
}

const FETCH_METHODS = new Set(['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD'])

function splitFetchFull(rest: string, lineno: number): FetchSpec {
  const asParsed = parseFetchAs(rest)
  if (!asParsed) {
    throw new Error(`syntax: line ${lineno}: bad \`fetch … as …\` (try \`fetch <url> [method M] [headers H] [as json <var> | as <var>]\`)`)
  }
  let head = asParsed.head
  let headersSrc: string | null = null
  const hsplit = splitTopKw(head, 'headers')
  if (hsplit && hsplit[0].trim()) {
    head = hsplit[0].trim()
    headersSrc = hsplit[1].trim()
    if (!headersSrc) throw new Error(`syntax: line ${lineno}: \`headers\` needs a dict`)
  }
  let method = 'GET'
  const msplit = splitTopKw(head, 'method')
  if (msplit) {
    const parts = msplit[1].trim().split(/\s+/)
    const m = (parts[0] ?? '').toUpperCase()
    if (!FETCH_METHODS.has(m)) {
      throw new Error(`syntax: line ${lineno}: bad fetch method \`${parts[0] ?? ''}\` (GET/POST/PUT/DELETE/PATCH/HEAD)`)
    }
    if (parts.length > 1) throw new Error(`syntax: line ${lineno}: \`method\` takes one word (\`method ${m}\` before \`headers\`)`)
    method = m
    head = msplit[0].trim()
  }
  if (!head) throw new Error(`syntax: line ${lineno}: \`fetch\` needs a URL`)
  return { urlSrc: head, method, headersSrc, wantJson: asParsed.wantJson, asVar: asParsed.asVar }
}

async function evalFetchHeaders(r: Runner, src: string | null, lineno: number): Promise<Record<string, string>> {
  if (!src) return {}
  const v = await evalExprAsync(r, src, lineno)
  if (v.t !== 'dict') throw new Error(`syntax: line ${lineno}: \`headers\` needs a dict (got ${typeName(v)})`)
  const out: Record<string, string> = {}
  for (const [k, val] of v.v) out[k] = display(val)
  return out
}

/** `import "path" [as prefix]` parts (path is an expression source). */
function splitImport(rest: string, lineno: number): { pathSrc: string; prefix: string | null } {
  const parts = splitTopKw(rest, 'as')
  if (parts) {
    const prefix = parts[1].trim()
    if (!validName(prefix)) throw new Error(`syntax: line ${lineno}: bad import prefix \`${prefix}\``)
    if (RESERVED_NAMES.has(prefix)) {
      throw new Error(`syntax: line ${lineno}: \`${prefix}\` is a builtin — pick another prefix`)
    }
    if (!parts[0].trim()) throw new Error(`syntax: line ${lineno}: \`import\` needs \`import "lib.cybsh" [as ns]\``)
    return { pathSrc: parts[0].trim(), prefix }
  }
  if (!rest.trim()) throw new Error(`syntax: line ${lineno}: \`import\` needs \`import "lib.cybsh" [as ns]\``)
  return { pathSrc: rest.trim(), prefix: null }
}

/** Load another `.cybsh` file's `def`s via `cat` (defs only). */
async function importDefs(r: Runner, path: string, prefix: string | null, lineno: number): Promise<void> {
  if (!path.toLowerCase().endsWith(CYBSH_SCRIPT_EXT)) {
    throw new Error(`invalid: line ${lineno}: \`import\` needs a ${CYBSH_SCRIPT_EXT} file (got \`${path}\`)`)
  }
  if (r.importDepth >= CYBSH_MAX_IMPORT_DEPTH) {
    throw new Error(`too_large: line ${lineno}: \`import\` nesting exceeds ${CYBSH_MAX_IMPORT_DEPTH}`)
  }
  const quoted = path.replace(/"/g, '\\"')
  let source: string
  try {
    source = String(await r.deps.execCybsh(`cat "${quoted}"`))
  } catch (e) {
    throw new Error(`${e instanceof Error ? e.message : String(e)} (line ${lineno})`)
  }
  if (source.length > CYBSH_MAX_SOURCE_BYTES) {
    throw new Error(`too_large: line ${lineno}: imported script exceeds ${CYBSH_MAX_SOURCE_BYTES} bytes`)
  }
  checkScriptVersion(source)
  const lines = splitLines(source)
  r.importDepth++
  try {
    let count = 0
    let i = 0
    while (i < lines.length) {
      const t = lines[i]!.text
      if (isKw(t, 'def')) {
        const header = stripColon(restOf(t, 'def', lines[i]!.lineno), lines[i]!.lineno)
        const { name: rawName, params } = splitDef(header, lines[i]!.lineno)
        const parent = lines[i]!.indent
        if (i + 1 >= lines.length || lines[i + 1]!.indent <= parent) {
          throw new Error(`syntax: line ${lineno}: imported \`def\` has no body`)
        }
        const end = skipBlock(lines, i + 1, parent)
        const name = prefix ? `${prefix}_${rawName}` : rawName
        if (r.funcs.size >= CYBSH_MAX_FUNCS && !r.funcs.has(name)) {
          throw new Error(`too_large: line ${lineno}: script holds ${CYBSH_MAX_FUNCS} functions already`)
        }
        r.funcs.set(name, { params, body: lines.slice(i + 1, end), definedAt: lines[i]!.lineno })
        count++
        i = end
      } else {
        i++
      }
    }
    if (!count) throw new Error(`not_found: line ${lineno}: \`${path}\` defines no \`def\`s to import`)
  } finally {
    r.importDepth--
  }
}

/** `await <expr> [timeout N]` parts (N in 1..MAX, default 10). */
function splitAwait(rest: string, lineno: number): { condSrc: string; tries: number } {
  const parts = splitTopKw(rest, 'timeout')
  if (parts) {
    const words = parts[1].trim().split(/\s+/)
    const n = Number(words[0])
    if (!words[0] || !Number.isInteger(n) || n < 1 || n > CYBSH_MAX_AWAIT_TRIES) {
      throw new Error(`syntax: line ${lineno}: \`timeout\` needs a number 1–${CYBSH_MAX_AWAIT_TRIES}`)
    }
    if (words.length > 1) throw new Error(`syntax: line ${lineno}: \`await … timeout N\` takes nothing after N`)
    if (!parts[0].trim()) throw new Error(`syntax: line ${lineno}: \`await\` needs an expression`)
    return { condSrc: parts[0].trim(), tries: n }
  }
  if (!rest.trim()) throw new Error(`syntax: line ${lineno}: \`await\` needs an expression`)
  return { condSrc: rest.trim(), tries: CYBSH_DEFAULT_AWAIT_TRIES }
}

/** Is this line a `match` arm header? */
function isMatchArm(text: string): boolean {
  // Parenthesized bindings (`ok(v):`, `err(e):`) count — the gate must
  // agree with `matchArmBinding`, or arms are skipped silently instead of
  // running (or refusing loudly on bad syntax).
  return text === 'else:' || text === 'else' || text === 'ok:' || text === 'err:' || isKw(text, 'ok') || isKw(text, 'err') || text.startsWith('ok(') || text.startsWith('err(')
}

/** `ok(v):` / `err(e):` / `else:` arm binding. */
function matchArmBinding(text: string, lineno: number): { kind: string; binding: string | null } {
  const inner = text.trim().replace(/:+$/, '').trim()
  if (inner === 'else') return { kind: 'else', binding: null }
  for (const kind of ['ok', 'err']) {
    if (inner === kind) return { kind, binding: null }
    if (inner.startsWith(kind)) {
      const rest = inner.slice(kind.length).trim()
      const name = rest.replace(/^\(/, '').replace(/\)$/, '').trim()
      if (rest && validName(name) && (rest.startsWith('(') || rest.startsWith(' '))) {
        return { kind, binding: name }
      }
    }
  }
  throw new Error(`syntax: line ${lineno}: bad \`match\` arm \`${text}\` (try \`ok(v):\`, \`err(e):\`, \`else:\`)`)
}

/** Evaluate a match target, capturing failures as err material. */
async function evalMatchTarget(r: Runner, src: string, lineno: number): Promise<{ ok: true; value: Value } | { ok: false; error: string }> {
  const t = src.trim()
  if (isKw(t, 'sh')) {
    const cmdline = unquoteArg(restOf(t, 'sh', lineno), lineno)
    try {
      return { ok: true as const, value: await r.inlineValue(cmdline, lineno) }
    } catch (e) {
      return { ok: false as const, error: e instanceof Error ? e.message : String(e) }
    }
  }
  try {
    return { ok: true as const, value: await evalExprAsync(r, src, lineno) }
  } catch (e) {
    return { ok: false as const, error: e instanceof Error ? e.message : String(e) }
  }
}

/** Run `match <expr>:` — target errors become the `err` arm. */
async function runMatch(
  r: Runner,
  lines: SrcLine[],
  idx: number,
  targetSrc: string,
  lineno: number,
  parent: number,
  end: number,
): Promise<number> {
  const target = await evalMatchTarget(r, targetSrc, lineno)
  let isErr = !target.ok
  let bindValue: Value = vNull()
  let errText = ''
  if (target.ok) {
    const c = classifyResult(target.value)
    isErr = c.isErr
    bindValue = c.value
    errText = c.err
  } else {
    errText = target.error
  }
  const armParent = lines[idx + 1]!.indent
  let okArm: { binding: string | null; start: number; end: number } | null = null
  let errArm: { binding: string | null; start: number; end: number } | null = null
  let elseArm: { start: number; end: number } | null = null
  let j = idx + 1
  while (j < end) {
    if (lines[j]!.indent !== armParent || !isMatchArm(lines[j]!.text)) {
      j = skipBlock(lines, j + 1, armParent)
      continue
    }
    const { kind, binding } = matchArmBinding(lines[j]!.text, lines[j]!.lineno)
    const start = j + 1
    const armEnd = skipBlock(lines, start, armParent)
    if (kind === 'ok' && !okArm) okArm = { binding, start, end: armEnd }
    else if (kind === 'err' && !errArm) errArm = { binding, start, end: armEnd }
    else if (kind === 'else' && !elseArm) elseArm = { start, end: armEnd }
    else throw new Error(`syntax: line ${lines[j]!.lineno}: duplicate \`match\` arm \`${kind}\``)
    j = armEnd
  }
  if (isErr) {
    if (errArm) {
      if (errArm.binding) r.setVar(errArm.binding, vStr(trunc(errText)))
      await runBlock(r, lines, errArm.start, armParent)
      r.setVar('_', vStr(trunc(errText)))
    } else if (elseArm) {
      await runBlock(r, lines, elseArm.start, armParent)
    }
  } else if (okArm) {
    if (okArm.binding) r.setVar(okArm.binding, bindValue)
    r.setVar('_', bindValue)
    await runBlock(r, lines, okArm.start, armParent)
  } else if (elseArm) {
    r.setVar('_', bindValue)
    await runBlock(r, lines, elseArm.start, armParent)
  } else {
    r.setVar('_', bindValue)
  }
  return end
}

/** `catch`, `catch:`, `catch e`, `catch e:` — the `try` sibling. */
function isCatch(text: string): boolean {
  return text === 'catch' || text === 'catch:' || isKw(text, 'catch')
}

function catchBinding(text: string, lineno: number): string | null {
  if (text === 'catch' || text === 'catch:') return null
  const name = restOf(text, 'catch', lineno).replace(/:+$/, '').trim()
  if (!name) return null
  if (!validName(name)) throw new Error(`syntax: line ${lineno}: bad catch binding \`${name}\``)
  return name
}

/** Builtins, literals and operators a `def` may not shadow. */
const RESERVED_NAMES = new Set([
  'len', 'int', 'str', 'json', 'split', 'range', 'sh', 'set', 'push',
  'del', 'keys', 'values', 'ok', 'err', 'unwrap', 'is_ok', 'is_err',
  'env', 'arg', 'fingerprint', 'true', 'false', 'null', 'none', 'nil',
  'and', 'or', 'not',
])

/** `name(p1, p2)` header of a `def` (parens required, names validated). */
function splitDef(header: string, lineno: number): { name: string; params: string[] } {
  const open = header.indexOf('(')
  if (open < 0) throw new Error(`syntax: line ${lineno}: \`def\` needs \`def name(p1, …):\``)
  const name = header.slice(0, open).trim()
  if (!validName(name)) throw new Error(`syntax: line ${lineno}: bad function name \`${name}\``)
  if (RESERVED_NAMES.has(name)) {
    throw new Error(`syntax: line ${lineno}: \`${name}\` is a builtin — pick another function name`)
  }
  const rest = header.slice(open + 1).trim()
  if (!rest.endsWith(')')) throw new Error(`syntax: line ${lineno}: \`def\` params need a closing \`)\``)
  const params: string[] = []
  for (const p of rest.slice(0, -1).split(',')) {
    const t = p.trim()
    if (!t) continue
    if (!validName(t) || params.includes(t)) {
      throw new Error(`syntax: line ${lineno}: bad/duplicate param \`${t}\``)
    }
    params.push(t)
  }
  if (params.length > 8) throw new Error(`syntax: line ${lineno}: \`def\` takes at most 8 params`)
  return { name, params }
}

/**
 * Call a user function: bind params as locals, run the body, restore.
 * Globals are readable inside; assignments stay local (Starlark rule).
 */
async function callFunc(r: Runner, name: string, args: Value[], lineno: number): Promise<Value> {
  const def = r.funcs.get(name)
  if (!def) {
    throw new Error(
      `syntax: line ${lineno}: unknown function \`${name}\` (try \`${BUILTIN_HINT}\` or \`def\` it first)`,
    )
  }
  if (args.length !== def.params.length) {
    throw new Error(
      `syntax: line ${lineno}: \`${name}\` takes ${def.params.length} argument(s), got ${args.length}`,
    )
  }
  if (r.callDepth >= CYBSH_MAX_CALL_DEPTH) {
    throw new Error(`too_large: line ${lineno}: call depth exceeds ${CYBSH_MAX_CALL_DEPTH} (recursive \`def\`?)`)
  }
  const savedVars = r.vars
  const savedFlow = r.flow
  // Params shadow globals of the same name; every other global stays
  // readable. Assignments never escape (restored below — Starlark rule).
  r.vars = new Map(savedVars)
  for (let i = 0; i < def.params.length; i++) r.vars.set(def.params[i]!, args[i]!)
  r.callDepth++
  try {
    const parent = def.body.length ? def.body[0]!.indent - 1 : -1
    await runBlock(r, def.body, 0, parent)
    return r.flow ?? vNull()
  } finally {
    r.flow = savedFlow
    r.vars = savedVars
    r.callDepth--
  }
}

/** First quote-aware `name(` call for a user `def`. Null when absent. */
function findFirstUserCall(src: string, funcs: Map<string, FuncDef>): { start: number; end: number; name: string; inner: string } | null {
  let quote: string | null = null
  for (let i = 0; i < src.length; i++) {
    const c = src[i]!
    if (quote) {
      if (c === quote) quote = null
      continue
    }
    if (c === '"' || c === "'") {
      quote = c
      continue
    }
    if (/[A-Za-z_]/.test(c) && !/[A-Za-z0-9_.]/.test(src[i - 1] ?? '')) {
      let j = i
      while (j < src.length && /[A-Za-z0-9_]/.test(src[j]!)) j++
      const name = src.slice(i, j)
      if (!funcs.has(name)) {
        i = j - 1
        continue
      }
      let k = j
      while (k < src.length && /\s/.test(src[k]!)) k++
      if (src[k] !== '(') {
        i = j - 1
        continue
      }
      let depth = 0
      let q2: string | null = null
      for (let m = k; m < src.length; m++) {
        const d = src[m]!
        if (q2) {
          if (d === q2) q2 = null
          continue
        }
        if (d === '"' || d === "'") {
          q2 = d
          continue
        }
        if (d === '(') depth++
        else if (d === ')') {
          depth--
          if (depth === 0) return { start: i, end: m + 1, name, inner: src.slice(k + 1, m) }
        }
      }
      throw new Error(`syntax: unclosed \`${name}(\` — expected a matching \`)\``)
    }
  }
  return null
}

/** Serialize a value back into expression grammar (for call substitution). */
function valueToLiteral(v: Value): string {
  // Round-trip through JSON, not display: `display` prints dict/list
  // string items bare (`{theme: os-dark}`), which re-parses as variable
  // lookups (`not_found: no variable …`). JSON is valid cybsh literal
  // syntax (quoted keys, nested lists/dicts), so `let x = sh("… --json")`
  // survives hoisting whenever the shell answers JSON with strings in it.
  // (Scalars keep their old shape: strings were already JSON-quoted here,
  // and numbers/bools/null spell identically in both.)
  return JSON.stringify(valueToJson(v))
}

/** Plain-JSON mirror of a script value (keys stay sorted by construction). */
function valueToJson(v: Value): unknown {
  switch (v.t) {
    case 'null': return null
    case 'bool': return v.v
    case 'num': return v.v
    case 'str': return v.v
    case 'list': return v.v.map(valueToJson)
    case 'dict': return Object.fromEntries([...v.v.entries()].map(([k, item]) => [k, valueToJson(item)]))
  }
}

async function runStatement(r: Runner, lines: SrcLine[], idx: number): Promise<number> {
  const line = lines[idx]!
  const { text, lineno } = line
  r.bump()

  if (isKw(text, 'if')) {
    const cond = stripColon(restOf(text, 'if', lineno), lineno)
    blockIndent(lines, idx + 1, line.indent)
    if (truthy(await evalExprAsync(r, cond, lineno))) {
      const end = await runBlock(r, lines, idx + 1, line.indent)
      return skipElifChain(lines, end, line.indent)
    }
    let j = skipBlock(lines, idx + 1, line.indent)
    for (;;) {
      if (j < lines.length && lines[j]!.indent === line.indent) {
        const t = lines[j]!.text
        if (isKw(t, 'elif')) {
          const c = stripColon(restOf(t, 'elif', lines[j]!.lineno), lines[j]!.lineno)
          if (truthy(await evalExprAsync(r, c, lines[j]!.lineno))) {
            const end = await runBlock(r, lines, j + 1, line.indent)
            return skipElifChain(lines, end, line.indent)
          }
          j = skipBlock(lines, j + 1, line.indent)
          continue
        }
        if (t === 'else:' || t === 'else') {
          const end = await runBlock(r, lines, j + 1, line.indent)
          return skipElifChain(lines, end, line.indent)
        }
      }
      return j
    }
  }
  if (isKw(text, 'elif') || text === 'else:' || text === 'else') {
    throw new Error(`syntax: line ${lineno}: \`${text.split(' ')[0]}\` without \`if\``)
  }

  if (isKw(text, 'while')) {
    const cond = stripColon(restOf(text, 'while', lineno), lineno)
    blockIndent(lines, idx + 1, line.indent)
    let iters = 0
    while (truthy(await evalExprAsync(r, cond, lineno))) {
      if (iters >= CYBSH_MAX_ITERS) throw new Error(`too_large: \`while\` exceeded ${CYBSH_MAX_ITERS} iterations (line ${lineno})`)
      await runBlock(r, lines, idx + 1, line.indent)
      iters++
      r.bump()
    }
    return skipBlock(lines, idx + 1, line.indent)
  }

  if (isKw(text, 'for')) {
    const header = stripColon(restOf(text, 'for', lineno), lineno)
    const m = header.match(/^([A-Za-z_][A-Za-z0-9_]*)\s+in\s+(.+)$/)
    if (!m?.[2]) throw new Error(`syntax: line ${lineno}: \`for\` needs \`for <name> in <expr>:\``)
    const name = m[1]!
    const items = await forItems(r, m[2].trim(), lineno)
    blockIndent(lines, idx + 1, line.indent)
    if (items.length > CYBSH_MAX_ITERS) {
      throw new Error(`too_large: \`for\` has ${items.length} items, limit is ${CYBSH_MAX_ITERS} (line ${lineno})`)
    }
    for (const item of items) {
      r.setVar(name, item)
      await runBlock(r, lines, idx + 1, line.indent)
      r.bump()
    }
    r.vars.delete(name)
    return skipBlock(lines, idx + 1, line.indent)
  }

  if (text === 'print' || text === 'print()' || isKw(text, 'print') || text.startsWith('print(')) {
    let arg = ''
    if (text !== 'print' && text !== 'print()') {
      arg = text.startsWith('print(') && text.endsWith(')') ? text.slice(6, -1) : restOf(text, 'print', lineno)
    }
    if (!arg.trim()) r.emit('')
    else {
      const parts = splitTopCommas(arg)
      if (parts.length === 1) r.emit(display(await evalExprAsync(r, arg.trim(), lineno)))
      else {
        const vals: string[] = []
        for (const p of parts) vals.push(display(await evalExprAsync(r, p, lineno)))
        r.emit(vals.join(' '))
      }
    }
    return idx + 1
  }

  if (isKw(text, 'fetch')) {
    const spec = splitFetchFull(restOf(text, 'fetch', lineno), lineno)
    const url = display(await evalExprAsync(r, spec.urlSrc, lineno))
    const headers = await evalFetchHeaders(r, spec.headersSrc, lineno)
    const req: CybshFetchReq = { url, method: spec.method, headers, wantJson: spec.wantJson }
    const body = await r.fetchChecked(req, lineno)
    if (spec.wantJson) {
      const v = materializeJson(body)
      if (!v) throw new Error(`invalid: line ${lineno}: \`fetch ${url}\` did not return JSON (try plain \`as\`)`)
      if (spec.asVar) {
        r.setVar(spec.asVar, v)
        r.emit(`fetched ${url} (json → ${spec.asVar})`)
      } else {
        r.setVar('_', v)
        r.emit(display(v))
      }
    } else if (spec.asVar) {
      if (!validName(spec.asVar)) throw new Error(`syntax: line ${lineno}: bad variable name '${spec.asVar}'`)
      r.setVar(spec.asVar, vStr(body))
      r.emit(`fetched ${url} (${body.length} bytes → ${spec.asVar})`)
    } else {
      r.setVar('_', vStr(body))
      r.emit(body)
    }
    return idx + 1
  }

  // `import "lib.cybsh" [as ns]` — merge another file's `def`s (defs only).
  if (isKw(text, 'import')) {
    const rest = restOf(text, 'import', lineno).trim()
    const { pathSrc, prefix } = splitImport(rest, lineno)
    const path = display(await evalExprAsync(r, pathSrc, lineno))
    await importDefs(r, path, prefix, lineno)
    return idx + 1
  }

  // `with [name = expr]:` — scoped block (assignments inside don't escape).
  if (text === 'with:' || text === 'with' || isKw(text, 'with')) {
    let rest = ''
    if (text !== 'with:' && text !== 'with') {
      rest = stripColon(restOf(text, 'with', lineno).trim(), lineno)
    }
    const parent = line.indent
    blockIndent(lines, idx + 1, parent)
    const end = skipBlock(lines, idx + 1, parent)
    const savedVars = r.vars
    const savedFlow = r.flow
    r.vars = new Map(savedVars)
    try {
      if (rest) {
        const m = rest.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([\s\S]+)$/)
        if (!m?.[1] || !m?.[2]) throw new Error(`syntax: line ${lineno}: \`with\` needs \`with [name = expr]:\``)
        r.setVar(m[1], await evalExprAsync(r, m[2].trim(), lineno))
      }
      await runBlock(r, lines, idx + 1, parent)
    } finally {
      r.vars = savedVars
      r.flow = savedFlow
    }
    return end
  }

  // `match <expr>:` with `ok(v):/err(e):/else:` arms.
  if (isKw(text, 'match')) {
    const targetSrc = stripColon(restOf(text, 'match', lineno), lineno)
    const parent = line.indent
    blockIndent(lines, idx + 1, parent)
    const end = skipBlock(lines, idx + 1, parent)
    return runMatch(r, lines, idx, targetSrc, lineno, parent, end)
  }
  if (text === 'ok:' || text === 'err:' || isKw(text, 'ok') || isKw(text, 'err')) {
    throw new Error(`syntax: line ${lineno}: \`ok/err\` arms need \`match\``)
  }

  // `await <expr> [timeout N]` — logical poll until truthy.
  if (isKw(text, 'await')) {
    const { condSrc, tries } = splitAwait(restOf(text, 'await', lineno), lineno)
    let lastErr: string | null = null
    let lastVal: Value = vNull()
    for (let i = 0; i < tries; i++) {
      r.bump()
      try {
        const v = await evalExprAsync(r, condSrc, lineno)
        lastVal = v
        if (truthy(v)) {
          r.setVar('_', v)
          return idx + 1
        }
      } catch (e) {
        lastErr = e instanceof Error ? e.message : String(e)
      }
    }
    throw new Error(
      lastErr
        ? `timeout: line ${lineno}: \`await\` still failing after ${tries} tries (last: ${lastErr})`
        : `timeout: line ${lineno}: \`await\` still falsy after ${tries} tries`,
    )
  }

  if (text === 'vars') {
    const names = [...r.vars.keys()].sort()
    if (!names.length) r.emit('(no variables)')
    else for (const name of names) r.emit(`${name}: ${typeName(r.vars.get(name)!)} = ${truncShow(display(r.vars.get(name)!))}`)
    return idx + 1
  }
  if (isKw(text, 'free')) {
    const name = restOf(text, 'free', lineno).trim()
    if (!r.vars.delete(name)) throw new Error(`not_found: no variable \`${name}\` (line ${lineno})`)
    r.emit(`freed ${name}`)
    return idx + 1
  }
  if (text === 'gc' || text === 'gc --apply') {
    const before = [...r.vars.values()].reduce((n, v) => n + display(v).length, 0)
    r.vars.delete('_')
    const after = [...r.vars.values()].reduce((n, v) => n + display(v).length, 0)
    r.emit(`gc: ${r.vars.size} var(s) alive, released ~${before - after} byte(s) of last-output buffer (values are owned — no tracing collector needed; \`free <name>\` drops a binding)`)
    return idx + 1
  }

  if (isKw(text, 'js')) {
    const v = evalExpr(r, normalizeJs(restOf(text, 'js', lineno)), lineno)
    r.emit(display(v))
    return idx + 1
  }

  // `try:` + `catch [var]:` — explicit failure handling (Elvish over bash).
  if (text === 'try:' || text === 'try') {
    const parent = line.indent
    blockIndent(lines, idx + 1, parent)
    const bodyEnd = skipBlock(lines, idx + 1, parent)
    try {
      await runBlock(r, lines, idx + 1, parent)
      let j = bodyEnd
      if (j < lines.length && lines[j]!.indent === parent && isCatch(lines[j]!.text)) {
        j = skipBlock(lines, j + 1, parent)
      }
      return j
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      let j = bodyEnd
      if (j < lines.length && lines[j]!.indent === parent && isCatch(lines[j]!.text)) {
        const t = lines[j]!.text
        const lj = lines[j]!.lineno
        const binding = catchBinding(t, lj)
        // An error wins over any pending `return`.
        r.flow = null
        if (binding) r.setVar(binding, vStr(trunc(msg)))
        return runBlock(r, lines, j + 1, parent)
      }
      throw e instanceof Error ? e : new Error(msg)
    }
  }
  if (isCatch(text)) {
    throw new Error(`syntax: line ${lineno}: \`catch\` without \`try\``)
  }

  // `def name(p1, p2):` — user functions (lexical, owned, no global mutation).
  if (isKw(text, 'def')) {
    const header = stripColon(restOf(text, 'def', lineno), lineno)
    const { name, params } = splitDef(header, lineno)
    const parent = line.indent
    blockIndent(lines, idx + 1, parent)
    const end = skipBlock(lines, idx + 1, parent)
    if (r.funcs.size >= CYBSH_MAX_FUNCS && !r.funcs.has(name)) {
      throw new Error(`too_large: script holds ${CYBSH_MAX_FUNCS} functions already`)
    }
    r.funcs.set(name, { params, body: lines.slice(idx + 1, end), definedAt: lineno })
    return end
  }

  // `return [expr]` — only inside `def`.
  if (text === 'return' || isKw(text, 'return')) {
    if (r.callDepth === 0) throw new Error(`syntax: line ${lineno}: \`return\` outside \`def\``)
    r.flow = text === 'return' ? vNull() : await evalExprAsync(r, restOf(text, 'return', lineno), lineno)
    return idx + 1
  }

  // `fail "msg"` — raise a script error (`fail: msg`, caught by `catch`).
  if (isKw(text, 'fail')) {
    throw new Error(`fail: ${display(await evalExprAsync(r, restOf(text, 'fail', lineno), lineno))}`)
  }

  if (text.startsWith('$ ') || text === '$') {
    const cmdline = text.slice(1).trim()
    if (!cmdline) return idx + 1
    await r.runInline(cmdline, lineno)
    return idx + 1
  }

  const assign = splitAssignment(text)
  if (assign) {
    const v = await evalAssignRhs(r, assign.expr, lineno)
    if (v.t === 'str' && v.v.length > CYBSH_MAX_STR) {
      throw new Error(`too_large: line ${lineno}: value is ${v.v.length} bytes, limit is ${CYBSH_MAX_STR} (\`sh\` output is capped automatically; split the data)`)
    }
    r.setVar(assign.name, v)
    return idx + 1
  }

  if (isKw(text, 'sh')) {
    await r.runInline(unquoteArg(restOf(text, 'sh', lineno), lineno), lineno)
    return idx + 1
  }

  // Bare cybsh line, e.g. `ls /` or `sync status` (same table as `help`).
  const verb = text.split(/\s+/, 1)[0]!
  if (BARE_VERBS.has(verb)) {
    await r.runInline(text, lineno)
    return idx + 1
  }

  // Anything else is tried as a bare expression (Oils rule: one way to run
  // code). Non-null results print, like `sh "…"` lines do; a typo still
  // fails loudly, with expression errors attached.
  try {
    const v = await evalExprAsync(r, text, lineno)
    if (v.t !== 'null') r.emit(display(v))
    r.setVar('_', v)
    return idx + 1
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    throw new Error(
      `syntax: line ${lineno}: unknown statement \`${truncShow(text, 60)}\` (${msg}; try \`print\`, \`let\`, \`def\`, \`if/elif/else\`, \`match\`, \`with\`, \`import\`, \`await\`, \`try/catch\`, \`for\`, \`while\`, \`$ <cybsh>\`, \`sh "…"\`, \`js …\`, \`fetch …\`, \`ui …\`, \`vars/free/gc\`)`,
    )
  }
}

async function forItems(r: Runner, exprSrc: string, lineno: number): Promise<Value[]> {
  const v = await evalExprAsync(r, exprSrc, lineno)
  if (v.t === 'list') return v.v
  // Dicts iterate their sorted keys (deterministic on every run).
  if (v.t === 'dict') return [...v.v.keys()].sort().map((k) => vStr(k))
  if (v.t === 'str') {
    return v.v.includes('\n') ? v.v.split('\n').map((s) => vStr(s)) : v.v.split(/\s+/).filter((s) => s.length > 0).map((s) => vStr(s))
  }
  if (v.t === 'num') {
    const items: Value[] = []
    for (let i = 0; i < Math.max(0, Math.floor(v.v)); i++) items.push(vNum(i))
    return items
  }
  throw new Error(`syntax: line ${lineno}: \`for\` needs a list, \`range()\`, or a string — got ${typeName(v)}`)
}

/** Parse and execute a script. Inline `sh` lines go through `execCybsh`. */
export async function runCybshScript(
  source: string,
  deps: CybshScriptDeps,
  args: string[] = [],
): Promise<CybshScriptResult> {
  if (source.length > CYBSH_MAX_SOURCE_BYTES) {
    throw new Error(`too_large: script is ${source.length} bytes, limit is ${CYBSH_MAX_SOURCE_BYTES}`)
  }
  checkScriptVersion(source)
  const lines = splitLines(source)
  const r = new Runner(deps)
  r.caps = parseScriptCaps(source)
  r.setVar('args', vList(args.map((a) => vStr(a))))
  // Top level runs at parent depth -1 so indent-0 lines execute.
  await runBlock(r, lines, 0, -1)
  let output = r.chunks.join('\n')
  if (r.truncated) {
    output += `\n… output truncated at ${CYBSH_MAX_OUTPUT_BYTES} bytes (fewer \`print\`/\`sh\` lines for the full log)`
  }
  const fm = parseFrontmatter(source)
  return {
    output,
    effects: r.effects,
    vars: r.vars.size,
    caps: r.caps.active
      ? { active: true, net: r.caps.net, read: r.caps.read, write: r.caps.write, deny: r.caps.deny }
      : null,
    shCalls: r.shCalls,
    fetchCalls: r.fetchCalls,
    schedule: fm.schedule,
    triggers: fm.triggers,
  }
}
