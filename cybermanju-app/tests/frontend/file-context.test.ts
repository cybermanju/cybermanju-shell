// CyberManju OS — File context collector test (tests/frontend/file-context.test.ts).
// Role: pin the heuristic mirror's additive multi-line doc hoisting + attachment rules.
// Exports: doc-join, blank-break, plain-comment, attribute-passthrough cases.
// Integrates: scripts/collect-file-context.mjs (JS fast path of cybermanju-types::code).
// Contracts: consecutive doc lines join with a space (≤400 chars); blank lines and
//   non-def code reset pending docs; plain `//` never hoists; attributes keep docs.
import { describe, expect, it } from 'vitest'
import { collectFileContext } from '../../scripts/collect-file-context.mjs'

const docsOf = (file: string, text: string, name: string): string | undefined =>
  collectFileContext(file, text).symbols.find(s => s.name === name)?.docs

describe('collectFileContext doc hoisting', () => {
  // NOTE: inputs start with a code line because leading comments parse as
  // the file header (which never attaches to symbols) — same as real files.
  it('joins consecutive doc lines into one summary', () => {
    const docs = docsOf(
      'x.rs',
      'use foo;\n/// First line.\n/// Second line.\npub fn run() {}\n',
      'run',
    )
    expect(docs).toBe('First line. Second line.')
  })

  it('breaks attachment on blank lines', () => {
    const docs = docsOf('x.rs', 'use foo;\n/// Orphan.\n\npub fn free() {}\n', 'free')
    expect(docs).toBe('')
  })

  it('never hoists plain narration', () => {
    const docs = docsOf('x.rs', 'use foo;\n// plain narration\npub fn plain() {}\n', 'plain')
    expect(docs).toBe('')
  })

  it('keeps docs across attributes but resets on code', () => {
    const ctx = collectFileContext(
      'x.rs',
      'use foo;\n/// Kept.\n#[tauri::command]\npub fn kept() {}\n/// Dropped.\nlet x = 1;\npub fn next() {}\n',
    )
    // `let` is not a Rust definition keyword, so the middle line resets docs.
    expect(ctx.symbols.find(s => s.name === 'kept')?.docs).toBe('Kept.')
    expect(ctx.symbols.find(s => s.name === 'next')?.docs).toBe('')
  })

  it('joins JSDoc blocks across the closer line', () => {
    const docs = docsOf(
      'x.ts',
      "import y from 'y';\n/** First.\n* Second.\n*/\nexport function go() {}\n",
      'go',
    )
    expect(docs).toBe('First. Second.')
  })

  it('slides the window over narration and attributes', () => {
    const docs = docsOf(
      'x.rs',
      'use foo;\n/// Creates a vault repo.\n/// Returns the stored name.\n// Flat arity is the IPC contract.\n#[allow(clippy::too_many_arguments)]\n#[tauri::command(async)]\npub fn make() {}\n',
      'make',
    )
    expect(docs).toBe('Creates a vault repo. Returns the stored name.')
  })
})
