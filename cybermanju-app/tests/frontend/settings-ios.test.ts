// CyberManju OS — Settings ios.test (tests/frontend/settings-ios.test.ts).
// iOS-Settings contract for the phone sheet: large title, grouped lists
// with icon tiles, a filtering search rail, and 44px cell rhythm — scoped
// to the max-width:560px block so desktop windows keep their density.
import { describe, expect, it } from 'vitest'
import fs from 'node:fs'

const settings = fs.readFileSync('src/components/SettingsPage.vue', 'utf8')

function mobileBlock(): string {
  const start = settings.indexOf('@media (max-width: 560px)')
  expect(start, 'missing @media (max-width: 560px) block').toBeGreaterThanOrEqual(0)
  return settings.slice(start)
}

describe('iOS large title + SF voice', () => {
  it('uses an SF stack and a large title on phones', () => {
    const m = mobileBlock()
    expect(m).toContain('"SF Pro Text"')
    expect(m).toMatch(/\.st-title\s*\{[^}]*font-size:\s*26px/)
    expect(m).toMatch(/\.st-title\s*\{[^}]*font-weight:\s*800/)
  })
})

describe('iOS grouped lists', () => {
  it('tints the scroll area and keeps groups as solid inset-rounded cards', () => {
    const m = mobileBlock()
    expect(m).toMatch(/\.st-body\s*\{[\s\S]*?color-mix\(in srgb, var\(--ui-text\)/)
    expect(m).toMatch(/\.ui-card\)\s*\{[\s\S]*?background:\s*var\(--ui-surface\)/)
    expect(m).toMatch(/\.ui-card\)\s*\{[\s\S]*?border-radius:\s*14px/)
  })

  it('turns card icons into iOS-style tiles with stronger titles', () => {
    const m = mobileBlock()
    expect(m).toMatch(/\.ui-card__icon\)\s*\{[\s\S]*?width:\s*28px/)
    expect(m).toMatch(/\.ui-card__icon\)\s*\{[\s\S]*?border-radius:\s*8px/)
    expect(m).toMatch(/\.ui-card__title\)\s*\{[^}]*font-size:\s*15px/)
  })
})

describe('iOS search rail', () => {
  it('keeps chips on one snap rail with a 16px search field', () => {
    const m = mobileBlock()
    expect(m).toMatch(/\.st-jumps\s*\{[\s\S]*?flex-wrap:\s*nowrap/)
    expect(m).toMatch(/\.st-jumps\s*\{[\s\S]*?overflow-x:\s*auto/)
    expect(m).toMatch(/\.st-jump-search\s*\{[\s\S]*?font-size:\s*16px/)
    expect(m).toMatch(/\.st-jump\s*\{[\s\S]*?min-height:\s*40px/)
  })

  it('filters whole groups by label + row keywords, with a No-Results state', () => {
    expect(settings).toContain('SECTION_KEYWORDS')
    expect(settings).toContain('sectionVisible')
    for (const id of ['appearance', 'remote', 'broker', 'sync', 'gestures', 'keys', 'about']) {
      expect(settings, `missing v-show for ${id}`).toContain(`sectionVisible('${id}')`)
    }
    expect(settings).toContain('No results for')
    expect(settings).toContain(`@click="sectionQuery = ''"`)
  })

  it('keeps the scroll spy off search-hidden cards', () => {
    expect(settings).toContain('offsetParent === null')
  })
})

describe('iOS 44px cell rhythm', () => {
  it('keeps rows label-left/control-right at 44px with solid hairlines', () => {
    const m = mobileBlock()
    expect(m).toMatch(/\.st-row\s*\{[\s\S]*?min-height:\s*44px/)
    expect(m).toMatch(/\.st-row\s*\{[\s\S]*?flex-direction:\s*row/)
    expect(m).toMatch(/\.st-row\s*\+\s*\.st-row\s*\{[^}]*border-top:\s*1px solid var\(--ui-hairline\)/)
  })

  it('flattens nested tables and gives action buttons touch heights', () => {
    const m = mobileBlock()
    expect(m).toMatch(/\.st-table\s*\{[^}]*border:\s*0/)
    expect(m).toMatch(/\.st-table-row\s*\{[\s\S]*?min-height:\s*44px/)
    expect(m).toMatch(/\.st-actions \.ui-btn\s*\{[^}]*min-height:\s*40px/)
  })
})
