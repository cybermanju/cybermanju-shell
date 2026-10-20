// Scene heuristic matcher — TypeScript twin contract.
//
// Same tables (`crates/scene/data/scenes.json`) and same algorithm as the
// Rust `cybermanju-scene` crate: multilingual filename/tag matching with
// confidence scores, child→parent inference, face-cluster human boost, and
// honest silence on garbage. Mirrors `crates/scene/src/tests.rs`.
import { describe, expect, it } from 'vitest'
import {
  classifyScene,
  matchSceneQuery,
  normScene,
  sceneCategoryList,
} from '../../src/utils/scene'

const top = (scores: { category: string }[]): string | undefined => scores[0]?.category
const scoreOf = (scores: { category: string; score: number }[], cat: string): number =>
  scores.find(s => s.category === cat)?.score ?? 0
const has = (scores: { category: string }[], cat: string): boolean =>
  scores.some(s => s.category === cat)

describe('scene tables', () => {
  it('exposes 14+ categories including the requested ones', () => {
    const ids = sceneCategoryList().map(([id]) => id)
    expect(ids.length).toBeGreaterThanOrEqual(14)
    for (const want of ['human', 'forest', 'beach', 'party', 'animal', 'dog', 'cat', 'city', 'snow']) {
      expect(ids).toContain(want)
    }
  })
})

describe('multilingual beach matching', () => {
  const cases = [
    'praia_grande.jpg',
    'playa_del_carmen.png',
    'plage_normandie.jpg',
    'ostsee_strand.heic',
    'spiaggia_roma.jpg',
    'scheveningen_strand.jpg',
    'пляж_сочи.jpg',
    '海滩度假.jpg',
    '沖縄ビーチ.jpg',
    'sandy_beach_day.jpg',
  ]
  for (const name of cases) {
    it(`tops beach for ${name}`, () => {
      const scores = classifyScene({ fileName: name })
      expect(top(scores)).toBe('beach')
      expect(scoreOf(scores, 'beach')).toBeGreaterThanOrEqual(0.5)
    })
  }
})

describe('forests, parties, animals, humans across languages', () => {
  const cases: [string, string][] = [
    ['floresta_amazonica.jpg', 'forest'],
    ['black_forest_trail.jpg', 'forest'],
    ['foret_de_fontainebleau.jpg', 'forest'],
    ['festa_junina.jpg', 'party'],
    ['birthday_party_2024.jpg', 'party'],
    ['結婚式_京都.jpg', 'wedding'],
    ['hochzeit_berlin.jpg', 'wedding'],
    ['meu_cachorro.jpg', 'dog'],
    ['gato_no_telhado.jpg', 'cat'],
    ['птица_в_парке.jpg', 'bird'],
    ['portrait_maria.jpg', 'human'],
    ['城市夜景.jpg', 'city'],
    ['schnee_winter.jpg', 'snow'],
  ]
  for (const [name, want] of cases) {
    it(`${name} reports ${want}`, () => {
      expect(has(classifyScene({ fileName: name }), want)).toBe(true)
    })
  }
})

describe('dynamic query matching', () => {
  const cases: [string, string][] = [
    ['praia', 'beach'],
    ['playa', 'beach'],
    ['plage', 'beach'],
    ['strand', 'beach'],
    ['festa', 'party'],
    ['cachorro', 'dog'],
    ['gato', 'cat'],
    ['floresta', 'forest'],
    ['neve', 'snow'],
    ['casamento', 'wedding'],
  ]
  for (const [q, want] of cases) {
    it(`query ${q} matches ${want}`, () => {
      expect(has(matchSceneQuery(q), want)).toBe(true)
    })
  }
})

describe('scoring behavior', () => {
  it('ranks more evidence higher', () => {
    const one = classifyScene({ fileName: 'beach.jpg' })
    const two = classifyScene({
      fileName: 'beach_party.jpg',
      tags: ['praia', 'sunset'],
    })
    expect(scoreOf(two, 'beach')).toBeGreaterThanOrEqual(scoreOf(one, 'beach'))
    expect(has(two, 'sunset')).toBe(true)
  })

  it('lets user tags carry a match alone (IMG_0042 + praia)', () => {
    const scores = classifyScene({ fileName: 'IMG_0042.jpg', tags: ['praia', 'família'] })
    expect(has(scores, 'beach')).toBe(true)
  })

  it('infers parents from children (dog implies animal, discounted)', () => {
    const scores = classifyScene({ fileName: 'meu_cachorro_rex.jpg' })
    expect(has(scores, 'dog')).toBe(true)
    expect(scoreOf(scores, 'animal')).toBeGreaterThan(0)
    expect(scoreOf(scores, 'animal')).toBeLessThan(scoreOf(scores, 'dog'))
  })

  it('boosts human on face-cluster membership, stays silent otherwise', () => {
    expect(classifyScene({ fileName: 'IMG_0001.jpg' })).toEqual([])
    expect(top(classifyScene({ fileName: 'IMG_0001.jpg', hasFaces: true }))).toBe('human')
  })

  it('stays silent on empty and hash-named garbage', () => {
    for (const name of ['', 'IMG_0042.jpg', 'DSC_9918.heic', 'document.pdf', 'a.jpg']) {
      expect(classifyScene({ fileName: name })).toEqual([])
    }
  })

  it('carries matched keywords on every score', () => {
    const scores = classifyScene({ fileName: 'praia_grande.jpg' })
    expect(scores[0].hits.length).toBeGreaterThan(0)
  })
})

describe('normScene', () => {
  it('folds latin diacritics and lowercases', () => {
    expect(normScene('PÔR DO SOL')).toBe('por do sol')
    expect(normScene('пляЖ')).toBe('пляж')
  })
})

describe('english reachability', () => {
  it('fires every category from its own English keywords', async () => {
    const tables = (await import('../../crates/scene/data/scenes.json')).default as {
      categories: { id: string; keywords: Record<string, string[]> }[]
    }
    for (const cat of tables.categories) {
      const hit = cat.keywords.en.some(kw =>
        has(classifyScene({ fileName: `${kw}_photo.jpg` }), cat.id)
      )
      expect(hit).toBe(true)
    }
  })
})
