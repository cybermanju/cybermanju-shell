// CyberManju OS — Skills (src/utils/skills.ts).
// Role: skills as shareable apps, day one (ANALYSIS.md §4.8) —
// `.cybermanju/skills/*/SKILL.md` + the sync layer → installable skills that
// ship prompts + `.cybsh` scripts. No marketplace infrastructure: a skill is
// a folder, `SKILL.md` is the manifest, `skill install <url>` copies it.
// Exports: SkillManifest, parseSkillMd, validateSkill, listSkills,
// skillSlug (shareable-apps layer) + SkillPanelTarget, skillPanels
// (panels-later: `panel:` frontmatter → addressable panels) +
// SKILL_FILE_BUDGET,
// SKILL_TOTAL_BUDGET, HermesSkillManifest, ParsedSkill, SkillRecord,
// parseSkillFrontmatter, assembleStandingOrders, selectSkills (Hermes
// standing-orders layer: budget-capped prompt assembly + goal selection).
// Integrates: standalone over `{ path, content }` volume listings;
// `skill` cybsh verb (staticCybsh.ts) executes; sync carries the folder.
// Contracts: pure, never throws; frontmatter is `---\nkey: value\n---`;
// name+description required, everything else optional with safe defaults.
// `panel:` frontmatter (ANALYSIS.md §4.8 panels-later) lets a skill ship one
// panel target (`panel: <id>`, optional `tab:` + `path:`) rendered through the
// panels alias table — no marketplace infrastructure, just a manifest field.
export interface SkillManifest {
  /** Folder slug (`photo-triage`). */
  slug: string
  name: string
  version: string
  description: string
  /** `.cybsh` entry point relative to the skill folder. */
  run: string
  /** Full volume path of the SKILL.md file. */
  path: string
  /** Optional panel target shipped by the skill (`ui_open_panel` id). */
  panel?: string
  /** Optional tab prop for the panel target. */
  tab?: string
  /** Optional files-only path prop for the panel target. */
  panelPath?: string
}

export function skillSlug(path: string): string {
  const parts = path.split('/').filter(Boolean)
  parts.pop()
  return parts.pop() ?? 'skill'
}

/** Parse `SKILL.md` text; null when the manifest is unusable. */
export function parseSkillMd(path: string, text: string): SkillManifest | null {
  const lines = text.split(/\r?\n/)
  let body = text
  const meta: Record<string, string> = {}
  if (lines[0]?.trim() === '---') {
    const end = lines.findIndex((l, i) => i > 0 && l.trim() === '---')
    if (end > 0) {
      for (const line of lines.slice(1, end)) {
        const m = /^([A-Za-z0-9_-]+)\s*:\s*(.+)$/.exec(line)
        if (m) meta[m[1].toLowerCase()] = m[2].trim()
      }
      body = lines.slice(end + 1).join('\n')
    }
  }
  const name = (meta.name ?? '').trim() || skillSlug(path).replace(/-/g, ' ')
  const description = (meta.description ?? '').trim() || body.trim().split('\n')[0]?.slice(0, 200) || name
  if (!name || !description) return null
  const panel = (meta.panel ?? '').trim()
  return {
    slug: skillSlug(path),
    name,
    version: (meta.version ?? '').trim() || '0.1.0',
    description,
    run: (meta.run ?? '').trim() || 'skill.cybsh',
    path,
    ...(panel ? { panel } : {}),
    ...((meta.tab ?? '').trim() ? { tab: (meta.tab ?? '').trim() } : {}),
    ...((meta.path ?? '').trim() ? { panelPath: (meta.path ?? '').trim() } : {}),
  }
}

/** Human-readable problems (empty = installable). */
export function validateSkill(manifest: SkillManifest, files: string[]): string[] {
  const problems: string[] = []
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(manifest.slug)) {
    problems.push(`slug '${manifest.slug}' must be lowercase alnum + dashes`)
  }
  if (!/^\d+\.\d+\.\d+$/.test(manifest.version)) {
    problems.push(`version '${manifest.version}' should be semver`)
  }
  const dir = manifest.path.split('/').slice(0, -1).join('/')
  if (!files.some((f) => f === `${dir}/${manifest.run}`)) {
    problems.push(`missing run script ${dir}/${manifest.run}`)
  }
  return problems
}

/** Every `SKILL.md` under `.cybermanju/skills/` becomes a manifest. */
export function listSkills(files: Array<{ path: string; content: string }>): SkillManifest[] {
  const out: SkillManifest[] = []
  for (const file of files) {
    if (!file.path.includes('.cybermanju/skills/')) continue
    if (!file.path.endsWith('/SKILL.md') && file.path !== '.cybermanju/skills/SKILL.md') continue
    const manifest = parseSkillMd(file.path, file.content)
    if (manifest) out.push(manifest)
  }
  out.sort((a, b) => a.slug.localeCompare(b.slug))
  return out
}

/** Panel target a skill ships (`skill:<slug>`), or null when panel-less. */
export interface SkillPanelTarget {
  /** Stable panel address (`skill:<slug>`). */
  address: string
  slug: string
  panel: string
  tab?: string
  path?: string
}

