// CyberManju OS — heuristic face detection v2 for the browser (no ONNX).
//
// TypeScript twin of `crates/faces/src/heuristic.rs`: white-patch lighting
// compensation (Hsu), luma-adaptive skin bounds (Chai–Ngan box mid-luma,
// wider rect toward the extremes), 3×3 morphological opening, geometry
// filters, then eye/mouth verification (eye pair, or one eye plus a Hsu
// mouth-map mouth) before anything counts as a face. Same `heuristic-v2`
// engine label, same 512-d crop embedding layout, same confidence shape.
//
// Integer discipline: every division that is integer math in Rust uses
// `Math.trunc` here, and every constant matches — the twins decide alike.
// Browser-free core: detection runs over a plain `{ data, width, height }`
// RGBA buffer so vitest drives it with synthetic pixels (node has no
// canvas). Image *decoding* (`decodeImageBytes` via `createImageBitmap`) is
// the only browser-only part and stays out of the tested core.

export const FACE_HEURISTIC_ENGINE = 'heuristic-v2'
export const FACE_EMBEDDING_DIM = 512

const WORKING_LONG_SIDE = 160
const MIN_BLOB_PIXELS = 24
const MIN_AREA_FRAC = 0.005
const MAX_AREA_FRAC = 0.5
const MIN_ASPECT = 0.5
const MAX_ASPECT = 1.8
const MIN_FILL_RATIO = 0.35
const MAX_FACES = 8
const CROP_MARGIN_FRAC = 0.1
const EMBED_GRID = 16

const MID_LUMA_LO = 80
const MID_LUMA_HI = 200
const ABS_LUMA_MIN = 30

const MIN_WHITE_PIXELS = 24
const GAIN_LO = 128
const GAIN_HI = 512

const EYE_DARK_NUM = 45
const EYE_DARK_DEN = 100
const EYE_DARK_FLOOR = 25
const EYE_MIN_AREA_FRAC = 0.002
const EYE_MAX_AREA_FRAC = 0.06
const EYE_TOP_NUM = 7
const EYE_TOP_DEN = 10
const PAIR_DY_MAX = 0.25
const PAIR_GAP_MIN = 0.06
const PAIR_SPAN_MAX = 0.9
const PAIR_MID_TOL = 0.22
const EYE_MAX_W_FRAC = 0.5
const EYE_MIN_SIDE = 2

const MOUTH_RESP_NUM = 45
const MOUTH_RESP_DEN = 100
const MOUTH_ABS_FLOOR = 20
const MOUTH_TOP_NUM = 45
const MOUTH_TOP_DEN = 100
const MOUTH_MIN_AREA_FRAC = 0.003
const MOUTH_MAX_AREA_FRAC = 0.1
const MOUTH_W_OVER_H = 1.4
const MOUTH_MID_TOL = 0.25

/** Raw RGBA pixels, row-major (canvas `ImageData` shape, alpha ignored). */
export interface RgbaFrame {
  data: Uint8Array | Uint8ClampedArray
  width: number
  height: number
}

export interface HeuristicFaceHit {
  /** Normalized bbox `[x, y, w, h]`, each 0..1. */
  bbox: [number, number, number, number]
  /** 0..1, approximate by design. */
  confidence: number
  /** 512-d L2-normalized, derived from the face crop. */
  embedding: number[]
  engine: typeof FACE_HEURISTIC_ENGINE
}

interface Blob {
  minX: number
  minY: number
  maxX: number
  maxY: number
  pixels: number
}

function trunc(n: number): number {
  // Mirror of Rust integer division (truncation toward zero).
  return Math.trunc(n)
}

function lumaOf(r: number, g: number, b: number): number {
  return trunc((299 * r + 587 * g + 114 * b) / 1000)
}

function chromaOf(r: number, g: number, b: number, luma: number): { cr: number; cb: number } {
  return {
    cr: trunc((713 * (r - luma)) / 1000) + 128,
    cb: trunc((564 * (b - luma)) / 1000) + 128,
  }
}

