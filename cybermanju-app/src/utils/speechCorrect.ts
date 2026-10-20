// CyberManju OS — SpeechCorrect (src/utils/speechCorrect.ts).
// Spoken-word → text normalisation for voice input.
//
// Speech recognition returns words ("open bracket", "new line", "see dee").
// This module turns them into characters before they reach the agent prompt,
// the code editor, or cybsh — per mode, so prose keeps its capitals while
// code/shell get symbols and dictionary fixes.

import { bestMatch } from './fuzzyCorrect'

export type VoiceMode = 'prose' | 'code' | 'shell'

export type VoiceLang = 'en-US' | 'pt-BR'

export const VOICE_LANGS: readonly VoiceLang[] = ['en-US', 'pt-BR']

export const VOICE_LANG_STORAGE_KEY = 'cybermanju.voice.lang.v1'

/** User-facing preference: explicit EN/PT-BR, or `auto` (browser-detected). */
export type VoiceLangPref = 'auto' | VoiceLang

/** True for `pt`, `pt-BR`, `pt-PT`, … — anything the STT engine treats as Portuguese. */
export function isPtLang(lang: string): boolean {
  return lang.trim().toLowerCase().startsWith('pt')
}

/**
 * Default recognition language: Brazilian Portuguese when the browser
 * prefers Portuguese, English otherwise. Accepts an override (for tests)
 * falling back to `navigator.language`.
 */
export function detectVoiceLang(input?: string): VoiceLang {
  const raw =
    input ??
    (typeof navigator !== 'undefined' ? (navigator.language ?? navigator.languages?.[0] ?? '') : '')
  return isPtLang(raw ?? '') ? 'pt-BR' : 'en-US'
}

/**
 * Resolve an effective STT language from a user preference. `auto` (the
 * default — neither EN nor PT picked) follows the browser locale at call
 * time, so dictation autodetects the user's language on every use.
 */
export function resolveVoiceLang(pref: VoiceLangPref, input?: string): VoiceLang {
  return pref === 'auto' ? detectVoiceLang(input) : pref
}

/** Lowercase + strip diacritics so `cê dê` matches `ce de`, `vírgula` matches `virgula`. */
export function foldAccents(s: string): string {
  return s
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
}

const PROSE_PUNCT: Array<[RegExp, string]> = [
  [/\bnew paragraph\b/gi, '\n\n'],
  [/\bnew line\b/gi, '\n'],
  [/\bquestion mark\b/gi, '?'],
  [/\bexclamation (mark|point)\b/gi, '!'],
  [/\bopen quote\b/gi, ' "'],
  [/\bclose quote\b/gi, '" '],
  [/\bcomma\b/gi, ','],
  [/\bperiod\b/gi, '.'],
  [/\bcolon\b/gi, ':'],
  [/\bsemicolon\b/gi, ';'],
]

const CODE_WORDS: Array<[RegExp, string]> = [
  [/\bopen paren(?:thesis|theses)?\b/gi, '('],
  [/\bclose paren(?:thesis|theses)?\b/gi, ')'],
  [/\bopen bracket\b/gi, '['],
  [/\bclose bracket\b/gi, ']'],
  [/\bopen brace\b/gi, '{'],
  [/\bclose brace\b/gi, '}'],
  [/\bopen angle\b/gi, '<'],
  [/\bclose angle\b/gi, '>'],
  [/\bdouble equals\b/gi, '=='],
  [/\bfat arrow\b/gi, '=>'],
  [/\barrow\b/gi, '->'],
  [/\bless than\b/gi, '<'],
  [/\bgreater than\b/gi, '>'],
  [/\bdouble quote\b/gi, '"'],
  [/\bsingle quote\b/gi, "'"],
  [/\bback ?tick\b/gi, '`'],
  [/\bunderscore\b/gi, '_'],
  [/\bhyphen\b/gi, '-'],
  [/\bforward slash\b/gi, '/'],
  [/\bbackslash\b/gi, '\\'],
  [/\bvertical bar\b/gi, '|'],
  [/\bampersand\b/gi, '&'],
  [/\bat sign\b/gi, '@'],
  [/\bhash(?:tag)?\b/gi, '#'],
  [/\bdollar(?: sign)?\b/gi, '$'],
  [/\bpercent\b/gi, '%'],
  [/\bcaret\b/gi, '^'],
  [/\btilde\b/gi, '~'],
  [/\basterisk\b/gi, '*'],
  [/\bplus\b/gi, '+'],
  [/\bequals?(?: sign)?\b/gi, '='],
  [/\bcolon\b/gi, ':'],
  [/\bsemicolon\b/gi, ';'],
  [/\bcomma\b/gi, ','],
  [/\bdot\b/gi, '.'],
  [/\bquestion mark\b/gi, '?'],
  [/\bexclamation (mark|point)\b/gi, '!'],
  [/\bnew line\b/gi, '\n'],
  [/\btab\b/gi, '\t'],
  [/\bspace\b/gi, ' '],
]

