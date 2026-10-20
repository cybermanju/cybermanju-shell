// CyberManju OS — tests/frontend/rest-fetch-timeout.test.ts.
// Role: see header automation pilot (collect-file-context + AI merge pending full prose).
// Exports/Integrates: documented at the item level; header to be expanded.
import { afterEach, describe, expect, it, vi } from 'vitest'
import { invoke, setServerUrl } from '@/composables/useTauri'

function response(body: unknown): Response {
  const text = JSON.stringify(body)
  return {
    ok: true,
    status: 200,
    statusText: 'OK',
    text: async () => text,
    json: async () => body,
  } as Response
}

describe('REST response-body timeout', () => {
  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
    setServerUrl('')
  })

  it('aborts and retries when headers arrive but the response body stalls', async () => {
    setServerUrl('https://dashboard.example')
    vi.useFakeTimers()

    const fetchMock = vi.fn(
      (_input: RequestInfo | URL, _init?: RequestInit): Promise<Response> =>
        Promise.resolve(response({ ready: true })),
    )
    fetchMock.mockImplementationOnce(async (_input, init) => {
      const signal = init?.signal as AbortSignal
      const stalledBody = () => new Promise<never>((_resolve, reject) => {
        const abort = () => reject(new DOMException('The operation was aborted', 'AbortError'))
        if (signal.aborted) abort()
        else signal.addEventListener('abort', abort, { once: true })
      })
      return {
        ok: true,
        status: 200,
        statusText: 'OK',
        text: stalledBody,
        json: stalledBody,
      } as unknown as Response
    })
    vi.stubGlobal('fetch', fetchMock)

    const pending = invoke<{ ready: boolean }>('get_sync_status')
    await vi.advanceTimersByTimeAsync(15000)
    await vi.advanceTimersByTimeAsync(300)

    await expect(pending).resolves.toEqual({ ready: true })
    expect(fetchMock).toHaveBeenCalledTimes(2)
    const firstInit = fetchMock.mock.calls[0]?.[1]
    expect(firstInit?.signal).toBeInstanceOf(AbortSignal)
    expect((firstInit?.signal as AbortSignal).aborted).toBe(true)
  })
})
