// CyberManju — pure helpers that make the agent thread readable.
//
// Everything here is transport-agnostic (native REST job + browser loop share
// the same `ChatMessage[]`), side-effect free and unit-tested, so the panel
// stays a renderer. Mirrors where it matters: tool titles follow the same
// salient-argument order as Rust `config::salient_arg`.

import type { AgentConfig, ChatMessage, PermissionRuleset, TokenUsage } from '@/types'
import { agentPermissionPreset } from '@/types'
import { decideLocalTool } from '@/composables/useAgent'

/** How each tool is announced while it runs (opencode-style verb + target). */
export interface ToolMeta {
  name: string
  /** Present progressive shown while the call is in flight. */
  verb: string
  /** Past tense shown in the collapsed row once it finishes. */
  done: string
  icon: string
}

const GENERIC: ToolMeta = { name: 'tool', verb: 'Running', done: 'Ran', icon: 'solar:widget-bold' }

export const AGENT_TOOL_META: Record<string, ToolMeta> = {
  read: { name: 'read', verb: 'Reading', done: 'Read', icon: 'solar:file-text-bold' },
  write: { name: 'write', verb: 'Writing', done: 'Wrote', icon: 'solar:diskette-bold' },
  edit: { name: 'edit', verb: 'Editing', done: 'Edited', icon: 'solar:pen-bold' },
  list: { name: 'list', verb: 'Listing', done: 'Listed', icon: 'solar:folder-bold' },
  grep: { name: 'grep', verb: 'Searching', done: 'Searched', icon: 'solar:file-search-bold' },
  glob: { name: 'glob', verb: 'Finding', done: 'Found', icon: 'solar:folder-open-bold' },
  bash: { name: 'bash', verb: 'Running', done: 'Ran', icon: 'solar:command-bold' },
  task: { name: 'task', verb: 'Delegating', done: 'Delegated', icon: 'solar:user-circle-bold' },
  question: { name: 'question', verb: 'Asking', done: 'Asked', icon: 'solar:question-circle-bold' },
  memory_recall: { name: 'memory_recall', verb: 'Recalling', done: 'Recalled', icon: 'solar:history-bold' },
  memory_remember: { name: 'memory_remember', verb: 'Remembering', done: 'Remembered', icon: 'solar:bookmark-bold' },
  self_research: { name: 'self_research', verb: 'Researching', done: 'Researched', icon: 'solar:telescope-bold' },
  skill_save: { name: 'skill_save', verb: 'Saving skill', done: 'Saved skill', icon: 'solar:backpack-bold' },
  mcp_attach: { name: 'mcp_attach', verb: 'Attaching MCP', done: 'Attached MCP', icon: 'solar:plug-circle-bold' },
  repo_analyze: { name: 'repo_analyze', verb: 'Analyzing repo', done: 'Analyzed repo', icon: 'solar:git-branch-bold' },
  os_exec: { name: 'os_exec', verb: 'Running cybsh', done: 'Ran cybsh', icon: 'solar:command-bold' },
  ui_open_panel: { name: 'ui_open_panel', verb: 'Opening panel', done: 'Opened panel', icon: 'solar:monitor-bold' },
  ui_notify: { name: 'ui_notify', verb: 'Notifying', done: 'Notified', icon: 'solar:bell-bold' },
  secret_list: { name: 'secret_list', verb: 'Listing vault', done: 'Listed vault', icon: 'solar:wallet-bold' },
  secret_get: { name: 'secret_get', verb: 'Revealing secret', done: 'Revealed secret', icon: 'solar:key-bold' },
  todo_add: { name: 'todo_add', verb: 'Adding todo', done: 'Added todo', icon: 'solar:checklist-bold' },
  todo_list: { name: 'todo_list', verb: 'Listing todos', done: 'Listed todos', icon: 'solar:checklist-bold' },
  todo_done: { name: 'todo_done', verb: 'Completing todo', done: 'Completed todo', icon: 'solar:check-circle-bold' },
}

export function toolMeta(name: string): ToolMeta {
  if (AGENT_TOOL_META[name]) return AGENT_TOOL_META[name]
  if (name.startsWith('mcp__')) {
    return { name, verb: 'MCP', done: 'MCP', icon: 'solar:plug-circle-bold' }
  }
  return { ...GENERIC, name }
}