// ─── pt-BR twins: same symbols, Portuguese triggers ─────────────────────
// Accent variants are spelled out (`virgula`/`vírgula`) so matching works
// with or without diacritics, whichever the STT engine returns. Long
// phrases come first so `ponto de interrogação` wins over bare `ponto`.

const PROSE_PUNCT_PT: Array<[RegExp, string]> = [
  [/\bnov[oa] par[áa]grafo\b/gi, '\n\n'],
  [/\bquebra de linha\b/gi, '\n'],
  [/\bnova linha\b/gi, '\n'],
  [/\bponto de interroga[çc][ãa]o\b/gi, '?'],
  [/\bponto de exclama[çc][ãa]o\b/gi, '!'],
  [/\babre aspas\b/gi, ' "'],
  [/\bfecha aspas\b/gi, '" '],
  [/\bponto e v[íi]rgula\b/gi, ';'],
  [/\bdois pontos\b/gi, ':'],
  [/\bponto final\b/gi, '.'],
  [/\bv[íi]rgula\b/gi, ','],
  [/\bponto\b/gi, '.'],
]

const CODE_WORDS_PT: Array<[RegExp, string]> = [
  [/\babre par[êe]nteses?\b/gi, '('],
  [/\bfecha par[êe]nteses?\b/gi, ')'],
  [/\babre colchetes?\b/gi, '['],
  [/\bfecha colchetes?\b/gi, ']'],
  [/\babre chaves?\b/gi, '{'],
  [/\bfecha chaves?\b/gi, '}'],
  [/\babre [âa]ngulo\b/gi, '<'],
  [/\bfecha [âa]ngulo\b/gi, '>'],
  [/\bigual igual\b/gi, '=='],
  [/\bduplo igual\b/gi, '=='],
  [/\bseta gorda\b/gi, '=>'],
  [/\bseta\b/gi, '->'],
  [/\bmenor que\b/gi, '<'],
  [/\bmaior que\b/gi, '>'],
  [/\baspas duplas\b/gi, '"'],
  [/\baspas simples\b/gi, "'"],
  [/\bcrase\b/gi, '`'],
  [/\bacento grave\b/gi, '`'],
  [/\bsublinhado\b/gi, '_'],
  [/\bunderline\b/gi, '_'],
  [/\bh[íi]fen\b/gi, '-'],
  [/\bbarra invertida\b/gi, '\\'],
  [/\bbarra vertical\b/gi, '|'],
  [/\bbarra\b/gi, '/'],
  [/\be comercial\b/gi, '&'],
  [/\barroba\b/gi, '@'],
  [/\bcerquilha\b/gi, '#'],
  [/\bhashtag\b/gi, '#'],
  [/\bjogo da velha\b/gi, '#'],
  [/\bcifr[ãa]o\b/gi, '$'],
  [/\bpor ?cento\b/gi, '%'],
  [/\bporcentagem\b/gi, '%'],
  [/\bcircunflexo\b/gi, '^'],
  [/\btil\b/gi, '~'],
  [/\basterisco\b/gi, '*'],
  [/\bmais\b/gi, '+'],
  [/\bigual\b/gi, '='],
  [/\bdois pontos\b/gi, ':'],
  [/\bponto e v[íi]rgula\b/gi, ';'],
  [/\bv[íi]rgula\b/gi, ','],
  [/\bponto de interroga[çc][ãa]o\b/gi, '?'],
  [/\bponto de exclama[çc][ãa]o\b/gi, '!'],
  [/\bponto final\b/gi, '.'],
  [/\bponto\b/gi, '.'],
  [/\bquebra de linha\b/gi, '\n'],
  [/\bnova linha\b/gi, '\n'],
  [/\btabula[çc][ãa]o\b/gi, '\t'],
  [/\btab\b/gi, '\t'],
  [/\bespa[çc]o\b/gi, ' '],
]

