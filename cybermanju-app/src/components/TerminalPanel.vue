<script setup lang="ts">
// CyberManju OS — TerminalPanel (src/components/TerminalPanel.vue).
// Role: Unix-style cybsh terminal emulator (prompt, scrollback, line editing).
// Exports: default Vue SFC (Terminal panel).
// Integrates: @/stores/app, @/composables/useTheme, @/composables/useVoiceInput,
//   @/utils/speechCorrect, @/utils/shellComplete.
// Contracts: transport-agnostic (everything via store.execShellLine /
//   completeShellLine); server prompt stays `cybsh> ` on the wire, the
//   `user@host:cwd$` prompt here is display-only.
// CyberManju OS — cybsh terminal emulator (AGENT-8)
//
// A Unix-terminal-style front end to the server-side shell: bash-like
// `cybsh@os:/cwd$` prompt with exit-status colour, emacs line editing,
// reverse history search, scrollback find, clickable links, font zoom,
// transcript export and smart autoscroll. Transport-agnostic: everything
// goes through the store, so the same panel runs in tauri, rest and wasm.
import { computed, inject, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useTheme } from '@/composables/useTheme'
import { useVoiceInput } from '@/composables/useVoiceInput'
import { invoke } from '@/composables/useTauri'
import { ShortcutsKey } from '@/composables/shortcutsKey'
import { correctShellLine, CYBSH_PHRASES } from '@/utils/speechCorrect'
import {
  commonPrefix,
  containsClearSequence,
  currentWord,
  expandHistory,
  formatPrompt,
  ghostSuffix,
  historyTokenHints,
  isVerbPosition,
  linkifyLine,
  loadHistory,
  localCompletions,
  nextHistoryMatch,
  prevHistoryMatch,
  pushHistory,
  quoteCompletionName,
  replaceWord,
  resolveCompletionDir,
  saveHistory,
  shortenCwd,
  splitCompletionWord,
  stripClearSequences,
  trackCd,
  wantsDirsOnly,
} from '@/utils/shellComplete'

interface Seg {
  text: string
  color?: string
  bg?: string
  bold?: boolean
  dim?: boolean
  italic?: boolean
  underline?: boolean
  strike?: boolean
}

interface Line {
  id: number
  kind: 'in' | 'out' | 'err' | 'sys'
  text: string
  /** SGR segments, computed once at push time — never per frame. */
  segs: Seg[]
  /** Prompt snapshot for echoed input lines (cwd at exec time). */
  pcwd?: string
  /** `$` was red: the previous command had failed. */
  failed?: boolean
}

const store = useAppStore()

/** Scrollback cap — a few thousand lines is the 60 fps budget (item 15). */
const MAX_LINES = 2000
const FONT_KEY = 'cybermanju.term.fontSize'
const DEFAULT_FONT = 13

const lines = ref<Line[]>([])
const input = ref('')
/** Volume cwd shown in the prompt (optimistic; the server owns the truth). */
const cwd = ref('/')
/** Exit status of the last command (server reports ok/err only → 0/1). */
const lastExit = ref(0)
const lastMs = ref(0)
/** Shell history: persisted across reloads, prefix-filtered on ↑/↓. */
const history = ref<string[]>(loadHistory())
const histIndex = ref(-1)
const draft = ref('')
const scrollEl = ref<HTMLElement | null>(null)
const inputEl = ref<HTMLTextAreaElement | null>(null)
/** Caret position for ghost-text gating and word-bound Tab completion. */
const caretPos = ref(0)
const running = computed(() => store.shellBusy)
const fontSize = ref(DEFAULT_FONT)
/** Smart autoscroll: stay pinned only while the user is at the bottom. */
const stick = ref(true)
const unread = ref(0)
/** Scrollback find bar. */
const findOpen = ref(false)
const findQuery = ref('')
const findPos = ref(0)
const findEl = ref<HTMLInputElement | null>(null)
/** Bash-style reverse history search (Ctrl+R). */
const rOpen = ref(false)
const rQuery = ref('')
const rPos = ref(0)
const lastOk = computed(() => lastExit.value === 0)
let lineId = 0

const ANSI_FG = [
  '#d0d0d0', // 30 black — kept light on the black panel
  '#ff5f56', // 31 red
  '#5af78e', // 32 green
  '#f3f99d', // 33 yellow
  '#57c7ff', // 34 blue
  '#ff6ac1', // 35 magenta
  '#9aedfe', // 36 cyan
  '#e6e6e6', // 37 white
]
const ANSI_BRIGHT = ['#808080', '#ff7b72', '#7dff9f', '#faf3a6', '#7fd4ff', '#ff8ed4', '#bff3ff', '#ffffff']
const ANSI_BG = ['#000000', '#7a2a26', '#1f5c38', '#6b6330', '#1f4d6b', '#6b2a52', '#2a5c66', '#555555']
const ANSI_BG_BRIGHT = ['#3a3a3a', '#ff7b72', '#3a7a4f', '#8a8340', '#2a5f8a', '#8a3a68', '#3a7a8a', '#888888']

function ansi256(n: number): string {
  if (n < 8) return ANSI_FG[n]
  if (n < 16) return ANSI_BRIGHT[n - 8]
  if (n < 232) {
    const i = n - 16
    const r = Math.floor(i / 36)
    const g = Math.floor((i % 36) / 6)
    const b = i % 6
    const v = (c: number) => (c === 0 ? 0 : 55 + c * 40)
    return `rgb(${v(r)},${v(g)},${v(b)})`
  }
  const g = 8 + (n - 232) * 10
  return `rgb(${g},${g},${g})`
}

function push(kind: Line['kind'], text: string, extra?: Partial<Line>) {
  if (text === '') return
  lines.value.push({ id: ++lineId, kind, text, segs: segments(text), ...extra })
  if (lines.value.length > MAX_LINES) lines.value.splice(0, lines.value.length - MAX_LINES)
  if (stick.value) void scrollToBottom()
  else unread.value++
}

/** Split one server line on SGR sequences into renderable segments.
 * Full-ish xterm palette: fg/bg, bright, bold/dim/italic/underline/strike,
 * 256-colour and truecolour. Cursor/clear sequences are stripped upstream. */
