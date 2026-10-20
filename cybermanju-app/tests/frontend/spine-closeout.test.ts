// CyberManju OS — Spine close-out.test (tests/frontend/spine-closeout.test.ts).
// ANALYSIS.md final close-out: op-log resolver data (§2C), direct LAN sync
// plan + peer-auth gate (§2B.2 follow-up), skill panels (§4.8 panels-later),
// notes auto-index bridge (§4.2), key-rotation checklist (§3), and the
// `oplog`/`lan`/`skill panel` cybsh verbs. All pure + node-env fakes.
import { describe, expect, it } from 'vitest'
import {
  conflictSummary,
  conflictTitle,
  isSameFile,
  normalizeConflicts,
  resolutionPayload,
  resolveLabel,
  syncPlanSteps,
} from '../../src/utils/oplog'
import {
  describePeer,
  isPeerSyncable,
  lanSyncSteps,
  peerBaseUrl,
  peerBlockReason,
} from '../../src/utils/lanSync'
import { rotationChecklist, rotationVerifySummary } from '../../src/utils/keyRotation'
import { noteToMemory } from '../../src/utils/notes'
import { parseSkillMd, skillPanels } from '../../src/utils/skills'
import { isSkillPanel, skillSlugForPanel } from '../../src/utils/panels'
import {
  handlesStaticVerb,
  runStaticCybshLine,
  type StaticCybshDeps,
} from '../../src/utils/staticCybsh'

function deps(volume: Record<string, string> = {}, fallback = true): StaticCybshDeps {
  return {
    readVolume: () => volume,
    writeVolumeFile: async (path, content) => {
      volume[path] = content
    },
    ...(fallback ? { execFallback: async (line: string) => `ran:${line}` } : {}),
  } as unknown as StaticCybshDeps
}

const PAIR = {
  a: { seq: 3, opId: 'laptop-3', deviceId: 'laptop', createdAt: '', kind: 'rename', fileId: 'f9' },
  b: { seq: 7, opId: 'phone-7', deviceId: 'phone', createdAt: '', kind: 'rename', fileId: 'f9' },
}

describe('op-log resolver data', () => {
  it('titles and summarizes fork pairs', () => {
    expect(conflictTitle(PAIR)).toBe('rename × rename on f9')
    expect(conflictSummary(PAIR)).toContain('laptop rename (seq 3)')
    expect(conflictSummary(PAIR)).toContain('phone rename (seq 7)')
    expect(isSameFile(PAIR)).toBe(true)
  })

  it('flags cross-file pairs as not same-file', () => {
    const cross = { ...PAIR, b: { ...PAIR.b, fileId: 'other' } }
    expect(isSameFile(cross)).toBe(false)
    expect(conflictTitle(cross)).toContain('rename × rename')
  })

  it('builds keep/merge payloads (unknown choice → merge)', () => {
    expect(resolutionPayload(PAIR, 'keep-a', 'laptop')).toMatchObject({
      opA: 'laptop-3',
      opB: 'phone-7',
      choice: 'keep-a',
      deviceId: 'laptop',
      fileId: 'f9',
    })
    expect(resolutionPayload(PAIR, 'bogus' as never, 'laptop').choice).toBe('merge')
    expect(resolveLabel('keep-b')).toBe('Keep second')
  })

  it('plans queue-and-replay pulls', () => {
    expect(syncPlanSteps(12)).toEqual([
      'pull: GET /api/oplog/since?since=12',
      'push: POST /api/oplog/push { deviceId, ops, signature, algorithm } (ML-DSA required)',
      'render: GET /api/oplog/conflicts',
    ])
  })

  it('normalizes {a,b} and [a,b] conflict shapes, drops junk', () => {
    const fromObjects = normalizeConflicts({ conflicts: [PAIR] })
    expect(fromObjects).toHaveLength(1)
    const fromArrays = normalizeConflicts({ conflicts: [[PAIR.a, PAIR.b]] })
    expect(fromArrays).toHaveLength(1)
    expect(normalizeConflicts({ conflicts: [{ a: 1 }, null, 'x'] })).toEqual([])
    expect(normalizeConflicts({})).toEqual([])
  })
})

describe('direct LAN sync', () => {
  const peer = { deviceId: 'phone-7', name: 'Phone', host: '192.168.1.60', port: 3456 }

  it('builds explicit peer base URLs', () => {
    expect(peerBaseUrl(peer)).toBe('http://192.168.1.60:3456')
    expect(describePeer(peer)).toBe('Phone (192.168.1.60:3456)')
  })

  it('gates sync on enrolled, non-revoked devices (fail-closed)', () => {
    expect(isPeerSyncable(peer, ['phone-7'])).toBe(true)
    expect(isPeerSyncable(peer, [])).toBe(false)
    expect(isPeerSyncable({ ...peer, revoked: true }, ['phone-7'])).toBe(false)
    expect(isPeerSyncable({ ...peer, deviceId: ' ' }, ['phone-7'])).toBe(false)
    expect(peerBlockReason(peer, [])).toContain('enroll it via Devices')
    expect(peerBlockReason({ ...peer, revoked: true }, ['phone-7'])).toContain('revoked')
    expect(peerBlockReason(peer, ['phone-7'])).toBe('')
  })

  it('plans pull→push→merge→render steps', () => {
    const steps = lanSyncSteps(peer, 4)
    expect(steps[0]).toBe('pull: GET http://192.168.1.60:3456/api/oplog/since?since=4')
    expect(steps).toHaveLength(4)
    expect(steps[3]).toBe('render: GET /api/oplog/conflicts')
  })
})

