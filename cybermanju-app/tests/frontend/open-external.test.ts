// CyberManju OS — External opener.test (tests/frontend/open-external.test.ts).
// Role: Vitest frontend suite — pins the runtime routing that replaced the
//   Android-breaking `plugin-shell open` calls (Google/GitHub/GitLab sign-in,
//   provider Connect, `OPEN WITH SYSTEM`, Help → Documentation).
// Exports: module root (see items below).
// Integrates: ../../src/utils/openExternal.ts (mocked useTauri + plugin-shell).
// Contracts: Android must always reach `launcher_open_url` / `launcher_open_file`,
//   desktop must keep the shell opener, and plain tabs must never shell out.
import fs from 'node:fs'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  android: false,
  tauri: false,
  invocations: [] as Array<[string, Record<string, unknown> | undefined]>,
  shellOpen: vi.fn((_path: string) => Promise.resolve()),
}))

vi.mock('@/composables/useTauri', () => ({
  isAndroidApp: () => mocks.android,
  isTauri: () => mocks.tauri,
  invoke: (cmd: string, args?: Record<string, unknown>) => {
    mocks.invocations.push([cmd, args])
    return Promise.resolve(null)
  },
}))

vi.mock('@tauri-apps/plugin-shell', () => ({ open: mocks.shellOpen }))

import { externalOpenRoute, openExternalUrl, openLocalFile } from '@/utils/openExternal'

beforeEach(() => {
  mocks.android = false
  mocks.tauri = false
  mocks.invocations.length = 0
  mocks.shellOpen.mockClear()
})

describe('externalOpenRoute', () => {
  it('prefers the Android Intent bridge, then the shell opener, then a tab', () => {
    expect(externalOpenRoute({ android: true, tauri: true })).toBe('android-intent')
    expect(externalOpenRoute({ android: true, tauri: false })).toBe('android-intent')
    expect(externalOpenRoute({ android: false, tauri: true })).toBe('shell')
    expect(externalOpenRoute({ android: false, tauri: false })).toBe('browser-tab')
  })
})

describe('openExternalUrl', () => {
  it('routes Android links through launcher_open_url, never the shell opener', async () => {
    mocks.android = true
    mocks.tauri = true
    const url = 'https://accounts.google.com/o/oauth2/v2/auth?x=1'
    await openExternalUrl(url)
    expect(mocks.invocations).toEqual([['launcher_open_url', { url }]])
    expect(mocks.shellOpen).not.toHaveBeenCalled()
  })

  it('keeps plugin-shell open for desktop Tauri builds', async () => {
    mocks.tauri = true
    const url = 'cybermanju://oauth/callback?code=s'
    await openExternalUrl(url)
    expect(mocks.shellOpen).toHaveBeenCalledWith(url)
    expect(mocks.invocations).toEqual([])
  })

  it('tabs out in a plain browser with noopener', async () => {
    const open = vi.fn(() => null)
    vi.stubGlobal('window', { open })
    try {
      await openExternalUrl('https://github.com/cybermanju/cybermanju.github.io')
    } finally {
      vi.unstubAllGlobals()
    }
    expect(open).toHaveBeenCalledWith(
      'https://github.com/cybermanju/cybermanju.github.io',
      '_blank',
      'noopener,noreferrer',
    )
    expect(mocks.shellOpen).not.toHaveBeenCalled()
    expect(mocks.invocations).toEqual([])
  })

  it('refuses an empty target on every runtime', async () => {
    await expect(openExternalUrl('   ')).rejects.toThrow(/unsupported: nothing to open/)
    await expect(openExternalUrl(undefined as unknown as string)).rejects.toThrow(/nothing to open/)
    expect(mocks.invocations).toEqual([])
    expect(mocks.shellOpen).not.toHaveBeenCalled()
  })
})

describe('openLocalFile', () => {
  it('routes Android files through launcher_open_file (Intent + FileProvider)', async () => {
    mocks.android = true
    mocks.tauri = true
    await openLocalFile('/sdcard/Download/report.pdf')
    expect(mocks.invocations).toEqual([
      ['launcher_open_file', { path: '/sdcard/Download/report.pdf' }],
    ])
    expect(mocks.shellOpen).not.toHaveBeenCalled()
  })

  it('keeps plugin-shell open for desktop Tauri builds', async () => {
    mocks.tauri = true
    await openLocalFile('/home/me/notes.txt')
    expect(mocks.shellOpen).toHaveBeenCalledWith('/home/me/notes.txt')
    expect(mocks.invocations).toEqual([])
  })

  it('refuses honestly in a browser (the File Manager downloads instead)', async () => {
    await expect(openLocalFile('/host/me/notes.txt')).rejects.toThrow(
      /unsupported: opening a device file needs the native app/,
    )
    expect(mocks.shellOpen).not.toHaveBeenCalled()
  })
})

describe('no call site shells out directly', () => {
  it('imports @tauri-apps/plugin-shell only from the shared opener', () => {
    const hits: string[] = []
    const walk = (dir: string) => {
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const full = `${dir}/${entry.name}`
        if (entry.isDirectory()) walk(full)
        else if (/\.(ts|vue)$/.test(entry.name)) {
          const src = fs.readFileSync(full, 'utf8')
          if (src.includes("'@tauri-apps/plugin-shell'")) hits.push(full)
        }
      }
    }
    walk('src')
    expect(hits).toEqual(['src/utils/openExternal.ts'])
  })

  it('wires the OAuth + file + help flows to the shared opener', () => {
    const supabase = fs.readFileSync('src/composables/useSupabase.ts', 'utf8')
    expect(supabase).toContain('await openExternalUrl(url)')
    const accounts = fs.readFileSync('src/components/AccountManagerPanel.vue', 'utf8')
    expect(accounts).toContain('await openExternalUrl(url)')
    const files = fs.readFileSync('src/components/FileManager.vue', 'utf8')
    expect(files).toContain('await openLocalFile(abs)')
    const help = fs.readFileSync('src/components/TopMenuBar.vue', 'utf8')
    expect(help).toContain('openExternalUrl(')
    for (const src of [supabase, accounts, files, help]) {
      expect(src).not.toContain("await import('@tauri-apps/plugin-shell')")
    }
  })
})
