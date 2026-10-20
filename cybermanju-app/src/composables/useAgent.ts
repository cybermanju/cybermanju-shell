// CyberManju — browser-local agent loop (static-host / Pages transport).
//
// The WASM dispatcher exposes single-turn primitives (`agent_catalog`,
// `agent_prompt`); this composable drives the multi-turn loop entirely in
// the browser: transcript, tool execution against the volume dispatcher,
// ask-approval parking, usage accounting, and localStorage transcripts.
// Provider keys live in memory only — they are never written to storage.
//
// Mirrors the native permission semantics in `crates/agent/src/config.rs`
// (last-match-wins wildcards, plan denies mutations, deny beats auto).
// Cross-device parity: `bash` runs the cybsh volume subset locally
// (`wasmOsDispatch('exec')`); device-only verbs proxy to the dashboard when
// reachable, else answer `unsupported:`. `task` runs one bounded subagent
// (depth-1, 5 turns, full file/shell toolset) — foreground waits inline,
// `background:true` detaches (id now, results injected as new messages
// while the parent keeps working, waited for at the end; 8 concurrent max,
// totals unbounded). `mcp__*` runs Streamable-HTTP servers
// over `fetch`; `stdio` servers need desktop/Docker and answer `unsupported:`.

import { ref } from 'vue'
import { wasmAgentCatalog, wasmAgentPrompt, type WasmAgentTurn } from './useWasmBackend'
import { wasmDbDispatch, wasmModuleExports, wasmOsDispatch } from './useWasmBackend'
import { prepareWireMessages } from '@/utils/agentUi'
import { rankResearchPaths } from '@/utils/researchRank'
import { cleanToolOutput } from '@/utils/redact'
import { LocalMemoryStore, MEMORY_TEXT_CAP_CHARS, renderRecallBlock, type MemoryStorage } from '@/utils/memory'
import type {
  AgentConfig,
  AgentJob,
  AgentMemory,
  AgentSession,
  ChatMessage,
  McpServerConfig,
  MemoryHit,
  PermissionRuleset,
  ProviderPreset,
  TokenUsage,
  ToolCall,
} from '@/types'

const SESSIONS_KEY = 'cybermanju.agent.sessions.v1'
export const LOCAL_CONFIGS_KEY = 'cybermanju.agent.configs.v1'
const CONFIGS_KEY = LOCAL_CONFIGS_KEY
const MEMORIES_KEY = 'cybermanju.agent.memories.v1'

/** Shared "active assistant" selection — AgentPanel + CodeStudio read/write
 *  the same id so switching configs in one panel follows in the other. */
export const CHAT_CONFIG_ID_KEY = 'cybermanju.agent.chatConfigId.v1'
export const CHAT_CONFIG_EVENT = 'cybermanju:agent-chatconfig-changed'
export const AGENT_CONFIGS_EVENT = 'cybermanju:agent-configs-changed'
export const AGENT_KEYS_EVENT = 'cybermanju:agent-keys-changed'

export function readChatConfigId(): string {
  try {
    return localStorage.getItem(CHAT_CONFIG_ID_KEY) ?? ''
  } catch {
    return ''
  }
}

export function writeChatConfigId(id: string): void {
  try {
    if (id) localStorage.setItem(CHAT_CONFIG_ID_KEY, id)
    else localStorage.removeItem(CHAT_CONFIG_ID_KEY)
  } catch {
    // Private mode — selection simply doesn't persist.
  }
  try {
    window.dispatchEvent(new CustomEvent<string>(CHAT_CONFIG_EVENT, { detail: id }))
  } catch {
    // Non-DOM test env.
  }
}

function emitAgentEvent(name: string): void {
  try {
    window.dispatchEvent(new CustomEvent(name))
  } catch {
    // Non-DOM test env.
  }
}

/** Bumped whenever an in-memory provider key is sealed/forgotten so panels
 *  whose `hasKey` badges depend on the non-reactive vault can refresh. */
export const localKeyRevision = ref(0)

/** Native parity: one nesting level, five turns, 64KiB tool cap. */
export const BROWSER_TASK_MAX_DEPTH = 1
export const BROWSER_SUBAGENT_MAX_TURNS = 5
const TOOL_OUTPUT_CAP = 65536
const SUBAGENT_SUMMARY_CAP = 4000
/** Subagent toolset: full file/shell tools plus memory recall/remember (available, never required). Never nested task, questions, or MCP. */
const SUBAGENT_TOOLS = new Set(['read', 'list', 'grep', 'glob', 'self_research', 'repo_analyze', 'edit', 'write', 'bash', 'memory_recall', 'memory_remember'])

/** Max detached background subagents per run (native `MAX_BG_SUBAGENTS` parity).
 *  Only *concurrent* workers are capped — finished slots are reusable, so a
 *  run may fan out as widely as the work needs. */
export const MAX_BG_SUBAGENTS = 8

/** Tool result for a detached `task background:true` spawn — the id the
 *  completion arrives under. Byte-identical wording to native
 *  `agent_loop::bg_started_message` so the model learns one shape. */
export function bgStartedMessage(id: string, goal: string): string {
  return `background subagent ${id} started for \`${goal}\` — keep working; its result arrives automatically as a new message.`
}

/** Transcript injection for a finished background subagent (native
 *  `agent_loop::bg_finished_message` parity). */
export function bgFinishedMessage(id: string, result: string): string {
  return `background subagent ${id} finished:\n${result}`
}

/** One detached background subagent: the promise plus its id. Totals are
 *  unbounded — only *concurrent* workers are capped (native `BgSubagent`
 *  parity); finished slots are reusable so the parent may fan out as widely
 *  as the work needs. */
export type BgEntry = {
  id: string
  goal: string
  promise: Promise<string>
  result: string | null
  settled: boolean
}

/** Honest refusal when all background slots are busy (native `run_task` parity). */
export function bgBusyMessage(running: number): string {
  return `busy: ${running} background subagents already running (cap ${MAX_BG_SUBAGENTS}) — wait for results to arrive, or use a foreground task`
}

/** Finished workers, harvested without blocking. */
export function sweepBgTasks(pending: BgEntry[]): Array<{ id: string; result: string }> {
  const done: Array<{ id: string; result: string }> = []
  for (let i = pending.length - 1; i >= 0; i--) {
    if (pending[i].settled) {
      const entry = pending.splice(i, 1)[0]
      done.push({ id: entry.id, result: entry.result ?? '(subagent produced no text)' })
    }
  }
  // Preserve spawn order for stable transcript injection.
  return done.reverse()
}

/** Wait until every entry settles (or the run aborts). Waiting itself never
 *  ends a run — the caller injects the results and keeps looping. */
async function waitBgSettled(pending: BgEntry[]): Promise<void> {
  while (pending.some(p => !p.settled)) {
    if (abortRequested) return
    await new Promise(r => setTimeout(r, 200))
  }
}

/** Shell-style wildcard: `*` spans any run, `?` exactly one char. */
export function matchWildcard(pattern: string, input: string): boolean {
  const px = pattern
  const memo = new Map<string, boolean>()
  function go(pi: number, ii: number): boolean {
    const key = `${pi}:${ii}`
    const hit = memo.get(key)
    if (hit !== undefined) return hit
    let out: boolean
    if (pi >= px.length) {
      out = ii >= input.length
    } else if (px[pi] === '*') {
      let p = pi
      while (p < px.length && px[p] === '*') p++
      out = false
      for (let skip = ii; skip <= input.length; skip++) {
        if (go(p, skip)) {
          out = true
          break
        }
      }
    } else if (px[pi] === '?') {
      out = ii < input.length && go(pi + 1, ii + 1)
    } else {
      out = ii < input.length && input[ii] === px[pi] && go(pi + 1, ii + 1)
    }
    memo.set(key, out)
    return out
  }
  return go(0, 0)
}

/** Glob matcher mirroring Rust `config::match_glob`: `*`/`?` stay inside
 *  one segment, `**` crosses separators. */
export function matchGlob(pattern: string, path: string): boolean {
  const trim = (s: string) => s.replace(/\\/g, '/').split('/').filter(Boolean)
  const px = trim(pattern.length ? pattern : '**')
  const ix = trim(path)
  function go(pi: number, ii: number): boolean {
    if (pi >= px.length) return ii >= ix.length
    if (px[pi] === '**') {
      let p = pi + 1
      while (p < px.length && px[p] === '**') p++
      for (let skip = ii; skip <= ix.length; skip++) {
        if (go(p, skip)) return true
      }
      return false
    }
    if (ii >= ix.length) return false
    return matchWildcard(px[pi], ix[ii]) && go(pi + 1, ii + 1)
  }
  return go(0, 0)
}

/** Regex when it compiles, literal substring when it does not. */
export function compileGrep(pattern: string): { test: (line: string) => boolean; regex: boolean } {
  try {
    const re = new RegExp(pattern)
    return { test: line => re.test(line), regex: true }
  } catch {
    return { test: line => line.includes(pattern), regex: false }
  }
}

function matchInput(tool: string, input: Record<string, unknown>): string {
  const arg = salientArg(input)
  return arg ? `${tool} ${arg}` : tool
}

function salientArg(input: Record<string, unknown>): string {
  return (
    (input.command as string) ??
    (input.pattern as string) ??
    (input.path as string) ??
    (input.glob as string) ??
    (input.query as string) ??
    (input.url as string) ??
    (input.repo as string) ??
    (input.name as string) ??
    (input.text as string) ??
    ''
  )
}

export type LocalDecision = { kind: 'allow' } | { kind: 'ask'; summary: string } | { kind: 'deny'; reason: string }

