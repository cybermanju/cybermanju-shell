// CyberManju OS — wiki component-forms test (tests/frontend/wiki-subjects.test.ts).
// Role: pin the build-time form assignment — every module gets a distinct,
//   function-themed silhouette (never a bare cube/circle/sphere), a glyph,
//   and an honest reason, and every 3D key resolves in wiki-src/shapes.ts.
// Exports: SUBJECTS form coverage, uniqueness, shapes.ts resolution cases.
// Integrates: scripts/generate-wiki.mjs (SUBJECTS table + main-guard) and
//   wiki-src/shapes.ts (pure geometry, node-safe).
// Contracts: 16 subjects; shape keys unique; keys exist in SHAPE_KEYS; no
//   sphere/box/cube/circle; glyphs unique; each entry carries shapeWhy.
import { describe, expect, it } from 'vitest'
import { SUBJECTS } from '../../scripts/generate-wiki.mjs'
import { SHAPE_KEYS, shapeOf, shapeClip, KIND_SHAPE, subjectOfPath } from '../../wiki-src/shapes'

const BANNED = ['sphere', 'box', 'cube', 'circle']

describe('SUBJECTS form table', () => {
  it('covers all 16 subjects', () => {
    expect(SUBJECTS).toHaveLength(16)
  })

  it('assigns a shape, glyph and reason to every subject', () => {
    for (const s of SUBJECTS) {
      expect(typeof s.shape, `${s.id} shape`).toBe('string')
      expect(typeof s.glyph, `${s.id} glyph`).toBe('string')
      expect((s.shapeWhy ?? '').length, `${s.id} shapeWhy`).toBeGreaterThan(8)
    }
  })

  it('uses a unique silhouette per module', () => {
    const shapes = SUBJECTS.map((s) => s.shape)
    expect(new Set(shapes).size).toBe(SUBJECTS.length)
  })

  it('uses a unique glyph per module', () => {
    const glyphs = SUBJECTS.map((s) => s.glyph)
    expect(new Set(glyphs).size).toBe(SUBJECTS.length)
  })

  it('never falls back to a boring cube/sphere/circle', () => {
    for (const s of SUBJECTS) {
      expect(BANNED, `${s.id} uses ${s.shape}`).not.toContain(s.shape)
    }
  })

  it('resolves every shape key in the shapes.ts library', () => {
    for (const s of SUBJECTS) {
      expect(SHAPE_KEYS, `${s.id} → ${s.shape}`).toContain(s.shape)
      // shapeOf builds a real Object3D (not a bare null/undefined form).
      expect(shapeOf(s.shape)).toBeTruthy()
      // clip is a CSS polygon() string (the 2D silhouette mirror).
      expect(shapeClip(s.shape)).toMatch(/^polygon\(/)
    }
  })

  it('gives every subject file paths for cross-module chip jumps', () => {
    for (const s of SUBJECTS) {
      expect(Array.isArray(s.paths), `${s.id} paths`).toBe(true)
      expect((s.paths ?? []).length, `${s.id} paths`).toBeGreaterThan(0)
    }
  })
})

describe('shapes.ts library', () => {
  it('KIND_SHAPE covers the symbol kinds the structure finder emits', () => {
    for (const kind of ['function', 'class', 'module', 'interface', 'const', 'type', 'impl', 'macro']) {
      expect(KIND_SHAPE[kind], kind).toBeTruthy()
      expect(SHAPE_KEYS).toContain(KIND_SHAPE[kind])
    }
  })

  it('subjectOfPath matches prefixes the same way wiki-graph does', () => {
    const subs = [{ id: 'alpha', paths: ['src/alpha/'] }, { id: 'beta', paths: ['src/beta/'] }]
    expect(subjectOfPath('src/alpha/one.rs', subs)).toBe('alpha')
    expect(subjectOfPath('src/beta/two.rs', subs)).toBe('beta')
    expect(subjectOfPath('src/other/x.rs', subs)).toBe(null)
  })

  it('falls back to the overview crystal for unknown keys', () => {
    expect(shapeClip('definitely-not-a-shape')).toBe(shapeClip('facet'))
  })
})