function segments(text: string): Seg[] {
  const out: Seg[] = []
  const re = /\x1b\[([0-9;]*)m|\x1b\[[0-9;]*[A-Z]|\x1b\([AB0]/g
  let color: string | undefined
  let bg: string | undefined
  let bold = false
  let dim = false
  let italic = false
  let underline = false
  let strike = false
  let cursor = 0
  let match: RegExpExecArray | null
  const emit = (chunk: string) => {
    if (chunk) {
      out.push(
        color || bg || bold || dim || italic || underline || strike
          ? { text: chunk, color, bg, bold, dim, italic, underline, strike }
          : { text: chunk },
      )
    }
  }
  while ((match = re.exec(text)) !== null) {
    emit(text.slice(cursor, match.index))
    cursor = match.index + match[0].length
    const seq = match[0]
    if (!seq.endsWith('m')) continue // cursor motion / charset — drop
    const codes = (match[1] === '' ? '0' : match[1]).split(';').map(Number)
    for (let i = 0; i < codes.length; i++) {
      const code = codes[i]
      if (code === 0) {
        color = undefined
        bg = undefined
        bold = dim = italic = underline = strike = false
      } else if (code === 1) bold = true
      else if (code === 2) dim = true
      else if (code === 3) italic = true
      else if (code === 4) underline = true
      else if (code === 9) strike = true
      else if (code === 22) bold = dim = false
      else if (code === 23) italic = false
      else if (code === 24) underline = false
      else if (code === 29) strike = false
      else if (code === 39) color = undefined
      else if (code === 49) bg = undefined
      else if (code >= 30 && code <= 37) color = ANSI_FG[code - 30]
      else if (code >= 90 && code <= 97) color = ANSI_BRIGHT[code - 90]
      else if (code >= 40 && code <= 47) bg = ANSI_BG[code - 40]
      else if (code >= 100 && code <= 107) bg = ANSI_BG_BRIGHT[code - 100]
      else if ((code === 38 || code === 48) && codes[i + 1] === 5 && codes[i + 2] !== undefined) {
        const c = ansi256(codes[i + 2])
        if (code === 38) color = c
        else bg = c
        i += 2
      } else if ((code === 38 || code === 48) && codes[i + 1] === 2 && codes[i + 5] !== undefined) {
        const c = `rgb(${codes[i + 2]},${codes[i + 3]},${codes[i + 4]})`
        if (code === 38) color = c
        else bg = c
        i += 5
      }
    }
  }
  emit(text.slice(cursor))
  // Erase-in-line (`\x1b[K`) has no scrollback meaning — drop the marker.
  for (const seg of out) seg.text = seg.text.replace(/\x1b\[K/g, '')
  return out.filter((s) => s.text !== '')
}

function segStyle(seg: Seg): Record<string, string> {
  const style: Record<string, string> = {}
  if (seg.color) style.color = seg.color
  if (seg.bg) style.backgroundColor = seg.bg
  if (seg.bold) style.fontWeight = '700'
  if (seg.dim) style.opacity = '0.65'
  if (seg.italic) style.fontStyle = 'italic'
  const deco: string[] = []
  if (seg.underline) deco.push('underline')
  if (seg.strike) deco.push('line-through')
  if (deco.length) style.textDecoration = deco.join(' ')
  return style
}

async function scrollToBottom() {
  await nextTick()
  const el = scrollEl.value
  if (el) el.scrollTop = el.scrollHeight
}

function onScroll() {
  const el = scrollEl.value
  if (!el) return
  stick.value = el.scrollHeight - el.scrollTop - el.clientHeight < 24
  if (stick.value) unread.value = 0
}

function jumpToBottom() {
  stick.value = true
  unread.value = 0
  void scrollToBottom()
  focusInput()
}

/** Copy one output line — explicit button so drag-selection still works. */
async function copyLine(line: Line) {
  try {
    await navigator.clipboard.writeText(line.text)
  } catch {
    // Clipboard unavailable (insecure origin) — the text is still selectable.
  }
}

function copyAll() {
  const text = lines.value.map((l) => (l.kind === 'in' ? `${formatPrompt(l.pcwd ?? '/')} $ ${l.text}` : l.text)).join('\n')
  navigator.clipboard.writeText(text).catch(() => {})
}

function exportLog() {
  const stamp = new Date().toISOString().replace(/[:.]/g, '').slice(0, 15)
  const text = lines.value.map((l) => (l.kind === 'in' ? `${formatPrompt(l.pcwd ?? '/')}$ ${l.text}` : l.text)).join('\n')
  const blob = new Blob([text + '\n'], { type: 'text/plain' })
  const a = document.createElement('a')
  a.href = URL.createObjectURL(blob)
  a.download = `cybsh-${stamp}.log`
  a.click()
  setTimeout(() => URL.revokeObjectURL(a.href), 5000)
}

/** Clicking a `/volume/path` inserts it at the caret (quoting on spaces). */
function insertPath(path: string) {
  const el = inputEl.value
  const caret = el?.selectionStart ?? input.value.length
  const token = path.includes(' ') ? `'${path}'` : path
  input.value = `${input.value.slice(0, caret)}${token} ${input.value.slice(caret)}`
  caretPos.value = caret + token.length + 1
  autosize()
  nextTick(() => {
    inputEl.value?.setSelectionRange(caretPos.value, caretPos.value)
    focusInput()
  })
}

/** Ghost auto-suggest: history first, command table second. Shown only
 *  when the buffer is single-line and the caret is at the end. → or Tab accepts. */
const ghost = computed(() => {
  if (input.value.includes('\n')) return ''
  if (caretPos.value !== input.value.length) return ''
  if (rOpen.value) return ''
  return ghostSuffix(input.value, history.value, CYBSH_PHRASES)
})

function acceptGhost(): boolean {
  if (!ghost.value) return false
  input.value += ghost.value
  caretPos.value = input.value.length
  autosize()
  nextTick(() => {
    inputEl.value?.setSelectionRange(caretPos.value, caretPos.value)
  })
  return true
}

/**
 * Tab: readline-style completion from four sources, merged and deduped —
 * the live server table, the built-in verb table (so Tab works offline),
 * live volume paths (`ls /do` → `ls /docs/`, `cat <Tab>` lists the cwd),
 * and history words. One candidate completes; several extend the common
 * prefix silently and list on the second Tab.
 */
async function complete() {
  const caret = caretPos.value
  const prefix = input.value.slice(0, caret)
  const after = input.value.slice(caret)
  if (!prefix.trim()) {
    push('sys', 'Tab completes commands · subcommands (disk …) · paths · history words — type to narrow')
    return
  }
  // Second Tab on the same prefix lists (bash-style).
  if (lastTab.value && lastTab.value.prefix === prefix && lastTab.value.hits.length > 1) {
    push('in', input.value, { pcwd: cwd.value, failed: !lastOk.value })
    push('out', lastTab.value.hits.join('   '))
    push('sys', `${lastTab.value.hits.length} candidates — keep typing`)
    lastTab.value = null
    return
  }
  // The server round-trip never throws (miss → []), but belt-and-braces:
  // the local table below guarantees verb candidates regardless.
  let serverHits: string[] = []
  try {
    serverHits = await store.completeShellLine(prefix)
  } catch {
    serverHits = []
  }
  const word = currentWord(prefix, prefix.length)
  const argPosition = !isVerbPosition(prefix, prefix.length)
  // `cd` completes folders only; every other verb takes files and folders.
  const dirsOnly = wantsDirsOnly(prefix)
  let pathHits: string[] = []
  if (argPosition && word && !word.startsWith('-') && !/ -os(\s|$)/.test(` ${prefix} `)) {
    pathHits = await completePathHits(prefix, word, dirsOnly)
  } else if (argPosition && !word) {
    // Bare `ls <Tab>` / `cd <Tab>` — offer the cwd listing
    // (folders only after `cd`).
    pathHits = await completePathHits(prefix, '', dirsOnly)
  }
  let historyHits: string[] = []
  if (word && argPosition && !pathHits.length) {
    historyHits = historyTokenHints(history.value, word)
  }
  const hits = [...new Set([...serverHits, ...localCompletions(prefix), ...pathHits, ...historyHits])]
  if (!hits.length) {
    if (!acceptGhost()) push('sys', 'no candidates — ↑ walks matching history, ctrl+r searches it')
    lastTab.value = null
    return
  }
  if (hits.length === 1) {
    completeTo(hits[0], prefix, caret, after, word)
    lastTab.value = null
    return
  }
  const common = commonPrefix(hits)
  if (common.length > prefix.length) {
    input.value = common + after
    caretPos.value = common.length
    autosize()
    nextTick(() => {
      inputEl.value?.setSelectionRange(caretPos.value, caretPos.value)
    })
    lastTab.value = { prefix: common, hits }
    return
  }
  push('in', input.value, { pcwd: cwd.value, failed: !lastOk.value })
  push('out', hits.join('   '))
  push('sys', `${hits.length} candidates — keep typing`)
  lastTab.value = null
}

/** Apply one full-line completion, preserving mid-line text after the caret. */
function completeTo(hit: string, prefix: string, caret: number, after: string, word: string) {
  const atEnd = caret === input.value.length
  if (hit === prefix) {
    // Exact verb already typed — advance into its arguments.
    input.value = `${hit} `
    caretPos.value = input.value.length
  } else if (!hit.startsWith(prefix) && word) {
    // Bare-word candidate (history token): replace the word under the caret.
    const next = replaceWord(input.value, caret, hit)
    input.value = next.line
    caretPos.value = next.caret
  } else {
    // Replace everything up to the caret; keep text after the caret only
    // when it starts at a word boundary (otherwise it is the fragment
    // being completed and would double up, e.g. `cre|ate`).
    const keep = after && /^\s/.test(after) ? after : ''
    const completed = atEnd && !hit.endsWith('/') && !hit.endsWith(' ') ? `${hit} ` : hit
    input.value = completed + (atEnd ? '' : keep)
    caretPos.value = atEnd ? input.value.length : hit.length
  }
  autosize()
  nextTick(() => {
    inputEl.value?.setSelectionRange(caretPos.value, caretPos.value)
  })
}

async function execAndRender(cmd: string) {
  const atCwd = cwd.value
  const wasOk = lastOk.value
  push('in', cmd, { pcwd: atCwd, failed: !wasOk })
  const started = performance.now()
  const result = await store.execShellLine(cmd)
  lastMs.value = Math.round(performance.now() - started)
  lastExit.value = result.ok ? 0 : 1
  // `clear` answers with a clear-screen sequence — honour it like xterm
  // instead of printing it as a blank line.
  if (containsClearSequence(result.output)) {
    lines.value = []
    unread.value = 0
    const rest = stripClearSequences(result.output).trim()
    if (rest) for (const chunk of rest.split('\n')) push(result.ok ? 'out' : 'err', chunk)
  } else {
    for (const chunk of stripClearSequences(result.output).split('\n')) {
      push(result.ok ? 'out' : 'err', chunk)
    }
  }
  if (!result.ok && result.output === '') push('err', 'command failed (exit 1)')
  if (lastMs.value > 2000) push('sys', `took ${(lastMs.value / 1000).toFixed(1)}s`)
  applyCybshUiEffects(result.output)
  // The server owns the cwd: track `cd` optimistically for an instant
  // prompt, and learn the truth whenever the user runs `pwd`.
  if (result.ok && !/ -os(\s|$)/.test(` ${cmd} `)) {
    const next = trackCd(cmd, atCwd)
    if (next !== null) cwd.value = next
    const pwdMatch = /^\s*pwd\s*$/.exec(cmd)
    if (pwdMatch) {
      const first = stripClearSequences(result.output).split('\n')[0]?.trim()
      if (first && first.startsWith('/')) cwd.value = first
    }
  }
  return result
}

/**
 * `.cybsh` OS-interface effects: `theme`/`ui` verbs (and scripts calling
 * them) print machine `ui: theme=<id>` / `ui: accent=<hex|system>` /
 * `ui: density|glass|motion|glow|accent-for=…` lines. Applying them here
 * makes the whole interface change on every transport — the verbs already
 * persist the same intent to the volume mirror + localStorage.
 */
function applyCybshUiEffects(output: string) {
  let touched = false
  try {
    const { setTheme, setAccent, setAccentFor, setDensity, setGlass, setMotion, setGlow, themes } = useTheme()
    for (const raw of output.split('\n')) {
      const line = raw.trim()
      if (!line.startsWith('ui:')) continue
      const rest = line.slice(3).trim()
      const eq = rest.indexOf('=')
      if (eq < 0) continue
      const key = rest.slice(0, eq).trim()
      const value = rest.slice(eq + 1).trim()
      if (key === 'theme' && value in themes) {
        setTheme(value as keyof typeof themes)
        touched = true
      } else if (key === 'accent') {
        if (value === 'system' || value === 'default' || value === 'none') setAccent(null)
        else if (/^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(value)) setAccent(value)
        else continue
        touched = true
      } else if (key === 'accent-for') {
        // Per-theme accent: `<theme-id>:<#hex|system>`.
        const sep = value.indexOf(':')
        if (sep < 0) continue
        const themeId = value.slice(0, sep).trim()
        const hex = value.slice(sep + 1).trim()
        if (!(themeId in themes)) continue
        if (hex === 'system' || hex === 'default' || hex === 'none') setAccentFor(themeId, null)
        else if (/^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(hex)) setAccentFor(themeId, hex)
        else continue
        touched = true
      } else if (key === 'density' && (value === 'compact' || value === 'comfortable')) {
        setDensity(value)
        touched = true
      } else if (key === 'glass' && (value === '0' || value === '1' || value === '2' || value === '3')) {
        setGlass(Number(value) as 0 | 1 | 2 | 3)
        touched = true
      } else if (key === 'motion' && (value === 'auto' || value === 'full' || value === 'reduced')) {
        setMotion(value)
        touched = true
      } else if (key === 'glow') {
        if (value === 'on' || value === 'true' || value === '1') setGlow(true)
        else if (value === 'off' || value === 'false' || value === '0') setGlow(false)
        else continue
        touched = true
      }
    }
  } catch {
    // Theme store unavailable — the persisted intent still applies on reload.
  }
  if (touched) push('sys', 'ui updated from shell effect')
}

function clearScreen() {
  lines.value = []
  unread.value = 0
  stick.value = true
  void scrollToBottom()
  focusInput()
}

async function submit() {
  const raw = input.value
  input.value = ''
  lastTab.value = null
  autosize()
  histIndex.value = -1
  draft.value = ''
  if (raw.trim() === '') {
    push('in', '', { pcwd: cwd.value, failed: !lastOk.value })
    return
  }
  // Local emulator builtins — never sent, never enter server history.
  const head = raw.trim()
  if (head === 'reset') {
    clearScreen()
    push('sys', 'terminal reset — scrollback dropped, cwd kept')
    return
  }
  if (head === 'exit' || head === 'logout') {
    push('in', raw, { pcwd: cwd.value, failed: !lastOk.value })
    push('sys', 'cybsh sessions persist — Clear drops the scrollback, the shell keeps running')
    return
  }
  // Multi-line buffer (shift+enter / trailing `\` / paste): run line by
  // line like a pasted script, keep the last fragment for editing.
  const queue = raw.split('\n').map((l) => (l.endsWith('\r') ? l.slice(0, -1) : l))
  for (const cmd of queue) {
    if (cmd.trim() === '') continue
    // Bash-style `!!` / `!n` / `!prefix` / `!$` expansion.
    const expanded = expandHistory(cmd, history.value)
    if (expanded.error) {
      push('in', cmd, { pcwd: cwd.value, failed: true })
      push('err', expanded.error)
      input.value = cmd
      autosize()
      return
    }
    const line = expanded.line
    if (expanded.expanded) push('sys', `! → ${line}`)
    history.value = pushHistory(history.value, line)
    saveHistory(history.value)
    // Client-side dictionary fallback: repair the verb/subcommand before the
    // server ever sees it (`lss→ls`, `disk lis→list`, spoken `see dee→cd`).
    const fix = correctShellLine(line)
    if (fix.fixed) push('sys', `auto-fix: ${fix.fixes.join(', ')}`)
    const effective = fix.line || line
    const result = await execAndRender(effective)
    // Server still confused but names a neighbour? Apply its did-you-mean once
    // instead of making the user retype.
    if (!result.ok && !fix.fixed) {
      const m = /did you mean '([^']+)'/.exec(result.output)
      if (m) {
        const parts = effective.split(' ')
        parts[0] = m[1]
        push('sys', `retrying as '${parts.join(' ')}'`)
        await execAndRender(parts.join(' '))
      }
    }
  }
  input.value = ''
  autosize()
  void scrollToBottom()
}

/** Multi-line paste: submit each complete line, keep the last fragment. */
async function onPaste(event: ClipboardEvent) {
  const text = event.clipboardData?.getData('text') ?? ''
  if (!text.includes('\n')) return
  event.preventDefault()
  const parts = text.split(/\r?\n/)
  const hasTrailing = parts[parts.length - 1] === ''
  const queue = parts.slice(0, -1)
  const tail = hasTrailing ? '' : parts[parts.length - 1]
  for (const cmd of queue) {
    if (cmd.trim() === '') continue
    input.value = cmd
    await submit()
  }
  input.value = tail
  autosize()
}

function trackCaret(el: HTMLTextAreaElement | null) {
  caretPos.value = el?.selectionStart ?? input.value.length
}

function setCaret(pos: number) {
  caretPos.value = Math.max(0, Math.min(pos, input.value.length))
  nextTick(() => {
    inputEl.value?.setSelectionRange(caretPos.value, caretPos.value)
  })
}

function autosize() {
  const el = inputEl.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = `${Math.min(el.scrollHeight, 160)}px`
  trackCaret(el)
}

/** Word jump target like readline's forward/backward-word. */
function wordJump(value: string, caret: number, dir: 1 | -1): number {
  if (dir > 0) {
    const m = /\s*\S+/.exec(value.slice(caret))
    return m ? caret + m[0].length : value.length
  }
  const upto = value.slice(0, caret).replace(/\s+$/, '')
  const m = /\S+\s*$/.exec(upto)
  return m?.index ?? 0
}

function killWordBack() {
  const el = inputEl.value
  const caret = el?.selectionStart ?? input.value.length
  const start = wordJump(input.value, caret, -1)
  input.value = input.value.slice(0, start) + input.value.slice(caret)
  setCaret(start)
  autosize()
}

function killWordForward() {
  const el = inputEl.value
  const caret = el?.selectionStart ?? input.value.length
  const end = wordJump(input.value, caret, 1)
  input.value = input.value.slice(0, caret) + input.value.slice(end)
  setCaret(caret)
  autosize()
}

/** One `readdir` row, as every transport returns it (camelCase). */
interface DirEntry {
  name?: string
  isDir?: boolean
  kind?: string
}

/** Double-Tab state: second Tab on the same prefix lists the candidates. */
const lastTab = ref<{ prefix: string; hits: string[] } | null>(null)

/**
 * File-path completion off the live volume (`os_ls` on every transport).
 * Returns full-line candidates (`ls /do` → `ls /docs/`) so the single
 * verb-completion code path below just works. Never throws.
 */
async function completePathHits(prefix: string, word: string, dirsOnly = false): Promise<string[]> {
  const base = prefix.slice(0, prefix.length - word.length)
  // `cd ..<Tab>` — the parent dir itself is the only candidate.
  if (word === '..' || word.endsWith('/..')) return [`${base}${word}/`]
  const { rawDir, frag } = splitCompletionWord(word)
  const dir = resolveCompletionDir(rawDir, cwd.value)
  let entries: DirEntry[]
  try {
    const raw = await invoke<DirEntry[]>('os_ls', { path: dir })
    if (!Array.isArray(raw)) return []
    entries = raw
  } catch {
    return []
  }
  const head = word.slice(0, word.length - frag.length)
  const out: string[] = []
  for (const entry of entries) {
    const name = typeof entry?.name === 'string' ? entry.name : ''
    if (!name || !name.startsWith(frag) || name === '.' || name === '..') continue
    const isDir = entry?.isDir === true || entry?.kind === 'dir'
    // `cd` only accepts folders — files are never candidates there.
    if (dirsOnly && !isDir) continue
    out.push(`${base}${head}${quoteCompletionName(name)}${isDir ? '/' : ''}`)
    // A huge directory must not flood the scrollback — bash pages, we cap.
    if (out.length >= 100) break
  }
  return out.sort()
}

/** Reverse search results for the current query (newest first). */
const rMatches = computed(() => {
  if (!rQuery.value) return [...history.value].reverse()
  return [...history.value].reverse().filter((h) => h.includes(rQuery.value))
})
const rCurrent = computed(() => rMatches.value[Math.min(rPos.value, Math.max(0, rMatches.value.length - 1))] ?? '')

function openReverseSearch() {
  rOpen.value = true
  rQuery.value = ''
  rPos.value = 0
}

function acceptReverseSearch() {
  if (rCurrent.value) {
    input.value = rCurrent.value
    caretPos.value = input.value.length
    autosize()
    nextTick(() => {
      inputEl.value?.setSelectionRange(caretPos.value, caretPos.value)
    })
  }
  rOpen.value = false
  focusInput()
}

/** Scrollback find matches (line indices, oldest first). */
const findMatches = computed(() => {
  if (!findQuery.value) return []
  const q = findQuery.value.toLowerCase()
  const out: number[] = []
  lines.value.forEach((l, i) => {
    if (l.text.toLowerCase().includes(q)) out.push(i)
  })
  return out
})

function jumpFind(dir: 1 | -1) {
  if (!findMatches.value.length) return
  findPos.value = (findPos.value + dir + findMatches.value.length) % findMatches.value.length
  nextTick(() => {
    const lineIdx = findMatches.value[findPos.value]
    const line = lines.value[lineIdx]
    scrollEl.value?.querySelector(`[data-lid="${line?.id}"]`)?.scrollIntoView({ block: 'center' })
  })
}

watch(findQuery, () => {
  findPos.value = 0
  if (findMatches.value.length) {
    nextTick(() => {
      const line = lines.value[findMatches.value[0]]
      scrollEl.value?.querySelector(`[data-lid="${line?.id}"]`)?.scrollIntoView({ block: 'center' })
    })
  }
})

function onKeydown(event: KeyboardEvent) {
  const el = event.target as HTMLTextAreaElement
  const caret = el.selectionStart ?? input.value.length
  const ctrl = event.ctrlKey || event.metaKey
  // Own the keystroke: the terminal grabs its keys like xterm does, so
  // window-level shortcuts (command palette on ctrl+k, search on ctrl+f,
  // view modes on ctrl+g/l/m, …) never double-fire while typing here.
  const own = (e: KeyboardEvent) => {
    e.preventDefault()
    e.stopPropagation()
  }
  // Any real keypress invalidates the double-Tab listing state.
  if (!['Tab', 'Shift', 'Control', 'Alt', 'Meta', 'CapsLock'].includes(event.key)) {
    lastTab.value = null
  }

  // Reverse-search overlay owns its keys while open.
  if (rOpen.value) {
    if (event.key === 'Escape' || (event.key === 'c' && ctrl) || (event.key === 'g' && ctrl)) {
      own(event)
      rOpen.value = false
      focusInput()
      return
    }
    if (event.key === 'Enter') {
      own(event)
      acceptReverseSearch()
      return
    }
    if (event.key === 'r' && ctrl) {
      own(event)
      if (rMatches.value.length) rPos.value = (rPos.value + 1) % rMatches.value.length
      return
    }
    return // typing goes to the overlay input
  }

  if (event.key === 'Tab') {
    own(event)
    trackCaret(el)
    void complete()
    return
  }
  // Enter submits; shift+enter inserts a newline; a trailing `\` continues.
  if (event.key === 'Enter' && !event.shiftKey) {
    const trimmedEnd = input.value.slice(0, caret).endsWith('\\') && caret === input.value.length
    if (!trimmedEnd) {
      own(event)
      void submit()
      return
    }
    // `echo foo \` + Enter → keep editing on the next visual line.
    own(event)
    input.value = `${input.value.slice(0, caret - 1)}\n${input.value.slice(caret)}`
    setCaret(caret)
    autosize()
    return
  }
  if (event.key === 'ArrowRight' && ghost.value) {
    if ((el.selectionStart ?? 0) >= input.value.length) {
      own(event)
      acceptGhost()
      return
    }
  }
  if (event.key === 'Escape') {
    if (findOpen.value) {
      event.preventDefault()
      findOpen.value = false
      focusInput()
      return
    }
    event.preventDefault()
    input.value = ''
    histIndex.value = -1
    draft.value = ''
    caretPos.value = 0
    autosize()
    return
  }
  // ── emacs line editing ──────────────────────────────────────────
  if (ctrl && (event.key === 'a' || event.key === 'A')) {
    own(event)
    setCaret(0)
    return
  }
  if (ctrl && (event.key === 'e' || event.key === 'E')) {
    own(event)
    setCaret(input.value.length)
    return
  }
  if (ctrl && (event.key === 'u' || event.key === 'U')) {
    own(event)
    input.value = input.value.slice(caret)
    setCaret(0)
    autosize()
    return
  }
  if (ctrl && (event.key === 'k' || event.key === 'K')) {
    own(event)
    input.value = input.value.slice(0, caret)
    setCaret(caret)
    autosize()
    return
  }
  // Kill-word-back: alt+backspace (readline's other binding). Plain ctrl+w
  // is deliberately NOT bound — the browser closes the tab on it and Tauri
  // closes the window, so it can never reach the terminal.
  if ((event.altKey && event.key === 'Backspace') || (ctrl && (event.key === 'w' || event.key === 'W'))) {
    // Ctrl+W only fires where the browser/window manager lets it through
    // (native shells); everywhere else the key never arrives.
    own(event)
    killWordBack()
    return
  }
  if (ctrl && (event.key === 'l' || event.key === 'L')) {
    own(event)
    clearScreen()
    return
  }
  if (ctrl && !event.shiftKey && (event.key === 'r' || event.key === 'R')) {
    own(event)
    openReverseSearch()
    nextTick(() => document.getElementById('cybsh-rsearch')?.focus())
    return
  }
  if (ctrl && (event.key === 'd' || event.key === 'D')) {
    if (!input.value) {
      own(event)
      push('sys', 'no logout in cybsh — the session stays open; Clear drops the scrollback')
      return
    }
    // Delete the char under the caret (readline `delete-char`).
    if (caret < input.value.length) {
      own(event)
      input.value = input.value.slice(0, caret) + input.value.slice(caret + 1)
      setCaret(caret)
      autosize()
    }
    return
  }
  // Ctrl+C — abandon the line being typed (the job itself is `kill <id>`).
  // With shift (ctrl+shift+c, the xterm copy chord) the key passes through.
  if (event.key === 'c' && ctrl && !event.shiftKey) {
    own(event)
    push('in', input.value, { pcwd: cwd.value, failed: !lastOk.value })
    push('out', '^C')
    input.value = ''
    histIndex.value = -1
    autosize()
    return
  }
  if (event.altKey && (event.key === 'f' || event.key === 'F')) {
    own(event)
    setCaret(wordJump(input.value, caret, 1))
    return
  }
  if (event.altKey && (event.key === 'b' || event.key === 'B')) {
    own(event)
    setCaret(wordJump(input.value, caret, -1))
    return
  }
  if (event.altKey && (event.key === 'd' || event.key === 'D')) {
    own(event)
    killWordForward()
    return
  }
  if (event.altKey && event.key === '.') {
    own(event)
    const last = history.value[history.value.length - 1]
    if (last) {
      const toks = last.trim().split(/\s+/)
      const token = toks[toks.length - 1]
      input.value = input.value.slice(0, caret) + token + input.value.slice(caret)
      setCaret(caret + token.length)
      autosize()
    }
    return
  }
  // Ctrl+F opens scrollback find (plain F types; ctrl+shift+f is the
  // faces panel globally and passes through).
  if (ctrl && !event.shiftKey && (event.key === 'f' || event.key === 'F')) {
    own(event)
    findOpen.value = true
    nextTick(() => findEl.value?.focus())
    return
  }
  // History: single-line buffers walk prefix matches; multiline keeps
  // native caret movement like any terminal input area.
  const singleLine = !input.value.includes('\n')
  if (event.key === 'ArrowUp' && singleLine) {
    event.preventDefault()
    if (history.value.length === 0) return
    if (histIndex.value === -1) {
      draft.value = input.value
      histIndex.value = prevHistoryMatch(history.value, draft.value, history.value.length - 1)
    } else {
      const i = prevHistoryMatch(history.value, draft.value, histIndex.value - 1)
      if (i !== -1) histIndex.value = i
    }
    if (histIndex.value !== -1) {
      input.value = history.value[histIndex.value] ?? ''
      caretPos.value = input.value.length
      autosize()
      nextTick(() => {
        inputEl.value?.setSelectionRange(caretPos.value, caretPos.value)
      })
    }
    return
  }
  if (event.key === 'ArrowDown' && singleLine) {
    event.preventDefault()
    if (histIndex.value === -1) return
    const i = nextHistoryMatch(history.value, draft.value, histIndex.value + 1)
    if (i !== -1) {
      histIndex.value = i
      input.value = history.value[histIndex.value] ?? ''
    } else {
      histIndex.value = -1
      input.value = draft.value
    }
    caretPos.value = input.value.length
    autosize()
    nextTick(() => {
      inputEl.value?.setSelectionRange(caretPos.value, caretPos.value)
    })
    return
  }
}

function focusInput() {
  // Never steal a drag-selection: only focus when the user is not
  // selecting scrollback text.
  try {
    if (window.getSelection()?.toString()) return
  } catch {
    /* selection API unavailable — focus anyway */
  }
  inputEl.value?.focus()
}

// ── keyboard ownership ──────────────────────────────────────────────
// The app's global shortcuts live on document-capture/window listeners and
// would otherwise hijack terminal keys (ctrl+r refresh, ctrl+f search,
// ctrl+u upload, ctrl+d dark-mode, ctrl+k palette, alt+f maximize, …).
// Like a real terminal emulator, cybsh grabs the keyboard while focused:
// the global registry pauses on focus-in and resumes on focus-out.
const shortcutsApi = inject(ShortcutsKey, null)

function pauseGlobals() {
  try {
    shortcutsApi?.pause()
  } catch {
    /* no registry above us (tests) — nothing to pause */
  }
}

function resumeGlobals() {
  try {
    shortcutsApi?.resume()
  } catch {
    /* nothing to resume */
  }
}

function onFocusIn(e: FocusEvent) {
  const root = e.currentTarget as HTMLElement | null
  if (root && e.target instanceof Node && root.contains(e.target)) pauseGlobals()
}

function onFocusOut(e: FocusEvent) {
  const root = e.currentTarget as HTMLElement | null
  const next = e.relatedTarget as Node | null
  if (root && (!next || !root.contains(next))) resumeGlobals()
}

// External inserts (voice dictation) land at the end — keep the caret
// model in sync when the user is typing in the field.
watch(input, () => {
  if (document.activeElement === inputEl.value && inputEl.value) {
    caretPos.value = inputEl.value.selectionStart ?? input.value.length
  } else {
    caretPos.value = input.value.length
  }
})

// Provider sync auto-update: a finished run re-fetches the file manager +
// volume `df` in the store, and the terminal prints the summary as a `sys`
// notice so the next `ls`/`df` visibly includes the synced data.
watch(() => store.lastSyncAt, (at) => {
  if (!at) return
  push('sys', store.lastSyncSummary || 'sync finished — file manager + terminal refreshed')
})

/* ── voice input (shell mode: verbs + symbols, corrected on insert) ── */
const voice = useVoiceInput('shell')
let stopVoice: (() => void) | null = null
function toggleVoice() {
  if (voice.listening.value) {
    stopVoice?.()
    stopVoice = null
    focusInput()
    return
  }
  stopVoice = voice.dictateInto(input)
}

function applyFontSize(delta: number) {
  fontSize.value = Math.max(10, Math.min(22, fontSize.value + delta))
  try {
    localStorage.setItem(FONT_KEY, String(fontSize.value))
  } catch {
    /* private mode — session-only zoom */
  }
}

onBeforeUnmount(() => {
  stopVoice?.()
  resumeGlobals()
})

onMounted(async () => {
  try {
    const saved = localStorage.getItem(FONT_KEY)
    if (saved && Number(saved) >= 10 && Number(saved) <= 22) fontSize.value = Number(saved)
  } catch {
    /* keep default */
  }
  push('sys', `cybsh — \`help\` lists commands · TAB completes verbs+paths (→ accepts the grey hint, TAB twice lists) · ↑/↓ history · ctrl+r searches · ctrl+l clears · shift+enter for a second line`)
  if (store.osWorkers === null) await store.fetchOsWorkers()
  // Learn the volume cwd for the prompt without echoing (one `pwd` row in
  // server history — the prompt is display-only, the server owns the truth).
  try {
    const here = await store.execShellLine('pwd')
    const first = stripClearSequences(here.output).split('\n')[0]?.trim()
    if (here.ok && first && first.startsWith('/')) cwd.value = first
  } catch {
    /* offline — prompt keeps `/` until the first cd */
  }
  autosize()
  void scrollToBottom()
})
</script>

<template>
  <div
    class="terminal-panel"
    :style="{ fontSize: `${fontSize}px` }"
    @click="focusInput"
    @focusin="onFocusIn"
    @focusout="onFocusOut"
  >
    <div class="panel-header">
      <div class="header-left">
        <span class="term-lights" aria-hidden="true"><i class="lt r" /><i class="lt y" /><i class="lt g" /></span>
        <span class="icon-terminal"><AppIcon name="solar:file-terminal-bold" /></span>
        <h2 class="panel-title" :title="`volume cwd ${cwd}`">cybsh@os:{{ shortenCwd(cwd) }}</h2>
        <span class="job-badge" :class="{ on: running }">{{ running ? 'Busy' : 'Idle' }}</span>
        <span v-if="lastExit !== 0" class="exit-badge" :title="`last command failed (exit ${lastExit})`">exit {{ lastExit }}</span>
        <span v-else-if="lastMs > 500" class="meta-badge" :title="`last command took ${lastMs}ms`">{{ lastMs }}ms</span>
      </div>
      <div class="header-right">
        <button class="ghost-btn" type="button" title="Find in scrollback (ctrl+f)" aria-label="Find in scrollback" @click.stop="findOpen = !findOpen; nextTick(() => findEl?.focus())">Find</button>
        <button class="ghost-btn" type="button" title="Smaller font" aria-label="Smaller font" @click.stop="applyFontSize(-1)">A−</button>
        <button class="ghost-btn" type="button" title="Larger font" aria-label="Larger font" @click.stop="applyFontSize(1)">A+</button>
        <button class="ghost-btn" type="button" title="Copy transcript" aria-label="Copy transcript" @click.stop="copyAll">Copy</button>
        <button class="ghost-btn" type="button" title="Download .log transcript" aria-label="Download transcript" @click.stop="exportLog">Export</button>
        <button class="ghost-btn" type="button" title="Clear screen (ctrl+l)" aria-label="Clear screen" @click.stop="clearScreen">Clear</button>
      </div>
    </div>

    <div v-if="findOpen" class="term-findbar">
      <input
        ref="findEl"
        v-model="findQuery"
        class="term-find-input"
        type="text"
        placeholder="find in scrollback…"
        spellcheck="false"
        autocomplete="off"
        aria-label="Find in scrollback"
        @keydown.enter.prevent="jumpFind(1)"
        @keydown.tab.prevent="jumpFind(1)"
        @keydown.escape.prevent="findOpen = false; focusInput()"
      />
      <span class="term-find-count">{{ findMatches.length ? `${findPos + 1}/${findMatches.length}` : '0/0' }}</span>
      <button class="ghost-btn" type="button" aria-label="Previous match" @click="jumpFind(-1)">↑</button>
      <button class="ghost-btn" type="button" aria-label="Next match" @click="jumpFind(1)">↓</button>
      <button class="ghost-btn" type="button" aria-label="Close find" @click="findOpen = false; focusInput()">✕</button>
    </div>

    <div v-if="rOpen" class="term-rsearch">
      <span class="term-rsearch-label">(reverse-i-search):</span>
      <input
        id="cybsh-rsearch"
        v-model="rQuery"
        class="term-rsearch-input"
        type="text"
        spellcheck="false"
        autocomplete="off"
        aria-label="Reverse history search"
        @keydown.enter.prevent="acceptReverseSearch"
        @keydown.tab.prevent="rPos = rMatches.length ? (rPos + 1) % rMatches.length : 0"
        @keydown.escape.prevent="rOpen = false; focusInput()"
      />
      <span class="term-rsearch-hit">{{ rCurrent || '(no match)' }}</span>
      <span class="term-rsearch-hint">enter accepts · ctrl+r cycles · esc cancels</span>
    </div>

    <div ref="scrollEl" class="term-scroll" role="log" aria-label="cybsh output" @scroll="onScroll">
      <div
        v-for="(line, li) in lines"
        :key="line.id"
        class="term-line"
        :class="[
          `kind-${line.kind}`,
          { 'term-current-find': findQuery && findMatches[findPos] === li },
          { 'term-has-find': findQuery && findMatches.includes(li) },
        ]"
        :data-lid="line.id"
        tabindex="0"
      >
        <template v-if="line.kind === 'in'">
          <span class="term-user">cybsh@os:</span><span class="term-cwd">{{ shortenCwd(line.pcwd ?? '/') }}</span><span class="term-dollar" :class="{ fail: line.failed }">$</span>
          <span class="term-cmd">{{ line.text }}</span>
        </template>
        <template v-else>
          <span v-for="(seg, i) in line.segs" :key="i" :style="segStyle(seg)"><template v-for="(part, j) in linkifyLine(seg.text)" :key="`${i}-${j}`"><a
            v-if="part.url"
            :href="part.text"
            target="_blank"
            rel="noopener noreferrer"
            class="term-link"
            @click.stop
          >{{ part.text }}</a><button
            v-else-if="part.path"
            type="button"
            class="term-path"
            :title="`Insert ${part.text} into the command line`"
            @click.stop="insertPath(part.text)"
          >{{ part.text }}</button><span v-else>{{ part.text }}</span></template></span>
        </template>
        <button
          class="term-copy"
          type="button"
          title="Copy line"
          aria-label="Copy line"
          @click.stop="copyLine(line)"
        >⧉</button>
      </div>
      <div v-if="!lines.length" class="term-empty">— scrollback empty —</div>
    </div>

    <button v-if="!stick && unread > 0" class="term-jump" type="button" @click="jumpToBottom">↓ {{ unread }} new — jump to bottom</button>

    <div class="term-input-row">
      <span class="term-prompt" aria-hidden="true"><span class="term-user">cybsh@os:</span><span class="term-cwd">{{ shortenCwd(cwd) }}</span><span class="term-dollar" :class="{ fail: !lastOk }">$</span></span>
      <div class="term-input-wrap">
        <div class="term-ghost" aria-hidden="true"><span class="ghost-hide">{{ input }}</span><span class="ghost-show">{{ ghost }}</span></div>
        <textarea
          ref="inputEl"
          v-model="input"
          class="term-input"
          rows="1"
          spellcheck="false"
          autocomplete="off"
          autocapitalize="off"
          aria-label="cybsh command"
          :placeholder="running ? 'working…' : ''"
          @keydown="onKeydown"
          @keyup="trackCaret($event.target as HTMLTextAreaElement)"
          @click="trackCaret($event.target as HTMLTextAreaElement)"
          @paste="onPaste"
          @input="autosize"
        />
      </div>
      <button class="term-send-btn" type="button" :disabled="!input.trim()" title="Send command" aria-label="Send command" @click="submit">
        <AppIcon name="solar:arrow-up-bold" :size="16" />
      </button>
      <select
        v-if="voice.isSupported.value"
        class="ghost-select"
        :value="voice.lang.value"
        aria-label="Voice language"
        title="Voice language: English (EN) or Portuguese (PT-BR)"
        @change="voice.setLang(($event.target as HTMLSelectElement).value as 'en-US' | 'pt-BR')"
      >
        <option value="en-US">EN</option>
        <option value="pt-BR">PT</option>
      </select>
      <button
        v-if="voice.isSupported.value"
        class="ghost-btn"
        type="button"
        :title="voice.listening.value ? `Listening… ${voice.interim.value}` : 'Voice command (typos auto-fixed)'"
        @click="toggleVoice"
      >{{ voice.listening.value ? 'Stop' : 'Dictate' }}</button>
    </div>
    <div class="term-hints" aria-hidden="true">tab complete (×2 lists) · ↑↓ history · ctrl+r search · ctrl+l clear · !n re-runs · shift+enter newline · alt+⌫ kills word</div>
  </div>
</template>

<style scoped>
.terminal-panel {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  overflow: hidden;
  background: var(--ui-terminal);
  color: var(--ui-text);
  font-family: var(--ui-font-mono);
  font-size: 13px;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--ui-border);
  flex: 0 0 auto;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 0 0 auto;
}

