// CyberManju OS — CSV (src/utils/csv.ts).
// Role: round-out daily use, part 3 (ANALYSIS.md §4.9) — cheap CSV/TSV
// dataset view so the agent can then operate on tables. Pure RFC-4180-ish
// parse (quotes, escaped quotes, embedded newlines) + delimiter sniffing.
// Exports: DatasetTable, parseDelimited, sniffDelimiter, inferColumnType.
// Integrates: standalone; ArchivePanel DATA tab parses file content loaded
// through the existing store content actions.
// Contracts: pure, never throws; caps at 5000 rows / 100 columns
// (`truncated: true` beyond); binary/NUL input refused with null.
export interface DatasetTable {
  columns: string[]
  rows: string[][]
  delimiter: string
  truncated: boolean
}

const MAX_ROWS = 5000
const MAX_COLS = 100

/** Sniff `,` vs `\t` vs `;` from the first non-empty line. */
export function sniffDelimiter(text: string): string {
  const first = text.split(/\r?\n/).find((l) => l.trim().length > 0) ?? ''
  const counts = [',', '\t', ';'].map((d) => ({
    d,
    n: first.split(d).length - 1,
  }))
  counts.sort((a, b) => b.n - a.n)
  return counts[0].n > 0 ? counts[0].d : ','
}

/** Parse delimited text into a table; null on NUL/binary input. */
export function parseDelimited(text: string, delimiter?: string): DatasetTable | null {
  if (text.includes('\0')) return null
  const delim = delimiter ?? sniffDelimiter(text)
  const rows: string[][] = []
  let row: string[] = []
  let cell = ''
  let quoted = false
  let truncated = false
  const pushCell = () => {
    row.push(cell)
    cell = ''
  }
  const pushRow = () => {
    // Skip fully-empty trailing rows (common file ending).
    if (!(row.length === 1 && row[0] === '')) rows.push(row.slice(0, MAX_COLS))
    row = []
  }
  for (let i = 0; i < text.length; i++) {
    if (rows.length >= MAX_ROWS) {
      truncated = true
      break
    }
    const c = text[i]
    if (quoted) {
      if (c === '"') {
        if (text[i + 1] === '"') {
          cell += '"'
          i++
        } else {
          quoted = false
        }
      } else {
        cell += c
      }
    } else if (c === '"') {
      quoted = true
    } else if (c === delim) {
      pushCell()
    } else if (c === '\r') {
      continue
    } else if (c === '\n') {
      pushCell()
      pushRow()
    } else {
      cell += c
    }
  }
  if (rows.length < MAX_ROWS) {
    pushCell()
    pushRow()
  }
  if (rows.length === 0) return { columns: [], rows: [], delimiter: delim, truncated }
  return { columns: rows[0], rows: rows.slice(1), delimiter: delim, truncated }
}

/** Column type hint over non-empty values (number > date > text). */
export function inferColumnType(values: string[]): 'number' | 'date' | 'text' {
  const vals = values.filter((v) => v.trim() !== '')
  if (vals.length === 0) return 'text'
  if (vals.every((v) => /^-?\d+(\.\d+)?$/.test(v.trim()))) return 'number'
  if (vals.every((v) => !Number.isNaN(Date.parse(v.trim())))) return 'date'
  return 'text'
}
