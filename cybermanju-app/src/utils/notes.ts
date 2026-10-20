// CyberManju OS — Notes (src/utils/notes.ts).
// Role: knowledge-base over markdown files in the vault (ANALYSIS.md §4.2) —
// note detection, `[[wikilink]]` extraction, backlink graph. Indexing rides
// the Tantivy filename+content+tags BM25 already wired for tags; backlinks
// are pure frontend per the ANALYSIS note.
// Exports: NoteDoc, isNoteFile, extractWikilinks, extractMdLinks,
// buildNoteIndex, NoteIndex, NoteMemoryPayload, noteToMemory.
// Integrates: standalone; ArchivePanel NOTES tab renders it over
// store.files (+ contentText where loaded).
// Contracts: pure, never throws; links resolve against note basenames
// case-insensitively, `.md`-suffix-agnostic.
export interface NoteMemoryPayload {
  /** Agent-memory text: title + first 1800 chars (2200-char recall budget). */
  text: string
  /** Tags carried into the memory row (note + backlink count). */
  tags: string[]
}
export interface NoteDoc {
  id: string
  name: string
  path?: string
  contentText?: string
}

export interface MdLink {
  text: string
  target: string
}

export interface NoteIndex {
  notes: NoteDoc[]
  /** note id → linked note ids (resolved). */
  links: Record<string, string[]>
  /** note id → note ids linking to it. */
  backlinks: Record<string, string[]>
  /** notes nothing links to (excluding the notes themselves). */
  orphans: string[]
  /** { from, target } for links matching no note. */
  broken: Array<{ from: string; target: string }>
}

/** `.md` / `.markdown` / `.txt` count as notes; folders never do. */
export function isNoteFile(name: string, fileType?: string): boolean {
  if (fileType === 'folder') return false
  return /\.(md|markdown|txt)$/i.test(name.trim())
}

/** `[[target]]` and `[[target|label]]` — labels never leak into targets. */
export function extractWikilinks(text: string): string[] {
  const out: string[] = []
  const re = /\[\[([^\]|]+)(?:\|[^\]]*)?\]\]/g
  let m: RegExpExecArray | null
  while ((m = re.exec(text)) !== null) {
    const target = m[1].trim()
    if (target) out.push(target)
  }
  return [...new Set(out)]
}

/** Inline `[text](target)` links; image `![..](..)` included (same graph). */
export function extractMdLinks(text: string): MdLink[] {
  const out: MdLink[] = []
  const re = /!?\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g
  let m: RegExpExecArray | null
  while ((m = re.exec(text)) !== null) {
    const target = m[1].trim()
    if (target && !/^(https?:|mailto:|#)/i.test(target)) out.push({ text: '', target })
  }
  return out
}

function baseName(name: string): string {
  return name.replace(/\.(md|markdown|txt)$/i, '').toLowerCase()
}

/** Build the link graph + backlinks + orphans + broken-link list. */
export function buildNoteIndex(notes: NoteDoc[]): NoteIndex {
  const byBase = new Map<string, string>()
  for (const note of notes) {
    if (!byBase.has(baseName(note.name))) byBase.set(baseName(note.name), note.id)
  }
  const links: Record<string, string[]> = {}
  const backlinks: Record<string, string[]> = {}
  const broken: Array<{ from: string; target: string }> = []
  for (const note of notes) {
    const text = note.contentText ?? ''
    const raw = [...extractWikilinks(text), ...extractMdLinks(text).map((l) => l.target)]
    const resolved: string[] = []
    for (const target of new Set(raw)) {
      const clean = target.split('#')[0].split('/').pop()!.replace(/\.(md|markdown|txt)$/i, '').toLowerCase()
      const id = byBase.get(clean)
      if (id && id !== note.id) resolved.push(id)
      else if (!id) broken.push({ from: note.id, target })
    }
    links[note.id] = resolved
  }
  for (const note of notes) backlinks[note.id] = []
  for (const [from, tos] of Object.entries(links)) {
    for (const to of tos) backlinks[to].push(from)
  }
  const orphans = notes.filter((n) => backlinks[n.id].length === 0).map((n) => n.id)
  return { notes, links, backlinks, orphans, broken }
}

/** Auto-index bridge (ANALYSIS.md §4.2): a saved note becomes one agent-memory
 * row. Text is capped for the 2200-char recall budget; empty notes refuse. */
export function noteToMemory(note: NoteDoc, backlinkCount = 0): NoteMemoryPayload | null {
  const body = (note.contentText ?? '').trim()
  if (!body) return null
  const head = body.length > 1800 ? `${body.slice(0, 1800)}…` : body
  const tags = ['note']
  if (backlinkCount > 0) tags.push(`backlinks:${backlinkCount}`)
  return { text: `${note.name}\n${head}`, tags }
}
