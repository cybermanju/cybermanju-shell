// CyberManju OS — launcher-center.test (tests/frontend/launcher-center.test.ts).
// Role: pin the reviewed launcher/notification-center fixes (static source
// reads, no cargo): the transport fast-route refuses EVERY `launcher_*`
// command with `unsupported:` off-Tauri (incl. `launcher_open_url` /
// `launcher_open_file`), MessagesHub renders the system-history tab and
// never marks it read on open/tab-switch, the message digest survives an
// open, and gallery/pack icons stage locally until Save.
// Exports: none (vitest suite).
// Integrates: src/composables/useTauri.ts; src-tauri/src/commands/launcher.rs
// (command-name truth, read-only); src/components/MessagesHub.vue,
// src/components/AppDrawer.vue, src/components/MobileLauncher.vue.
// Contracts: node-env safe; string-level presence only (repo test style) —
// live badge behavior is covered by useNotifications' own suite.
import fs from 'node:fs'
import { describe, expect, it } from 'vitest'

const tauri = fs.readFileSync('src/composables/useTauri.ts', 'utf8')
const launcherRs = fs.readFileSync('src-tauri/src/commands/launcher.rs', 'utf8')
const hub = fs.readFileSync('src/components/MessagesHub.vue', 'utf8')
const drawer = fs.readFileSync('src/components/AppDrawer.vue', 'utf8')
const mobile = fs.readFileSync('src/components/MobileLauncher.vue', 'utf8')

/** Slice the Android-launcher fast-route (the `if (cmd === …)` header through
 * the `unsupported:` refusal) out of `invokeInternal`. */
function fastRouteBlock(): string {
  const start = tauri.indexOf("if (cmd === 'launcher_list_apps'")
  expect(start).toBeGreaterThan(-1)
  const marker = tauri.indexOf('unsupported: Android apps need', start)
  expect(marker).toBeGreaterThan(start)
  // Slice THROUGH the refusal so the block includes the `unsupported:` text.
  return tauri.slice(start, marker + 'unsupported:'.length)
}

/** Every launcher command the Rust side actually exposes. */
function rustLauncherCommands(): string[] {
  return [...launcherRs.matchAll(/pub fn (launcher_[a-z_]+)/g)].map((m) => m[1])
}

describe('launcher transport fast-route covers every command (AGENT-1 unsupported:)', () => {
  it('refuses all 12 launcher commands off-Tauri, incl. open_url/open_file', () => {
    const block = fastRouteBlock()
    const commands = rustLauncherCommands()
    // Sanity: the Rust surface is the command list, not a stale copy.
    expect(commands.length).toBeGreaterThanOrEqual(12)
    expect(commands).toContain('launcher_open_url')
    expect(commands).toContain('launcher_open_file')
    for (const cmd of commands) {
      expect(block, `fast-route misses ${cmd}`).toContain(`'${cmd}'`)
    }
    expect(block).toContain('unsupported:')
  })
})

describe('MessagesHub system tab (never wipes the badge on open)', () => {
  it('renders two tabs, the system history rows and its header actions', () => {
    expect(hub).toContain('class="msg-tabs"')
    expect(hub).toContain("@click=\"setTab('messages')\"")
    expect(hub).toContain("@click=\"setTab('system')\"")
    expect(hub).toContain('v-for="h in sysHistory"')
    expect(hub).toContain('@click="readRow(h.id)"')
    expect(hub).toContain('@click="markAll"')
    expect(hub).toContain('@click="clearSystem"')
    // The system tab carries the unread badge and the panel header totals it.
    expect(hub).toMatch(/tab === 'system'[\s\S]{0,400}sysUnread > 0/)
    expect(hub).toContain('totalUnread > 0')
  })

  it('marks all read only from the explicit button, never on open or tab switch', () => {
    // Exactly one call site: inside `markAll()`.
    expect(hub.match(/sys\.markAllRead\(\)/g) ?? []).toHaveLength(1)
    expect(hub).toMatch(/function markAll\(\)\s*\{\s*sys\.markAllRead\(\)/)
    const setTabAt = hub.indexOf('function setTab')
    expect(setTabAt).toBeGreaterThan(-1)
    expect(hub.slice(setTabAt, setTabAt + 240)).not.toContain('markAllRead')
    // The open-watch refreshes data only — no auto-read, no auto markSeen.
    const openAt = hub.indexOf('() => launcher.messagesOpen')
    expect(openAt).toBeGreaterThan(-1)
    const openWatch = hub.slice(openAt, openAt + 700)
    expect(openWatch).not.toContain('markAllRead')
    expect(openWatch).not.toContain('markSeen')
    expect(hub).not.toContain('hub.refresh(true).then(() => hub.markSeen())')
  })

  it('keeps the platform note chats-only (system history renders everywhere)', () => {
    const systemAt = hub.indexOf("tab === 'system'")
    const branchAt = hub.indexOf('<template v-else>', systemAt)
    expect(systemAt).toBeGreaterThan(-1)
    expect(branchAt).toBeGreaterThan(systemAt)
    const systemPane = hub.slice(systemAt, branchAt)
    expect(systemPane).toContain('sysHistory')
    expect(systemPane).not.toContain('hub.supported')
    expect(systemPane).not.toContain('Install options')
  })
})

describe('staged launcher icons commit only in Save (no deep-watch persistence)', () => {
  it('AppDrawer stages gallery + pack icons in a component-local ref', () => {
    expect(drawer).toContain('stagedIcon.value = url')
    // The early-persist calls are gone from both pickers.
    expect(drawer).not.toContain('launcher.setIcon(active.value.key')
    expect(drawer).not.toContain('launcher.setIconPack(active.value.key')
    const saveAt = drawer.indexOf('function saveActive()')
    expect(saveAt).toBeGreaterThan(-1)
    const saveBody = drawer.slice(saveAt, saveAt + 800)
    expect(saveBody).toContain('launcher.setIcon(key, stagedIcon.value)')
    expect(saveBody).toContain('launcher.setIconPack(key, packDraft.value)')
    expect(saveBody).toContain('store.notifySuccess(')
  })

  it('MobileLauncher stages the home gallery icon and refreshes badges on return', () => {
    expect(mobile).toContain('homeIconDraft.value = url')
    expect(mobile).not.toContain('launcher.setIcon(homeKey(app), url)')
    const saveAt = mobile.indexOf('function saveHomeCustom()')
    expect(saveAt).toBeGreaterThan(-1)
    const saveBody = mobile.slice(saveAt, saveAt + 700)
    expect(saveBody).toContain('launcher.setIcon(key, homeIconDraft.value)')
    expect(saveBody).toContain('store.notifySuccess(')
    // Returning to the app must bypass the cached-'ready' early return.
    expect(mobile).toContain('msgHub.refresh(true)')
  })
})
