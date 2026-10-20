// CyberManju OS — REST smoke harness tests (scripts/smoke-rest-http.test.mjs).
// Role: verify the real-HTTP smoke harness accepts the expected API contract and fails on security/readiness regressions.
// Exports: none (node:test suite).
// Integrates: scripts/smoke-rest-http.mjs; ephemeral local HTTP servers only, no Rust or Docker required.
// Contracts: mocks test harness behavior only and do not claim the Rust server passed these cases.

import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createServer } from 'node:http'
import { afterEach, test } from 'node:test'
import { runHttpBehavioralSmoke } from './smoke-rest-http.mjs'

const servers = new Set()

async function startServer(handler) {
  const server = createServer((request, response) => {
    let body = ''
    request.setEncoding('utf8')
    request.on('data', (chunk) => { body += chunk })
    request.on('end', () => handler(request, response, body))
  })
  await new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', resolve)
  })
  servers.add(server)
  return `http://127.0.0.1:${server.address().port}`
}

function json(response, status, value) {
  response.writeHead(status, { 'content-type': 'application/json' })
  response.end(JSON.stringify(value))
}

afterEach(async () => {
  await Promise.all([...servers].map((server) => new Promise((resolve) => server.close(resolve))))
  servers.clear()
})

test('deployment readiness probes use GET rather than wget spider HEAD', () => {
  const dockerfile = readFileSync(new URL('../Dockerfile', import.meta.url), 'utf8')
  const compose = readFileSync(new URL('../docker-compose.yml', import.meta.url), 'utf8')
  assert.match(dockerfile, /CMD wget -q -O \/dev\/null http:\/\/localhost:3456\/api\/readyz \|\| exit 1/)
  assert.match(compose, /test: \["CMD", "wget", "-q", "-O", "\/dev\/null", "http:\/\/localhost:3456\/api\/readyz"\]/)
})

test('passes bounded readiness, bootstrap, login, authenticated and admin-gate behaviors', async () => {
  let registered = false
  let password
  const token = 'test-bearer-token-long-enough-for-smoke'
  const baseUrl = await startServer((request, response, bodyText) => {
    const { method, url, headers } = request
    if (method === 'GET' && url === '/api/readyz') return json(response, 200, { ready: true, checks: { database: 'ok', searchIndex: 'ok', diskWritable: 'ok' } })
    if (method === 'GET' && url === '/api/auth/status') return json(response, 200, { registrationOpen: !registered })
    if (method === 'GET' && url === '/api/files') {
      return headers.authorization === `Bearer ${token}` ? json(response, 200, []) : json(response, 401, { error: 'auth: unauthorized' })
    }
    if (method === 'POST' && url === '/api/users/register') {
      const body = JSON.parse(bodyText)
      if (registered) return json(response, 403, { error: 'Registration is closed' })
      registered = true
      password = body.password
      return json(response, 201, { username: body.username, role: 'user', passwordHash: '' })
    }
    if (method === 'POST' && url === '/api/auth/login') {
      const body = JSON.parse(bodyText)
      return body.password === password
        ? json(response, 200, { token, role: 'user' })
        : json(response, 401, { error: 'Invalid credentials' })
    }
    if (method === 'POST' && url === '/api/users') return json(response, headers.authorization === `Bearer ${token}` ? 403 : 401, { error: 'Forbidden' })
    return json(response, 404, { error: 'not found' })
  })

  const logs = []
  const result = await runHttpBehavioralSmoke({ baseUrl, log: (message) => logs.push(message) })
  assert.equal(result.checks, 9)
  assert.equal(logs.length, 8)
  assert.match(logs.at(-1), /non-admin/)
})

test('fails closed when the health readiness endpoint stays unavailable', async () => {
  const baseUrl = await startServer((_request, response) => json(response, 503, { ready: false, checks: { database: 'error' } }))
  await assert.rejects(
    runHttpBehavioralSmoke({ baseUrl, timeoutMs: 40, pollIntervalMs: 5, log: () => {} }),
    /did not become ready.*HTTP 503/,
  )
})

test('fails when protected files are accessible without credentials', async () => {
  const baseUrl = await startServer((request, response) => {
    if (request.url === '/api/readyz') return json(response, 200, { ready: true, checks: { database: 'ok' } })
    if (request.url === '/api/auth/status') return json(response, 200, { registrationOpen: true })
    if (request.url === '/api/files') return json(response, 200, [])
    return json(response, 404, {})
  })
  await assert.rejects(
    runHttpBehavioralSmoke({ baseUrl, log: () => {} }),
    /Anonymous GET \/api\/files: expected HTTP 401, received HTTP 200/,
  )
})

test('fails when bootstrap registration grants an admin role', async () => {
  const baseUrl = await startServer((request, response, bodyText) => {
    if (request.url === '/api/readyz') return json(response, 200, { ready: true, checks: { database: 'ok' } })
    if (request.url === '/api/auth/status') return json(response, 200, { registrationOpen: true })
    if (request.method === 'GET' && request.url === '/api/files') return json(response, 401, {})
    if (request.method === 'POST' && request.url === '/api/users/register') {
      const body = JSON.parse(bodyText)
      return json(response, 201, { username: body.username, role: 'admin', passwordHash: '' })
    }
    return json(response, 404, {})
  })
  await assert.rejects(
    runHttpBehavioralSmoke({ baseUrl, log: () => {} }),
    /wrong identity or role/,
  )
})
