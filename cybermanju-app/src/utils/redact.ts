// CyberManju OS — Redact (src/utils/redact.ts).
// Secret redaction for browser-loop tool outputs (mirror of
// `crates/agent/src/redact.rs` — keep the marker lists in sync).
//
// Tool results flow into the transcript, which persists to localStorage and
// is sent to the provider — a pasted API key in file contents or an
// `Authorization:` echo must never land there. `redactText` scans for known
// secret markers (ASCII case-insensitive) plus JWT shapes and replaces the
// secret run with `***`, keeping the marker so the model still sees *which*
// credential kind appeared. `cleanToolOutput` is the `clean_output` twin:
// redacted text plus a note when something was hidden (otherwise the model
// retries the failing read).

const MARKERS: readonly string[] = [
  'sk-ant-',
  'sk-',
  'AKIA',
  'ghp_',
  'gho_',
  'ghu_',
  'github_pat_',
  'xox',
  'glpat-',
  '-----BEGIN PRIVATE KEY-----',
  '-----BEGIN RSA PRIVATE KEY-----',
  '-----BEGIN OPENSSH PRIVATE KEY-----',
  'Bearer ',
  'api_key=',
  'apiKey=',
  'apikey=',
  'key=',
  'password=',
  'passwd=',
  'passphrase=',
  'secret=',
  'token=',
  'jwt_secret',
  'ya29.',
]

function isTokenChar(ch: string): boolean {
  return /^[A-Za-z0-9\-_./+=~]$/.test(ch)
}

/** Redact secrets in `text`. Returns the redacted text and hit count. */
export function redactText(text: string): { text: string; count: number } {
  const hits: Array<[number, number]> = []
  // Longest marker first, so `sk-ant-…` wins over its prefix `sk-`.
  const markers = [...MARKERS].sort((a, b) => b.length - a.length)
  const claimed = new Set<number>()
  for (const marker of markers) {
    const needle = marker.toLowerCase()
    let from = 0
    while (from + needle.length <= text.length) {
      const window = text.slice(from, from + needle.length).toLowerCase()
      if (window !== needle) {
        from += 1
        continue
      }
      const start = from
      const secretStart = start + marker.length
      let end = secretStart
      if (marker.startsWith('-----BEGIN')) {
        let probe = secretStart
        if (text[probe] === '\n') probe += 1
        end = probe
        while (end < text.length && text[end] !== '\n') end += 1
      } else {
        let run = 0
        while (end < text.length && run < 512 && isTokenChar(text[end])) {
          end += 1
          run += 1
        }
      }
      if (end > secretStart && !claimed.has(start)) {
        claimed.add(start)
        hits.push([secretStart, end])
        from = end
      } else {
        from = secretStart
      }
    }
  }
  scanJwt(text, hits)

  if (!hits.length) return { text, count: 0 }
  hits.sort((a, b) => a[0] - b[0] || a[1] - b[1])
  const merged: Array<[number, number]> = []
  for (const [start, end] of hits) {
    const last = merged[merged.length - 1]
    if (last && start <= last[1]) {
      last[1] = Math.max(last[1], end)
      continue
    }
    merged.push([start, end])
  }
  let out = ''
  let cursor = 0
  for (const [start, end] of merged) {
    out += text.slice(cursor, start) + '***'
    cursor = end
  }
  out += text.slice(cursor)
  return { text: out, count: merged.length }
}

/** JWT shapes (`eyJ…​.…​.…`) with no marker at all. */
function scanJwt(text: string, hits: Array<[number, number]>): void {
  let i = 0
  while (i + 4 < text.length) {
    if (text.slice(i, i + 3) === 'eyJ') {
      let end = i + 3
      let dots = 0
      while (end < text.length && isTokenChar(text[end])) {
        if (text[end] === '.') dots += 1
        end += 1
      }
      if (dots === 2 && end - i > 20 && end - i < 4096) {
        hits.push([i, end])
        i = end
        continue
      }
    }
    i += 1
  }
}

/** Redact a tool result before it enters the transcript (native twin). */
export function cleanToolOutput(output: string): string {
  const { text, count } = redactText(output)
  if (count === 0) return text
  return `${text}\n(redacted ${count} secret(s) from tool output)`
}
