// CyberManju OS — refresh-after-sync (pure, unit-tested).
//
// A provider sync run mutates what the file manager lists and what the
// terminal sees (`ls`, `df`, task tables). The poller used to go quiet on
// the terminal state, so every view stayed stale until a manual refresh.
// `refreshAfterSync` is the single fan-out every sync-completion path calls:
// file manager + volume `df` + sync status/runs re-fetch, sibling tabs are
// told via the `cybermanju-os` broadcast bus (App.vue already listens for
// `cybermanju:files-changed`), and the returned summary line is what the
// terminal prints as a `sys` notice.

export const SYNC_TERMINAL_STATES = ['idle', 'done', 'error', 'completed', 'cancelled'] as const

export type SyncTerminalState = (typeof SYNC_TERMINAL_STATES)[number]

/** True once the run will not produce more progress events. */
export function isSyncTerminal(status: unknown): boolean {
  return typeof status === 'string' && (SYNC_TERMINAL_STATES as readonly string[]).includes(status)
}

/** True for the terminal states that mean "new data landed". */
export function isSyncSuccess(status: unknown): boolean {
  return status === 'done' || status === 'completed'
}

export interface SyncProgressLike {
  status?: string | null
  processedFiles?: number | null
  totalFiles?: number | null
  errors?: string[] | null
}

/** One line for the toast + terminal `sys` notice on a finished run. */
export function syncSummaryLine(progress: SyncProgressLike | null | undefined): string {
  const done = Number(progress?.processedFiles ?? 0) || 0
  const total = Number(progress?.totalFiles ?? 0) || 0
  const files = total > 0 ? `${done}/${total} files` : `${done} files`
  return `sync done — ${files} · file manager + terminal refreshed`
}

/** First provider error for the failure toast (empty when there is none). */
export function firstSyncError(progress: SyncProgressLike | null | undefined): string {
  const errs = Array.isArray(progress?.errors) ? progress.errors : []
  return errs.length ? String(errs[0]) : ''
}

export interface SyncRefreshDeps {
  fetchFiles: () => Promise<unknown>
  fetchOsDf: () => Promise<unknown>
  fetchSyncStatus: () => Promise<unknown>
  fetchSyncRuns: () => Promise<unknown>
  /** Same-tab views already re-fetch above; this reaches sibling tabs. */
  postBroadcast?: (message: string) => void
}

/**
 * Re-fetch everything a sync run can change. Never throws — a refresh
 * failure must not mask a finished sync (each fetcher reports its own).
 */
export async function refreshAfterSync(deps: SyncRefreshDeps): Promise<void> {
  await Promise.allSettled([
    deps.fetchFiles(),
    deps.fetchOsDf(),
    deps.fetchSyncStatus(),
    deps.fetchSyncRuns(),
  ])
  try {
    deps.postBroadcast?.('cybermanju:files-changed')
  } catch {
    // Broadcast is best-effort; the local views already refreshed.
  }
}
