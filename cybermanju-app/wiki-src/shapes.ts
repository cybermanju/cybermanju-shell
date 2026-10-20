// CyberManju OS — wiki component forms (wiki-src/shapes.ts).
// Role: the per-subject FORM library — every module gets its own 3D
//   geometry (diamond, gear, star, chevron, spindle… chosen at build time
//   in generate-wiki.mjs's SUBJECTS table to match what the module DOES)
//   plus its 2D clip-path silhouette for the rail/chips/legend and its
//   vis-network shape for graphify's graph.html. No subject is ever a
//   bare cube or circle — 16 subjects, 16 distinct silhouettes.
// Exports: shapeOf(key) (form with build()+clip), shapeClip(key),
//   KIND_SHAPE (symbol-kind → satellite form), subjectOfPath(file, subs),
//   SHAPE_KEYS.
// Integrates: wiki-src/main.ts consumes shapeOf for nodes/satellites and
//   shapeClip for the DOM; scripts/generate-wiki.mjs owns the subject→key
//   assignment (emitted into subjects.json as shape/glyph/shapeWhy) and
//   injects the vis mapping into the copied graph.html.
// Contracts: pure (no DOM/canvas — only three geometry math), so vitest
//   imports it in node env; geometry is memoized per key (shared across
//   instances, materials stay per node); build(size) scales uniformly.
import * as THREE from 'three';

interface ShapeDef {
  /** Build the object (single mesh or small group) with this material. */
  build(mat: THREE.Material, size?: number): THREE.Object3D;
  /** CSS clip-path polygon — the 2D silhouette of the same form. */
  clip: string;
}

function poly(points: [number, number][]): string {
  return `polygon(${points.map(([x, y]) => `${x}% ${y}%`).join(', ')})`;
}

function starPoints(outer: number, inner: number, n: number): string {
  const pts: [number, number][] = [];
  for (let i = 0; i < n * 2; i++) {
    const r = i % 2 ? inner : outer;
    const a = (i / (n * 2)) * Math.PI * 2 - Math.PI / 2;
    pts.push([+(50 + Math.cos(a) * r).toFixed(2), +(50 + Math.sin(a) * r).toFixed(2)]);
  }
  return poly(pts);
}

function gearPoints(teeth: number, rOut: number, rIn: number): string {
  const pts: [number, number][] = [];
  const steps = teeth * 4;
  for (let i = 0; i < steps; i++) {
    const r = i % 4 === 0 || i % 4 === 3 ? rIn : rOut;
    const a = (i / steps) * Math.PI * 2 - Math.PI / 2;
    pts.push([+(50 + Math.cos(a) * r).toFixed(2), +(50 + Math.sin(a) * r).toFixed(2)]);
  }
  return poly(pts);
}

/** Thick C (open ring) — the sync cycle silhouette. */
function ringC(rOut: number, rIn: number, gapDeg: number): string {
  const pts: [number, number][] = [];
  const half = (gapDeg / 2) * (Math.PI / 180);
  const end = Math.PI * 2 - half;
  for (let a = half; a <= end + 0.01; a += Math.PI / 10) {
    pts.push([+(50 + Math.cos(a) * rOut).toFixed(2), +(50 + Math.sin(a) * rOut).toFixed(2)]);
  }
  for (let a = end; a >= half - 0.01; a -= Math.PI / 10) {
    pts.push([+(50 + Math.cos(a) * rIn).toFixed(2), +(50 + Math.sin(a) * rIn).toFixed(2)]);
  }
  return poly(pts);
}

// ── geometry memo (one BufferGeometry per key, shared by all instances) ─
const geoCache = new Map<string, THREE.BufferGeometry | THREE.BufferGeometry[]>();
function memoGeo(key: string, make: () => THREE.BufferGeometry | THREE.BufferGeometry[]): THREE.BufferGeometry | THREE.BufferGeometry[] {
  let g = geoCache.get(key);
  if (!g) {
    g = make();
    geoCache.set(key, g);
  }
  return g;
}

function single(key: string, make: () => THREE.BufferGeometry): (mat: THREE.Material, size?: number) => THREE.Object3D {
  return (mat, size = 1) => {
    const m = new THREE.Mesh(memoGeo(key, make) as THREE.BufferGeometry, mat);
    if (size !== 1) m.scale.setScalar(size);
    return m;
  };
}

function multi(key: string, make: () => THREE.BufferGeometry[]): (mat: THREE.Material, size?: number) => THREE.Object3D {
  return (mat, size = 1) => {
    const g = new THREE.Group();
    for (const part of memoGeo(key, make) as THREE.BufferGeometry[]) {
      const m = new THREE.Mesh(part, mat);
      if (size !== 1) m.scale.setScalar(size);
      g.add(m);
    }
    return g;
  };
}

function extruded(shape: THREE.Shape, depth: number): THREE.BufferGeometry {
  const g = new THREE.ExtrudeGeometry(shape, { depth, bevelEnabled: false, curveSegments: 6 });
  g.center();
  return g;
}

