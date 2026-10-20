// CyberManju OS — Agent panel accessibility (tests/frontend/agent-panel-a11y.test.ts).
// Role: regression tests for accessible names of agent configuration controls.
// Exports: none.
// Integrates: Vitest, Node fs, src/components/AgentPanel.vue.
// Contracts: all UiSelect controls expose a purpose-specific accessible name.
import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

const agentPanel = readFileSync('src/components/AgentPanel.vue', 'utf8')
const selectTags = [...agentPanel.matchAll(/<UiSelect\b[\s\S]*?\/>/g)].map(([tag]) => tag)

const namedSelects: Array<[string, string]> = [
  ['chatConfigId', 'Assistant configuration'],
  ['form.agentKind', 'Agent mode'],
  ['form.dialectOverride', 'API dialect'],
  ['form.authSchemeOverride', 'Authentication scheme'],
  ['form.shellMode', 'Shell mode'],
  ['permPreset', 'Permission preset'],
  ['mcpForm.transport', 'MCP transport'],
  ['fallbackPick', 'Fallback assistant'],
]

describe('Agent panel accessibility', () => {
  it.each(namedSelects)('names the %s selector', (model, label) => {
    const tag = selectTags.find((candidate) => candidate.includes(`:model-value="${model}"`))
    expect(tag).toContain(`aria-label="${label}"`)
  })

  it('names each per-tool permission selector with its tool', () => {
    const tag = selectTags.find((candidate) => candidate.includes(':model-value="t.action"'))
    expect(tag).toContain(":aria-label=\"'Permission for ' + t.tool\"")
  })
})
