// CyberManju OS — UseStudioAgent (src/composables/useStudioAgent.ts).
// Studio agent driver — one interface over both OS agent transports.
//
// Native (desktop / Docker / REST): server-side loop via the app store
//   (startAgentRun + 1.5s job poll + live transcript re-read).
// WASM static host: browser-local loop from useAgent (volume tools only).
//
// The code editor (CodeStudio) and any other panel consume this instead of
// re-implementing AgentPanel's run/approval/queue logic.

import { ref, computed, watch, onBeforeUnmount, nextTick } from 'vue'
import { useAppStore } from '@/stores/app'
import { isStaticHost } from '@/composables/useTauri'
import { deploymentStorageEventKey } from '@/utils/deploymentScope'
import {
  useAgent,
  runLocalAgent,
  listLocalConfigs,
  saveLocalConfig,
  listLocalSessions,
  saveLocalSession,
  deleteLocalSession,
  abortLocalRun,
  getLocalKey,
  setLocalKey,
  hasLocalKey,
  recallBlockLocal,
  readChatConfigId,
  writeChatConfigId,
  CHAT_CONFIG_EVENT,
  CHAT_CONFIG_ID_KEY,
  LOCAL_CONFIGS_KEY,
  AGENT_CONFIGS_EVENT,
  AGENT_KEYS_EVENT,
  localKeyRevision,
} from '@/composables/useAgent'
import { buildThread, estimateTranscriptTokens, contextWindowFor, estimateCost } from '@/utils/agentUi'
import { agentErrorHint } from '@/types'
import type { AgentConfig, AgentJob, AgentSession, ProviderPreset } from '@/types'
import type { LocalFailoverRoute } from '@/composables/useAgent'

export interface FileCtx {
  label: string
  path: string
  language: string
  /** Already truncated by the caller (4–8 KiB recommended). */
  content: string
  selection?: string
}

function newLocalId(prefix: string): string {
  try {
    return `${prefix}-${crypto.randomUUID().slice(0, 8)}`
  } catch {
    return `${prefix}-${Date.now().toString(36)}`
  }
}

const localNow = () => new Date().toISOString()

