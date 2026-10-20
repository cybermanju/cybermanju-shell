// CyberManju OS — Agent ui.test (tests/frontend/agent-ui.test.ts).
// Agent thread + awareness helpers. These are the pieces the panel renders
// from, so they are tested as a unit: if they drift, every transport shows a
// different thread for the same transcript.
import { describe, expect, it } from 'vitest'
import {
  applyBalancedToConfig,
  applyYoloToConfig,
  buildThread,
  contextWindowFor,
  estimateCost,
  estimateTranscriptTokens,
  isYoloConfig,
  isYoloRuleset,
  permissionLabel,
  salientArg,
  stripDeniedTools,
  toolPermissions,
  toolStateFromResult,
  toolTitle,
  type ToolGroupRow,
} from '../../src/utils/agentUi'
import { renderMarkdown } from '../../src/utils/markdown'
import {
  anchorTrailerHex,
  decideLocalTool,
  isFailoverWorthy,
  normalizeAnchorLocal,
  rememberAllowLocal,
  stripAnchorLocal,
  stripEcho,
} from '../../src/composables/useAgent'
import { agentErrorHint, agentPermissionPreset, defaultMcpServers, type AgentConfig, type ChatMessage, type PermissionRuleset } from '../../src/types'

const readCall = (id: string, path: string): ChatMessage => ({
  role: 'assistant_tool',
  content: 'reading the file first',
  toolInput: [{ id, name: 'read', input: { path } }],
})

const toolResult = (id: string, name: string, content: string): ChatMessage => ({
  role: 'tool',
  content,
  toolCallId: id,
  toolName: name,
})

describe('buildThread', () => {
  it('pairs a call with its result and keeps the lead text', () => {
    const rows = buildThread(
      [
        { role: 'user', content: 'look at src/app.ts' },
        readCall('c1', '/src/app.ts'),
        toolResult('c1', 'read', 'export const a = 1\n'),
        { role: 'assistant', content: 'It exports `a`.' },
      ],
      false,
    )
    expect(rows.map(r => r.kind)).toEqual(['message', 'tools', 'message'])
    const group = rows[1] as ToolGroupRow
    expect(group.lead).toBe('reading the file first')
    expect(group.rows).toHaveLength(1)
    expect(group.rows[0].state).toBe('done')
    expect(group.rows[0].title).toContain('src/app.ts')
    expect(group.rows[0].result).toContain('export const a')
  })

  it('marks an unanswered call running while a run is live, error when it is not', () => {
    const transcript: ChatMessage[] = [readCall('c1', '/a.ts')]
    expect((buildThread(transcript, true)[0] as ToolGroupRow).rows[0].state).toBe('running')
    expect((buildThread(transcript, false)[0] as ToolGroupRow).rows[0].state).toBe('error')
  })

  it('keeps a denial visible as denied rather than generic failure', () => {
    const rows = buildThread(
      [readCall('c1', '/secret'), toolResult('c1', 'read', 'deny: `read` is denied by the ruleset')],
      false,
    )
    expect((rows[0] as ToolGroupRow).rows[0].state).toBe('denied')
  })

  it('surfaces a result that arrived without its assistant block', () => {
    const rows = buildThread([toolResult('c9', 'grep', 'src/app.ts:12: export const a')], false)
    expect(rows).toHaveLength(1)
    const group = rows[0] as ToolGroupRow
    expect(group.rows[0].state).toBe('done')
    expect(group.rows[0].result).toContain('app.ts:12')
  })
})

describe('toolStateFromResult', () => {
  it('classifies by the machine prefix, not by wording', () => {
    expect(toolStateFromResult('ok')).toBe('done')
    expect(toolStateFromResult('  denied: user rejected')).toBe('denied')
    expect(toolStateFromResult('deny: bash is denied')).toBe('denied')
    expect(toolStateFromResult('not_found: old_block absent')).toBe('error')
    expect(toolStateFromResult('context: prompt is too long')).toBe('error')
    expect(toolStateFromResult('truncated: output capped')).toBe('error')
    expect(toolStateFromResult(null)).toBe('running')
  })
})

