// CyberManju OS — ShortcutsKey (src/composables/shortcutsKey.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: ShortcutsAPI, ShortcutsKey.
// Integrates: vue, @/….
// Contracts: none special (pure logic; errors bubble to caller).
import type { InjectionKey } from 'vue'
import type { ShortcutGroup } from '@/composables/useShortcuts'

export interface ShortcutsAPI {
  getShortcut(action: string): string
  getPrimary(action: string): string
  getFallback(action: string): string
  isBlocked(action: string): boolean
  getAllShortcuts(): Array<{ action: string; keys: string; primary: string; fallback: string; blockedInBrowser: boolean; group: ShortcutGroup; description: string }>
  getComponentActions(componentId: string): string[]
  getContextActions(fileType: string): string[]
  pause(): void
  resume(): void
}

export const ShortcutsKey: InjectionKey<ShortcutsAPI> = Symbol('shortcuts')
