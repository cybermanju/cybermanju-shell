// CyberManju OS — UseKeyboardShortcuts (src/composables/useKeyboardShortcuts.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: useKeyboardShortcuts, getGlobalShortcuts.
// Integrates: vue, @/….
// Contracts: none special (pure logic; errors bubble to caller).
import { onMounted, onUnmounted } from 'vue'
import { useAppStore } from '@/stores/app'
import { useWindowManager } from '@/composables/useWindowManager'
import { isEditableTarget } from '@/utils/dom'

type ShortcutHandler = () => void

interface Shortcut {
  key: string
  ctrl?: boolean
  shift?: boolean
  meta?: boolean
  handler: ShortcutHandler
  description: string
}

export function useKeyboardShortcuts(shortcuts: Shortcut[]) {
  function handleKeydown(e: KeyboardEvent) {
    if (!e.ctrlKey && !e.metaKey && isEditableTarget(e.target)) return
    const store = useAppStore()

    for (const s of shortcuts) {
      const matchKey = e.key.toLowerCase() === s.key.toLowerCase()
      const matchCtrl = s.ctrl ? (e.ctrlKey || e.metaKey) : !e.ctrlKey && !e.metaKey
      const matchShift = s.shift ? e.shiftKey : !e.shiftKey
      const matchMeta = s.meta ? e.metaKey : true

      if (matchKey && matchCtrl && matchShift && matchMeta) {
        e.preventDefault()
        e.stopPropagation()
        s.handler()
        return
      }
    }
  }

  onMounted(() => window.addEventListener('keydown', handleKeydown))
  onUnmounted(() => window.removeEventListener('keydown', handleKeydown))
}

export function getGlobalShortcuts(store: ReturnType<typeof useAppStore>): Shortcut[] {
  return [
    {
      key: 'k', ctrl: true, handler: () => { store.commandPaletteOpen = !store.commandPaletteOpen },
      description: 'Toggle command palette',
    },
    {
      key: 'f', ctrl: true, handler: () => { const el = document.querySelector<HTMLInputElement>('.search-input'); el?.focus() },
      description: 'Focus search bar',
    },
    {
      key: 'Escape', handler: () => {
        if (store.commandPaletteOpen) { store.commandPaletteOpen = false; return }
        if (store.selectedFileId) { store.selectedFileId = null; return }
      },
      description: 'Close panel / deselect file',
    },
    {
      key: 'n', ctrl: true, handler: () => { store.createFolderPromptOpen = !store.createFolderPromptOpen },
      description: 'Create new folder',
    },
    {
      key: 'Delete', handler: () => {
        if (store.selectedFileId) store.deleteFile(store.selectedFileId)
      },
      description: 'Delete selected file',
    },
    {
      key: 'e', ctrl: true, handler: () => { useWindowManager().open('encryption', { tab: 'shield' }) },
      description: 'Open file shield',
    },
    {
      key: 'c', ctrl: true, shift: true, handler: () => { useWindowManager().open('encryption', { tab: 'compress' }) },
      description: 'Open compression engine',
    },
    {
      key: 'b', ctrl: true, handler: () => { store.sidebarCollapsed = !store.sidebarCollapsed },
      description: 'Toggle sidebar',
    },
    {
      key: 'g', ctrl: true, handler: () => { store.viewMode = 'grid' },
      description: 'Grid view',
    },
    {
      key: 'l', ctrl: true, handler: () => { store.viewMode = 'list' },
      description: 'List view',
    },
    {
      key: 'm', ctrl: true, handler: () => { store.viewMode = store.viewMode === 'masonry' ? 'grid' : 'masonry' },
      description: 'Toggle masonry view',
    },
    {
      key: '?', handler: () => { store.showShortcutsHelp = !store.showShortcutsHelp },
      description: 'Show keyboard shortcuts',
    },
  ]
}
