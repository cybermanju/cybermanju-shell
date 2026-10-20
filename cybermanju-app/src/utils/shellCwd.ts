// CyberManju OS — ShellCwd (src/utils/shellCwd.ts).
// Client-side mirror of the stateful cybsh working directory.
//
// The server shell (`crates/os/src/shell.rs`) keeps a single process-wide cwd,
// so every `cd` a panel sends persists for the next `os_exec` call — including
// calls from *other* panels. Each panel keeps its own mirror: it seeds the
// mirror when it navigates explicitly (`cd <dir> && …`) and advances it with
// `trackShellCwd` for lines the user types. The mirror can drift if another
// panel `cd`s elsewhere; that is expected and cheap — the next explicit
// navigation re-seeds it.

/** Quote one path for cybsh (mirrors the local `sh()` helpers in the panels). */
export function shQuote(p: string): string {
  return `"${p.replace(/"/g, '\\"')}"`
}

function stripQuotes(s: string): string {
  const t = s.trim()
  if (t.length >= 2 && ((t.startsWith('"') && t.endsWith('"')) || (t.startsWith("'") && t.endsWith("'")))) {
    return t.slice(1, -1)
  }
  return t
}

function normalizeCwd(cwd: string, dest: string): string {
  const d = stripQuotes(dest).trim()
  if (!d || d === '-') return cwd
  // `~` expansion lives server-side; never guess it here.
  if (d === '~') return cwd
  const parts = (d.startsWith('/') ? d : `${cwd.replace(/\/+$/, '')}/${d}`).split('/')
  const out: string[] = []
  for (const part of parts) {
    if (!part || part === '.') continue
    if (part === '..') {
      if (out.length) out.pop()
      continue
    }
    out.push(part)
  }
  return `/${out.join('/')}` || '/'
}

const CD_SEGMENT = /^\s*cd(?:\s+(.*?))?\s*$/

/**
 * Advance a panel's cwd mirror past one typed line. Only `cd` segments move
 * the mirror (`cd /x`, `cd rel`, `cd ..`, `cd "a b"`, chained with `&&`/`;`);
 * everything else leaves it untouched. Tracks the LAST `cd` in the chain,
 * matching sequential shell execution.
 */
export function trackShellCwd(cwd: string, line: string): string {
  let next = cwd || '/'
  for (const segment of line.split(/&&|;/)) {
    const m = CD_SEGMENT.exec(segment)
    if (!m) continue
    const dest = (m[1] ?? '').trim()
    // Bare `cd` with no destination: leave the mirror alone (server decides).
    if (!dest) continue
    // `cd -os …` moves the *host* cwd (a separate namespace) — the volume
    // mirror must not follow it, or the next explicit navigation shows a
    // garbage `-os "…"` path.
    if (/^(-os|--host|--os)(\s|$)/.test(dest)) continue
    next = normalizeCwd(next, dest)
  }
  return next
}
