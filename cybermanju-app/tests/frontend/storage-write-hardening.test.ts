// CyberManju OS — storage-write-hardening.test (tests/frontend/storage-write-hardening.test.ts).
// Role: guard persisted dashboard probes and ensure every browser volume writer shares one FIFO.
// Exports: none (Vitest suite).
// Integrates: useTauri, useVolumeMirror, volumeWriteQueue, and crates/os-wasm/src/os.rs.
// Contracts: dead URLs are cleared before transport fallback; storage errors are surfaced, not committed to memory.
import fs from 'node:fs'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ensureServerUrlReachable, getServerUrl, setServerUrl } from '@/composables/useTauri'
import { withVolumeWriteQueue, volumeWriteQueue } from '@/utils/volumeWriteQueue'

afterEach(() => {
  setServerUrl('')
  vi.unstubAllGlobals()
})

describe('saved dashboard endpoint validation', () => {
  it('probes once for concurrent callers and accepts the dashboard health identity', async () => {
    let finish!: (response: Response) => void
    const pending = new Promise<Response>((resolve) => { finish = resolve })
    const fetchMock = vi.fn(() => pending)
    vi.stubGlobal('fetch', fetchMock)
    setServerUrl('https://dashboard.example/')

    const first = ensureServerUrlReachable()
    const second = ensureServerUrlReachable()
    expect(fetchMock).toHaveBeenCalledTimes(1)
    finish(new Response(JSON.stringify({ service: 'CyberManju OS Web Dashboard', status: 'degraded' }), {
      status: 200, headers: { 'Content-Type': 'application/json' },
    }))
    await expect(Promise.all([first, second])).resolves.toEqual([true, true])
    expect(getServerUrl()).toBe('https://dashboard.example')
  })

  it('clears an unreachable or unrelated persisted URL for static-host fallback', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ service: 'other' }), { status: 200 })))
    setServerUrl('https://stale.example')
    await expect(ensureServerUrlReachable()).resolves.toBe(false)
    expect(getServerUrl()).toBe('')
  })
})

describe('production volume writer hardening', () => {
  it('serializes result-producing operations on the shared production queue', async () => {
    await volumeWriteQueue.drain()
    const order: string[] = []
    let release!: () => void
    const gate = new Promise<void>((resolve) => { release = resolve })
    const first = withVolumeWriteQueue(0, async () => {
      order.push('first-start')
      await gate
      order.push('first-end')
      return 1
    })
    const second = withVolumeWriteQueue(0, () => {
      order.push('second')
      return 2
    })
    await Promise.resolve()
    expect(order).toEqual(['first-start'])
    release()
    await expect(Promise.all([first, second])).resolves.toEqual([1, 2])
    expect(order).toEqual(['first-start', 'first-end', 'second'])
  })

  it('takes the static delete snapshot and applies fallback inside the queued callback', () => {
    const tauri = fs.readFileSync('src/composables/useTauri.ts', 'utf8')
    const deletion = tauri.match(/deleteVolumePath: async \(path, recursive\) => withVolumeWriteQueue\(0, async \(\) => \{([\s\S]*?)\n  \}\),\n  killTask/)
    expect(deletion).not.toBeNull()
    const body = deletion?.[1] ?? ''
    expect(body.indexOf('const volume = readStaticVolume()')).toBeGreaterThan(-1)
    expect(body).toContain("wasmOsDispatch('rm', {")
    expect(body).toContain('for (const k of keys) delete volume[k]')
    expect(body).toContain('localStorage.setItem(STATIC_VOLUME_KEY, JSON.stringify(volume))')
    expect(body).not.toContain('await withVolumeWriteQueue')
  })

  it('routes Tauri fallback and volume mirror writes through that queue', () => {
    const tauri = fs.readFileSync('src/composables/useTauri.ts', 'utf8')
    const mirror = fs.readFileSync('src/composables/useVolumeMirror.ts', 'utf8')
    expect(tauri).toContain('withVolumeWriteQueue')
    expect(mirror).toContain('withVolumeWriteQueue')
    expect(mirror).toContain('return withVolumeWriteQueue(0, async () =>')
  })

  it('returns localStorage persistence errors through Rust OS mutation envelopes', () => {
    const rust = fs.readFileSync('crates/os-wasm/src/os.rs', 'utf8')
    expect(rust).toMatch(/fn save_volume\([^)]*\) -> Result<\(\), String>/)
    expect(rust).toContain('disk_full: browser storage refused the write')
    expect(rust).toContain('if let Err(e) = save_volume(&volume)')
    expect(rust).toContain('if let Err(e) = save_ui(&updated)')
  })

  it('keeps shared loading active until each overlapping store operation settles', () => {
    const app = fs.readFileSync('src/stores/app.ts', 'utf8')
    expect(app).toContain('let loadingOperations = 0')
    expect(app).toContain('isLoading.value = loadingOperations > 0')
    expect(app).toContain('beginLoading()')
    expect(app).toContain('endLoading()')
  })
})
