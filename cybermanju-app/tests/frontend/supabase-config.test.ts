// CyberManju OS — Supabase config.test (tests/frontend/supabase-config.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest.
// Contracts: none special (pure logic; errors bubble to caller).
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  clearSupabaseConfig,
  getSupabaseConfig,
  hydrateSupabaseConfig,
  OAUTH_CALLBACK_URL,
  setSupabaseConfig,
  supabaseConfigured,
  supabaseOAuthQueryParams,
  supabaseRedirectTo,
  supabaseScopesFor,
} from '@/composables/useSupabase'

function memoryStorage(): Storage {
  const values = new Map<string, string>()
  return {
    get length() {
      return values.size
    },
    clear() {
      values.clear()
    },
    getItem(key: string) {
      return values.get(String(key)) ?? null
    },
    key(index: number) {
      return [...values.keys()][index] ?? null
    },
    removeItem(key: string) {
      values.delete(String(key))
    },
    setItem(key: string, value: string) {
      values.set(String(key), String(value))
    },
  } as Storage
}

beforeEach(() => {
  vi.stubEnv('VITE_SUPABASE_URL', 'https://broker.example.test/')
  vi.stubEnv('VITE_SUPABASE_ANON_KEY', 'build-public-anon-key')
  vi.stubGlobal('localStorage', memoryStorage())
  clearSupabaseConfig()
})

afterEach(() => {
  vi.unstubAllEnvs()
  vi.unstubAllGlobals()
})

describe('Supabase broker configuration (build pair + Settings paste)', () => {
  it('uses the build-time pair when nothing was pasted', () => {
    expect(getSupabaseConfig()).toEqual({
      url: 'https://broker.example.test',
      key: 'build-public-anon-key',
      source: 'build-env',
    })
    expect(supabaseConfigured()).toBe(true)
  })

  it('prefers the Settings paste over the baked pair', () => {
    expect(setSupabaseConfig('https://manual.example.test/', 'manual-public-anon-key')).toBeNull()
    expect(getSupabaseConfig()).toEqual({
      url: 'https://manual.example.test',
      key: 'manual-public-anon-key',
      source: 'settings',
    })
    expect(supabaseConfigured()).toBe(true)
  })

  it('configures a brokerless build purely from the Settings paste', async () => {
    vi.stubEnv('VITE_SUPABASE_URL', '')
    vi.stubEnv('VITE_SUPABASE_ANON_KEY', '')
    await hydrateSupabaseConfig()
    expect(supabaseConfigured()).toBe(false)
    expect(setSupabaseConfig('https://manual.example.test', 'manual-public-anon-key')).toBeNull()
    expect(getSupabaseConfig().source).toBe('settings')
    expect(supabaseConfigured()).toBe(true)
  })

  it('rejects bad pastes and keeps the previous pair', () => {
    expect(setSupabaseConfig('', '')).toMatch(/paste/i)
    expect(setSupabaseConfig('notaurl', 'manual-public-anon-key')).toMatch(/https/i)
    expect(setSupabaseConfig('https://manual.example.test', 'short')).toMatch(/too short/i)
    // Nothing was written — the baked pair still serves.
    expect(getSupabaseConfig()).toEqual({
      url: 'https://broker.example.test',
      key: 'build-public-anon-key',
      source: 'build-env',
    })
    expect(supabaseConfigured()).toBe(true)
  })

  it('forgetting the paste falls back to the baked pair', () => {
    expect(setSupabaseConfig('https://manual.example.test', 'manual-public-anon-key')).toBeNull()
    clearSupabaseConfig()
    expect(getSupabaseConfig()).toEqual({
      url: 'https://broker.example.test',
      key: 'build-public-anon-key',
      source: 'build-env',
    })
  })

  it('reports an empty source when the build has no broker and nothing was pasted', () => {
    vi.stubEnv('VITE_SUPABASE_URL', '')
    vi.stubEnv('VITE_SUPABASE_ANON_KEY', '')
    clearSupabaseConfig()
    expect(getSupabaseConfig()).toEqual({ url: '', key: '', source: 'none' })
    expect(supabaseConfigured()).toBe(false)
  })

  it('reports a half-filled build pair as unconfigured build-env', async () => {
    vi.stubEnv('VITE_SUPABASE_URL', 'https://broker.example.test')
    vi.stubEnv('VITE_SUPABASE_ANON_KEY', '')
    await hydrateSupabaseConfig()
    expect(getSupabaseConfig().source).toBe('build-env')
    expect(supabaseConfigured()).toBe(false)
  })

  it('honours the legacy VITE_SUPABASE_KEY alias', () => {
    vi.stubEnv('VITE_SUPABASE_URL', 'https://broker.example.test')
    vi.stubEnv('VITE_SUPABASE_ANON_KEY', '')
    vi.stubEnv('VITE_SUPABASE_KEY', 'legacy-public-key')
    expect(getSupabaseConfig()).toEqual({
      url: 'https://broker.example.test',
      key: 'legacy-public-key',
      source: 'build-env',
    })
  })

  it('requests a fresh offline Google consent when reconnecting Drive', () => {
    expect(supabaseScopesFor('googleDrive')).toContain('https://www.googleapis.com/auth/drive.file')
    expect(supabaseOAuthQueryParams('googleDrive', true)).toEqual({ access_type: 'offline', prompt: 'consent' })
    expect(supabaseOAuthQueryParams('google', true)).toEqual({ access_type: 'offline', prompt: 'consent' })
    expect(supabaseOAuthQueryParams('github', true)).toBeUndefined()
    expect(supabaseOAuthQueryParams('googleDrive')).toBeUndefined()
  })
})

describe('OAuth redirect selection (Tauri deep-link vs web popup)', () => {
  const DESKTOP_UA =
    'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36'
  const ANDROID_UA =
    'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Mobile Safari/537.36'

  function stubBrowser(tauri: boolean, ua: string) {
    vi.stubGlobal('window', {
      __TAURI__: tauri || undefined,
      location: { origin: 'https://app.example.test', pathname: '/index.html' },
      opener: null,
      closed: false,
      screen: { width: 1400, height: 900 },
    })
    vi.stubGlobal('navigator', { userAgent: ua, platform: 'Linux', maxTouchPoints: 0 })
  }

  it('uses the https page return on plain web', () => {
    stubBrowser(false, DESKTOP_UA)
    expect(supabaseRedirectTo(true)).toBe('https://app.example.test/index.html?oauth=popup')
    expect(supabaseRedirectTo(false)).toBe('https://app.example.test/index.html')
  })

  it('uses the cybermanju:// callback on Tauri desktop (system browser)', () => {
    // A popup WebView cannot navigate back to a custom scheme — Linux
    // WebKitGTK aborts with "scheme that is not HTTP(S)" — so desktop must
    // never return a tauri:// page URL here.
    stubBrowser(true, DESKTOP_UA)
    expect(supabaseRedirectTo(true)).toBe(OAUTH_CALLBACK_URL)
    expect(supabaseRedirectTo(false)).toBe(OAUTH_CALLBACK_URL)
    expect(supabaseRedirectTo(true)).toBe('cybermanju://oauth/callback')
  })

  it('uses the cybermanju:// callback on Tauri mobile', () => {
    stubBrowser(true, ANDROID_UA)
    expect(supabaseRedirectTo(true)).toBe(OAUTH_CALLBACK_URL)
    expect(supabaseRedirectTo(false)).toBe(OAUTH_CALLBACK_URL)
  })
})
