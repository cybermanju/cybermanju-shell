// CyberManju OS — tests/frontend/wasm-load-retry.test.ts.
// Role: see header automation pilot (collect-file-context + AI merge pending full prose).
// Exports/Integrates: documented at the item level; header to be expanded.
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { wasmInit } = vi.hoisted(() => ({ wasmInit: vi.fn() }))

vi.mock(
  'cybermanju-os-wasm',
  () => ({
    default: wasmInit,
    os_dispatch: () => JSON.stringify({ ok: true, output: 'ready' }),
  }),
  { virtual: true },
)

import { wasmModuleExports } from '@/composables/useWasmBackend'

describe('WASM module initialization retry', () => {
  beforeEach(() => {
    wasmInit.mockReset()
  })

  it('retries after a transient initialization failure, then reuses the loaded module', async () => {
    wasmInit
      .mockRejectedValueOnce(new Error('temporary initialization failure'))
      .mockResolvedValueOnce(undefined)

    await expect(wasmModuleExports()).rejects.toThrow('temporary initialization failure')

    const firstSuccess = await wasmModuleExports<{ os_dispatch: () => string }>()
    const secondSuccess = await wasmModuleExports<{ os_dispatch: () => string }>()

    expect(firstSuccess.os_dispatch()).toContain('ready')
    expect(secondSuccess).toBe(firstSuccess)
    expect(wasmInit).toHaveBeenCalledTimes(2)
  })
})