/** Same precedence as Rust `config::salient_arg`. */
export function salientArg(input: unknown): string {
  const obj = (input ?? {}) as Record<string, unknown>
  const pick = (key: string): string => {
    const v = obj[key]
    return typeof v === 'string' ? v : ''
  }
  return (
    pick('command') ||
    pick('pattern') ||
    pick('path') ||
    pick('glob') ||
    pick('query') ||
    pick('url') ||
    pick('repo') ||
    pick('name') ||
    pick('goal') ||
    pick('question') ||
    pick('text') ||
    pick('title') ||
    ''
  )
}

const clip = (text: string, max = 90): string => {
  const t = text.replace(/\s+/g, ' ').trim()
  return t.length > max ? `${t.slice(0, max - 1)}…` : t
}

/** One-line human title: `Reading src/agent_api.rs`. */
export function toolTitle(name: string, input: unknown): string {
  const meta = toolMeta(name)
  if (name === 'write') {
    const obj = (input ?? {}) as Record<string, unknown>
    const bytes = typeof obj.content === 'string' ? obj.content.length : 0
    const target = salientArg(input)
    return target ? `${meta.verb} ${target} (${bytes} B)` : meta.verb
  }
  if (name === 'edit') {
    const obj = (input ?? {}) as Record<string, unknown>
    const removed = typeof obj.old_block === 'string' ? obj.old_block.length : 0
    const target = salientArg(input)
    return target ? `${meta.verb} ${target} (−${removed} B)` : meta.verb
  }
  const target = salientArg(input)
  return target ? `${meta.verb} ${clip(target)}` : meta.verb
}

// ─── transcript → renderable thread ───────────────────────────────────────

export type ToolRowState = 'running' | 'done' | 'denied' | 'error'

export interface ToolRow {
  key: string
  callId: string | null
  name: string
  input: unknown
  title: string
  result: string | null
  state: ToolRowState
}

export interface MessageRow {
  kind: 'message'
  key: string
  message: ChatMessage
}

export interface ToolGroupRow {
  kind: 'tools'
  key: string
  /** Text the assistant wrote alongside its tool calls. */
  lead: string
  rows: ToolRow[]
}

export type ThreadRow = MessageRow | ToolGroupRow

/** Classify a tool result by its machine prefix (house contract). */
export function toolStateFromResult(result: string | null): ToolRowState {
  if (result == null) return 'running'
  const head = result.trimStart().toLowerCase()
  if (head.startsWith('denied:') || head.startsWith('deny:')) return 'denied'
  if (
    head.startsWith('error:') ||
    head.startsWith('not_found:') ||
    head.startsWith('conflict:') ||
    head.startsWith('unsupported:') ||
    head.startsWith('invalid:') ||
    head.startsWith('integrity:') ||
    head.startsWith('network:') ||
    head.startsWith('auth:') ||
    head.startsWith('rate_limited:') ||
    head.startsWith('context:') ||
    head.startsWith('too_large:') ||
    head.startsWith('truncated:')
  ) {
    return 'error'
  }
  return 'done'
}

function callsFrom(message: ChatMessage): Array<{ id: string | null; name: string; input: unknown }> {
  const raw = message.toolInput
  if (!Array.isArray(raw)) return []
  return raw.map(c => {
    const call = (c ?? {}) as { id?: string; name?: string; input?: unknown }
    return {
      id: typeof call.id === 'string' ? call.id : null,
      name: typeof call.name === 'string' ? call.name : 'tool',
      input: call.input ?? {},
    }
  })
}

/**
 * Pair `assistant_tool` messages with the `tool` results that answer them and
 * collapse the transcript into renderable rows. A tool still awaiting its
 * result while a run is live renders as `running` — that is what turns the
 * frozen thread into a live one.
 */
