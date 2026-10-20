// CyberManju OS — Bookmarks (src/utils/bookmarks.ts).
// Role: round-out daily use, part 2 (ANALYSIS.md §4.9) — bookmarks whose
// import rides the existing `import_from_url` command. Pure list + codec;
// the fetch itself stays the store.importFromUrl action.
// Exports: BookmarkItem, addBookmark, removeBookmark, markImported,
// parseBookmarksStore, serializeBookmarksStore, isHttpUrl.
// Integrates: standalone (localStorage `cybermanju.bookmarks.v1`);
// ArchivePanel BOOKMARKS tab.
// Contracts: pure, never throws; only http(s) URLs are accepted; dup URLs
// re-title instead of duplicating.
export interface BookmarkItem {
  id: string
  url: string
  title: string
  createdAt: string
  /** File id once `import_from_url` has archived the page. */
  importedFileId?: string
}

export function isHttpUrl(value: string): boolean {
  try {
    const u = new URL(value.trim())
    return u.protocol === 'http:' || u.protocol === 'https:'
  } catch {
    return false
  }
}

function uid(existing: Set<string>): string {
  let id = ''
  do {
    id = `b-${Date.now().toString(36)}-${Math.floor(Math.random() * 0xffffff).toString(36)}`
  } while (existing.has(id))
  return id
}

export function addBookmark(
  list: BookmarkItem[],
  url: string,
  title?: string,
): { list: BookmarkItem[]; item: BookmarkItem } | null {
  const clean = url.trim()
  if (!isHttpUrl(clean)) return null
  const dup = list.find((b) => b.url === clean)
  if (dup) {
    const next = list.map((b) =>
      b.url === clean ? { ...b, title: title?.trim() || b.title } : b,
    )
    return { list: next, item: { ...dup, title: title?.trim() || dup.title } }
  }
  const item: BookmarkItem = {
    id: uid(new Set(list.map((b) => b.id))),
    url: clean,
    title: title?.trim().slice(0, 200) || clean,
    createdAt: new Date().toISOString(),
  }
  return { list: [...list, item], item }
}

export function removeBookmark(list: BookmarkItem[], id: string): BookmarkItem[] {
  return list.filter((b) => b.id !== id)
}

export function markImported(list: BookmarkItem[], id: string, fileId: string): BookmarkItem[] {
  return list.map((b) => (b.id === id ? { ...b, importedFileId: fileId } : b))
}

export function parseBookmarksStore(value: unknown): BookmarkItem[] {
  let raw: unknown = value
  if (typeof raw === 'string') {
    try {
      raw = JSON.parse(raw)
    } catch {
      return []
    }
  }
  if (!Array.isArray(raw)) return []
  return raw.filter((e): e is BookmarkItem => {
    return !!e && typeof e === 'object' && typeof (e as BookmarkItem).id === 'string' && isHttpUrl((e as BookmarkItem).url ?? '')
  })
}

export function serializeBookmarksStore(list: BookmarkItem[]): string {
  return JSON.stringify(list)
}
