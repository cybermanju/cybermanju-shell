// CyberManju OS — Server auth.test (tests/frontend/server-auth.test.ts).
// Docker/web login gate — the 401 fan-out happened because the frontend had
// no way to sign in: `authenticate_user`/`register_user` existed but no UI
// called them, and the 401 hint pointed at provider OAuth (which never
// authenticates /api calls). These tests lock the REST surface the gate
// needs: public bootstrap probe, login/register, session revocation.
import { describe, expect, it } from 'vitest'
import { REST_ROUTES } from '@/composables/useTauri'

describe('server auth routes (Docker login gate)', () => {
  it('probes bootstrap state without credentials', () => {
    const r = REST_ROUTES.auth_status
    expect(r.method).toBe('GET')
    expect(r.buildPath({})).toBe('/api/auth/status')
  })

  it('revokes the session on logout', () => {
    const r = REST_ROUTES.logout_user
    expect(r.method).toBe('POST')
    expect(r.buildPath({})).toBe('/api/auth/logout')
  })

  it('keeps login/register on the public JWT endpoints', () => {
    expect(REST_ROUTES.authenticate_user.buildPath({})).toBe('/api/users/login')
    expect(REST_ROUTES.authenticate_user.transformRequest!({ username: 'a', password: 'b' })).toEqual({
      username: 'a',
      password: 'b',
    })
    expect(REST_ROUTES.register_user.buildPath({})).toBe('/api/users/register')
  })
})
