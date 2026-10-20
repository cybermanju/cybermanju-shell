// CyberManju OS — Face heuristic.test (tests/frontend/face-heuristic.test.ts).
// Heuristic face detection v2 (TS twin of `crates/faces/src/heuristic.rs`)
// — pure over synthetic RGBA pixels (node has no canvas): a skin rect with
// eyes + mouth is found and labeled, a featureless blob (hand/wall) is
// rejected, a lone eye needs its mouth, dark skin passes via the wide
// rect, geometry is sane, embeddings are 512-d unit vectors, and grouping
// joins/founds/mixes like the native cosine-threshold command.
import { describe, expect, it } from 'vitest'
import {
  assignFaceGroups,
  detectFacesHeuristic,
  embeddingDistance,
  FACE_EMBEDDING_DIM,
  FACE_HEURISTIC_ENGINE,
  type RgbaFrame,
} from '../../src/utils/faceHeuristic'

const SKIN: [number, number, number] = [200, 150, 120]
const DARK_SKIN: [number, number, number] = [50, 30, 20]
const BLUE: [number, number, number] = [40, 80, 220]
const EYE: [number, number, number] = [25, 25, 25]
const DARK_EYE: [number, number, number] = [12, 12, 12]
const MOUTH: [number, number, number] = [150, 60, 60]
const DARK_MOUTH: [number, number, number] = [90, 25, 20]

function frame(w: number, h: number, fill: [number, number, number]): RgbaFrame {
  const data = new Uint8ClampedArray(w * h * 4)
  for (let i = 0; i < w * h; i++) {
    data[i * 4] = fill[0]
    data[i * 4 + 1] = fill[1]
    data[i * 4 + 2] = fill[2]
    data[i * 4 + 3] = 255
  }
  return { data, width: w, height: h }
}

function paint(f: RgbaFrame, x0: number, y0: number, x1: number, y1: number, px: [number, number, number]): void {
  for (let y = y0; y < y1; y++) {
    for (let x = x0; x < x1; x++) {
      const i = (y * f.width + x) * 4
      f.data[i] = px[0]
      f.data[i + 1] = px[1]
      f.data[i + 2] = px[2]
    }
  }
}

/** 200×200, blue background, skin rect with two dark eyes + red mouth. */
function faceFrame(): RgbaFrame {
  const f = frame(200, 200, BLUE)
  paint(f, 70, 50, 130, 140, SKIN)
  paint(f, 82, 78, 94, 88, EYE)
  paint(f, 106, 78, 118, 88, EYE)
  paint(f, 88, 115, 112, 125, MOUTH)
  return f
}

describe('detectFacesHeuristic', () => {
  it('finds one labeled face with eyes and mouth', () => {
    const hits = detectFacesHeuristic(faceFrame())
    expect(hits).toHaveLength(1)
    const [hit] = hits as [{ bbox: number[]; confidence: number; embedding: number[]; engine: string }]
    expect(hit.engine).toBe(FACE_HEURISTIC_ENGINE)
    expect(FACE_HEURISTIC_ENGINE).toBe('heuristic-v2')
    expect(hit.confidence).toBeGreaterThan(0.5)
    expect(hit.confidence).toBeLessThanOrEqual(0.95)
    expect(hit.bbox[0]).toBeGreaterThan(0.2)
    expect(hit.bbox[0]).toBeLessThan(0.45)
    expect(hit.embedding).toHaveLength(FACE_EMBEDDING_DIM)
    const norm = Math.sqrt(hit.embedding.reduce((s, v) => s + v * v, 0))
    expect(norm).toBeCloseTo(1, 4)
  })

  it('rejects a featureless skin blob (hand / wall / arm)', () => {
    const f = frame(200, 200, BLUE)
    paint(f, 70, 50, 130, 140, SKIN)
    expect(detectFacesHeuristic(f)).toEqual([])
  })

  it('accepts one eye plus a mouth', () => {
    const f = faceFrame()
    paint(f, 106, 78, 118, 88, SKIN)
    expect(detectFacesHeuristic(f)).toHaveLength(1)
  })

  it('rejects a lone eye with no mouth', () => {
    const f = faceFrame()
    paint(f, 106, 78, 118, 88, SKIN)
    paint(f, 88, 115, 112, 125, SKIN)
    expect(detectFacesHeuristic(f)).toEqual([])
  })

  it('finds dark skin via the wide rect', () => {
    const f = frame(200, 200, BLUE)
    paint(f, 70, 50, 130, 140, DARK_SKIN)
    paint(f, 82, 78, 94, 88, DARK_EYE)
    paint(f, 106, 78, 118, 88, DARK_EYE)
    paint(f, 88, 115, 112, 125, DARK_MOUTH)
    expect(detectFacesHeuristic(f)).toHaveLength(1)
  })

  it('stays silent on pure blue and full-frame skin (wall)', () => {
    expect(detectFacesHeuristic(frame(160, 120, BLUE))).toEqual([])
    expect(detectFacesHeuristic(frame(120, 120, SKIN))).toEqual([])
  })

  it('rejects bad input without throwing', () => {
    expect(detectFacesHeuristic({ data: new Uint8ClampedArray(0), width: 0, height: 0 })).toEqual([])
    expect(detectFacesHeuristic({ data: new Uint8ClampedArray(10), width: 10, height: 10 })).toEqual([])
  })

  it('is deterministic', () => {
    const f = faceFrame()
    expect(detectFacesHeuristic(f)).toEqual(detectFacesHeuristic(f))
  })
})

describe('embeddingDistance + assignFaceGroups', () => {
  it('measures 0 for identical vectors, 1 for orthogonal', () => {
    const v = [0.6, 0.8, 0, 0]
    expect(embeddingDistance(v, v)).toBeCloseTo(0, 6)
    expect(embeddingDistance([1, 0], [0, 1])).toBeCloseTo(1, 6)
  })

  it('founds a group, then joins the same face', () => {
    const emb = detectFacesHeuristic(faceFrame())[0]?.embedding as number[]
    const first = assignFaceGroups([emb], 'file-a', [])
    expect(first.groups).toHaveLength(1)
    expect(first.groupIds).toHaveLength(1)
    expect(first.engine).toBe(FACE_HEURISTIC_ENGINE)
    const gid = first.groupIds[0] as string
    const second = assignFaceGroups([emb], 'file-b', first.groups)
    expect(second.groups).toHaveLength(1)
    expect(second.groupIds).toEqual([gid])
    expect(second.groups[0]?.fileIds).toEqual(['file-a', 'file-b'])
  })

  it('founds a second group for a far-apart embedding', () => {
    const emb = new Array<number>(FACE_EMBEDDING_DIM).fill(0)
    emb[0] = 1
    const other = new Array<number>(FACE_EMBEDDING_DIM).fill(0)
    other[FACE_EMBEDDING_DIM - 1] = 1
    const first = assignFaceGroups([emb], 'file-a', [])
    const second = assignFaceGroups([other], 'file-b', first.groups)
    expect(second.groups).toHaveLength(2)
  })
})