/** "camel case foo bar" → "fooBar", "snake case Foo Bar" → "foo_bar", etc. */
function applyCasePhrases(s: string): string {
  return s
    .replace(/\bcamel case ((?:[a-z]+\s?)+)/gi, (_, words: string) => {
      const parts = words.trim().split(/\s+/)
      return parts.map((p, i) => (i === 0 ? p.toLowerCase() : p[0].toUpperCase() + p.slice(1).toLowerCase())).join('')
    })
    .replace(/\bsnake case ((?:[a-z]+\s?)+)/gi, (_, words: string) => words.trim().toLowerCase().split(/\s+/).join('_'))
    .replace(/\bkebab case ((?:[a-z]+\s?)+)/gi, (_, words: string) => words.trim().toLowerCase().split(/\s+/).join('-'))
}

/** Collapse spaces that symbol replacement leaves behind: `foo . bar` → `foo.bar`. */
function glueSymbols(s: string): string {
  return s
    .replace(/\s*([.()[\]{};:,])\s*/g, '$1')
    .replace(/\(\s+/g, '(')
    .replace(/\s+\)/g, ')')
}

/** Capitalise sentence starts for prose dictation (unicode-aware for `á`, `ç`, …). */
function capitaliseProse(s: string): string {
  const t = s.trimStart()
  if (!t) return s
  return s.replace(/(^|[.!?]\s+|\n+)([\p{L}\p{N}_])/gu, (_, pre: string, ch: string) => pre + ch.toUpperCase())
}

/**
 * Multi-word speech splits for two-letter shell verbs ("see dee" → "cd").
 * Keys are accent-folded lowercase: `joinShellSplits` folds the input the
 * same way, so `cê dê`, `CÊ DÊ` and `ce de` all hit the `ce de` entry.
 */
const SHELL_SPLITS: Record<string, string> = {
  'see dee': 'cd',
  'see pea': 'cp',
  'ell ess': 'ls',
  'pee ess': 'ps',
  'tee oh pee': 'top',
  'dee eff': 'df',
  'dee you': 'du',
  'pee double you dee': 'pwd',
  'make deer': 'mkdir',
  'are em': 'rm',
  'em vee': 'mv',
}

const SHELL_SPLITS_PT: Record<string, string> = {
  'ce de': 'cd',
  'c de': 'cd',
  'ce pe': 'cp',
  'ele esse': 'ls',
  'eli esse': 'ls',
  'l s': 'ls',
  'pe esse': 'ps',
  'ti o pi': 'top',
  'topi': 'top',
  'de efe': 'df',
  'de u': 'du',
  'pe u de': 'pwd',
  'erre eme': 'rm',
  'erri eme': 'rm',
  'r m': 'rm',
  'eme ve': 'mv',
  'eme vee': 'mv',
}

const SHELL_SPLITS_ALL: Record<string, string> = { ...SHELL_SPLITS, ...SHELL_SPLITS_PT }
const SHELL_SPLIT_MAX_WORDS = Math.max(
  ...Object.keys(SHELL_SPLITS_ALL).map((k) => k.split(' ').length),
)

/**
 * Fold split phrases back to verbs over word windows, comparing
 * accent-folded lowercase but keeping unmatched words byte-identical —
 * so `CÊ DÊ /Tmp` → `cd /Tmp` while `café.txt` never becomes `cafe.txt`.
 */
function joinShellSplits(s: string): string {
  const words = s.trim().split(/\s+/).filter(Boolean)
  if (!words.length) return ''
  const folded = words.map((w) => foldAccents(w))
  const out: string[] = []
  let i = 0
  while (i < words.length) {
    let hit: string | null = null
    let hitLen = 0
    for (let len = Math.min(SHELL_SPLIT_MAX_WORDS, words.length - i); len >= 1; len--) {
      const phrase = folded.slice(i, i + len).join(' ')
      const verb = SHELL_SPLITS_ALL[phrase]
      if (verb) {
        hit = verb
        hitLen = len
        break
      }
    }
    if (hit) {
      out.push(hit)
      i += hitLen
    } else {
      out.push(words[i])
      i += 1
    }
  }
  return out.join(' ')
}