export function buildThread(messages: ChatMessage[], running = false): ThreadRow[] {
  const rows: ThreadRow[] = []
  /** No result yet: live run ⇒ in flight; stopped run ⇒ it never landed. */
  const stateFor = (result: string | null): ToolRowState =>
    result == null ? (running ? 'running' : 'error') : toolStateFromResult(result)
  const resultsById = new Map<string, string>()
  const resultNames = new Map<string, string>()
  const orphanResults: Array<{ id: string | null; name: string | null; content: string }> = []

  for (const m of messages) {
    if (m.role === 'tool') {
      const id = m.toolCallId ?? null
      if (id && !resultsById.has(id)) {
        resultsById.set(id, m.content ?? '')
        if (m.toolName) resultNames.set(id, m.toolName)
      } else {
        orphanResults.push({ id, name: m.toolName ?? null, content: m.content ?? '' })
      }
    }
  }

  const consumed = new Set<string>()
  let orphanAt = 0
  const takeOrphan = (name: string): string | null => {
    while (orphanAt < orphanResults.length) {
      const o = orphanResults[orphanAt++]
      if (o.name === name || o.name == null) return o.content
    }
    return null
  }

  messages.forEach((m, i) => {
    if (m.role === 'tool') return
    if (m.role === 'assistant_tool') {
      const rowsOut: ToolRow[] = []
      callsFrom(m).forEach((call, n) => {
        let result: string | null = null
        if (call.id && resultsById.has(call.id)) {
          result = resultsById.get(call.id) ?? null
          consumed.add(call.id)
        } else {
          result = takeOrphan(call.name)
        }
        rowsOut.push({
          key: `${i}-${n}`,
          callId: call.id,
          name: call.name,
          input: call.input,
          title: toolTitle(call.name, call.input),
          result,
          state: stateFor(result),
        })
      })
      // Any tool result the model's wire lost (missing ids) still shows up.
      if (rowsOut.length === 0) {
        const orphan = takeOrphan(m.toolName ?? '')
        if (orphan != null) {
          rowsOut.push({
            key: `${i}-0`,
            callId: null,
            name: m.toolName ?? 'tool',
            input: {},
            title: toolTitle(m.toolName ?? 'tool', {}),
            result: orphan,
            state: stateFor(orphan),
          })
        }
      }
      // Results that arrived without a matching assistant_tool block.
      while (orphanAt < orphanResults.length) {
        const o = orphanResults[orphanAt]
        rowsOut.push({
          key: `${i}-x${orphanAt}`,
          callId: o.id,
          name: o.name ?? 'tool',
          input: {},
          title: toolTitle(o.name ?? 'tool', {}),
          result: o.content,
          state: stateFor(o.content),
        })
        orphanAt++
      }
      if (!rowsOut.length) return
      rows.push({ kind: 'tools', key: `t${i}`, lead: m.content ?? '', rows: rowsOut })
      return
    }
    rows.push({ kind: 'message', key: `m${i}`, message: m })
  })

  // Results whose assistant block never arrived (lost wire, or reversed
  // order) — dropping them would silently lose the agent's last answer.
  for (const [id, content] of resultsById) {
    if (consumed.has(id)) continue
    const name = resultNames.get(id) ?? 'tool'
    rows.push({
      kind: 'tools',
      key: `u${id}`,
      lead: '',
      rows: [
        {
          key: `u${id}`,
          callId: id,
          name,
          input: {},
          title: toolTitle(name, {}),
          result: content,
          state: stateFor(content),
        },
      ],
    })
  }

  // Trailing tool results with no group (defensive; normally unreachable).
  while (orphanAt < orphanResults.length) {
    const o = orphanResults[orphanAt++]
    if (consumed.has(o.id ?? '')) continue
    if (o.id && resultsById.has(o.id)) continue
    rows.push({
      kind: 'tools',
      key: `o${orphanAt}`,
      lead: '',
      rows: [
        {
          key: `o${orphanAt}`,
          callId: o.id,
          name: o.name ?? 'tool',
          input: {},
          title: toolTitle(o.name ?? 'tool', {}),
          result: o.content,
          state: stateFor(o.content),
        },
      ],
    })
  }
  return rows
}

// ─── context + cost awareness ─────────────────────────────────────────────

/** ~4 chars/token: a rough, honest proxy for what the next call re-sends. */
export function estimateTranscriptTokens(messages: ChatMessage[]): number {
  let chars = 0
  for (const m of messages) {
    chars += (m.content ?? '').length
    if (m.toolInput) chars += JSON.stringify(m.toolInput).length
  }
  return Math.ceil(chars / 4)
}

/** Approximate context window by model family — always labelled EST. */
export function contextWindowFor(model: string): number {
  const m = model.toLowerCase()
  if (m.includes('gemini')) return 1_048_576
  if (m.includes('claude')) return 200_000
  if (m.includes('gpt-5') || m.includes('gpt-4.1') || m.includes('o3') || m.includes('o4')) {
    return 200_000
  }
  if (m.includes('deepseek')) return 65_536
  if (m.includes('grok')) return 131_072
  if (m.includes('mistral')) return 131_072
  if (m.includes('llama') || m.includes('qwen') || m.includes('gemma')) return 131_072
  return 128_000
}

