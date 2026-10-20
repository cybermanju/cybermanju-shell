// CyberManju OS — heuristic face detection v2 (no ONNX needed).
//
// Where the SCRFD+ArcFace ONNX stack is unavailable (mobile builds that must
// not link `ort`, desktops with no downloaded model, offline machines) this
// module is the labeled fallback: a classical skin-segmentation detector
// that reports what it actually saw, with a confidence per face and the
// engine string `heuristic-v2` so callers (and the UI) never confuse it with
// the ONNX path.
//
// Method (classical, literature-grounded — no learned weights anywhere):
//   1. working RGB grid (long side ≤ 160) by box sampling;
//   2. white-patch lighting compensation (Hsu et al., ICIP 2001): top-5%
//      luma pixels estimate the illuminant; per-channel gains pull them to
//      gray. Skipped unless every gain lands in [0.5, 2.0] — a uniform frame
//      has no white patch to estimate from;
//   3. luma-adaptive skin bounds: the Chai–Ngan box (Cb 77–127, Cr 133–173)
//      for mid luma, the wider three-Gaussian quick-reject rect (Cb 75–135,
//      Cr 130–180) toward dark/bright extremes — dark skin lives there;
//   4. morphological opening (3×3 erosion → dilation): speckles die, thin
//      face↔neck/hand bridges break, blobs stay blobs;
//   5. 4-connected components + geometry filters (area fraction, aspect,
//      fill ratio) — hands and walls usually die here;
//   6. eye/mouth verification per candidate (the step that tells a face
//      from a hand — Yang et al., ICIP 1998): luma valleys for eye
//      candidates with pair geometry (aligned, separated, symmetric about
//      the face axis — Campadelli et al., IST 2003), and a simplified Hsu
//      mouth map (M = Cr²·(Cr−Cb)) for the mouth. Accept on an eye pair,
//      or one eye plus a mouth. A uniform skin blob with no features is
//      rejected — that is the point, not a limitation;
//   7. per accepted face: normalized bbox + confidence (fill/size base,
//      feature bonus) + a 512-d embedding from the face crop (16×16 gray +
//      16×16 local contrast, L2-normalized), so same-shoot crops cluster
//      while different crops stay apart.
//
// Honesty rules (AUDIT F7 still holds):
//   - every detection derives from pixel content — nothing is seeded from
//     hashes or filenames;
//   - empty vec means no signal (bad input, no skin, no eyes) — silence is
//     silence, exactly like the ONNX path;
//   - embeddings are 512-d like the ONNX path (`EMBEDDING_DIM`) so both
//     engines share one grouping pipeline, and groups record which engine
//     filled them (`FaceGroup.detection_engine`);
//   - known limits (documented, not hidden): frontal-ish faces of readable
//     size (tiny faces need ONNX), one lighting compensation pass (harsh
//     mixed lighting still hurts), and cross-photo identity of the same
//     person (that needs learned invariance — ONNX).
//
// All arithmetic is integer until the final confidence/embedding step:
// deterministic across platforms, and the TypeScript twin
// (`src/utils/faceHeuristic.ts`) mirrors every constant and truncation.

use super::EMBEDDING_DIM;

/// Engine label recorded on groups filled by this module.
pub const HEURISTIC_ENGINE: &str = "heuristic-v2";

/// Long side of the working grid.
const WORKING_LONG_SIDE: u32 = 160;
/// Minimum blob size in working-grid pixels (kills speckles).
const MIN_BLOB_PIXELS: u32 = 24;
/// Blob area as a fraction of the frame: below is noise, above is a wall.
const MIN_AREA_FRAC: f32 = 0.005;
const MAX_AREA_FRAC: f32 = 0.50;
/// Face bbox aspect window (w/h).
const MIN_ASPECT: f32 = 0.5;
const MAX_ASPECT: f32 = 1.8;
/// Blob pixels inside their own bbox — faces are compact, limbs are not.
const MIN_FILL_RATIO: f32 = 0.35;
/// Never report more faces than this per image.
const MAX_FACES: usize = 8;
/// Crop margin around a blob before embedding (fraction of bbox size).
const CROP_MARGIN_FRAC: f32 = 0.10;
/// Embedding grid per side — 16×16 gray + 16×16 contrast = 512 dims.
const EMBED_GRID: usize = 16;