/** Mirror of Rust `config::decide` — keep semantics identical. */
export function decideLocalTool(
  rules: PermissionRuleset,
  agentKind: 'build' | 'plan',
  tool: string,
  input: Record<string, unknown>,
): LocalDecision {
  // Plan persona: read-only. skill_save writes a file, mcp_attach mutates
  // the config, os_exec runs volume commands — all denied like edit/write/
  // bash. self_research and repo_analyze are read-only and stay available.
  if (agentKind === 'plan' && (tool === 'edit' || tool === 'write' || tool === 'bash' || tool === 'skill_save' || tool === 'mcp_attach' || tool === 'os_exec' || tool === 'secret_list' || tool === 'secret_get' || tool === 'todo_add' || tool === 'todo_done')) {
    return { kind: 'deny', reason: `deny: plan agent may not run \`${tool}\`` }
  }
  const rule = rules.rules[tool]
  let action = rules.default
  if (typeof rule === 'string') {
    action = rule
  } else if (Array.isArray(rule)) {
    const target = matchInput(tool, input)
    const arg = salientArg(input)
    for (const [pattern, act] of rule) {
      if (
        matchWildcard(pattern, target) ||
        matchWildcard(pattern, tool) ||
        (arg !== '' && matchWildcard(pattern, arg))
      ) {
        action = act
      }
    }
  }
  if (action === 'allow') {
    // Standing orders never auto-allow: a `write`/`edit` that would pass
    // silently under an `allow` rule is downgraded to a human decision
    // (Hermes applies the same guard even under `--yolo`). `deny` still wins.
    if (
      (tool === 'write' || tool === 'edit') &&
      isProtectedInstructionPath(String(input.path ?? ''))
    ) {
      return {
        kind: 'ask',
        summary: `Approve \`${tool}\` — protected standing orders`,
      }
    }
    return { kind: 'allow' }
  }
  if (action === 'deny') return { kind: 'deny', reason: `deny: \`${tool}\` is denied by the permission ruleset` }
  return { kind: 'ask', summary: `Approve \`${tool}\`?` }
}

/** Mirror of Rust `config::remember_allow`: "allow always" becomes an
 *  explicit, visible rule on the tool — never an invisible always-list. */
export function rememberAllowLocal(rules: PermissionRuleset, tool: string): void {
  rules.rules[tool] = 'allow'
}

// ─── standing orders + refusal circuit breaker ───────────────────

/** Basenames that are the agent's standing orders — they get folded into the
 *  system prompt, so whoever rewrites them owns every later turn. */
const PROTECTED_INSTRUCTION_BASENAMES = new Set(['agents.md', 'skill.md'])

/** Mirror of Rust `config::is_protected_instruction_path` (Hermes
 *  `security.protected_instruction_files`): case-insensitive basename in any
 *  directory, plus the `.cybermanju/rules.md` path form. */
export function isProtectedInstructionPath(path: string): boolean {
  const norm = String(path || '').replace(/\\/g, '/').toLowerCase()
  const base = norm.slice(norm.lastIndexOf('/') + 1)
  if (PROTECTED_INSTRUCTION_BASENAMES.has(base)) return true
  return /(^|\/)\.cybermanju\/rules\.md$/.test(norm)
}

/** AUTO APPROVE is the exact state a prompt injection aims for, so it never
 *  covers a standing-orders write: that call fails closed with a denial the
 *  model can act on. Returns the denial text, or `null` when the call is an
 *  ordinary ask (parked for the human as usual). */
export function protectedAutoApproveDenial(
  tool: string,
  input: Record<string, unknown>,
): string | null {
  if (tool !== 'write' && tool !== 'edit') return null
  if (!isProtectedInstructionPath(String(input.path ?? ''))) return null
  return (
    'deny: protected instruction file (AGENTS.md / SKILL.md / .cybermanju/rules.md) — ' +
    'AUTO APPROVE never covers standing orders; disable AUTO APPROVE to write one'
  )
}

/** Consecutive refusals that stop the loop from asking again — after this
 *  many `no`s on the same tool, re-prompting is nagging, not consent
 *  (Hermes `approvals.denial_breaker_threshold`, same default). */
export const DENIAL_BREAKER_THRESHOLD = 3

export class DenialBreaker {
  private readonly counts = new Map<string, number>()

  /** `refused` false resets the tool — one approval clears the tally. */
  record(tool: string, refused: boolean): void {
    if (refused) this.counts.set(tool, (this.counts.get(tool) ?? 0) + 1)
    else this.counts.delete(tool)
  }

  isOpen(tool: string): boolean {
    return (this.counts.get(tool) ?? 0) >= DENIAL_BREAKER_THRESHOLD
  }

  reason(tool: string): string {
    return (
      `denied: \`${tool}\` was refused ${DENIAL_BREAKER_THRESHOLD} times in a row — the approval ` +
      'breaker is open; stop retrying it, take a different approach, or edit the permission ruleset'
    )
  }
}

// ─── volume tools (WASM dispatcher) ─────────────────────────────

async function wasmRead(path: string): Promise<string> {
  const res = (await wasmOsDispatch('exec', { line: `cat "${path}"` })) as {
    ok: boolean
    output: string
  }
  if (!res.ok) throw new Error(res.output || `not_found: ${path}`)
  return res.output
}

async function wasmWrite(path: string, content: string): Promise<string> {
  const res = (await wasmOsDispatch('write', { path, content })) as {
    ok: boolean
    output: string
  }
  if (!res.ok) throw new Error(res.output || 'write failed')
  return res.output
}

async function wasmList(path: string): Promise<string> {
  const res = (await wasmOsDispatch('ls', { path })) as { ok: boolean; output: string } | string[]
  if (Array.isArray(res)) return res.join('\n')
  if (!res.ok) throw new Error(res.output || `not_found: ${path}`)
  return res.output
}

/** The dispatcher's empty-directory sentence is for humans, never a filename. */
export const EMPTY_DIR_NOTE = '(empty directory)'

export function splitVolumeListing(output: string): string[] {
  return output
    .split('\n')
    .map(s => s.trim())
    .filter(s => s && s !== EMPTY_DIR_NOTE)
}

/** Cap tool output like native `TOOL_OUTPUT_CAP` (char-boundary safe). */
function capOutput(text: string): string {
  if (text.length <= TOOL_OUTPUT_CAP) return text
  return `${text.slice(0, TOOL_OUTPUT_CAP)}\n… truncated at 64 KiB`
}

/** Split `mcp__server__tool` back into parts (mirrors `mcp::split_tool_name`). */
export function splitMcpToolName(name: string): [string, string] | null {
  const rest = name.startsWith('mcp__') ? name.slice('mcp__'.length) : null
  if (!rest) return null
  const i = rest.indexOf('__')
  if (i <= 0 || i + 2 >= rest.length) return null
  const server = rest.slice(0, i)
  const tool = rest.slice(i + 2)
  if (!server || !tool || server.includes('__') || tool.includes('__')) return null
  return [server, tool]
}

/** MCP servers attached to a browser-local config (localStorage). */
export function localMcpServers(configId: string): Record<string, McpServerConfig> {
  try {
    const all = JSON.parse(localStorage.getItem(CONFIGS_KEY) ?? '[]') as AgentConfig[]
    return all.find(c => c.id === configId)?.mcpServers ?? {}
  } catch {
    return {}
  }
}

/** SSE `data:` lines → JSON values (skips pings, `[DONE]`, unparseable). */
function parseSseDataLines(body: string): unknown[] {
  const out: unknown[] = []
  for (const line of body.split('\n')) {
    const t = line.trim()
    if (!t || t.startsWith(':')) continue
    const data = t.startsWith('data:') ? t.slice(5).trim() : t
    if (!data || data === '[DONE]') continue
    try {
      out.push(JSON.parse(data))
    } catch {
      // Heartbeats never kill the run.
    }
  }
  return out
}

const mcpSessionCache = new Map<string, string>()

/** One Streamable-HTTP MCP round-trip over `fetch` (native parity). */
async function mcpHttpRoundtrip(
  url: string,
  headers: Array<[string, string]>,
  method: string,
  params: Record<string, unknown>,
  id: number,
): Promise<{ value: unknown; sessionId?: string }> {
  const h = new Headers()
  h.set('Content-Type', 'application/json')
  h.set('Accept', 'application/json, text/event-stream')
  for (const [k, v] of headers) {
    try {
      h.set(k, v)
    } catch {
      // Bad header names never kill the run.
    }
  }
  const cached = mcpSessionCache.get(url)
  if (cached) {
    try {
      h.set('Mcp-Session-Id', cached)
    } catch {
      // Ignore.
    }
  }
  const ctrl = new AbortController()
  const timer = setTimeout(() => ctrl.abort(), 120000)
  let resp: Response
  try {
    resp = await fetch(url, {
      method: 'POST',
      headers: h,
      body: JSON.stringify({ jsonrpc: '2.0', id, method, params }),
      signal: ctrl.signal,
    })
  } catch (e) {
    throw new Error(`network: MCP POST failed (${e instanceof Error ? e.message : String(e)}) — the server must allow CORS, or use the dashboard proxy`)
  } finally {
    clearTimeout(timer)
  }
  if (resp.status === 202) return { value: {}, sessionId: cached }
  const text = await resp.text().catch(() => '')
  let sessionId = cached
  resp.headers.forEach((v, k) => {
    if (k.toLowerCase() === 'mcp-session-id' && !sessionId && v) {
      mcpSessionCache.set(url, v)
      sessionId = v
    }
  })
  if (!resp.ok) throw new Error(`network: MCP HTTP ${resp.status} — ${text.slice(0, 240)}`)
  let parsed: unknown = null
  try {
    parsed = JSON.parse(text)
  } catch {
    parsed = null
  }
  if (parsed && typeof parsed === 'object') return { value: parsed, sessionId }
  for (const v of parseSseDataLines(text)) {
    if (v && typeof v === 'object') {
      const o = v as Record<string, unknown>
      if (o.id === id || o.result !== undefined) return { value: v, sessionId }
    }
  }
  throw new Error('integrity: MCP HTTP stream carried no matching response')
}