// ─── cybsh dictionary (mirrors crates/os `command_table` plus the static
// layer's vault verbs in `src/utils/staticCybsh.ts`: `oauth`, `compress`,
// `decompress`) ───────────────

export const CYBSH_COMMANDS = [
  'help', 'history', 'clear', 'version', 'echo', 'ls', 'cd', 'pwd', 'cat',
  'cp', 'mv', 'rm', 'mkdir', 'touch', 'stat', 'du', 'df', 'mount', 'umount',
  'disk', 'providers', 'quota', 'oauth', 'sync', 'scrub', 'repair', 'gc',
  'lease', 'ps', 'top', 'kill', 'jobs', 'compute', 'workers', 'keygen',
  'encrypt', 'decrypt', 'compress', 'decompress', 'search',
  'grep', 'find', 'head', 'tail', 'wc', 'write', 'edit', 'ai',
  'run', 'theme', 'ui',
] as const

const CYBSH_SUBCOMMANDS: Record<string, readonly string[]> = {
  disk: ['create', 'attach', 'detach', 'resize', 'check', 'destroy', 'list', 'status', 'df'],
  sync: ['start', 'status', 'list', 'runs', 'cancel', 'restore', 'move'],
  compute: ['run'],
  lease: ['acquire', 'release', 'status'],
  repair: ['run', 'rebuild', 'gc', 'status'],
  scrub: ['run', 'runs'],
  oauth: ['status', 'start'],
  ai: ['ask', 'prompt'],
  ui: ['theme', 'accent', 'get'],
  theme: ['get'],
}

/** Every completable phrase (`disk create`, …) for ghost text when offline. */
export const CYBSH_PHRASES: string[] = [
  ...CYBSH_COMMANDS,
  ...Object.entries(CYBSH_SUBCOMMANDS).flatMap(([cmd, subs]) => subs.map((s) => `${cmd} ${s}`)),
  'history clear',
]

const CYBSH_ALIASES: Record<string, string> = {
  dir: 'ls',
  del: 'rm',
  erase: 'rm',
  copy: 'cp',
  move: 'mv',
  cls: 'clear',
  quit: 'clear',
  exit: 'clear',
  list: 'ls',
}

// Portuguese aliases (keys are accent-folded — `histórico` → `historico`).
const CYBSH_ALIASES_PT: Record<string, string> = {
  ajuda: 'help',
  historico: 'history',
  limpar: 'clear',
  limpa: 'clear',
  versao: 'version',
  lista: 'ls',
  listar: 'ls',
  apagar: 'rm',
  apaga: 'rm',
  excluir: 'rm',
  copiar: 'cp',
  copia: 'cp',
  mover: 'mv',
  sair: 'clear',
}

export interface ShellFix {
  line: string
  fixed: boolean
  fixes: string[]
}

/** Split a shell line on whitespace, honouring single/double quotes. */
function splitShell(line: string): string[] {
  const out: string[] = []
  let cur = ''
  let quote = ''
  for (const ch of line.trim()) {
    if (quote) {
      cur += ch
      if (ch === quote) quote = ''
    } else if (ch === '"' || ch === "'") {
      quote = ch
      cur += ch
    } else if (/\s/.test(ch)) {
      if (cur) {
        out.push(cur)
        cur = ''
      }
    } else {
      cur += ch
    }
  }
  if (cur) out.push(cur)
  return out
}

/**
 * Dictionary fallback for a cybsh line: fixes the verb (aliases + fuzzy
 * neighbours like `lss→ls`, `sl→ls`) and known subcommands
 * (`disk lis→list`). Never invents — unknown words pass through untouched
 * for the server's own did-you-mean.
 */
