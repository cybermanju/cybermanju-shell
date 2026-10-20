// CyberManju OS — crypto REST routing regression tests.
// Role: exercise frontend invoke requests for the shared byte-level pipeline.
// Integrates: useTauri, fetch, Vitest; cipher correctness is covered separately.
// Contracts: no desktop-only rejection or metadata-only fallback on failures.
import { afterEach, describe, expect, it, vi } from 'vitest'
import { invoke, setServerUrl, WRITE_ONLY_COMMANDS } from '@/composables/useTauri'

afterEach(() => {
  setServerUrl('')
  vi.unstubAllGlobals()
})

function dashboardFetch(operationStatus = 200) {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const health = String(input).endsWith('/api/health')
    const body = health
      ? { service: 'CyberManju OS Web Dashboard', status: 'ok' }
      : operationStatus === 200 ? { encrypted: true } : { message: 'integrity: ciphertext is invalid' }
    return new Response(JSON.stringify(body), {
      status: health ? 200 : operationStatus,
      headers: { 'Content-Type': 'application/json' },
    })
  })
  vi.stubGlobal('fetch', fetchMock)
  setServerUrl('http://dashboard.test')
  return fetchMock
}

describe('shared file crypto REST actions', () => {
  it('sends encrypt and decrypt to file-scoped byte handlers', async () => {
    const fetchMock = dashboardFetch()
    await invoke('encrypt_file', { fileId: 'file-1', algorithm: 'kyber1024' })
    expect(fetchMock).toHaveBeenCalledWith('http://dashboard.test/api/files/file-1/encrypt',
      expect.objectContaining({ method: 'POST', body: JSON.stringify({ algorithm: 'kyber1024' }) }))
    await invoke('decrypt_file', { fileId: 'file-1' })
    expect(fetchMock).toHaveBeenCalledWith('http://dashboard.test/api/files/file-1/decrypt',
      expect.objectContaining({ method: 'POST', body: '{}' }))
    expect(WRITE_ONLY_COMMANDS.has('encrypt_file')).toBe(false)
    expect(WRITE_ONLY_COMMANDS.has('decrypt_file')).toBe(false)
  })

  it('supports explicit key provisioning and batch decryption', async () => {
    const fetchMock = dashboardFetch()
    await invoke('generate_keypair', { algorithm: 'hybrid' })
    expect(fetchMock).toHaveBeenCalledWith('http://dashboard.test/api/encryption/keys',
      expect.objectContaining({ method: 'POST', body: JSON.stringify({ algorithm: 'hybrid' }) }))
    await invoke('batch_decrypt', { fileIds: ['a', 'b'] })
    expect(fetchMock).toHaveBeenCalledWith('http://dashboard.test/api/batch/decrypt',
      expect.objectContaining({ method: 'POST', body: JSON.stringify({ fileIds: ['a', 'b'] }) }))
  })

  it('rejects a failed byte operation instead of falling back to metadata mutation', async () => {
    const fetchMock = dashboardFetch(409)
    await expect(invoke('encrypt_file', { fileId: 'file-1', algorithm: 'hybrid' }))
      .rejects.toThrow('integrity: ciphertext is invalid')
    const operations = fetchMock.mock.calls.map(([url]) => String(url)).filter(url => !url.endsWith('/api/health'))
    expect(operations).toEqual(['http://dashboard.test/api/files/file-1/encrypt'])
  })
})
