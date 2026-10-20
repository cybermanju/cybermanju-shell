// CyberManju — minimal, safe markdown for agent transcripts.
//
// Deliberately dependency-free: agent output is untrusted model text, so the
// only safe renderer is one that escapes FIRST and then re-introduces a
// whitelist of markup. No raw HTML ever survives. Fences are lifted out
// before escaping so code keeps its exact bytes.

const HTML_ESCAPES: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
}

export function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, ch => HTML_ESCAPES[ch])
}

/** Only web-safe link targets survive; `javascript:` and friends do not. */
function safeHref(href: string): string | null {
  const url = href.trim()
  if (/^(https?:\/\/|mailto:|#|\/)/i.test(url)) return url
  return null
}

/** Inverse of `escapeHtml`, for hrefs captured out of already-escaped text. */
function unescapeText(text: string): string {
  return text
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&amp;/g, '&')
}

function renderInline(text: string): string {
  // Escape FIRST: model output is untrusted, and `<`/`>` are the only thing
  // standing between a transcript and tag injection. Everything below adds
  // markup back on top of that escaped string.
  let out = escapeHtml(text)
  // `code` first so its contents are never re-processed.
  out = out.replace(/`([^`\n]+)`/g, (_m, code: string) => `<code>${code}</code>`)
  // [label](href)
  out = out.replace(/\[([^\]\n]+)\]\(([^)\s]+)\)/g, (m, label: string, href: string) => {
    const safe = safeHref(unescapeText(href))
    return safe ? `<a href="${escapeHtml(safe)}" target="_blank" rel="noopener noreferrer">${label}</a>` : m
  })
  // **bold** then *italic*
  out = out.replace(/\*\*([^*\n]+)\*\*/g, '<strong>$1</strong>')
  out = out.replace(/(^|[^*\w])\*([^*\n]+)\*(?=[^*\w]|$)/g, '$1<em>$2</em>')
  return out
}

interface Block {
  type: 'code' | 'text'
  html: string
}

/**
 * Render agent markdown to an HTML string that is safe for `v-html`.
 * Supports: fenced code, headings, hr, blockquotes, ul/ol, paragraphs with
 * hard line breaks, inline code / bold / italic / links.
 */
export function renderMarkdown(source: string): string {
  const src = (source ?? '').replace(/\r\n?/g, '\n')
  if (!src.trim()) return ''

  // 1. Pull fenced code out verbatim.
  const fences: string[] = []
  // Guard char for lifted fences (NUL — never appears in model text).
  const FENCE_GUARD = String.fromCharCode(0)
  const lift = (lang: string, body: string): string => {
    const i = fences.push(`<pre class="md-code"><code${lang ? ` class="language-${escapeHtml(lang)}"` : ''}>${escapeHtml(body.replace(/\n$/, ''))}</code></pre>`) - 1
    return `${FENCE_GUARD}FENCE${i}${FENCE_GUARD}`
  }
  const stripped = src.replace(/```([A-Za-z0-9_+-]*)\n?([\s\S]*?)```/g, (_m, lang: string, body: string) => lift(lang, body))
  // Trailing unclosed fence (the model sometimes stops before the closer):
  // treat the rest as code rather than leaking raw ticks. Skipped when the
  // tail already holds a lifted fence, so placeholders never end up escaped
  // inside a code block.
  const stripped2 = stripped.replace(/```([A-Za-z0-9_+-]*)\n([\s\S]*)$/, (_m, lang: string, body: string) =>
    /\u0000FENCE\d+\u0000/.test(body) ? _m : lift(lang, body),
  )

  const blocks: Block[] = []
  const lines = stripped2.split('\n')
  let para: string[] = []
  let list: { ordered: boolean; items: string[] } | null = null

  const flushPara = () => {
    if (!para.length) return
    blocks.push({ type: 'text', html: `<p>${para.map(renderInline).join('<br>')}</p>` })
    para = []
  }
  const flushList = () => {
    if (!list) return
    const tag = list.ordered ? 'ol' : 'ul'
    blocks.push({ type: 'text', html: `<${tag}>${list.items.map(i => `<li>${renderInline(i)}</li>`).join('')}</${tag}>` })
    list = null
  }
  const flushAll = () => {
    flushPara()
    flushList()
  }

  for (const raw of lines) {
    const line = raw.trimEnd()
    const fence = /^\u0000FENCE\d+\u0000$/.test(line.trim())
    if (fence) {
      flushAll()
      blocks.push({ type: 'code', html: line.trim() })
      continue
    }
    if (!line.trim()) {
      flushAll()
      continue
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line)
    if (heading) {
      flushAll()
      const level = Math.min(6, heading[1].length + 1) // h2..h7→h2..h6, keeps h1 for the app
      blocks.push({ type: 'text', html: `<h${level}>${renderInline(heading[2])}</h${level}>` })
      continue
    }
    if (/^([-*_])\1{2,}$/.test(line.trim())) {
      flushAll()
      blocks.push({ type: 'text', html: '<hr>' })
      continue
    }
    const quote = /^>\s?(.*)$/.exec(line)
    if (quote) {
      flushAll()
      blocks.push({ type: 'text', html: `<blockquote>${renderInline(quote[1])}</blockquote>` })
      continue
    }
    const ul = /^\s*[-*+]\s+(.*)$/.exec(line)
    const ol = /^\s*(\d+)[.)]\s+(.*)$/.exec(line)
    if (ul || ol) {
      flushPara()
      const ordered = !!ol
      const item = (ol ? ol[2] : ul![1]) as string
      if (list && list.ordered === ordered) list.items.push(item)
      else {
        flushList()
        list = { ordered, items: [item] }
      }
      continue
    }
    flushList()
    para.push(line)
  }
  flushAll()

  // 2. Restore fences.
  return blocks
    .map(b => b.html)
    .join('\n')
    .replace(/\u0000FENCE(\d+)\u0000/g, (_m, i: string) => fences[Number(i)] ?? '')
}
