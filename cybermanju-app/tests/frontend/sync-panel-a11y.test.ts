// CyberManju OS — Sync panel accessibility (tests/frontend/sync-panel-a11y.test.ts).
// Role: regression tests for accessible names of sync configuration selectors.
// Exports: none.
// Integrates: Vitest, Node fs, src/components/SyncPanel.vue.
// Contracts: run and move selectors identify their configuration purpose.
import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

const syncPanel = readFileSync('src/components/SyncPanel.vue', 'utf8')

describe('Sync panel accessibility', () => {
  it('names the configuration used to start a sync run', () => {
    expect(syncPanel).toMatch(/<UiSelect\s+aria-label="Sync configuration to run"[\s\S]*?:model-value="runConfigId"/)
  })

  it('names both source and destination configuration selectors', () => {
    expect(syncPanel).toMatch(/<UiSelect\s+aria-label="Move source configuration"[\s\S]*?:model-value="moveFromId"/)
    expect(syncPanel).toMatch(/<UiSelect\s+aria-label="Move destination configuration"[\s\S]*?:model-value="moveToId"/)
  })
})