/** Working RGB grid (box-sampled) plus per-cell luma. */
function sampleGrid(frame: RgbaFrame, gw: number, gh: number): { grid: Array<[number, number, number]>; lumas: number[] } {
  const { data, width, height } = frame
  const grid: Array<[number, number, number]> = new Array(gw * gh)
  const lumas = new Array<number>(gw * gh).fill(0)
  for (let gy = 0; gy < gh; gy++) {
    const y0 = trunc((gy * height) / gh)
    const y1 = Math.max(y0 + 1, trunc(((gy + 1) * height) / gh))
    for (let gx = 0; gx < gw; gx++) {
      const x0 = trunc((gx * width) / gw)
      const x1 = Math.max(x0 + 1, trunc(((gx + 1) * width) / gw))
      let sr = 0
      let sg = 0
      let sb = 0
      let n = 0
      for (let y = y0; y < Math.min(y1, height); y++) {
        for (let x = x0; x < Math.min(x1, width); x++) {
          const i = (y * width + x) * 4
          sr += data[i] ?? 0
          sg += data[i + 1] ?? 0
          sb += data[i + 2] ?? 0
          n++
        }
      }
      const idx = gy * gw + gx
      if (n === 0) {
        grid[idx] = [0, 0, 0]
        continue
      }
      const r = trunc(sr / n)
      const g = trunc(sg / n)
      const b = trunc(sb / n)
      grid[idx] = [r, g, b]
      lumas[idx] = lumaOf(r, g, b)
    }
  }
  return { grid, lumas }
}

/** White-patch lighting compensation (fixed-point gains ×256). */
function compensateWhitePatch(grid: Array<[number, number, number]>, lumas: number[]): Array<[number, number, number]> {
  const sorted = [...lumas].sort((a, b) => a - b)
  if (sorted.length === 0) return grid
  const cutoff = sorted[trunc((95 * sorted.length) / 100)] ?? 0
  let sr = 0
  let sg = 0
  let sb = 0
  let n = 0
  for (let i = 0; i < grid.length; i++) {
    if ((lumas[i] ?? 0) >= cutoff) {
      const px = grid[i] as [number, number, number]
      sr += px[0]
      sg += px[1]
      sb += px[2]
      n++
    }
  }
  if (n < MIN_WHITE_PIXELS) return grid
  const means = [trunc(sr / n), trunc(sg / n), trunc(sb / n)]
  if (means.some((m) => m === 0)) return grid
  const gains = means.map((m) => trunc((255 * 256) / (m as number)))
  if (!gains.every((g) => (g ?? 0) >= GAIN_LO && (g ?? 0) <= GAIN_HI)) return grid
  return grid.map(([r, g, b]) => [
    Math.min(255, trunc((r * (gains[0] ?? 256)) / 256)),
    Math.min(255, trunc((g * (gains[1] ?? 256)) / 256)),
    Math.min(255, trunc((b * (gains[2] ?? 256)) / 256)),
  ])
}

/** Luma-adaptive skin test: tight box mid-luma, wider rect at extremes. */
function isSkin(r: number, g: number, b: number, luma: number): boolean {
  if (luma < ABS_LUMA_MIN) return false
  const { cr, cb } = chromaOf(r, g, b, luma)
  if (luma >= MID_LUMA_LO && luma <= MID_LUMA_HI) {
    return cr >= 133 && cr <= 173 && cb >= 77 && cb <= 127
  }
  return cr >= 130 && cr <= 180 && cb >= 75 && cb <= 135
}

/** 3×3 opening: erosion (≥5 of 9) then dilation (any of 9). */
function morphOpen(mask: boolean[], gw: number, gh: number): boolean[] {
  const at = (m: boolean[], x: number, y: number): boolean => {
    if (x < 0 || y < 0 || x >= gw || y >= gh) return false
    return m[y * gw + x] ?? false
  }
  const eroded = new Array<boolean>(mask.length).fill(false)
  for (let y = 0; y < gh; y++) {
    for (let x = 0; x < gw; x++) {
      let n = 0
      for (let dy = -1; dy <= 1; dy++) {
        for (let dx = -1; dx <= 1; dx++) {
          if (at(mask, x + dx, y + dy)) n++
        }
      }
      eroded[y * gw + x] = n >= 5
    }
  }
  const out = new Array<boolean>(mask.length).fill(false)
  for (let y = 0; y < gh; y++) {
    for (let x = 0; x < gw; x++) {
      let any = false
      for (let dy = -1; dy <= 1 && !any; dy++) {
        for (let dx = -1; dx <= 1; dx++) {
          if (at(eroded, x + dx, y + dy)) {
            any = true
            break
          }
        }
      }
      out[y * gw + x] = any
    }
  }
  return out
}

