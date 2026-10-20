// CyberManju OS — Skill+Task verbs.test (tests/frontend/skill-task-verbs.test.ts).
// ANALYSIS.md §4.8 (skills as shareable apps) + §4.9 (todos in the shell):
// `skill list|show|run` over the volume skills dir, `task` over the shared
// Archive TASKS store. Deps are minimal fakes (node env, no window).
import { describe, expect, it } from 'vitest'
import {
  handlesStaticVerb,
  runStaticCybshLine,
  type StaticCybshDeps,
} from '../../src/utils/staticCybsh'

const SKILL_MD = `---
name: Photo Triage
version: 1.2.0
description: Sort new photos fast
run: triage.cybsh
---
# Photo Triage
`

function deps(volume: Record<string, string> = {}): StaticCybshDeps {
  return {
    readVolume: () => volume,
    writeVolumeFile: async (path, content) => {
      volume[path] = content
    },
    execFallback: async (line: string) => `ran:${line}`,
  } as unknown as StaticCybshDeps
}

describe('skill verb', () => {
  it('is a static verb', () => {
    expect(handlesStaticVerb('skill')).toBe(true)
    expect(handlesStaticVerb('task')).toBe(true)
  })

  it('lists installed skills', async () => {
    const d = deps({
      '.cybermanju/skills/photo-triage/SKILL.md': SKILL_MD,
      '.cybermanju/skills/photo-triage/triage.cybsh': '# triage\necho hi',
    })
    const list = await runStaticCybshLine('skill list', d)
    expect(list?.ok).toBe(true)
    expect(list?.output).toContain('photo-triage 1.2.0')
  })

  it('says so when no skills are installed', async () => {
    const list = await runStaticCybshLine('skill list', deps({}))
    expect(list?.ok).toBe(true)
    expect(list?.output).toContain('no skills installed')
  })

  it('shows a manifest with problems', async () => {
    const d = deps({ '.cybermanju/skills/lonely/SKILL.md': SKILL_MD })
    const show = await runStaticCybshLine('skill show lonely', d)
    expect(show?.ok).toBe(true)
    expect(show?.output).toContain('missing run script')
    const ghost = await runStaticCybshLine('skill show nope', d)
    expect(ghost?.ok).toBe(false)
    expect(ghost?.output).toMatch(/^not_found:/)
  })

  it('runs the entry script line by line', async () => {
    const d = deps({
      '.cybermanju/skills/photo-triage/SKILL.md': SKILL_MD,
      '.cybermanju/skills/photo-triage/triage.cybsh': '# triage\necho hi\necho yo',
    })
    const run = await runStaticCybshLine('skill run photo-triage', d)
    expect(run?.ok).toBe(true)
    expect(run?.output).toBe('ran:echo hi\nran:echo yo')
  })

  it('refuses an unknown subcommand with usage', async () => {
    const bad = await runStaticCybshLine('skill frobnicate', deps({}))
    expect(bad?.ok).toBe(false)
    expect(bad?.output).toContain('usage: skill')
  })
})

describe('task verb', () => {
  it('adds and reports usage errors (node env: no localStorage)', async () => {
    const d = deps({})
    const added = await runStaticCybshLine('task add Buy milk', d)
    expect(added?.ok).toBe(true)
    expect(added?.output).toMatch(/^task: added /)
    const blank = await runStaticCybshLine('task add', d)
    expect(blank?.ok).toBe(false)
    expect(blank?.output).toContain('usage: task add')
    const ghost = await runStaticCybshLine('task done abc', d)
    expect(ghost?.ok).toBe(false)
    expect(ghost?.output).toMatch(/^not_found:/)
  })
})
