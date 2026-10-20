// CyberManju OS — Self research.test (tests/frontend/self-research.test.ts).
// Self-research ranking twin of `rank_research_files` — UI/cybsh questions
// must bridge to the implementor files (shells, tokens, theme hook).
import { describe, it, expect } from 'vitest'
import { rankResearchPaths, researchQueryTerms, RESEARCH_MAX_FILES } from '@/utils/researchRank'

const PATHS = [
  'crates/os/src/shell.rs',
  'crates/os-wasm/src/os.rs',
  'src/utils/staticCybsh.ts',
  'src/ui/tokens.ts',
  'src/composables/useTheme.ts',
  'src/components/TerminalPanel.vue',
  'docs/OPERATIONS.md',
  'src/components/AgentPanel.vue',
]

describe('research ranking', () => {
  it('is deterministic and source-preferring', () => {
    const ranked = rankResearchPaths(['logo.png', 'src/agent_loop.rs', 'docs/agent-review.md'], 'agent loop', 5)
    expect(ranked).toContain('src/agent_loop.rs')
    expect(ranked).not.toContain('logo.png')
    expect(rankResearchPaths(PATHS, '', 5)).toEqual([])
    expect(rankResearchPaths(PATHS, 'zzzqqq', 5)).toEqual([])
  })

  it('bridges interface questions to implementors', () => {
    const themed = rankResearchPaths(PATHS, 'change the interface theme', 8)
    expect(themed).toContain('src/ui/tokens.ts')
    expect(themed).toContain('crates/os/src/shell.rs')
    expect(themed).toContain('src/composables/useTheme.ts')
    const verbs = rankResearchPaths(PATHS, 'cybsh ui verbs', 8)
    expect(verbs).toContain('src/utils/staticCybsh.ts')
    expect(verbs).toContain('crates/os/src/shell.rs')
    const accent = rankResearchPaths(PATHS, 'per theme accent color', 8)
    expect(accent).toContain('src/ui/tokens.ts')
  })

  it('caps at the research file budget', () => {
    expect(RESEARCH_MAX_FILES).toBe(12)
    const many = Array.from({ length: 30 }, (_, i) => `src/theme${i}.ts`)
    expect(rankResearchPaths(many, 'theme', 30).length).toBeLessThanOrEqual(RESEARCH_MAX_FILES)
  })

  it('expands aliases without duplicates', () => {
    const terms = researchQueryTerms('ui theme')
    expect(terms).toContain('tokens')
    expect(new Set(terms).size).toBe(terms.length)
  })
})
