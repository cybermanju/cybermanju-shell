// CyberManju OS — LiveUiVerbs (src/utils/liveUiVerbs.ts).
// Role: pure TS helpers (unit-tested, transport-agnostic).
// Exports: tryLiveUiLine.
// Integrates: @/….
// Contracts: none special (pure logic; errors bubble to caller).
import { THEMES, type ThemeId } from '@/ui/tokens'
import { useTheme } from '@/composables/useTheme'
import type { ShellResult } from '@/types'

/**
 * Live interface-inspection verbs, answered from the running DOM + token
 * table instead of any backend — so `ui vars` / `ui palette` report the
 * ACTUAL rendered interface identically on desktop, web, and Pages.
 *
 * Interpreted in `invoke('os_exec')` before transport routing, which is
 * what makes them transport-agnostic: the cybsh terminal always runs in a
 * browser DOM (Tauri webview included). Server-side `.cybsh` scripts use
 * `ui get` (theme.json-backed on all three shells) instead.
 */
export function tryLiveUiLine(rawLine: string): ShellResult | null {
  const line = rawLine.trim()
  if (typeof document === 'undefined') return null
  const parts = line.split(/\s+/).filter(Boolean)
  if (parts[0] !== 'ui' || (parts[1] !== 'vars' && parts[1] !== 'palette')) return null
  const json = parts.includes('--json')
  const args = parts.slice(2).filter((a) => !a.startsWith('-'))

  if (parts[1] === 'vars') {
    return uiVars(line, args[0] ?? '', json)
  }
  return uiPalette(line, args[0] ?? '', json)
}

/** Every live `--ui-*` token with its computed (rendered) value. */
function uiVars(line: string, filter: string, json: boolean): ShellResult {
  const theme = useTheme()
  const cs = getComputedStyle(document.documentElement)
  const q = filter.toLowerCase()
  const rows: Array<[string, string]> = []
  for (const key of Object.keys(theme.cssVars.value)) {
    if (q && !key.toLowerCase().includes(q)) continue
    rows.push([key, cs.getPropertyValue(key).trim()])
  }
  if (!rows.length) {
    const detail = `no interface tokens match '${filter}'`
    return { ok: false, line, output: detail, error: detail, prompt: 'cybsh> ' }
  }
  if (json) {
    return { ok: true, line, output: JSON.stringify(Object.fromEntries(rows)), prompt: 'cybsh> ' }
  }
  const head = `--ui-* tokens live in the DOM (${rows.length}${filter ? ` matching '${filter}'` : ''})`
  return { ok: true, line, output: `${head}\n${rows.map(([k, v]) => `${k}: ${v}`).join('\n')}`, prompt: 'cybsh> ' }
}

/** A theme's full color table + design language (shape/font/elevation). */
function uiPalette(line: string, want: string, json: boolean): ShellResult {
  const theme = useTheme()
  const id = (want || theme.themeId.value) as ThemeId
  const def = (THEMES as Record<string, (typeof THEMES)[ThemeId] | undefined>)[id]
  if (!def) {
    const ids = Object.keys(THEMES).join(', ')
    const detail = `invalid: unknown theme '${want}' (try: ${ids})`
    return { ok: false, line, output: detail, error: detail, prompt: 'cybsh> ' }
  }
  const active = id === theme.themeId.value ? ' · active' : ''
  if (json) {
    return { ok: true, line, output: JSON.stringify(def), prompt: 'cybsh> ' }
  }
  const p = def.palette
  const colors = [
    ['bg', p.bg],
    ['bgDeep', p.bgDeep],
    ['surface', p.surface],
    ['surface2', p.surface2],
    ['surface3', p.surface3],
    ['window', p.window],
    ['windowIdle', p.windowIdle],
    ['border', p.border],
    ['borderStrong', p.borderStrong],
    ['borderHover', p.borderHover],
    ['hairline', p.hairline],
    ['text', p.text],
    ['text2', p.text2],
    ['text3', p.text3],
    ['textFaint', p.textFaint],
    ['accent', p.accent],
    ['onAccent', p.onAccent],
    ['success', p.success],
    ['warning', p.warning],
    ['danger', p.danger],
    ['info', p.info],
  ]
  const d = def.design
  const head = `${def.label} (${def.id}, ${def.mode}${active}) · ${d.shape} shape · ${d.font} type · ${d.elevation} elevation`
  return {
    ok: true,
    line,
    output: `${head}\n${colors.map(([k, v]) => `${k}: ${v}`).join('\n')}`,
    prompt: 'cybsh> ',
  }
}
