// CyberManju OS — Supabase Auth bridge (static/offline builds)
//
// Why this exists: PKCE OAuth from a pure static page is impossible against
// the providers directly (GitHub demands `client_secret` + serves no CORS;
// Google's token endpoint is equally browser-hostile). Supabase Auth acts
// as the broker: OUR static page talks only to Supabase (CORS-open), and
// Supabase's server does the secret-holding exchange with the provider.
//
// Flow (all standard supabase-js v2 PKCE):
//  1. `startSupabaseOAuth()` / `startSupabaseSignIn()` —
//     `signInWithOAuth({ skipBrowserRedirect: true })` returns the Supabase
//     authorize URL (PKCE verifier stashed in localStorage by the client).
//     Web builds open it in a popup WITHOUT `noopener` so the opener chain
//     survives; Tauri builds (desktop + mobile) open it in the SYSTEM browser
//     with the `cybermanju://oauth/callback` return instead.
//  2. User approves at the provider → Supabase → redirectTo: the popup page
//     (`?oauth=popup`) on web, the deep-link callback (`onOpenUrl`) on Tauri.
//  3. The popup boots this same bundle but takes the popup fast-path
//     (`isOAuthPopup()` → App.vue minimal view): `finishSupabaseReturn()`
//     lets the client auto-exchange `?code=` (`detectSessionInUrl`), reads
//     `session.provider_token`, records the account, postMessages the opener
//     (`cybermanju:oauth-done`) and closes itself with retries — it NEVER
//     renders the full app in the popup window.
//  4. The opener wakes on the message (or its session poll), verifies the
//     token belongs to the expected provider, closes the popup reference it
//     holds, and finalizes — CONNECTED.
// Sign-in is always FRESH (existing session signed out first), so every
// provider button stays usable and multiple accounts on the same provider
// can be added one after another; `connectedAccounts` remembers them all
// while Supabase holds the single active session.
//
// The provider token is a real provider credential (GitHub honors it as a
// token; Drive accepts the Bearer). Both sign-in and per-card Connect request
// the minimum scopes the sync backends need (Google includes `drive.file`),
// so a top sign-in token is already Drive-capable. Token expiry: GitHub
// OAuth tokens don't expire by default; Google access tokens last ~1h (no
// browser-safe refresh — the panel asks for a reconnect when Google probes
// fail).

import { ref } from 'vue'
import type { Session, SupabaseClient } from '@supabase/supabase-js'
import { isTauri } from './useTauri'
import { openExternalUrl } from '@/utils/openExternal'

const PENDING_CFG_KEY = 'cybermanju.oauthConfigId'
const TOKEN_STASH_KEY = 'cybermanju.providerToken'

export type OAuthBackend = 'github' | 'google' | 'gitlab'
/**
 * Custom-scheme OAuth return for every Tauri build (desktop + mobile).
 * The SYSTEM browser completes the provider dance, then the OS routes this
 * URL back into the app via the deep-link plugin — the WebView never
 * navigates to it, so Linux WebKitGTK's
 * "Redirection to URL with a scheme that is not HTTP(S)" block never fires.
 */
export const OAUTH_CALLBACK_URL = 'cybermanju://oauth/callback'
/**
 * Bare custom scheme for OS handler registration. Source of truth for the
 * value is `src-tauri/tauri.conf.json > plugins.deep-link.desktop.schemes`
 * (the bundler stamps it into every installer); this constant only feeds the
 * runtime `register()` self-heal below, so keep the two in sync.
 */
export const OAUTH_DEEP_LINK_SCHEME = 'cybermanju'
/** @deprecated Use OAUTH_CALLBACK_URL — kept for backwards compatibility. */
export const MOBILE_OAUTH_CALLBACK_URL = OAUTH_CALLBACK_URL

export interface ProviderTokenStash {
  backend: OAuthBackend
  providerToken: string
  providerRefreshToken?: string | null
  at: number
}

let client: SupabaseClient | null = null
let clientKey = ''

function readLS(key: string): string {
  try {
    return localStorage.getItem(key) || ''
  } catch {
    return ''
  }
}

function writeLS(key: string, value: string) {
  try {
    if (value) localStorage.setItem(key, value)
    else localStorage.removeItem(key)
  } catch {
    // Private mode — session-only.
  }
}

export type SupabaseConfigSource = 'settings' | 'build-env' | 'none'

export interface SupabaseConfig {
  url: string
  key: string
  source: SupabaseConfigSource
}

