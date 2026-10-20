// CyberManju OS — Agent wasm parity.test (tests/frontend/agent-wasm-parity.test.ts).
// Browser parity: bash-subset + depth-1 subagents + HTTP MCP helpers.
import { describe, expect, it } from 'vitest'
import {
  BROWSER_SUBAGENT_MAX_TURNS,
  BROWSER_TASK_MAX_DEPTH,
  MAX_BG_SUBAGENTS,
  bgBusyMessage,
  bgFinishedMessage,
  bgStartedMessage,
  decideLocalTool,
  listLocalMcpTools,
  localMcpServers,
  splitMcpToolName,
  splitVolumeListing,
  sweepBgTasks,
  EMPTY_DIR_NOTE,
  type BgEntry,
} from '../../src/composables/useAgent'
import { stripDeniedTools, toolMeta } from '../../src/utils/agentUi'
import type { PermissionRuleset } from '../../src/types'

describe('splitMcpToolName', () => {
  it('splits namespaced tools', () => {
    expect(splitMcpToolName('mcp__exa__web_search_exa')).toEqual(['exa', 'web_search_exa'])
  })
  it('rejects malformed names', () => {
    expect(splitMcpToolName('read')).toBeNull()
    expect(splitMcpToolName('mcp__only')).toBeNull()
    expect(splitMcpToolName('mcp__a__b__c')).toBeNull()
    expect(splitMcpToolName('mcp____tool')).toBeNull()
  })
})

describe('splitVolumeListing', () => {
  it('drops the empty-directory sentence so glob never matches it as a file', () => {
    expect(splitVolumeListing(EMPTY_DIR_NOTE)).toEqual([])
    expect(splitVolumeListing(`${EMPTY_DIR_NOTE}\nnotes.txt\nlogs/`)).toEqual(['notes.txt', 'logs/'])
    expect(splitVolumeListing('a.txt\n\nb.txt')).toEqual(['a.txt', 'b.txt'])
  })
})

describe('browser task budget', () => {
  it('matches native depth-1 / 5-turn parity', () => {
    expect(BROWSER_TASK_MAX_DEPTH).toBe(1)
    expect(BROWSER_SUBAGENT_MAX_TURNS).toBe(5)
  })
})

describe('browser background subagents', () => {
  it('caps concurrent workers at native parity (8) with an honest busy refusal', () => {
    expect(MAX_BG_SUBAGENTS).toBe(8)
    expect(bgBusyMessage(8)).toBe(
      'busy: 8 background subagents already running (cap 8) — wait for results to arrive, or use a foreground task',
    )
  })
  it('spawn/arrival messages match the native shape so the model learns one shape', () => {
    const started = bgStartedMessage('bg-3', 'audit auth')
    expect(started).toContain('bg-3')
    expect(started).toContain('audit auth')
    expect(started).toContain('keep working')
    expect(bgFinishedMessage('bg-3', 'all clean')).toBe('background subagent bg-3 finished:\nall clean')
  })
  it('sweep harvests only settled entries in spawn order (parent keeps working meanwhile)', () => {
    const pending: BgEntry[] = [
      { id: 'bg-1', goal: 'a', promise: Promise.resolve('x'), result: null, settled: false },
      { id: 'bg-2', goal: 'b', promise: Promise.resolve('y'), result: 'done-b', settled: true },
      { id: 'bg-3', goal: 'c', promise: Promise.resolve('z'), result: 'done-c', settled: true },
    ]
    expect(sweepBgTasks(pending)).toEqual([
      { id: 'bg-2', result: 'done-b' },
      { id: 'bg-3', result: 'done-c' },
    ])
    // Settled slots are removed (reusable) — the running entry stays.
    expect(pending.map(p => p.id)).toEqual(['bg-1'])
    expect(sweepBgTasks(pending)).toEqual([])
  })
})

describe('listLocalMcpTools', () => {
  it('returns empty when no config is stored', async () => {
    try {
      localStorage.clear()
    } catch {
      // Node env has no localStorage — the helper already fails closed.
    }
    await expect(listLocalMcpTools('missing')).resolves.toEqual([])
  })
  it('fails closed without throwing (node has no localStorage)', () => {
    expect(localMcpServers('anything')).toEqual({})
  })
})

describe('browser tool wiring', () => {
  const open: PermissionRuleset = { default: 'allow', rules: {} }
  it('plan stays read-only for write/edit/bash/skill_save/mcp_attach/os_exec', () => {
    expect(decideLocalTool(open, 'plan', 'bash', { command: 'ls' }).kind).toBe('deny')
    expect(decideLocalTool(open, 'plan', 'write', { path: 'a' }).kind).toBe('deny')
    expect(decideLocalTool(open, 'plan', 'skill_save', { name: 's' }).kind).toBe('deny')
    expect(decideLocalTool(open, 'plan', 'mcp_attach', { name: 's' }).kind).toBe('deny')
    expect(decideLocalTool(open, 'plan', 'os_exec', { command: 'ls /' }).kind).toBe('deny')
    expect(decideLocalTool(open, 'plan', 'secret_list', {}).kind).toBe('deny')
    expect(decideLocalTool(open, 'plan', 'secret_get', { id: 's1' }).kind).toBe('deny')
    expect(decideLocalTool(open, 'plan', 'read', { path: 'a' }).kind).toBe('allow')
    expect(decideLocalTool(open, 'plan', 'self_research', { query: 'q' }).kind).toBe('allow')
    expect(decideLocalTool(open, 'plan', 'repo_analyze', { repo: 'o/r' }).kind).toBe('allow')
    // UI tools are read-only for the plan persona — they open a panel or
    // show a toast, never mutate the volume.
    expect(decideLocalTool(open, 'plan', 'ui_open_panel', { panel: 'files' }).kind).toBe('allow')
    expect(decideLocalTool(open, 'plan', 'ui_notify', { level: 'info', message: 'x' }).kind).toBe('allow')
  })
  it('stripDeniedTools drops plan mutations (D7 parity)', () => {
    const tools = ['read', 'write', 'edit', 'bash', 'skill_save', 'mcp_attach', 'self_research', 'repo_analyze', 'task', 'memory_recall', 'os_exec', 'ui_open_panel', 'ui_notify', 'secret_list', 'secret_get']
    expect(stripDeniedTools(tools, open, 'plan')).toEqual(['read', 'self_research', 'repo_analyze', 'task', 'memory_recall', 'ui_open_panel', 'ui_notify'])
  })
  it('toolMeta covers every browser tool incl. mcp__', () => {
    for (const t of ['read', 'write', 'edit', 'list', 'grep', 'glob', 'bash', 'task', 'question', 'memory_recall', 'memory_remember', 'self_research', 'skill_save', 'mcp_attach', 'repo_analyze', 'os_exec', 'ui_open_panel', 'ui_notify', 'secret_list', 'secret_get']) {
      expect(toolMeta(t).name).toBe(t)
    }
    expect(toolMeta('mcp__exa__web_search_exa').verb).toBe('MCP')
  })
})
