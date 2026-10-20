// CyberManju OS — ResearchRank (src/utils/researchRank.ts).
/**
 * Self-research ranking twin of `rank_research_files` in
 * `crates/agent/src/self_research.rs` — same alias bridge, same scoring
 * (path-substring overlap + source bonus, score desc then path asc), so the
 * browser loop surfaces the same implementor files as the native loop.
 * Keep the two in sync when either changes.
 */

/** Max files a single `self_research` sweep reads back. */
export const RESEARCH_MAX_FILES = 12

const RESEARCH_ALIASES: Record<string, string[]> = {
  theme: ['tokens', 'shell', 'staticcybsh'],
  accent: ['tokens', 'theme', 'shell'],
  ui: ['tokens', 'theme', 'panel', 'shell', 'staticcybsh'],
  interface: ['ui', 'tokens', 'theme', 'panel'],
  appearance: ['tokens', 'theme'],
  cybsh: ['shell', 'staticcybsh'],
  shell: ['cybsh', 'staticcybsh'],
  terminal: ['terminal', 'shell', 'cybsh'],
  verb: ['shell', 'cybsh', 'staticcybsh'],
  command: ['shell', 'cybsh'],
  glass: ['tokens', 'theme'],
  density: ['tokens', 'theme'],
  motion: ['tokens', 'theme'],
  glow: ['tokens', 'theme'],
  palette: ['tokens', 'theme'],
  color: ['tokens', 'theme', 'palette'],
  colour: ['tokens', 'theme', 'palette'],
  font: ['tokens', 'theme'],
  radius: ['tokens', 'theme'],
  shadow: ['tokens', 'theme'],
}

const SOURCE_BONUS = /\.(rs|ts|vue|md)$/i

export function researchQueryTerms(query: string): string[] {
  const want = query
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((w) => w.length >= 2)
  for (const word of [...want]) {
    for (const alias of RESEARCH_ALIASES[word] ?? []) {
      if (!want.includes(alias)) want.push(alias)
    }
  }
  return want
}

/** Deterministic rank: score desc, path asc. No overlap → `[]`. */
export function rankResearchPaths(paths: string[], query: string, limit: number): string[] {
  const want = researchQueryTerms(query)
  if (!want.length) return []
  const cap = Math.min(RESEARCH_MAX_FILES, Math.max(1, limit || 8))
  return paths
    .map((p) => {
      const lower = p.toLowerCase()
      let score = 0
      for (const w of want) if (lower.includes(w)) score++
      if (score > 0 && SOURCE_BONUS.test(p)) score++
      return { p, score }
    })
    .filter((r) => r.score > 0)
    .sort((a, b) => b.score - a.score || (a.p < b.p ? -1 : 1))
    .slice(0, cap)
    .map((r) => r.p)
}
