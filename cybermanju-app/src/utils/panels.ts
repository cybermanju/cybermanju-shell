// CyberManju OS — merged-window aliases (pure, unit-tested).
//
// Several historic panel ids render the same window (code→editor,
// storage→disks, …). Aliases resolve in `useWindowManager.open()` so there
// is ever ONE live window per surface; an optional per-alias tab prop
// steers the merged window to the right tab.

import type { PanelType } from '@/types'

/** Historic id → canonical window id. Targets must never be aliases. */
export const PANEL_ALIASES: Partial<Record<PanelType, PanelType>> = {
  code: 'editor',
  webdash: 'dashboard',
  storage: 'disks',
  compression: 'encryption',
  users: 'accounts',
  // Merged omni-search: FACES + COLLECTIONS + SEARCH are one window now —
  // the old ids (and the organize-tab aliases) resolve to `search` so every
  // shortcut, palette entry and menu item lands on the unified surface.
  faces: 'search',
  collections: 'search',
  favorites: 'search',
  'loose-groups': 'search',
  style: 'search',
  preview: 'files',
  cron: 'processes',
  automation: 'processes',
  schedules: 'processes',
  passwords: 'secrets',
  credentials: 'secrets',
  // NOTE: `vault` intentionally NOT an alias — AccountManager already uses
  // "Vault file" as a tab label; aliasing would collide in the palette.
}

/** Props injected when opening via an alias (usually the initial tab). */
export const ALIAS_TAB_PROPS: Partial<Record<PanelType, Record<string, unknown>>> = {
  webdash: {},
  storage: { tab: 'overview' },
  compression: { tab: 'compress' },
  users: { tab: 'users' },
  // Omni-search aliases carry no tab props — the unified window has a
  // single input, not tabs. Keys stay absent so the alias test
  // (`only carries tab props for aliased ids`) is unaffected either way.
  preview: { inspector: true, inspTab: 'info' },
  cron: { tab: 'schedules' },
  automation: { tab: 'schedules' },
  schedules: { tab: 'schedules' },
  passwords: {},
  credentials: {},
}

/** Canonical window id for a panel id (identity when not aliased). */
export function resolvePanel(panelType: PanelType): PanelType {
  return PANEL_ALIASES[panelType] ?? panelType
}

/** Prefix for skill-shipped panels (ANALYSIS.md §4.8 panels-later). */
export const SKILL_PANEL_PREFIX = 'skill:'

/** `true` for `skill:<slug>` addresses (never a canonical window id). */
export function isSkillPanel(address: string): boolean {
  return address.startsWith(SKILL_PANEL_PREFIX) && address.length > SKILL_PANEL_PREFIX.length
}

/** Resolve a `skill:<slug>` address to its skill slug ('' when not one). */
export function skillSlugForPanel(address: string): string {
  if (!isSkillPanel(address)) return ''
  return address.slice(SKILL_PANEL_PREFIX.length).trim()
}
