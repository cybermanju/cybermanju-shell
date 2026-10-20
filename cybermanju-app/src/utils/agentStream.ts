// CyberManju — agent SSE stream, TS side (pure half).
//
// Mirrors `crates/agent/src/stream.rs`: the native job tail
// (`GET /api/agent/jobs/:id/events`) frames one `event:` + N `data:`
// lines per job snapshot with `: ping` heartbeats and a terminal
// `data: [DONE]`. This module parses those chunks without any fetch,
// DOM, or timer — the impure polling/streaming driver stays at the call
// site (and the 1.5 s poller remains the fallback: browsers using
// `EventSource` cannot send the `Authorization` header).

/** One decoded SSE event. */
export interface AgentStreamEvent {
  event: string
  data: string
}

/** Terminal sentinel the server sends after a final job-state event. */
export function isStreamDone(data: string): boolean {
  return (data ?? '').trim() === '[DONE]'
}

/**
 * Resolve the REST origin for agent event streams. Native IPC and a static
 * WASM host have no local `/events` endpoint; `null` tells the caller to poll
 * through `invoke()` instead of probing localhost. An explicit server URL
 * always wins, including on a static page configured to use a remote server.
 */
export function resolveAgentEventsBase(target: {
  serverUrl: string
  isTauri: boolean
  isStaticHost: boolean
  origin: string
}): string | null {
  const configured = target.serverUrl.trim().replace(/\/+$/, '')
  if (configured) return configured
  if (target.isTauri || target.isStaticHost) return null
  const origin = target.origin.trim().replace(/\/+$/, '')
  return origin || 'http://localhost:3456'
}

/**
 * Decode one SSE chunk into events. Blank lines dispatch; `event:` sets
 * the pending name (last wins); `data:` lines join with `\n`; comments
 * and unknown fields are ignored; dataless dispatches are heartbeats.
 */
export function parseAgentStreamChunk(chunk: string): AgentStreamEvent[] {
  const out: AgentStreamEvent[] = []
  let name = 'message'
  let dataLines: string[] = []
  const flush = () => {
    if (dataLines.length === 0) {
      name = 'message'
      return
    }
    out.push({ event: name, data: dataLines.join('\n') })
    name = 'message'
    dataLines = []
  }
  for (const raw of (chunk ?? '').split('\n')) {
    const line = raw.endsWith('\r') ? raw.slice(0, -1) : raw
    if (line === '') {
      flush()
      continue
    }
    if (line.startsWith(':')) continue
    if (line.startsWith('event:')) {
      const value = line.slice('event:'.length).trim()
      name = /^[A-Za-z0-9_-]+$/.test(value) && value !== '' ? value : 'message'
      continue
    }
    if (line.startsWith('data:')) {
      const value = line.startsWith('data: ')
        ? line.slice('data: '.length)
        : line.slice('data:'.length)
      dataLines.push(value)
      continue
    }
    // Unknown field (`id:`, `retry:`, …) — ignored per the SSE spec.
  }
  flush()
  return out
}
