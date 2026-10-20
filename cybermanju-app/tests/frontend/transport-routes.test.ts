// CyberManju OS — Transport routes.test (tests/frontend/transport-routes.test.ts).
// Transport parity — every invoke() command resolves on web/smoke paths.
//
// Guards the gaps where a Tauri command existed but web/static transports
// threw "not supported": each mapped command must build the server path the
// dashboard actually serves (see crates/web/src/lib.rs), and local-only
// commands must be listed in WRITE_ONLY_COMMANDS instead of 404ing.
import fs from 'node:fs'
import { describe, expect, it } from 'vitest'
import { MOBILE_NATIVE_OS_COMMANDS, REST_FIRST, REST_ROUTES, WRITE_ONLY_COMMANDS } from '@/composables/useTauri'

describe('REST route coverage', () => {
  it('maps suggest to the server suggest endpoint', () => {
    const r = REST_ROUTES.suggest
    expect(r.method).toBe('GET')
    expect(r.buildPath({ prefix: 'va', limit: 5 })).toContain('/api/search/suggest')
    expect(r.buildPath({ prefix: 'va', limit: 5 })).toContain('q=va')
  })

  it('maps share + permission + preview commands to their server routes', () => {
    expect(REST_ROUTES.get_shared_file.buildPath({ token: 'abc' })).toBe('/api/shared/abc')
    expect(REST_ROUTES.set_file_permission.buildPath({})).toBe('/api/permissions')
    expect(REST_ROUTES.get_preview.buildPath({ fileId: 'f1' })).toBe('/api/files/f1/preview')
    expect(REST_ROUTES.upload_remote_file.buildPath({})).toBe('/api/sync/upload')
  })

  it('maps os_write to PUT /api/os/write (P1-6)', () => {
    const r = REST_ROUTES.os_write
    expect(r.method).toBe('PUT')
    expect(r.buildPath({})).toBe('/api/os/write')
    expect(r.transformRequest!({ path: '/note.txt', content: 'hi' })).toEqual({
      path: '/note.txt',
      content: 'hi',
    })
    expect(REST_FIRST.has('os_write')).toBe(true)
  })

  it('routes local mobile telemetry + the cybsh OS layer through native IPC', () => {
    for (const cmd of ['os_ps', 'os_top', 'os_workers', 'os_jobs', 'os_df']) {
      expect(REST_ROUTES[cmd]).toBeTruthy()
      expect(REST_FIRST.has(cmd)).toBe(true)
      expect(MOBILE_NATIVE_OS_COMMANDS.has(cmd)).toBe(true)
    }
    // Mobile starts no dashboard, so the native backend serves the full
    // cybsh OS layer over IPC instead: the terminal, the volume file verbs
    // and the `-os` host verbs all run against the same redb as desktop.
    // Every one of these must stay registered in commands/os_shell.rs +
    // lib.rs (the second half of this test), or the WebView call dies with
    // a localhost:3456 connection refusal.
    for (const cmd of ['os_exec', 'os_complete', 'os_stat', 'os_ls', 'os_du', 'os_write']) {
      expect(MOBILE_NATIVE_OS_COMMANDS.has(cmd)).toBe(true)
    }
    const shell = fs.readFileSync('src-tauri/src/commands/os_shell.rs', 'utf8')
    const runtime = fs.readFileSync('src-tauri/src/lib.rs', 'utf8')
    for (const cmd of ['os_exec', 'os_complete', 'os_stat', 'os_ls', 'os_du', 'os_write']) {
      expect(shell).toMatch(new RegExp(`pub fn ${cmd}\\b`))
      expect(runtime).toContain(`commands::os_shell::${cmd}`)
    }
  })

  it('registers every mobile telemetry IPC command in the Tauri backend', () => {
    const commands = fs.readFileSync('src-tauri/src/commands/os_metrics.rs', 'utf8')
    const runtime = fs.readFileSync('src-tauri/src/lib.rs', 'utf8')
    for (const cmd of ['os_ps', 'os_top', 'os_workers', 'os_jobs', 'os_df']) {
      expect(commands).toMatch(new RegExp(`pub fn ${cmd}\\b`))
      expect(runtime).toContain(`commands::os_metrics::${cmd}`)
    }
  })

  it('routes mobile sync status through registered native IPC', () => {
    expect(MOBILE_NATIVE_OS_COMMANDS.has('get_sync_status')).toBe(true)
    const commands = fs.readFileSync('src-tauri/src/commands/sync.rs', 'utf8')
    const runtime = fs.readFileSync('src-tauri/src/lib.rs', 'utf8')
    expect(commands).toMatch(/pub fn get_sync_status\b/)
    expect(runtime).toContain('sync_cmd::get_sync_status')
  })

  it('serves provider quota over native IPC (no dashboard needed)', () => {
    // `get_sync_usage` is a pure function of the saved config, so Android
    // gets real quota over IPC. `oauth_start` stays REST-only by design —
    // it mints dashboard redirect URLs and needs the server OAuth client.
    expect(REST_ROUTES.get_sync_usage).toBeTruthy()
    expect(REST_FIRST.has('get_sync_usage')).toBe(true)
    expect(MOBILE_NATIVE_OS_COMMANDS.has('get_sync_usage')).toBe(true)
    const commands = fs.readFileSync('src-tauri/src/commands/sync.rs', 'utf8')
    const runtime = fs.readFileSync('src-tauri/src/lib.rs', 'utf8')
    expect(commands).toMatch(/pub fn get_sync_usage\b/)
    expect(runtime).toContain('sync_cmd::get_sync_usage')
    expect(MOBILE_NATIVE_OS_COMMANDS.has('oauth_start')).toBe(false)
  })

  it('encodes OS volume paths as ?path= (P1-7)', () => {
    expect(REST_ROUTES.os_stat.buildPath({ path: '/foo bar' })).toBe(
      '/api/os/stat?path=%2Ffoo%20bar',
    )
    expect(REST_ROUTES.os_ls.buildPath({ path: '/a/b' })).toBe('/api/os/ls?path=%2Fa%2Fb')
    expect(REST_ROUTES.os_du.buildPath({})).toBe('/api/os/du?path=%2F')
    // Parent traversal survives as data, not as URL segments.
    expect(REST_ROUTES.os_stat.buildPath({ path: '/x/../y' })).toBe(
      '/api/os/stat?path=%2Fx%2F..%2Fy',
    )
  })

  it('keeps the whole sync domain REST_FIRST (P1-5)', () => {
    for (const cmd of [
      'list_sync_configs',
      'create_sync_config',
      'delete_sync_config',
      'start_sync',
      'cancel_sync',
      'get_sync_progress',
      'test_sync_connection',
      'list_remote_files',
      'get_sync_job',
      'list_sync_runs',
      'get_sync_status',
      'restore_sync_file',
      'delete_remote_file',
      'create_provider_repo',
      'seed_repo_files',
      'upload_remote_file',
      'get_sync_usage',
      'oauth_start',
    ]) {
      expect(REST_ROUTES[cmd]).toBeTruthy()
      expect(REST_FIRST.has(cmd)).toBe(true)
    }
  })

  it('keeps the paginated search REST route for the wasm unwrap (P1-8)', () => {
    const r = REST_ROUTES.search_files_paginated
    expect(r.method).toBe('GET')
    expect(r.buildPath({ query: 'vault', limit: 20, offset: 40 })).toContain('/api/search/paginated')
  })

  it('routes op-log resolve + LAN peer sync over REST (spine close-out)', () => {
    const resolve = REST_ROUTES.oplog_resolve
    expect(resolve.method).toBe('POST')
    expect(resolve.buildPath({})).toBe('/api/oplog/resolve')
    expect(resolve.transformRequest!({ opA: 'a-1', opB: 'b-2', choice: 'merge' })).toMatchObject({
      opA: 'a-1',
      opB: 'b-2',
      choice: 'merge',
    })
    expect(REST_FIRST.has('oplog_resolve')).toBe(true)
    const sync = REST_ROUTES.peer_sync
    expect(sync.method).toBe('POST')
    expect(sync.buildPath({ id: 'phone-7' })).toBe('/api/peers/phone-7/sync')
    expect(sync.transformRequest!({ id: 'phone-7', ops: [] })).toMatchObject({ ops: [] })
    expect(REST_FIRST.has('peer_sync')).toBe(true)
    // Op-log reads stay REST-first alongside the new write arms.
    for (const cmd of ['oplog_since', 'oplog_push', 'oplog_conflicts']) {
      expect(REST_ROUTES[cmd], cmd).toBeTruthy()
      expect(REST_FIRST.has(cmd), cmd).toBe(true)
    }
  })
  it('routes the scheduler family over REST + native IPC (Phase 1)', () => {
    const cmds = [
      'cron_list',
      'cron_save',
      'cron_delete',
      'cron_run',
      'cron_history',
      'cron_set_enabled',
      'cron_ensure_started',
    ] as const
    for (const cmd of cmds) {
      expect(REST_ROUTES[cmd], cmd).toBeTruthy()
      expect(REST_FIRST.has(cmd), cmd).toBe(true)
      expect(MOBILE_NATIVE_OS_COMMANDS.has(cmd), cmd).toBe(true)
    }
    expect(REST_ROUTES.cron_list.buildPath({})).toBe('/api/cron')
    expect(REST_ROUTES.cron_save.transformRequest!({ row: { path: '/j.cybsh', expr: '@hourly' } })).toMatchObject({
      path: '/j.cybsh',
      expr: '@hourly',
      enabled: true,
    })
    expect(REST_ROUTES.cron_delete.buildPath({ id: 'sched-1' })).toBe('/api/cron/sched-1')
    expect(REST_ROUTES.cron_run.buildPath({ id: 'sched-1' })).toBe('/api/cron/sched-1/run')
    expect(REST_ROUTES.cron_history.buildPath({ id: 'sched-1' })).toBe('/api/cron/sched-1/runs')
    expect(REST_ROUTES.cron_set_enabled.buildPath({ id: 'sched-1', enabled: true })).toBe('/api/cron/sched-1/enable')
    expect(REST_ROUTES.cron_set_enabled.buildPath({ id: 'sched-1', enabled: false })).toBe('/api/cron/sched-1/disable')
    // Daemon arming is a side effect of the first GET; the client only sees `true`.
    expect(REST_ROUTES.cron_ensure_started.transformResponse!(null, {})).toBe(true)
    // Desktop/mobile IPC handlers must stay wired in the Tauri backend.
    const commands = fs.readFileSync('src-tauri/src/commands/cron.rs', 'utf8')
    const runtime = fs.readFileSync('src-tauri/src/lib.rs', 'utf8')
    for (const cmd of ['cron_list', 'cron_save', 'cron_delete', 'cron_run', 'cron_history']) {
      expect(commands).toMatch(new RegExp(`pub fn ${cmd}\\b`))
      expect(runtime).toContain(`commands::cron::${cmd}`)
    }
  })

  it('routes the secrets keystore over REST + native IPC + wasm (Phase 3)', () => {
    const cmds = [
      'secret_list',
      'secret_save',
      'secret_update',
      'secret_get',
      'secret_reveal',
      'secret_delete',
    ] as const
    for (const cmd of cmds) {
      expect(REST_ROUTES[cmd], cmd).toBeTruthy()
      expect(REST_FIRST.has(cmd), cmd).toBe(true)
      expect(MOBILE_NATIVE_OS_COMMANDS.has(cmd), cmd).toBe(true)
    }
    expect(REST_ROUTES.secret_list.buildPath({})).toBe('/api/secrets')
    expect(REST_ROUTES.secret_get.buildPath({ id: 's-1' })).toBe('/api/secrets/s-1')
    expect(REST_ROUTES.secret_reveal.buildPath({ id: 's-1' })).toBe('/api/secrets/s-1/reveal')
    expect(REST_ROUTES.secret_delete.buildPath({ id: 's-1' })).toBe('/api/secrets/s-1')
    // Save sends the plaintext `value` (sealed server-side, never echoed).
    const body = REST_ROUTES.secret_save.transformRequest!({
      request: { title: 'GitHub', kind: 'login', value: 'hunter2', username: 'octocat' },
    })
    expect(body).toMatchObject({ title: 'GitHub', kind: 'login', value: 'hunter2', username: 'octocat' })
    // Desktop/mobile IPC handlers stay wired in the Tauri backend.
    const commands = fs.readFileSync('src-tauri/src/commands/secrets.rs', 'utf8')
    const runtime = fs.readFileSync('src-tauri/src/lib.rs', 'utf8')
    for (const cmd of ['secret_list', 'secret_save', 'secret_get', 'secret_reveal', 'secret_delete']) {
      expect(commands).toMatch(new RegExp(`pub fn ${cmd}\\b`))
      expect(runtime).toContain(`commands::secrets::${cmd}`)
    }
    // REST segment is registered (404-before-401 gate) and Admin-gated for
    // mutations/reveal — list stays Authenticated.
    const security = fs.readFileSync('crates/web/src/security.rs', 'utf8')
    expect(security).toContain('"secrets"')
    expect(security).toContain('["api", "secrets", _, "reveal"] => RequiredRole::Admin')
  })

  it('keeps local-only commands in WRITE_ONLY instead of 404ing', () => {
    for (const cmd of ['get_compression_stats', 'get_symbols', 'parse_file', 'upload_file']) {
      expect(WRITE_ONLY_COMMANDS.has(cmd)).toBe(true)
    }
    for (const cmd of ['suggest', 'get_shared_file', 'set_file_permission', 'get_preview']) {
      expect(WRITE_ONLY_COMMANDS.has(cmd)).toBe(false)
      expect(REST_ROUTES[cmd]).toBeTruthy()
    }
  })

  it('routes face detection to the server faces endpoints (heuristic everywhere)', () => {
    const single = REST_ROUTES.detect_faces
    expect(single.method).toBe('POST')
    expect(single.buildPath({ fileId: 'f1' })).toBe('/api/faces/detect')
    expect(single.transformRequest!({ fileId: 'f1' })).toEqual({ fileId: 'f1' })
    const batch = REST_ROUTES.detect_faces_batch_cmd
    expect(batch.method).toBe('POST')
    expect(batch.buildPath({})).toBe('/api/faces/detect-batch')
    expect(WRITE_ONLY_COMMANDS.has('detect_faces')).toBe(false)
    expect(WRITE_ONLY_COMMANDS.has('detect_faces_batch_cmd')).toBe(false)
    // Grouping admin ops stay desktop-only.
    for (const cmd of ['recluster_faces', 'rename_face_group', 'merge_face_groups', 'delete_face_group', 'find_similar_faces']) {
      expect(WRITE_ONLY_COMMANDS.has(cmd)).toBe(true)
    }
  })
})