/// Luma split points for the adaptive skin bounds (Hsu-style zones).
const MID_LUMA_LO: u32 = 80;
const MID_LUMA_HI: u32 = 200;
const ABS_LUMA_MIN: u32 = 30;

/// White-patch: need this many bright pixels to trust the estimate.
const MIN_WHITE_PIXELS: usize = 24;
/// Gain clamp ×256: [0.5, 2.0] — outside means "no white patch", skip.
const GAIN_LO: u32 = 128;
const GAIN_HI: u32 = 512;

/// Eye evidence: luma below this fraction of the bbox mean (floor 25).
const EYE_DARK_NUM: u32 = 45;
const EYE_DARK_DEN: u32 = 100;
const EYE_DARK_FLOOR: u32 = 25;
/// Eye blob area as a fraction of the bbox: [0.2%, 6%].
const EYE_MIN_AREA_FRAC: f32 = 0.002;
const EYE_MAX_AREA_FRAC: f32 = 0.06;
/// Eye blobs must end above 70% of the bbox height.
const EYE_TOP_NUM: u32 = 7;
const EYE_TOP_DEN: u32 = 10;
/// Pair geometry (fractions of bbox w/h).
const PAIR_DY_MAX: f32 = 0.25;
const PAIR_GAP_MIN: f32 = 0.06;
const PAIR_SPAN_MAX: f32 = 0.90;
const PAIR_MID_TOL: f32 = 0.22;
const EYE_MAX_W_FRAC: f32 = 0.50;
const EYE_MIN_SIDE: u32 = 2;

/// Mouth: response above this fraction of the bbox max (and never below
/// the absolute floor — mid-luma skin itself answers ~18, lips answer 25+),
/// lower 55% only.
const MOUTH_RESP_NUM: u32 = 45;
const MOUTH_RESP_DEN: u32 = 100;
const MOUTH_ABS_FLOOR: u32 = 20;
const MOUTH_TOP_NUM: u32 = 45;
const MOUTH_TOP_DEN: u32 = 100;
/// Mouth blob area [0.3%, 10%], wider than tall, roughly centered.
const MOUTH_MIN_AREA_FRAC: f32 = 0.003;
const MOUTH_MAX_AREA_FRAC: f32 = 0.10;
const MOUTH_W_OVER_H: f32 = 1.4;
const MOUTH_MID_TOL: f32 = 0.25;

/// One heuristic detection: normalized bbox, confidence, content embedding.
#[derive(Debug, Clone)]
pub struct HeuristicFace {
    /// Normalized bbox `[x, y, w, h]`, each 0.0–1.0 in frame space.
    pub bbox: [f32; 4],
    /// 0.0–1.0 — geometry base plus feature bonus, approximate by design.
    pub confidence: f32,
    /// 512-d L2-normalized, derived from the face crop pixels.
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, Copy)]
struct Blob {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
    pixels: u32,
}

impl Blob {
    fn width(&self) -> u32 {
        self.max_x - self.min_x + 1
    }
    fn height(&self) -> u32 {
        self.max_y - self.min_y + 1
    }
}

