// CyberManju OS — Sync refresh.test (tests/frontend/sync-refresh.test.ts).
// Refresh-after-sync: terminal-state detection, summary lines, fan-out order.
import { describe, expect, it, vi } from 'vitest'
import {
  firstSyncError,
  isSyncSuccess,
  isSyncTerminal,
  refreshAfterSync,
  syncSummaryLine,
} from '@/utils/syncRefresh'

describe('isSyncTerminal', () => {
  it('matches the poller terminal set', () => {
    for (const s of ['idle', 'done', 'error', 'completed', 'cancelled']) {
      expect(isSyncTerminal(s)).toBe(true)
    }
  })

  it('keeps running states polling', () => {
    for (const s of ['running', 'queued', 'starting', '', null, undefined, 42]) {
      expect(isSyncTerminal(s)).toBe(false)
    }
  })
})

describe('isSyncSuccess', () => {
  it('lands new data on done/completed only', () => {
    expect(isSyncSuccess('done')).toBe(true)
    expect(isSyncSuccess('completed')).toBe(true)
    expect(isSyncSuccess('error')).toBe(false)
    expect(isSyncSuccess('cancelled')).toBe(false)
    expect(isSyncSuccess('idle')).toBe(false)
  })
})

describe('syncSummaryLine', () => {
  it('names file counts for the toast + terminal notice', () => {
    const line = syncSummaryLine({ processedFiles: 12, totalFiles: 12 })
    expect(line).toContain('12/12 files')
    expect(line).toContain('file manager + terminal refreshed')
  })

  it('degrades without counts', () => {
    expect(syncSummaryLine(null)).toContain('sync done')
    expect(syncSummaryLine({})).toContain('sync done')
  })
})

describe('firstSyncError', () => {
  it('returns the first provider error', () => {
    expect(firstSyncError({ errors: ['auth: bad token', 'network: x'] })).toBe('auth: bad token')
    expect(firstSyncError({ errors: [] })).toBe('')
    expect(firstSyncError(null)).toBe('')
  })
})

describe('refreshAfterSync', () => {
  it('fans out to files + df + status + runs and broadcasts', async () => {
    const calls: string[] = []
    const posted: string[] = []
    await refreshAfterSync({
      fetchFiles: async () => { calls.push('files') },
      fetchOsDf: async () => { calls.push('df') },
      fetchSyncStatus: async () => { calls.push('status') },
      fetchSyncRuns: async () => { calls.push('runs') },
      postBroadcast: (m) => { posted.push(m) },
    })
    expect(calls.sort()).toEqual(['df', 'files', 'runs', 'status'])
    expect(posted).toEqual(['cybermanju:files-changed'])
  })

  it('never throws when a fetcher fails', async () => {
    const failing = vi.fn(async () => { throw new Error('nope') })
    await expect(refreshAfterSync({
      fetchFiles: failing,
      fetchOsDf: failing,
      fetchSyncStatus: failing,
      fetchSyncRuns: failing,
    })).resolves.toBeUndefined()
  })
})