describe('REST response transforms', () => {
  it('passes real BM25 scores and snippets through search_files', () => {
    const out = REST_ROUTES.search_files.transformResponse!(
      [{ fileId: 'a', fileName: 'a.txt', score: 2.5, snippet: 'hit' }],
      {},
    ) as Array<Record<string, unknown>>
    expect(out[0].score).toBe(2.5)
    expect(out[0].snippet).toBe('hit')
  })

  it('falls back cleanly when the server sends file-shaped rows', () => {
    const out = REST_ROUTES.search_files.transformResponse!(
      [{ id: 'a', name: 'a.txt' }],
      {},
    ) as Array<Record<string, unknown>>
    expect(out[0]).toMatchObject({ fileId: 'a', fileName: 'a.txt', score: 1.0, snippet: '' })
  })

  it('passes real encryption status through instead of hardcoding false', () => {
    const out = REST_ROUTES.get_encryption_status.transformResponse!(
      { isEncrypted: true, algorithm: 'hybrid', available: true },
      {},
    ) as Record<string, unknown>
    expect(out.isEncrypted).toBe(true)
    expect(out.algorithm).toBe('hybrid')
  })

  it('routes vault backup export over REST + native IPC + wasm', () => {
    expect(REST_ROUTES.backup_export).toBeTruthy()
    expect(REST_FIRST.has('backup_export')).toBe(true)
    expect(MOBILE_NATIVE_OS_COMMANDS.has('backup_export')).toBe(true)
    expect(REST_ROUTES.backup_export.buildPath({})).toBe('/api/backup/export')
    // Desktop/mobile IPC handler stays wired in the Tauri backend.
    const commands = fs.readFileSync('src-tauri/src/commands/vault_snapshot.rs', 'utf8')
    const runtime = fs.readFileSync('src-tauri/src/lib.rs', 'utf8')
    expect(commands).toMatch(/pub fn backup_export\b/)
    expect(runtime).toContain('commands::vault_snapshot::backup_export')
    // REST segment is registered (404-before-401 gate) and Admin-gated —
    // the image holds sealed secrets and transcripts.
    const security = fs.readFileSync('crates/web/src/security.rs', 'utf8')
    expect(security).toContain('"backup"')
    expect(security).toContain('["api", "backup", "export"] => RequiredRole::Admin')
  })
})
