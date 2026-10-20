// CyberManju OS — Backup Panel accessibility (tests/frontend/backup-panel-a11y.test.ts).
// Role: regression test for accessible backup restore controls.
// Exports: none.
// Integrates: Vitest, Node fs, src/components/BackupPanel.vue.
// Contracts: the snapshot file picker has a descriptive accessible name.
import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

const panel = readFileSync('src/components/BackupPanel.vue', 'utf8')

describe('backup restore accessibility', () => {
  it('names the snapshot file picker for assistive technology', () => {
    expect(panel).toMatch(/<input type="file" aria-label="Choose backup snapshot file" accept="\.cybermanju" @change="pickFile" \/>/)
  })
})