.term-lights {
  display: inline-flex;
  gap: 5px;
}

.term-lights .lt {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  opacity: 0.85;
}

.term-lights .r { background: #ff5f56; }
.term-lights .y { background: #ebf755; }
.term-lights .g { background: #5af78e; }

.icon-terminal {
  color: var(--ui-accent);
  display: inline-flex;
}

.panel-title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  font-family: var(--ui-font-mono);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.job-badge {
  font-size: 11px;
  font-weight: 500;
  padding: 1px 8px;
  border-radius: var(--ui-radius-full);
  border: 1px solid var(--ui-border);
  color: var(--ui-text-3);
  white-space: nowrap;
}

.job-badge.on {
  color: var(--ui-warning);
  border-color: var(--ui-warning);
}

.exit-badge {
  font-size: 11px;
  font-weight: 700;
  font-family: var(--ui-font-mono);
  padding: 1px 8px;
  border-radius: var(--ui-radius-full);
  border: 1px solid var(--ui-danger);
  color: var(--ui-danger);
  white-space: nowrap;
}

.meta-badge {
  font-size: 11px;
  font-family: var(--ui-font-mono);
  padding: 1px 8px;
  border-radius: var(--ui-radius-full);
  border: 1px solid var(--ui-border);
  color: var(--ui-text-3);
  white-space: nowrap;
}

.ghost-btn {
  background: transparent;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-xs);
  color: color-mix(in srgb, var(--ui-text) 70%, transparent);
  font-family: inherit;
  font-size: 11px;
  padding: 3px 8px;
  cursor: pointer;
  white-space: nowrap;
}

.ghost-btn:hover {
  color: var(--ui-text);
  border-color: var(--ui-border-strong);
}

.ghost-select {
  background: transparent;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-xs);
  color: color-mix(in srgb, var(--ui-text) 70%, transparent);
  font-family: inherit;
  font-size: 11px;
  font-weight: 700;
  padding: 3px 6px;
  cursor: pointer;
}

.term-findbar,
.term-rsearch {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  border-bottom: 1px solid var(--ui-border);
  background: color-mix(in srgb, var(--ui-surface) 60%, transparent);
  flex: 0 0 auto;
  font-size: 12px;
}

.term-find-input,
.term-rsearch-input {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-xs);
  color: var(--ui-text);
  font-family: inherit;
  font-size: 12px;
  padding: 4px 8px;
  outline: none;
}