export function correctShellLine(raw: string): ShellFix {
  const line = joinShellSplits(raw)
  const parts = splitShell(line)
  if (!parts.length) return { line: '', fixed: false, fixes: [] }
  const fixes: string[] = []
  const verbFolded = foldAccents(parts[0])
  const alias = CYBSH_ALIASES[verbFolded] ?? CYBSH_ALIASES_PT[verbFolded]
  const verb = parts[0].toLowerCase()
  if (alias) {
    parts[0] = alias
    fixes.push(`${verb}→${parts[0]}`)
  } else if (!(CYBSH_COMMANDS as readonly string[]).includes(verbFolded)) {
    const hit = bestMatch(verbFolded, CYBSH_COMMANDS)
    if (hit) {
      parts[0] = hit.match
      fixes.push(`${verb}→${hit.match}`)
    }
  }
  const subs = CYBSH_SUBCOMMANDS[parts[0]]
  if (subs && parts[1] && !parts[1].startsWith('-')) {
    const sub = parts[1].toLowerCase()
    if (!subs.includes(sub)) {
      const hit = bestMatch(sub, subs)
      if (hit) {
        parts[1] = hit.match
        fixes.push(`${sub}→${hit.match}`)
      }
    }
  }
  return { line: parts.join(' '), fixed: fixes.length > 0, fixes }
}

// ─── code-word fixes (homophones STT mangles) ────────────────────────────

// Keys are accent-folded: `função` → `funcao` before lookup.
const CODE_HOMOPHONES: Record<string, string> = {
  function: 'function',
  funkshun: 'function',
  const: 'const',
  cost: 'const',
  let: 'let',
  lent: 'let',
  return: 'return',
  import: 'import',
  export: 'export',
  class: 'class',
  async: 'async',
  await: 'await',
}

const CODE_HOMOPHONES_PT: Record<string, string> = {
  funcao: 'function',
  constante: 'const',
  retorna: 'return',
  retorne: 'return',
  importar: 'import',
  importe: 'import',
  exportar: 'export',
  exporte: 'export',
  classe: 'class',
  assincrono: 'async',
  espera: 'await',
  aguarde: 'await',
  se: 'if',
  senao: 'else',
  para: 'for',
  enquanto: 'while',
}

export function correctCodeWords(s: string, lang: VoiceLang = 'en-US'): string {
  const pt = isPtLang(lang)
  return s.replace(/\b[a-zà-ÿ]+\b/gi, (w) => {
    const folded = foldAccents(w)
    const fixPt = pt ? CODE_HOMOPHONES_PT[folded] : undefined
    if (fixPt) return fixPt
    const fix = CODE_HOMOPHONES[folded]
    return fix ?? w
  })
}

/**
 * Full pipeline: spoken words → insertable text for the given mode.
 * English tables always apply; pt-BR tables additionally apply when
 * `lang` is Portuguese, so mixed speech (`abre parênteses` + `camel case`)
 * works in pt-BR mode while en-US stays exactly as before.
 */
export function normalizeSpoken(raw: string, mode: VoiceMode, lang: VoiceLang = 'en-US'): string {
  let s = raw.trim()
  if (!s) return s
  const pt = isPtLang(lang)
  if (mode === 'shell') {
    s = joinShellSplits(s)
    if (pt) for (const [re, rep] of CODE_WORDS_PT) s = s.replace(re, rep)
    for (const [re, rep] of CODE_WORDS) s = s.replace(re, rep)
    return glueSymbols(s)
  }
  if (mode === 'code') {
    if (pt) for (const [re, rep] of CODE_WORDS_PT) s = s.replace(re, rep)
    for (const [re, rep] of CODE_WORDS) s = s.replace(re, rep)
    s = applyCasePhrases(s)
    s = correctCodeWords(s, lang)
    return glueSymbols(s)
  }
  if (pt) for (const [re, rep] of PROSE_PUNCT_PT) s = s.replace(re, rep)
  for (const [re, rep] of PROSE_PUNCT) s = s.replace(re, rep)
  // Collapse horizontal whitespace but keep dictated line breaks: `nova linha`
  // / `new line` / `new paragraph` survive instead of flattening to a space.
  s = s
    .replace(/[^\S\n]+/g, ' ')
    .replace(/ +\n/g, '\n')
    .replace(/\n +/g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .replace(/\s+([,.?!;:])/g, '$1')
  return capitaliseProse(s)
}