/** Approximate list price per 1M tokens (input, output). EST only. */
const PRICE_PER_MTI: Array<{ match: RegExp; input: number; output: number }> = [
  { match: /claude-(opus|sonnet|haiku)/, input: 3, output: 15 },
  { match: /gpt-5/, input: 1.25, output: 10 },
  { match: /gpt-4o/, input: 2.5, output: 10 },
  { match: /gemini-.*flash/, input: 0.3, output: 2.5 },
  { match: /gemini-/, input: 1.25, output: 10 },
  { match: /deepseek/, input: 0.27, output: 1.1 },
  { match: /grok-/, input: 3, output: 15 },
  { match: /mistral-large/, input: 2, output: 6 },
  { match: /llama/, input: 0.59, output: 0.79 },
]

/** USD estimate for a usage total, or `null` when the model is unknown. */
export function estimateCost(model: string, usage: TokenUsage): number | null {
  const m = model.toLowerCase()
  const price = PRICE_PER_MTI.find(p => p.match.test(m))
  if (!price) return null
  return (usage.inputTokens / 1_000_000) * price.input + (usage.outputTokens / 1_000_000) * price.output
}

// ─── context pressure: prune the wire, never the transcript ──────────────

/** Head/tail kept when a tool result is pruned — enough to stay useful. */
const PRUNE_HEAD = 400
const PRUNE_TAIL = 200
/** Tool results below this are not worth cutting (and stay byte-exact). */
const PRUNE_MIN_CHARS = 1200
/** Newest messages are never touched: the model needs the live turn. */
const PRUNE_KEEP_LAST = 12

export interface WirePruneOptions {
  model: string
  /** Share of the context window that starts pruning (Hermes compresses far
   *  earlier; we only ever cut tool *outputs*, the safest 1:1 reclaim). */
  thresholdPct?: number
  keepLast?: number
  minChars?: number
}

export interface WirePruneResult {
  /** What to send to the provider — the same array when nothing was cut. */
  messages: ChatMessage[]
  pruned: number
  reclaimedChars: number
}

/**
 * Cut oversized **tool results** out of the copy handed to the provider once
 * the transcript crosses the context threshold. User/assistant text and the
 * newest messages are never modified, and the on-screen transcript is never
 * mutated — the caller keeps the full history (Hermes' `proactive_prune`
 * does the same no-LLM first pass before any summarization).
 *
 * A pruned result starts with `truncated:` so the machine prefix contract
 * still classifies it (`toolStateFromResult` → error/badge, not "done").
 */
export function prepareWireMessages(
  messages: ChatMessage[],
  opts: WirePruneOptions,
): WirePruneResult {
  const thresholdPct = opts.thresholdPct ?? 0.85
  const keepLast = opts.keepLast ?? PRUNE_KEEP_LAST
  const minChars = opts.minChars ?? PRUNE_MIN_CHARS
  const threshold = Math.floor(contextWindowFor(opts.model) * thresholdPct)
  const estimate = estimateTranscriptTokens(messages)
  if (estimate <= threshold) return { messages, pruned: 0, reclaimedChars: 0 }

  // +64 tokens of headroom so one pass lands under the line instead of
  // hovering on it and re-pruning every turn.
  let budget = estimate - threshold + 64
  const out = messages.slice()
  let pruned = 0
  let reclaimedChars = 0
  const lastKept = out.length - keepLast
  for (let i = 0; i < lastKept && budget > 0; i++) {
    const m = out[i]
    if (m.role !== 'tool') continue
    const text = m.content ?? ''
    if (text.length <= minChars) continue
    const head = text.slice(0, PRUNE_HEAD)
    const tail = text.slice(-PRUNE_TAIL)
    const cut = text.length - PRUNE_HEAD - PRUNE_TAIL
    out[i] = {
      ...m,
      content:
        `truncated: pruned ${cut} chars (context budget) — re-run the tool for the full text\n` +
        `${head}\n…\n${tail}`,
    }
    pruned += 1
    reclaimedChars += cut
    budget -= Math.ceil(cut / 4)
  }
  return { messages: out, pruned, reclaimedChars }
}

// ─── capability surface ───────────────────────────────────────────────────

export interface ToolPermission {
  tool: string
  action: 'allow' | 'ask' | 'deny'
}

/**
 * What the agent is currently allowed to do — the same `decide` evaluation
 * the loop applies, surfaced so a human can see the sandbox before running.
 */
export function toolPermissions(
  rules: PermissionRuleset | undefined,
  kind: 'build' | 'plan',
  tools: string[],
): ToolPermission[] {
  const ruleset = rules ?? { default: 'ask' as const, rules: {} }
  return tools.map(tool => {
    const d = decideLocalTool(ruleset, kind, tool, {})
    return { tool, action: d.kind === 'allow' ? 'allow' : d.kind === 'deny' ? 'deny' : 'ask' }
  })
}