function unwrapMcpResponse(v: unknown): unknown {
  const o = (v ?? {}) as Record<string, unknown>
  if (o.error && typeof o.error === 'object') {
    const e = o.error as Record<string, unknown>
    throw new Error(`error: MCP server error ${String(e.code ?? '')}: ${String(e.message ?? 'unknown')}`)
  }
  if (o.result !== undefined) return o.result
  throw new Error('integrity: MCP response had neither result nor error')
}

/** Render a `tools/call` result like native `mcp::render_call_result`. */
function renderMcpResult(result: unknown): string {
  const r = (result ?? {}) as Record<string, unknown>
  const content = r.content
  let text: string
  if (Array.isArray(content)) {
    const parts: string[] = []
    for (const c of content) {
      const o = (c ?? {}) as Record<string, unknown>
      if (o.type === 'text' && typeof o.text === 'string') parts.push(o.text)
      else if (o.type === 'image' || o.type === 'audio') parts.push('[media omitted — describe it in text instead]')
      else parts.push(JSON.stringify(c))
    }
    text = parts.join('\n')
  } else if (result === null || result === undefined) {
    text = JSON.stringify(result)
  } else if (typeof result === 'string') {
    text = result
  } else {
    text = JSON.stringify(result)
  }
  if (!text) text = '(empty result)'
  if (r.isError === true) text = `error: MCP tool reported failure:\n${text}`
  if (text.length > 32768) text = `${text.slice(0, 32768)}\n… truncated at 32 KiB`
  return text
}

