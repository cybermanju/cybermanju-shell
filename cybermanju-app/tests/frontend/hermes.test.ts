// CyberManju OS — Hermes.test (tests/frontend/hermes.test.ts).
// Hermes insights — TS-side recommendations, pinned by tests.
// Covers the five pillars in docs/HERMES-INSIGHTS.md: skills, memory,
// permissions (strip/explain), sessions (fork/export/import), gateway.
import { describe, expect, it } from 'vitest'
import {
  assembleStandingOrders,
  parseSkillFrontmatter,
  selectSkills,
  SKILL_FILE_BUDGET,
  SKILL_TOTAL_BUDGET,
  type SkillRecord,
} from '../../src/utils/skills'
import {
  AUTO_COMPACT_PCT,
  exportSession,
  forkSession,
  importSession,
  sessionTitleFor,
  shouldAutoCompact,
} from '../../src/utils/sessionMemory'
import {
  backoffMs,
  classifyGatewayError,
  resolveTransport,
  shouldRetry,
} from '../../src/utils/gateway'
import { explainDecision, stripDeniedTools } from '../../src/utils/agentUi'
import type { AgentSession, PermissionRuleset } from '../../src/types'

describe('skills: frontmatter', () => {
  it('parses name/description/version/tools and returns the body', () => {
    const raw = '---\nname: review\ndescription: code review helper\nversion: 1.2.0\ntools: [read, grep]\n---\n# Review\nDo the thing.'
    const { manifest, body } = parseSkillFrontmatter(raw)
    expect(manifest).toMatchObject({ name: 'review', description: 'code review helper', version: '1.2.0' })
    expect(manifest?.tools).toEqual(['read', 'grep'])
    expect(body).toBe('# Review\nDo the thing.')
  })

  it('accepts csv tools and is case-insensitive on keys', () => {
    const { manifest } = parseSkillFrontmatter('---\nName: x\nDescription: y\nTools: read, grep\n---\nbody')
    expect(manifest?.tools).toEqual(['read', 'grep'])
  })

  it('returns null manifest when there is no frontmatter, never throws', () => {
    const { manifest, body } = parseSkillFrontmatter('# plain skill, no manifest')
    expect(manifest).toBeNull()
    expect(body).toBe('# plain skill, no manifest')
    expect(parseSkillFrontmatter('').manifest).toBeNull()
  })
})

describe('skills: standing-orders assembly', () => {
  it('caps each file at 8 KiB and marks the cut truncated:', () => {
    const big = 'x'.repeat(SKILL_FILE_BUDGET + 100)
    const res = assembleStandingOrders([{ path: 'AGENTS.md', content: big }])
    expect(res.included).toHaveLength(1)
    expect(res.included[0].truncated).toBe(true)
    expect(res.included[0].chars).toBe(SKILL_FILE_BUDGET)
    expect(res.block).toContain('truncated:')
    expect(res.omitted).toEqual([])
  })

  it('caps the total at 24 KiB and omits the overflow', () => {
    const files = [1, 2, 3, 4].map(i => ({ path: `S${i}.md`, content: 'y'.repeat(SKILL_FILE_BUDGET) }))
    const res = assembleStandingOrders(files)
    // 3 × 8 KiB = 24 KiB fits; the fourth has no room left.
    expect(res.included).toHaveLength(3)
    expect(res.omitted).toEqual(['S4.md'])
    expect(res.block).toContain('<standing-orders>')
    const used = res.included.reduce((n, e) => n + e.chars, 0)
    expect(used).toBeLessThanOrEqual(SKILL_TOTAL_BUDGET)
  })

  it('passes small files through byte-exact with no marker', () => {
    const res = assembleStandingOrders([{ path: 'SKILL.md', content: '# hi\n' }])
    expect(res.included[0].truncated).toBe(false)
    expect(res.block).toContain('# hi')
    expect(res.block).not.toContain('truncated:')
  })
})

describe('skills: selection', () => {
  const skill = (id: string, name: string, description: string): SkillRecord => ({
    id,
    manifest: { name, description, tools: [] },
    body: '',
  })

  it('ranks by goal overlap and breaks ties by id', () => {
    const skills = [
      skill('b', 'deploy helper', 'ships containers'),
      skill('a', 'review helper', 'reviews code'),
      skill('c', 'review bot', 'reviews code diffs'),
    ]
    const picked = selectSkills(skills, 'please review this code', 2)
    expect(picked.map(s => s.id)).toEqual(['a', 'c'])
  })

  it('returns [] when nothing overlaps and when the goal is empty', () => {
    const skills = [skill('a', 'review helper', 'reviews code')]
    expect(selectSkills(skills, 'bake sourdough')).toEqual([])
    expect(selectSkills(skills, '')).toEqual([])
  })
})

const baseSession = (): AgentSession => ({
  id: 'ses-1',
  title: 'Original',
  configId: 'cfg',
  providerId: 'prov',
  model: 'test-model',
  agentKind: 'build',
  workingDir: '/',
  messages: [
    { role: 'user', content: 'one' },
    { role: 'assistant', content: 'two' },
    { role: 'user', content: 'three' },
  ],
  usage: { inputTokens: 10, outputTokens: 20 },
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
})

describe('memory: compact trigger + titles', () => {
  it('fires at 85 % and is garbage-safe', () => {
    expect(shouldAutoCompact(85)).toBe(true)
    expect(shouldAutoCompact(84.9)).toBe(false)
    expect(shouldAutoCompact(100)).toBe(true)
    expect(shouldAutoCompact(Number.NaN)).toBe(false)
    expect(shouldAutoCompact(90, 95)).toBe(false)
    expect(AUTO_COMPACT_PCT).toBe(85)
  })

  it('titles from the first eight words', () => {
    expect(sessionTitleFor('  look at src/app.ts and tell me  what it does today please ok more')).toBe(
      'look at src/app.ts and tell me what it',
    )
    expect(sessionTitleFor('')).toBe('Untitled session')
  })
})

