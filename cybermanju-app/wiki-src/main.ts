// CyberManju OS — wiki 3D app (wiki-src/main.ts).
// Role: Three.js scenery (orbiting module FORMS around a manju mascot) +
//   reader panels; the scene renders the graphify knowledge graph live
//   (subject↔subject EXTRACTED/INFERRED edges, ★ god-node satellites,
//   community-colored orbit frames, traced-path highlighting) with a 2D
//   switch to graphify's graph.html overlay. Every module node is its own
//   function-themed silhouette (wiki-src/shapes.ts: diamond, gear, star,
//   chevron, spindle… assigned at build time — never a bare cube), and
//   satellites are MAGNETIC — they lean toward the cursor, are pickable
//   by screen proximity, and clicking one opens the real source block +
//   auto explanation from the lazily fetched symbols-<id>.json payload.
// Exports: boot() — fetches subjects.json, builds scene + rail + panel +
//   hash routing + the 2D/3D toggle.
// Integrates: built by vite.config.wiki.ts into dist-wasm/wiki/; data from
//   scripts/generate-wiki.mjs (subjects.json incl. graph payload + form
//   table shape/glyph/shapeWhy + symbols-<id>.json deep-view payload);
//   static twins at wiki/<id>.html; 2D overlay loads ./graph.html
//   (graphify, shape-injected).
// Contracts: fully static (no backend); honors prefers-reduced-motion;
//   read-only fetch; graph UI only appears when the payload says engine
//   graphify — heuristic builds show no fabricated provenance.

import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/addons/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/addons/postprocessing/UnrealBloomPass.js';
import { OutputPass } from 'three/addons/postprocessing/OutputPass.js';
import { shapeOf, shapeEdgeGeo, shapeClip, KIND_SHAPE, subjectOfPath } from './shapes';

interface TopSymbol {
  name: string;
  kind: string;
  docs: string;
  file: string;
  line: number;
}

// Cross-subject knowledge-graph edge (graphify engine): provenance stays
// split — EXTRACTED (read from the source AST) never merges with INFERRED
// (resolved by the extractor), same honesty rule as faces/code-intel.
interface GraphEdge {
  to: string;
  weight: number;
  extracted: number;
  inferred: number;
  relations: Record<string, number>;
  sample?: string;
}

interface GodNode {
  name: string;
  file: string;
  line: number;
  degree: number;
  subject?: string;
  community?: number;
}

// One graphify symbol-edge neighbor (real repo connection, provenance kept).
interface SymbolNeighbor {
  name: string;
  file: string;
  line: number;
  relation: string;
  confidence: string;
  weight: number;
  via: 'out' | 'in';
}

// Deep-view entry from symbols-<id>.json: real extracted source + auto
// explanation assembled at build time (engine-labeled, no LLM).
interface SymbolEntry {
  name: string;
  kind: string;
  docs: string;
  file: string;
  line: number;
  code: string;
  codeLines: number;
  explanation: string;
  degree?: number;
  community?: number;
  connections?: SymbolNeighbor[];
}

interface GraphInfo {
  engine: string;
  commit: string | null;
  stats: {
    nodes: number;
    edges: number;
    mapped: number;
    extracted: number;
    inferred: number;
    communities: number;
    subjects: number;
  };
  godNodes: GodNode[];
  communities: { id: number; size: number; subjects: string[] }[];
  paths: Record<string, string[]>;
}

interface Subject {
  id: string;
  title: string;
  tag: string;
  blurb: string;
  color: string;
  // Build-time form assignment (generate-wiki.mjs SUBJECTS): the 3D
  // silhouette key, its emoji glyph, and the honest reason it fits.
  shape?: string | null;
  glyph?: string | null;
  shapeWhy?: string | null;
  paths?: string[];
  fileCount: number;
  symbolCount: number;
  contracts: string[];
  links: string[];
  top: TopSymbol[];
  engine?: string;
  community?: number | null;
  edges?: GraphEdge[];
  godNodes?: GodNode[];
  symbolsUrl?: string | null;
}

interface WikiData {
  version: string;
  graph?: GraphInfo | null;
  subjects: Subject[];
}

const REDUCED = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

function esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

// Community palette (graphify Leiden clusters): one hue per community id so
// modules of the same subsystem share an orbit-frame color at a glance.
const COMM_PALETTE = ['#34d399', '#60a5fa', '#fbbf24', '#f472b6', '#a78bfa', '#fb7185', '#22d3ee', '#4ade80'];
function commColor(id: number | null | undefined): string {
  return id == null || id < 0 ? '#ffffff' : COMM_PALETTE[Math.abs(id) % COMM_PALETTE.length];
}

// One shared radial texture per color (award sites never mint a canvas
// per sprite — this keeps ~100 glow sprites to a handful of textures).
const glowTexCache = new Map<string, THREE.CanvasTexture>();
function glowTexture(color: string): THREE.CanvasTexture {
  let tex = glowTexCache.get(color);
  if (!tex) {
    const c = document.createElement('canvas');
    // Tiny canvas on purpose: magnified with NearestFilter the glow reads
    // as chunky pixel-art light instead of a photorealistic gradient.
    c.width = c.height = 24;
    const g = c.getContext('2d')!;
    const grad = g.createRadialGradient(12, 12, 1, 12, 12, 12);
    grad.addColorStop(0, color);
    grad.addColorStop(0.4, color + 'aa');
    grad.addColorStop(1, color + '00');
    g.fillStyle = grad;
    g.fillRect(0, 0, 24, 24);
    tex = new THREE.CanvasTexture(c);
    tex.magFilter = THREE.NearestFilter;
    glowTexCache.set(color, tex);
  }
  return tex;
}

function glowSprite(color: string, size: number, opacity: number): THREE.Sprite {
  const tex = glowTexture(color);
  const mat = new THREE.SpriteMaterial({
    map: tex,
    transparent: true,
    opacity,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
  });
  const sp = new THREE.Sprite(mat);
  sp.scale.set(size, size, 1);
  return sp;
}

// Pixel-art nameplate: opaque dark plate, chunky accent frame with stepped
// corner rivets, bitmap type (Silkscreen) with a hard black outline. Plates
// live in labelScene and render AFTER the post chain (see tick), so bloom +
// tone mapping can never smear the type — labels stay crisp at any distance.
// Textures are memoized per (text, accent): satellite churn (rebuild on every
// selection) reuses canvases instead of re-rasterizing, and cached textures
// are NEVER disposed by clearSats — they outlive the sprites that borrow them.
const plateCache = new Map<string, THREE.CanvasTexture>();
const PLATE_H = 118;
function plateTexture(text: string, accent: string): THREE.CanvasTexture {
  const key = `${text}|${accent}`;
  const cached = plateCache.get(key);
  if (cached) return cached;
  const c = document.createElement('canvas');
  const g = c.getContext('2d')!;
  const font = '700 56px "Silkscreen", "Baloo 2", ui-monospace, monospace';
  g.font = font;
  const padX = 34;
  c.width = Math.ceil(g.measureText(text).width) + padX * 2;
  c.height = PLATE_H;
  const g2 = c.getContext('2d')!;
  // Opaque plate — nothing (starfield, nebulae, bloom) shines through.
  g2.fillStyle = '#0a0514';
  g2.fillRect(0, 0, c.width, c.height);
  // Chunky 8px accent frame.
  g2.fillStyle = accent;
  g2.fillRect(0, 0, c.width, 8);
  g2.fillRect(0, c.height - 8, c.width, 8);
  g2.fillRect(0, 0, 8, c.height);
  g2.fillRect(c.width - 8, 0, 8, c.height);
  // Stepped corner rivets (white block + accent core = pixel-notch detail).
  for (const [rx, ry] of [
    [0, 0],
    [c.width - 16, 0],
    [0, c.height - 16],
    [c.width - 16, c.height - 16],
  ]) {
    g2.fillStyle = '#f5f3ff';
    g2.fillRect(rx, ry, 16, 16);
    g2.fillStyle = accent;
    g2.fillRect(rx + 4, ry + 4, 8, 8);
  }
  g2.font = font;
  g2.textBaseline = 'middle';
  g2.lineJoin = 'miter';
  g2.miterLimit = 2;
  // Hard black outline first, then the bright fill — readable on any bg.
  g2.strokeStyle = '#000000';
  g2.lineWidth = 10;
  g2.strokeText(text, padX, PLATE_H / 2 + 3);
  g2.fillStyle = '#ffffff';
  g2.fillText(text, padX, PLATE_H / 2 + 3);
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  // Nearest on magnify: chunky pixels when zoomed in, mipmapped when far.
  tex.magFilter = THREE.NearestFilter;
  tex.minFilter = THREE.LinearMipmapLinearFilter;
  plateCache.set(key, tex);
  return tex;
}

function textSprite(text: string, h = 1.15, accent = '#a855f7'): THREE.Sprite {
  const tex = plateTexture(text, accent);
  const sp = new THREE.Sprite(
    new THREE.SpriteMaterial({
      map: tex,
      transparent: false,
      depthWrite: false,
      toneMapped: false,
    }),
  );
  sp.scale.set((h * tex.image.width) / PLATE_H, h, 1);
  sp.userData.baseScale = sp.scale.clone();
  return sp;
}

function manjuFace(): THREE.CanvasTexture {
  const c = document.createElement('canvas');
  c.width = c.height = 256;
  const g = c.getContext('2d')!;
  g.clearRect(0, 0, 256, 256);
  const ink = '#2b1247';
  // Eyes (anime ovals + highlights).
  for (const x of [86, 170]) {
    g.fillStyle = ink;
    g.beginPath();
    g.ellipse(x, 108, 17, 24, 0, 0, Math.PI * 2);
    g.fill();
    g.fillStyle = '#ffffff';
    g.beginPath();
    g.arc(x + 5, 100, 6, 0, Math.PI * 2);
    g.fill();
  }
  // Blush.
  g.fillStyle = 'rgba(255,77,109,0.55)';
  for (const x of [52, 204]) {
    g.beginPath();
    g.ellipse(x, 152, 20, 11, 0, 0, Math.PI * 2);
    g.fill();
  }
  // Smile.
  g.strokeStyle = ink;
  g.lineWidth = 6;
  g.lineCap = 'round';
  g.beginPath();
  g.arc(128, 138, 20, Math.PI * 0.2, Math.PI * 0.8);
  g.stroke();
  // Pixelate the face: crush to 32px and blow back up with no smoothing,
  // so the mascot matches the voxel scenery instead of looking airbrushed.
  const tiny = document.createElement('canvas');
  tiny.width = tiny.height = 32;
  tiny.getContext('2d')!.drawImage(c, 0, 0, 32, 32);
  g.imageSmoothingEnabled = false;
  g.clearRect(0, 0, 256, 256);
  g.drawImage(tiny, 0, 0, 256, 256);
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.magFilter = THREE.NearestFilter;
  return tex;
}