.term-find-input:focus,
.term-rsearch-input:focus {
  border-color: var(--ui-accent);
}

.term-find-count {
  font-family: var(--ui-font-mono);
  color: var(--ui-text-3);
  white-space: nowrap;
}

.term-rsearch-label {
  color: var(--ui-accent);
  font-weight: 700;
  white-space: nowrap;
}

.term-rsearch-hit {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.term-rsearch-hint {
  color: var(--ui-text-3);
  font-size: 11px;
  white-space: nowrap;
}

.term-scroll {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  touch-action: pan-x pan-y;
  scrollbar-gutter: stable;
  padding: 8px 12px;
  /* `strict` included size containment, which fights the flex layout and
     can pin the scroll height so new lines never become reachable.
     Layout + paint containment keeps the 60fps budget without that. */
  contain: layout paint;
}

.term-scroll::-webkit-scrollbar {
  width: 8px;
}

.term-scroll::-webkit-scrollbar-thumb {
  background: color-mix(in srgb, var(--ui-text) 28%, transparent);
  border-radius: var(--ui-radius-full);
  border: 2px solid transparent;
  background-clip: content-box;
}

.term-scroll::-webkit-scrollbar-thumb:hover {
  background: color-mix(in srgb, var(--ui-text) 42%, transparent);
  background-clip: content-box;
  border: 2px solid transparent;
}

.term-line {
  position: relative;
  white-space: pre-wrap;
  word-break: break-word;
  line-height: 1.45;
  min-height: 1.45em;
  padding-right: 28px;
  border-radius: var(--ui-radius-xs);
}

.term-line:focus {
  outline: 1px solid color-mix(in srgb, var(--ui-accent) 50%, transparent);
}

.term-line:hover {
  background: color-mix(in srgb, var(--ui-text) 4%, transparent);
}

.term-line .term-copy {
  position: absolute;
  right: 2px;
  top: 1px;
  display: none;
  background: transparent;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-xs);
  color: var(--ui-text-3);
  font-size: 11px;
  line-height: 1;
  padding: 2px 5px;
  cursor: copy;
}