describe('sessions: fork / export / import', () => {
  it('forks with a new id, sliced transcript, and no source mutation', () => {
    const src = baseSession()
    const fork = forkSession(src, { idFactory: () => 'ses-2', keepLast: 2 })
    expect(fork.id).toBe('ses-2')
    expect(fork.messages.map(m => m.content)).toEqual(['two', 'three'])
    expect(fork.title).toBe('Original (fork)')
    expect(src.messages).toHaveLength(3)
    expect(src.id).toBe('ses-1')
  })

  it('resets usage on request and keeps it otherwise', () => {
    expect(forkSession(baseSession(), { resetUsage: true }).usage).toEqual({ inputTokens: 0, outputTokens: 0 })
    expect(forkSession(baseSession()).usage).toEqual({ inputTokens: 10, outputTokens: 20 })
  })

  it('round-trips through the versioned envelope', () => {
    const rt = importSession(exportSession(baseSession()))
    expect(rt.ok).toBe(true)
    if (rt.ok) {
      expect(rt.session.id).toBe('ses-1')
      expect(rt.session.messages).toHaveLength(3)
    }
  })

  it('rejects corrupt payloads with invalid:, never throws', () => {
    expect(importSession('not json').ok).toBe(false)
    expect(importSession(JSON.stringify({ version: 999, session: {} })).ok).toBe(false)
    expect(importSession(JSON.stringify({ version: 1, session: { id: '', messages: [] } })).ok).toBe(false)
    const bad = importSession(JSON.stringify({ version: 1, session: { id: 'x', messages: [{ role: 'user' }] } }))
    expect(bad.ok).toBe(false)
    if (!bad.ok) expect(bad.error.startsWith('invalid:')).toBe(true)
  })
})

describe('permissions: strip + explain (Hermes D7 first step)', () => {
  const rules: PermissionRuleset = { default: 'ask', rules: { read: 'allow', bash: 'deny' } }

  it('hides denied tools from the advertised schema', () => {
    expect(stripDeniedTools(['read', 'write', 'bash'], rules, 'build')).toEqual(['read', 'write'])
  })

  it('drops mutations for the plan persona automatically', () => {
    expect(stripDeniedTools(['read', 'edit', 'write', 'bash'], { default: 'allow', rules: {} }, 'plan')).toEqual([
      'read',
    ])
  })

  it('explains each verdict in one human line', () => {
    expect(explainDecision(rules, 'build', 'read', { path: '/a.ts' })).toMatch(/^ALLOW read \/a\.ts/)
    expect(explainDecision({ default: 'ask', rules: {} }, 'build', 'bash', { command: 'ls' })).toMatch(/^ASK bash/)
    expect(explainDecision({ default: 'ask', rules: {} }, 'build', 'bash', { command: 'ls' })).toMatch(
      /needs approval|Approve/,
    )
    expect(explainDecision(rules, 'build', 'bash', { command: 'ls' })).toMatch(/^DENY bash/)
    // Standing-orders write under an allow rule downgrades to ASK, never silent ALLOW.
    expect(explainDecision({ default: 'allow', rules: { edit: 'allow' } }, 'build', 'edit', { path: 'AGENTS.md' })).toMatch(
      /^ASK edit.*protected standing orders/,
    )
  })
})

describe('gateway: transport / backoff / classification', () => {
  it('mirrors the useTauri routing matrix', () => {
    expect(resolveTransport({ isTauri: true, serverUrl: '', port: '' })).toBe('tauri')
    expect(resolveTransport({ isTauri: false, serverUrl: 'http://nas:3456', port: '' })).toBe('rest')
    expect(resolveTransport({ isTauri: false, serverUrl: '', port: '3456' })).toBe('rest')
    expect(resolveTransport({ isTauri: false, serverUrl: '', port: '4173' })).toBe('wasm')
    expect(resolveTransport({ isTauri: false, serverUrl: '', port: '' })).toBe('wasm')
  })

  it('backs off exponentially under a cap', () => {
    expect(backoffMs(0)).toBe(300)
    expect(backoffMs(1)).toBe(600)
    expect(backoffMs(2)).toBe(1200)
    expect(backoffMs(99)).toBe(5000)
    expect(backoffMs(-3)).toBe(300)
  })

  it('classifies failures like the native provider classifier', () => {
    expect(classifyGatewayError('401 Unauthorized')).toBe('auth')
    expect(classifyGatewayError('auth: provider rejected the key')).toBe('auth')
    expect(classifyGatewayError('429 rate_limited: slow down')).toBe('rate_limited')
    expect(classifyGatewayError('context_length_exceeded: too long')).toBe('context')
    expect(classifyGatewayError('context: prompt is too long')).toBe('context')
    expect(classifyGatewayError('Network error calling GET /api/files: fetch failed')).toBe('network')
    expect(classifyGatewayError('something nobody classified')).toBe('unknown')
  })

  it('retries transient failures only — never auth or context', () => {
    expect(shouldRetry('network')).toBe(true)
    expect(shouldRetry('rate_limited')).toBe(true)
    expect(shouldRetry('auth')).toBe(false)
    expect(shouldRetry('context')).toBe(false)
    expect(shouldRetry('unknown')).toBe(false)
  })
})