// Manual override pasted in Settings → OAuth broker. Stored per device /
// profile (localStorage): the anon key is public by design, but the broker
// choice roams with NEITHER the vault NOR the OS — every device (and every
// browser profile) needs the paste once, then provider tokens minted on one
// device roam to the others through the encrypted sync configs.
const MANUAL_URL_KEY = 'cybermanju.supabaseUrl'
const MANUAL_KEY_KEY = 'cybermanju.supabaseKey'

const BROKER_HELP = 'Supabase broker is not configured — set the project URL + anon key in Settings → OAuth broker'

function readBuildEnv(): { url: string; key: string } {
  const url = String(
    (import.meta.env.VITE_SUPABASE_URL as string | undefined) ?? '',
  ).trim().replace(/\/+$/, '')
  const envKey =
    String(
      (import.meta.env.VITE_SUPABASE_ANON_KEY as string | undefined) ?? '',
    ).trim() ||
    // Legacy alias, honoured even when ANON_KEY is present-but-empty (CI
    // injects unset secrets as empty strings, and `'' ?? x` stays `''`).
    String((import.meta.env.VITE_SUPABASE_KEY as string | undefined) ?? '').trim()
  return { url, key: envKey }
}

function readManualPair(): { url: string; key: string } {
  const url = readLS(MANUAL_URL_KEY).trim().replace(/\/+$/, '')
  const key = readLS(MANUAL_KEY_KEY).trim()
  return { url, key }
}

export function getSupabaseConfig(): SupabaseConfig {
  // An explicit Settings paste wins over the baked pair (lets any build —
  // distro package, older release, CI without secrets — be fixed at runtime,
  // and lets a rotated broker be picked up without waiting for a release).
  const manual = readManualPair()
  if (manual.url && manual.key) return { url: manual.url, key: manual.key, source: 'settings' }
  const env = readBuildEnv()
  if (env.url && env.key) return { url: env.url, key: env.key, source: 'build-env' }
  if (env.url || env.key || manual.url || manual.key) {
    // Half-filled on one side only — surface what exists so the UI can say
    // which half is missing instead of a bare "not configured".
    const url = manual.url || env.url
    const key = manual.key || env.key
    return { url, key, source: manual.url || manual.key ? 'settings' : 'build-env' }
  }
  return { url: '', key: '', source: 'none' }
}

function readConfiguredFlag(): boolean {
  const { url, key } = getSupabaseConfig()
  return url.startsWith('http') && key.length > 0
}

/**
 * Reactive mirror of the effective broker. Changes on Save / Forget /
 * boot-hydrate, and across tabs/windows of the same profile via the
 * `storage` listener below.
 */
const configuredFlag = ref(readConfiguredFlag())

function refreshConfiguredFlag(): boolean {
  configuredFlag.value = readConfiguredFlag()
  return configuredFlag.value
}

export function supabaseConfigured(): boolean {
  return configuredFlag.value
}

/** Persist a Settings paste. Returns an error string, or null on success. */
export function setSupabaseConfig(url: string, key: string): string | null {
  const cleanUrl = String(url ?? '').trim().replace(/\/+$/, '')
  const cleanKey = String(key ?? '').trim()
  if (!cleanUrl || !cleanKey) return 'Paste the project URL and the anon / publishable key first.'
  if (!cleanUrl.startsWith('http')) return 'Project URL must be a full https://… address.'
  if (cleanKey.length < 8) return 'That key looks too short — paste the anon / publishable (sb_publishable_… or eyJ…) key.'
  writeLS(MANUAL_URL_KEY, cleanUrl)
  writeLS(MANUAL_KEY_KEY, cleanKey)
  client = null
  clientKey = ''
  refreshConfiguredFlag()
  return null
}

/** Forget the Settings paste (falls back to the baked pair when one exists). */
export function clearSupabaseConfig() {
  writeLS(MANUAL_URL_KEY, '')
  writeLS(MANUAL_KEY_KEY, '')
  client = null
  clientKey = ''
  refreshConfiguredFlag()
}

/**
 * Re-read the effective broker (build env + Settings paste). Called at boot
 * and safe to call any time — returns whether the broker is usable now.
 */
export async function hydrateSupabaseConfig(): Promise<boolean> {
  return refreshConfiguredFlag()
}

// Same profile, another window/tab saved or forgot the broker — stay in
// sync without a reload (badges, buttons and gates read `configuredFlag`).
if (typeof window !== 'undefined' && typeof window.addEventListener === 'function') {
  window.addEventListener('storage', (e) => {
    if (e.key === MANUAL_URL_KEY || e.key === MANUAL_KEY_KEY) {
      client = null
      clientKey = ''
      refreshConfiguredFlag()
    }
  })
}

