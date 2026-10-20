// CyberManju OS — Skills.test (tests/frontend/skills.test.ts).
// ANALYSIS.md §4.8: skills as shareable apps (SKILL.md manifest layer).
import { describe, expect, it } from 'vitest'
import { listSkills, parseSkillMd, validateSkill } from '../../src/utils/skills'

const SKILL = `---
name: Photo Triage
version: 1.2.0
description: Sort new photos fast
run: triage.cybsh
---
# Photo Triage
Do the thing.
`

describe('parseSkillMd', () => {
  it('reads frontmatter with safe defaults', () => {
    const m = parseSkillMd('.cybermanju/skills/photo-triage/SKILL.md', SKILL)!
    expect(m).toMatchObject({ slug: 'photo-triage', name: 'Photo Triage', version: '1.2.0', run: 'triage.cybsh' })
  })

  it('falls back to folder slug + first line without frontmatter', () => {
    const m = parseSkillMd('.cybermanju/skills/my-skill/SKILL.md', '# My Skill\nbody')!
    expect(m.slug).toBe('my-skill')
    expect(m.version).toBe('0.1.0')
  })
})

describe('validateSkill', () => {
  it('flags bad slugs/versions/missing run scripts', () => {
    const m = parseSkillMd('.cybermanju/skills/photo-triage/SKILL.md', SKILL)!
    expect(validateSkill(m, ['.cybermanju/skills/photo-triage/triage.cybsh'])).toEqual([])
    expect(validateSkill(m, [])).toHaveLength(1)
    expect(validateSkill({ ...m, slug: 'Bad_Slug' }, []).length).toBeGreaterThan(1)
  })
})

describe('listSkills', () => {
  it('lists SKILL.md files under .cybermanju/skills, sorted', () => {
    const files = [
      { path: '.cybermanju/skills/b/SKILL.md', content: SKILL },
      { path: '.cybermanju/skills/a/SKILL.md', content: SKILL },
      { path: 'other/SKILL.md', content: SKILL },
    ]
    expect(listSkills(files).map((s) => s.slug)).toEqual(['a', 'b'])
  })
})
