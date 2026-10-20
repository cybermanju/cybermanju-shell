// CyberManju OS — Tasks (src/utils/tasks.ts).
// Role: round-out daily use, part 1 (ANALYSIS.md §4.9) — tasks/todos the agent
// can add/complete. Pure list ops + localStorage codec; the agent bridge is a
// thin follow-up (same shape as `memory_remember`), the panel works today.
// Exports: TodoItem, addTodo, toggleTodo, removeTodo, clearDone,
// parseTodosStore, serializeTodosStore, openTodos, doneTodos.
// Integrates: standalone (localStorage `cybermanju.todos.v1`); ArchivePanel
// TASKS tab; `task` cybsh verb in staticCybsh.ts.
// Contracts: pure, never throws; ids unique within the list; titles trim to
// 500 chars; garbage store parses to [].
export interface TodoItem {
  id: string
  title: string
  done: boolean
  createdAt: string
  doneAt?: string
  source: 'local' | 'agent'
}

const MAX_TITLE = 500

function uid(existing: Set<string>): string {
  let id = ''
  do {
    id = `t-${Date.now().toString(36)}-${Math.floor(Math.random() * 0xffffff).toString(36)}`
  } while (existing.has(id))
  return id
}

/** Append a todo (blank titles refused → null, caller keeps the list). */
export function addTodo(
  list: TodoItem[],
  title: string,
  opts?: { id?: string; source?: 'local' | 'agent'; createdAt?: string },
): { list: TodoItem[]; item: TodoItem } | null {
  const clean = title.trim().slice(0, MAX_TITLE)
  if (!clean) return null
  const existing = new Set(list.map((t) => t.id))
  let id = opts?.id ?? ''
  if (!id || existing.has(id)) id = uid(existing)
  const item: TodoItem = {
    id,
    title: clean,
    done: false,
    createdAt: opts?.createdAt ?? new Date().toISOString(),
    source: opts?.source ?? 'local',
  }
  return { list: [...list, item], item }
}

export function toggleTodo(list: TodoItem[], id: string): TodoItem[] {
  return list.map((t) =>
    t.id === id
      ? { ...t, done: !t.done, doneAt: !t.done ? new Date().toISOString() : undefined }
      : t,
  )
}

export function removeTodo(list: TodoItem[], id: string): TodoItem[] {
  return list.filter((t) => t.id !== id)
}

/** Drop completed items; returns the pruned list + how many left. */
export function clearDone(list: TodoItem[]): { list: TodoItem[]; cleared: number } {
  const kept = list.filter((t) => !t.done)
  return { list: kept, cleared: list.length - kept.length }
}

export function openTodos(list: TodoItem[]): TodoItem[] {
  return list.filter((t) => !t.done)
}

export function doneTodos(list: TodoItem[]): TodoItem[] {
  return list.filter((t) => t.done)
}

export function parseTodosStore(value: unknown): TodoItem[] {
  let raw: unknown = value
  if (typeof raw === 'string') {
    try {
      raw = JSON.parse(raw)
    } catch {
      return []
    }
  }
  if (!Array.isArray(raw)) return []
  return raw.filter((e): e is TodoItem => {
    return (
      !!e && typeof e === 'object' &&
      typeof (e as TodoItem).id === 'string' &&
      typeof (e as TodoItem).title === 'string' &&
      typeof (e as TodoItem).done === 'boolean'
    )
  })
}

export function serializeTodosStore(list: TodoItem[]): string {
  return JSON.stringify(list)
}
