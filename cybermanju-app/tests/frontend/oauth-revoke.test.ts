// CyberManju OS — Oauth revoke.test (tests/frontend/oauth-revoke.test.ts).
// OAuth revoke — the Accounts panel's Revoke buttons rest on
// `revokeProviderGrant`: Google revokes provider-side (mocked fetch here),
// providers without a browser-safe endpoint clear locally, and an empty
// token is a no-op so Revoke never fires network calls with nothing.
import { afterEach, describe, expect, it, vi } from 'vitest'
import { revokeProviderGrant } from '@/composables/useSupabase'

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('revokeProviderGrant', () => {
  it('is a no-op without a token', async () => {
    const r = await revokeProviderGrant('google', '  ')
    expect(r.revokedAtProvider).toBe(false)
    expect(r.detail).toMatch(/no token/)
  })

  it('clears locally where no browser-safe revoke exists', async () => {
    for (const p of ['github', 'gitlab']) {
      const r = await revokeProviderGrant(p, 'gho_secret')
      expect(r.revokedAtProvider).toBe(false)
      expect(r.detail).toMatch(/cleared locally/)
    }
  })

  it('revokes the Google grant provider-side', async () => {
    const fetchMock = vi.fn(async () => ({ ok: true, status: 200 }) as Response)
    vi.stubGlobal('fetch', fetchMock)
    const r = await revokeProviderGrant('google', 'ya29.secret')
    expect(r.revokedAtProvider).toBe(true)
    expect(fetchMock).toHaveBeenCalledOnce()
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(url).toContain('oauth2.googleapis.com/revoke')
    expect(url).toContain(encodeURIComponent('ya29.secret'))
    expect(init.method).toBe('POST')
  })

  it('falls back to a local clear when Google refuses', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => ({ ok: false, status: 400 }) as Response))
    const r = await revokeProviderGrant('google', 'ya29.stale')
    expect(r.revokedAtProvider).toBe(false)
    expect(r.detail).toMatch(/cleared locally/)
  })
})
