// CyberManju OS — Shells (src/ui/shells.ts).
/**
 * Shell-style registry — structure switches here, tokens switch in tokens.ts.
 *
 * - `macos`: TopMenuBar (24px) + floating Dock + ShellShortcutDock.
 * - `plasma`: PlasmaPanel (44px bottom panel with launcher, pager, task
 *   manager, tray, clock) + KickoffMenu. TopMenuBar/Dock stay mounted but
 *   hidden via `[data-ui-shell]` CSS so global shortcuts and menus keep
 *   working in both styles.
 *
 * Panel logic, resolvePanel(), transport awareness and async-panel loaders
 * stay shared — this registry only picks chrome components and insets.
 */
import type { Component } from 'vue'
import { defineAsyncComponent } from 'vue'
import TopMenuBar from '@/components/TopMenuBar.vue'
import Dock from '@/components/Dock.vue'
import ShellShortcutDock from '@/components/ShellShortcutDock.vue'
import type { ShellStyle } from '@/ui/tokens'

const PlasmaPanel = defineAsyncComponent(() => import('@/components/PlasmaPanel.vue'))
const KickoffMenu = defineAsyncComponent(() => import('@/components/KickoffMenu.vue'))

export interface ShellChrome {
  /** Top chrome (menu bar). Null when the shell has no menu bar. */
  topBar: Component | null
  /** Bottom chrome (dock or panel). */
  bottom: Component | null
  /** Shortcut-dock visibility (macOS layout helpers). */
  shortcutDock: Component | null
  /** Launcher popup (Kickoff in plasma). */
  launcher: Component | null
}

export const SHELL_CHROME: Record<ShellStyle, ShellChrome> = {
  macos: {
    topBar: TopMenuBar,
    bottom: Dock,
    shortcutDock: ShellShortcutDock,
    launcher: null,
  },
  plasma: {
    topBar: null,
    bottom: PlasmaPanel,
    shortcutDock: null,
    launcher: KickoffMenu,
  },
}

/** Layout insets owned by each shell (px, before density scaling). */
export const SHELL_INSETS: Record<ShellStyle, { top: number; bottom: number }> = {
  macos: { top: 24, bottom: 0 },
  plasma: { top: 0, bottom: 44 },
}
