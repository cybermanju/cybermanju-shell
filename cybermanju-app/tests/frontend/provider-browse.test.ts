// CyberManju OS — Provider browse.test (tests/frontend/provider-browse.test.ts).
// Provider browse: every Google Drive / GitHub / GitLab repo stays visible
// in the file manager (`/providers`), with folder rows synthesized when the
// native recursive fallback (no isDir) engages, and moves land in the vault
// (.cybermanju home `/`) or straight between providers.
import { describe, expect, it } from 'vitest'
import {
  configIdFromPending,
  isPendingMountId,
  pendingMountId,
  remoteFilesToProviderNodes,
} from '../../src/utils/providerBrowse'

const pathFor = (mountId: string, remotePath: string) =>
  remotePath ? `/providers/${mountId}/${remotePath}` : `/providers/${mountId}`

describe('provider placeholders (all repos visible before mount)', () => {
  it('pending ids round-trip to the config', () => {
    expect(pendingMountId('cfg-1')).toBe('pending-cfg-1')
    expect(isPendingMountId('pending-cfg-1')).toBe(true)
    expect(isPendingMountId('mnt-abc')).toBe(false)
    expect(configIdFromPending('pending-cfg-1')).toBe('cfg-1')
    expect(configIdFromPending('mnt-abc')).toBe('')
  })
})

describe('recursive fallback synthesizes one level of folders', () => {
  it('direct files + implied dirs, dirs first', () => {
    const nodes = remoteFilesToProviderNodes(
      [
        { name: 'a.cyb3', path: 'docs/a.cyb3', sizeBytes: 12, modifiedAt: '', url: '' },
        { name: 'b.cyb3', path: 'docs/nested/b.cyb3', sizeBytes: 34, modifiedAt: '', url: '' },
        { name: 'README.md', path: 'README.md', sizeBytes: 5, modifiedAt: '', url: '' },
      ],
      'mnt-1',
      '/providers/mnt-1',
      pathFor,
    )
    // docs/nested/b.cyb3 folds into docs/ — one level only.
    expect(nodes.map((n) => n.name)).toEqual(['docs', 'README.md'])
    expect(nodes[0].fileType).toBe('folder')
    expect(nodes[1].fileType).toBe('file')
    expect(nodes[0].path).toBe('/providers/mnt-1/docs')
    expect(nodes[0].id).toBe('providers/mnt-1/docs')
  })

  it('drive ids and git paths both render', () => {
    const nodes = remoteFilesToProviderNodes(
      [
        { name: 'vault.cybermanju', path: 'file-1', sizeBytes: 4096, modifiedAt: '', url: '' },
        { name: 'notes.txt', path: 'notes.txt', sizeBytes: 10, modifiedAt: '', url: '' },
      ],
      'mnt-drive',
      '/providers/mnt-drive',
      pathFor,
    )
    expect(nodes).toHaveLength(2)
    expect(nodes.every((n) => n.fileType === 'file')).toBe(true)
  })

  it('empty / missing listings stay empty (no phantom rows)', () => {
    expect(remoteFilesToProviderNodes([], 'm', '/providers/m', pathFor)).toEqual([])
  })
})
