// CyberManju OS — Plasma Panel accessibility (tests/frontend/plasma-panel-a11y.test.ts).
// Role: regression test for screen-reader names on icon-only tray controls.
// Exports: none.
// Integrates: Vitest, Node fs, src/components/PlasmaPanel.vue.
// Contracts: every icon-only system tray action has an explicit accessible name.
import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

const panel = readFileSync('src/components/PlasmaPanel.vue', 'utf8')

describe('Plasma system tray accessibility', () => {
  it('gives every icon-only tray action an explicit screen-reader name', () => {
    const comment = panel.indexOf('<!-- System tray -->')
    expect(comment).toBeGreaterThanOrEqual(0)
    const trayStart = panel.indexOf('<div class="pp-tray"', comment)
    expect(trayStart).toBeGreaterThan(comment)
    const trayEnd = panel.indexOf('</div>', trayStart)
    expect(trayEnd).toBeGreaterThan(trayStart)

    const buttons = panel.slice(trayStart, trayEnd).match(/<button[\s\S]*?<\/button>/g) ?? []
    expect(buttons).toHaveLength(5)
    for (const button of buttons) {
      expect(button).toMatch(/\s(?:aria-label|:aria-label)=/)
    }
  })
})