.term-line:hover .term-copy,
.term-line:focus-within .term-copy {
  display: inline-block;
}

.term-line .term-copy:hover {
  color: var(--ui-text);
  border-color: var(--ui-border-strong);
}

.term-has-find {
  outline: 1px dashed color-mix(in srgb, var(--ui-warning) 60%, transparent);
}

.term-current-find {
  outline: 1px solid var(--ui-warning);
  background: color-mix(in srgb, var(--ui-warning) 12%, transparent);
}

.kind-in {
  color: var(--ui-text);
}

.kind-out {
  color: var(--ui-text);
}

.kind-err {
  color: var(--ui-danger);
}

.kind-sys {
  color: color-mix(in srgb, var(--ui-text) 40%, transparent);
  font-style: italic;
}

.term-user {
  color: var(--ui-accent);
  font-weight: 700;
}

.term-cwd {
  color: var(--ui-info);
}

.term-dollar {
  color: var(--ui-success);
  font-weight: 700;
  margin-right: 6px;
}

.term-dollar.fail {
  color: var(--ui-danger);
}

.term-cmd {
  color: var(--ui-text);
}

.term-link {
  color: var(--ui-accent);
  text-decoration: underline;
  text-underline-offset: 2px;
}

.term-path {
  background: none;
  border: none;
  padding: 0;
  margin: 0;
  font: inherit;
  color: inherit;
  text-decoration: underline dotted;
  text-underline-offset: 2px;
  cursor: pointer;
}