/// Detect faces in raw RGB bytes (`R,G,B` triples, row-major).
///
/// Returns an empty vec for bad input (wrong length, zero dims) or no
/// skin-plus-eyes signal — never an error, never a guess.
pub fn detect_faces_heuristic_rgb(rgb: &[u8], width: u32, height: u32) -> Vec<HeuristicFace> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    if rgb.len() != (width as usize) * (height as usize) * 3 {
        return Vec::new();
    }
    // Working grid: scale down, never up.
    let scale = if width.max(height) > WORKING_LONG_SIDE {
        WORKING_LONG_SIDE as f32 / width.max(height) as f32
    } else {
        1.0
    };
    let gw = ((width as f32 * scale).round() as u32).max(1);
    let gh = ((height as f32 * scale).round() as u32).max(1);
    let (grid, lumas) = sample_grid(rgb, width, height, gw, gh);
    let grid = compensate_white_patch(grid, &lumas);
    let mask = skin_mask(&grid, &lumas);
    let mask = morph_open(&mask, gw, gh);
    let frame_pixels = (gw as f32) * (gh as f32);
    let blobs = connected_blobs(&mask, gw, gh);
    let mut out = Vec::new();
    for blob in blobs {
        if blob.pixels < MIN_BLOB_PIXELS {
            continue;
        }
        let bw = blob.width() as f32;
        let bh = blob.height() as f32;
        let area_frac = blob.pixels as f32 / frame_pixels;
        if !(MIN_AREA_FRAC..=MAX_AREA_FRAC).contains(&area_frac) {
            continue;
        }
        let aspect = bw / bh.max(1.0);
        if !(MIN_ASPECT..=MAX_ASPECT).contains(&aspect) {
            continue;
        }
        let fill = blob.pixels as f32 / (bw * bh).max(1.0);
        if fill < MIN_FILL_RATIO {
            continue;
        }
        // Back to normalized frame coordinates.
        let x = blob.min_x as f32 / gw as f32;
        let y = blob.min_y as f32 / gh as f32;
        let w = bw / gw as f32;
        let h = bh / gh as f32;
        let features = verify_features(&grid, &lumas, gw, &blob);
        let Some(features) = features else {
            continue;
        };
        let size_prior = (area_frac / 0.08).clamp(0.0, 1.0);
        let mut confidence = 0.30 + 0.50 * fill * (0.4 + 0.6 * size_prior) + features.bonus;
        confidence = confidence.clamp(0.0, 0.95);
        let embedding = crop_embedding(rgb, width, height, x, y, w, h);
        out.push(HeuristicFace {
            bbox: [x, y, w, h],
            confidence,
            embedding,
        });
    }
    // Best faces win — confidence order, capped.
    out.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    out.truncate(MAX_FACES);
    out
}

/// Box-sampled working grid (RGB triples) plus per-cell luma.
fn sample_grid(rgb: &[u8], width: u32, height: u32, gw: u32, gh: u32) -> (Vec<[u32; 3]>, Vec<u32>) {
    let mut grid = vec![[0u32; 3]; (gw as usize) * (gh as usize)];
    let mut lumas = vec![0u32; (gw as usize) * (gh as usize)];
    for gy in 0..gh {
        let y0 = (gy * height / gh) as usize;
        let y1 = (((gy + 1) * height / gh).max(y0 as u32 + 1)) as usize;
        for gx in 0..gw {
            let x0 = (gx * width / gw) as usize;
            let x1 = (((gx + 1) * width / gw).max(x0 as u32 + 1)) as usize;
            let (mut sr, mut sg, mut sb, mut n) = (0u32, 0u32, 0u32, 0u32);
            for y in y0..y1.min(height as usize) {
                for x in x0..x1.min(width as usize) {
                    let i = (y * width as usize + x) * 3;
                    sr += rgb[i] as u32;
                    sg += rgb[i + 1] as u32;
                    sb += rgb[i + 2] as u32;
                    n += 1;
                }
            }
            let idx = (gy * gw + gx) as usize;
            if n == 0 {
                continue;
            }
            let (r, g, b) = (sr / n, sg / n, sb / n);
            grid[idx] = [r, g, b];
            lumas[idx] = (299 * r + 587 * g + 114 * b) / 1000;
        }
    }
    (grid, lumas)
}

fn chroma_of(px: &[u32; 3], luma: u32) -> (i32, i32) {
    let cr = (713 * px[0] as i32 - 713 * luma as i32) / 1000 + 128;
    let cb = (564 * px[2] as i32 - 564 * luma as i32) / 1000 + 128;
    (cr, cb)
}

/// White-patch lighting compensation (Hsu): the brightest pixels estimate
/// the illuminant; pull them to gray. Skipped unless every gain is sane —
/// a frame with no bright patch has nothing to estimate from.
fn compensate_white_patch(mut grid: Vec<[u32; 3]>, lumas: &[u32]) -> Vec<[u32; 3]> {
    let mut sorted = lumas.to_vec();
    sorted.sort_unstable();
    if sorted.is_empty() {
        return grid;
    }
    let cutoff = sorted[(95 * sorted.len()) / 100];
    let (mut sr, mut sg, mut sb, mut n) = (0u32, 0u32, 0u32, 0u32);
    for (i, px) in grid.iter().enumerate() {
        if lumas[i] >= cutoff {
            sr += px[0];
            sg += px[1];
            sb += px[2];
            n += 1;
        }
    }
    if n < MIN_WHITE_PIXELS as u32 {
        return grid;
    }
    // Fixed-point gains ×256.
    let means = [sr / n, sg / n, sb / n];
    let mut gains = [0u32; 3];
    for (i, mean) in means.iter().enumerate() {
        if *mean == 0 {
            return grid;
        }
        gains[i] = 255 * 256 / mean;
    }
    let [gr, gg, gb] = gains;
    if ![gr, gg, gb].iter().all(|g| (GAIN_LO..=GAIN_HI).contains(g)) {
        return grid;
    }
    for px in grid.iter_mut() {
        px[0] = (px[0] * gr / 256).min(255);
        px[1] = (px[1] * gg / 256).min(255);
        px[2] = (px[2] * gb / 256).min(255);
    }
    grid
}