export async function getSupabaseClient(): Promise<SupabaseClient | null> {
  const { url, key } = getSupabaseConfig()
  if (!url || !key) return null
  const cacheKey = `${url}|${key.slice(0, 8)}`
  if (!client || clientKey !== cacheKey) {
    const { createClient } = await import('@supabase/supabase-js')
    client = createClient(url, key, {
      auth: { flowType: 'pkce', detectSessionInUrl: true },
    })
    clientKey = cacheKey
  }
  return client
}

/** Frontend sync backend → Supabase provider slug. */
export function supabaseProviderFor(backendType: string): OAuthBackend | null {
  switch (backendType) {
    case 'github':
      return 'github'
    case 'googleDrive':
      return 'google'
    case 'gitlab':
      return 'gitlab'
    default:
      return null
  }
}

/** Minimum scopes the sync backends need from each provider. */
export function supabaseScopesFor(backendType: string): string {
  switch (backendType) {
    case 'github':
      return 'repo read:user user:email'
    case 'googleDrive':
      return 'openid email profile https://www.googleapis.com/auth/drive.file'
    case 'gitlab':
      return 'api'
    default:
      return ''
  }
}

/**
 * Scopes the top sign-in buttons request. Sign-in used to ask for identity
 * only (`openid email profile`), so the session's `provider_token` could not
 * touch Drive — folder create probed 403 while the user looked logged in.
 * Sign-in now asks for the same sync scopes as the per-card Connect flow,
 * so the session token is Drive-capable straight away (Google) / repo-capable
 * (GitHub) / api-capable (GitLab). Existing sessions minted before this keep
 * the old narrow grant — sign out + sign in again to pick up the new scopes.
 */
export function supabaseSignInScopes(provider: OAuthBackend): string {
  switch (provider) {
    case 'google':
      return supabaseScopesFor('googleDrive')
    case 'github':
      return supabaseScopesFor('github')
    case 'gitlab':
      return supabaseScopesFor('gitlab')
    default:
      return 'openid email profile'
  }
}

/** Google must show a fresh consent screen when an existing grant lacks drive.file. */
export function supabaseOAuthQueryParams(backendType: string, forceGoogleConsent = false): Record<string, string> | undefined {
  const provider = backendType === 'google' ? 'google' : supabaseProviderFor(backendType)
  if (!forceGoogleConsent || provider !== 'google') return undefined
  return { access_type: 'offline', prompt: 'consent' }
}

export function supabaseRedirectTo(popup: boolean): string {
  // Every Tauri build (desktop AND mobile) authenticates in the SYSTEM
  // browser with the cybermanju:// deep-link return: a popup WebView cannot
  // navigate back to a custom scheme (Linux WebKitGTK aborts the whole login
  // with "Redirection to URL with a scheme that is not HTTP(S)").
  if (isTauri()) return OAUTH_CALLBACK_URL
  return `${window.location.origin}${window.location.pathname}${popup ? '?oauth=popup' : ''}`
}

/** Supabase rejected the redirect — tell the user which URL to allowlist. */
function withRedirectHint(e: unknown, redirectTo: string): Error {
  const msg = e instanceof Error ? e.message : String(e)
  if (/redirect/i.test(msg)) {
    return new Error(
      `${msg} — add '${redirectTo}' (plus this app's own URL) to Supabase → Authentication → URL Configuration → Redirect URLs`,
    )
  }
  return e instanceof Error ? e : new Error(msg)
}

export function setPendingOAuthConfig(configId: string | null) {
  writeLS(PENDING_CFG_KEY, configId ?? '')
}

export function getPendingOAuthConfig(): string {
  return readLS(PENDING_CFG_KEY)
}

export function stashProviderToken(stash: ProviderTokenStash) {
  writeLS(TOKEN_STASH_KEY, JSON.stringify(stash))
}

export function takeProviderTokenStash(): ProviderTokenStash | null {
  let raw = ''
  try {
    raw = localStorage.getItem(TOKEN_STASH_KEY) || ''
    localStorage.removeItem(TOKEN_STASH_KEY)
  } catch {
    return null
  }
  if (!raw) return null
  try {
    const parsed = JSON.parse(raw) as ProviderTokenStash
    if (!parsed?.providerToken) return null
    // Stale (older than 10 min) — the code lives 5 min; be generous.
    if (Date.now() - (parsed.at || 0) > 10 * 60 * 1000) return null
    return parsed
  } catch {
    return null
  }
}

