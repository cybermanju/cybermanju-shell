// CyberManju OS — Smoke.test (tests/frontend/smoke.test.ts).
// Frontend smoke test (AGENT-4 item 19) — proves the test harness runs and
// that the transport helpers behave outside a browser/Tauri shell. Real
// component/behaviour tests are AGENT-5's to add.
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  getAuthToken,
  getServerUrl,
  isTauri,
  isWebMode,
  setAuthToken,
  setServerUrl,
} from '../../src/composables/useTauri'

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('transport helpers', () => {
  it('runs in web mode outside the Tauri shell', () => {
    expect(isTauri()).toBe(false)
    expect(isWebMode()).toBe(true)
  })

  it('does not treat the false __TAURI__ build sentinel as native', () => {
    vi.stubGlobal('window', { __TAURI__: false })
    expect(isTauri()).toBe(false)
    expect(isWebMode()).toBe(true)
  })

  it('recognizes a truthy Tauri bridge', () => {
    vi.stubGlobal('window', { __TAURI__: {} })
    expect(isTauri()).toBe(true)
    expect(isWebMode()).toBe(false)
  })

  it('strips trailing slashes from the server URL', () => {
    setServerUrl('http://localhost:3456///')
    expect(getServerUrl()).toBe('http://localhost:3456')
  })

  it('accepts then clears the auth token', () => {
    setAuthToken('token-123')
    expect(getAuthToken()).toBe('token-123')
    setAuthToken('')
    expect(getAuthToken()).toBe('')
  })
})
