// CyberManju OS — Responsive.test (tests/frontend/responsive.test.ts).
// Responsive contract: every panel must stay usable in a narrow window and
// on phones. Viewport `@media` queries never fire for a small floating
// window on a wide monitor, so `AppWindow` mirrors the breakpoint as
// `.app-window--narrow` and `src/assets/ui.css` carries the per-window
// collapse rules next to the viewport ones. This test pins that pairing:
// every collapse behavior must exist in BOTH forms.
import { describe, expect, it } from 'vitest'
import fs from 'node:fs'

const css = fs.readFileSync('src/assets/ui.css', 'utf8')

function viewportBlock(): string {
  const start = css.indexOf('@media (max-width: 768px)')
  const narrow = css.indexOf('── narrow windows')
  expect(start, 'missing @media (max-width: 768px) sheet block').toBeGreaterThanOrEqual(0)
  expect(narrow, 'missing narrow-window section').toBeGreaterThan(start)
  return css.slice(start, narrow)
}

function narrowBlock(): string {
  const narrow = css.indexOf('── narrow windows')
  expect(narrow, 'missing narrow-window section').toBeGreaterThanOrEqual(0)
  return css.slice(narrow)
}

describe('mobile sheets (viewport)', () => {
  it('pins windows to fullscreen sheets with no drag/resize', () => {
    const v = viewportBlock()
    expect(v).toMatch(/\.app-window\s*\{[^}]*position:\s*fixed/)
    expect(v).toMatch(/right:\s*env\(safe-area-inset-right/)
    expect(v).toMatch(/left:\s*env\(safe-area-inset-left/)
    expect(v).toMatch(/border-radius:\s*0\s*!important/)
    expect(v).toMatch(/\.resize-handle\s*\{[^}]*display:\s*none/)
    expect(v).toMatch(/\.dock-container[\s\S]*?display:\s*none/)
  })

  it('keeps tab strips swipeable and controls at touch sizes', () => {
    const v = viewportBlock()
    for (const tabs of ['\\.am-tabs', '\\.dm-tabs', '\\.org-tabs', '\\.sh-tabs']) {
      expect(v, `${tabs} must scroll horizontally`).toMatch(new RegExp(`${tabs}[\\s\\S]*?overflow-x:\\s*auto`))
    }
    expect(v).toMatch(/min-height:\s*44px/)
    expect(v).toMatch(/font-size:\s*16px/)
  })

  it('scrolls wide surfaces inside the sheet instead of widening it', () => {
    const v = viewportBlock()
    expect(v).toMatch(/\.task-table[\s\S]*?overflow-x:\s*auto/)
    expect(v).toMatch(/\.tf-canvas[\s\S]*?min-width:\s*440px/)
  })
})

describe('narrow windows (container)', () => {
  // Every collapse behavior needs a window-width twin: viewport queries
  // cannot see floating-window geometry.
  const twins: Array<[string, RegExp]> = [
    ['file manager single column', /\.app-window--narrow\s+\.fm-main[\s\S]*?grid-template-columns:\s*1fr/],
    ['file manager rails hidden', /\.app-window--narrow\s+\.fm-side/],
    ['code studio rails hidden', /\.app-window--narrow\s+\.cs-ai/],
    ['tab strips scroll', /\.app-window--narrow\s+\.org-tabs/],
    ['stats grid halves', /\.app-window--narrow\s+\.stats-grid/],
    ['disk cards stack', /\.app-window--narrow\s+\.disk-panel\s+\.cards/],
    ['process table scrolls', /\.app-window--narrow\s+\.task-table/],
    ['transfer canvas shrinks', /\.app-window--narrow\s+\.tf-canvas/],
    ['api locators wrap', /\.app-window--narrow\s+\.api-path/],
  ]

  for (const [label, re] of twins) {
    it(`covers narrow windows: ${label}`, () => {
      expect(narrowBlock(), label).toMatch(re)
    })
  }

  it('keeps the mobile agent integrated and exposes editor workspaces', () => {
    const studio = fs.readFileSync('src/components/CodeStudio.vue', 'utf8')
    expect(studio).toMatch(/@media\s*\(max-width:\s*860px\)[\s\S]*?\.cs-ai\s*\{\s*display:\s*flex;\s*position:\s*absolute/)
    expect(studio).toContain('<nav v-if="isMobileLayout" class="cs-mobilebar"')
    expect(narrowBlock()).toMatch(/\.app-window--narrow\s+\.cs-side/)
    expect(studio).toContain("'is-mobile-drawer': isMobileLayout")
    expect(narrowBlock()).toMatch(/\.app-window--narrow\s+\.cs-side\.is-mobile-drawer\s*\{\s*display:\s*flex/)
  })

  it('names icon-only close buttons in the editor panels', () => {
    const studio = fs.readFileSync('src/components/CodeStudio.vue', 'utf8')
    expect(studio).toMatch(/<button class="cs-ibtn" aria-label="Close find bar" @click="findOpen = false">/)
    expect(studio).toMatch(/<button class="cs-ibtn" aria-label="Close bottom panel" @click="bottomOpen = false">/)
  })
})

describe('mobile home + nav wiring', () => {
  it('renders the launcher under the sheets and the bottom nav above them', () => {
    const shell = fs.readFileSync('src/components/DesktopShell.vue', 'utf8')
    expect(shell).toMatch(/<MobileLauncher/)
    const app = fs.readFileSync('src/App.vue', 'utf8')
    expect(app).toMatch(/<MobileNav\s*\/>/)
    const nav = fs.readFileSync('src/components/MobileNav.vue', 'utf8')
    for (const tab of ['Home', 'Files', 'Search', 'Agent', 'Vaults', 'Settings']) {
      expect(nav, `missing ${tab} tab`).toContain(tab)
    }
    expect(nav).toMatch(/env\(safe-area-inset-bottom/)
  })

  it('keeps home apps clickable and opens desktop shortcuts on one click', () => {
    const shell = fs.readFileSync('src/components/DesktopShell.vue', 'utf8')
    expect(shell).toContain('@click="openShortcut(shortcut.panel)"')
    expect(shell).toMatch(/\.desktop-workspace\s*\{\s*pointer-events:\s*none/)
    expect(shell).toMatch(/\.desktop-workspace\s+:deep\(\.app-window\)\s*\{\s*pointer-events:\s*auto/)
    expect(shell).toMatch(/\.desktop-icons\s*\{\s*display:\s*none/)
    const launcher = fs.readFileSync('src/components/MobileLauncher.vue', 'utf8')
    expect(launcher).toMatch(/class="mla"[\s\S]*?@click\.stop/)
  })
})

describe('global motion accessibility policy', () => {
  it('registers reduced-motion handling at application startup', () => {
    const app = fs.readFileSync('src/App.vue', 'utf8')
    expect(app).toContain("import { useMotion } from '@/composables/useMotion'")
    expect(app).toMatch(/const wm = useWindowManager\(\)[\s\S]{0,120}useMotion\(\)/)
  })
})

describe('mobile onboarding surface', () => {
  it('uses an opaque full-screen sheet without CRT scanlines', () => {
    const wizard = fs.readFileSync('src/components/MobileSetupWizard.vue', 'utf8')
    expect(wizard).toMatch(/\.msw\s*\{[\s\S]*?position:\s*fixed[\s\S]*?z-index:\s*11000/)
    expect(wizard).toContain('background: var(--ui-bg, #f2f2f7)')
    expect(wizard).not.toContain('repeating-linear-gradient')
    expect(wizard).toContain('msw-welcome-title')
    expect(wizard).toContain('var(--ui-on-accent)')
  })

  it('signs in inline in both setup wizards (Account Manager stays secondary)', () => {
    const desktop = fs.readFileSync('src/components/SetupWizard.vue', 'utf8')
    const mobile = fs.readFileSync('src/components/MobileSetupWizard.vue', 'utf8')
    const accounts = fs.readFileSync('src/components/AccountManagerPanel.vue', 'utf8')
    expect(desktop).toContain('signInWithPopup')
    expect(mobile).toContain('signInWithPopup')
    expect(desktop).toContain('cybermanju:open-accounts')
    expect(mobile).toContain('cybermanju:open-accounts')
    expect(accounts).toContain('wizardCanSave')
    expect(accounts).toContain('if (!token && isOauthCapable(wiz.backendType))')
  })

  it('provides touch-first Files tools and a chat-first Agent drawer', () => {
    const files = fs.readFileSync('src/components/FileManager.vue', 'utf8')
    const agent = fs.readFileSync('src/components/AgentPanel.vue', 'utf8')
    const accounts = fs.readFileSync('src/components/AccountManagerPanel.vue', 'utf8')
    expect(files).toContain('fm-mobile-tools-menu')
    expect(files).toContain('Browse folders')
    expect(files).toContain('@container file-manager (max-width: 560px)')
    expect(agent).toContain('agent-mobile-nav')
    expect(agent).toContain('aria-label="Close navigation"')
    expect(agent).toContain('@container agent (max-width: 760px)')
    expect(accounts).toContain('@container account-manager (max-width: 700px)')
    expect(accounts).toContain('am-flyout-backdrop')
  })
})
