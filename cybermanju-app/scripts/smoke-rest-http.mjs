// CyberManju OS — REST HTTP smoke harness (scripts/smoke-rest-http.mjs).
// Role: exercise a deliberately bounded set of real server behaviors after the Docker image boots.
// Exports: runHttpBehavioralSmoke; CLI accepts a base URL and readiness timeout.
// Integrates: crates/web routes served by Docker and check-transport-parity.mjs --http.
// Contracts: this is a REST-only behavioral smoke, not full REST/IPC/WASM parity; bootstrap starts with an empty database.

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

async function requestJson(fetchImpl, url, options = {}) {
  const response = await fetchImpl(url, options)
  const text = await response.text()
  let body
  try {
    body = text ? JSON.parse(text) : null
  } catch {
    throw new Error(`${options.method ?? 'GET'} ${url} returned non-JSON HTTP ${response.status}: ${text.slice(0, 200)}`)
  }
  return { response, body }
}

function assertStatus(result, expected, label) {
  if (result.response.status !== expected) {
    throw new Error(`${label}: expected HTTP ${expected}, received HTTP ${result.response.status} (${JSON.stringify(result.body)})`)
  }
}

/** Run real HTTP behavior checks against a freshly started, empty REST server. */
export async function runHttpBehavioralSmoke({
  baseUrl,
  fetchImpl = fetch,
  timeoutMs = 60_000,
  pollIntervalMs = 500,
  log = (message) => console.log(message),
}) {
  let parsedBase
  try {
    parsedBase = new URL(baseUrl)
  } catch {
    throw new Error(`Invalid HTTP smoke base URL: ${baseUrl}`)
  }
  if (!['http:', 'https:'].includes(parsedBase.protocol)) {
    throw new Error(`HTTP smoke URL must use http or https: ${baseUrl}`)
  }
  const base = parsedBase.toString().replace(/\/$/, '')
  const deadline = Date.now() + timeoutMs
  let lastReadinessError = 'no response'
  let isReady = false

  // Readiness is stronger than liveness: DB, search index and writable data directory must all work.
  while (Date.now() < deadline) {
    try {
      const readiness = await requestJson(fetchImpl, `${base}/api/readyz`, { signal: AbortSignal.timeout(5_000) })
      if (readiness.response.status === 200 && readiness.body?.ready === true) {
        const failingChecks = Object.entries(readiness.body.checks ?? {}).filter(([, value]) => value !== 'ok')
        if (failingChecks.length === 0) {
          isReady = true
          break
        }
        lastReadinessError = `ready response contains failing checks: ${JSON.stringify(failingChecks)}`
      } else {
        lastReadinessError = `GET /api/readyz returned HTTP ${readiness.response.status}: ${JSON.stringify(readiness.body)}`
      }
    } catch (error) {
      lastReadinessError = error instanceof Error ? error.message : String(error)
    }
    await sleep(Math.min(pollIntervalMs, Math.max(0, deadline - Date.now())))
  }
  if (!isReady) throw new Error(`Server did not become ready within ${timeoutMs}ms: ${lastReadinessError}`)
  log('HTTP behavior: /api/readyz reports ready with all checks ok')

  const status = await requestJson(fetchImpl, `${base}/api/auth/status`)
  assertStatus(status, 200, 'GET /api/auth/status')
  if (status.body?.registrationOpen !== true) {
    throw new Error(`Fresh database must allow bootstrap registration; received ${JSON.stringify(status.body)}`)
  }
  log('HTTP behavior: unauthenticated registration status is open on fresh database')

  const anonymousFiles = await requestJson(fetchImpl, `${base}/api/files`)
  assertStatus(anonymousFiles, 401, 'Anonymous GET /api/files')
  log('HTTP behavior: anonymous protected file listing is rejected (401)')

  const suffix = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 9)}`
  const username = `ci-smoke-${suffix}`
  const password = `Smoke-${suffix}-Strong9!`
  const register = await requestJson(fetchImpl, `${base}/api/users/register`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ username, password, displayName: 'CI REST smoke', role: 'user' }),
  })
  assertStatus(register, 201, 'Bootstrap POST /api/users/register')
  if (register.body?.username !== username || register.body?.role !== 'user') {
    throw new Error(`Bootstrap registration returned the wrong identity or role: ${JSON.stringify(register.body)}`)
  }
  if ([register.body?.passwordHash, register.body?.password_hash].some((hash) => typeof hash === 'string' && hash.length > 0)) {
    throw new Error('Bootstrap registration exposed a non-empty password hash')
  }
  log('HTTP behavior: bootstrap creates a real non-admin user without exposing its password hash')

  const closedStatus = await requestJson(fetchImpl, `${base}/api/auth/status`)
  assertStatus(closedStatus, 200, 'GET /api/auth/status after bootstrap')
  if (closedStatus.body?.registrationOpen !== false) {
    throw new Error(`Bootstrap registration should close after first user: ${JSON.stringify(closedStatus.body)}`)
  }

  const secondRegistration = await requestJson(fetchImpl, `${base}/api/users/register`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ username: `${username}-second`, password, role: 'user' }),
  })
  assertStatus(secondRegistration, 403, 'Second bootstrap POST /api/users/register')
  log('HTTP behavior: bootstrap closes after the first registered user (403)')

  const login = await requestJson(fetchImpl, `${base}/api/auth/login`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ username, password }),
  })
  assertStatus(login, 200, 'POST /api/auth/login')
  const token = login.body?.token
  if (typeof token !== 'string' || token.length < 20 || login.body?.role !== 'user') {
    throw new Error(`Login did not return a usable user session: ${JSON.stringify(login.body)}`)
  }
  log('HTTP behavior: registered credentials produce a bearer session')

  const authenticatedFiles = await requestJson(fetchImpl, `${base}/api/files`, {
    headers: { authorization: `Bearer ${token}` },
  })
  assertStatus(authenticatedFiles, 200, 'Authenticated GET /api/files')
  if (!Array.isArray(authenticatedFiles.body)) {
    throw new Error(`Authenticated file listing must be a JSON array, received ${JSON.stringify(authenticatedFiles.body)}`)
  }
  log('HTTP behavior: bearer session reaches authenticated file listing (200 JSON array)')

  const userAdminWrite = await requestJson(fetchImpl, `${base}/api/users`, {
    method: 'POST',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    body: JSON.stringify({ username: `${username}-admin-check`, password, role: 'user' }),
  })
  assertStatus(userAdminWrite, 403, 'Non-admin POST /api/users')
  log('HTTP behavior: authenticated non-admin cannot create accounts (403)')

  return { baseUrl: base, checks: 9 }
}

async function main(argv) {
  const [baseUrl, ...rest] = argv
  let timeoutMs = 60_000
  if (!baseUrl || rest.length > 2 || (rest.length && rest[0] !== '--timeout-ms')) {
    throw new Error('Usage: node scripts/smoke-rest-http.mjs <base-url> [--timeout-ms <milliseconds>]')
  }
  if (rest.length === 2) {
    timeoutMs = Number(rest[1])
    if (!Number.isInteger(timeoutMs) || timeoutMs < 1) throw new Error('--timeout-ms must be a positive integer')
  }
  const result = await runHttpBehavioralSmoke({ baseUrl, timeoutMs })
  console.log(`REST HTTP behavioral smoke: clean (${result.checks} behavior assertions; REST only, not full cross-transport parity).`)
}

if (process.argv[1] && new URL(import.meta.url).pathname === process.argv[1]) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(`REST HTTP behavioral smoke failed: ${error instanceof Error ? error.message : String(error)}`)
    process.exitCode = 1
  })
}