/** Every skill with `panel:` frontmatter becomes one addressable panel. */
export function skillPanels(manifests: SkillManifest[]): SkillPanelTarget[] {
  const out: SkillPanelTarget[] = []
  for (const m of manifests) {
    if (!m.panel?.trim()) continue
    out.push({
      address: `skill:${m.slug}`,
      slug: m.slug,
      panel: m.panel.trim(),
      ...(m.tab ? { tab: m.tab } : {}),
      ...(m.panelPath ? { path: m.panelPath } : {}),
    })
  }
  out.sort((a, b) => a.slug.localeCompare(b.slug))
  return out
}

// ─── Hermes standing-orders layer ─────────────────────────────────────
// Versioned instruction packs (manifest + markdown body) folded into the
// system prompt as standing orders. Pure, unit-tested half: frontmatter
// parsing, budget-capped assembly, and goal-overlap selection. Budgets
// mirror `docs/OPERATIONS.md` §4b: 8 KiB per file, 24 KiB total.
// (Named `Hermes*` to distinguish from the shareable-apps SkillManifest.)

/** Per-file cap: one bloated skill cannot eat the window. */
export const SKILL_FILE_BUDGET = 8 * 1024
/** Total cap across all standing-order files. */
export const SKILL_TOTAL_BUDGET = 24 * 1024

export interface HermesSkillManifest {
  name: string
  description: string
  version?: string
  tools: string[]
}

export interface ParsedSkill {
  manifest: HermesSkillManifest | null
  body: string
}

function parseHermesTools(value: string): string[] {
  const t = value.trim().replace(/^\[/, '').replace(/\]$/, '')
  return t
    .split(',')
    .map(s => s.trim().replace(/^['"]|['"]$/g, ''))
    .filter(Boolean)
}

/**
 * Parse `---\nkey: value\n---\nbody` frontmatter. Tolerant: no frontmatter →
 * `{ manifest: null, body: text }`; malformed lines are skipped; never throws.
 * Recognised keys (case-insensitive): `name`, `description`, `version`,
 * `tools` (csv or `[a, b]`).
 */
export function parseSkillFrontmatter(text: string): ParsedSkill {
  const src = text ?? ''
  const m = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/.exec(src)
  if (!m) return { manifest: null, body: src }
  const raw: Record<string, string> = {}
  for (const line of m[1].split('\n')) {
    const i = line.indexOf(':')
    if (i <= 0) continue
    raw[line.slice(0, i).trim().toLowerCase()] = line.slice(i + 1).trim()
  }
  const name = raw.name ?? ''
  const description = raw.description ?? ''
  if (!name && !description) return { manifest: null, body: src.slice(m[0].length) }
  return {
    manifest: {
      name,
      description,
      version: raw.version || undefined,
      tools: raw.tools ? parseHermesTools(raw.tools) : [],
    },
    body: src.slice(m[0].length),
  }
}

export interface StandingOrderFile {
  path: string
  content: string
}

export interface StandingOrderEntry {
  path: string
  chars: number
  truncated: boolean
}

export interface StandingOrdersResult {
  block: string
  included: StandingOrderEntry[]
  /** Files dropped because the total budget was already spent. */
  omitted: string[]
}

/**
 * Assemble the `<standing-orders>` prompt block under the file/total budgets.
 * Truncated files carry a `truncated:` marker so the machine-prefix contract
 * still classifies them as partial, never as complete instructions.
 */
export function assembleStandingOrders(files: StandingOrderFile[]): StandingOrdersResult {
  const included: StandingOrderEntry[] = []
  const omitted: string[] = []
  const parts: string[] = []
  let used = 0
  for (const f of files) {
    const full = f.content ?? ''
    if (!full) continue
    const room = SKILL_TOTAL_BUDGET - used
    if (room <= 0) {
      omitted.push(f.path)
      continue
    }
    const fileCap = Math.min(SKILL_FILE_BUDGET, room)
    const truncated = full.length > fileCap
    const slice = truncated ? full.slice(0, fileCap) : full
    const cut = full.length - slice.length
    const body = truncated
      ? `${slice}\n…truncated: cut ${cut} chars (skill budget ${SKILL_FILE_BUDGET}/${SKILL_TOTAL_BUDGET})`
      : slice
    parts.push(`<file path="${f.path}">\n${body}\n</file>`)
    used += slice.length
    included.push({ path: f.path, chars: slice.length, truncated })
  }
  return {
    block: `<standing-orders>\n${parts.join('\n')}\n</standing-orders>`,
    included,
    omitted,
  }
}

export interface SkillRecord {
  id: string
  manifest: HermesSkillManifest
  body: string
}

const hermesTokenize = (text: string): string[] =>
  text
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(w => w.length >= 2)

/**
 * Rank skills by keyword overlap between the goal and `name + description`.
 * Deterministic (score desc, id asc). No overlap → `[]`: unrelated skills are
 * never injected. `limit` defaults to 3.
 */
export function selectSkills(skills: SkillRecord[], goal: string, limit = 3): SkillRecord[] {
  const want = new Set(hermesTokenize(goal ?? ''))
  if (!want.size) return []
  const n = Math.max(1, Math.floor(limit))
  return skills
    .map(s => {
      const hay = `${s.manifest.name} ${s.manifest.description}`.toLowerCase()
      let score = 0
      for (const w of want) {
        if (hay.includes(w)) score++
      }
      return { s, score }
    })
    .filter(r => r.score > 0)
    .sort((a, b) => b.score - a.score || (a.s.id < b.s.id ? -1 : a.s.id > b.s.id ? 1 : 0))
    .slice(0, n)
    .map(r => r.s)
}