/// Luma-adaptive skin test: tight Chai–Ngan box mid-luma, wider rect toward
/// the extremes (dark and bright skin live outside the tight cluster).
fn is_skin(px: &[u32; 3], luma: u32) -> bool {
    if luma < ABS_LUMA_MIN {
        return false;
    }
    let (cr, cb) = chroma_of(px, luma);
    let mid = (MID_LUMA_LO..=MID_LUMA_HI).contains(&luma);
    if mid {
        (133..=173).contains(&cr) && (77..=127).contains(&cb)
    } else {
        (130..=180).contains(&cr) && (75..=135).contains(&cb)
    }
}

fn skin_mask(grid: &[[u32; 3]], lumas: &[u32]) -> Vec<bool> {
    grid.iter()
        .zip(lumas.iter())
        .map(|(px, luma)| is_skin(px, *luma))
        .collect()
}

/// 3×3 opening: erosion (keep iff ≥5 of 9 skin) then dilation (set iff any
/// of 9 skin). Speckles die, thin bridges break, blobs survive.
fn morph_open(mask: &[bool], gw: u32, gh: u32) -> Vec<bool> {
    let at = |m: &[bool], x: i32, y: i32| -> bool {
        if x < 0 || y < 0 || x >= gw as i32 || y >= gh as i32 {
            return false;
        }
        m[(y as u32 * gw + x as u32) as usize]
    };
    let mut eroded = vec![false; mask.len()];
    for y in 0..gh as i32 {
        for x in 0..gw as i32 {
            let mut n = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if at(mask, x + dx, y + dy) {
                        n += 1;
                    }
                }
            }
            eroded[(y as u32 * gw + x as u32) as usize] = n >= 5;
        }
    }
    let mut out = vec![false; mask.len()];
    for y in 0..gh as i32 {
        for x in 0..gw as i32 {
            let mut any = false;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if at(&eroded, x + dx, y + dy) {
                        any = true;
                        break;
                    }
                }
                if any {
                    break;
                }
            }
            out[(y as u32 * gw + x as u32) as usize] = any;
        }
    }
    out
}

/// 4-connected components over the mask (iterative — no recursion depth risk).
fn connected_blobs(mask: &[bool], gw: u32, gh: u32) -> Vec<Blob> {
    connected_blobs_strided(mask, gw, 0, 0, gw, gh)
}