async function boot(): Promise<void> {
  const panel = document.getElementById('panel')!;
  const rail = document.getElementById('rail')!;
  const tooltip = document.getElementById('tooltip')!;
  const search = document.getElementById('search') as HTMLInputElement;

  // Skeleton while subjects.json loads (replaced on first render).
  panel.innerHTML = `<div class="tag">loading…</div><h1>CyberManju wiki</h1>
    <div class="skel" style="width:92%"></div><div class="skel" style="width:78%"></div>
    <div class="skel" style="width:85%"></div><div class="skel" style="width:60%"></div>`;

  let data: WikiData;
  try {
    const res = await fetch('./subjects.json', { cache: 'no-store' });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    data = (await res.json()) as WikiData;
  } catch (err) {
    panel.innerHTML = `<h1>Wiki data missing</h1><p class="empty">Could not load <code>subjects.json</code> (${esc(
      String(err),
    )}). Run <code>npm run wiki</code> to regenerate it.</p>`;
    return;
  }
  const byId = new Map(data.subjects.map((s) => [s.id, s]));

  // ── Symbol deep-view store ───────────────────────────────
  // symbols-<id>.json carries the REAL source block + explanation per
  // symbol, fetched on demand when a planet is opened (cached forever —
  // the payload is a build artifact, rebuilt with the wiki each push).
  const symStores = new Map<string, Map<string, SymbolEntry>>();
  const symPending = new Map<string, Promise<void>>();
  function loadSymbols(sub: Subject): Promise<Map<string, SymbolEntry>> {
    const cached = symStores.get(sub.id);
    if (cached) return Promise.resolve(cached);
    const store = new Map<string, SymbolEntry>();
    symStores.set(sub.id, store);
    if (!sub.symbolsUrl) return Promise.resolve(store);
    const pending = symPending.get(sub.id);
    if (pending) return pending.then(() => store);
    const p = fetch(sub.symbolsUrl, { cache: 'no-store' })
      .then((r) => (r.ok ? r.json() : { symbols: [] }))
      .then((payload: { symbols?: SymbolEntry[] }) => {
        for (const e of payload.symbols ?? []) store.set(e.name, e);
      })
      .catch(() => {
        /* offline / missing payload — deep view degrades to docs only */
      });
    symPending.set(sub.id, p);
    return p.then(() => store);
  }

  // ── 2D/3D switch: graphify's graph.html in an overlay ─────
  // The 2D view only exists when the graph built in CI; the iframe loads
  // once on first use and rendering pauses underneath it (battery/GPU).
  const toggle2d = document.getElementById('toggle2d') as HTMLButtonElement | null;
  const graph2d = document.getElementById('graph2d')!;
  const graphFrame = document.getElementById('graph2dframe') as HTMLIFrameElement;
  let twoD = false;
  function set2D(on: boolean): void {
    if (!data.graph || !toggle2d) return;
    twoD = on;
    graph2d.hidden = !on;
    graph2d.setAttribute('aria-hidden', String(!on));
    toggle2d.setAttribute('aria-pressed', String(on));
    toggle2d.textContent = on ? '3D view' : '2D graph';
    document.title = on ? 'Knowledge graph — CyberManju wiki' : 'CyberManju OS — 3D wiki';
    if (on && graphFrame.dataset.loaded !== '1') {
      graphFrame.src = './graph.html';
      graphFrame.dataset.loaded = '1';
    }
  }
  if (toggle2d && data.graph) {
    toggle2d.hidden = false;
    toggle2d.addEventListener('click', () => set2D(!twoD));
  }

  // Pixel-art type: wait (briefly) for the bitmap font so canvas nameplates
  // are measured and rasterized in Silkscreen. Offline/slow fonts fall back
  // to Baloo 2 / monospace — still legible, just less blocky.
  try {
    await Promise.race([
      Promise.all([
        document.fonts.load('700 56px "Silkscreen"'),
        document.fonts.load('400 56px "Silkscreen"'),
      ]),
      new Promise((r) => window.setTimeout(r, 1200)),
    ]);
  } catch {
    /* font API unavailable — use the fallback stack */
  }

  // ── Scene ────────────────────────────────────────────────
  const mount = document.getElementById('scene')!;
  const renderer = new THREE.WebGLRenderer({
    antialias: true,
    alpha: true,
    powerPreference: 'high-performance',
    stencil: false,
  });
  // Filmic look (the award-site baseline): ACES tone mapping + exposure.
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.12;
  let dprTier = 0;
  const DPR_STEPS = [
    Math.min(window.devicePixelRatio, 2),
    1.5,
    1.2,
    1,
  ].filter((v, i, a) => a.indexOf(v) === i);
  renderer.setPixelRatio(DPR_STEPS[dprTier]);
  renderer.setSize(window.innerWidth, window.innerHeight);
  mount.appendChild(renderer.domElement);

  const scene = new THREE.Scene();
  scene.fog = new THREE.FogExp2(0x0d0714, 0.0095);
  // HUD layer: every text nameplate lives here and renders AFTER the post
  // chain (see tick) — no bloom, no fog, no lights, no tone mapping.
  // Labels keep full brightness and razor edges at any orbit distance.
  const labelScene = new THREE.Scene();

  const camera = new THREE.PerspectiveCamera(
    55,
    window.innerWidth / window.innerHeight,
    0.1,
    400,
  );
  camera.position.set(0, 9.5, 36);

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.target.set(0, 1.5, 0);
  controls.enableDamping = true;
  controls.dampingFactor = 0.06;
  controls.minDistance = 9;
  controls.maxDistance = 80;
  controls.maxPolarAngle = 1.68;
  controls.autoRotate = !REDUCED;
  controls.autoRotateSpeed = 0.55;
  renderer.domElement.addEventListener('pointerdown', () => {
    controls.autoRotate = false;
  });

  // Post chain (the signature award-site look): bloom for the neon nodes,
  // OutputPass so tone mapping + sRGB apply inside the composer.
  // Small screens skip the chain — half the pixels, same scene.
  const BLOOM_OK = window.innerWidth >= 720 && !REDUCED;
  const composer = new EffectComposer(renderer);
  composer.addPass(new RenderPass(scene, camera));
  const bloom = new UnrealBloomPass(
    new THREE.Vector2(window.innerWidth, window.innerHeight),
    0.85, // strength
    0.55, // radius
    0.6, // threshold: only the emissive / bright bits bleed
  );
  if (BLOOM_OK) composer.addPass(bloom);
  composer.addPass(new OutputPass());

  scene.add(new THREE.HemisphereLight(0xc4b5fd, 0x1a0f2e, 0.95));
  const key = new THREE.DirectionalLight(0xffffff, 1.5);
  key.position.set(12, 20, 8);
  scene.add(key);
  const rim = new THREE.DirectionalLight(0xe879f9, 0.6);
  rim.position.set(-14, 4, -12);
  scene.add(rim);

  // Starfield.
  {
    const n = 1400;
    const pos = new Float32Array(n * 3);
    for (let k = 0; k < n; k++) {
      const r = 45 + Math.random() * 55;
      const th = Math.random() * Math.PI * 2;
      const ph = Math.acos(2 * Math.random() - 1);
      pos[k * 3] = r * Math.sin(ph) * Math.cos(th);
      pos[k * 3 + 1] = Math.abs(r * Math.cos(ph)) * 0.7 - 6;
      pos[k * 3 + 2] = r * Math.sin(ph) * Math.sin(th);
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
    const stars = new THREE.Points(
      geo,
      new THREE.PointsMaterial({
        color: 0xcbb8ff,
        size: 0.35,
        transparent: true,
        opacity: 0.8,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
      }),
    );
    stars.name = 'stars';
    scene.add(stars);
  }

  // Nebulae (animated-video-background feel, zero video files).
  const nebulae: THREE.Sprite[] = [];
  for (const [color, x, y, z, size, op] of [
    ['#7c3aed', -32, 10, -34, 55, 0.2],
    ['#ff4d6d', 30, 4, -30, 44, 0.15],
    ['#a855f7', 4, 16, -42, 60, 0.18],
    ['#e879f9', -8, -2, 30, 36, 0.1],
  ] as const) {
    const sp = glowSprite(color, size, op);
    sp.position.set(x, y, z);
    scene.add(sp);
    nebulae.push(sp);
  }

  // Floor grid.
  const grid = new THREE.GridHelper(90, 45, 0xa855f7, 0x3b1a5e);
  const gridMat = grid.material as THREE.Material;
  gridMat.transparent = true;
  gridMat.opacity = 0.26;
  grid.position.y = -4.5;
  scene.add(grid);

  // ── Manju mascot ─────────────────────────────────────────
  const mascot = new THREE.Group();
  const body = new THREE.Mesh(
    new THREE.SphereGeometry(1.5, 48, 32),
    new THREE.MeshStandardMaterial({ color: 0xfff3e2, roughness: 0.6 }),
  );
  body.scale.set(1, 0.82, 0.95);
  mascot.add(body);
  const face = new THREE.Mesh(
    new THREE.PlaneGeometry(1.3, 1.3),
    new THREE.MeshBasicMaterial({ map: manjuFace(), transparent: true }),
  );
  face.position.set(0, 0.18, 1.44);
  mascot.add(face);
  const halo = glowSprite('#e879f9', 5.2, 0.35);
  mascot.add(halo);
  mascot.position.y = 1.6;
  scene.add(mascot);

  // ── Subject nodes on two tilted rings ────────────────────
  // Wide radii (≈12 / ≈16 units between neighbors) so the forms + nameplates
  // never crowd each other, even with all 16 modules on screen. Each node is
  // a Group: the subject's function-themed FORM (wiki-src/shapes.ts — gear,
  // lens, diamond, chevron… assigned at build time, never a bare cube) plus
  // a white edge-outline accent that inherits the form's scale/rotation.
  const nodes: THREE.Group[] = [];
  const nodeMats: THREE.MeshStandardMaterial[] = [];
  const nodeBase: THREE.Vector3[] = [];
  const nodeLabels: THREE.Sprite[] = [];
  const nodeRings: THREE.LineLoop[] = [];
  const nodeById = new Map<string, THREE.Group>();
  const lines: THREE.Line[] = [];
  const nodeEdgeMat = new THREE.LineBasicMaterial({
    color: 0xffffff,
    transparent: true,
    opacity: 0.7,
  });
  const squareGeo = new THREE.BufferGeometry().setFromPoints([
    new THREE.Vector3(-1.55, 0, -1.55),
    new THREE.Vector3(1.55, 0, -1.55),
    new THREE.Vector3(1.55, 0, 1.55),
    new THREE.Vector3(-1.55, 0, 1.55),
  ]);
  const ringDefs = [
    { r: 15.5, y: 4.4, tilt: 0.12 },
    { r: 20.5, y: -1.4, tilt: -0.1 },
  ];
  data.subjects.forEach((sub, ix) => {
    const ring = ringDefs[ix % 2];
    const per = Math.ceil(data.subjects.length / 2);
    const slot = Math.floor(ix / 2);
    const a = (slot / per) * Math.PI * 2 + (ix % 2 ? Math.PI / per : 0);
    const pos = new THREE.Vector3(
      Math.cos(a) * ring.r,
      ring.y + Math.sin(a * 2) * ring.r * ring.tilt,
      Math.sin(a) * ring.r,
    );
    const color = new THREE.Color(sub.color);
    const mat = new THREE.MeshStandardMaterial({
      color,
      emissive: color,
      emissiveIntensity: 0.55,
      roughness: 0.55,
      metalness: 0.1,
      flatShading: true,
    });
    const grp = new THREE.Group();
    // Inner group carries the uniform form scale so the edge outline (built
    // at unit size from the same geometry) always matches the silhouette.
    const inner = new THREE.Group();
    inner.add(shapeOf(sub.shape).build(mat));
    const eg = shapeEdgeGeo(sub.shape);
    if (eg) inner.add(new THREE.LineSegments(eg, nodeEdgeMat));
    inner.scale.setScalar(1.35);
    grp.add(inner);
    grp.position.copy(pos);
    grp.userData.id = sub.id;
    scene.add(grp);
    nodes.push(grp);
    nodeMats.push(mat);
    nodeBase.push(pos.clone());
    nodeById.set(sub.id, grp);

    // Voxel orbit frame: a spinning pixel-square, tinted by the module's
    // Leiden community (graphify) so same-subsystem colors group at a glance.
    const orbit = new THREE.LineLoop(
      squareGeo,
      new THREE.LineBasicMaterial({
        color: new THREE.Color(commColor(sub.community)),
        transparent: true,
        opacity: 0.5,
      }),
    );
    orbit.position.copy(pos);
    orbit.rotation.x = Math.PI / 2.4;
    scene.add(orbit);
    nodeRings.push(orbit);

    // Nameplate leads with the module glyph (the same emoji that marks it
    // on the rail, static pages and 2D graph tooltips).
    const label = textSprite(sub.glyph ? `${sub.glyph} ${sub.title}` : sub.title, 1.15, sub.color);
    label.position.copy(pos).add(new THREE.Vector3(0, 2.05, 0));
    labelScene.add(label);
    nodeLabels.push(label);

    const geo = new THREE.BufferGeometry().setFromPoints([
      new THREE.Vector3(0, 1.6, 0),
      pos,
    ]);
    // Spoke tinted with the module's own color — the wire tells you whose
    // node it reaches even before the nameplate resolves.
    const link = new THREE.Line(
      geo,
      new THREE.LineBasicMaterial({
        color,
        transparent: true,
        opacity: 0.3,
      }),
    );
    scene.add(link);
    lines.push(link);
  });

  // ── Knowledge-graph edges between subject nodes ─────────────
  // The scene IS the graph: every cross-subject EXTRACTED/INFERRED edge
  // from subjects.json draws a tether between the two orbiting modules
  // (solid = EXTRACTED-dominant, dashed = INFERRED-dominant). Edges follow
  // the bobbing nodes every frame; hovering/selecting a module or tracing
  // a path brightens its edges.
  interface SubEdge {
    a: string;
    b: string;
    line: THREE.Line;
    dashed: boolean;
  }
  const subEdges: SubEdge[] = [];
  {
    const seen = new Set<string>();
    for (const sub of data.subjects) {
      for (const e of sub.edges ?? []) {
        if (!nodeById.has(sub.id) || !nodeById.has(e.to)) continue;
        const key = sub.id < e.to ? `${sub.id}|${e.to}` : `${e.to}|${sub.id}`;
        if (seen.has(key)) continue;
        seen.add(key);
        const geo = new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), new THREE.Vector3()]);
        const dashed = e.inferred > e.extracted;
        const line = new THREE.Line(
          geo,
          dashed
            ? new THREE.LineDashedMaterial({ color: 0xffffff, transparent: true, opacity: 0.12, dashSize: 0.45, gapSize: 0.3 })
            : new THREE.LineBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.16 }),
        );
        line.frustumCulled = false;
        scene.add(line);
        subEdges.push({ a: sub.id, b: e.to, line, dashed });
      }
    }
  }

  // Active traced path (module hops); brightens the chain's edges + nodes
  // until the next selection. Set by the panel's path tracer. pathSet is the
  // O(1) membership test the tick loop uses (was Array.includes).
  let pathHighlight: string[] = [];
  let pathSet = new Set<string>();
  let pathPairs = new Set<string>();
  function setPath(path?: string[]): void {
    pathHighlight = path ?? [];
    pathSet = new Set(pathHighlight);
    pathPairs = new Set();
    for (let i = 0; i + 1 < pathHighlight.length; i++) {
      const a = pathHighlight[i];
      const b = pathHighlight[i + 1];
      pathPairs.add(a < b ? `${a}|${b}` : `${b}|${a}`);
    }
  }

  // Voxel lanterns: ONE draw call (InstancedMesh, per-instance color)
  // instead of one sprite each. Basic material + bloom does the glowing.
  // Boxes, not spheres — drifting pixels that match the voxel scenery.
  const LANTERN_N = REDUCED ? 0 : 70;
  const lanternMesh = new THREE.InstancedMesh(
    new THREE.BoxGeometry(0.22, 0.22, 0.22),
    new THREE.MeshBasicMaterial({ toneMapped: false }),
    Math.max(LANTERN_N, 1),
  );
  lanternMesh.count = LANTERN_N;
  lanternMesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
  const lanternPos = new Float32Array(Math.max(LANTERN_N, 1) * 3);
  const lanternSpeed = new Float32Array(Math.max(LANTERN_N, 1));
  {
    const warm = new THREE.Color('#ffb37a');
    const pink = new THREE.Color('#e879f9');
    const dummy = new THREE.Object3D();
    for (let k = 0; k < LANTERN_N; k++) {
      lanternPos[k * 3] = (Math.random() - 0.5) * 60;
      lanternPos[k * 3 + 1] = -4 + Math.random() * 16;
      lanternPos[k * 3 + 2] = (Math.random() - 0.5) * 60;
      lanternSpeed[k] = 0.25 + Math.random() * 0.55;
      dummy.position.set(lanternPos[k * 3], lanternPos[k * 3 + 1], lanternPos[k * 3 + 2]);
      dummy.updateMatrix();
      lanternMesh.setMatrixAt(k, dummy.matrix);
      lanternMesh.setColorAt(k, k % 3 ? warm : pink);
    }
    lanternMesh.instanceMatrix.needsUpdate = true;
    if (lanternMesh.instanceColor) lanternMesh.instanceColor.needsUpdate = true;
  }
  scene.add(lanternMesh);

  // Select-burst pool: one Points cloud, 160 sparks reused forever.
  const BURST_N = 160;
  const burstPos = new Float32Array(BURST_N * 3).fill(-999);
  const burstVel = new Float32Array(BURST_N * 3);
  const burstLife = new Float32Array(BURST_N);
  const burstGeo = new THREE.BufferGeometry();
  burstGeo.setAttribute('position', new THREE.BufferAttribute(burstPos, 3));
  burstGeo.setAttribute('color', new THREE.BufferAttribute(new Float32Array(BURST_N * 3), 3));
  const burstPts = new THREE.Points(
    burstGeo,
    new THREE.PointsMaterial({
      size: 0.3,
      vertexColors: true,
      transparent: true,
      opacity: 0.95,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      toneMapped: false,
    }),
  );
  burstPts.frustumCulled = false;
  scene.add(burstPts);
  let burstCursor = 0;
  function spawnBurst(point: THREE.Vector3, colorHex: string): void {
    if (REDUCED) return;
    const c = new THREE.Color(colorHex);
    for (let k = 0; k < 40; k++) {
      const i = burstCursor;
      burstCursor = (burstCursor + 1) % BURST_N;
      burstPos[i * 3] = point.x;
      burstPos[i * 3 + 1] = point.y;
      burstPos[i * 3 + 2] = point.z;
      const th = Math.random() * Math.PI * 2;
      const ph = Math.acos(2 * Math.random() - 1);
      const sp = 3 + Math.random() * 5;
      burstVel[i * 3] = Math.sin(ph) * Math.cos(th) * sp;
      burstVel[i * 3 + 1] = Math.abs(Math.cos(ph)) * sp * 0.9 + 1.5;
      burstVel[i * 3 + 2] = Math.sin(ph) * Math.sin(th) * sp;
      burstLife[i] = 0.9 + Math.random() * 0.5;
      (burstGeo.getAttribute('color') as THREE.BufferAttribute).setXYZ(i, c.r, c.g, c.b);
    }
    (burstGeo.getAttribute('color') as THREE.BufferAttribute).needsUpdate = true;
  }

  // Selection beacon: square light prism + square pulse ring, recolored
  // per subject. 4-sided geometry keeps the voxel look; the prism is spun
  // 45° so its faces (not its edges) face the camera at rest.
  const beacon = new THREE.Mesh(
    new THREE.CylinderGeometry(0.45, 0.68, 7, 4, 1, true),
    new THREE.MeshBasicMaterial({
      color: 0xa855f7,
      transparent: true,
      opacity: 0.28,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      side: THREE.DoubleSide,
    }),
  );
  beacon.rotation.y = Math.PI / 4;
  beacon.visible = false;
  scene.add(beacon);
  const pulseGeo = new THREE.RingGeometry(1.0, 1.3, 4);
  pulseGeo.rotateZ(Math.PI / 4); // diamond → axis-aligned pixel square
  const pulse = new THREE.Mesh(
    pulseGeo,
    new THREE.MeshBasicMaterial({
      color: 0xffffff,
      transparent: true,
      opacity: 0.5,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      side: THREE.DoubleSide,
    }),
  );
  pulse.visible = false;
  pulse.rotation.x = -Math.PI / 2;
  scene.add(pulse);

  // Shooting stars: one streak every few seconds.
  const shooters: { sp: THREE.Sprite; vel: THREE.Vector3; life: number }[] = [];
  let nextShooter = 3;
  function spawnShooter(now: number): void {
    const sp = glowSprite('#f5f3ff', 2.6, 0.9);
    sp.position.set((Math.random() - 0.5) * 70, 18 + Math.random() * 10, -30 - Math.random() * 20);
    sp.scale.set(7, 1.1, 1);
    scene.add(sp);
    shooters.push({
      sp,
      vel: new THREE.Vector3(-(14 + Math.random() * 10), -5 - Math.random() * 3, 0),
      life: now + 1.1,
    });
    nextShooter = now + 2.5 + Math.random() * 4;
  }

  // Mascot blink state (squash-and-stretch, no texture swap needed).
  let blinkAt = 2.5;

  // ── Satellites: mini stars orbiting the selected node ──────────
  // Real connections only: linked modules (graphify EXTRACTED/INFERRED
  // edges when the graph ran at build time, mined import + shared-signal
  // links as heuristic fallback) and the module's own top symbols
  // (★ god nodes first — the AST's most-connected hubs). Nothing here
  // is generic — every satellite resolves to project data.
  interface Sat {
    mesh: THREE.Group;
    mat: THREE.MeshStandardMaterial;
    label: THREE.Sprite;
    link: THREE.Line;
    linkPos: THREE.BufferAttribute;
    angle: number;
    radius: number;
    speed: number;
    yOff: number;
    kind: 'symbol' | 'link';
    subId: string;
    symName: string;
    tip: string;
    god: boolean;
    inferred: boolean;
    mag: THREE.Vector3;
  }
  let sats: Sat[] = [];
  // Raycast targets, rebuilt only when nodes/sats change — never per frame
  // and never per pick (the old code allocated a fresh array every pointer
  // move).
  let pickTargets: THREE.Object3D[] = [...nodes];
  function refreshPickTargets(): void {
    pickTargets = sats.length ? [...sats.map((s) => s.mesh), ...nodes] : [...nodes];
  }

  function clearSats(): void {
    for (const s of sats) {
      scene.remove(s.mesh);
      labelScene.remove(s.label);
      scene.remove(s.link);
      s.mat.dispose();
      // Label textures live in the plateCache memo — disposing the shared
      // map here would blank every other sprite that reuses the canvas.
      (s.label.material as THREE.Material).dispose();
      (s.link.material as THREE.Material).dispose();
      s.link.geometry.dispose();
    }
    sats = [];
    refreshPickTargets();
  }

  function buildSats(sub: Subject): void {
    clearSats();
    const items: {
      kind: 'symbol' | 'link';
      subId: string;
      symName: string;
      label: string;
      color: string;
      tip: string;
      god: boolean;
      inferred: boolean;
      shapeKey: string;
    }[] = [];
    const edgeByTo = new Map((sub.edges ?? []).map((e) => [e.to, e]));
    const linkIds = (sub.edges?.length ? sub.edges.map((e) => e.to) : sub.links).slice(0, 4);
    for (const lid of linkIds) {
      const l = byId.get(lid);
      if (!l || lid === sub.id) continue;
      const e = edgeByTo.get(lid);
      items.push({
        kind: 'link', subId: lid, symName: '', label: l.glyph ? `${l.glyph} ${l.title}` : l.title, color: l.color,
        god: false,
        inferred: !!e && e.inferred > e.extracted,
        // Link satellites take the TARGET module's form — the silhouette
        // itself says where the tether leads.
        shapeKey: l.shape ?? 'facet',
        tip: e
          ? `${l.title} — w${e.weight}, EXTRACTED ${e.extracted} / INFERRED ${e.inferred} (${Object.entries(e.relations).map(([r, n]) => `${r} ×${n}`).join(', ')})`
          : `${l.title} — ${l.tag}`,
      });
    }
    // ★ god nodes: the AST's most-connected hubs inside this module —
    // always the star form, so hubs are recognizable at any orbit distance.
    for (const g of sub.godNodes ?? []) {
      if (items.length >= 10) break;
      if (items.some((i) => i.symName === g.name)) continue;
      items.push({
        kind: 'symbol', subId: sub.id, symName: g.name, label: `★${g.name}`, color: '#f5f3ff',
        god: true, inferred: false, shapeKey: 'star',
        tip: `★ god node — ${g.name} (${g.file}:${g.line}, degree ${g.degree})`,
      });
    }
    for (const s of sub.top) {
      if (items.length >= 10) break;
      if (items.some((i) => i.symName === s.name)) continue;
      items.push({
        kind: 'symbol', subId: sub.id, symName: s.name, label: s.name, color: sub.color,
        god: false, inferred: false,
        // Symbol satellites take the form of what they ARE (cursor = fn,
        // globe = class, hex = module item…) via KIND_SHAPE.
        shapeKey: KIND_SHAPE[s.kind] ?? 'facet',
        tip: s.docs ? `${s.name} — ${s.docs}` : `${s.name} (${s.kind}) — ${s.file}:${s.line}`,
      });
    }
    items.forEach((it, k) => {
      const color = new THREE.Color(it.color);
      const mat = new THREE.MeshStandardMaterial({
        color,
        emissive: color,
        emissiveIntensity: 0.9,
        roughness: 0.5,
        flatShading: true,
      });
      const mesh = new THREE.Group();
      const body = shapeOf(it.shapeKey).build(mat, it.god ? 0.85 : 0.55);
      mesh.add(body);
      mesh.userData = { sat: true, kind: it.kind, subId: it.subId, symName: it.symName };
      scene.add(mesh);
      const label = textSprite(it.label, it.god ? 0.72 : 0.62, it.color);
      labelScene.add(label);
      const linkPos = new THREE.BufferAttribute(new Float32Array(6), 3);
      const linkGeo = new THREE.BufferGeometry();
      linkGeo.setAttribute('position', linkPos);
      // Tether style carries the provenance: solid = EXTRACTED-dominant,
      // dashed = INFERRED-dominant (LineDashedMaterial needs line distances
      // refreshed as the satellite orbits — done in the loop below).
      const link = new THREE.Line(
        linkGeo,
        it.inferred
          ? new THREE.LineDashedMaterial({ color, transparent: true, opacity: 0.32, dashSize: 0.35, gapSize: 0.25 })
          : new THREE.LineBasicMaterial({ color, transparent: true, opacity: 0.4 }),
      );
      scene.add(link);
      sats.push({
        mesh, mat, label, link, linkPos,
        angle: (k / Math.max(items.length, 1)) * Math.PI * 2,
        radius: 2.7 + (k % 3) * 0.6,
        speed: (0.25 + (k % 4) * 0.06) * (k % 2 ? 1 : -1),
        yOff: ((k * 37) % 20) / 10 - 1,
        kind: it.kind, subId: it.subId, symName: it.symName, tip: it.tip,
        god: it.god, inferred: it.inferred,
        mag: new THREE.Vector3(),
      });
    });
    refreshPickTargets();
  }

  // ── Reader UI ────────────────────────────────────────────
  let selected: string | null = null;
  let selectedSym: string | null = null;
  let panelToken = 0; // guards async deep-view fills against stale renders
  const focusTarget = new THREE.Vector3(0, 1.5, 0);

  // Cinematic flight: eased camera glide (easeInOutCubic) instead of a
  // linear lerp, plus a subtle FOV punch while travelling. Cancelled the
  // moment the user grabs the camera.
  interface Flight {
    fromPos: THREE.Vector3;
    toPos: THREE.Vector3;
    t0: number;
    dur: number;
  }
  let fly: Flight | null = null;

  function easeInOutCubic(x: number): number {
    return x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2;
  }

  function startFly(point: THREE.Vector3): void {
    if (REDUCED) return;
    const dir = point.clone().sub(new THREE.Vector3(0, 1.5, 0));
    if (dir.lengthSq() < 0.01) dir.set(0, 0.35, 1);
    dir.normalize();
    const goal = point
      .clone()
      .add(dir.multiplyScalar(9.5))
      .add(new THREE.Vector3(0, 3.2, 0));
    fly = { fromPos: camera.position.clone(), toPos: goal, t0: performance.now(), dur: 1400 };
  }

  function startFlyHome(): void {
    if (REDUCED) return;
    fly = {
      fromPos: camera.position.clone(),
      toPos: new THREE.Vector3(0, 9.5, 36),
      t0: performance.now(),
      dur: 1400,
    };
  }

  function renderRail(filter = ''): void {
    const q = filter.trim().toLowerCase();
    rail.innerHTML = '';
    for (const sub of data.subjects) {
      if (
        q &&
        !sub.title.toLowerCase().includes(q) &&
        !sub.tag.toLowerCase().includes(q) &&
        !sub.id.includes(q) &&
        !sub.top.some((t) => t.name.toLowerCase().includes(q)) &&
        !(sub.godNodes ?? []).some((g) => g.name.toLowerCase().includes(q)) &&
        !sub.contracts.some((c) => c.toLowerCase().includes(q))
      ) {
        continue;
      }
      const b = document.createElement('button');
      b.style.setProperty('--dot', sub.color);
      if (sub.id === selected) b.classList.add('on');
      // The dot takes the module's 2D silhouette (same clip-path family as
      // the static pages) and the title leads with the glyph.
      b.innerHTML = `<span class="dot" style="clip-path:${shapeClip(sub.shape)}"></span><span>${esc(
        sub.glyph ? `${sub.glyph} ${sub.title}` : sub.title,
      )}</span><span class="meta">${sub.fileCount}f</span>`;
      b.addEventListener('click', () => {
        window.location.hash = `#/${sub.id}`;
      });
      rail.appendChild(b);
    }
    if (!rail.children.length) {
      rail.innerHTML = '<p class="empty" style="padding:8px">No modules match.</p>';
    }
  }

  function symbolMatches(q: string, cap: number): { sub: Subject; sym: TopSymbol }[] {
    const out: { sub: Subject; sym: TopSymbol }[] = [];
    if (!q) return out;
    for (const sub of data.subjects) {
      for (const g of sub.godNodes ?? []) {
        if (g.name.toLowerCase().includes(q)) {
          out.push({
            sub,
            sym: { name: g.name, kind: 'god node', docs: `★ degree ${g.degree}`, file: g.file, line: g.line },
          });
          if (out.length >= cap) return out;
        }
      }
      for (const sym of sub.top) {
        if (sym.name.toLowerCase().includes(q)) {
          out.push({ sub, sym });
          if (out.length >= cap) return out;
        }
      }
    }
    return out;
  }

  function renderPanel(): void {
    const q = search.value.trim().toLowerCase();
    if (!selected || !byId.has(selected)) {
      const totalFiles = data.subjects.reduce((a, s) => a + s.fileCount, 0);
      const totalSyms = data.subjects.reduce((a, s) => a + s.symbolCount, 0);
      panel.style.setProperty('--ac', '#a855f7');
      document.title = 'CyberManju OS — 3D wiki';
      const hits = symbolMatches(q, 12);
      const hitsHtml = hits.length
        ? `<h2>Symbol matches</h2><div class="chips matches">${hits
            .map(
              ({ sub, sym }) =>
                `<button data-gosym="${sub.id}/${encodeURIComponent(sym.name)}" title="${esc(sym.file)}:${sym.line}${sym.docs ? ` — ${esc(sym.docs)}` : ''}"><code>${esc(sym.name)}</code><span>${esc(sub.title)}</span></button>`,
            )
            .join('')}</div>`
        : '';
      // Knowledge-graph summary (only when graphify ran at build time —
      // never claim a graph that isn't there). Communities are the Leiden
      // clusters; the orbit-frame color of each module matches its chip.
      const g = data.graph;
      const legendHtml = g
        ? `<div class="chips legend">${g.communities
            .map(
              (c) =>
                `<span class="comm" title="community #${c.id} — ${c.size} symbols (graphify Leiden)"><i style="background:${commColor(c.id)}"></i>${c.subjects
                  .map((s) => {
                    const t = byId.get(s);
                    return t?.glyph ? `${t.glyph} ${t.title}` : (t?.title ?? s);
                  })
                  .join(' + ')}</span>`,
            )
            .join('')}</div>`
        : '';
      const graphHtml = g
        ? `<h2>Knowledge graph <span class="mut">engine: ${esc(g.engine)} · solid = EXTRACTED · dashed = INFERRED</span></h2>
        <div class="stats"><span>${g.stats.nodes} nodes</span><span>${g.stats.edges} edges</span><span>EXTRACTED ${g.stats.extracted} / INFERRED ${g.stats.inferred}</span></div>
        <h2>★ God nodes <span class="mut">most-connected hubs</span></h2>
        <div class="chips matches">${g.godNodes
          .slice(0, 8)
          .map((n) => {
            const s = n.subject ? byId.get(n.subject) : undefined;
            // Clicking a hub jumps straight into its deep view (source +
            // explanation), not just the module shell.
            return `<button ${s ? `data-gosym="${s.id}/${encodeURIComponent(n.name)}"` : 'disabled'} title="${esc(n.file)}:${n.line} — degree ${n.degree}"><code>★${esc(n.name)}</code></button>`;
          })
          .join('')}</div>
        <h2>Communities <span class="mut">orbit-frame colors</span></h2>
        ${legendHtml}
        <ul class="links"><li><button class="linkbtn" id="open2d">switch to 2D graph</button></li><li><a href="./graph.html">graph.html direct</a></li></ul>`
        : '';
      panel.innerHTML = `
        <div class="tag">quantum-resistant file manager + OS layer · v${esc(data.version)}</div>
        <h1>CyberManju wiki</h1>
        <p class="blurb">Orbit the manju — every glowing module is a part of the system.
        Click one (or pick from the rail) to read what it owns, what it exposes,
        and how it plugs into the whole.</p>
        <div class="stats"><span>${data.subjects.length} subjects</span><span>${totalFiles} files</span><span>${totalSyms} symbols</span>${g?.commit ? `<span>graph @ ${esc(g.commit.slice(0, 7))}</span>` : ''}</div>
        ${hitsHtml}
        ${graphHtml}
        <h2>For agents</h2>
        <ul class="links"><li><a href="../llms.txt">llms.txt</a></li><li><a href="./overview.txt">overview.txt</a></li></ul>
        <h2>For humans</h2>
        <ul class="links"><li><a href="./overview.html">static pages</a></li><li><a href="../">open the app</a></li></ul>
        <p class="keys"><kbd>/</kbd> search · <kbd>↑↓</kbd> move · <kbd>↵</kbd> open · <kbd>esc</kbd> overview</p>`;
      panel.querySelectorAll('button[data-go]').forEach((b) => {
        b.addEventListener('click', () => {
          window.location.hash = `#/${(b as HTMLButtonElement).dataset.go}`;
        });
      });
      panel.querySelectorAll('button[data-gosym]').forEach((b) => {
        b.addEventListener('click', () => {
          window.location.hash = `#/${(b as HTMLButtonElement).dataset.gosym}`;
        });
      });
      panel.querySelector('button#open2d')?.addEventListener('click', () => set2D(true));
      return;
    }
    const sub = byId.get(selected)!;
    const ix = data.subjects.findIndex((s) => s.id === sub.id);
    const prev = data.subjects[(ix - 1 + data.subjects.length) % data.subjects.length];
    const next = data.subjects[(ix + 1) % data.subjects.length];
    panel.style.setProperty('--ac', sub.color);
    const pageUrl = `${window.location.origin}${window.location.pathname}#/${sub.id}`;

    // Symbol deep view: the satellite's docs, kind, and exact file:line.
    // God nodes resolve too (they may sit outside the top-symbols slice).
    const topHit = selectedSym ? sub.top.find((s) => s.name === selectedSym) : undefined;
    const godHit = selectedSym ? (sub.godNodes ?? []).find((g) => g.name === selectedSym) : undefined;
    const symHit = topHit
      ? { ...topHit }
      : godHit
        ? {
            name: godHit.name,
            kind: 'god node (graphify)',
            docs: `Most-connected hub of ${sub.title} — degree ${godHit.degree}, community #${godHit.community ?? sub.community ?? '?'}.`,
            file: godHit.file,
            line: godHit.line,
          }
        : undefined;
    if (symHit) {
      document.title = `${symHit.name} — ${sub.title} — CyberManju wiki`;
      const symUrl = `${pageUrl}/${encodeURIComponent(symHit.name)}`;
      panel.innerHTML = `
      <div class="crumbs"><button data-go="">wiki</button><span>/</span><button data-go="${sub.id}">${esc(sub.title)}</button><span>/</span><b>${esc(symHit.name)}</b>
        <button class="copy" data-copy="${esc(symUrl)}" title="Copy link to this symbol">copy link</button></div>
      <div class="tag">${esc(symHit.kind)} · ${esc(sub.tag)}</div>
      <h1>${esc(symHit.name)}</h1>
      <p class="blurb">${symHit.docs ? esc(symHit.docs) : 'No doc-comment on this item — the code below is the documentation.'}</p>
      <div class="stats"><span>${esc(symHit.file)}:${symHit.line}</span><span>v${esc(data.version)}</span></div>
      <h2>How it works <span class="mut">auto explanation · engine: heuristic</span></h2>
      <div class="explain" id="symExplain">loading explanation…</div>
      <h2>Source <span class="mut">real repo code</span></h2>
      <pre class="code" id="symCode">loading source…</pre>
      <h2>Connections <span class="mut">graphify symbol edges</span></h2>
      <div class="chips matches" id="symConns"><code>loading…</code></div>
      <h2>Navigate</h2>
      <ul class="links">
        <li><button data-go="${sub.id}" class="linkbtn">← ${esc(sub.title)} module</button></li>
        <li><a href="./${sub.id}.txt">full ${esc(sub.id)}.txt</a></li>
      </ul>
      <div class="pager">
        <button data-go="${prev.id}">← ${esc(prev.title)}</button>
        <button data-go="${next.id}">${esc(next.title)} →</button>
      </div>`;
      wirePanelButtons(pageUrl);
      // Fill code + explanation + connections from the lazily fetched
      // payload (token-guarded so a fast second click can't be overwritten).
      const token = ++panelToken;
      void loadSymbols(sub).then((store) => {
        if (token !== panelToken || !panel.isConnected) return;
        const entry = store.get(symHit.name);
        const explainEl = panel.querySelector('#symExplain');
        const codeEl = panel.querySelector('#symCode');
        const connsEl = panel.querySelector('#symConns');
        if (!entry) {
          if (explainEl) {
            explainEl.innerHTML =
              'No symbol payload for this item — run <code>npm run wiki:graph && npm run wiki</code> so CI can attach the code + graph edges.';
          }
          if (codeEl) codeEl.textContent = `// source not bundled — open ${symHit.file}:${symHit.line}`;
          if (connsEl) connsEl.innerHTML = '<code>no symbol payload</code>';
          return;
        }
        if (explainEl) explainEl.innerHTML = esc(entry.explanation).replace(/`([^`]+)`/g, '<code>$1</code>').replace(/_([^_]+)_/g, '<i>$1</i>');
        if (codeEl) codeEl.textContent = entry.code || `// no body extracted — see ${entry.file}:${entry.line}`;
        if (connsEl) {
          const conns = entry.connections ?? [];
          if (!conns.length) {
            connsEl.innerHTML = '<code>no graph edges for this symbol</code>';
          } else {
            connsEl.innerHTML = conns
              .map((c) => {
                const inSubject = store.has(c.name);
                const badge = `<span class="badge${c.confidence === 'INFERRED' ? ' inf' : ''}" title="${esc(c.confidence)} w${c.weight}">${esc(c.via === 'in' ? `← ${c.relation}` : `→ ${c.relation}`)}</span>`;
                const tip = `${c.name} — ${c.file}:${c.line} · ${c.relation} · ${c.confidence} w${c.weight}`;
                if (inSubject) {
                  return `<button data-sym="${esc(c.name)}" title="${esc(tip)}"><code>${esc(c.name)}</code>${badge}</button>`;
                }
                // Cross-module edge: resolve the neighbor's subject by its
                // file path — the chip becomes a clickable jump to the other
                // module (its own glyph + form), never a dead label.
                const otherId = subjectOfPath(c.file, data.subjects);
                const other = otherId ? byId.get(otherId) : undefined;
                if (other && otherId !== sub.id) {
                  const g2 = other.glyph ?? '✦';
                  return `<button data-gosym="${otherId}/${encodeURIComponent(c.name)}" title="${esc(tip)}"><code>${esc(g2)} ${esc(c.name)}</code>${badge}</button>`;
                }
                return `<span title="${esc(tip)}"><code>${esc(c.name)}</code>${badge}</span>`;
              })
              .join('');
            connsEl.querySelectorAll('button[data-sym]').forEach((b) => {
              b.addEventListener('click', () => {
                window.location.hash = `#/${sub.id}/${encodeURIComponent((b as HTMLButtonElement).dataset.sym ?? '')}`;
              });
            });
            connsEl.querySelectorAll('button[data-gosym]').forEach((b) => {
              b.addEventListener('click', () => {
                window.location.hash = `#/${(b as HTMLButtonElement).dataset.gosym}`;
              });
            });
          }
        }
      });
      return;
    }

    document.title = `${sub.title} — CyberManju wiki`;
    const syms = sub.top
      .slice(0, 24)
      .map(
        (s) =>
          `<button class="symbtn" data-sym="${esc(s.name)}" title="${esc(s.file)}:${s.line}${s.docs ? ` — ${esc(s.docs)}` : ''}"><code>${esc(s.name)}</code></button>`,
      )
      .join('');
    // Connected modules: graphify edges carry weight + EXTRACTED/INFERRED
    // provenance in the tooltip and badge; heuristic links stay bare.
    const edgeByTo = new Map((sub.edges ?? []).map((e) => [e.to, e]));
    const linkIds = sub.edges?.length ? sub.edges.map((e) => e.to) : sub.links;
    const linksHtml = linkIds.length
      ? `<h2>Connected modules ${sub.engine === 'graphify' ? '<span class="mut">solid = EXTRACTED · dashed = INFERRED</span>' : ''}</h2><div class="chips matches">${linkIds
          .map((lid) => {
            const l = byId.get(lid);
            const e = edgeByTo.get(lid);
            if (!l) return '';
            const tip = e
              ? `w${e.weight} · EXTRACTED ${e.extracted} / INFERRED ${e.inferred} (${Object.entries(e.relations).map(([r, n]) => `${r} ×${n}`).join(', ')})${e.sample ? `\n${e.sample}` : ''}`
              : l.tag;
            const badge = e ? `<span class="badge${e.inferred > e.extracted ? ' inf' : ''}">w${e.weight}</span>` : '';
            return `<button data-go="${l.id}" title="${esc(tip)}"><code>${esc(l.glyph ? `${l.glyph} ${l.title}` : l.title)}</code>${badge}</button>`;
          })
          .join('')}</div>`
      : '';
    const godsHtml = sub.godNodes?.length
      ? `<h2>★ God nodes <span class="mut">most-connected hubs (graphify)</span></h2><div class="chips">${sub.godNodes
          .map(
            (n) =>
              `<button class="symbtn" data-sym="${esc(n.name)}" title="${esc(n.file)}:${n.line} — degree ${n.degree}"><code>★${esc(n.name)}</code></button>`,
          )
          .join('')}</div>`
      : '';
    // Path tracer: precomputed all-pairs module hops from the graph.
    const traceHtml = data.graph
      ? `<h2>Path trace <span class="mut">graphify hops, undirected</span></h2>
      <div class="trace">
        <select id="traceTo" aria-label="Trace path to module">${data.subjects
          .filter((o) => o.id !== sub.id)
          .map((o) => `<option value="${o.id}">${esc(o.title)}</option>`)
          .join('')}</select>
        <button id="traceGo">trace</button>
      </div>
      <div id="traceOut" class="traced"></div>`
      : '';
    panel.innerHTML = `
      <div class="crumbs"><button data-go="">wiki</button><span>/</span><b>${esc(sub.title)}</b>
        <button class="copy" data-copy="${esc(pageUrl)}" title="Copy link to this module">copy link</button></div>
      <div class="tag">${esc(sub.tag)}${sub.engine === 'graphify' ? ' · graph: graphify' : ''}</div>
      <h1>${esc(sub.title)}</h1>
      <p class="blurb">${esc(sub.blurb)}</p>
      ${sub.shape && sub.shapeWhy ? `<p class="formnote">${esc(sub.glyph ?? '✦')} form <code>${esc(sub.shape)}</code> — ${esc(sub.shapeWhy)}</p>` : ''}
      <div class="stats"><span>${sub.fileCount} files</span><span>${sub.symbolCount} symbols</span>${sub.community != null ? `<span>community #${sub.community}</span>` : ''}<span>v${esc(data.version)}</span></div>
      ${linksHtml}
      ${godsHtml}
      <h2>Top symbols</h2><div class="chips symchips">${syms}</div>
      <h2>Contracts</h2><div class="chips">${sub.contracts.length ? sub.contracts.map((c) => `<code>${esc(c)}</code>`).join('') : '<code>none — pure logic</code>'}</div>
      ${traceHtml}
      <h2>Read offline</h2>
      <ul class="links">
        <li><a href="./${sub.id}.txt">.txt for LLMs</a></li>
        <li><a href="./${sub.id}.json">.json</a></li>
        <li><a href="./${sub.id}.html">static page</a></li>
        ${data.graph ? '<li><a href="./graph.html">graph.html</a></li>' : ''}
      </ul>
      <div class="pager">
        <button data-go="${prev.id}">← ${esc(prev.title)}</button>
        <button data-go="${next.id}">${esc(next.title)} →</button>
      </div>`;
    wirePanelButtons(pageUrl);
    wireTrace(sub);
  }

  // Panel-internal navigation: module buttons, symbol buttons, copy-link.
  function wirePanelButtons(pageUrl: string): void {
    panel.querySelectorAll('button[data-go]').forEach((b) => {
      const go = (b as HTMLButtonElement).dataset.go;
      b.addEventListener('click', () => {
        window.location.hash = go ? `#/${go}` : '#/';
      });
    });
    panel.querySelectorAll('button[data-sym]').forEach((b) => {
      const sym = (b as HTMLButtonElement).dataset.sym;
      b.addEventListener('click', () => {
        if (selected && sym) window.location.hash = `#/${selected}/${encodeURIComponent(sym)}`;
      });
    });
    const copy = panel.querySelector('button[data-copy]');
    if (copy) {
      copy.addEventListener('click', async () => {
        const url = (copy as HTMLButtonElement).dataset.copy ?? pageUrl;
        try {
          await navigator.clipboard.writeText(url);
          copy.textContent = 'copied ✓';
        } catch {
          copy.textContent = url;
        }
        window.setTimeout(() => {
          if (copy.isConnected) copy.textContent = 'copy link';
        }, 1600);
      });
    }
  }

  // Path tracer: renders the precomputed shortest module chain as clickable
  // chips (graphify's subject graph, undirected hops).
  function wireTrace(sub: Subject): void {
    const go = panel.querySelector<HTMLButtonElement>('#traceGo');
    const toSel = panel.querySelector<HTMLSelectElement>('#traceTo');
    const out = panel.querySelector<HTMLElement>('#traceOut');
    if (!go || !toSel || !out) return;
    go.addEventListener('click', () => {
      const to = toSel.value;
      const path = data.graph?.paths[`${sub.id}>${to}`];
      const target = byId.get(to);
      if (!path || !target) {
        setPath();
        out.innerHTML = '<p class="empty">No path — different graph components.</p>';
        return;
      }
      // Light the chain up in the 3D scene while rendering it in the panel.
      setPath(path);
      out.innerHTML = path
        .map((id) => {
          const s = byId.get(id);
          return s ? `<button data-go="${s.id}" class="linkbtn">${esc(s.title)}</button>` : '';
        })
        .join('<span class="arrow"> → </span>');
      out.querySelectorAll('button[data-go]').forEach((b) => {
        b.addEventListener('click', () => {
          window.location.hash = `#/${(b as HTMLButtonElement).dataset.go}`;
        });
      });
    });
  }

  function select(id: string | null, sym: string | null = null): void {
    selected = id && byId.has(id) ? id : null;
    hoveredSat = null;
    setPath(); // any traced path dies with the next navigation
    const sub = selected ? byId.get(selected) : undefined;
    // Accept top symbols AND god nodes as deep-view targets (satellite
    // clicks land here either way; god hubs may sit outside top).
    selectedSym =
      sub && sym
        ? sub.top.some((s) => s.name === sym) || (sub.godNodes ?? []).some((g) => g.name === sym)
          ? sym
          : null
        : null;
    renderRail(search.value);
    renderPanel();
    clearSats();
    const mesh = selected ? nodeById.get(selected) : undefined;
    if (mesh && sub) {
      focusTarget.copy(mesh.position);
      startFly(mesh.position);
      buildSats(sub);
      void loadSymbols(sub); // prefetch source blocks so satellite clicks open instantly
      beacon.visible = true;
      pulse.visible = true;
      beacon.position.copy(mesh.position).add(new THREE.Vector3(0, 1.2, 0));
      pulse.position.copy(mesh.position).add(new THREE.Vector3(0, -1.4, 0));
      const c = new THREE.Color(sub.color);
      (beacon.material as THREE.MeshBasicMaterial).color.copy(c);
      (pulse.material as THREE.MeshBasicMaterial).color.copy(c);
      spawnBurst(mesh.position, sub.color);
    } else {
      focusTarget.set(0, 1.5, 0);
      beacon.visible = false;
      pulse.visible = false;
      startFlyHome();
    }
  }

  function route(): void {
    const m = window.location.hash.match(/^#\/([\w-]+)(?:\/([^/]+))?/);
    select(m ? m[1] : null, m && m[2] ? decodeURIComponent(m[2]) : null);
  }
  window.addEventListener('hashchange', route);

  search.addEventListener('input', () => {
    renderRail(search.value);
    // Live symbol matches surface in the overview panel.
    if (!selected) renderPanel();
  });
  search.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      const first = rail.querySelector('button');
      if (first) (first as HTMLButtonElement).click();
    }
  });

  // Global keys: `/` focuses search, arrows walk the rail, Esc goes home.
  // (Skipped while typing anywhere except the search box itself.)
  window.addEventListener('keydown', (e) => {
    const tag = (e.target as HTMLElement | null)?.tagName;
    // Esc exits the 2D graph overlay before it does anything else.
    if (e.key === 'Escape' && twoD) {
      set2D(false);
      return;
    }
    if (e.key === '/' && tag !== 'INPUT' && tag !== 'TEXTAREA') {
      e.preventDefault();
      search.focus();
      return;
    }
    if (tag === 'INPUT' || tag === 'TEXTAREA') return;
    const order = [...rail.querySelectorAll('button')].map(
      (b) => b.textContent ?? '',
    );
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const ids = data.subjects
        .filter((s) => order.some((t) => t.includes(s.title)))
        .map((s) => s.id);
      const pool = ids.length ? ids : data.subjects.map((s) => s.id);
      let ix = pool.indexOf(selected ?? '');
      ix = e.key === 'ArrowDown' ? (ix + 1) % pool.length : (ix - 1 + pool.length) % pool.length;
      window.location.hash = `#/${pool[ix]}`;
    } else if (e.key === 'Escape') {
      if (selectedSym && selected) window.location.hash = `#/${selected}`;
      else if (window.location.hash) window.location.hash = '#/';
    }
  });

  // ── Picking + magnetic hover ─────────────────────────────
  // Satellites and module nodes are MAGNETIC: within MAGNET_R world units
  // of the cursor ray they lean toward the cursor, glow up, and become
  // clickable by screen proximity (PICK_PX) — not only by exact face hit,
  // so the small orbiting sub-files stay easy to grab. Dragging the camera
  // and prefers-reduced-motion both disable the pull.
  const ray = new THREE.Raycaster();
  const ptr = new THREE.Vector2();
  const ptrNdc = new THREE.Vector2();
  let ptrActive = false;
  let hovered: string | null = null;
  let hoveredSat: Sat | null = null;
  let downAt: [number, number] | null = null;
  const MAGNET_R = 2.6;
  const PICK_PX = 56;
  const nodeMag: THREE.Vector3[] = nodeBase.map(() => new THREE.Vector3());
  const magTmp = new THREE.Vector3();
  const magDelta = new THREE.Vector3();
  const mpos = new THREE.Vector3(); // caller scratch — never magnetPull's
  const nearV = new THREE.Vector3();

  // Pull `out` toward the closest point of the cursor ray; returns 0..1
  // proximity strength (1 = cursor ray passes through the object).
  function magnetPull(worldPos: THREE.Vector3, out: THREE.Vector3): number {
    out.set(0, 0, 0);
    if (REDUCED || !ptrActive || downAt) return 0;
    ray.setFromCamera(ptrNdc, camera);
    const origin = ray.ray.origin;
    const dir = ray.ray.direction;
    const t = Math.max(0, magTmp.copy(worldPos).sub(origin).dot(dir));
    magTmp.copy(dir).multiplyScalar(t).add(origin);
    magDelta.copy(magTmp).sub(worldPos);
    const dist = magDelta.length();
    if (dist >= MAGNET_R || dist < 1e-4) return dist >= MAGNET_R ? 0 : 1;
    const strength = 1 - dist / MAGNET_R;
    out.copy(magDelta.normalize().multiplyScalar(Math.min(0.7, dist * 0.55) * strength));
    return strength;
  }

  interface PickHit {
    kind: 'subject' | 'symbol' | 'link';
    subId: string;
    symName: string;
  }

  // Screen-proximity fallback when the exact raycast misses.
  function pickNear(px: number, py: number): PickHit | null {
    let bestD = PICK_PX;
    let best: PickHit | null = null;
    const consider = (obj: THREE.Object3D, hit: PickHit): void => {
      nearV.copy(obj.position).project(camera);
      if (nearV.z > 1 || nearV.z < -1) return;
      const sx = (nearV.x * 0.5 + 0.5) * window.innerWidth;
      const sy = (-nearV.y * 0.5 + 0.5) * window.innerHeight;
      const d = Math.hypot(sx - px, sy - py);
      if (d < bestD) {
        bestD = d;
        best = hit;
      }
    };
    for (const s of sats) {
      consider(s.mesh, { kind: s.kind, subId: s.subId, symName: s.symName });
    }
    for (const m of nodes) {
      consider(m, { kind: 'subject', subId: (m.userData.id as string) ?? '', symName: '' });
    }
    return best;
  }

  // Walk up from a hit child (shape mesh) to the Group that carries the
  // userData payload (node id / satellite descriptor).
  function pickOwner(obj: THREE.Object3D): THREE.Object3D {
    let o: THREE.Object3D | null = obj;
    while (o && !o.userData?.id && !o.userData?.sat) o = o.parent;
    return o ?? obj;
  }

  function pick(ev: { clientX: number; clientY: number }): PickHit | null {
    ptr.x = (ev.clientX / window.innerWidth) * 2 - 1;
    ptr.y = -(ev.clientY / window.innerHeight) * 2 + 1;
    ray.setFromCamera(ptr, camera);
    // Recursive raycast against the cached target list: forms are Groups
    // with mesh children, so the hit lands on a child and walks up.
    const hit = ray.intersectObjects(pickTargets, true)[0];
    if (!hit) return pickNear(ev.clientX, ev.clientY);
    const ud = pickOwner(hit.object).userData as {
      sat?: boolean;
      kind?: 'symbol' | 'link';
      subId?: string;
      symName?: string;
      id?: string;
    };
    if (ud.sat) {
      return { kind: ud.kind ?? 'symbol', subId: ud.subId ?? '', symName: ud.symName ?? '' };
    }
    return { kind: 'subject', subId: ud.id ?? '', symName: '' };
  }

  renderer.domElement.addEventListener('pointermove', (ev) => {
    ptrNdc.set((ev.clientX / window.innerWidth) * 2 - 1, -(ev.clientY / window.innerHeight) * 2 + 1);
    ptrActive = true;
    const h = pick(ev);
    hoveredSat = null;
    hovered = h && h.kind === 'subject' ? h.subId : null;
    renderer.domElement.style.cursor = h ? 'pointer' : 'grab';
    if (!h) {
      tooltip.hidden = true;
      return;
    }
    if (h.kind === 'subject' && byId.has(h.subId)) {
      const sub = byId.get(h.subId)!;
      tooltip.hidden = false;
      tooltip.textContent = `${sub.glyph ? `${sub.glyph} ` : ''}${sub.title} — ${sub.tag}${sub.community != null ? ` · community #${sub.community}` : ''}${sub.engine === 'graphify' ? ' · graphify' : ''}`;
    } else if (h.kind === 'link' && byId.has(h.subId)) {
      const sub = byId.get(h.subId)!;
      hoveredSat = sats.find((s) => s.kind === 'link' && s.subId === h.subId) ?? null;
      tooltip.hidden = false;
      tooltip.textContent = `→ ${sub.title} — ${sub.tag}`;
    } else if (h.kind === 'symbol') {
      hoveredSat = sats.find((s) => s.kind === 'symbol' && s.symName === h.symName) ?? null;
      tooltip.hidden = false;
      tooltip.textContent = hoveredSat ? hoveredSat.tip : h.symName;
    } else {
      tooltip.hidden = true;
      return;
    }
    tooltip.style.left = `${ev.clientX + 14}px`;
    tooltip.style.top = `${ev.clientY + 10}px`;
  });
  renderer.domElement.addEventListener('pointerdown', (ev) => {
    downAt = [ev.clientX, ev.clientY];
    // The user grabbed the camera: cancel any in-flight fly-to.
    fly = null;
    tooltip.hidden = true;
  });
  renderer.domElement.addEventListener('pointerup', (ev) => {
    if (!downAt) return;
    const dx = ev.clientX - downAt[0];
    const dy = ev.clientY - downAt[1];
    downAt = null;
    if (dx * dx + dy * dy > 36) return; // was a drag, not a tap
    const h = pick(ev);
    if (!h) window.location.hash = '#/';
    else if (h.kind === 'subject') window.location.hash = `#/${h.subId}`;
    else if (h.kind === 'link') window.location.hash = `#/${h.subId}`;
    else if (h.symName) window.location.hash = `#/${h.subId}/${encodeURIComponent(h.symName)}`;
  });
  renderer.domElement.addEventListener('dblclick', (ev) => {
    if (!pick(ev)) window.location.hash = '#/';
  });
  renderer.domElement.addEventListener('pointerleave', () => {
    ptrActive = false;
    tooltip.hidden = true;
  });

  // Pause rendering when the tab is hidden (battery + GPU).
  let hidden = false;
  document.addEventListener('visibilitychange', () => {
    hidden = document.hidden;
  });

  // ── Loop ─────────────────────────────────────────────────
  const clock = new THREE.Clock();
  const dummy = new THREE.Object3D();
  let lastFrame = performance.now();
  let frameAcc = 0;
  let frameN = 0;
  function tick(): void {
    requestAnimationFrame(tick);
    if (hidden || twoD) return;
    const now = performance.now();
    const dt = Math.min(0.05, (now - lastFrame) / 1000);
    lastFrame = now;
    const t = clock.getElapsedTime();
    if (!REDUCED) {
      // Mascot life: bob + slow look-around + a blink (squash) every ~4s.
      const sinceBlink = t - blinkAt;
      const squash = sinceBlink > 0 && sinceBlink < 0.16 ? 0.9 : 1;
      mascot.position.y = 1.6 + Math.sin(t * 1.8) * 0.32;
      mascot.rotation.y = Math.sin(t * 0.4) * 0.35;
      mascot.scale.set(2 - squash, squash, 1);
      if (sinceBlink > 4 + (blinkAt % 1.7)) blinkAt = t;
      const stars = scene.getObjectByName('stars');
      if (stars) stars.rotation.y = t * 0.008;
      nebulae.forEach((n, k) => {
        n.position.y += Math.sin(t * 0.24 + k * 2.1) * 0.0016;
      });
      // Lantern drift: one matrix compose per lantern, one draw call total.
      for (let k = 0; k < LANTERN_N; k++) {
        lanternPos[k * 3 + 1] += lanternSpeed[k] * dt;
        if (lanternPos[k * 3 + 1] > 13) lanternPos[k * 3 + 1] = -4;
        dummy.position.set(lanternPos[k * 3], lanternPos[k * 3 + 1], lanternPos[k * 3 + 2]);
        dummy.updateMatrix();
        lanternMesh.setMatrixAt(k, dummy.matrix);
      }
      lanternMesh.instanceMatrix.needsUpdate = true;
      if (t >= nextShooter) spawnShooter(t);
      for (let k = shooters.length - 1; k >= 0; k--) {
        const s = shooters[k];
        s.sp.position.addScaledVector(s.vel, 0.016);
        const left = s.life - t;
        (s.sp.material as THREE.SpriteMaterial).opacity = Math.max(0, Math.min(0.9, left));
        if (left <= 0) {
          scene.remove(s.sp);
          (s.sp.material.map as THREE.Texture).dispose();
          s.sp.material.dispose();
          shooters.splice(k, 1);
        }
      }
      // Select-burst sparks: gravity + fade, dead sparks park at y=-999.
      for (let k = 0; k < BURST_N; k++) {
        if (burstLife[k] <= 0) continue;
        burstLife[k] -= dt;
        if (burstLife[k] <= 0) {
          burstPos[k * 3 + 1] = -999;
          continue;
        }
        burstVel[k * 3 + 1] -= 4 * dt;
        burstPos[k * 3] += burstVel[k * 3] * dt;
        burstPos[k * 3 + 1] += burstVel[k * 3 + 1] * dt;
        burstPos[k * 3 + 2] += burstVel[k * 3 + 2] * dt;
      }
      burstGeo.getAttribute('position').needsUpdate = true;
      // Beacon breathing + expanding pulse ring on the selected node.
      if (beacon.visible) {
        const bm = beacon.material as THREE.MeshBasicMaterial;
        bm.opacity = 0.22 + Math.sin(t * 3) * 0.08;
        const cycle = (t % 1.6) / 1.6;
        pulse.scale.setScalar(1 + cycle * 2.2);
        (pulse.material as THREE.MeshBasicMaterial).opacity = 0.5 * (1 - cycle);
        // Ride the bobbing node.
        const sel = selected ? nodeById.get(selected) : undefined;
        if (sel) {
          beacon.position.set(sel.position.x, sel.position.y + 1.2, sel.position.z);
          pulse.position.set(sel.position.x, sel.position.y - 1.4, sel.position.z);
        }
      }
    }
    nodes.forEach((grp, ix) => {
      const mat = nodeMats[ix];
      const id = grp.userData.id as string;
      const onPath = pathSet.has(id);
      // Magnetic lean: pull toward the cursor ray when it passes close.
      let mag = 0;
      if (!REDUCED) {
        const bob = Math.sin(t * 1.1 + ix * 1.7) * 0.28;
        mpos.set(nodeBase[ix].x, nodeBase[ix].y + bob, nodeBase[ix].z);
        mag = magnetPull(mpos, nodeMag[ix]);
        grp.position.copy(mpos).add(nodeMag[ix]);
      }
      const active = id === selected || id === hovered || onPath || mag > 0.55;
      mat.emissiveIntensity += ((active ? 1.25 : 0.55) - mat.emissiveIntensity) * 0.15;
      const s =
        id === selected
          ? 1 + Math.sin(t * 4) * 0.1
          : onPath
            ? 1.08 + Math.sin(t * 4) * 0.03
            : mag > 0.05
              ? 1 + mag * 0.3
              : 1;
      grp.scale.setScalar(s);
      grp.rotation.y += active ? 0.012 : 0.003;
      grp.rotation.x += 0.001;
      // Nameplates and orbit frames follow the (possibly magnetic) node.
      if (!REDUCED) {
        nodeLabels[ix].position.set(grp.position.x, grp.position.y + 2.05, grp.position.z);
        nodeRings[ix].position.copy(grp.position);
        nodeRings[ix].rotation.z += 0.004;
      }
      // Selected / hovered / magnetically-near nameplates pop.
      const lab = nodeLabels[ix];
      const base = lab.userData.baseScale as THREE.Vector3;
      const boost =
        id === selected ? 1.14 + Math.sin(t * 4) * 0.04 : id === hovered || mag > 0.55 ? 1.07 + mag * 0.08 : 1;
      lab.scale.set(base.x * boost, base.y * boost, 1);
    });
    // Satellites orbit the selected node; tethers follow both endpoints.
    // Magnetic: near the cursor ray they lean out of the orbit, glow up
    // and (with the pick proximity fallback) stay clickable at any speed.
    if (sats.length) {
      const center = selected ? nodeById.get(selected)?.position : undefined;
      if (center) {
        for (const s of sats) {
          if (!REDUCED) s.angle += s.speed * dt;
          const px = center.x + Math.cos(s.angle) * s.radius;
          const pz = center.z + Math.sin(s.angle) * s.radius;
          const py = center.y + s.yOff + (REDUCED ? 0 : Math.sin(t * 1.6 + s.angle * 2) * 0.15);
          let mag = 0;
          if (!REDUCED) {
            mpos.set(px, py, pz);
            mag = magnetPull(mpos, s.mag);
            mpos.add(s.mag);
            s.mesh.position.copy(mpos);
            s.label.position.set(mpos.x, mpos.y + 0.72, mpos.z);
          } else {
            s.mesh.position.set(px, py, pz);
            s.label.position.set(px, py + 0.72, pz);
          }
          const p = s.linkPos.array as Float32Array;
          p[0] = center.x; p[1] = center.y; p[2] = center.z;
          p[3] = s.mesh.position.x; p[4] = s.mesh.position.y; p[5] = s.mesh.position.z;
          s.linkPos.needsUpdate = true;
          // Dashed tethers need fresh line distances as they swing.
          if (s.inferred) s.link.computeLineDistances();
          const hot = hoveredSat === s || mag > 0.55;
          s.mat.emissiveIntensity += ((hot ? 1.6 : 0.9) - s.mat.emissiveIntensity) * 0.2;
          s.mesh.scale.setScalar(hot ? 1.35 + mag * 0.25 : 1 + mag * 0.3);
          s.mesh.rotation.y += 0.01;
        }
      }
    }
    // Highlight the selected module's link back to the manju.
    lines.forEach((link, ix) => {
      const on = data.subjects[ix]?.id === selected;
      const lm = link.material as THREE.LineBasicMaterial;
      lm.opacity += ((on ? 0.7 : 0.3) - lm.opacity) * 0.15;
    });
    // Graph edges ride the bobbing nodes; brighten on selection/hover and
    // blaze (full opacity) along a traced path.
    for (const se of subEdges) {
      const ma = nodeById.get(se.a);
      const mb = nodeById.get(se.b);
      if (!ma || !mb) continue;
      const attr = se.line.geometry.getAttribute('position') as THREE.BufferAttribute;
      const arr = attr.array as Float32Array;
      arr[0] = ma.position.x; arr[1] = ma.position.y; arr[2] = ma.position.z;
      arr[3] = mb.position.x; arr[4] = mb.position.y; arr[5] = mb.position.z;
      attr.needsUpdate = true;
      if (se.dashed) se.line.computeLineDistances();
      const onPath = pathPairs.has(se.a < se.b ? `${se.a}|${se.b}` : `${se.b}|${se.a}`);
      const hot = se.a === selected || se.b === selected || se.a === hovered || se.b === hovered;
      const goal = onPath ? 0.95 : hot ? 0.55 : 0.16;
      const lm = se.line.material as THREE.LineBasicMaterial | THREE.LineDashedMaterial;
      lm.opacity += (goal - lm.opacity) * 0.15;
    }
    // Eased flight + FOV punch while travelling.
    if (fly) {
      const k = Math.min(1, (performance.now() - fly.t0) / fly.dur);
      camera.position.lerpVectors(fly.fromPos, fly.toPos, easeInOutCubic(k));
      if (k >= 1) fly = null;
    }
    const fovGoal = fly ? 50 : 55;
    if (Math.abs(camera.fov - fovGoal) > 0.05) {
      camera.fov += (fovGoal - camera.fov) * 0.08;
      camera.updateProjectionMatrix();
    }
    controls.target.lerp(focusTarget, 0.06);
    controls.update();
    if (BLOOM_OK) composer.render();
    else renderer.render(scene, camera);
    // HUD pass: nameplates draw straight to screen (no bloom, no fog, no
    // tone mapping), so the pixel type stays razor sharp over the scenery.
    renderer.autoClear = false;
    renderer.clearDepth();
    renderer.render(labelScene, camera);
    renderer.autoClear = true;
    // Adaptive resolution (production-site staple): sustained slow frames
    // step DPR down, sustained fast frames step it back up.
    frameAcc += dt * 1000;
    frameN++;
    if (frameN >= 120) {
      const avg = frameAcc / frameN;
      if (avg > 26 && dprTier < DPR_STEPS.length - 1) {
        dprTier++;
        renderer.setPixelRatio(DPR_STEPS[dprTier]);
        composer.setSize(window.innerWidth, window.innerHeight);
      } else if (avg < 12 && dprTier > 0) {
        dprTier--;
        renderer.setPixelRatio(DPR_STEPS[dprTier]);
        composer.setSize(window.innerWidth, window.innerHeight);
      }
      frameAcc = 0;
      frameN = 0;
    }
  }

  window.addEventListener('resize', () => {
    camera.aspect = window.innerWidth / window.innerHeight;
    camera.updateProjectionMatrix();
    renderer.setSize(window.innerWidth, window.innerHeight);
    composer.setSize(window.innerWidth, window.innerHeight);
  });

  renderRail();
  route();
  tick();
}

void boot();
