// CyberManju OS — CodeDetect (src/utils/codeDetect.ts).
// Language detection + per-language highlighting for the code editor.
//
// `detectLanguage(fileName, content)` layers four signals so it keeps
// working where extension-only maps fail: pasted snippets (no file name),
// extensionless scripts (shebang), polyglot headers (modeline), and
// extension collisions (`.h` C vs C++, `.m` Matlab vs Objective-C).

export type DetectVia = 'filename' | 'shebang' | 'modeline' | 'content' | 'fallback'

export interface DetectedLanguage {
  language: string
  via: DetectVia
}

const EXT_MAP: Record<string, string> = {
  rs: 'rust', py: 'python', pyw: 'python', js: 'javascript', jsx: 'javascript',
  mjs: 'javascript', cjs: 'javascript', ts: 'typescript', tsx: 'typescript',
  mts: 'typescript', cts: 'typescript', go: 'go', sh: 'bash', bash: 'bash',
  zsh: 'bash', fish: 'bash', vue: 'vue', svelte: 'svelte', json: 'json',
  jsonc: 'json', md: 'markdown', markdown: 'markdown', html: 'html',
  htm: 'html', css: 'css', scss: 'css', less: 'css', sql: 'sql', toml: 'toml',
  yaml: 'yaml', yml: 'yaml', xml: 'xml', svg: 'xml', c: 'c', h: 'c',
  cpp: 'cpp', hpp: 'cpp', cc: 'cpp', cxx: 'cpp', hh: 'cpp', cs: 'csharp',
  java: 'java', kt: 'kotlin', kts: 'kotlin', swift: 'swift', rb: 'ruby',
  php: 'php', lua: 'lua', r: 'r', pl: 'perl', pm: 'perl', ps1: 'powershell',
  dart: 'dart', hs: 'haskell', ex: 'elixir', exs: 'elixir', erl: 'erlang',
  sol: 'solidity', proto: 'proto', gql: 'graphql', graphql: 'graphql',
  tex: 'latex', ini: 'ini', cfg: 'ini', conf: 'ini', cmake: 'cmake',
  dockerfile: 'docker', makefile: 'make',
}

const BASENAME_MAP: Record<string, string> = {
  dockerfile: 'docker', makefile: 'make', cmakelists: 'cmake',
  'package.json': 'json', 'tsconfig.json': 'json', '.bashrc': 'bash',
  '.zshrc': 'bash', gemfile: 'ruby', rakefile: 'ruby',
}

const SHEBANG_MAP: Array<[RegExp, string]> = [
  [/\bpython3?\b/, 'python'],
  [/\bnode\b|\bdeno\b|\bbun\b/, 'javascript'],
  [/\b(bash|sh|zsh|fish)\b/, 'bash'],
  [/\bruby\b/, 'ruby'],
  [/\bperl\b/, 'perl'],
  [/\bphp\b/, 'php'],
  [/\blua\b/, 'lua'],
]