/**
 * True when THIS window is an OAuth popup we opened — not the main tab.
 * Supabase redirects back with `?code=` (or `?error=` on deny) and our
 * `redirectTo` carries the `?oauth=popup` marker while `window.opener` is
 * alive. The popup boots the same bundle, so without this check it renders
 * the whole app inside the small window and never closes.
 */
export function isOAuthPopup(): boolean {
  try {
    if (typeof window === 'undefined') return false
    const params = new URLSearchParams(window.location.search)
    if (params.get('oauth') === 'popup') return true
    if (typeof window.opener !== 'undefined' && window.opener && !window.opener.closed) {
      if (params.has('code') || params.has('error')) return true
    }
  } catch {
    return false
  }
  return false
}

/**
 * Tell the opener tab the popup flow finished. The opener treats the shared
 * session as source of truth and only uses this to wake up early — so even
 * if `window.close()` is refused, the flow still completes over there.
 */
function notifyOpenerOAuthDone(ok: boolean, provider?: string | null): void {
  try {
    if (typeof window === 'undefined' || !window.opener || window.opener.closed) return
    const msg = { type: 'cybermanju:oauth-done', ok, provider: provider ?? null }
    try {
      window.opener.postMessage(msg, window.location.origin)
    } catch {
      window.opener.postMessage(msg, '*')
    }
  } catch {
    // No opener to tell — the opener polls the shared session as fallback.
  }
}

/**
 * Aggressively close an OAuth popup. A bare `window.close()` is refused when
 * the opener chain was severed (`noopener`) or the browser wants a gesture —
 * retry on a timer. ONLY call this in a popup (`isOAuthPopup()`), never in
 * the main tab.
 */
export function closeOAuthPopup(): void {
  for (const delayMs of [0, 600, 2000]) {
    window.setTimeout(() => {
      try {
        if (window.closed) return
        window.close()
      } catch {
        // Blocked — the opener also holds a reference and closes from its
        // side; the fallback message below invites a manual close.
      }
    }, delayMs)
  }
}

/**
 * Begin PKCE OAuth without leaving the app: returns the Supabase authorize
 * URL for us to open in a popup. Throws when Supabase is not configured.
 */
export async function startSupabaseOAuth(backendType: string, popup = true, forceGoogleConsent = false): Promise<{ url: string }> {
  const provider = supabaseProviderFor(backendType)
  if (!provider) throw new Error(`unsupported: no Supabase OAuth for '${backendType}'`)
  const sb = await getSupabaseClient()
  if (!sb) throw new Error(BROKER_HELP)
  const redirectTo = supabaseRedirectTo(popup)
  const { data, error } = await sb.auth.signInWithOAuth({
    provider,
    options: {
      redirectTo,
      scopes: supabaseScopesFor(backendType) || undefined,
      queryParams: supabaseOAuthQueryParams(backendType, forceGoogleConsent),
      skipBrowserRedirect: true,
    },
  })
  if (error || !data?.url) throw withRedirectHint(error ?? new Error('Supabase did not return an authorize URL'), redirectTo)
  return { url: data.url }
}

export async function supabaseSession(): Promise<Session | null> {
  const sb = await getSupabaseClient()
  if (!sb) return null
  const { data, error } = await sb.auth.getSession()
  if (error) return null
  return data.session
}

export async function supabaseSignOut(): Promise<void> {
  const sb = await getSupabaseClient()
  if (!sb) return
  await sb.auth.signOut()
  client = null
}

/**
 * Best-effort provider-side OAuth grant revocation from the browser.
 *
 * Google exposes a CORS-open revoke endpoint, so the grant actually dies
 * provider-side. GitHub / GitLab app grants need `client_secret` (server
 * only), so there is no browser-safe revoke — those clear locally here and
 * the caller still drops every local copy (session, stash, saved secret), so
 * autologin and token reuse from this browser stop either way.
 */
export interface RevokeGrantResult {
  revokedAtProvider: boolean
  detail: string
}

