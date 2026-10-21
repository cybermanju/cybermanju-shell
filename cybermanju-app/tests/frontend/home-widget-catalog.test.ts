// CyberManju OS — Home widget catalog contracts.
// Role: validate readiness, panel references and accessible alternatives.
import { describe, expect, it } from 'vitest'
import { homeWidgetCatalog } from '../../src/data/homeWidgetCatalog'
import fs from 'node:fs'
describe('Home widget library', () => {
  it('has unique identifiers and only the six implemented local renderers', () => {
    expect(new Set(homeWidgetCatalog.map(w => w.id)).size).toBe(homeWidgetCatalog.length)
    expect(homeWidgetCatalog.filter(w => w.kind === 'local').map(w => w.id).sort()).toEqual(['checklist','clock','counter','focus','notes','pixel-pet'])
  })
  it('uses real panel IDs for app cards and keeps native/concept capabilities explicit', () => {
    const panels = ['files','search','agent','storage','accounts','terminal','editor','sync','map','collections','settings','trash']
    for (const widget of homeWidgetCatalog.filter(w => w.kind === 'app-link')) expect(panels).toContain(widget.appId)
    expect(homeWidgetCatalog.some(w => w.kind === 'native')).toBe(true)
    expect(homeWidgetCatalog.some(w => w.kind === 'concept')).toBe(true)
    const ui = fs.readFileSync('src/components/HomeWorkspaceStudio.vue', 'utf8')
    expect(ui).toContain('Android host required')
    expect(ui).toContain('Concept · not connected')
    expect(ui).toContain(':disabled="!widgetReady(widget)')
    expect(ui).not.toMatch(/invoke\(/)
  })
  it('keeps accessible alternatives and reduced-motion support', () => {
    const ui = fs.readFileSync('src/components/HomeWorkspaceStudio.vue', 'utf8')
    for (const marker of ['aria-modal="true"','dialogKeys','Previous home screen','Next home screen','prefers-reduced-motion','Eagle view of home workspaces']) expect(ui).toContain(marker)
  })
})