function localSystemPrompt(config: AgentConfig): string {
  const root = config.workingDir ? `/${config.workingDir}` : '/'
  return (
    `You are CyberManju, an AI coding agent running fully in the browser over a local file volume.\n` +
    `Working root: ${root}\n` +
    `Agent mode: ${config.agentKind} (plan = read-only, never edit).\n` +
    `SANDBOX: browser file volume — read/list/grep/glob/write/edit plus cybsh-subset bash, ` +
    `one bounded subagent (task) with the full file/shell toolset, and HTTP MCP servers (mcp__*). Device shell ` +
    `and stdio MCP need the dashboard and answer unsupported: there.\n` +
    `TOOLS — paths: leading / = volume root, else working-dir-relative.\n` +
    `- read {path}: always read a file before editing it; the output ends with a ` +
    `\`[blake3:<hex>]\` line — pass it as expected_hash on edit, and never write it back ` +
    `(write/edit strip it automatically).\n` +
    `- list {path?}: one directory level; orient at / first.\n` +
    `- grep {pattern, path?, limit?}: regex over contents (invalid regex searches literally).\n` +
    `- glob {pattern, path?}: find files (* stays in one segment, ** crosses).\n` +
    `- edit {path, old_block, new_block, expected_hash?}: replace ONE exact block; missing → not_found:, ` +
    `ambiguous → conflict:, then re-read and send a larger block. expected_hash pins the file ` +
    `you read so a concurrent writer cannot slip through.\n` +
    `- write {path, content}: full-file create/overwrite; prefer edit for small changes.\n` +
    `- bash {command}: cybsh volume commands; device verbs answer unsupported:.\n` +
    `- task {goal, context?}: one bounded subagent with the full file/shell toolset (read/edit/write/bash, 5 turns).\n` +
    `- question {question}: ask the human when genuinely blocked — sparingly.\n` +
    `- self_research {query, path?, limit?}: introspect YOUR OWN source — sweep this repo for how something works, get file:line snippets back. Read-only, plan-safe. Call it before claiming how the agent works.\n` +
    `- skill_save {name, description, content}: persist a reusable skill into the \`.cybermanju\` container — survives restarts, syncs across devices.\n` +
    `- mcp_attach {name, url}: attach an HTTP MCP server to this assistant persistently (verified before saving; stdio refused here).\n` +
    `- repo_analyze {repo, branch?}: analyze any public GitHub repo WITHOUT cloning (metadata + tree + README over HTTPS; no git needed, works everywhere).\n` +
    `- os_exec {command}: run ONE cybsh line against the OS volume — same verbs as bash's cybsh subset, ` +
    `never a device shell (non-cybsh verbs answer unsupported:). One line per call; no pipes or chains.\n` +
    `- ui_open_panel {panel, tab?, path?}: open a UI panel for the user (files, terminal, agent, settings, …). ` +
    `Unknown ids answer not_found:; optional tab/path navigate inside the panel. Read-only and always allowed.\n` +
    `- ui_notify {level, message}: show the user a notification (info/success/warning/error). Always allowed; use sparingly.\n` +
    `- secret_list {}: list vault secret METADATA only (id/title/kind/username/url/tags/hasValue) — ` +
    `never values. Pair with secret_get only when the user explicitly asked you to use a stored credential.\n` +
    `- secret_get {id}: reveal ONE stored secret's plaintext. The result carries the plaintext to you — ` +
    `default permission is ASK (the approval card shows the title, never the value). Only call it when ` +
    `the user explicitly asked you to use this credential.\n` +
    `- mcp__server__tool: attached HTTP MCP servers only; stdio answers unsupported:.\n` +
    `- memory_recall {query, top_k?}: search long-term memory (past sessions, stored facts). ` +
    `Bounded and possibly stale — verify before acting.\n` +
    `- memory_remember {text}: store ONE durable fact for future sessions; one fact per call, ` +
    `never secrets or whole files.\n` +
    `INTERFACE: the Terminal panel runs cybsh — the same shell as your bash tool. Its ` +
    `\`theme\`/\`ui\` verbs customize the whole interface and persist to the volume mirror: ` +
    `\`ui theme <id>\`, \`ui accent <#hex|default> [--for <theme>]\`, \`ui density|glass|motion|glow\`, ` +
    `and inspect with \`ui get\`, \`ui vars [filter]\`, \`ui palette [theme]\`. When asked about ` +
    `the interface or cybsh verbs, call self_research first (topics: themes, cybsh verbs) ` +
    `and cite file:line — never invent verb names.\n` +
    `STANDING ORDERS: AGENTS.md, SKILL.md, .cybermanju/rules.md and persisted skills ` +
    `(.cybermanju/skills/*/SKILL.md) define your instructions, ` +
    `so writing one always asks for approval — AUTO APPROVE never covers them.\n` +
    `SELF-UNDERSTANDING: when asked how YOU work, call self_research first and cite the files it returns; ` +
    `extend yourself with skill_save (knowledge) and mcp_attach (HTTP tools).\n` +
    `WORKFLOW: orient (list/glob) → read → act → verify. Small verified steps; ` +
    `never invent file contents. Denials are information — work around them, never ` +
    `retry identically. Report errors with their machine prefix. Answer concisely; ` +
    `lead with what changed (file:line).`
  )
}

/** Shared singleton — editor + agent panel observe the same run. */
let shared: ReturnType<typeof createDriver> | null = null