/** Call one `mcp__server__tool` over Streamable HTTP (stdio → honest refusal). */
async function execMcpTool(fullName: string, input: Record<string, unknown>, configId: string): Promise<string> {
  const parts = splitMcpToolName(fullName)
  if (!parts) throw new Error(`unsupported: \`${fullName}\` is not an MCP tool`)
  const [server, tool] = parts
  const servers = localMcpServers(configId)
  const cfg = servers[server]
  if (!cfg) throw new Error(`not_found: MCP server \`${server}\` is not attached`)
  if (cfg.enabled === false) throw new Error(`denied: MCP server \`${server}\` is disabled`)
  if ((cfg.transport ?? 'http') !== 'http') {
    throw new Error(`unsupported: MCP server \`${server}\` uses stdio — stdio needs the desktop app or Docker server (spawn processes); attach an HTTP server or connect a dashboard`)
  }
  const url = String(cfg.url ?? '').trim()
  if (!/^https?:\/\//.test(url)) throw new Error(`invalid: MCP server \`${server}\` needs an http(s) url`)
  const idBase = Math.floor(Math.random() * 1000000)
  // Initialize handshake (session id negotiated here), then the call.
  const init = await mcpHttpRoundtrip(url, cfg.headers ?? [], 'initialize', {
    protocolVersion: '2024-11-05',
    capabilities: { tools: {} },
    clientInfo: { name: 'cybermanju', version: '0.1.0' },
  }, idBase + 1)
  unwrapMcpResponse(init.value)
  try {
    await mcpHttpRoundtrip(url, cfg.headers ?? [], 'notifications/initialized', {}, idBase + 2)
  } catch {
    // Fire-and-forget notification — ignore failures.
  }
  const call = await mcpHttpRoundtrip(url, cfg.headers ?? [], 'tools/call', {
    name: tool,
    arguments: input,
  }, idBase + 3)
  return capOutput(renderMcpResult(unwrapMcpResponse(call.value)))
}

/** Discover `tools/list` across enabled HTTP servers (for prompt advertisement). */
export async function listLocalMcpTools(configId: string): Promise<Array<{ server: string; name: string; description: string; input_schema: unknown }>> {
  const servers = localMcpServers(configId)
  const out: Array<{ server: string; name: string; description: string; input_schema: unknown }> = []
  for (const [name, cfg] of Object.entries(servers)) {
    if (cfg.enabled === false || (cfg.transport ?? 'http') !== 'http') continue
    const url = String(cfg.url ?? '').trim()
    if (!/^https?:\/\//.test(url)) continue
    try {
      const idBase = Math.floor(Math.random() * 1000000)
      const init = await mcpHttpRoundtrip(url, cfg.headers ?? [], 'initialize', {
        protocolVersion: '2024-11-05',
        capabilities: { tools: {} },
        clientInfo: { name: 'cybermanju', version: '0.1.0' },
      }, idBase + 1)
      unwrapMcpResponse(init.value)
      const listed = await mcpHttpRoundtrip(url, cfg.headers ?? [], 'tools/list', {}, idBase + 2)
      const result = unwrapMcpResponse(listed.value) as Record<string, unknown>
      const tools = Array.isArray(result.tools) ? result.tools : []
      for (const t of tools.slice(0, 128)) {
        const o = (t ?? {}) as Record<string, unknown>
        const tname = String(o.name ?? '').trim()
        if (!tname || tname.length > 128) continue
        out.push({
          server: name,
          name: `mcp__${name}__${tname}`,
          description: String(o.description ?? '').slice(0, 2000),
          input_schema: o.inputSchema ?? o.input_schema ?? { type: 'object', properties: {} },
        })
      }
    } catch {
      // One unreachable server never kills discovery.
    }
    if (out.length >= 256) break
  }
  return out.slice(0, 256).sort((a, b) => (a.name < b.name ? -1 : 1))
}

/** Minimum anchor strength: 16 hex chars = 64 bits — mirrors Rust `edit`.
 *  Anything shorter refuses with `integrity:` instead of coin-flipping. */
const MIN_ANCHOR_HEX = 16

/** Trailer `read` appends: `\n[blake3:<hex>]`. Mirrors `edit::anchor_line`. */
function anchorLineLocal(hash: string): string {
  return `\n[blake3:${hash}]`
}

/** Accept every shape a model might echo back — the whole line, a
 *  `blake3:` prefix, brackets, whitespace — mirrors Rust `edit::normalize_anchor`. */
export function normalizeAnchorLocal(raw: string): string {
  const t = raw.trim()
  const stripped = t.startsWith('[blake3:') ? t.slice('[blake3:'.length)
    : t.startsWith('blake3:') ? t.slice('blake3:'.length)
    : t
  return stripped.replace(/\]+$/, '').trim()
}

/** Hex of a syntactically valid trailing `[blake3:<64hex>]` line, or `null`.
 *  Pure syntax (one trailing newline tolerated) — says nothing about whose
 *  trailer it is. Mirrors Rust `edit::anchor_trailer`. */
export function anchorTrailerHex(text: string): string | null {
  const body = text.endsWith('\n') ? text.slice(0, -1) : text
  const i = body.lastIndexOf('\n')
  const line = i < 0 ? body : body.slice(i + 1)
  const m = /^\[blake3:([0-9a-fA-F]{64})\]$/.exec(line)
  return m ? m[1].toLowerCase() : null
}

/** Bytes before the trailing anchor line. Mirrors Rust `edit::body_before_trailer`. */
function bodyBeforeTrailer(text: string): string {
  const body = text.endsWith('\n') ? text.slice(0, -1) : text
  const i = body.lastIndexOf('\n')
  return i < 0 ? '' : text.slice(0, i)
}

/** Drop a trailing anchor line only when it verifies as ours: the hex must
 *  equal the BLAKE3 of the bytes before it (the exact trailer `read`
 *  appended), or of `existing` — bytes already stored (the pre-edit file in
 *  an edit flow). A file that legitimately ends with an anchor-shaped line
 *  survives untouched. `hashBody` is injectable so tests stay hermetic; when
 *  no hash is available we preserve bytes rather than delete them.
 *  Mirrors Rust `edit::strip_echo`. */
export async function stripEcho(
  text: string,
  existing: string | null,
  hashBody: (body: string) => Promise<string | null> = blake3HexIfAvailable,
): Promise<string> {
  const hex = anchorTrailerHex(text)
  if (!hex) return text
  const body = bodyBeforeTrailer(text)
  const actual = await hashBody(body)
  // Without a hash we cannot prove the trailer is ours — keep the bytes.
  if (!actual) return text
  if (actual === hex) return body
  if (existing !== null) {
    const prev = await hashBody(existing)
    if (prev === hex) return body
  }
  return text
}

/** Drop a trailing anchor line we added (hash-verified). Anything else —
 *  including anchor-shaped content lines — is preserved. The async verified
 *  form of the old sync strip; mirrors Rust `edit::strip_anchor`. */
export async function stripAnchorLocal(
  text: string,
  hashBody: (body: string) => Promise<string | null> = blake3HexIfAvailable,
): Promise<string> {
  return stripEcho(text, null, hashBody)
}

/** BLAKE3 hex of text via the loaded wasm module, or `null` when the module
 *  is not up yet (then a provided anchor simply is not checked — we never
 *  claim a verification we did not perform). */
async function blake3HexIfAvailable(text: string): Promise<string | null> {
  try {
    const w = await wasmModuleExports<{ blake3_hash(d: Uint8Array): string }>()
    if (!w || typeof w.blake3_hash !== 'function') return null
    return w.blake3_hash(new TextEncoder().encode(text))
  } catch {
    return null
  }
}

/** Mirror of Rust `edit::apply_edit` (exact-once anchored replacement).
 *  `expectedHash` is honoured the same way: a stale BLAKE3 refuses with
 *  `integrity:` instead of writing over a file someone else moved. */
async function applyEditLocal(
  current: string,
  oldBlock: string,
  newBlock: string,
  expectedHash?: string,
): Promise<string> {
  if (!oldBlock) throw new Error('invalid: old_block is empty')
  const anchor = normalizeAnchorLocal(expectedHash ?? '')
  if (anchor) {
    // A race check is only real with 64+ bits behind it (mirrors Rust): a
    // shorter anchor refuses even when the wasm hash is unavailable, because
    // weakness is a property of the anchor, not of our ability to verify.
    if (anchor.length < MIN_ANCHOR_HEX) {
      throw new Error(
        `integrity: anchor '${anchor}' is too short (need ≥${MIN_ANCHOR_HEX} hex chars / 64+ bits) — re-read and retry`,
      )
    }
    const actual = await blake3HexIfAvailable(current)
    // Prefix-accept, like Rust: a 16+-char anchor still detects a moved file
    // and never turns into a permanent edit blocker.
    if (actual && !actual.startsWith(anchor)) {
      throw new Error(
        `integrity: file changed since anchor (expected ${anchor}, got ${actual}) — re-read and retry`,
      )
    }
  }
  const hits = current.split(oldBlock).length - 1
  if (hits === 0) throw new Error('not_found: old_block does not occur in the file — re-read and retry')
  if (hits > 1) throw new Error(`conflict: old_block occurs ${hits} times — resend a larger, unique block`)
  return current.replace(oldBlock, newBlock)
}

async function execLocalTool(
  call: { name: string; input: Record<string, unknown> },
  cwd: string,
  configId = '',
  ctx: { depth: number; opts?: LocalRunOpts; spawnBackground?: (goal: string, context: string) => string } = { depth: 0 },
): Promise<string> {
  const join = (p: string) => {
    const raw = String(p || '')
    if (raw.startsWith('/')) return raw
    return cwd === '/' ? `/${raw}` : `${cwd}/${raw}`
  };
  switch (call.name) {
    case 'read': {
      const raw = await wasmRead(join(String(call.input.path ?? '')))
      // Same contract as native: hand back an anchor the model can pass as
      // `expected_hash`. Never claim one we could not compute.
      const hash = await blake3HexIfAvailable(raw)
      return hash ? `${raw}${anchorLineLocal(hash)}` : raw
    }
    case 'list': {
      const p = String(call.input.path ?? '')
      return wasmList(p ? join(p) : cwd)
    }
    case 'write': {
      const path = join(String(call.input.path ?? ''))
      // A `[blake3:…]` line echoed out of a `read` is metadata, never
      // content — strip it only when it verifies as ours (exact round-trip
      // trailer, or the trailer of the file already stored for edited
      // echoes). Anything else is user content and stays.
      const rawIn = String(call.input.content ?? '')
      let content = rawIn
      if (anchorTrailerHex(rawIn)) {
        let existing: string | null = null
        try {
          existing = await wasmRead(path)
        } catch {
          // New file (or unreadable): only the round-trip trailer can verify.
        }
        content = await stripEcho(rawIn, existing)
      }
      if (content.length > 1024 * 1024) throw new Error('too_large: content exceeds the 1 MiB browser write cap')
      await wasmWrite(path, content)
      const hash = await blake3HexIfAvailable(content)
      return hash ? `wrote ${path} (${content.length} bytes, blake3:${hash})` : `wrote ${path} (${content.length} bytes)`
    }
    case 'edit': {
      const path = join(String(call.input.path ?? ''))
      const current = await wasmRead(path)
      const anchor = String(call.input.expected_hash ?? '').trim()
      const updated = await applyEditLocal(
        current,
        String(call.input.old_block ?? ''),
        String(call.input.new_block ?? ''),
        anchor || undefined,
      )
      // Hash exactly the bytes that land on disk, so the printed anchor
      // verifies against the file the next read returns. Strip our trailer
      // against the pre-edit bytes: an echo of the read trailer in
      // `new_block` is still metadata, not content.
      const written = await stripEcho(updated, current)
      await wasmWrite(path, written)
      const hash = await blake3HexIfAvailable(written)
      return hash ? `edited ${path} (blake3:${hash})` : `edited ${path}`
    }
    case 'grep': {
      const rawPattern = String(call.input.pattern ?? '')
      if (!rawPattern) throw new Error('invalid: pattern is required')
      const base = String(call.input.path ?? '')
      const limit = Math.min(50, Math.max(1, Number(call.input.limit ?? 50)))
      const { test } = compileGrep(rawPattern)
      const start = base ? join(base) : cwd
      const listing = await wasmList(start)
      const names = splitVolumeListing(listing).filter(s => !s.endsWith('/'))
      const matches: string[] = []
      for (const name of names.slice(0, 200)) {
        const full = start === '/' ? `/${name}` : `${start}/${name}`
        try {
          const text = await wasmRead(full)
          text.split('\n').forEach((line, i) => {
            if (matches.length < limit && test(line)) {
              matches.push(`${full}:${i + 1}: ${line.trim().slice(0, 240)}`)
            }
          })
        } catch {
          // Unreadable entries are skipped, never fatal.
        }
        if (matches.length >= limit) break
      }
      return matches.length ? matches.join('\n') : `no matches for \`${rawPattern}\``
    }
    case 'glob': {
      const pattern = String(call.input.pattern ?? '**') || '**'
      const base = String(call.input.path ?? '')
      const limit = Math.min(200, Math.max(1, Number(call.input.limit ?? 200)))
      const start = base ? join(base) : cwd
      const hits: string[] = []
      const stack = [start]
      let seen = 0
      while (stack.length && hits.length < limit && seen < 2000) {
        const dir = stack.pop()!
        let names: string[]
        try {
          names = splitVolumeListing(await wasmList(dir)).filter(s => !s.startsWith('.'))
        } catch {
          continue
        }
        for (const name of names) {
          if (hits.length >= limit || seen >= 2000) break
          if (name.endsWith('/')) {
            stack.push(dir === '/' ? `/${name.slice(0, -1)}` : `${dir}/${name.slice(0, -1)}`)
            continue
          }
          seen++
          const full = dir === '/' ? `/${name}` : `${dir}/${name}`
          const rel = full === start
            ? name
            : full.startsWith(`${start}/`)
              ? full.slice(start.length + 1)
              : full.replace(/^\//, '')
          if (matchGlob(pattern, rel)) hits.push(full)
        }
      }
      hits.sort()
      return hits.length ? hits.join('\n') : `no files match \`${pattern}\``
    }
    case 'bash': {
      const command = String(call.input.command ?? '').trim()
      if (!command) throw new Error('invalid: command is required')
      let res: { ok: boolean; output: string }
      try {
        res = (await wasmOsDispatch('exec', { line: command })) as { ok: boolean; output: string }
      } catch (e) {
        const detail = e instanceof Error ? e.message : String(e)
        if (detail.includes('not bundled') || detail.includes('wasm backend unavailable')) {
          throw new Error('unsupported: browser volume unavailable in this build — use desktop/Docker, or rebuild the Pages pack with wasm-pack')
        }
        throw new Error(`error: bash dispatch failed — ${detail}`)
      }
      if (res.ok) return capOutput(res.output || '(empty output)')
      // The dispatcher already answers `unsupported:` / `unknown command:`
      // for device-only verbs — keep the prefix verbatim. Only append the
      // dashboard pointer when the dispatcher did not already give one
      // (vault verbs point at the terminal single-command line themselves).
      const out = String(res.output || 'command failed')
      if (/^(unsupported:|unknown command:)/.test(out)) {
        if (/dashboard|single-command/i.test(out)) throw new Error(out)
        throw new Error(`${out} — device shell (curl/wget/git/python) needs the desktop app or dashboard (:3456)`)
      }
      throw new Error(out.startsWith('error:') ? out : `error: ${out}`)
    }
    case 'task': {
      const goal = String(call.input.goal ?? '').trim()
      if (!goal) throw new Error('invalid: task needs a goal')
      if (ctx.depth >= BROWSER_TASK_MAX_DEPTH) {
        throw new Error('deny: max subagent depth reached — finish this level yourself')
      }
      if (!ctx.opts) throw new Error('unsupported: subagent needs the parent run context — retry from the Agent panel')
      const context = String(call.input.context ?? '').trim()
      // Detached spawn: id now, results injected as new messages while the
      // parent keeps working. Totals are unbounded (concurrent cap only).
      // Root-level only — deeper levels fall through to the foreground
      // depth gate (native `run_task` parity).
      if (call.input.background === true && ctx.depth === 0 && ctx.spawnBackground) {
        return ctx.spawnBackground(goal, context)
      }
      return runLocalSubagent(goal, context, ctx.opts, configId, ctx.depth)
    }
    case 'question':
      throw new Error('unsupported: routed through approvals, never executed directly')
    case 'os_exec': {
      // Pure cybsh surface — same dispatcher as bash's cybsh path, but
      // never a device-shell fallthrough and never the -os host flag.
      const command = String(call.input.command ?? '').trim()
      if (!command) throw new Error('invalid: command is required')
      if (command.split(/\s+/).some(a => a === '-os' || a === '--host' || a === '--os')) {
        throw new Error("unsupported: 'os_exec -os' addresses the host filesystem — run it in the desktop app")
      }
      let res: { ok: boolean; output: string }
      try {
        res = (await wasmOsDispatch('exec', { line: command })) as { ok: boolean; output: string }
      } catch (e) {
        const detail = e instanceof Error ? e.message : String(e)
        if (detail.includes('not bundled') || detail.includes('wasm backend unavailable')) {
          throw new Error('unsupported: browser volume unavailable in this build — use desktop/Docker')
        }
        throw new Error(`error: os_exec dispatch failed — ${detail}`)
      }
      if (res.ok) return capOutput(res.output || '(empty output)')
      const out = String(res.output || 'command failed')
      // The dispatcher answers `unsupported:` / `unknown command:` for
      // device-only verbs — keep the prefix verbatim, never fall through.
      if (/^(unsupported:|unknown command:)/.test(out)) throw new Error(out)
      throw new Error(out.startsWith('error:') ? out : `error: ${out}`)
    }
    case 'ui_open_panel': {
      const panel = String(call.input.panel ?? '').trim()
      if (!panel) throw new Error('invalid: panel is required')
      const { isKnownPanel } = await import('@/utils/uiRequests')
      if (!isKnownPanel(panel)) throw new Error(`not_found: unknown panel '${panel}'`)
      const { useWindowManager } = await import('@/composables/useWindowManager')
      const wm = useWindowManager()
      const props: Record<string, unknown> = {}
      const tab = String(call.input.tab ?? '').trim()
      const path = String(call.input.path ?? '').trim()
      if (tab) props.tab = tab
      if (path) props.path = path
      const { resolvePanel } = await import('@/utils/panels')
      wm.open(resolvePanel(panel as never), props)
      return `ok: opened panel '${panel}'`
    }
    case 'ui_notify': {
      const levelRaw = String(call.input.level ?? 'info').trim()
      const level = ['info', 'success', 'warning', 'error'].includes(levelRaw) ? levelRaw : ''
      if (!level) {
        throw new Error(`invalid: level must be info/success/warning/error (got '${levelRaw}')`)
      }
      const message = String(call.input.message ?? '').trim()
      if (!message) throw new Error('invalid: message is required')
      const { useNotifications } = await import('@/composables/useNotifications')
      useNotifications().notify(level as 'info' | 'success' | 'warning' | 'error', message)
      return 'ok: notified'
    }
    case 'secret_list': {
      // Metadata only — the wasm op strips `valueSealed` before returning.
      const rows = (await wasmDbDispatch('secrets.list', {})) as Array<Record<string, unknown>>
      return JSON.stringify(rows ?? [])
    }
    case 'secret_get': {
      const id = String(call.input.id ?? '').trim()
      if (!id) throw new Error('invalid: secret id is required')
      // Plaintext goes to the model (permission gate already asked); the
      // transcript redactor scrubs provider keys, and the tool description
      // warns the model to use it sparingly.
      const { sessionVaultPassphrase } = await import('@/composables/useWasmBackend')
      const passphrase = sessionVaultPassphrase()
      if (!passphrase) {
        throw new Error('unsupported: vault locked — unlock the .cybermanju file first')
      }
      const res = (await wasmDbDispatch('secrets.reveal', { id, passphrase })) as { value?: string }
      return String(res?.value ?? '')
    }
    case 'memory_recall': {
      const query = String(call.input.query ?? '').trim()
      if (!query) throw new Error('invalid: query is required')
      const topK = Math.min(10, Math.max(1, Number(call.input.top_k ?? 3) || 3))
      const hits = localMemories.recall(configId || undefined, query, topK)
      if (!hits.length) return 'no memories match — proceed with the transcript alone'
      return hits.map(h => `- ${h.text}`).join('\n')
    }
    case 'memory_remember': {
      const text = String(call.input.text ?? '').trim().slice(0, MEMORY_TEXT_CAP_CHARS)
      if (!text) throw new Error('invalid: nothing memorable after cleaning')
      // Available at every depth (parent config scope) — never required.
      const row = localMemories.remember(configId || 'browser', text)
      if (!row) throw new Error('invalid: nothing memorable after cleaning')
      return `remembered ${row.id} (${row.text.length} chars, keyword-only: no embeddings on this transport)`
    }
    case 'self_research': {
      // Introspection entry point (a real tool, not prompt text): sweep the
      // agent's own volume for how something works, read the top hits,
      // return file:line-grounded snippets. Read-only, plan-safe.
      const query = String(call.input.query ?? '').trim()
      if (!query) throw new Error('invalid: query is required')
      const limit = Math.min(12, Math.max(1, Number(call.input.limit ?? 8) || 8))
      const sub = String(call.input.path ?? '').trim()
      const start = sub ? join(sub) : cwd
      const want = query.toLowerCase().split(/[^a-z0-9]+/).filter(w => w.length >= 2)
      if (!want.length) throw new Error('invalid: query has no searchable terms')
      const isSource = (n: string) =>
        /\.(rs|ts|vue|md|toml|json)$/i.test(n) || n === 'Dockerfile' || n === 'AGENTS.md' || n === 'SKILL.md'
      const all: string[] = []
      const stack = [start]
      let seen = 0
      while (stack.length && seen < 2000 && all.length < 2000) {
        const dir = stack.pop()!
        let names: string[]
        try {
          names = splitVolumeListing(await wasmList(dir)).filter(s => !s.startsWith('.'))
        } catch {
          continue
        }
        for (const name of names) {
          if (name.endsWith('/')) {
            const base = name.slice(0, -1)
            if (['node_modules', 'target', 'dist', 'dist-wasm', 'build', '.git'].includes(base)) continue
            stack.push(dir === '/' ? `/${base}` : `${dir}/${base}`)
            continue
          }
          if (!isSource(name)) continue
          seen++
          const full = dir === '/' ? `/${name}` : `${dir}/${name}`
          const rel = full === start ? name : full.startsWith(`${start}/`) ? full.slice(start.length + 1) : full.replace(/^\//, '')
          all.push(rel)
        }
      }
      const scored = rankResearchPaths(all, query, limit)
      if (!scored.length) return `self_research: no files match \`${query}\` — try broader terms or list/glob first`
      let re: RegExp | null = null
      try {
        re = new RegExp(query)
      } catch {
        re = null
      }
      let out = `self_research: \`${query}\` (${scored.length} files)\n`
      for (const p of scored) {
        const full = p.startsWith('/') ? p : start === '/' ? `/${p}` : `${start}/${p}`
        let text = ''
        try {
          text = await wasmRead(full)
        } catch {
          continue
        }
        out += `\n=== ${p} ===\n`
        let shown = 0
        text.split('\n').forEach((line, i) => {
          if (shown >= 12) return
          const hit = re ? (() => { try { return re!.test(line) } catch { return false } })() : line.toLowerCase().includes(query.toLowerCase())
          if (hit) {
            out += `  L${i + 1}: ${line.trim().slice(0, 240)}\n`
            shown++
          }
        })
        if (!shown) out += `  (no query lines; head)\n${text.slice(0, 800)}\n`
        if (out.length > 32768) break
      }
      out += '\nRead full files with `read` for exact code; cite file:line.'
      return out.length > 32768 ? `${out.slice(0, 32768)}\n… truncated at 32 KiB` : out
    }
    case 'skill_save': {
      // Persist into the `.cybermanju` container so skills survive restarts
      // and sync across devices with provider data (native parity).
      const name = String(call.input.name ?? '').trim()
      if (!/^[A-Za-z0-9._-]{1,64}$/.test(name)) {
        throw new Error('invalid: skill name must be 1-64 chars of [A-Za-z0-9._-]')
      }
      const description = String(call.input.description ?? '').trim().slice(0, 500)
      if (!description) throw new Error('invalid: description is required')
      const content = String(call.input.content ?? '').trim()
      if (!content) throw new Error('invalid: content is required')
      if (content.length > 8192) throw new Error(`too_large: skill content is ${content.length} bytes, cap is 8192`)
      const rel = `.cybermanju/skills/${name}/SKILL.md`
      const path = cwd === '/' ? `/${rel}` : rel.startsWith('/') ? rel : `/${rel}`
      const file = `---\nname: ${name}\ndescription: ${description}\nversion: 1\ntools: [read, list, grep, glob]\n---\n${content}\n`
      await wasmWrite(path, file)
      return `saved skill '${name}' → ${rel} (${file.length} bytes; folded into future prompts, syncs with provider data)`
    }
    case 'mcp_attach': {
      // Verify-then-persist an HTTP MCP server on this config. stdio is
      // refused (needs the desktop app or Docker server).
      const name = String(call.input.name ?? '').trim()
      if (!/^[A-Za-z0-9_-]{1,64}$/.test(name)) {
        throw new Error('invalid: MCP server name must be 1-64 chars of [A-Za-z0-9_-]')
      }
      const url = String(call.input.url ?? '').trim()
      if (!/^https?:\/\//.test(url)) {
        throw new Error('invalid: mcp_attach needs an http(s) url (stdio servers spawn processes — attach those from the desktop UI)')
      }
      const idBase = Math.floor(Math.random() * 1000000)
      const init = await mcpHttpRoundtrip(url, [], 'initialize', {
        protocolVersion: '2024-11-05',
        capabilities: { tools: {} },
        clientInfo: { name: 'cybermanju', version: '0.1.0' },
      }, idBase + 1)
      unwrapMcpResponse(init.value)
      const listed = await mcpHttpRoundtrip(url, [], 'tools/list', {}, idBase + 2)
      const result = unwrapMcpResponse(listed.value) as Record<string, unknown>
      const tools = Array.isArray(result.tools) ? result.tools : []
      try {
        const raw = localStorage.getItem(CONFIGS_KEY) ?? '[]'
        const all = JSON.parse(raw) as AgentConfig[]
        const i = all.findIndex(c => c.id === configId)
        if (i < 0) throw new Error(`not_found: config is not on this device`)
        const cfg = all[i]
        cfg.mcpServers = cfg.mcpServers ?? {}
        cfg.mcpServers[name] = { transport: 'http', args: [], env: {}, headers: [], enabled: true, url } as McpServerConfig
        localStorage.setItem(CONFIGS_KEY, JSON.stringify(all))
      } catch (e) {
        if (e instanceof Error && /not_found|config/.test(e.message)) throw e
        throw new Error(`not_found: cannot persist MCP server (${e instanceof Error ? e.message : String(e)})`)
      }
      return `attached MCP '${name}' (${url}) — ${tools.length} tools verified; takes effect on the next run (mcp__${name}__*)`
    }
    case 'repo_analyze': {
      // Git-free GitHub analysis over plain HTTPS: metadata + recursive tree
      // + README. No git binary, so it works on WASM/mobile too.
      const slugRaw = String(call.input.repo ?? call.input.url ?? '').trim()
      const slug = slugRaw
        .replace(/^https?:\/\/github\.com\//, '')
        .replace(/^github\.com\//, '')
        .replace(/\.git$/, '')
        .replace(/^\/+|\/+$/g, '')
      const parts = slug.split('/').filter(Boolean)
      if (parts.length < 2) throw new Error(`invalid: '${slugRaw}' is not owner/repo — try \`owner/repo\``)
      const [owner, repo] = parts
      if (!/^[A-Za-z0-9._-]{1,100}$/.test(owner) || !/^[A-Za-z0-9._-]{1,100}$/.test(repo)) {
        throw new Error(`invalid: bad repo slug '${owner}/${repo}'`)
      }
      const gh = async (url: string) => {
        const ctrl = new AbortController()
        const timer = setTimeout(() => ctrl.abort(), 30000)
        try {
          const r = await fetch(url, {
            headers: { Accept: 'application/vnd.github+json', 'User-Agent': 'cybermanju-os' },
            signal: ctrl.signal,
          })
          if (r.status === 404) throw new Error(`not_found: repo '${owner}/${repo}' not found or private`)
          if (r.status === 403 || r.status === 429) throw new Error(`rate_limited: GitHub API throttled the request (HTTP ${r.status})`)
          if (!r.ok) throw new Error(`network: GitHub API HTTP ${r.status}`)
          return (await r.json()) as Record<string, unknown>
        } catch (e) {
          if (e instanceof Error && /^(not_found|rate_limited|network):/.test(e.message)) throw e
          throw new Error(`network: GitHub request failed (${e instanceof Error ? e.message : String(e)})`)
        } finally {
          clearTimeout(timer)
        }
      }
      const meta = await gh(`https://api.github.com/repos/${owner}/${repo}`)
      const branch = String(call.input.branch ?? '').trim() || String(meta.default_branch ?? 'main')
      const tree = await gh(`https://api.github.com/repos/${owner}/${repo}/git/trees/${branch}?recursive=1`)
      let readmeHead = ''
      try {
        const rm = await gh(`https://api.github.com/repos/${owner}/${repo}/readme`)
        const b64 = String(rm.content ?? '').replace(/\s+/g, '')
        if (b64) {
          const bin = atob(b64)
          const bytes = Uint8Array.from(bin, c => c.charCodeAt(0))
          readmeHead = new TextDecoder().decode(bytes).slice(0, 1500)
        }
      } catch {
        // Missing docs never fail the analysis.
      }
      const blobs = (Array.isArray(tree.tree) ? tree.tree : [])
        .filter((e: unknown) => (e as Record<string, unknown>).type === 'blob')
        .map((e: unknown) => String((e as Record<string, unknown>).path ?? ''))
        .filter(Boolean)
        .slice(0, 500)
      const exts = new Map<string, number>()
      for (const p of blobs) {
        const dot = p.lastIndexOf('.')
        const slash = p.lastIndexOf('/')
        if (dot > slash + 1 && p.length - dot <= 9) {
          const e = p.slice(dot + 1).toLowerCase()
          exts.set(e, (exts.get(e) ?? 0) + 1)
        }
      }
      const topExts = [...exts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8).map(([e, n]) => `${e}×${n}`).join(' ') || '(none)'
      const entryFiles = ['README.md', 'ARCHITECTURE.md', 'AGENTS.md', 'Cargo.toml', 'package.json', 'go.mod', 'pyproject.toml', 'Dockerfile', 'docker-compose.yml']
        .filter(n => blobs.some(p => p === n || p.endsWith(`/${n}`)))
      const top = new Map<string, number>()
      for (const p of blobs) {
        const first = p.split('/')[0]
        top.set(first, (top.get(first) ?? 0) + 1)
      }
      const layout = [...top.entries()].sort((a, b) => b[1] - a[1]).slice(0, 20).map(([d, n]) => `  ${d}/ (${n})`).join('\n')
      let out =
        `repo: ${owner}/${repo} (branch ${branch}, ★${String(meta.stargazers_count ?? 0)}, primary ${String(meta.language ?? '?')})\n` +
        `desc: ${String(meta.description ?? '(no description)')}\n` +
        `files: ${blobs.length} blobs | exts: ${topExts}\n` +
        `start here: ${entryFiles.length ? entryFiles.join(', ') : '(no standard entry files)'}\nlayout:\n${layout}\n`
      if (readmeHead.trim()) out += `readme:\n${readmeHead}\n`
      out += 'next: read entry files above, then glob/grep the layout dirs.'
      return out.length > 32768 ? `${out.slice(0, 32768)}\n… truncated at 32 KiB` : out
    }
    default: {
      if (call.name.startsWith('mcp__')) {
        return execMcpTool(call.name, call.input, configId)
      }
      throw new Error(`unsupported: unknown tool '${call.name}'`)
    }
  }
}

// ─── loop state ─────────────────────────────────────────────────

export interface LocalRunOpts {
  baseUrl: string
  dialect: 'openAi' | 'anthropic'
  model: string
  headers: Array<[string, string]>
  system: string
  maxTurns: number
  permission: PermissionRuleset
  autoApprove: boolean
  agentKind: 'build' | 'plan'
  /** Config scope for local memories (namespaces recall + remember). */
  configId?: string
  /** Fallback routes tried in order on terminal provider errors. */
  fallbacks?: LocalFailoverRoute[]
  /** Persist an "allow always" rule the user just granted (config write). */
  onRemember?: (tool: string) => void
}

export interface ApprovalRequest {
  tool: string
  input: Record<string, unknown>
  summary: string
  question?: string | null
  resolve: (approved: boolean, answer?: string, remember?: boolean) => void
}

const running = ref(false)
const pendingApproval = ref<ApprovalRequest | null>(null)
/** One-liner of what the browser loop is doing right now (job line in the UI). */
export const localActivity = ref('')
let abortRequested = false

/** Ask the running browser loop to stop at the next turn boundary. */
export function abortLocalRun(): void {
  abortRequested = true
  if (pendingApproval.value) {
    const parked = pendingApproval.value
    pendingApproval.value = null
    parked.resolve(false)
  }
}

function waitApproval(
  req: Omit<ApprovalRequest, 'resolve'>,
): Promise<{ approved: boolean; answer?: string; remember?: boolean }> {
  return new Promise(resolve => {
    pendingApproval.value = {
      ...req,
      resolve: (approved, answer, remember) => {
        pendingApproval.value = null
        resolve({ approved, answer, remember })
      },
    }
  })
}

function assistantToolWire(calls: ToolCall[]): unknown[] {
  return calls.map(c => ({ id: c.id, name: c.name, input: c.input }))
}

/** Native `post_with_retry` parity (2s/4s/8s): `rate_limited:` turns sleep
 *  in slices so abort stays responsive. Anything else fails immediately —
 *  auth errors must never be retried into a lockout. */
const RATE_BACKOFFS_MS = [2000, 4000, 8000]

/** Sleep that resolves `true` early when the run was aborted. */
function sleepAbortable(ms: number): Promise<boolean> {
  const end = Date.now() + ms
  return (async () => {
    while (Date.now() < end) {
      if (abortRequested) return true
      await new Promise(r => setTimeout(r, Math.min(200, Math.max(0, end - Date.now()))))
    }
    return abortRequested
  })()
}

type ProviderTurnRequest = Parameters<typeof wasmAgentPrompt>[0]

/** One fallback hop: label for the activity line plus a full endpoint. */
export interface LocalFailoverRoute {
  label: string
  baseUrl: string
  dialect: 'openAi' | 'anthropic'
  model: string
  headers: Array<[string, string]>
}

/** Terminal provider errors worth trying the next route on (native
 *  `is_failover_worthy` parity): dead keys, throttling, transport failures
 *  and empty credits fail over; malformed requests and overflows stop. */
export function isFailoverWorthy(error: string): boolean {
  const prefix = error.split(':')[0]?.trim()
  return prefix === 'auth' || prefix === 'rate_limited' || prefix === 'network' || prefix === 'limit'
}

/** One provider turn with transport-throw mapping (never throws itself). */
async function providerTurn(req: ProviderTurnRequest): Promise<WasmAgentTurn> {
  try {
    return await wasmAgentPrompt(req)
  } catch (e) {
    const detail = e instanceof Error ? e.message : String(e)
    // The vite stub (no wasm-pack pkg) throws `not bundled` — translate to
    // an actionable house-prefixed error instead of an unhandled rejection.
    if (detail.includes('not bundled') || detail.includes('wasm backend unavailable')) {
      return { ok: false, error: 'network: browser engine not bundled — use desktop/Docker, or rebuild the Pages pack with wasm-pack' }
    }
    return { ok: false, error: `network: provider call failed — ${detail}` }
  }
}

/** Drive one prompt to completion in the browser. Mutates `messages`. */
export async function runLocalAgent(
  opts: LocalRunOpts,
  messages: ChatMessage[],
  usage: TokenUsage,
  onUpdate?: () => void,
): Promise<{ stopped: 'done' | 'limit' | 'aborted' | 'error'; error?: string }> {
  return runLoopInternal(opts, messages, usage, 0, onUpdate)
}

/**
 * Bounded inline subagent (native `run_subagent` parity): same provider,
 * full file/shell toolset plus memory tools, isolated transcript, no
 * approval parking, no nested subagents. Returns the parent-facing summary.
 */
async function runLocalSubagent(
  goal: string,
  context: string,
  parentOpts: LocalRunOpts,
  configId: string,
  parentDepth: number,
): Promise<string> {
  const subOpts: LocalRunOpts = {
    ...parentOpts,
    maxTurns: Math.min(BROWSER_SUBAGENT_MAX_TURNS, Math.max(1, parentOpts.maxTurns)),
    agentKind: 'build',
    system: `${parentOpts.system}\nSUBAGENT TOOLSET: this run has \`read\`, \`list\`, \`grep\`, \`glob\`, \`self_research\`, \`repo_analyze\`, \`edit\`, \`write\`, \`bash\` and the memory tools (\`memory_recall\`, \`memory_remember\` — use them when past context or a durable lesson helps) — no task or question. Investigate, edit and verify; the parent reviews it.`,
    configId,
  }
  const subMessages: ChatMessage[] = [
    { role: 'user', content: `Subagent goal: ${goal}${context ? `\nContext: ${context}` : ''}` },
  ]
  const subUsage: TokenUsage = { inputTokens: 0, outputTokens: 0 }
  const prevActivity = localActivity.value
  try {
    localActivity.value = 'subagent · running'
    const res = await runLoopInternal(subOpts, subMessages, subUsage, parentDepth + 1)
    if (res.stopped === 'aborted') return 'subagent cancelled with the parent run'
    const last = [...subMessages].reverse().find(m => m.role === 'assistant' && m.content)
    const text = (last?.content ?? '').trim() || '(subagent produced no text)'
    return `subagent result:\n${text.slice(0, SUBAGENT_SUMMARY_CAP)}`
  } finally {
    localActivity.value = prevActivity
  }
}

async function runLoopInternal(
  opts: LocalRunOpts,
  messages: ChatMessage[],
  usage: TokenUsage,
  depth: number,
  onUpdate?: () => void,
): Promise<{ stopped: 'done' | 'limit' | 'aborted' | 'error'; error?: string }> {
  const isRoot = depth === 0
  if (isRoot) {
    running.value = true
    abortRequested = false
  }
  localActivity.value = `thinking · ${opts.model}`
  // Doom-loop guard (native `agent_api` parity): three byte-identical calls
  // in a row are denied instead of silently burning the turn budget.
  let lastSig: { name: string; json: string } | null = null
  let repeats = 0
  // Refusal breaker (Hermes parity): three `no`s on one tool stop the loop
  // from parking on the human for the same call a fourth time.
  const breaker = new DenialBreaker()
  // Parent runs advertise discovered HTTP MCP tools; subagents run the
  // full file/shell subset with no MCP and no nesting (native parity).
  let extraTools: Array<Record<string, unknown>> = []
  if (isRoot && opts.configId) {
    try {
      const defs = await listLocalMcpTools(opts.configId)
      extraTools = defs.map(d => ({ name: d.name, description: d.description, input_schema: d.input_schema }))
    } catch {
      extraTools = []
    }
  }
  // Detached background subagents (`task background:true`): spawned by the
  // `task` branch via `spawnBackground` below, harvested without blocking
  // after every batch, waited for when the model would otherwise stop.
  // Totals are unbounded — only concurrent workers are capped (native
  // `MAX_BG_SUBAGENTS` parity) and finished slots are reusable.
  const bgPending: BgEntry[] = []
  let bgNext = 0
  // Spawn one detached subagent: id now, result injected as a new message
  // while the parent keeps working. Root-level only — subagents never nest.
  const spawnBackground = isRoot
    ? (goal: string, context: string): string => {
      const runningCount = bgPending.filter(p => !p.settled).length
      if (runningCount >= MAX_BG_SUBAGENTS) return bgBusyMessage(runningCount)
      bgNext += 1
      const id = `bg-${bgNext}`
      const entry: BgEntry = { id, goal, promise: Promise.resolve(''), result: null, settled: false }
      // Detach: the subagent runs with the parent provider route and an
      // isolated transcript; completion lands in `entry` via settled flag
      // so the parent loop never blocks on it mid-run.
      entry.promise = runLocalSubagent(goal, context, opts, opts.configId ?? '', depth).then(
        result => {
          entry.result = result
          entry.settled = true
          return result
        },
        e => {
          const detail = e instanceof Error ? e.message : String(e)
          entry.result = `subagent failed: ${detail}`
          entry.settled = true
          return entry.result
        },
      )
      bgPending.push(entry)
      return bgStartedMessage(id, goal)
    }
    : undefined
  /** Harvest finished workers into the transcript as new user messages. */
  const injectSettledBg = (): boolean => {
    if (!bgPending.length) return false
    const done = sweepBgTasks(bgPending)
    if (!done.length) return false
    for (const { id, result } of done) {
      messages.push({ role: 'user', content: bgFinishedMessage(id, result) })
    }
    onUpdate?.()
    return true
  }
  // Active provider route: the primary first, then fallbacks on terminal
  // errors. The transcript is provider-neutral, so the same thread
  // continues on the next route.
  let routeBase = opts.baseUrl
  let routeDialect = opts.dialect
  let routeModel = opts.model
  let routeHeaders = opts.headers
  const pendingFallbacks = [...(opts.fallbacks ?? [])]
  const triedLabels: string[] = []
  async function turnWithBackoff(req: ProviderTurnRequest): Promise<WasmAgentTurn> {
    let res = await providerTurn(req)
    let attempt = 0
    while (!res.ok && (res.error ?? '').startsWith('rate_limited:') && attempt < RATE_BACKOFFS_MS.length) {
      const waitS = RATE_BACKOFFS_MS[attempt] / 1000
      localActivity.value = `rate limited · retry ${attempt + 1}/3 in ${waitS}s…`
      onUpdate?.()
      if (await sleepAbortable(RATE_BACKOFFS_MS[attempt])) {
        return { ok: false, error: 'cancelled: aborted while backing off' }
      }
      attempt += 1
      res = await providerTurn(req)
    }
    return res
  }
  try {
    for (let turn = 0; turn < Math.min(50, Math.max(1, opts.maxTurns)); turn++) {
      if (abortRequested) return { stopped: 'aborted' as const }
      localActivity.value = `thinking · ${routeModel}`
      // Context pressure is relieved on the *wire* only: the transcript the
      // user sees keeps every byte, the provider gets the pruned copy.
      const wire = prepareWireMessages(messages, { model: routeModel })
      if (wire.pruned > 0) {
        localActivity.value = `compacting · pruned ${wire.pruned} tool result(s)`
      }
      const buildReq = (): ProviderTurnRequest => ({
        url: routeDialect === 'anthropic' ? `${routeBase.replace(/\/$/, '')}/v1/messages` : `${routeBase.replace(/\/$/, '')}/chat/completions`,
        dialect: routeDialect,
        model: routeModel,
        headers: routeHeaders,
        system: opts.system,
        messages: wire.messages as unknown as Array<Record<string, unknown>>,
        tools: true,
        extraTools,
      })
      let res = await turnWithBackoff(buildReq())
      // Dead key, throttling, transport failure or empty credits: continue
      // the same transcript on the next route instead of failing the run.
      while (!res.ok && !abortRequested && isFailoverWorthy(res.error ?? '') && pendingFallbacks.length) {
        const next = pendingFallbacks.shift()!
        routeBase = next.baseUrl
        routeDialect = next.dialect
        routeModel = next.model
        routeHeaders = next.headers
        triedLabels.push(next.label)
        lastSig = null
        repeats = 0
        localActivity.value = `failover → ${next.label} · ${next.model}`
        onUpdate?.()
        res = await turnWithBackoff(buildReq())
      }
      if (abortRequested) return { stopped: 'aborted' as const }
      if (!res.ok) {
        const err = res.error ?? 'network: provider call failed'
        return {
          stopped: 'error',
          error: triedLabels.length ? `${err} (tried: ${opts.model} → ${triedLabels.join(' → ')})` : err,
        }
      }
      const turnData = res.turn!
      usage.inputTokens += turnData.usage.inputTokens ?? 0
      usage.outputTokens += turnData.usage.outputTokens ?? 0
      if (!turnData.tool_calls.length) {
        messages.push({ role: 'assistant', content: turnData.content })
        onUpdate?.()
        // The model stopped calling tools — but background subagents may
        // still be running. Wait for them and feed their results back as
        // new messages instead of ending mid-flight (native `drain_bg_tasks`
        // parity). Waiting never ends the run; the loop continues.
        if (isRoot && bgPending.length) {
          localActivity.value = `waiting for ${bgPending.filter(p => !p.settled).length || bgPending.length} background subagent(s)`
          onUpdate?.()
          await waitBgSettled(bgPending)
          if (abortRequested) return { stopped: 'aborted' as const }
          if (injectSettledBg()) continue
        }
        return { stopped: 'done' }
      }
      messages.push({
        role: 'assistant_tool',
        content: turnData.content,
        toolInput: assistantToolWire(turnData.tool_calls),
      } as ChatMessage)
      onUpdate?.()
      for (const call of turnData.tool_calls) {
        const input = (call.input ?? {}) as Record<string, unknown>
        const json = JSON.stringify(call.input ?? {})
        if (lastSig && lastSig.name === call.name && lastSig.json === json) {
          repeats += 1
        } else {
          lastSig = { name: call.name, json }
          repeats = 1
        }
        if (repeats >= 3) {
          messages.push({
            role: 'tool',
            content: 'denied: identical tool call repeated 3 times (doom-loop guard) — vary the input or explain',
            toolCallId: call.id,
            toolName: call.name,
          })
          onUpdate?.()
          continue
        }
        if (call.name === 'question') {
          if (depth > 0) {
            messages.push({
              role: 'tool',
              content: 'deny: subagents cannot ask questions — report findings to the parent run',
              toolCallId: call.id,
              toolName: call.name,
            })
            onUpdate?.()
            continue
          }
          const q = String(input.question ?? 'The agent has a question.')
          localActivity.value = 'question · waiting for you'
          const ans = await waitApproval({ tool: 'question', input, summary: q, question: q })
          localActivity.value = `thinking · ${opts.model}`
          messages.push({
            role: 'tool',
            // User-typed answers can paste secrets — same redaction as tool output.
            content: cleanToolOutput(ans.approved ? `user answered: ${ans.answer || 'approved without comment'}` : 'declined: user declined to answer'),
            toolCallId: call.id,
            toolName: call.name,
          })
          onUpdate?.()
          continue
        }
        localActivity.value = activityLine(call.name, input)
        // Subagents never park on approvals and never nest/ask/remember: the
        // full file/shell subset is enforced here (runtime gate mirrors
        // native `run_subagent`).
        if (depth > 0) {
          if (!SUBAGENT_TOOLS.has(call.name)) {
            messages.push({
              role: 'tool',
              content: `deny: subagents cannot run \`${call.name}\` — report findings to the parent run`,
              toolCallId: call.id,
              toolName: call.name,
            })
            onUpdate?.()
            continue
          }
          if (call.name.startsWith('mcp__')) {
            messages.push({
              role: 'tool',
              content: `deny: subagents cannot run \`${call.name}\` — report findings to the parent run`,
              toolCallId: call.id,
              toolName: call.name,
            })
            onUpdate?.()
            continue
          }
        }
        const decision = decideLocalTool(opts.permission, opts.agentKind, call.name, input)
        if (decision.kind === 'deny') {
          breaker.record(call.name, true)
          messages.push({
            role: 'tool',
            content: `${decision.reason} — adjust the permission ruleset to allow it`,
            toolCallId: call.id,
            toolName: call.name,
          })
          onUpdate?.()
          continue
        }
        if (decision.kind === 'ask' && opts.autoApprove) {
          // Auto mode approves ordinary asks — never standing orders.
          const refused = protectedAutoApproveDenial(call.name, input)
          if (refused) {
            breaker.record(call.name, true)
            messages.push({ role: 'tool', content: refused, toolCallId: call.id, toolName: call.name })
            onUpdate?.()
            continue
          }
          breaker.record(call.name, false)
        }
        if (decision.kind === 'ask' && !opts.autoApprove) {
          if (depth > 0) {
            messages.push({
              role: 'tool',
              content: `denied: subagent approval is disabled for \`${call.name}\` — work around it or explain`,
              toolCallId: call.id,
              toolName: call.name,
            })
            onUpdate?.()
            continue
          }
          if (breaker.isOpen(call.name)) {
            messages.push({
              role: 'tool',
              content: breaker.reason(call.name),
              toolCallId: call.id,
              toolName: call.name,
            })
            onUpdate?.()
            continue
          }
          localActivity.value = `${call.name} · waiting for approval`
          const ans = await waitApproval({ tool: call.name, input, summary: decision.summary, question: null })
          if (!ans.approved) {
            breaker.record(call.name, true)
            const feedback = (ans.answer ?? '').trim()
            messages.push({
              role: 'tool',
              content: cleanToolOutput(feedback
                ? `denied: user rejected \`${call.name}\` — user feedback: ${feedback} — work around it or explain`
                : `denied: user rejected \`${call.name}\` — work around it or explain`),
              toolCallId: call.id,
              toolName: call.name,
            })
            onUpdate?.()
            continue
          }
          breaker.record(call.name, false)
          if (ans.remember) {
            rememberAllowLocal(opts.permission, call.name)
            opts.onRemember?.(call.name)
          }
        }
        if (decision.kind === 'allow') breaker.record(call.name, false)
        try {
          const output = await execLocalTool(call, '/', opts.configId ?? '', { depth, opts, spawnBackground })
          // Native `clean_output` parity: secrets never reach the transcript.
          messages.push({ role: 'tool', content: cleanToolOutput(output), toolCallId: call.id, toolName: call.name })
        } catch (e) {
          const detail = e instanceof Error ? e.message : String(e)
          const mapped = detail.includes('not bundled') || detail.includes('wasm backend unavailable')
            ? 'unsupported: browser volume unavailable in this build — use desktop/Docker, or rebuild the Pages pack with wasm-pack'
            : detail
          messages.push({ role: 'tool', content: cleanToolOutput(mapped.startsWith('unsupported:') || mapped.startsWith('error:') ? mapped : `error: ${mapped}`), toolCallId: call.id, toolName: call.name })
        }
        onUpdate?.()
      }
      // Harvest finished background subagents without blocking: their
      // results arrive as new user messages so the next turn sees them
      // while the parent kept working meanwhile (native `sweep_bg_tasks`
      // parity).
      if (isRoot) injectSettledBg()
    }
    // Turn budget is spent, but background results must not be lost: wait
    // for them and save them to the transcript before stopping (native
    // `LimitReached` drain parity) — the next run continues from saved state.
    if (isRoot && bgPending.length) {
      localActivity.value = `waiting for ${bgPending.filter(p => !p.settled).length || bgPending.length} background subagent(s)`
      onUpdate?.()
      await waitBgSettled(bgPending)
      if (!abortRequested) injectSettledBg()
      else return { stopped: 'aborted' as const }
    }
    return { stopped: 'limit' }
  } finally {
    if (isRoot) {
      running.value = false
      localActivity.value = ''
    }
  }
}

/** `edit /src/app.rs` — same salient-argument shape the native job line shows. */
function activityLine(name: string, input: Record<string, unknown>): string {
  let arg = ''
  try {
    arg = JSON.stringify(input ?? {})
  } catch {
    arg = ''
  }
  if (arg === '{}' || arg === 'undefined') return name
  arg = arg.replace(/["[\]{}]/g, '').replace(/,/g, ' ').trim()
  return arg.length > 150 ? `${name} ${arg.slice(0, 150)}…` : `${name} ${arg}`
}

// ─── local configs + sessions (localStorage; keys never persisted) ──
// Provider keys live in memory only — this module-level vault is the single
// shared holder so the setup wizards and the Agent panel see the same keys
// within one page load (a reload clears them by design).

const localKeyVault = new Map<string, string>()

export function setLocalKey(configId: string, key: string): void {
  if (!configId) return
  if (key) localKeyVault.set(configId, key)
  else localKeyVault.delete(configId)
  localKeyRevision.value++
  emitAgentEvent(AGENT_KEYS_EVENT)
}

export function getLocalKey(configId: string): string {
  return localKeyVault.get(configId) ?? ''
}

export function hasLocalKey(configId: string): boolean {
  return localKeyVault.has(configId)
}

export function forgetLocalKey(configId: string): void {
  localKeyVault.delete(configId)
  localKeyRevision.value++
  emitAgentEvent(AGENT_KEYS_EVENT)
}

function readJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    if (!raw) return fallback
    return JSON.parse(raw) as T
  } catch {
    return fallback
  }
}

function writeJson(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    // Quota or private mode — sessions simply don't persist.
  }
}

export function listLocalConfigs(): AgentConfig[] {
  return readJson<AgentConfig[]>(CONFIGS_KEY, [])
}

export function saveLocalConfig(config: AgentConfig): void {
  const all = listLocalConfigs().filter(c => c.id !== config.id)
  all.push({ ...config, hasKey: false })
  writeJson(CONFIGS_KEY, all)
  emitAgentEvent(AGENT_CONFIGS_EVENT)
}

export function deleteLocalConfig(id: string): void {
  writeJson(
    CONFIGS_KEY,
    listLocalConfigs().filter(c => c.id !== id),
  )
  emitAgentEvent(AGENT_CONFIGS_EVENT)
}

export function listLocalSessions(): AgentSession[] {
  const all = readJson<AgentSession[]>(SESSIONS_KEY, [])
  return all.sort((a, b) => (b.updatedAt || '').localeCompare(a.updatedAt || '')).slice(0, 100)
}

export function saveLocalSession(session: AgentSession): void {
  const all = readJson<AgentSession[]>(SESSIONS_KEY, [])
  const i = all.findIndex(s => s.id === session.id)
  if (i >= 0) all[i] = session
  else all.unshift(session)
  writeJson(SESSIONS_KEY, all.slice(0, 100))
}

export function deleteLocalSession(id: string): void {
  writeJson(
    SESSIONS_KEY,
    readJson<AgentSession[]>(SESSIONS_KEY, []).filter(s => s.id !== id),
  )
}

// ─── local semantic memory (keyword-only recall; the browser mints no
// vectors without an embeddings endpoint — rows stay comparable via the
// same rank/score contract as the native store) ──────────────────────

const localStorageMemory: MemoryStorage = {
  load: () => readJson<AgentMemory[]>(MEMORIES_KEY, []),
  save: all => writeJson(MEMORIES_KEY, all.slice(-500)),
}

export const localMemories = new LocalMemoryStore(localStorageMemory)

export function listLocalMemories(configId?: string): AgentMemory[] {
  return localMemories.list(configId)
}

export function deleteLocalMemory(id: string): void {
  localMemories.remove(id)
}

/** Bounded recall block for the browser system prompt ("" when empty). */
export function recallBlockLocal(configId: string, query: string, topK = 3): string {
  const hits: MemoryHit[] = localMemories.recall(configId, query, topK)
  if (!hits.length) return ''
  return renderRecallBlock(hits) + '\n(degraded: keyword-only recall — no embeddings on this transport)\n'
}

export function useAgent() {
  return {
    running,
    pendingApproval,
    localActivity,
    wasmAgentCatalog,
    runLocalAgent,
    listLocalConfigs,
    saveLocalConfig,
    deleteLocalConfig,
    setLocalKey,
    getLocalKey,
    hasLocalKey,
    forgetLocalKey,
    listLocalSessions,
    saveLocalSession,
    deleteLocalSession,
    listLocalMemories,
    deleteLocalMemory,
    recallBlockLocal,
    matchWildcard,
    decideLocalTool,
    isFailoverWorthy,
    listLocalMcpTools,
    splitMcpToolName,
  }
}

export type LocalAgentJob = {
  jobId: string
  sessionId: string
  configId: string
  status: string
  turnsUsed: number
  maxTurns: number
  usage: TokenUsage
  result?: string | null
  error?: string | null
  pending?: { tool: string; input: Record<string, unknown>; summary: string; question?: string | null } | null
};
