// CyberManju OS — Stars (src/utils/stars.ts).
// Persisted favorites — star-id list codec.
//
// The Rust `FileNode` schema ships no star column on any backend (Tauri,
// REST, WASM), so stars live beside the file table: a JSON string array in
// localStorage (`cybermanju.stars.v1`) mirrored into the worker kv (`stars`
// key) so static-host favorites ride inside the `.cybermanju` container next
// to third-party provider data. Both mirrors hold the same shape, parsed
// here exactly once.

/** Parse a stored star list (array or JSON string of one) into clean ids. */
export function parseStarIds(value: unknown): string[] {
  if (typeof value === 'string') {
    try {
      const parsed: unknown = JSON.parse(value)
      return Array.isArray(parsed)
        ? parsed.filter((v): v is string => typeof v === 'string' && v.length > 0)
        : []
    } catch {
      return []
    }
  }
  if (Array.isArray(value)) {
    return value.filter((v): v is string => typeof v === 'string' && v.length > 0)
  }
  return []
}

/** Serialize a star-id set for either mirror (localStorage or container kv). */
export function serializeStarIds(ids: Iterable<string>): string {
  return JSON.stringify([...new Set(ids)])
}