function createDriver() {
  const store = useAppStore()
  const agent = useAgent()

  const wasmMode = computed(() => {
    try {
      return isStaticHost()
    } catch {
      return false
    }
  })

  // Shared with AgentPanel via localStorage: switching the assistant in one
  // panel follows in the other, and the sealed key is never asked for twice.
  const chatConfigId = ref(readChatConfigId())
  const serverViewing = ref<AgentSession | null>(null)
  const localViewing = ref<AgentSession | null>(null)
  const viewing = computed(() => (wasmMode.value ? localViewing.value : serverViewing.value))
  const localPresets = ref<ProviderPreset[]>([])
  const localConfigs = ref<AgentConfig[]>([])
  const localSessions = ref<AgentSession[]>([])
  const localKeys = ref<Record<string, string>>({})
  const localJob = ref<AgentJob | null>(null)
  const answerInput = ref('')
  const queue = ref<string[]>([])
  const startedAt = ref(0)

  const configs = computed(() => (wasmMode.value ? localConfigs.value : store.agentConfigs))
  const sessions = computed(() => (wasmMode.value ? localSessions.value : store.agentSessions))
  const chatConfig = computed(() => configs.value.find(c => c.id === chatConfigId.value) ?? null)
  const activeJob = computed(() => (wasmMode.value ? localJob.value : store.activeAgentJob))

  function setViewing(s: AgentSession | null) {
    if (wasmMode.value) localViewing.value = s
    else serverViewing.value = s
  }

  function refreshLocal() {
    // Touch the revision so hasKey badges re-evaluate when another panel
    // seals/forgets a key (the vault itself is a non-reactive Map).
    void localKeyRevision.value
    localConfigs.value = listLocalConfigs().map(c => ({ ...c, hasKey: !!localKeys.value[c.id] || hasLocalKey(c.id) || c.hasKey }))
    localSessions.value = listLocalSessions()
    // Drop a stale selection (config deleted elsewhere), else adopt the
    // shared selection / first config so the Studio never idles on empty.
    if (chatConfigId.value && !localConfigs.value.some(c => c.id === chatConfigId.value)) {
      const shared = readChatConfigId()
      chatConfigId.value = localConfigs.value.some(c => c.id === shared) ? shared : ''
    }
    if (!chatConfigId.value) {
      const shared = readChatConfigId()
      if (shared && localConfigs.value.some(c => c.id === shared)) chatConfigId.value = shared
      else if (localConfigs.value.length) chatConfigId.value = localConfigs.value[0].id
    }
  }

  function adoptSharedSelection(id: string) {
    if (!id) return
    if (chatConfigId.value === id) return
    // WASM: only adopt ids that exist locally; server mode validates below
    // via the store list watcher.
    if (wasmMode.value && !localConfigs.value.some(c => c.id === id)) {
      refreshLocal()
      if (!localConfigs.value.some(c => c.id === id)) return
    }
    chatConfigId.value = id
  }

  let studioSyncWired = false
  function wireSharedSelection() {
    if (studioSyncWired || typeof window === 'undefined') return
    studioSyncWired = true
    window.addEventListener(CHAT_CONFIG_EVENT, (e: Event) => {
      adoptSharedSelection((e as CustomEvent<string>).detail ?? '')
    })
    window.addEventListener('storage', (e: StorageEvent) => {
      const key = deploymentStorageEventKey(e.key)
      if (key === CHAT_CONFIG_ID_KEY && e.newValue) adoptSharedSelection(e.newValue)
      if (key === LOCAL_CONFIGS_KEY) refreshLocal()
    })
    window.addEventListener(AGENT_CONFIGS_EVENT, () => refreshLocal())
    window.addEventListener(AGENT_KEYS_EVENT, () => refreshLocal())
  }
  wireSharedSelection()

  // Persist every local change so the Agent panel (and a reload) follows.
  watch(chatConfigId, id => {
    if (id && readChatConfigId() !== id) writeChatConfigId(id)
  })
  // Server-mode configs live in Pinia: follow shared selection + deletions.
  watch(() => store.agentConfigs.map(c => c.id).join(','), () => {
    if (wasmMode.value) return
    const ids = new Set(store.agentConfigs.map(c => c.id))
    if (chatConfigId.value && !ids.has(chatConfigId.value)) {
      const shared = readChatConfigId()
      chatConfigId.value = shared && ids.has(shared) ? shared : (store.agentConfigs[0]?.id ?? '')
    }
    if (!chatConfigId.value) {
      const shared = readChatConfigId()
      if (shared && ids.has(shared)) chatConfigId.value = shared
      else if (store.agentConfigs.length) chatConfigId.value = store.agentConfigs[0].id
    }
  })

  function localPresetFor(config: AgentConfig): ProviderPreset | null {
    return (
      localPresets.value.find(p => p.id === config.providerId) ??
      store.agentProviders.find(p => p.id === config.providerId) ??
      null
    )
  }

  const pendingApproval = computed(() => {
    if (wasmMode.value) {
      const p = agent.pendingApproval.value
      if (!p) return null
      return { tool: p.tool, input: p.input, summary: p.summary, question: p.question ?? null }
    }
    const job = activeJob.value
    if (!job || job.status !== 'waiting_approval' || !job.pending) return null
    if (viewing.value && job.sessionId !== viewing.value.id) return null
    return job.pending
  })

  const jobActive = computed(() => {
    if (wasmMode.value) return agent.running.value
    const s = activeJob.value?.status
    return s === 'running' || s === 'waiting_approval'
  })

  const jobLine = computed(() => {
    const job = activeJob.value
    if (!job) return ''
    const bits = [
      wasmMode.value ? 'LOCAL JOB' : `JOB ${job.jobId.slice(0, 8)}…`,
      job.status.toUpperCase(),
      `TURN ${job.turnsUsed}/${job.maxTurns}`,
    ]
    if (job.activity) bits.push(job.activity)
    if (queue.value.length) bits.push(`${queue.value.length} QUEUED`)
    return bits.join(' · ')
  })

  const jobError = computed(() => {
    if (wasmMode.value) return localJob.value?.error ?? ''
    return activeJob.value?.error ?? ''
  })
  const jobHint = computed(() => {
    if (!jobError.value) return ''
    const d = agentErrorHint(jobError.value)
    return `${d.prefix}: ${d.hint}`
  })

  const threadRows = computed(() => buildThread(viewing.value?.messages ?? [], jobActive.value))
  const contextTokens = computed(() => estimateTranscriptTokens(viewing.value?.messages ?? []))
  const contextWindow = computed(() => contextWindowFor(chatConfig.value?.model || viewing.value?.model || ''))
  const contextPct = computed(() => {
    const w = contextWindow.value || 1
    return Math.max(0, Math.min(100, Math.round((contextTokens.value / w) * 100)))
  })
  const costUsd = computed(() => {
    const v = viewing.value
    return v ? estimateCost(v.model, v.usage) : null
  })

  function withFileCtx(prompt: string, files: FileCtx[]): string {
    if (!files.length) return prompt
    const blocks = files.map(f => {
      const sel = f.selection ? `\n<selection>\n${f.selection}\n</selection>` : ''
      return `<file path="${f.path}" language="${f.language}">\n${f.content}${sel}\n</file>`
    })
    return `${prompt}\n\n<attached-files>\n${blocks.join('\n')}\n</attached-files>`
  }

  async function ensureCatalog() {
    if (wasmMode.value) {
      try {
        const presets = (await agent.wasmAgentCatalog()) as ProviderPreset[]
        if (presets.length) localPresets.value = presets
      } catch {
        localPresets.value = []
      }
      refreshLocal()
      return
    }
    await Promise.allSettled([store.fetchAgentProviders(), store.fetchAgentConfigs(), store.fetchAgentSessions()])
  }

  async function newSession(): Promise<AgentSession | null> {
    if (!chatConfigId.value) return null
    if (wasmMode.value) {
      const cfg = localConfigs.value.find(c => c.id === chatConfigId.value)
      if (!cfg) return null
      const now = localNow()
      const s: AgentSession = {
        id: newLocalId('ses'), title: 'Untitled session', configId: cfg.id,
        providerId: cfg.providerId, model: cfg.model, agentKind: cfg.agentKind,
        workingDir: cfg.workingDir, messages: [],
        usage: { inputTokens: 0, outputTokens: 0 }, createdAt: now, updatedAt: now,
      }
      saveLocalSession(s)
      refreshLocal()
      setViewing(s)
      return s
    }
    try {
      const { invoke } = await import('@/composables/useTauri')
      const created = await invoke<AgentSession>('create_agent_session', { configId: chatConfigId.value })
      await store.fetchAgentSessions()
      setViewing(created)
      return created
    } catch (e) {
      store.notifyError('Failed to create session', e)
      return null
    }
  }

  async function loadSession(id: string) {
    if (wasmMode.value) {
      setViewing(listLocalSessions().find(s => s.id === id) ?? null)
      return
    }
    setViewing(await store.loadAgentSession(id))
  }

  async function removeSession(id: string) {
    if (wasmMode.value) {
      deleteLocalSession(id)
      refreshLocal()
    } else {
      await store.deleteAgentSession(id)
    }
    if (viewing.value?.id === id) setViewing(null)
  }

  async function send(rawPrompt: string, files: FileCtx[] = []): Promise<boolean> {
    const prompt = withFileCtx(rawPrompt.trim(), files)
    if (!prompt || !chatConfigId.value) return false
    if (jobActive.value) {
      queue.value.push(prompt)
      return true
    }
    if (wasmMode.value) return sendLocal(prompt)
    // Ensure a session exists for this config; the job returns transcript via polling.
    let sessionId = viewing.value && viewing.value.configId === chatConfigId.value ? viewing.value.id : undefined
    if (!sessionId) {
      const created = await newSession()
      if (!created) return false
      sessionId = created.id
    }
    await store.startAgentRun(chatConfigId.value, prompt, sessionId)
    return true
  }

  async function sendLocal(prompt: string): Promise<boolean> {
    const cfg = localConfigs.value.find(c => c.id === chatConfigId.value)
    if (!cfg) {
      store.notifyError('No local config selected', 'save one in the Agent SETUP first')
      return false
    }
    const preset = localPresetFor(cfg)
    const base = (cfg.baseUrlOverride || preset?.baseUrl || '').replace(/\/$/, '')
    if (!base) {
      store.notifyError('No endpoint', 'set an endpoint override or pick a preset with one')
      return false
    }
    const key = localKeys.value[cfg.id] ?? getLocalKey(cfg.id) ?? ''
    // Fallback routes: same transcript continues on the next usable assistant
    // (extra account/key or another provider) when this one's call dies.
    // Mirrors AgentPanel.sendPromptLocal (max 4, skip self/dupes/unusable).
    const fallbackRoutes: LocalFailoverRoute[] = []
    {
      const seen = new Set([cfg.id])
      for (const fid of cfg.fallbackIds ?? []) {
        if (seen.has(fid) || fallbackRoutes.length >= 4) continue
        seen.add(fid)
        const fc = localConfigs.value.find(c => c.id === fid)
        if (!fc) continue
        const fp = localPresetFor(fc)
        const fbase = (fc.baseUrlOverride || fp?.baseUrl || '').replace(/\/$/, '')
        if (!fbase) continue
        const fkey = localKeys.value[fc.id] ?? getLocalKey(fc.id) ?? ''
        if (!fkey && !(fp?.keyless ?? false)) continue
        const fdialect = (fc.dialectOverride ?? fp?.dialect ?? 'openAi') as 'openAi' | 'anthropic'
        const fauth = (fc.authSchemeOverride ?? fp?.auth ?? 'bearer') as 'bearer' | 'header' | 'query' | 'none'
        const fheaders: Array<[string, string]> = [...(fp?.extraHeaders ?? [])]
        if (fauth === 'bearer' && fkey) fheaders.push(['Authorization', `Bearer ${fkey}`])
        else if (fauth === 'header') fheaders.push([fc.authNameOverride || fp?.authName || 'x-api-key', fkey])
        fallbackRoutes.push({ label: `${fc.name} · ${fc.model}`, baseUrl: fbase, dialect: fdialect, model: fc.model, headers: fheaders })
      }
    }
    if (!key && !(preset?.keyless ?? false) && !fallbackRoutes.length) {
      store.notifyError('No API key', 'seal the key first (held in memory only)')
      return false
    }
    const dialect = (cfg.dialectOverride ?? preset?.dialect ?? 'openAi') as 'openAi' | 'anthropic'
    const auth = (cfg.authSchemeOverride ?? preset?.auth ?? 'bearer') as 'bearer' | 'header' | 'query' | 'none'
    const headers: Array<[string, string]> = [...(preset?.extraHeaders ?? [])]
    if (auth === 'bearer' && key) headers.push(['Authorization', `Bearer ${key}`])
    else if (auth === 'header') headers.push([cfg.authNameOverride || preset?.authName || 'x-api-key', key])
    const url =
      auth === 'query'
        ? `${base}${dialect === 'anthropic' ? '/v1/messages' : '/chat/completions'}?${encodeURIComponent(cfg.authNameOverride || preset?.authName || 'key')}=${encodeURIComponent(key)}`
        : dialect === 'anthropic' ? `${base}/v1/messages` : `${base}/chat/completions`

    let session = viewing.value && viewing.value.configId === cfg.id ? viewing.value : null
    if (!session) {
      const now = localNow()
      session = {
        id: newLocalId('ses'), title: prompt.split(/\s+/).slice(0, 8).join(' ') || 'Untitled session',
        configId: cfg.id, providerId: cfg.providerId, model: cfg.model,
        agentKind: cfg.agentKind, workingDir: cfg.workingDir, messages: [],
        usage: { inputTokens: 0, outputTokens: 0 }, createdAt: now, updatedAt: now,
      }
      saveLocalSession(session)
      refreshLocal()
      setViewing(session)
    }
    session.messages.push({ role: 'user', content: prompt })
    session.updatedAt = localNow()
    saveLocalSession(session)
    refreshLocal()
    setViewing({ ...session })

    localJob.value = {
      jobId: newLocalId('job'), sessionId: session.id, configId: cfg.id,
      status: 'running', turnsUsed: 0, maxTurns: cfg.maxTurns, usage: { ...session.usage },
    }
    const outcome = await runLocalAgent(
      {
        baseUrl: base, dialect, model: cfg.model, headers,
        system: localSystemPrompt(cfg) + recallBlockLocal(cfg.id, prompt),
        maxTurns: cfg.maxTurns,
        permission: cfg.permission, autoApprove: cfg.autoApprove, agentKind: cfg.agentKind,
        configId: cfg.id,
        fallbacks: fallbackRoutes,
        onRemember: (tool: string) => {
          const updated: AgentConfig = {
            ...cfg, permission: { default: cfg.permission.default, rules: { ...cfg.permission.rules, [tool]: 'allow' } },
            updatedAt: localNow(),
          }
          saveLocalConfig(updated)
          refreshLocal()
          store.notifySuccess(`Agent rule written: rules["${tool}"] = allow`)
        },
      },
      session.messages, session.usage,
      () => {
        session!.updatedAt = localNow()
        saveLocalSession(session!)
        if (localJob.value) {
          localJob.value = { ...localJob.value, status: 'running', usage: { ...session!.usage }, activity: agent.localActivity.value || null }
        }
        setViewing({ ...session! })
      },
    )
    const finished: AgentJob = {
      ...(localJob.value ?? { jobId: newLocalId('job'), sessionId: session.id, configId: cfg.id, maxTurns: cfg.maxTurns, usage: { ...session.usage } }),
      status: outcome.stopped === 'done' || outcome.stopped === 'limit' ? 'done' : outcome.stopped === 'aborted' ? 'cancelled' : 'error',
      usage: { ...session.usage },
      result: outcome.stopped === 'limit' ? 'turn budget exhausted — transcript saved' : undefined,
      error: outcome.error,
    }
    session.updatedAt = localNow()
    saveLocalSession(session)
    refreshLocal()
    setViewing({ ...session })
    localJob.value = finished
    return true
  }

  async function abort() {
    if (wasmMode.value) {
      abortLocalRun()
      if (localJob.value) localJob.value = { ...localJob.value, status: 'cancelled' }
      return
    }
    const job = activeJob.value
    if (job) await store.abortAgentJob(job.jobId)
  }

  async function answer(approved: boolean, remember = false) {
    if (wasmMode.value) {
      const pending = agent.pendingApproval.value
      agent.pendingApproval.value = null
      pending?.resolve(approved, approved ? answerInput.value || undefined : undefined, approved && remember)
      answerInput.value = ''
      return
    }
    const job = activeJob.value
    if (!job) return
    await store.approveAgentJob(job.jobId, approved, approved ? answerInput.value || undefined : undefined, remember)
    answerInput.value = ''
    if (remember && approved) void store.fetchAgentConfigs()
  }

  function sealLocalKey(configId: string, key: string) {
    localKeys.value[configId] = key
    setLocalKey(configId, key)
    refreshLocal()
  }

  function dropQueued(index: number) {
    if (index >= 0 && index < queue.value.length) queue.value.splice(index, 1)
  }

  /** Terminal status: pull the final transcript, then drain the prompt queue. */
  watch(
    () => activeJob.value?.status,
    status => {
      const terminal = status === 'done' || status === 'error' || status === 'cancelled'
      if (terminal && !wasmMode.value && viewing.value) {
        void store.loadAgentSession(viewing.value.id).then(s => {
          if (s && activeJob.value?.sessionId === viewing.value?.id) setViewing(s)
        })
        void store.fetchAgentSessions()
      }
      if (!terminal || jobActive.value || !queue.value.length) return
      const next = queue.value.shift()!
      void nextTick(() => {
        void send(next)
      })
    },
  )

  async function refreshLiveThread() {
    if (wasmMode.value) return
    const job = activeJob.value
    const v = viewing.value
    if (!job || !v || job.sessionId !== v.id) return
    const fresh = await store.loadAgentSession(v.id)
    if (fresh && activeJob.value?.sessionId === v.id && viewing.value?.id === v.id) setViewing(fresh)
  }

  let liveTimer = 0
  watch(jobActive, active => {
    if (!startedAt.value && active) startedAt.value = Date.now()
    if (active) {
      if (!liveTimer) liveTimer = window.setInterval(() => { if (!document.hidden) void refreshLiveThread() }, 1500)
      return
    }
    if (liveTimer) {
      window.clearInterval(liveTimer)
      liveTimer = 0
    }
    startedAt.value = 0
    void refreshLiveThread()
  })

  onBeforeUnmount(() => {
    if (liveTimer) window.clearInterval(liveTimer)
  })

  return {
    wasmMode, configs, sessions, chatConfigId, chatConfig, viewing,
    activeJob, jobActive, jobLine, jobError, jobHint,
    pendingApproval, answerInput, queue,
    threadRows, contextTokens, contextWindow, contextPct, costUsd,
    localPresets, localKeys,
    send, abort, answer, newSession, loadSession, removeSession,
    refreshLocal, sealLocalKey, ensureCatalog, setViewing, dropQueued,
    store, agent,
  }
}

export function useStudioAgent() {
  if (!shared) shared = createDriver()
  return shared
}

export type StudioAgent = ReturnType<typeof useStudioAgent>