/** 4-connected components over a sub-rectangle (coords global to mask). */
function connectedBlobsStrided(
  mask: boolean[],
  stride: number,
  ox: number,
  oy: number,
  w: number,
  h: number,
): Blob[] {
  const seen = new Array<boolean>(mask.length).fill(false)
  const blobs: Blob[] = []
  for (let yy = oy; yy < oy + h; yy++) {
    for (let xx = ox; xx < ox + w; xx++) {
      const start = yy * stride + xx
      if (start >= mask.length || !mask[start] || seen[start]) continue
      const stack = [start]
      seen[start] = true
      const blob: Blob = { minX: Infinity, minY: Infinity, maxX: -1, maxY: -1, pixels: 0 }
      while (stack.length > 0) {
        const idx = stack.pop() as number
        const x = idx % stride
        const y = trunc(idx / stride)
        if (x < blob.minX) blob.minX = x
        if (y < blob.minY) blob.minY = y
        if (x > blob.maxX) blob.maxX = x
        if (y > blob.maxY) blob.maxY = y
        blob.pixels++
        const push = (nn: number) => {
          if (nn >= 0 && nn < mask.length && mask[nn] && !seen[nn]) {
            seen[nn] = true
            stack.push(nn)
          }
        }
        if (x > ox) push(idx - 1)
        if (x + 1 < ox + w) push(idx + 1)
        if (y > oy) push(idx - stride)
        if (y + 1 < oy + h) push(idx + stride)
      }
      blobs.push(blob)
    }
  }
  return blobs
}

function connectedBlobs(mask: boolean[], gw: number, gh: number): Blob[] {
  return connectedBlobsStrided(mask, gw, 0, 0, gw, gh)
}

interface FaceFeatures {
  bonus: number
}

/** Simplified Hsu mouth response M = Cr²·(Cr−Cb)/255² (integer, 0..~255). */
function mouthResponse(r: number, g: number, b: number): number {
  const luma = lumaOf(r, g, b)
  const { cr, cb } = chromaOf(r, g, b, luma)
  const crc = Math.max(0, cr)
  const cbc = Math.max(0, cb)
  return trunc((trunc((crc * crc) / 255) * Math.max(0, crc - cbc)) / 255)
}

