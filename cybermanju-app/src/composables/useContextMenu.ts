// CyberManju OS — UseContextMenu (src/composables/useContextMenu.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: ContextMenuEntry, useContextMenu.
// Integrates: vue.
// Contracts: none special (pure logic; errors bubble to caller).
import { reactive, readonly, type ComponentPublicInstance } from 'vue'

export interface ContextMenuEntry {
  id: string
  label: string
  icon?: string
  shortcut?: string
  disabled?: boolean
  divider?: boolean
  submenu?: ContextMenuEntry[]
  action?: (data: any) => void
}

interface ContextMenuState {
  visible: boolean
  x: number
  y: number
  items: ContextMenuEntry[]
  contextData: any
  activeComponentId: string | null
  focusIndex: number
  submenuStack: string[]
}

interface RegisteredContext {
  componentId: string
  entries: ContextMenuEntry[]
  priority: number
}

const state = reactive<ContextMenuState>({
  visible: false,
  x: 0,
  y: 0,
  items: [],
  contextData: null,
  activeComponentId: null,
  focusIndex: -1,
  submenuStack: [],
})

const registry = new Map<string, RegisteredContext>()

function registerContext(
  componentId: string,
  entries: ContextMenuEntry[],
  priority = 0
) {
  registry.set(componentId, { componentId, entries, priority })
}

function unregisterContext(componentId: string) {
  registry.delete(componentId)
}

function getComponentEntries(componentId: string): ContextMenuEntry[] {
  return registry.get(componentId)?.entries || []
}

function open(
  event: MouseEvent | TouchEvent,
  componentId: string,
  contextData?: any
) {
  const clientX = 'touches' in event ? event.touches[0].clientX : event.clientX
  const clientY = 'touches' in event ? event.touches[0].clientY : event.clientY

  const entries = getComponentEntries(componentId)
  if (entries.length === 0) return false

  state.visible = true
  state.x = clientX
  state.y = clientY
  state.items = entries
  state.contextData = contextData ?? null
  state.activeComponentId = componentId
  state.focusIndex = -1
  state.submenuStack = []

  event.preventDefault()
  event.stopPropagation()
  return true
}

function close() {
  state.visible = false
  state.items = []
  state.contextData = null
  state.activeComponentId = null
  state.focusIndex = -1
  state.submenuStack = []
}

function appendEntries(
  componentId: string,
  entries: ContextMenuEntry[]
) {
  const existing = registry.get(componentId)
  if (existing) {
    existing.entries.push(...entries)
  } else {
    registerContext(componentId, entries)
  }
}

function replaceEntries(
  componentId: string,
  entries: ContextMenuEntry[]
) {
  const existing = registry.get(componentId)
  if (existing) {
    existing.entries = entries
  } else {
    registerContext(componentId, entries)
  }
}

function resetRegistry() {
  registry.clear()
}

function triggerAction(entry: ContextMenuEntry) {
  if (entry.disabled || entry.divider || entry.submenu) return
  if (entry.action) {
    entry.action(state.contextData)
  }
  close()
}

export function useContextMenu() {
  return {
    state: readonly(state) as typeof state,
    registerContext,
    unregisterContext,
    getComponentEntries,
    open,
    close,
    appendEntries,
    replaceEntries,
    resetRegistry,
    triggerAction,
  }
}

export type { ContextMenuState, RegisteredContext }
