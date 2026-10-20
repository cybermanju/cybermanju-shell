// CyberManju OS — Poll (src/utils/poll.ts).
// Shared polling primitive for approve-then-verify flows (dashboard OAuth,
// Supabase OAuth, sync progress). AbortController-owned: callers keep one
// controller per flow, wire Cancel buttons + onBeforeUnmount to `abort()`,
// and never leak a `setInterval` again.

export interface PollOptions {
  /** ms between attempts. */
  intervalMs: number
  /** Give up after this many attempts (returns false). */
  maxAttempts: number
  /** Cancel between checks and interrupt waits; checks own their in-flight cancellation. */
  signal?: AbortSignal
  /** Progress callback with the 1-based attempt number. */
  onAttempt?: (attempt: number) => void
}

/** Cancel the inter-attempt wait without leaving a timer or abort listener behind. */
const delay = (ms: number, signal?: AbortSignal) => new Promise<void>((resolve, reject) => {
  if (signal?.aborted) {
    reject(new Error('cancelled'))
    return
  }
  const onAbort = () => {
    clearTimeout(timer)
    signal?.removeEventListener('abort', onAbort)
    reject(new Error('cancelled'))
  }
  const timer = setTimeout(() => {
    signal?.removeEventListener('abort', onAbort)
    resolve()
  }, ms)
  signal?.addEventListener('abort', onAbort, { once: true })
})

/**
 * Resolve `true` on the first passing check, `false` after `maxAttempts`.
 * Throws on abort so callers can distinguish cancel from timeout.
 */
export async function pollUntilTrue(
  check: () => Promise<boolean>,
  opts: PollOptions,
): Promise<boolean> {
  for (let attempt = 1; attempt <= opts.maxAttempts; attempt += 1) {
    if (opts.signal?.aborted) throw new Error('cancelled')
    opts.onAttempt?.(attempt)
    let ok = false
    try {
      ok = await check()
    } catch {
      ok = false
    }
    if (ok) return true
    if (attempt < opts.maxAttempts) await delay(opts.intervalMs, opts.signal)
  }
  return false
}