/** Eye/mouth verification: eye pair, or one eye plus a mouth. */
function verifyFeatures(
  grid: Array<[number, number, number]>,
  lumas: number[],
  gw: number,
  blob: Blob,
): FaceFeatures | null {
  const bw = blob.maxX - blob.minX + 1
  const bh = blob.maxY - blob.minY + 1
  const bboxArea = bw * bh
  let acc = 0
  let n = 0
  for (let y = blob.minY; y <= blob.maxY; y++) {
    for (let x = blob.minX; x <= blob.maxX; x++) {
      const i = y * gw + x
      if (i < lumas.length) {
        acc += lumas[i] ?? 0
        n++
      }
    }
  }
  if (n === 0) return null
  const mean = trunc(acc / n)
  const darkAt = Math.max(EYE_DARK_FLOOR, trunc((mean * EYE_DARK_NUM) / EYE_DARK_DEN))

  const eyeMask = new Array<boolean>(lumas.length).fill(false)
  for (let y = blob.minY; y <= blob.maxY; y++) {
    for (let x = blob.minX; x <= blob.maxX; x++) {
      const i = y * gw + x
      if (i < lumas.length && (lumas[i] ?? 255) < darkAt) eyeMask[i] = true
    }
  }
  const eyeTop = blob.minY + trunc((bh * EYE_TOP_NUM) / EYE_TOP_DEN)
  const eyes = connectedBlobsStrided(eyeMask, gw, blob.minX, blob.minY, bw, bh)
    .filter((b) => {
      const areaFrac = b.pixels / bboxArea
      return (
        b.maxY < eyeTop &&
        areaFrac >= EYE_MIN_AREA_FRAC &&
        areaFrac <= EYE_MAX_AREA_FRAC &&
        b.maxX - b.minX + 1 >= EYE_MIN_SIDE &&
        b.maxY - b.minY + 1 >= EYE_MIN_SIDE &&
        b.maxX - b.minX + 1 <= EYE_MAX_W_FRAC * bw
      )
    })
    .sort((a, b) => b.pixels - a.pixels)

  let paired = false
  for (let i = 0; i < eyes.length && !paired; i++) {
    for (let j = i + 1; j < eyes.length; j++) {
      const a = eyes[i] as Blob
      const b = eyes[j] as Blob
      const l = a.minX <= b.minX ? a : b
      const r = a.minX <= b.minX ? b : a
      const cyL = (l.minY + l.maxY) / 2
      const cyR = (r.minY + r.maxY) / 2
      if (Math.abs(cyL - cyR) > PAIR_DY_MAX * bh) continue
      if (r.minX - l.maxX < PAIR_GAP_MIN * bw) continue
      if (r.maxX - l.minX > PAIR_SPAN_MAX * bw) continue
      const mid = (l.minX + r.maxX) / 2
      const axis = blob.minX + bw / 2
      if (Math.abs(mid - axis) > PAIR_MID_TOL * bw) continue
      paired = true
      break
    }
  }

  const mouthY0 = blob.minY + trunc((bh * MOUTH_TOP_NUM) / MOUTH_TOP_DEN)
  let maxM = 0
  for (let y = mouthY0; y <= blob.maxY; y++) {
    for (let x = blob.minX; x <= blob.maxX; x++) {
      const i = y * gw + x
      if (i >= grid.length) continue
      const [r, g, b] = grid[i] as [number, number, number]
      const m = mouthResponse(r, g, b)
      if (m > maxM) maxM = m
    }
  }
  let hasMouth = false
  if (maxM > MOUTH_ABS_FLOOR) {
    const thresh = Math.max(trunc((maxM * MOUTH_RESP_NUM) / MOUTH_RESP_DEN), MOUTH_ABS_FLOOR)
    const mouthMask = new Array<boolean>(lumas.length).fill(false)
    for (let y = mouthY0; y <= blob.maxY; y++) {
      for (let x = blob.minX; x <= blob.maxX; x++) {
        const i = y * gw + x
        if (i >= grid.length) continue
        const [r, g, b] = grid[i] as [number, number, number]
        if (mouthResponse(r, g, b) >= thresh) mouthMask[i] = true
      }
    }
    const axis = blob.minX + bw / 2
    hasMouth = connectedBlobsStrided(mouthMask, gw, blob.minX, mouthY0, bw, blob.maxY - mouthY0 + 1).some(
      (b) => {
        const areaFrac = b.pixels / bboxArea
        const wb = b.maxX - b.minX + 1
        const hb = Math.max(1, b.maxY - b.minY + 1)
        const mid = (b.minX + b.maxX) / 2
        return (
          areaFrac >= MOUTH_MIN_AREA_FRAC &&
          areaFrac <= MOUTH_MAX_AREA_FRAC &&
          wb >= MOUTH_W_OVER_H * hb &&
          Math.abs(mid - axis) <= MOUTH_MID_TOL * bw
        )
      },
    )
  }

  if (paired) return { bonus: hasMouth ? 0.25 : 0.15 }
  if (eyes.length > 0 && hasMouth) return { bonus: 0.1 }
  return null
}

function pixelAt(frame: RgbaFrame, x: number, y: number): [number, number, number] {
  const sx = Math.max(0, Math.min(frame.width - 1, x))
  const sy = Math.max(0, Math.min(frame.height - 1, y))
  const i = (sy * frame.width + sx) * 4
  return [frame.data[i] ?? 0, frame.data[i + 1] ?? 0, frame.data[i + 2] ?? 0]
}

function grayOf(r: number, g: number, b: number): number {
  return ((299 * r + 587 * g + 114 * b) / 1000 / 255)
}

