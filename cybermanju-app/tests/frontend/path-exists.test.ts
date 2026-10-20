// CyberManju OS — tests/frontend/path-exists.test.ts.
// Role: see header automation pilot (collect-file-context + AI merge pending full prose).
// Exports/Integrates: documented at the item level; header to be expanded.
import { afterEach, describe, expect, it, vi } from 'vitest'
import { setServerUrl, useTauri } from '@/composables/useTauri'
import type { FileNode } from '@/types'

function fileNode(path: string): FileNode {
  return {
    id: 'file-1',
    name: path.split('/').at(-1) ?? '',
    fileType: 'file',
    sizeBytes: 12,
    encrypted: false,
    compressionLayers: [],
    createdAt: '2026-01-01T00:00:00Z',
    modifiedAt: '2026-01-01T00:00:00Z',
    path,
  }
}

function stubFileList(files: FileNode[]) {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const body = String(input).endsWith('/api/health')
      ? { service: 'CyberManju OS Web Dashboard', status: 'ok' }
      : files
    return new Response(JSON.stringify(body), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    })
  })
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

afterEach(() => {
  vi.unstubAllGlobals()
  setServerUrl('')
})

describe('pathExists in browser transports', () => {
  it('returns true only for a listed logical file path', async () => {
    setServerUrl('http://dashboard.test')
    const fetchMock = stubFileList([fileNode('/docs/guide.md')])

    await expect(useTauri().pathExists('/docs/guide.md')).resolves.toBe(true)
    expect(fetchMock).toHaveBeenCalledWith(
      'http://dashboard.test/api/files',
      expect.objectContaining({ method: 'GET' }),
    )
  })

  it('returns false when the list endpoint is reachable but the path is absent', async () => {
    setServerUrl('http://dashboard.test')
    stubFileList([fileNode('/docs/guide.md')])

    await expect(useTauri().pathExists('/missing.md')).resolves.toBe(false)
  })
})