describe('titles and salient arguments', () => {
  it('leads with the target a human cares about', () => {
    expect(salientArg({ path: '/src/app.ts' })).toBe('/src/app.ts')
    expect(salientArg({ repo: 'owner/repo' })).toBe('owner/repo')
    expect(salientArg({ name: 'my-skill' })).toBe('my-skill')
    expect(toolTitle('read', { path: '/src/app.ts' })).toBe('Reading /src/app.ts')
    expect(toolTitle('grep', { pattern: 'TODO', path: '/src' })).toContain('TODO')
    expect(toolTitle('question', { question: 'Which DB?' })).toContain('Which DB?')
    expect(toolTitle('self_research', { query: 'how MCP works' })).toContain('how MCP works')
    expect(toolTitle('repo_analyze', { repo: 'owner/repo' })).toContain('owner/repo')
  })
})

describe('context and cost awareness', () => {
  it('estimates the transcript at ~4 chars per token', () => {
    const msgs: ChatMessage[] = [{ role: 'user', content: 'x'.repeat(400) }]
    expect(estimateTranscriptTokens(msgs)).toBe(100)
  })

  it('knows the window belongs to the model family', () => {
    expect(contextWindowFor('claude-sonnet-4-5')).toBe(200_000)
    expect(contextWindowFor('gpt-5')).toBe(200_000)
    expect(contextWindowFor('gemini-2.5-pro')).toBe(1_048_576)
    expect(contextWindowFor('totally-unknown-model')).toBe(128_000)
  })

  it('prices only what it recognises and says so for the rest', () => {
    const usage = { inputTokens: 1_000_000, outputTokens: 1_000_000 }
    expect(estimateCost('claude-sonnet-4-5', usage)).toBeCloseTo(18, 5)
    expect(estimateCost('some-local-model', usage)).toBeNull()
  })
})

describe('capability surface', () => {
  const rules: PermissionRuleset = { default: 'ask', rules: { read: 'allow', bash: 'deny' } }

  it('reports the same decision the loop will apply', () => {
    const perms = toolPermissions(rules, 'build', ['read', 'write', 'bash'])
    expect(perms).toEqual([
      { tool: 'read', action: 'allow' },
      { tool: 'write', action: 'ask' },
      { tool: 'bash', action: 'deny' },
    ])
    expect(decideLocalTool(rules, 'build', 'write', {}).kind).toBe('ask')
  })

  it('shows a plan agent as read-only without reading the ruleset', () => {
    const perms = toolPermissions(rules, 'plan', ['edit', 'write', 'bash'])
    expect(perms.every(p => p.action === 'deny')).toBe(true)
  })

  it('labels the ruleset in one word', () => {
    expect(permissionLabel({ default: 'ask', rules: {} })).toBe('STRICT')
    expect(permissionLabel({ default: 'allow', rules: {} })).toBe('YOLO')
    expect(permissionLabel({ default: 'allow', rules: { read: 'allow' } })).toBe('OPEN')
    expect(permissionLabel(rules)).toBe('BALANCED')
    expect(permissionLabel(undefined)).toBe('DEFAULT')
  })

  it('writes "allow always" as a visible rule, never an always-list', () => {
    const target: PermissionRuleset = { default: 'ask', rules: {} }
    rememberAllowLocal(target, 'bash')
    expect(target.rules.bash).toBe('allow')
    expect(decideLocalTool(target, 'build', 'bash', { line: 'ls' }).kind).toBe('allow')
  })
})

