// CyberManju OS — Settings mobile.test (tests/frontend/settings-mobile.test.ts).
// Phone Settings contract: the sheet is a standard app settings STACK —
// large-title root index → one pushed detail behind a back bar — instead of
// the desktop jump rail + one long scroll, and the phone chrome above it is
// the slim status-bar strip (`--mobile-header-h`), not a desktop menu bar.
import { describe, expect, it } from 'vitest'
import fs from 'node:fs'

const settings = fs.readFileSync('src/components/SettingsPage.vue', 'utf8')
const topbar = fs.readFileSync('src/components/TopMenuBar.vue', 'utf8')
const appWindow = fs.readFileSync('src/components/AppWindow.vue', 'utf8')
const app = fs.readFileSync('src/App.vue', 'utf8')
const ui = fs.readFileSync('src/assets/ui.css', 'utf8')

const SECTION_IDS = ['appearance', 'remote', 'broker', 'sync', 'gestures', 'keys', 'about'] as const

describe('phone root: grouped section index', () => {
  it('lists every section as one inset group of icon/title/value/chevron cells', () => {
    expect(settings).toContain('class="st-list"')
    expect(settings).toContain('class="st-list-row"')
    expect(settings).toContain('openMobileSection(s)')
    expect(settings).toContain('sectionValue(s.id)')
    expect(settings).toContain('st-list-chevron')
    expect(settings).toContain('st-list-icon')
    // Large title + a 16px search field (no iOS focus zoom) sit above it.
    expect(settings).toContain('class="st-search"')
    expect(settings).toMatch(/\.st--mobile \.st-top--mobile:not\(\.is-detail\) \.st-title/)
    expect(settings).toMatch(/\.st-search input\s*\{[^}]*font-size:\s*16px/)
  })

  it('folds the connection summary into the root as its own group', () => {
    expect(settings).toContain('class="st-strip st-strip--mobile"')
    expect(settings).toMatch(/<section v-if="!isMobile" class="st-strip"/)
    expect(settings).toContain('st-foot')
  })

  it('keeps the whole stack off desktop windows (shell query, not a hard breakpoint)', () => {
    expect(settings).toContain(`:class="{ 'st--mobile': isMobile }"`)
    expect(settings).toContain("matchMedia?.('(max-width: 768px)')")
    // Desktop keeps its jump rail + pinned strip; phones render neither.
    expect(settings).toContain('<nav v-if="!isMobile" class="st-jumps"')
  })
})

describe('phone detail: pushed section behind a back bar', () => {
  it('renders a 44px nav bar with back · title · action', () => {
    expect(settings).toContain('v-if="isMobile && mobileSection"')
    expect(settings).toContain('class="st-top st-top--mobile is-detail"')
    expect(settings).toContain('st-detail-title')
    expect(settings).toContain('closeMobileSection')
    expect(settings).toMatch(/\.st--mobile \.st-top\.is-detail\s*\{[\s\S]*?min-height:\s*44px/)
  })

  it('shows exactly the pushed card, every group still reachable from the root', () => {
    for (const id of SECTION_IDS) {
      expect(settings, `card ${id} not gated on the stack`).toContain(`inSection('${id}')`)
      // The desktop search filter keeps owning visibility on top of the stack.
      expect(settings, `card ${id} lost its filter gate`).toContain(`sectionVisible('${id}')`)
    }
    expect(settings).toContain('return !isMobile.value || mobileSectionId.value === id')
    // …and the no-results state still covers the root list.
    expect(settings).toContain('No results for')
    expect(settings).toContain(`@click="sectionQuery = ''"`)
  })

  it('starts each push at the top of the group', () => {
    expect(settings).toMatch(/function openMobileSection[\s\S]*?bodyEl\.value\?\.scrollTo\(\{ top: 0 \}\)/)
    expect(settings).toMatch(/function closeMobileSection[\s\S]*?bodyEl\.value\?\.scrollTo\(\{ top: 0 \}\)/)
  })
})

describe('stack navigation integration', () => {
  it('pops the detail on Android system back and on Escape, never the sheet', () => {
    expect(settings).toContain(`window.addEventListener('cybermanju:mobile-back'`)
    expect(settings).toContain(`document.addEventListener('keydown', handleStackEscape)`)
    expect(settings).toMatch(/function handleMobileBack[\s\S]*?e\.preventDefault\(\)/)
    // App.vue lets the open sheet claim the back press before closing itself,
    // and names it in the detail so background sheets can't steal the press.
    expect(app).toContain(`new CustomEvent('cybermanju:mobile-back', {`)
    expect(app).toContain('detail: { panel: active.panelType }')
    expect(app).toContain('if (!innerBack.defaultPrevented)')
    expect(settings).toContain(`detail?.panel !== 'settings'`)
    expect(fs.readFileSync('src/components/FileManager.vue', 'utf8')).toContain(
      `detail?.panel !== 'files'`,
    )
  })

  it('keeps the Accounts → Configure deep link working on the stack', () => {
    expect(settings).toContain(`window.addEventListener('cybermanju:settings-focus'`)
    expect(settings).toContain('if (isMobile.value) openMobileSection(broker)')
  })

  it('leaves the scroll spy to the desktop page (the stack picks sections)', () => {
    expect(settings).toMatch(/function onBodyScroll\(\) \{\n\s*\/\/ The phone stack/)
    expect(settings).toContain('offsetParent === null')
  })
})

describe('phone chrome: status-bar strip + one header', () => {
  it('sizes the top strip from --mobile-header-h instead of a 52px bar', () => {
    expect(ui).toContain('--mobile-header-h: 26px')
    expect(topbar).toMatch(/height:\s*var\(--mobile-header-h\)/)
    expect(topbar).toMatch(/min-height:\s*var\(--mobile-header-h\)/)
    expect(topbar).not.toContain('calc(52px + env(safe-area-inset-top')
    expect(ui).not.toMatch(/top:\s*calc\(52px \+ env\(safe-area-inset-top/)
  })

  it('lands every fixed sheet exactly on the strip’s bottom edge', () => {
    // The notch is `.cybermanju-shell`'s padding; the strip adds no inset of
    // its own, so top = inset + strip height (no gap, no overlap).
    expect(ui).toMatch(/\.cybermanju-shell\s*\{[\s\S]*?padding-top:\s*env\(safe-area-inset-top/)
    expect(ui).toMatch(
      /\.app-window\s*\{[\s\S]*?top:\s*calc\(env\(safe-area-inset-top, 0px\) \+ var\(--mobile-header-h\)\)/,
    )
  })

  it('drops the generic sheet titlebar for Settings so the stack header is the only one', () => {
    expect(appWindow).toContain("'app-window--bare-title': isBareTitle")
    expect(appWindow).toContain(`props.win.panelType === 'settings'`)
    expect(appWindow).toMatch(/\.app-window--bare-title \.window-titlebar\s*\{\s*display:\s*none/)
    // Other panels keep a standard 44px mobile nav bar.
    expect(appWindow).toMatch(/\.window-titlebar\s*\{[^}]*height:\s*44px/)
  })
})