.term-path:hover {
  color: var(--ui-accent);
}

.term-empty {
  color: var(--ui-text-3);
  font-style: italic;
  padding: 12px 0;
}

.term-jump {
  position: absolute;
  left: 50%;
  bottom: 76px;
  transform: translateX(-50%);
  background: color-mix(in srgb, var(--ui-surface) 92%, transparent);
  border: 1px solid var(--ui-accent);
  border-radius: var(--ui-radius-full);
  color: var(--ui-accent);
  font-family: inherit;
  font-size: 12px;
  padding: 4px 14px;
  cursor: pointer;
  z-index: 3;
}

.term-input-row {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 8px 12px 4px;
  border-top: 1px solid var(--ui-border);
  flex: 0 0 auto;
}

.term-send-btn {
  display: none;
}

.term-prompt {
  white-space: nowrap;
  padding-top: 2px;
  line-height: 1.45;
}

.term-input {
  flex: 1;
  width: 100%;
  background: transparent;
  border: none;
  outline: none;
  color: var(--ui-text);
  font-family: inherit;
  font-size: inherit;
  line-height: 1.45;
  resize: none;
  overflow-y: auto;
  caret-color: var(--ui-accent);
  position: relative;
  z-index: 1;
  padding: 2px 0 0;
}

/* Ghost auto-suggest: the invisible current text keeps metrics identical
 * so the grey remainder aligns exactly with the real caret. */