describe('YOLO mode', () => {
  const yoloCfg = (over: Partial<AgentConfig> = {}): AgentConfig => ({
    id: 'cfg-1',
    name: 'yolo',
    providerId: 'openrouter',
    model: 'm',
    workingDir: '',
    agentKind: 'build',
    permission: agentPermissionPreset('yolo'),
    autoApprove: true,
    maxTurns: 25,
    hasKey: true,
    mcpServers: {
      fs: { transport: 'stdio', args: [], env: {}, headers: [], enabled: false },
    },
    createdAt: '',
    updatedAt: '',
    ...over,
  })

  it('the yolo preset allows everything with no rules to trip on', () => {
    expect(agentPermissionPreset('yolo')).toEqual({ default: 'allow', rules: {} })
    for (const tool of ['read', 'write', 'edit', 'bash', 'task', 'mcp__fs__read']) {
      expect(decideLocalTool(agentPermissionPreset('yolo'), 'build', tool, {}).kind).toBe('allow')
    }
    expect(stripDeniedTools(['read', 'write', 'edit', 'bash', 'task'], agentPermissionPreset('yolo'), 'build')).toEqual([
      'read',
      'write',
      'edit',
      'bash',
      'task',
    ])
  })

  it('detects YOLO rulesets and rejects anything with an ask or deny', () => {
    expect(isYoloRuleset(agentPermissionPreset('yolo'))).toBe(true)
    expect(isYoloRuleset(agentPermissionPreset('balanced'))).toBe(false)
    expect(isYoloRuleset(agentPermissionPreset('strict'))).toBe(false)
    expect(isYoloRuleset({ default: 'allow', rules: { bash: 'ask' } })).toBe(false)
    expect(isYoloRuleset({ default: 'allow', rules: { bash: [['*', 'allow'], ['rm *', 'deny']] } })).toBe(false)
    expect(isYoloRuleset(undefined)).toBe(false)
  })

  it('a YOLO config needs build kind plus auto-approve, not just the ruleset', () => {
    expect(isYoloConfig(yoloCfg())).toBe(true)
    expect(isYoloConfig(yoloCfg({ autoApprove: false }))).toBe(false)
    expect(isYoloConfig(yoloCfg({ agentKind: 'plan' }))).toBe(false)
    expect(isYoloConfig(null)).toBe(false)
  })

  it('enabling YOLO flips kind, ruleset, auto-approve, and every MCP server', () => {
    const out = applyYoloToConfig(yoloCfg({ agentKind: 'plan', autoApprove: false, permission: agentPermissionPreset('balanced') }))
    expect(out.agentKind).toBe('build')
    expect(out.autoApprove).toBe(true)
    expect(out.permission).toEqual({ default: 'allow', rules: {} })
    expect(out.mcpServers?.fs?.enabled).toBe(true)
    expect(isYoloConfig(out)).toBe(true)
  })

  it('disabling YOLO returns to balanced ask-by-default', () => {
    const out = applyBalancedToConfig(yoloCfg())
    expect(out.autoApprove).toBe(false)
    expect(out.permission).toEqual(agentPermissionPreset('balanced'))
    expect(isYoloConfig(out)).toBe(false)
  })
})

describe('default web search', () => {
  it('ships the keyless Exa MCP server', () => {
    const servers = defaultMcpServers()
    expect(servers.exa.transport).toBe('http')
    expect(servers.exa.url).toBe('https://mcp.exa.ai/mcp')
    expect(servers.exa.enabled).toBe(true)
  })

  it('balanced preset allows curl/wget and the Exa tools, still asks otherwise', () => {
    const preset = agentPermissionPreset('balanced')
    expect(decideLocalTool(preset, 'build', 'bash', { command: 'curl -sS https://example.com' }).kind).toBe('allow')
    expect(decideLocalTool(preset, 'build', 'bash', { command: 'wget -qO- https://example.com' }).kind).toBe('allow')
    expect(decideLocalTool(preset, 'build', 'mcp__exa__web_search_exa', { query: 'news' }).kind).toBe('allow')
    expect(decideLocalTool(preset, 'build', 'mcp__exa__web_fetch_exa', {}).kind).toBe('allow')
    expect(decideLocalTool(preset, 'build', 'bash', { command: 'rm -rf /tmp/x' }).kind).toBe('deny')
    expect(decideLocalTool(preset, 'build', 'bash', { command: 'ls /' }).kind).toBe('ask')
  })

  it('shell mode travels with the config (absent = backend auto)', () => {
    const cfg: AgentConfig = {
      id: 'cfg-shell',
      name: 'shell',
      providerId: 'openrouter',
      model: 'm',
      workingDir: '',
      agentKind: 'build',
      shellMode: 'cybsh',
      permission: agentPermissionPreset('balanced'),
      autoApprove: false,
      maxTurns: 25,
      hasKey: true,
      createdAt: '',
      updatedAt: '',
    }
    expect(applyYoloToConfig(cfg).shellMode).toBe('cybsh')
    expect(applyBalancedToConfig(cfg).shellMode).toBe('cybsh')
    // Old configs without the field stay valid — the backend defaults to auto.
    const legacy = { ...cfg } as Record<string, unknown>
    delete legacy.shellMode
    expect('shellMode' in legacy).toBe(false)
  })
})

