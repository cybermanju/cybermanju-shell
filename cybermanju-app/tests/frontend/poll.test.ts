// CyberManju OS — Polling regressions (tests/frontend/poll.test.ts).
// Role: verify retries and cancellation without real browser or network waits.
// Exports: none.
// Integrates: shared poll utility, AbortController, Vitest fake timers.
// Contracts: cancel waiting promptly; release timers/listeners; preserve retry results.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { pollUntilTrue } from '@/utils/poll'

beforeEach(() => { vi.useFakeTimers() })
afterEach(() => {
  vi.clearAllTimers()
  vi.useRealTimers()
  vi.restoreAllMocks()
})

describe('pollUntilTrue', () => {
  it('rejects an already cancelled flow before invoking a check', async () => {
    const controller = new AbortController()
    controller.abort()
    const check = vi.fn().mockResolvedValue(false)
    await expect(pollUntilTrue(check, {
      intervalMs: 1000, maxAttempts: 3, signal: controller.signal,
    })).rejects.toThrow('cancelled')
    expect(check).not.toHaveBeenCalled()
    expect(vi.getTimerCount()).toBe(0)
  })

  it('returns immediately on success without scheduling a wait', async () => {
    const check = vi.fn().mockResolvedValue(true)
    await expect(pollUntilTrue(check, { intervalMs: 1000, maxAttempts: 3 })).resolves.toBe(true)
    expect(check).toHaveBeenCalledTimes(1)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('cancels a pending wait immediately and removes its timer and listener', async () => {
    const controller = new AbortController()
    const add = vi.spyOn(controller.signal, 'addEventListener')
    const remove = vi.spyOn(controller.signal, 'removeEventListener')
    const check = vi.fn().mockResolvedValue(false)
    const result = pollUntilTrue(check, {
      intervalMs: 60_000, maxAttempts: 3, signal: controller.signal,
    })
    const rejection = expect(result).rejects.toThrow('cancelled')
    await vi.advanceTimersByTimeAsync(0)
    expect(vi.getTimerCount()).toBe(1)
    expect(add).toHaveBeenCalledTimes(1)
    controller.abort()
    await rejection
    expect(vi.getTimerCount()).toBe(0)
    expect(remove).toHaveBeenCalledWith('abort', add.mock.calls[0][1])
    await vi.advanceTimersByTimeAsync(60_000)
    expect(check).toHaveBeenCalledTimes(1)
  })

  it('does not schedule a delay if the signal aborts during a failed check', async () => {
    const controller = new AbortController()
    const check = vi.fn(async () => {
      controller.abort()
      return false
    })
    await expect(pollUntilTrue(check, {
      intervalMs: 60_000, maxAttempts: 3, signal: controller.signal,
    })).rejects.toThrow('cancelled')
    expect(check).toHaveBeenCalledTimes(1)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('cleans each completed wait while preserving attempts and progress', async () => {
    const controller = new AbortController()
    const add = vi.spyOn(controller.signal, 'addEventListener')
    const remove = vi.spyOn(controller.signal, 'removeEventListener')
    const check = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(false).mockResolvedValue(true)
    const onAttempt = vi.fn()
    const result = pollUntilTrue(check, {
      intervalMs: 1000, maxAttempts: 3, signal: controller.signal, onAttempt,
    })
    await vi.advanceTimersByTimeAsync(0)
    expect(vi.getTimerCount()).toBe(1)
    await vi.advanceTimersByTimeAsync(1000)
    expect(remove).toHaveBeenCalledTimes(1)
    expect(vi.getTimerCount()).toBe(1)
    await vi.advanceTimersByTimeAsync(1000)
    await expect(result).resolves.toBe(true)
    expect(check).toHaveBeenCalledTimes(3)
    expect(onAttempt.mock.calls).toEqual([[1], [2], [3]])
    expect(add).toHaveBeenCalledTimes(2)
    expect(remove).toHaveBeenCalledTimes(2)
    for (const [, listener] of add.mock.calls) {
      expect(remove).toHaveBeenCalledWith('abort', listener)
    }
    expect(vi.getTimerCount()).toBe(0)
    controller.abort()
    expect(vi.getTimerCount()).toBe(0)
  })

  it('returns false after the configured number of attempts without a signal', async () => {
    const check = vi.fn().mockResolvedValue(false)
    const result = pollUntilTrue(check, { intervalMs: 1000, maxAttempts: 3 })
    await vi.advanceTimersByTimeAsync(2000)
    await expect(result).resolves.toBe(false)
    expect(check).toHaveBeenCalledTimes(3)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('retries transient check errors as before', async () => {
    const check = vi.fn().mockRejectedValueOnce(new Error('network')).mockResolvedValue(true)
    const result = pollUntilTrue(check, { intervalMs: 1000, maxAttempts: 3 })
    await vi.advanceTimersByTimeAsync(1000)
    await expect(result).resolves.toBe(true)
    expect(check).toHaveBeenCalledTimes(2)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('returns false without a check when no attempts are requested', async () => {
    const check = vi.fn().mockResolvedValue(true)
    await expect(pollUntilTrue(check, { intervalMs: 1000, maxAttempts: 0 })).resolves.toBe(false)
    expect(check).not.toHaveBeenCalled()
    expect(vi.getTimerCount()).toBe(0)
  })
})