const MODELINE_FT = /vim?:.*\bft=([a-z0-9+#-]+)/i
const MODELINE_MODE = /-\*-\s*mode:\s*([a-z0-9+#-]+)/i

const CONTENT_SNIFFS: Array<[RegExp, string]> = [
  [/^\s*<\?php\b/m, 'php'],
  [/^\s*package\s+main\b/m, 'go'],
  [/^\s*fn\s+main\s*\(/, 'rust'],
  [/^\s*use\s+strict\b|^\s*module\.exports\b/m, 'javascript'],
  [/^\s*interface\s+\w+\s*\{|:\s*(string|number|boolean)(\[\])?\s*[;=]/m, 'typescript'],
  [/^\s*def\s+\w+\s*\(.*\)\s*:/m, 'python'],
  [/^\s*class\s+\w+\s*(?:\(|:)/m, 'python'],
  [/^\s*#[a-z-]+\s*\(.*\)\s*$/m, 'rust'],
  [/^\s*SELECT\b.*\bFROM\b/mis, 'sql'],
  [/^\s*\{[\s\S]*"[^"]+"\s*:/m, 'json'],
  [/^\s*<\w+[\s>]/m, 'html'],
  [/^\s*@media\b|^\s*[.#]?\w[\w-]*\s*\{[^}]*:[^}]*\}/m, 'css'],
  [/^\s*---\s*$/m, 'yaml'],
]

function extOf(name: string): string {
  const base = name.split('/').pop() ?? name
  const dot = base.lastIndexOf('.')
  if (dot <= 0) return ''
  return base.slice(dot + 1).toLowerCase()
}

/** Legacy helper kept for callers that only have a file name. */
export function langOf(name: string): string {
  return detectLanguage(name, '').language
}

export function detectLanguage(fileName: string, content: string): DetectedLanguage {
  const base = (fileName.split('/').pop() ?? fileName).toLowerCase()
  if (base) {
    const hit = BASENAME_MAP[base]
    if (hit) return { language: hit, via: 'filename' }
    const clean = base.replace(/^\./, '')
    const bhit = BASENAME_MAP[clean]
    if (bhit) return { language: bhit, via: 'filename' }
    const ext = extOf(base)
    if (ext) {
      // `.h` serves C and C++: sniff for classes/namespaces before defaulting.
      if (ext === 'h' && /\b(class|namespace|template\s*<)\b/.test(content)) {
        return { language: 'cpp', via: 'content' }
      }
      const mapped = EXT_MAP[ext]
      if (mapped) return { language: mapped, via: 'filename' }
      return { language: ext, via: 'fallback' }
    }
  }
  if (content) {
    const first = content.split('\n', 3).join('\n')
    if (/^#!/.test(first)) {
      for (const [re, lang] of SHEBANG_MAP) {
        if (re.test(first)) return { language: lang, via: 'shebang' }
      }
    }
    const head = content.slice(0, 2000)
    const ft = head.match(MODELINE_FT)?.[1].toLowerCase()
    if (ft) return { language: EXT_MAP[ft] ?? ft, via: 'modeline' }
    const mode = head.match(MODELINE_MODE)?.[1].toLowerCase()
    if (mode) return { language: EXT_MAP[mode] ?? mode, via: 'modeline' }
    for (const [re, lang] of CONTENT_SNIFFS) {
      if (re.test(content)) return { language: lang, via: 'content' }
    }
  }
  return { language: 'text', via: 'fallback' }
}

// ─── per-language highlighting ───────────────────────────────────────────

const BASE_KEYWORDS = [
  'if', 'else', 'for', 'while', 'do', 'switch', 'case', 'break', 'continue',
  'return', 'function', 'class', 'struct', 'enum', 'interface', 'type',
  'var', 'let', 'const', 'import', 'export', 'from', 'def', 'async', 'await',
  'try', 'catch', 'throw', 'new', 'this', 'super', 'extends', 'implements',
  'pub', 'fn', 'mut', 'use', 'mod', 'impl', 'trait', 'where', 'package',
  'void', 'int', 'float', 'double', 'char', 'bool', 'string', 'null',
  'undefined', 'true', 'false', 'static', 'private', 'public', 'protected',
  'readonly', 'abstract', 'virtual', 'override', 'match', 'loop', 'in', 'of',
  'self', 'Self',
]

const LANG_KEYWORDS: Record<string, string[]> = {
  rust: ['crate', 'dyn', 'ref', 'move', 'box', 'unsafe', 'extern', 'as', 'macro_rules', 'include', 'vec', 'Some', 'None', 'Ok', 'Err', 'mut', 'serde'],
  python: ['elif', 'except', 'finally', 'raise', 'lambda', 'with', 'as', 'yield', 'global', 'nonlocal', 'pass', 'del', 'is', 'not', 'and', 'or', 'print'],
  bash: ['fi', 'then', 'elif', 'esac', 'select', 'until', 'function', 'time', 'coproc', 'local', 'readonly', 'declare', 'echo', 'exit', 'test'],
  sql: ['select', 'from', 'where', 'join', 'left', 'right', 'inner', 'outer', 'on', 'group', 'order', 'by', 'having', 'insert', 'update', 'delete', 'create', 'table', 'distinct', 'limit', 'offset', 'as', 'and', 'or', 'not', 'null'],
  css: ['media', 'import', 'charset', 'keyframes', 'important'],
}

interface CommentStyle {
  line: string[]
  block?: [string, string]
}

const COMMENTS: Record<string, CommentStyle> = {
  python: { line: ['#'] },
  bash: { line: ['#'] },
  yaml: { line: ['#'] },
  toml: { line: ['#'] },
  ini: { line: ['#', ';'] },
  sql: { line: ['--'], block: ['/*', '*/'] },
  css: { line: [], block: ['/*', '*/'] },
  html: { line: [], block: ['<!--', '-->'] },
  xml: { line: [], block: ['<!--', '-->'] },
  vue: { line: ['//'], block: ['/*', '*/'] },
  svelte: { line: ['//'], block: ['/*', '*/'] },
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

/**
 * Keyword/string/number/comment highlighter. Same signature as the old
 * generic pass, but comment styles follow the language and keywords are
 * the union of the base set plus the language extras.
 */
export function highlightSyntax(code: string, lang: string): string {
  const lower = lang.toLowerCase()
  const extra = LANG_KEYWORDS[lower] ?? []
  const keywords = new Set([...BASE_KEYWORDS, ...extra])
  const style = COMMENTS[lower] ?? { line: ['//'], block: ['/*', '*/'] as [string, string] }
  const esc = escapeHtml(code)
  const e = (s: string) => escapeHtml(s).replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  // Protect comments first.
  let out = esc
  if (style.block) {
    const [b0, b1] = [e(style.block[0]), e(style.block[1])]
    out = out.replace(new RegExp(`${b0}[\\s\\S]*?${b1}`, 'g'), (m) => `\x01${m}\x02`)
  }
  for (const lc of style.line) {
    const rx = new RegExp(`(^|\\n)(${e(lc)}[^\\n]*)`, 'g')
    out = out.replace(rx, (_, pre: string, c: string) => `${pre}\x01${c}\x02`)
  }
  // Strings, then numbers, then keywords.
  out = out
    .replace(/("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`)/g, '\x03$1\x04')
    .replace(/\b(\d+(?:\.\d+)?)\b/g, '\x05$1\x06')
    .replace(/\b([a-zA-Z_]\w*)\b/g, (m) => (keywords.has(m) || keywords.has(m.toLowerCase()) ? `\x07${m}\x08` : m))
  return out
    .replace(/\x01([\s\S]*?)\x02/g, '<span class="sx-c">$1</span>')
    .replace(/\x03([\s\S]*?)\x04/g, '<span class="sx-s">$1</span>')
    .replace(/\x05([\s\S]*?)\x06/g, '<span class="sx-n">$1</span>')
    .replace(/\x07([\s\S]*?)\x08/g, '<span class="sx-k">$1</span>')
}

// NOTE: the four patterns above use \x01..\x08 sentinel escapes.

// ─── bracket balance (cheap client-side diagnostic) ─────────────────────

export interface BracketIssue {
  line: number
  message: string
}

/** Unbalanced `()[]{}` outside strings/comments, reported at the line. */
export function checkBrackets(content: string, language: string): BracketIssue[] {
  const lower = language.toLowerCase()
  const pairs: Record<string, string> = { '(': ')', '[': ']', '{': '}' }
  const closers = new Set(Object.values(pairs))
  // Strip strings + comments coarsely so brackets inside them don't count.
  const style = COMMENTS[lower] ?? { line: ['//'] }
  let code = content
    .replace(/("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`)/g, (m) => ' '.repeat(m.length))
  // Strip line comments so brackets inside them don't count.
  code = code
    .split('\n')
    .map((ln) => {
      for (const lc of style.line) {
        const i = ln.indexOf(lc)
        if (i !== -1) return ln.slice(0, i)
      }
      return ln
    })
    .join('\n')
  const stack: Array<{ ch: string; line: number }> = []
  const issues: BracketIssue[] = []
  const lines = code.split('\n')
  lines.forEach((ln, li) => {
    for (const ch of ln) {
      if (pairs[ch]) stack.push({ ch, line: li + 1 })
      else if (closers.has(ch)) {
        const top = stack[stack.length - 1]
        if (top && pairs[top.ch] === ch) stack.pop()
        else issues.push({ line: li + 1, message: `Unmatched '${ch}'` })
      }
    }
  })
  for (const left of stack) {
    issues.push({ line: left.line, message: `Unclosed '${left.ch}' (opened here)` })
  }
  return issues.slice(0, 20)
}