describe('renderMarkdown', () => {
  it('never lets raw markup through — the model writes untrusted text', () => {
    const html = renderMarkdown('hello <img src=x onerror="alert(1)"> and <script>bad()</script>')
    expect(html).not.toContain('<img')
    expect(html).not.toContain('<script>')
    expect(html).toContain('&lt;img')
  })

  it('keeps fenced code verbatim and escaped', () => {
    const html = renderMarkdown('```ts\nconst a: number = 1 < 2\n```')
    expect(html).toContain('class="language-ts"')
    expect(html).toContain('const a: number = 1 &lt; 2')
    expect(html).not.toContain('1 < 2')
  })

  it('renders links only for web-safe targets', () => {
    expect(renderMarkdown('[docs](https://example.com/a?b=1&c=2)')).toContain('href="https://example.com/a?b=1&amp;c=2"')
    expect(renderMarkdown('[x](javascript:alert(1))')).not.toContain('<a ')
    expect(renderMarkdown('[x](data:text/html,<script>)')).not.toContain('<a ')
  })

  it('renders structure a human can scan', () => {
    const html = renderMarkdown('# Title\n\n- one\n- two\n\n**bold** and `code`')
    expect(html).toContain('<h2>')
    expect(html).toContain('<li>')
    expect(html).toContain('<strong>bold</strong>')
    expect(html).toContain('<code>code</code>')
  })

  it('treats a trailing unclosed fence as code, not literal ticks', () => {
    const html = renderMarkdown('here:\n```ts\nconst a = 1 < 2\n')
    expect(html).toContain('<pre class="md-code">')
    expect(html).toContain('const a = 1 &lt; 2')
    expect(html).not.toContain('```')
  })
})

describe('agentErrorHint', () => {
  it('turns a machine prefix into the next human step', () => {
    expect(agentErrorHint('context: prompt is too long').prefix).toBe('context')
    expect(agentErrorHint('context: prompt is too long').hint).toMatch(/COMPACT/)
    expect(agentErrorHint('auth: 401').prefix).toBe('auth')
    expect(agentErrorHint('something nobody classified').hint).toMatch(/transcript/)
  })
})