/// Components over a sub-rectangle of a row-major mask (`stride` = full
/// row width). Returned coords are global to the mask.
fn connected_blobs_strided(
    mask: &[bool],
    stride: u32,
    ox: u32,
    oy: u32,
    w: u32,
    h: u32,
) -> Vec<Blob> {
    let mut seen = vec![false; mask.len()];
    let mut blobs = Vec::new();
    for yy in oy..(oy + h) {
        for xx in ox..(ox + w) {
            let start = (yy * stride + xx) as usize;
            if start >= mask.len() || !mask[start] || seen[start] {
                continue;
            }
            let mut stack = vec![start];
            seen[start] = true;
            let mut blob = Blob {
                min_x: u32::MAX,
                min_y: u32::MAX,
                max_x: 0,
                max_y: 0,
                pixels: 0,
            };
            while let Some(idx) = stack.pop() {
                let x = (idx as u32) % stride;
                let y = (idx as u32) / stride;
                blob.min_x = blob.min_x.min(x);
                blob.min_y = blob.min_y.min(y);
                blob.max_x = blob.max_x.max(x);
                blob.max_y = blob.max_y.max(y);
                blob.pixels += 1;
                // 4-neighbours, fenced to the sub-rectangle.
                if x > ox {
                    let n = idx - 1;
                    if mask[n] && !seen[n] {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
                if x + 1 < ox + w {
                    let n = idx + 1;
                    if n < mask.len() && mask[n] && !seen[n] {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
                if y > oy {
                    let n = idx - stride as usize;
                    if mask[n] && !seen[n] {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
                if y + 1 < oy + h {
                    let n = idx + stride as usize;
                    if n < mask.len() && mask[n] && !seen[n] {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
            }
            blobs.push(blob);
        }
    }
    blobs
}

struct FaceFeatures {
    bonus: f32,
}

/// Eye/mouth verification: luma valleys for eye candidates with pair
/// geometry, simplified Hsu mouth map (M = Cr²·(Cr−Cb)) for the mouth.
/// Accept on an eye pair, or one eye plus a mouth. `None` = not a face.
fn verify_features(grid: &[[u32; 3]], lumas: &[u32], gw: u32, blob: &Blob) -> Option<FaceFeatures> {
    let bw = blob.width();
    let bh = blob.height();
    let bbox_area = (bw as f32) * (bh as f32);
    // Bbox mean luma over ALL pixels (valleys included).
    let mut acc = 0u64;
    let mut n = 0u64;
    for y in blob.min_y..=blob.max_y {
        for x in blob.min_x..=blob.max_x {
            let i = (y * gw + x) as usize;
            if i < lumas.len() {
                acc += lumas[i] as u64;
                n += 1;
            }
        }
    }
    if n == 0 {
        return None;
    }
    let mean = (acc / n) as u32;
    let dark_at = (mean * EYE_DARK_NUM / EYE_DARK_DEN).max(EYE_DARK_FLOOR);

    // Eye-dark mask, restricted to the bbox.
    let mut eye_mask = vec![false; lumas.len()];
    for y in blob.min_y..=blob.max_y {
        for x in blob.min_x..=blob.max_x {
            let i = (y * gw + x) as usize;
            if i < lumas.len() && lumas[i] < dark_at {
                eye_mask[i] = true;
            }
        }
    }
    let eye_top = blob.min_y + (bh * EYE_TOP_NUM / EYE_TOP_DEN);
    let mut eyes: Vec<Blob> =
        connected_blobs_strided(&eye_mask, gw, blob.min_x, blob.min_y, bw, bh)
            .into_iter()
            .filter(|b| {
                let area_frac = b.pixels as f32 / bbox_area;
                b.max_y < eye_top
                    && (EYE_MIN_AREA_FRAC..=EYE_MAX_AREA_FRAC).contains(&area_frac)
                    && b.width() >= EYE_MIN_SIDE
                    && b.height() >= EYE_MIN_SIDE
                    && (b.width() as f32) <= EYE_MAX_W_FRAC * bw as f32
            })
            .collect();
    eyes.sort_by_key(|a| std::cmp::Reverse(a.pixels));

    // Pair geometry: aligned, separated, symmetric about the face axis.
    let mut paired = false;
    'pairs: for i in 0..eyes.len() {
        for j in (i + 1)..eyes.len() {
            let a = &eyes[i];
            let b = &eyes[j];
            let (l, r) = if a.min_x <= b.min_x { (a, b) } else { (b, a) };
            let cy_l = (l.min_y + l.max_y) as f32 / 2.0;
            let cy_r = (r.min_y + r.max_y) as f32 / 2.0;
            if (cy_l - cy_r).abs() > PAIR_DY_MAX * bh as f32 {
                continue;
            }
            let gap = r.min_x as f32 - l.max_x as f32;
            if gap < PAIR_GAP_MIN * bw as f32 {
                continue;
            }
            let span = r.max_x as f32 - l.min_x as f32;
            if span > PAIR_SPAN_MAX * bw as f32 {
                continue;
            }
            let mid = (l.min_x as f32 + r.max_x as f32) / 2.0;
            let axis = blob.min_x as f32 + bw as f32 / 2.0;
            if (mid - axis).abs() > PAIR_MID_TOL * bw as f32 {
                continue;
            }
            paired = true;
            break 'pairs;
        }
    }

    // Mouth map over the lower bbox: M = Cr²·(Cr−Cb), relative threshold.
    let mouth_y0 = blob.min_y + (bh * MOUTH_TOP_NUM / MOUTH_TOP_DEN);
    let mut responses: Vec<u32> = Vec::new();
    for y in mouth_y0..=blob.max_y {
        for x in blob.min_x..=blob.max_x {
            let i = (y * gw + x) as usize;
            if i >= grid.len() {
                continue;
            }
            let luma = lumas[i];
            let (cr, cb) = chroma_of(&grid[i], luma);
            // M = Cr²·(Cr−Cb)/255² — integer, 0..~255.
            let (cr, cb) = (cr.max(0) as u32, cb.max(0) as u32);
            let m = cr * cr / 255 * cr.saturating_sub(cb) / 255;
            responses.push(m);
        }
    }
    let max_m = responses.iter().copied().max().unwrap_or(0);
    let mut has_mouth = false;
    if max_m > MOUTH_ABS_FLOOR {
        let rel = (max_m * MOUTH_RESP_NUM / MOUTH_RESP_DEN).max(1);
        let thresh = rel.max(MOUTH_ABS_FLOOR);
        let mut mouth_mask = vec![false; lumas.len()];
        for y in mouth_y0..=blob.max_y {
            for x in blob.min_x..=blob.max_x {
                let i = (y * gw + x) as usize;
                if i >= grid.len() {
                    continue;
                }
                let luma = lumas[i];
                let (cr, cb) = chroma_of(&grid[i], luma);
                let (cr, cb) = (cr.max(0) as u32, cb.max(0) as u32);
                if cr * cr / 255 * cr.saturating_sub(cb) / 255 >= thresh {
                    mouth_mask[i] = true;
                }
            }
        }
        let axis = blob.min_x as f32 + bw as f32 / 2.0;
        has_mouth = connected_blobs_strided(
            &mouth_mask,
            gw,
            blob.min_x,
            mouth_y0,
            bw,
            blob.max_y - mouth_y0 + 1,
        )
        .into_iter()
        .any(|b| {
            let area_frac = b.pixels as f32 / bbox_area;
            let bw_b = b.width() as f32;
            let bh_b = b.height().max(1) as f32;
            let mid = (b.min_x + b.max_x) as f32 / 2.0;
            (MOUTH_MIN_AREA_FRAC..=MOUTH_MAX_AREA_FRAC).contains(&area_frac)
                && bw_b >= MOUTH_W_OVER_H * bh_b
                && (mid - axis).abs() <= MOUTH_MID_TOL * bw as f32
        });
    }

    if paired {
        Some(FaceFeatures {
            bonus: if has_mouth { 0.25 } else { 0.15 },
        })
    } else if !eyes.is_empty() && has_mouth {
        Some(FaceFeatures { bonus: 0.10 })
    } else {
        None
    }
}

/// 512-d embedding from the face crop: 16×16 gray levels + 16×16 local
/// contrast, L2-normalized. Same crop → same vector; different crops stay
/// apart — the property grouping needs, with no learned weights anywhere.
fn crop_embedding(rgb: &[u8], width: u32, height: u32, x: f32, y: f32, w: f32, h: f32) -> Vec<f32> {
    // Marginated crop in source pixels, clamped to the frame.
    let mx = (w * CROP_MARGIN_FRAC * width as f32) as i32;
    let my = (h * CROP_MARGIN_FRAC * height as f32) as i32;
    let x0 = ((x * width as f32) as i32 - mx).max(0);
    let y0 = ((y * height as f32) as i32 - my).max(0);
    let x1 = ((x + w) * width as f32) as i32 + mx;
    let y1 = ((y + h) * height as f32) as i32 + my;
    let x1 = x1.min(width as i32).max(x0 + 1);
    let y1 = y1.min(height as i32).max(y0 + 1);
    let cw = (x1 - x0).max(1) as f32;
    let ch = (y1 - y0).max(1) as f32;
    // 16×16 luma grid by box sampling.
    let mut gray = vec![0.0f32; EMBED_GRID * EMBED_GRID];
    for gy in 0..EMBED_GRID {
        for gx in 0..EMBED_GRID {
            let sx0 = x0 as f32 + gx as f32 * cw / EMBED_GRID as f32;
            let sx1 = x0 as f32 + (gx + 1) as f32 * cw / EMBED_GRID as f32;
            let sy0 = y0 as f32 + gy as f32 * ch / EMBED_GRID as f32;
            let sy1 = y0 as f32 + (gy + 1) as f32 * ch / EMBED_GRID as f32;
            let (mut acc, mut n) = (0u32, 0u32);
            for sy in (sy0 as u32)..(sy1 as u32 + 1).min(height) {
                for sx in (sx0 as u32)..(sx1 as u32 + 1).min(width) {
                    // usize math: absurd dimensions wrap u32 instead of failing.
                    let i = (sy as usize * width as usize + sx as usize) * 3;
                    if i + 2 < rgb.len() {
                        acc += (299 * rgb[i] as u32
                            + 587 * rgb[i + 1] as u32
                            + 114 * rgb[i + 2] as u32)
                            / 1000;
                        n += 1;
                    }
                }
            }
            gray[gy * EMBED_GRID + gx] = if n > 0 {
                acc as f32 / n as f32 / 255.0
            } else {
                0.0
            };
        }
    }
    // Local contrast: |pixel − 3×3 mean|, same grid.
    let mut out = vec![0.0f32; EMBEDDING_DIM];
    for gy in 0..EMBED_GRID {
        for gx in 0..EMBED_GRID {
            let mut acc = 0.0f32;
            let mut n = 0u32;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = gx as i32 + dx;
                    let ny = gy as i32 + dy;
                    if nx >= 0 && nx < EMBED_GRID as i32 && ny >= 0 && ny < EMBED_GRID as i32 {
                        acc += gray[(ny as usize) * EMBED_GRID + nx as usize];
                        n += 1;
                    }
                }
            }
            let mean = if n > 0 { acc / n as f32 } else { 0.0 };
            let g = gray[gy * EMBED_GRID + gx];
            out[gy * EMBED_GRID + gx] = g;
            out[EMBED_GRID * EMBED_GRID + gy * EMBED_GRID + gx] = (g - mean).abs();
        }
    }
    // L2-normalize (degenerate flat crop → tiny epsilon, never NaN).
    let norm = out.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    for v in out.iter_mut() {
        *v /= norm;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint_rect(frame: &mut [u8], w: u32, x0: u32, y0: u32, x1: u32, y1: u32, px: [u8; 3]) {
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * w + x) * 3) as usize;
                frame[i..i + 3].copy_from_slice(&px);
            }
        }
    }

    /// 200×200, blue background, skin rect with two dark eyes + red mouth.
    fn face_frame() -> (Vec<u8>, u32, u32) {
        let (w, h) = (200u32, 200u32);
        let mut frame = vec![0u8; (w as usize) * (h as usize) * 3];
        for chunk in frame.as_chunks_mut::<3>().0 {
            chunk.copy_from_slice(&[40u8, 80u8, 220u8]);
        }
        paint_rect(&mut frame, w, 70, 50, 130, 140, [200, 150, 120]);
        // Eyes: dark valleys in the upper half, symmetric about x=100.
        paint_rect(&mut frame, w, 82, 78, 94, 88, [25, 25, 25]);
        paint_rect(&mut frame, w, 106, 78, 118, 88, [25, 25, 25]);
        // Mouth: dark red, wide, in the lower third.
        paint_rect(&mut frame, w, 88, 115, 112, 125, [150, 60, 60]);
        (frame, w, h)
    }

    /// Same skin rect but featureless — a hand, a wall, an arm.
    fn plain_blob_frame() -> (Vec<u8>, u32, u32) {
        let (w, h) = (200u32, 200u32);
        let mut frame = vec![0u8; (w as usize) * (h as usize) * 3];
        for chunk in frame.as_chunks_mut::<3>().0 {
            chunk.copy_from_slice(&[40u8, 80u8, 220u8]);
        }
        paint_rect(&mut frame, w, 70, 50, 130, 140, [200, 150, 120]);
        (frame, w, h)
    }

    #[test]
    fn face_with_eyes_and_mouth_is_found() {
        let (frame, w, h) = face_frame();
        let faces = detect_faces_heuristic_rgb(&frame, w, h);
        assert_eq!(faces.len(), 1, "expected one face, got {faces:?}");
        let f = &faces[0];
        assert!((0.0..=1.0).contains(&f.confidence));
        assert!(
            f.confidence > 0.5,
            "eyes+mouth should clear 0.5, got {}",
            f.confidence
        );
        assert!(f.bbox[0] > 0.2 && f.bbox[0] < 0.45, "x={}", f.bbox[0]);
        assert!(f.bbox[1] > 0.15 && f.bbox[1] < 0.35, "y={}", f.bbox[1]);
        assert_eq!(f.embedding.len(), EMBEDDING_DIM);
        let norm = f.embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "norm={norm}");
    }

    #[test]
    fn featureless_blob_is_rejected() {
        // No eyes, no mouth → not a face, however skin-like.
        let (frame, w, h) = plain_blob_frame();
        assert!(detect_faces_heuristic_rgb(&frame, w, h).is_empty());
    }

    #[test]
    fn single_eye_plus_mouth_is_found() {
        let (mut frame, w, h) = face_frame();
        // Paint over the right eye with skin: one eye + mouth remain.
        paint_rect(&mut frame, w, 106, 78, 118, 88, [200, 150, 120]);
        let faces = detect_faces_heuristic_rgb(&frame, w, h);
        assert_eq!(faces.len(), 1, "one eye + mouth should pass, got {faces:?}");
    }

    #[test]
    fn lone_eye_without_mouth_is_rejected() {
        let (mut frame, w, h) = face_frame();
        paint_rect(&mut frame, w, 106, 78, 118, 88, [200, 150, 120]);
        paint_rect(&mut frame, w, 88, 115, 112, 125, [200, 150, 120]);
        let faces = detect_faces_heuristic_rgb(&frame, w, h);
        assert!(
            faces.is_empty(),
            "a single eye alone must not pass: {faces:?}"
        );
    }

    #[test]
    fn pure_blue_finds_nothing() {
        let (w, h) = (160u32, 120u32);
        let mut frame = vec![0u8; (w as usize) * (h as usize) * 3];
        for chunk in frame.as_chunks_mut::<3>().0 {
            chunk.copy_from_slice(&[40u8, 80u8, 220u8]);
        }
        assert!(detect_faces_heuristic_rgb(&frame, w, h).is_empty());
    }

    #[test]
    fn full_skin_frame_is_rejected_as_wall() {
        let (w, h) = (120u32, 120u32);
        let frame = vec![200u8; (w as usize) * (h as usize) * 3]
            .chunks_exact(3)
            .flat_map(|_| [200u8, 150u8, 120u8])
            .collect::<Vec<u8>>();
        // 100% skin coverage exceeds MAX_AREA_FRAC — a tint, not a face.
        assert!(detect_faces_heuristic_rgb(&frame, w, h).is_empty());
    }

    #[test]
    fn dark_skin_face_is_found() {
        // Very dark skin outside the mid-luma box — the wide rect must catch it.
        let (w, h) = (200u32, 200u32);
        let mut frame = vec![0u8; (w as usize) * (h as usize) * 3];
        for chunk in frame.as_chunks_mut::<3>().0 {
            chunk.copy_from_slice(&[40u8, 80u8, 220u8]);
        }
        paint_rect(&mut frame, w, 70, 50, 130, 140, [50, 30, 20]);
        paint_rect(&mut frame, w, 82, 78, 94, 88, [12, 12, 12]);
        paint_rect(&mut frame, w, 106, 78, 118, 88, [12, 12, 12]);
        paint_rect(&mut frame, w, 88, 115, 112, 125, [90, 25, 20]);
        let faces = detect_faces_heuristic_rgb(&frame, w, h);
        assert_eq!(faces.len(), 1, "dark skin face should pass, got {faces:?}");
    }

    #[test]
    fn bad_input_is_empty_not_panic() {
        assert!(detect_faces_heuristic_rgb(&[], 0, 0).is_empty());
        assert!(detect_faces_heuristic_rgb(&[1, 2, 3], 10, 10).is_empty());
        assert!(detect_faces_heuristic_rgb(&[0u8; 12], 2, 2).is_empty());
    }

    #[test]
    fn detection_is_deterministic() {
        let (frame, w, h) = face_frame();
        let a = detect_faces_heuristic_rgb(&frame, w, h);
        let b = detect_faces_heuristic_rgb(&frame, w, h);
        assert_eq!(a.len(), b.len());
        assert_eq!(a[0].embedding, b[0].embedding);
        assert_eq!(a[0].confidence, b[0].confidence);
    }

    #[test]
    fn different_crops_stay_apart() {
        let (frame_a, w, h) = face_frame();
        let mut frame_b = frame_a.clone();
        // Darken the lower half of the skin rect: same geometry, new content.
        paint_rect(&mut frame_b, w, 70, 100, 130, 140, [60, 40, 30]);
        let ea = &detect_faces_heuristic_rgb(&frame_a, w, h)[0].embedding;
        let eb = &detect_faces_heuristic_rgb(&frame_b, w, h)[0].embedding;
        let dist = super::super::embedding_distance(ea, eb);
        assert!(dist > 0.05, "crops differ yet dist={dist}");
    }
}