describe('skill panels + panel addresses', () => {
  const SKILL_MD = `---
name: Photo Triage
version: 1.2.0
description: Sort new photos fast
run: triage.cybsh
panel: search
tab: photos
---
# Photo Triage
`

  it('parses panel frontmatter', () => {
    const m = parseSkillMd('.cybermanju/skills/photo-triage/SKILL.md', SKILL_MD)!
    expect(m.panel).toBe('search')
    expect(m.tab).toBe('photos')
  })

  it('lists panel-shipping skills as skill: addresses', () => {
    const plain = parseSkillMd('.cybermanju/skills/plain/SKILL.md', '# Plain\nbody')!
    const triage = parseSkillMd('.cybermanju/skills/photo-triage/SKILL.md', SKILL_MD)!
    expect(skillPanels([plain, triage])).toEqual([
      { address: 'skill:photo-triage', slug: 'photo-triage', panel: 'search', tab: 'photos' },
    ])
  })

  it('recognizes skill: addresses', () => {
    expect(isSkillPanel('skill:photo-triage')).toBe(true)
    expect(isSkillPanel('search')).toBe(false)
    expect(skillSlugForPanel('skill:photo-triage')).toBe('photo-triage')
    expect(skillSlugForPanel('search')).toBe('')
  })
})

describe('notes auto-index + rotation checklist', () => {
  it('turns a saved note into a recall-budgeted memory row', () => {
    const mem = noteToMemory({ id: 'n1', name: 'Lisbon.md', contentText: '# Lisbon\nnotes here' }, 2)!
    expect(mem.text.startsWith('Lisbon.md')).toBe(true)
    expect(mem.tags).toEqual(['note', 'backlinks:2'])
    expect(noteToMemory({ id: 'n2', name: 'Empty.md', contentText: '   ' })).toBeNull()
  })

  it('caps memory text for the recall budget', () => {
    const mem = noteToMemory({ id: 'n', name: 'Big.md', contentText: `x`.repeat(5000) })!
    expect(mem.text.length).toBeLessThan(2200)
  })

  it('orders the rotation checklist with a verify summary', () => {
    const steps = rotationChecklist()
    expect(steps).toHaveLength(5)
    expect(steps[0].id).toBe('backup')
    expect(steps[4].id).toBe('verify')
    expect(rotationVerifySummary([])).toContain('0/5')
    expect(rotationVerifySummary(steps.map((s) => s.id))).toContain('complete')
  })
})

describe('oplog/lan/skill-panel verbs', () => {
  it('registers the verbs', () => {
    expect(handlesStaticVerb('oplog')).toBe(true)
    expect(handlesStaticVerb('lan')).toBe(true)
  })

  it('oplog plan works offline', async () => {
    const out = await runStaticCybshLine('oplog plan --since 9', deps({}, false))
    expect(out?.ok).toBe(true)
    expect(out?.output).toContain('since=9')
  })

  it('oplog resolve builds the payload locally', async () => {
    const out = await runStaticCybshLine('oplog resolve a-1 b-2 merge', deps({}, false))
    expect(out?.ok).toBe(true)
    expect(out?.output).toContain('POST /api/oplog/resolve')
  })

  it('oplog conflicts without a transport refuses honestly', async () => {
    const out = await runStaticCybshLine('oplog conflicts', deps({}, false))
    expect(out?.ok).toBe(false)
    expect(out?.output).toMatch(/^unsupported:/)
  })

  it('lan plan works offline; peers/sync need the dashboard', async () => {
    const plan = await runStaticCybshLine('lan plan 192.168.1.60:3456', deps({}, false))
    expect(plan?.ok).toBe(true)
    expect(plan?.output).toContain('http://192.168.1.60:3456')
    const peers = await runStaticCybshLine('lan peers', deps({}, false))
    expect(peers?.ok).toBe(false)
    expect(peers?.output).toMatch(/^unsupported:/)
  })

  it('skill panel resolves panel-shipping skills', async () => {
    const d = deps({
      '.cybermanju/skills/photo-triage/SKILL.md': `---
name: Photo Triage
description: Sort fast
panel: search
---
# Triage
`,
      '.cybermanju/skills/plain/SKILL.md': '# Plain\nbody',
    })
    const ok = await runStaticCybshLine('skill panel photo-triage', d)
    expect(ok?.ok).toBe(true)
    expect(ok?.output).toContain('skill:photo-triage')
    const missing = await runStaticCybshLine('skill panel plain', d)
    expect(missing?.ok).toBe(false)
    expect(missing?.output).toMatch(/^not_found:/)
  })
})