describe('read anchor (browser mirror of Rust `edit::strip_anchor`)', () => {
  const BODY = 'fn a() {}\n'
  const HEX = 'a'.repeat(64)
  // Hermetic stand-in for the wasm BLAKE3: only BODY hashes to HEX, every
  // other byte string hashes to something else.
  const fakeHash = async (s: string) => (s === BODY ? HEX : 'b'.repeat(64))

  it('removes a trailing [blake3:<hex>] line only when it verifies', async () => {
    await expect(stripAnchorLocal(`${BODY}\n[blake3:${HEX}]`, fakeHash)).resolves.toBe(BODY)
    // Anchor-only content strips only when it is ours (hash of empty).
    const emptyHash = async (s: string) => (s === '' ? HEX : 'b'.repeat(64))
    await expect(stripAnchorLocal(`\n[blake3:${HEX}]`, emptyHash)).resolves.toBe('')
    await expect(stripAnchorLocal(`[blake3:${HEX}]\n`, emptyHash)).resolves.toBe('')
  })

  it('preserves legitimate anchor-shaped content lines', async () => {
    const foreign = 'c'.repeat(64)
    const legit = `${BODY}\n[blake3:${foreign}]`
    await expect(stripAnchorLocal(legit, fakeHash)).resolves.toBe(legit)
    // …and preserves everything when no hash is available to prove ownership.
    const ours = `${BODY}\n[blake3:${HEX}]`
    await expect(stripAnchorLocal(ours, async () => null)).resolves.toBe(ours)
  })

  it('leaves ordinary content and near-miss lines alone', async () => {
    await expect(stripAnchorLocal('fn a() {}\n', fakeHash)).resolves.toBe('fn a() {}\n')
    await expect(stripAnchorLocal('[blake3:short]', fakeHash)).resolves.toBe('[blake3:short]')
    await expect(stripAnchorLocal('a\n[blake3:not-hex]\n', fakeHash)).resolves.toBe('a\n[blake3:not-hex]\n')
    await expect(stripAnchorLocal(`inner [blake3:${HEX}]`, fakeHash)).resolves.toBe(`inner [blake3:${HEX}]`)
  })

  it('detects an anchor-shaped trailer without judging ownership', () => {
    expect(anchorTrailerHex(`${BODY}\n[blake3:${HEX}]`)).toBe(HEX)
    expect(anchorTrailerHex('fn a() {}\n')).toBeNull()
    expect(anchorTrailerHex('[blake3:short]')).toBeNull()
  })

  it('strips an edited echo against the previously stored bytes', async () => {
    const before = 'line one\nline two\n'
    const after = 'line one\nline CHANGED\n'
    const hashBefore = async (s: string) => (s === before ? HEX : 'b'.repeat(64))
    const staleEcho = `${after}\n[blake3:${HEX}]`
    // Body-hash alone does not verify (the content changed)…
    await expect(stripEcho(staleEcho, null, hashBefore)).resolves.toBe(staleEcho)
    // …but against the bytes the model actually read, it is ours.
    await expect(stripEcho(staleEcho, before, hashBefore)).resolves.toBe(after)
    // A trailer the model composed itself stays even with `existing`.
    const forged = `${after}[blake3:${'f'.repeat(64)}]`
    await expect(stripEcho(forged, before, hashBefore)).resolves.toBe(forged)
  })
})

describe('anchor normalization (browser mirror of Rust `edit::normalize_anchor`)', () => {
  const hex = 'a'.repeat(64)

  it('accepts every shape a model might echo back', () => {
    expect(normalizeAnchorLocal(hex)).toBe(hex)
    expect(normalizeAnchorLocal(`blake3:${hex}`)).toBe(hex)
    expect(normalizeAnchorLocal(`[blake3:${hex}]`)).toBe(hex)
    expect(normalizeAnchorLocal(`  ${hex}  `)).toBe(hex)
    expect(normalizeAnchorLocal(hex.slice(0, 16))).toBe(hex.slice(0, 16))
  })

  it('treats an absent or empty anchor as no anchor', () => {
    expect(normalizeAnchorLocal('')).toBe('')
    expect(normalizeAnchorLocal('   ')).toBe('')
    expect(normalizeAnchorLocal('[blake3:]')).toBe('')
  })
})

describe('failover worthiness (browser mirror of Rust `is_failover_worthy`)', () => {
  it('fails over on dead keys, throttles, transport failures and empty credits', () => {
    for (const err of [
      'auth: bad key',
      'rate_limited: slow down',
      'network: timeout',
      'limit: provider credits exhausted',
    ]) {
      expect(isFailoverWorthy(err)).toBe(true)
    }
  })

  it('stops on errors that would fail identically everywhere', () => {
    for (const err of [
      'invalid: bad request',
      'context: too large',
      'integrity: moved',
      'conflict: ambiguous',
      'unsupported: no shell',
      'too_large: 1 MiB cap',
      'cancelled',
      '',
    ]) {
      expect(isFailoverWorthy(err)).toBe(false)
    }
  })
})