function starShape(outer: number, inner: number, n: number): THREE.Shape {
  const s = new THREE.Shape();
  for (let i = 0; i < n * 2; i++) {
    const r = i % 2 ? inner : outer;
    const a = (i / (n * 2)) * Math.PI * 2 - Math.PI / 2;
    const x = Math.cos(a) * r;
    const y = Math.sin(a) * r;
    if (i === 0) s.moveTo(x, y);
    else s.lineTo(x, y);
  }
  s.closePath();
  return s;
}

function gearShape(teeth: number, rOut: number, rIn: number): THREE.Shape {
  const s = new THREE.Shape();
  const steps = teeth * 4;
  for (let i = 0; i < steps; i++) {
    const r = i % 4 === 0 || i % 4 === 3 ? rIn : rOut;
    const a = (i / steps) * Math.PI * 2 - Math.PI / 2;
    const x = Math.cos(a) * r;
    const y = Math.sin(a) * r;
    if (i === 0) s.moveTo(x, y);
    else s.lineTo(x, y);
  }
  s.closePath();
  // Center bore so the gear reads as a mechanism, not a coin.
  const hole = new THREE.Path();
  hole.absarc(0, 0, rIn * 0.42, 0, Math.PI * 2, true);
  s.holes.push(hole);
  return s;
}

const SHAPES: Record<string, ShapeDef> = {
  // overview — the whole system faceted into one crystal.
  facet: {
    build: single('facet', () => new THREE.IcosahedronGeometry(1.0, 1)),
    clip: poly([[30, 0], [70, 0], [100, 30], [100, 70], [70, 100], [30, 100], [0, 70], [0, 30]]),
  },
  // agent — the agentic loop woven into a knot.
  knot: {
    build: single('knot', () => new THREE.TorusKnotGeometry(0.72, 0.24, 72, 8, 2, 3)),
    clip: starPoints(48, 30, 6),
  },
  // sync — the open scan→verify→repair cycle.
  orbit: {
    build: single('orbit', () => {
      const g = new THREE.TorusGeometry(0.82, 0.24, 8, 40, Math.PI * 1.5);
      g.rotateZ(Math.PI * 0.25);
      return g;
    }),
    clip: ringC(49, 33, 78),
  },
  // crypto — the post-quantum key diamond.
  diamond: {
    build: single('diamond', () => {
      const g = new THREE.OctahedronGeometry(1.0, 0);
      g.scale(1, 1.3, 1);
      return g;
    }),
    clip: poly([[50, 0], [100, 50], [50, 100], [0, 50]]),
  },
  // compression — the LZ4→ZSTD→Brotli funnel.
  funnel: {
    build: single('funnel', () => new THREE.ConeGeometry(1.0, 1.6, 14)),
    clip: poly([[12, 12], [88, 12], [64, 88], [36, 88]]),
  },
  // disk — the virtual-disk platter.
  platter: {
    build: single('platter', () => new THREE.CylinderGeometry(1.05, 1.05, 0.5, 12)),
    clip: poly([[22, 20], [78, 20], [100, 50], [78, 80], [22, 80], [0, 50]]),
  },
  // search — the BM25 magnifier ring + handle.
  lens: {
    build: multi('lens', () => {
      const ring = new THREE.TorusGeometry(0.68, 0.2, 8, 26);
      const handle = new THREE.CapsuleGeometry(0.16, 0.6, 4, 10);
      handle.rotateZ(-Math.PI / 4);
      handle.translate(0.78, -0.78, 0);
      return [ring, handle];
    }),
    clip: ringC(49, 33, 40),
  },
  // faces — the detection spotlight.
  star: {
    build: single('star', () => extruded(starShape(1.0, 0.45, 5), 0.4)),
    clip: starPoints(49, 22, 5),
  },
  // os-shell — the cybsh prompt chevron.
  chevron: {
    build: single('chevron', () => {
      const shape = new THREE.Shape();
      shape.moveTo(-0.5, 0.9);
      shape.lineTo(0.55, 0);
      shape.lineTo(-0.5, -0.9);
      shape.lineTo(-0.12, -0.66);
      shape.lineTo(0.14, 0);
      shape.lineTo(-0.12, 0.66);
      shape.closePath();
      return extruded(shape, 0.5);
    }),
    clip: poly([[8, 8], [62, 8], [100, 50], [62, 92], [8, 92], [46, 50]]),
  },
  // web-api — the faceted API globe.
  globe: {
    build: single('globe', () => new THREE.DodecahedronGeometry(1.0, 0)),
    clip: poly([[50, 2], [86, 22], [96, 62], [72, 96], [28, 96], [4, 62], [14, 22]]),
  },
  // db-types — the redb schema hex prism.
  hex: {
    build: single('hex', () => new THREE.CylinderGeometry(0.95, 0.95, 1.3, 6)),
    clip: poly([[25, 5], [75, 5], [100, 50], [75, 95], [25, 95], [0, 50]]),
  },
  // frontend — the Vue component pill.
  pill: {
    build: single('pill', () => {
      const g = new THREE.CapsuleGeometry(0.55, 1.1, 6, 14);
      g.rotateZ(Math.PI / 2);
      return g;
    }),
    clip: poly([[30, 2], [70, 2], [96, 30], [96, 70], [70, 98], [30, 98], [4, 70], [4, 30]]),
  },
  // tauri — the desktop shell shield.
  shield: {
    build: single('shield', () => {
      const shape = new THREE.Shape();
      shape.moveTo(0, 1.0);
      shape.lineTo(0.82, 0.66);
      shape.lineTo(0.82, -0.1);
      shape.quadraticCurveTo(0.82, -0.72, 0, -1.0);
      shape.quadraticCurveTo(-0.82, -0.72, -0.82, -0.1);
      shape.lineTo(-0.82, 0.66);
      shape.closePath();
      return extruded(shape, 0.5);
    }),
    clip: poly([[50, 0], [100, 16], [100, 52], [50, 100], [0, 52], [0, 16]]),
  },
  // cli — the ▶ pointer wedge.
  cursor: {
    build: single('cursor', () => {
      const shape = new THREE.Shape();
      shape.moveTo(-0.5, 0.92);
      shape.lineTo(-0.5, -0.92);
      shape.lineTo(0.75, 0);
      shape.closePath();
      return extruded(shape, 0.5);
    }),
    clip: poly([[16, 6], [16, 94], [90, 50]]),
  },
  // wasm — the in-browser hourglass spindle.
  spindle: {
    build: multi('spindle', () => {
      const top = new THREE.ConeGeometry(0.75, 1.0, 10);
      top.rotateX(Math.PI); // apex down — meets the lower cone at the waist
      top.translate(0, 0.5, 0);
      const bottom = new THREE.ConeGeometry(0.75, 1.0, 10);
      bottom.translate(0, -0.5, 0);
      return [top, bottom];
    }),
    clip: poly([[8, 4], [92, 4], [58, 50], [92, 96], [8, 96], [42, 50]]),
  },
  // ops — the CI/release gear.
  gear: {
    build: single('gear', () => extruded(gearShape(9, 1.0, 0.76), 0.42)),
    clip: gearPoints(9, 49, 38),
  },
};