.term-input-wrap {
  position: relative;
  flex: 1;
  display: flex;
  min-width: 0;
}

.term-ghost {
  position: absolute;
  inset: 0;
  z-index: 0;
  pointer-events: none;
  font-family: inherit;
  font-size: inherit;
  line-height: 1.45;
  white-space: pre-wrap;
  word-break: break-word;
  padding: 2px 0 0;
}

.ghost-hide {
  visibility: hidden;
}

.ghost-show {
  color: color-mix(in srgb, var(--ui-text) 35%, transparent);
}

.term-hints {
  padding: 2px 12px 8px 12px;
  color: color-mix(in srgb, var(--ui-text) 35%, transparent);
  font-size: 10px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 0 0 auto;
}

@media (max-width: 760px) {
  .term-hints {
    display: none;
  }

  .header-right .ghost-btn:nth-child(2),
  .header-right .ghost-btn:nth-child(3),
  .header-right .ghost-btn:nth-child(4) {
    display: none;
  }
}

@media (max-width: 560px) {
  .terminal-panel {
    height: 100dvh;
    min-height: 100dvh;
    padding-top: env(safe-area-inset-top, 0px);
    box-sizing: border-box;
  }

  .panel-header {
    min-height: 44px;
    padding: 8px 12px;
  }

  .term-lights {
    display: none;
  }

  .ghost-btn,
  .ghost-select {
    min-height: 44px;
    padding: 8px 12px;
    touch-action: manipulation;
  }

  /* `.header-right .ghost-btn` used to re-crush these to `min-height: 0`
     (≈25px) right after the tap-floor rule above — the header actions became
     the smallest targets on the phone screen. They keep the 44px floor; the
     ≤760px block already hides all but the first action, so the row fits. */

  .term-scroll {
    padding: 10px 12px;
    scroll-padding-bottom: 12px;
  }

  .term-line {
    font-size: 15px;
    line-height: 1.5;
  }

  .term-input-row {
    position: sticky;
    bottom: 0;
    z-index: 2;
    flex-wrap: wrap;
    padding: 8px 12px calc(8px + env(safe-area-inset-bottom, 0px));
    background: var(--ui-terminal);
    box-shadow: 0 -6px 16px color-mix(in srgb, var(--ui-terminal) 80%, transparent);
  }

  .term-prompt {
    min-height: 44px;
    display: inline-flex;
    align-items: center;
  }

  .term-input-wrap {
    min-height: 44px;
    align-items: center;
  }

  .term-send-btn {
    display: inline-flex;
    flex: 0 0 44px;
    align-items: center;
    justify-content: center;
    width: 44px;
    min-height: 44px;
    border: 1px solid color-mix(in srgb, var(--ui-accent) 48%, transparent);
    border-radius: 12px;
    background: color-mix(in srgb, var(--ui-accent) 16%, transparent);
    color: var(--ui-accent);
    touch-action: manipulation;
  }

  .term-send-btn:disabled {
    opacity: .45;
  }

  .term-input {
    min-height: 44px;
    font-size: 16px;
    touch-action: manipulation;
  }
  .term-ghost { font-size: 16px; }
}
</style>
