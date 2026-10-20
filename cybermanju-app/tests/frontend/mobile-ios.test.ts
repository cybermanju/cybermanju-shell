// CyberManju OS — Mobile ios.test (tests/frontend/mobile-ios.test.ts).
// iOS-aesthetic mobile contract, derived from the iOS4Android reference
// (cloned to /tmp, never into the repo): paged springboard with dots,
// three-zone status bar, squircle icons, dock + home indicator, and an
// iOS-style tab bar with haptics. Pins structure, not pixels.
import { describe, expect, it } from 'vitest'
import fs from 'node:fs'

const launcher = fs.readFileSync('src/components/MobileLauncher.vue', 'utf8')
const nav = fs.readFileSync('src/components/MobileNav.vue', 'utf8')
const topbar = fs.readFileSync('src/components/TopMenuBar.vue', 'utf8')

describe('iOS springboard pager (PageView + IconGrid homage)', () => {
  it('chunks apps into pages of 8 (2 rows x 4)', () => {
    expect(launcher).toContain('MLA_PAGE_SIZE = 8')
    expect(launcher).toMatch(/filtered\.value\.slice\(i, i \+ MLA_PAGE_SIZE\)/)
  })

  it('renders a horizontal snap pager with dots', () => {
    expect(launcher).toContain('mla-pages')
    expect(launcher).toContain('mla-grid')
    expect(launcher).toMatch(/scroll-snap-type:\s*x mandatory/)
    expect(launcher).toContain('mla-dots')
    expect(launcher).toContain('mla-page-dot')
    expect(launcher).toContain('goToPage')
  })

  it('resets to the first page on a new search', () => {
    expect(launcher).toMatch(/watch\(filtered/)
  })
})

describe('iOS status bar + icons + dock', () => {
  it('uses one unified mobile header (no duplicated status row or clock)', () => {
    // The single TopMenuBar header owns connectivity + vaults + the clock.
    expect(topbar).toContain('tmb-mobile-meta')
    expect(topbar).toContain('tmb-net')
    expect(topbar).toContain('tmb-vaults')
    expect(topbar).toMatch(/\.tmb-mobile-meta\s*\{[\s\S]*?display:\s*inline-flex/)
    // The launcher must not render its own status row / second clock.
    expect(launcher).not.toContain('mla-status')
    expect(launcher).not.toContain('mla-clock')
    expect(launcher).not.toContain('mla-vault-count')
  })

  it('respects the notch/safe-area at the top (shell owns the inset)', () => {
    // The bar is the slim `--mobile-header-h` status strip; the notch itself
    // is `.cybermanju-shell`'s safe-area padding, so the bar never grows
    // around it — and every fixed sheet top lands exactly on its bottom edge.
    expect(topbar).toContain('var(--mobile-header-h)')
    expect(topbar).not.toMatch(/height:\s*calc\(\d+px \+ env\(safe-area-inset-top/)
    const shell = fs.readFileSync('src/App.vue', 'utf8')
    expect(shell).toContain('class="cybermanju-shell"')
    const ui = fs.readFileSync('src/assets/ui.css', 'utf8')
    expect(ui).toMatch(/\.cybermanju-shell\s*\{[\s\S]*?padding-top:\s*env\(safe-area-inset-top/)
    expect(ui).toMatch(
      /top:\s*calc\(env\(safe-area-inset-top, 0px\) \+ var\(--mobile-header-h\)\)/,
    )
  })

  it('uses squircle icon corners with a top-light gloss', () => {
    expect(launcher).toMatch(/\.mla-tile\s*\{[\s\S]*?border-radius:\s*16px/)
    expect(launcher).toContain('.mla-tile::before')
    expect(launcher).toMatch(/\.mla-dock-icon\s*\{[\s\S]*?border-radius:\s*15px/)
  })

  it('keeps springboard density tight (page 1 + dock fit a phone viewport)', () => {
    expect(launcher).toMatch(/\.mla\s*\{[\s\S]*?gap:\s*16px/)
    expect(launcher).toMatch(/\.mla-title\s*\{[^}]*clamp\(24px/)
    expect(launcher).toMatch(/\.mla-grid\s*\{[\s\S]*?gap:\s*16px 7px/)
  })

  it('uses native press-dim feedback and iOS Spotlight wording', () => {
    expect(launcher).toMatch(/\.mla-app:active\s*\{[^}]*brightness\(0\.92\)/)
    expect(launcher).toContain('placeholder="Search"')
    expect(launcher).toMatch(/\.mla-dock-icon\s*\{[\s\S]*?width:\s*52px/)
  })

  it('shows a dock and a home-indicator pill', () => {
    expect(launcher).toContain('mla-dock')
    expect(launcher).toContain('mla-home')
  })
})

describe('iOS tab bar', () => {
  it('adds a home indicator and haptics on every tab', () => {
    expect(nav).toContain('mn-home')
    // Both goHome and go vibrate.
    const vibrateCount = (nav.match(/vibrate\?\.?\(/g) ?? []).length
    expect(vibrateCount).toBeGreaterThanOrEqual(2)
    expect(nav).toMatch(/blur\(28px\)/)
  })
})

describe('iOS native scaffold (P2-16)', () => {
  const conf = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8'))
  const runbook = fs.readFileSync('docs/IOS.md', 'utf8')

  it('sets the iOS deployment floor', () => {
    expect(conf.bundle.iOS.minimumSystemVersion).toBe('17.0')
  })

  it('keeps the mobile IPC set transport-complete (no dashboard on iOS)', () => {
    const tauri = fs.readFileSync('src/composables/useTauri.ts', 'utf8')
    for (const cmd of ['list_devices', 'list_todos', 'list_lan_peers']) {
      expect(tauri).toContain(`'${cmd}'`)
    }
  })

  it('documents the ATS exception + backup parity + signing gap honestly', () => {
    expect(runbook).toContain('NSAllowsLocalNetworking')
    expect(runbook).toContain('NSURLIsExcludedFromBackupKey')
    expect(runbook).toContain('ios-build')
  })

  it('ships the iOS native configure + unsigned device build path', () => {
    const configure = fs.readFileSync('scripts/ios-configure.sh', 'utf8')
    expect(configure).toContain('minimumSystemVersion')
    expect(configure).toContain('NSAllowsLocalNetworking')
    const signing = fs.readFileSync('scripts/ios-signing.sh', 'utf8')
    for (const verb of ['prepare)', 'verify)', 'cleanup)']) {
      expect(signing).toContain(verb)
    }
    expect(signing).toContain('REQUIRE_SIGNING')
    const ci = fs.readFileSync('.github/workflows/ci.yml', 'utf8')
    const iosJob = ci.slice(
      ci.indexOf('  ios-build:'),
      ci.indexOf('  macos-build:'),
    )
    expect(iosJob).toContain('ios-build:')
    expect(iosJob).toContain('macos-latest')
    expect(iosJob).toContain('tauri ios init')
    expect(iosJob).toContain(
      'npx tauri ios build --ci --target aarch64 --no-sign',
    )
    expect(iosJob).toContain('xcrun otool -l')
  })

  it('always uploads an unsigned device IPA without App Store export', () => {
    const ci = fs.readFileSync('.github/workflows/ci.yml', 'utf8')
    const iosJob = ci.slice(
      ci.indexOf('  ios-build:'),
      ci.indexOf('  macos-build:'),
    )
    expect(iosJob).toContain('dist-ios-device-unsigned/*.ipa')
    expect(iosJob).toContain('Payload/')
    expect(iosJob).toContain('--target aarch64 --no-sign')
    expect(iosJob).toContain('embedded.mobileprovision')
    expect(iosJob).toContain('codesign -dv --verbose=4')
    expect(iosJob).not.toMatch(/^\s*run:\s*.*--export-method/m)
    expect(iosJob).not.toMatch(/^\s*run:\s*.*--archive-only/m)
  })
})
