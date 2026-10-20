// CyberManju OS — tests/frontend/transport-first-paint.test.ts.
// Role: see header automation pilot (collect-file-context + AI merge pending full prose).
// Exports/Integrates: documented at the item level; header to be expanded.
import fs from 'node:fs'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { setServerUrl } from '@/composables/useTauri'
import { useTransport } from '@/composables/useTransport'
import { wasmBackendActive } from '@/composables/useWasmBackend'

beforeEach(() => {
  vi.stubEnv('VITE_TRANSPORT', '')
  vi.stubGlobal('window', { __TAURI__: false, location: { port: '' } })
  setServerUrl('')
})

afterEach(() => {
  vi.unstubAllEnvs()
  vi.unstubAllGlobals()
})

describe('transport labels at first paint', () => {
  it('classifies Pages synchronously before the WASM module loads', () => {
    expect(wasmBackendActive()).toBe(false)
    expect(useTransport()).toMatchObject({ backend: 'wasm', short: 'WASM' })
    expect(wasmBackendActive()).toBe(false)
  })

  it('keeps LandingPage, Settings, and shortcut labels on the shared classifier', () => {
    const landing = fs.readFileSync('src/components/LandingPage.vue', 'utf8')
    const settings = fs.readFileSync('src/components/SettingsPage.vue', 'utf8')
    const shortcuts = fs.readFileSync('src/components/ShellShortcutDock.vue', 'utf8')

    expect(landing).toContain("import { useTransport } from '@/composables/useTransport'")
    expect(landing).toContain('const transport = useTransport().label')
    expect(settings).toContain('computed(() => useTransport().label)')
    expect(shortcuts).toContain('computed(() => useTransport().short)')
  })

  it('honors the same explicit transport override before the async load', () => {
    vi.stubEnv('VITE_TRANSPORT', 'rest')
    expect(useTransport()).toMatchObject({ backend: 'rest', short: 'WEB' })
    expect(wasmBackendActive()).toBe(false)
  })
})