export async function revokeProviderGrant(
  provider: OAuthBackend | string,
  token: string,
): Promise<RevokeGrantResult> {
  if (!token.trim()) return { revokedAtProvider: false, detail: 'no token to revoke' }
  if (provider === 'google') {
    try {
      const ctrl = new AbortController()
      const timer = setTimeout(() => ctrl.abort(), 10_000)
      try {
        const res = await fetch(
          `https://oauth2.googleapis.com/revoke?token=${encodeURIComponent(token.trim())}`,
          {
            method: 'POST',
            headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
            signal: ctrl.signal,
          },
        )
        if (res.ok) return { revokedAtProvider: true, detail: 'Google grant revoked' }
        return { revokedAtProvider: false, detail: `Google revoke refused (${res.status}) — cleared locally` }
      } finally {
        clearTimeout(timer)
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      return { revokedAtProvider: false, detail: `Google revoke unreachable (${msg}) — cleared locally` }
    }
  }
  return {
    revokedAtProvider: false,
    detail: 'provider has no browser-safe revoke — cleared locally (re-create the grant at the provider to fully revoke)',
  }
}

/** Drop the one-shot provider-token stash without consuming its freshness check. */
export function clearProviderTokenStash(): void {
  writeLS(TOKEN_STASH_KEY, '')
}

/**
 * Revoke the active OAuth session: provider-side grant (best-effort) +
 * Supabase session (global scope so refresh/autologin dies server-side) +
 * local stash/pending state. Callers also forget the remembered account and
 * clear saved provider secrets to fully stop autologin.
 */
export async function revokeIdentitySession(): Promise<RevokeGrantResult> {
  let provider: OAuthBackend | string = ''
  let token = ''
  try {
    const session = await supabaseSession()
    provider = supabaseSessionProvider(session) ?? ''
    token = session?.provider_token ?? ''
  } catch {
    // No session to read — still sign out below.
  }
  const grant = token
    ? await revokeProviderGrant(provider || 'github', token)
    : { revokedAtProvider: false, detail: 'no active provider token' }
  try {
    const sb = await getSupabaseClient()
    if (sb) {
      try {
        await sb.auth.signOut({ scope: 'global' } as Parameters<typeof sb.auth.signOut>[0])
      } catch {
        await sb.auth.signOut()
      }
    }
  } catch {
    // Sign-out is best-effort — local state below still clears.
  }
  client = null
  identity.value = null
  clearProviderTokenStash()
  setPendingOAuthConfig(null)
  return grant
}

/**
 * Called once at app boot (and on the popup fast-path). If this page load
 * IS an OAuth return (`?code=`, `?error=`, or the `?oauth=popup` marker with
 * a live opener), the client auto-exchanges it (`detectSessionInUrl`) — here
 * we read the session, stash + record, clean the URL, notify the opener and
 * close popup returns. Returns true when it handled a return.
 */
export async function finishSupabaseReturn(): Promise<boolean> {
  let params: URLSearchParams
  try {
    params = new URLSearchParams(window.location.search)
  } catch {
    return false
  }
  const code = params.get('code')
  const err = params.get('error')
  const popup = isOAuthPopup()
  if (!code && !err && !popup) return false

  const stripReturnParams = () => {
    params.delete('code')
    params.delete('error')
    params.delete('error_description')
    params.delete('oauth')
    window.history.replaceState(null, '', `${window.location.pathname}${params.toString() ? `?${params}` : ''}${window.location.hash}`)
  }

  // No code to exchange: denied at the provider, broker missing, or a bare
  // marker. Never boot the app in a popup — tell the opener and close.
  if (!code) {
    stripReturnParams()
    if (popup) {
      notifyOpenerOAuthDone(false)
      closeOAuthPopup()
    }
    return true
  }
  if (!supabaseConfigured()) {
    // A code we can never exchange — strip it so it doesn't linger.
    stripReturnParams()
    if (popup) {
      notifyOpenerOAuthDone(false)
      closeOAuthPopup()
    }
    return true
  }
  let provider: OAuthBackend | null = null
  try {
    // `detectSessionInUrl` auto-exchanges `?code=` during client creation,
    // but the exchange races this read — one retry before giving up.
    let session = await supabaseSession()
    if (!session?.provider_token) {
      await new Promise((r) => setTimeout(r, 1200))
      session = await supabaseSession()
    }
    const token = session?.provider_token
    provider = supabaseSessionProvider(session)
    if (token) {
      stashProviderToken({
        backend: provider ?? 'github',
        providerToken: token,
        providerRefreshToken: session?.provider_refresh_token ?? null,
        at: Date.now(),
      })
    }
    // The popup path is also a sign-in: publish whoever came back.
    identity.value = identityFromSession(session)
    recordConnectedAccount(session)
    if (popup) notifyOpenerOAuthDone(true, provider)
  } catch {
    // Exchange failed (expired code, verifier mismatch) — still clean up.
    if (popup) notifyOpenerOAuthDone(false)
  }
  stripReturnParams()
  if (popup) closeOAuthPopup()
  return true
}

/** Provider slug behind a Supabase session (`user.app_metadata.provider`). */
export function supabaseSessionProvider(session: Session | null): OAuthBackend | null {
  const provider = (session?.user?.app_metadata as Record<string, unknown> | undefined)?.provider
  if (provider === 'github' || provider === 'google' || provider === 'gitlab') return provider
  return null
}

// ── Identity (OAuth sign-in — the only sign-in) ──────────────────────────
//
// There is no username/password anywhere in the app any more: the Supabase
// broker signs you in with Google, GitHub or GitLab, and the session that
// comes back IS the identity. Sessions live in supabase-js's own
// localStorage record, so a reload keeps you signed in without a token
// dance here.

export interface CyberIdentity {
  id: string
  email: string
  name: string
  avatarUrl: string
  provider: string
}

export const identity = ref<CyberIdentity | null>(null)

// ── Connected accounts (multi-account) ────────────────────────────────
//
// Supabase holds ONE active session, but users have several logins (work /
// personal, same provider twice). Every successful sign-in is recorded here
// (persisted in localStorage) so the Accounts panel can list them all and
// switch in one click: switch = sign out + fresh login with that provider,
// letting the provider's own chooser pick the account.

export interface ConnectedAccount {
  id: string
  email: string
  name: string
  avatarUrl: string
  provider: string
  lastUsed: number
}

const ACCOUNTS_KEY = 'cybermanju.connectedAccounts'

function loadConnectedAccounts(): ConnectedAccount[] {
  try {
    const raw = localStorage.getItem(ACCOUNTS_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as ConnectedAccount[]
    if (!Array.isArray(parsed)) return []
    return parsed.filter(a => a && typeof a.id === 'string' && typeof a.provider === 'string').slice(0, 10)
  } catch {
    return []
  }
}

export const connectedAccounts = ref<ConnectedAccount[]>(loadConnectedAccounts())

function persistConnectedAccounts(): void {
  try {
    localStorage.setItem(ACCOUNTS_KEY, JSON.stringify(connectedAccounts.value))
  } catch {
    // Private mode — memory only.
  }
}

/** Record a session's user in the remembered list (deduped by user id). */
export function recordConnectedAccount(session: Session | null): ConnectedAccount | null {
  const idn = identityFromSession(session)
  if (!idn) return null
  const acc: ConnectedAccount = {
    id: idn.id,
    email: idn.email,
    name: idn.name,
    avatarUrl: idn.avatarUrl,
    provider: idn.provider,
    lastUsed: Date.now(),
  }
  connectedAccounts.value = [acc, ...connectedAccounts.value.filter(a => a.id !== acc.id)].slice(0, 10)
  persistConnectedAccounts()
  return acc
}

/**
 * Forget a remembered account. Forgetting the ACTIVE one also ends the
 * session (there is nothing to stay signed in as).
 */
export function forgetConnectedAccount(id: string): void {
  connectedAccounts.value = connectedAccounts.value.filter(a => a.id !== id)
  persistConnectedAccounts()
  if (identity.value?.id === id) {
    identity.value = null
    void supabaseSignOut().catch(() => {})
  }
}

function identityFromSession(session: Session | null): CyberIdentity | null {
  const user = session?.user
  if (!user) return null
  const meta = (user.user_metadata ?? {}) as Record<string, unknown>
  const appMeta = (user.app_metadata ?? {}) as Record<string, unknown>
  const str = (v: unknown): string => (typeof v === 'string' ? v : '')
  return {
    id: user.id,
    email: str(user.email),
    name: str(meta.full_name) || str(meta.name) || str(meta.preferred_username) || str(user.email) || 'Signed in',
    avatarUrl: str(meta.avatar_url) || str(meta.picture),
    provider: str(appMeta.provider) || 'supabase',
  }
}

/** Read the current session and publish it as `identity`. */
export async function refreshIdentity(): Promise<CyberIdentity | null> {
  const session = await supabaseSession()
  identity.value = identityFromSession(session)
  // Repopulate the remembered list on fresh browsers (cleared site data).
  if (session) recordConnectedAccount(session)
  return identity.value
}

let mobileDeepLinkReady = false
let mobileDeepLinkListener: (() => void) | null = null
let mobileDeepLinkInstallPromise: Promise<void> | null = null
const pendingMobileDeepLinks: string[] = []
const handledMobileOAuthCodes = new Set<string>()

function isMobileOAuthUrl(value: string): boolean {
  try {
    const url = new URL(value)
    return url.protocol === 'cybermanju:' && url.hostname === 'oauth' && url.pathname === '/callback'
  } catch {
    return false
  }
}

async function exchangeMobileOAuthUrl(value: string): Promise<boolean> {
  const url = new URL(value)
  if (!isMobileOAuthUrl(value)) return false
  const providerError = url.searchParams.get('error_description') || url.searchParams.get('error')
  if (providerError) throw new Error(providerError)
  const code = url.searchParams.get('code')
  if (!code) throw new Error('The OAuth callback did not include an authorization code.')
  if (handledMobileOAuthCodes.has(code)) return true
  const sb = await getSupabaseClient()
  if (!sb) throw new Error(BROKER_HELP)
  const { data, error } = await sb.auth.exchangeCodeForSession(code)
  if (error) throw error
  const session = data.session ?? await supabaseSession()
  if (!session) throw new Error('Supabase returned no session for this sign-in.')
  const provider = supabaseSessionProvider(session)
  identity.value = identityFromSession(session)
  recordConnectedAccount(session)
  if (session.provider_token && provider) {
    stashProviderToken({
      backend: provider,
      providerToken: session.provider_token,
      providerRefreshToken: session.provider_refresh_token ?? null,
      at: Date.now(),
    })
  }
  handledMobileOAuthCodes.add(code)
  return true
}

async function processMobileOAuthUrls(urls: string[], announce: boolean): Promise<boolean> {
  let handled = false
  for (const url of urls) {
    if (!isMobileOAuthUrl(url)) continue
    handled = true
    const configId = getPendingOAuthConfig() || null
    try {
      await exchangeMobileOAuthUrl(url)
      if (announce) window.dispatchEvent(new CustomEvent('cybermanju:oauth-return', { detail: { ok: true, configId } }))
    } catch (e) {
      if (announce) window.dispatchEvent(new CustomEvent('cybermanju:oauth-return', {
        detail: { ok: false, configId, message: e instanceof Error ? e.message : String(e) },
      }))
    }
  }
  return handled
}

/** Install the native deep-link listener early; callbacks wait in memory until broker hydration completes. */
export function installMobileOAuthDeepLinks(): Promise<void> {
  if (!isTauri() || mobileDeepLinkListener) return Promise.resolve()
  if (mobileDeepLinkInstallPromise) return mobileDeepLinkInstallPromise
  const install = (async () => {
    const { getCurrent, isRegistered, onOpenUrl, register } = await import('@tauri-apps/plugin-deep-link')
    mobileDeepLinkListener = await onOpenUrl((urls) => {
      if (!mobileDeepLinkReady) pendingMobileDeepLinks.push(...urls)
      else void processMobileOAuthUrls(urls, true)
    })
    const initialUrls = await getCurrent().catch(() => null)
    if (initialUrls) pendingMobileDeepLinks.push(...initialUrls)
    // Desktop self-heal (Linux/Windows): claim the custom scheme at runtime
    // so installs without OS-level integration (bare AppImage with no
    // launcher daemon, xcopy-style exe) still receive the OAuth return.
    // Strictly best-effort and failure-swallowed: register() is unsupported
    // on macOS/mobile (bundler owns those), and a missing xdg-mime must
    // never break the listener install above.
    try {
      const already = await isRegistered(OAUTH_DEEP_LINK_SCHEME).catch(() => true)
      if (!already) await register(OAUTH_DEEP_LINK_SCHEME)
    } catch {
      /* OS registration unavailable here — installer/.desktop owns it. */
    }
  })()
  mobileDeepLinkInstallPromise = install.catch((error) => {
    mobileDeepLinkInstallPromise = null
    throw error
  })
  return mobileDeepLinkInstallPromise
}

/** Exchange any cold-start callback after the stored Supabase broker is hydrated. */
export async function activateMobileOAuthDeepLinks(): Promise<boolean> {
  if (!isTauri()) return false
  await installMobileOAuthDeepLinks()
  mobileDeepLinkReady = true
  const queued = pendingMobileDeepLinks.splice(0)
  return processMobileOAuthUrls(queued, true)
}

/**
 * Begin sign-in with one of the Supabase-brokered providers. Returns the
 * authorize URL for a popup (same shape as `startSupabaseOAuth`).
 */
export async function startSupabaseSignIn(provider: OAuthBackend, popup = true, forceGoogleConsent = false): Promise<{ url: string }> {
  const sb = await getSupabaseClient()
  if (!sb) {
    throw new Error(BROKER_HELP)
  }
  const redirectTo = supabaseRedirectTo(popup)
  const { data, error } = await sb.auth.signInWithOAuth({
    provider,
    options: {
      redirectTo,
      scopes: supabaseSignInScopes(provider),
      queryParams: supabaseOAuthQueryParams(provider, forceGoogleConsent),
      skipBrowserRedirect: true,
    },
  })
  if (error || !data?.url) {
    throw withRedirectHint(error ?? new Error('Supabase did not return an authorize URL'), redirectTo)
  }
  return { url: data.url }
}

/**
 * Full interactive sign-in: opens the provider in a centered popup and waits
 * for the session to land back in this tab. Every call starts FRESH — any
 * existing session is signed out first — so re-login with the same user, a
 * second account on the same provider, or another provider always lands a
 * new session (and the provider shows its account chooser). Throws when
 * popups are blocked or the flow never completes.
 */
export async function signInWithPopup(provider: OAuthBackend): Promise<CyberIdentity> {
  // Native (desktop + mobile): the SYSTEM browser approves, the OS routes
  // cybermanju://oauth/callback back into the app, and the poll below sees
  // the exchanged session. Never a popup WebView here — it cannot navigate
  // back to a custom scheme (Linux WebKitGTK: "scheme that is not HTTP(S)").
  if (isTauri()) {
    await installMobileOAuthDeepLinks()
    await supabaseSignOut().catch(() => {})
    identity.value = null
    const { url } = await startSupabaseSignIn(provider, false, provider === 'google')
    // System browser via the platform opener — never `plugin-shell open` on
    // Android (it shells out to `xdg-open`: `Scoped shell IO error: No such
    // file or directory (os error 2)`).
    await openExternalUrl(url)
    const deadline = Date.now() + 180_000
    while (Date.now() < deadline) {
      await new Promise((resolve) => setTimeout(resolve, 700))
      const session = await supabaseSession()
      const id = identityFromSession(session)
      if (id) {
        identity.value = id
        recordConnectedAccount(session)
        return id
      }
    }
    throw new Error('Sign-in timed out. Return to CyberManju OS after approving the provider.')
  }
  // Fresh login: end the previous session first so the poll below recognizes
  // ANY arriving session (same-user re-login used to hang forever here,
  // because `id !== before` could never turn true).
  const width = 520
  const height = 640
  const left = Math.max(0, Math.round(window.screen.width / 2 - width / 2))
  const top = Math.max(0, Math.round(window.screen.height / 2 - height / 2))
  let popup: Window | null = null
  try {
    // Reserve from the trusted click, before sign-out or PKCE setup awaits.
    popup = window.open('about:blank', 'cybermanju-signin', `width=${width},height=${height},left=${left},top=${top}`)
    if (popup === window) popup = null
  } catch {
    popup = null
  }
  let url = ''
  try {
    await supabaseSignOut().catch(() => {})
    identity.value = null
    ;({ url } = await startSupabaseSignIn(provider, !!popup, provider === 'google'))
    if (!popup) {
      // Unmarked `?code=` returns boot the full app and complete PKCE there.
      window.location.assign(url)
      return await new Promise<CyberIdentity>(() => {})
    }
    // Preserve the opener so the return page can message and close this popup.
    popup.location.href = url
  } catch (e) {
    try { if (popup && !popup.closed) popup.close() } catch { /* best effort */ }
    throw e
  }
  // Wake up early when the popup reports completion; the shared session in
  // localStorage stays the source of truth.
  let msgDone = false
  const onMsg = (e: MessageEvent) => {
    try {
      if (e?.data?.type === 'cybermanju:oauth-done') msgDone = true
    } catch {
      // Ignore malformed messages.
    }
  }
  window.addEventListener('message', onMsg)
  try {
    const deadline = Date.now() + 180_000
    while (Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, 700))
      const session = await supabaseSession()
      const id = identityFromSession(session)
      if (id) {
        try {
          if (!popup.closed) popup.close()
        } catch {
          // Already gone — the popup closes itself too.
        }
        identity.value = id
        recordConnectedAccount(session)
        return id
      }
      // A manually closed popup with no completion message ends the wait;
      // a self-closed popup after notifying keeps polling until the session
      // lands (or the deadline hits).
      if (popup.closed && !msgDone) break
    }
    const session = await supabaseSession()
    const id = identityFromSession(session)
    if (id) {
      identity.value = id
      recordConnectedAccount(session)
      return id
    }
  } finally {
    window.removeEventListener('message', onMsg)
  }
  throw new Error('sign-in did not complete — the popup closed before a session appeared')
}

export async function signOutIdentity(): Promise<void> {
  await supabaseSignOut()
  identity.value = null
}