/** One-word label for a ruleset, shown in the capability strip. */
export function permissionLabel(rules: PermissionRuleset | undefined): string {
  if (!rules) return 'DEFAULT'
  const actions = Object.values(rules.rules).flatMap(r => (Array.isArray(r) ? r.map(([, a]) => a) : [r]))
  // No overrides: the default alone decides — nothing ever asks (YOLO) or
  // everything asks (STRICT).
  if (actions.length === 0) return rules.default === 'ask' ? 'STRICT' : 'YOLO'
  if (rules.default === 'allow') return actions.every(a => a === 'allow') ? 'OPEN' : 'MIXED'
  return 'BALANCED'
}

// ─── Hermes parity: schema shaping + audit lines ────────────────────────────

/**
 * First TS step of AGENT-REVIEW D7 (Hermes strips denied tools from the
 * schema): drop every tool the loop would deny with empty input, so the schema
 * builder can advertise only what the model may actually attempt. A plan-kind
 * session therefore loses `edit`/`write`/`bash` automatically — the same
 * persona rule `decideLocalTool` enforces at runtime, so the surface cannot
 * drift from enforcement.
 */
export function stripDeniedTools(
  tools: string[],
  rules: PermissionRuleset | undefined,
  kind: 'build' | 'plan',
): string[] {
  const ruleset = rules ?? { default: 'ask' as const, rules: {} }
  return tools.filter(t => decideLocalTool(ruleset, kind, t, {}).kind !== 'deny')
}

/**
 * One-line human audit of a permission decision, reusing the salient argument
 * so the log reads like the job line (`ASK bash "rm -rf /" — needs approval`).
 */
export function explainDecision(
  rules: PermissionRuleset | undefined,
  kind: 'build' | 'plan',
  tool: string,
  input: unknown,
): string {
  const ruleset = rules ?? { default: 'ask' as const, rules: {} }
  const record = (input ?? {}) as Record<string, unknown>
  const d = decideLocalTool(ruleset, kind, tool, record)
  const target = salientArg(input)
  const what = target ? `${tool} ${clip(target, 60)}` : tool
  if (d.kind === 'allow') return `ALLOW ${what} — allowed by the permission ruleset`
  if (d.kind === 'ask') return `ASK ${what} — ${d.summary}`
  return `DENY ${what} — ${d.reason}`
}

// ─── YOLO mode: one tap to allow everything ───────────────────────────────

/**
 * True when the ruleset allows every tool without asking: `allow` default
 * with no `ask`/`deny` anywhere (granular pairs included). Mirrors the Rust
 * `is_tool_denied_everywhere` probe shape — an empty-allow ruleset is YOLO,
 * anything with a single ask or deny is not.
 */
export function isYoloRuleset(rules: PermissionRuleset | undefined): boolean {
  if (!rules || rules.default !== 'allow') return false
  return Object.values(rules.rules).every(r =>
    r === 'allow' || (Array.isArray(r) && r.length > 0 && r.every(([, a]) => a === 'allow')),
  )
}

/** True when the config runs fully unattended: YOLO ruleset + build kind + auto-approve. */
export function isYoloConfig(cfg: Pick<AgentConfig, 'permission' | 'agentKind' | 'autoApprove'> | null | undefined): boolean {
  if (!cfg) return false
  return cfg.agentKind === 'build' && cfg.autoApprove && isYoloRuleset(cfg.permission)
}

/**
 * Pure YOLO enable: allow-all ruleset, build persona (plan denies mutations
 * no matter the rules), auto-approve on, every attached MCP server enabled.
 * Rails that survive by design: plan is left behind only via the kind flip
 * (announced in the toast), protected standing-order writes still ask, and
 * the runtime `decide` gate stays in place.
 */
export function applyYoloToConfig(cfg: AgentConfig): AgentConfig {
  const mcpServers = Object.fromEntries(
    Object.entries(cfg.mcpServers ?? {}).map(([name, server]) => [name, { ...server, enabled: true }]),
  )
  return {
    ...cfg,
    agentKind: 'build',
    permission: agentPermissionPreset('yolo'),
    autoApprove: true,
    mcpServers,
  }
}

/** Pure YOLO disable: back to the balanced ask-by-default posture (MCP enable flags untouched). */
export function applyBalancedToConfig(cfg: AgentConfig): AgentConfig {
  return {
    ...cfg,
    permission: agentPermissionPreset('balanced'),
    autoApprove: false,
  }
}
