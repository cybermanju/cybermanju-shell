// CyberManju OS — Tasks+Bookmarks.test (tests/frontend/tasks-bookmarks.test.ts).
// ANALYSIS.md §4.9: todos the agent can add/complete + bookmarks via
// import_from_url.
import { describe, expect, it } from 'vitest'
import {
  addBookmark,
  isHttpUrl,
  markImported,
  parseBookmarksStore,
  removeBookmark,
  serializeBookmarksStore,
} from '../../src/utils/bookmarks'
import {
  addTodo,
  clearDone,
  doneTodos,
  openTodos,
  parseTodosStore,
  removeTodo,
  serializeTodosStore,
  toggleTodo,
} from '../../src/utils/tasks'

describe('todos', () => {
  it('adds, toggles, removes, clears', () => {
    const added = addTodo([], '  Buy milk ', { id: 't1' })
    expect(added?.item.title).toBe('Buy milk')
    let list = added!.list
    expect(addTodo(list, '   ')).toBeNull()
    list = toggleTodo(list, 't1')
    expect(openTodos(list)).toHaveLength(0)
    expect(doneTodos(list)).toHaveLength(1)
    expect(clearDone(list)).toEqual({ list: [], cleared: 1 })
    expect(removeTodo([{ ...added!.item }], 't1')).toEqual([])
  })

  it('round-trips the codec and rejects garbage', () => {
    const list = addTodo([], 'x', { id: 't1' })!.list
    expect(parseTodosStore(serializeTodosStore(list))).toHaveLength(1)
    expect(parseTodosStore('junk')).toEqual([])
  })
})

describe('bookmarks', () => {
  it('accepts http(s) only and re-titles dups', () => {
    expect(isHttpUrl('ftp://x')).toBe(false)
    const first = addBookmark([], 'https://example.com', 'Ex')!
    const dup = addBookmark(first.list, 'https://example.com', 'Ex2')!
    expect(dup.list).toHaveLength(1)
    expect(dup.item.title).toBe('Ex2')
    expect(addBookmark([], 'not a url')).toBeNull()
  })

  it('marks imports and removes', () => {
    const { list } = addBookmark([], 'https://example.com')!
    const id = list[0].id
    expect(markImported(list, id, 'f1')[0].importedFileId).toBe('f1')
    expect(removeBookmark(list, id)).toEqual([])
    expect(parseBookmarksStore(serializeBookmarksStore(list))).toHaveLength(1)
    expect(parseBookmarksStore('junk')).toEqual([])
  })
})