/** 512-d crop embedding: 16×16 gray + 16×16 local contrast, L2-normalized. */
function cropEmbedding(frame: RgbaFrame, x: number, y: number, w: number, h: number): number[] {
  const mx = Math.floor(w * CROP_MARGIN_FRAC * frame.width)
  const my = Math.floor(h * CROP_MARGIN_FRAC * frame.height)
  const x0 = Math.max(0, Math.floor(x * frame.width) - mx)
  const y0 = Math.max(0, Math.floor(y * frame.height) - my)
  const x1 = Math.min(frame.width, Math.ceil((x + w) * frame.width) + mx)
  const y1 = Math.min(frame.height, Math.ceil((y + h) * frame.height) + my)
  const cw = Math.max(1, x1 - x0)
  const ch = Math.max(1, y1 - y0)
  const gray = new Array<number>(EMBED_GRID * EMBED_GRID).fill(0)
  for (let gy = 0; gy < EMBED_GRID; gy++) {
    for (let gx = 0; gx < EMBED_GRID; gx++) {
      const sx0 = Math.floor(x0 + (gx * cw) / EMBED_GRID)
      const sx1 = Math.min(frame.width, Math.ceil(x0 + ((gx + 1) * cw) / EMBED_GRID))
      const sy0 = Math.floor(y0 + (gy * ch) / EMBED_GRID)
      const sy1 = Math.min(frame.height, Math.ceil(y0 + ((gy + 1) * ch) / EMBED_GRID))
      let acc = 0
      let n = 0
      for (let sy = sy0; sy < sy1; sy++) {
        for (let sx = sx0; sx < sx1; sx++) {
          const [r, g, b] = pixelAt(frame, sx, sy)
          acc += grayOf(r, g, b)
          n++
        }
      }
      gray[gy * EMBED_GRID + gx] = n > 0 ? acc / n : 0
    }
  }
  const out = new Array<number>(FACE_EMBEDDING_DIM).fill(0)
  for (let gy = 0; gy < EMBED_GRID; gy++) {
    for (let gx = 0; gx < EMBED_GRID; gx++) {
      let acc = 0
      let n = 0
      for (let dy = -1; dy <= 1; dy++) {
        for (let dx = -1; dx <= 1; dx++) {
          const nx = gx + dx
          const ny = gy + dy
          if (nx >= 0 && nx < EMBED_GRID && ny >= 0 && ny < EMBED_GRID) {
            acc += gray[ny * EMBED_GRID + nx] ?? 0
            n++
          }
        }
      }
      const mean = n > 0 ? acc / n : 0
      const g = gray[gy * EMBED_GRID + gx] ?? 0
      out[gy * EMBED_GRID + gx] = g
      out[EMBED_GRID * EMBED_GRID + gy * EMBED_GRID + gx] = Math.abs(g - mean)
    }
  }
  const norm = Math.sqrt(out.reduce((s, v) => s + v * v, 0)) || 1e-6
  return out.map((v) => v / norm)
}

/**
 * Detect faces in raw RGBA pixels. Empty array for bad input or no
 * skin-plus-eyes signal — never an error, never a guess.
 */
export function detectFacesHeuristic(frame: RgbaFrame): HeuristicFaceHit[] {
  const { width, height } = frame
  if (!width || !height || frame.data.length < width * height * 4) return []
  const longest = Math.max(width, height)
  const scale = longest > WORKING_LONG_SIDE ? WORKING_LONG_SIDE / longest : 1
  const gw = Math.max(1, Math.round(width * scale))
  const gh = Math.max(1, Math.round(height * scale))
  const { grid: rawGrid, lumas } = sampleGrid(frame, gw, gh)
  const grid = compensateWhitePatch(rawGrid, lumas)
  const skin = grid.map((px, i) => isSkin(px[0], px[1], px[2], lumas[i] ?? 0))
  const mask = morphOpen(skin, gw, gh)
  const framePixels = gw * gh
  const blobs = connectedBlobs(mask, gw, gh).sort((a, b) => b.pixels - a.pixels)
  const out: HeuristicFaceHit[] = []
  for (const blob of blobs) {
    if (blob.pixels < MIN_BLOB_PIXELS) continue
    const bw = blob.maxX - blob.minX + 1
    const bh = blob.maxY - blob.minY + 1
    const areaFrac = blob.pixels / framePixels
    if (areaFrac < MIN_AREA_FRAC || areaFrac > MAX_AREA_FRAC) continue
    const aspect = bw / Math.max(1, bh)
    if (aspect < MIN_ASPECT || aspect > MAX_ASPECT) continue
    const fill = blob.pixels / Math.max(1, bw * bh)
    if (fill < MIN_FILL_RATIO) continue
    const x = blob.minX / gw
    const y = blob.minY / gh
    const w = bw / gw
    const h = bh / gh
    const features = verifyFeatures(grid, lumas, gw, blob)
    if (!features) continue
    const sizePrior = Math.min(1, Math.max(0, areaFrac / 0.08))
    const confidence = Math.min(0.95, Math.max(0, 0.3 + 0.5 * fill * (0.4 + 0.6 * sizePrior) + features.bonus))
    out.push({
      bbox: [x, y, w, h],
      confidence,
      embedding: cropEmbedding(frame, x, y, w, h),
      engine: FACE_HEURISTIC_ENGINE,
    })
  }
  out.sort((a, b) => b.confidence - a.confidence)
  return out.slice(0, MAX_FACES)
}