/** CSS clip-path for a shape key (2D mirrors of the 3D forms). */
export function shapeClip(key: string | null | undefined): string {
  return SHAPES[key ?? '']?.clip ?? SHAPES.facet.clip;
}

// Shared throwaway material for geometry-only builds (never rendered).
const EDGE_DUMMY = new THREE.MeshBasicMaterial();
const edgeCache = new Map<string, THREE.EdgesGeometry>();

/** Edge-outline geometry for a shape key (the wireframe accent that rides
 *  each module node). Built from the shape's first mesh; memoized per key. */
export function shapeEdgeGeo(
  key: string | null | undefined,
  thresholdDeg = 28,
): THREE.EdgesGeometry | null {
  const k = key && SHAPES[key] ? key : 'facet';
  const hit = edgeCache.get(k);
  if (hit) return hit;
  const built = SHAPES[k].build(EDGE_DUMMY, 1);
  let geo: THREE.BufferGeometry | null = null;
  const asMesh = built as THREE.Mesh;
  if (asMesh.isMesh) {
    geo = asMesh.geometry;
  } else {
    let found: THREE.Mesh | null = null;
    built.traverse((o) => {
      if (!found) {
        const m = o as THREE.Mesh;
        if (m.isMesh) found = m;
      }
    });
    geo = (found as THREE.Mesh | null)?.geometry ?? null;
  }
  if (!geo) return null;
  const edges = new THREE.EdgesGeometry(geo, thresholdDeg);
  edgeCache.set(k, edges);
  return edges;
}

/** The form for a shape key — falls back to the overview crystal. */
export function shapeOf(key: string | null | undefined): ShapeDef {
  return SHAPES[key ?? ''] ?? SHAPES.facet;
}

/** Symbol-kind → satellite form. God nodes override to star; link
 *  satellites use the target module's own form (the tether shows where
 *  it leads). */
export const KIND_SHAPE: Record<string, string> = {
  function: 'cursor',
  'class': 'globe',
  module: 'hex',
  interface: 'facet',
  const: 'hex',
  type: 'hex',
  impl: 'chevron',
  macro: 'gear',
};

/** File-prefix subject match — mirrors scripts/wiki-graph.mjs subjectOfFile
 *  so SPA connection chips can jump across modules. */
export function subjectOfPath(
  file: string,
  subjects: { id: string; paths?: string[] }[],
): string | null {
  for (const s of subjects) {
    for (const p of s.paths ?? []) {
      if (file === p || file.startsWith(p)) return s.id;
    }
  }
  return null;
}

export const SHAPE_KEYS = Object.keys(SHAPES);
