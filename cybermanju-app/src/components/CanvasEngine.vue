<!-- CyberManju OS — CanvasEngine (src/components/CanvasEngine.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <canvas ref="canvasRef" class="canvas-engine" />
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'

const props = withDefaults(defineProps<{
  enabled?: boolean
  mode?: number
}>(), {
  enabled: true,
  mode: 0,
})

const canvasRef = ref<HTMLCanvasElement | null>(null)
let animId = 0
let ctx: CanvasRenderingContext2D | null = null
let w = 0, h = 0
let t = 0

let verts: Float64Array
let edges: Int32Array
let rotX = 0, rotY = 0, rotZ = 0

let lissajousPhase = 0
let particles: { x: number; y: number; vx: number; vy: number; life: number }[] = []
let scanlineOffset = 0

// Tiles built ONCE (on mount / resize) so the per-frame cost is a handful of
// pattern fills instead of thousands of primitive fills — same pixels, same
// alpha, ~3 orders of magnitude fewer draw calls.
const CHECKER_TILE = 40 // size * 2 — one lattice cell of the checkerboard
let checkerPattern: CanvasPattern | null = null
let scanPattern: CanvasPattern | null = null

function buildTiles() {
  if (!ctx) return

  // One 40×40 cell: solid square top-left + diagonal square bottom-right,
  // exactly the pair the old nested fillRect loop painted per cell.
  const c = document.createElement('canvas')
  c.width = CHECKER_TILE
  c.height = CHECKER_TILE
  const cc = c.getContext('2d')
  if (cc) {
    cc.fillStyle = 'rgba(255, 255, 255, 0.03)'
    cc.fillRect(0, 0, CHECKER_TILE / 2, CHECKER_TILE / 2)
    cc.fillRect(CHECKER_TILE / 2, CHECKER_TILE / 2, CHECKER_TILE / 2, CHECKER_TILE / 2)
  }
  checkerPattern = ctx.createPattern(c, 'repeat')

  // One 1×4 cell: a single 1px row of alpha 0.015 → the CRT scanline ladder.
  const s = document.createElement('canvas')
  s.width = 1
  s.height = 4
  const sc = s.getContext('2d')
  if (sc) {
    sc.fillStyle = 'rgba(255, 255, 255, 0.015)'
    sc.fillRect(0, 0, 1, 1)
  }
  scanPattern = ctx.createPattern(s, 'repeat')
}

function resize() {
  const c = canvasRef.value
  if (!c) return
  const dpr = window.devicePixelRatio || 1
  w = window.innerWidth
  h = window.innerHeight
  c.width = w * dpr
  c.height = h * dpr
  c.style.width = w + 'px'
  c.style.height = h + 'px'
  if (ctx) ctx.scale(dpr, dpr)
}

function initWireframe() {
  const s = 60
  verts = new Float64Array([
    -s,-s,-s,  s,-s,-s,  s,s,-s,  -s,s,-s,
    -s,-s, s,  s,-s, s,  s,s, s,  -s,s, s,
  ])
  edges = new Int32Array([
    0,1, 1,2, 2,3, 3,0,
    4,5, 5,6, 6,7, 7,4,
    0,4, 1,5, 2,6, 3,7,
  ])
}

function initParticles() {
  particles = []
  for (let i = 0; i < 80; i++) {
    particles.push({
      x: Math.random() * w,
      y: Math.random() * h,
      vx: (Math.random() - 0.5) * 2,
      vy: (Math.random() - 0.5) * 2 - 1,
      life: Math.random(),
    })
  }
}

function project(x: number, y: number, z: number): [number, number] {
  const fov = 300
  const cx = w / 2
  const cy = h / 2
  const scale = fov / (fov + z)
  return [x * scale + cx, -y * scale + cy]
}

function rotate3d(x: number, y: number, z: number): [number, number, number] {
  let rx = x, ry = y, rz = z
  let t = ry
  ry = t * Math.cos(rotX) - rz * Math.sin(rotX)
  rz = t * Math.sin(rotX) + rz * Math.cos(rotX)
  t = rx
  rx = t * Math.cos(rotY) + rz * Math.sin(rotY)
  rz = -t * Math.sin(rotY) + rz * Math.cos(rotY)
  t = rx
  rx = t * Math.cos(rotZ) - ry * Math.sin(rotZ)
  ry = t * Math.sin(rotZ) + ry * Math.cos(rotZ)
  return [rx, ry, rz]
}

function drawWireframe() {
  if (!ctx || !verts || !edges) return
  const numVerts = verts.length / 3
  const projected: [number, number][] = []
  for (let i = 0; i < numVerts; i++) {
    const [rx, ry, rz] = rotate3d(verts[i*3], verts[i*3+1], verts[i*3+2])
    projected.push(project(rx, ry, rz))
  }
  ctx.strokeStyle = '#FFFFFF'
  ctx.lineWidth = 1.5
  // One path holding every edge, one stroke (disjoint subpaths render the
  // same as N separate strokes — without N begin/stroke flushes per frame).
  ctx.beginPath()
  for (let i = 0; i < edges.length; i += 2) {
    const p1 = projected[edges[i]]
    const p2 = projected[edges[i + 1]]
    ctx.moveTo(p1[0], p1[1])
    ctx.lineTo(p2[0], p2[1])
  }
  ctx.stroke()
  ctx.fillStyle = '#FFFFFF'
  for (const p of projected) {
    ctx.fillRect(p[0] - 2, p[1] - 2, 4, 4)
  }
}

function drawLissajous() {
  if (!ctx) return
  ctx.strokeStyle = '#FFFFFF'
  ctx.lineWidth = 1
  const cx = w / 2
  const cy = h / 2
  const r = Math.min(w, h) * 0.3
  ctx.beginPath()
  const steps = 200
  const a = 3, b = 4
  for (let i = 0; i <= steps; i++) {
    const th = (i / steps) * Math.PI * 2
    const x = cx + r * Math.sin(a * th + lissajousPhase * 0.3)
    const y = cy + r * Math.sin(b * th + lissajousPhase * 0.5)
    if (i === 0) ctx.moveTo(x, y)
    else ctx.lineTo(x, y)
  }
  ctx.stroke()
  const s2 = 5, b2 = 6
  ctx.beginPath()
  for (let i = 0; i <= steps; i++) {
    const th = (i / steps) * Math.PI * 2
    const x = cx + r * 0.6 * Math.sin(s2 * th + lissajousPhase * 0.2 + 1)
    const y = cy + r * 0.6 * Math.sin(b2 * th + lissajousPhase * 0.4 + 2)
    if (i === 0) ctx.moveTo(x, y)
    else ctx.lineTo(x, y)
  }
  ctx.stroke()
}

function drawParticles() {
  if (!ctx) return
  // One fillStyle for the whole batch; per-particle brightness rides on
  // globalAlpha (same source-over result, no per-particle style parsing).
  ctx.fillStyle = '#FFFFFF'
  for (const p of particles) {
    p.x += p.vx
    p.y += p.vy
    p.life -= 0.005
    if (p.life <= 0 || p.x < 0 || p.x > w || p.y < 0 || p.y > h) {
      p.x = Math.random() * w
      p.y = h + 10
      p.vx = (Math.random() - 0.5) * 2
      p.vy = -(1 + Math.random() * 2)
      p.life = 0.5 + Math.random() * 0.5
    }
    ctx.globalAlpha = Math.max(0, p.life)
    ctx.fillRect(p.x, p.y, 3, 3)
  }
  ctx.globalAlpha = 1
}

function drawOpArtRings() {
  if (!ctx) return
  ctx.strokeStyle = 'rgba(255, 255, 255, 0.06)'
  ctx.lineWidth = 1
  const cx = w / 2
  const cy = h / 2
  const maxR = Math.sqrt(w * w + h * h) / 2 + 50
  const offset = (t * 0.3) % 40
  // One path, one stroke: N subpaths render identically to N strokes but
  // avoid N style/state flushes per frame.
  ctx.beginPath()
  for (let r = offset; r < maxR; r += 40) {
    ctx.moveTo(cx + r, cy)
    ctx.arc(cx, cy, r, 0, Math.PI * 2)
  }
  ctx.stroke()
}

function drawCheckerboard() {
  if (!ctx || !checkerPattern) return
  const offX = (t * 0.5) % CHECKER_TILE
  const offY = (t * 0.3) % CHECKER_TILE
  ctx.save()
  ctx.translate(offX - CHECKER_TILE, offY - CHECKER_TILE)
  ctx.fillStyle = checkerPattern
  ctx.fillRect(0, 0, w + CHECKER_TILE * 2, h + CHECKER_TILE * 2)
  ctx.restore()
}

function drawScanlines() {
  if (!ctx || !scanPattern) return
  scanlineOffset = (scanlineOffset + 2) % 4
  ctx.save()
  ctx.translate(0, scanlineOffset)
  ctx.fillStyle = scanPattern
  ctx.fillRect(0, -4, w, h + 8)
  ctx.restore()
}

function draw() {
  if (!ctx) return
  t++
  rotX = t * 0.008
  rotY = t * 0.012
  rotZ = t * 0.005
  lissajousPhase += 0.02

  ctx.clearRect(0, 0, w, h)

  drawOpArtRings()
  drawCheckerboard()

  const modeCycle = Math.floor(t / 300) % 3
  switch (modeCycle) {
    case 0:
      drawWireframe()
      break
    case 1:
      drawLissajous()
      drawParticles()
      break
    case 2:
      drawWireframe()
      drawParticles()
      break
  }

  drawScanlines()

  animId = requestAnimationFrame(draw)
}

function start() {
  if (animId) return
  resize()
  initParticles()
  initWireframe()
  draw()
}

function stop() {
  if (animId) {
    cancelAnimationFrame(animId)
    animId = 0
  }
}

/** Backgrounded app → stop burning frames on an invisible wallpaper. */
function onVisibility() {
  if (document.hidden) stop()
  else if (props.enabled) start()
}

onMounted(() => {
  const c = canvasRef.value
  if (c) {
    ctx = c.getContext('2d')
  }
  buildTiles()
  if (props.enabled) start()
  window.addEventListener('resize', resize)
  document.addEventListener('visibilitychange', onVisibility)
})

onUnmounted(() => {
  stop()
  window.removeEventListener('resize', resize)
  document.removeEventListener('visibilitychange', onVisibility)
})

watch(() => props.enabled, (v) => {
  v ? start() : stop()
})
</script>

<style scoped>
.canvas-engine {
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  z-index: 0;
  pointer-events: none;
}
</style>