/** Cosine distance between same-length vectors (grouping metric). */
export function embeddingDistance(a: number[], b: number[]): number {
  const n = Math.min(a.length, b.length)
  let dot = 0
  let na = 0
  let nb = 0
  for (let i = 0; i < n; i++) {
    const x = a[i] ?? 0
    const y = b[i] ?? 0
    dot += x * y
    na += x * x
    nb += y * y
  }
  const denom = Math.sqrt(na) * Math.sqrt(nb)
  if (denom === 0) return 1
  return 1 - dot / denom
}

export interface FaceGroupDraft {
  id: string
  name: string
  fileIds: string[]
  centroidEmbedding: number[]
  cohesion: number
  embeddingCount: number
  detectionEngine: string
}

function newGroupId(): string {
  return `face-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

/**
 * Cosine-threshold assignment mirroring the native `detect_faces` command:
 * each embedding joins the nearest group under threshold (tightened as
 * groups grow), else founds `Person <id>`. Returns the touched groups.
 */
export function assignFaceGroups(
  embeddings: number[][],
  fileId: string,
  groups: FaceGroupDraft[],
  matchThreshold = 0.55,
): { groups: FaceGroupDraft[]; groupIds: string[]; engine: typeof FACE_HEURISTIC_ENGINE } {
  const touched: FaceGroupDraft[] = groups.map((g) => ({ ...g, fileIds: [...g.fileIds] }))
  const groupIds: string[] = []
  for (const emb of embeddings) {
    let best = -1
    let bestDist = Infinity
    for (let i = 0; i < touched.length; i++) {
      const g = touched[i] as FaceGroupDraft
      const threshold = matchThreshold - Math.min(0.1, g.fileIds.length * 0.005)
      const dist = embeddingDistance(emb, g.centroidEmbedding)
      if (dist < threshold && dist < bestDist) {
        best = i
        bestDist = dist
      }
    }
    if (best >= 0) {
      const g = touched[best] as FaceGroupDraft
      if (!g.fileIds.includes(fileId)) g.fileIds.push(fileId)
      // Incremental centroid drift toward the newcomer (matches native).
      g.centroidEmbedding = g.centroidEmbedding.map((v, i) => (v + (emb[i] ?? 0)) / 2)
      if (g.detectionEngine !== FACE_HEURISTIC_ENGINE) g.detectionEngine = 'mixed'
      g.embeddingCount++
      groupIds.push(g.id)
    } else {
      const id = newGroupId()
      touched.push({
        id,
        name: `Person ${id.slice(-4).toUpperCase()}`,
        fileIds: [fileId],
        centroidEmbedding: [...emb],
        cohesion: 0,
        embeddingCount: 1,
        detectionEngine: FACE_HEURISTIC_ENGINE,
      })
      groupIds.push(id)
    }
  }
  return { groups: touched, groupIds, engine: FACE_HEURISTIC_ENGINE }
}

// ── browser-only decoding (never imported by tests) ─────────────────────

/**
 * Decode image bytes to RGBA via the platform decoder (`createImageBitmap`
 * + `OffscreenCanvas` — every modern browser, incl. workers). Returns `null`
 * for non-images or undecodable bytes: silence, not a guess.
 */
export async function decodeImageBytes(bytes: Uint8Array): Promise<RgbaFrame | null> {
  try {
    if (typeof createImageBitmap === 'undefined' || typeof OffscreenCanvas === 'undefined') return null
    const blob = new Blob([bytes as unknown as BlobPart], { type: 'application/octet-stream' })
    const bitmap = await createImageBitmap(blob)
    if (!bitmap.width || !bitmap.height) {
      bitmap.close()
      return null
    }
    // Cap the working size (perf) — geometry is normalized, so this is safe.
    const scale = Math.min(1, 640 / Math.max(bitmap.width, bitmap.height))
    const w = Math.max(1, Math.round(bitmap.width * scale))
    const h = Math.max(1, Math.round(bitmap.height * scale))
    const canvas = new OffscreenCanvas(w, h)
    const ctx = canvas.getContext('2d')
    if (!ctx) {
      bitmap.close()
      return null
    }
    ctx.drawImage(bitmap, 0, 0, w, h)
    bitmap.close()
    const pixels = ctx.getImageData(0, 0, w, h)
    return { data: pixels.data, width: w, height: h }
  } catch {
    return null
  }
}
