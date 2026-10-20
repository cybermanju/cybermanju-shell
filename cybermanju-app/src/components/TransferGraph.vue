<!-- CyberManju OS — visual transfer board.
  //
  // One window showing files/folders next to provider nodes: browse any
  // mounted provider on the left, pin files as draggable nodes on the
  // canvas, click (or drop) a file node onto a provider node to plan a
  // transfer, flip each row between cp and mv, then Apply runs the plan
  // through the VFS canal (read → write → delete-on-move, recursive for
  // folders). Unmounted configs get an inline Mount button; honest limits
  // (5 MiB write cap, encrypted artifacts needing the passphrase) surface
  // per-row instead of failing silently. -->
<template>
  <div class="tf">
    <header class="tf-top">
      <div class="tf-brand">
        <span class="tf-brand-mark"><AppIcon name="solar:share-bold" :size="20" /></span>
        <div>
          <h2 class="tf-title">Transfer</h2>
          <p class="tf-subtitle">{{ planText }} · {{ mounts.length }} mounts</p>
        </div>
      </div>
      <div class="tf-top-actions">
        <button class="tf-btn" type="button" :disabled="!edges.length || running" @click="clearPlan">Clear</button>
        <button class="tf-btn primary" type="button" :disabled="!edges.length || running" @click="applyPlan">
          <AppIcon name="solar:play-bold" :size="13" /> {{ running ? `Applying… ${doneCount}/${edges.length}` : `Apply${edges.length ? ` (${edges.length})` : ''}` }}
        </button>
        <button v-if="running" class="tf-btn danger" type="button" @click="cancelApply">Cancel</button>
      </div>
    </header>

    <div v-if="running" class="tf-progress" role="status" aria-live="polite"><div class="tf-progress-fill" :style="{ width: `${applyPct}%` }"></div></div>

    <!-- ── source browser ── -->
    <section class="tf-browser" aria-label="Source browser">
      <div class="tf-brow-row">
        <label class="tf-field inline">
          <span class="tf-field-label">Source</span>
          <select v-model="browseMountId" class="tf-input" aria-label="Source mount" @change="browsePath = ''; void refreshEntries()">
            <option v-if="!mounts.length" value="" disabled>No mounts yet</option>
            <option v-for="m in mounts" :key="m.id" :value="m.id">{{ m.name }} · {{ m.backendType }}</option>
          </select>
        </label>
        <nav class="tf-crumbs" aria-label="Path">
          <button class="tf-link" type="button" @click="browsePath = ''; void refreshEntries()">{{ browseMountName || 'root' }}</button>
          <template v-for="(seg, i) in pathSegs" :key="i">
            <span class="muted">/</span>
            <button class="tf-link" type="button" @click="jumpTo(i)">{{ seg }}</button>
          </template>
        </nav>
        <button class="tf-icon-btn" type="button" title="Refresh listing" aria-label="Refresh listing" @click="void refreshEntries()">
          <AppIcon name="solar:restart-bold" :size="14" />
        </button>
        <label class="tf-field inline grow" title="Only needed for encrypted artifacts — empty falls back to the vault master passphrase">
          <span class="tf-field-label">Artifact passphrase</span>
          <input v-model="passphrase" class="tf-input" type="password" placeholder="empty = vault master passphrase" autocomplete="off" aria-label="Artifact passphrase" />
        </label>
      </div>
      <p v-if="!mounts.length" class="tf-hint">
        No provider mounts yet — mount one below on the canvas, or add providers in
        <button class="tf-link" type="button" @click="openAccounts">Accounts → Connections</button>.
      </p>
      <p v-else-if="browseLoading" class="tf-hint">Listing…</p>
      <p v-else-if="browseError" class="tf-note err">{{ browseError }}</p>
      <ul v-else class="tf-entries">
        <li v-if="browsePath" class="tf-entry">
          <button class="tf-entry-main" type="button" @click="goUp"><AppIcon name="solar:folder-bold" :size="15" /> …</button>
        </li>
        <li v-for="e in entries" :key="e.locator || e.path" class="tf-entry">
          <button class="tf-entry-main" type="button" :title="e.path" @click="e.isDir ? enterDir(e) : void 0" :disabled="!e.isDir">
            <AppIcon :name="e.isDir ? 'solar:folder-bold' : 'solar:file-bold'" :size="15" />
            <span class="tf-entry-name">{{ e.name }}</span>
            <span v-if="!e.isDir" class="muted small">{{ humanBytes(e.sizeBytes) }}</span>
          </button>
          <button
            class="tf-btn xs"
            type="button"
            :title="`Pin ${e.name} on the board`"
            :disabled="isPinned(e)"
            @click="pinEntry(e)"
          >{{ isPinned(e) ? 'Pinned' : '+ Board' }}</button>
        </li>
      </ul>
      <p class="tf-hint">Files over {{ humanBytes(TRANSFER_WRITE_CAP_BYTES) }} are refused before a single byte is read — split those plans, or move the file outside the board.</p>
    </section>

    <!-- ── node canvas ── -->
    <section class="tf-canvas-wrap" aria-label="Transfer canvas">
      <div ref="canvasRef" class="tf-canvas" @pointermove="onCanvasMove" @pointerup="onCanvasUp" @pointercancel="onCanvasUp">
        <svg class="tf-wires" aria-hidden="true">
          <defs>
            <marker id="tf-arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
              <path d="M 0 1 L 9 5 L 0 9 z" fill="currentColor" />
            </marker>
          </defs>
          <path
            v-for="es in edges"
            :key="es.edge.id"
            :d="wirePath(es.edge)"
            class="tf-wire"
            :class="[`op-${es.edge.op}`, `st-${es.status}`]"
            marker-end="url(#tf-arrow)"
          />
        </svg>

        <!-- file nodes -->
        <div
          v-for="(n, fi) in fileNodes"
          :key="n.id"
          class="tf-node file"
          :class="{ selected: selectedFileId === n.id, dragging: dragId === n.id }"
          :style="`transform: translate(${n.x}px, ${n.y}px); --d: ${Math.min(fi, 8) * 35}ms`"
          @pointerdown="onNodeDown($event, n.id)"
        >
          <span class="tf-node-icon"><AppIcon :name="n.isDir ? 'solar:folder-bold' : 'solar:file-bold'" :size="16" /></span>
          <span class="tf-node-meta">
            <strong class="tf-node-name" :title="n.remotePath">{{ n.name }}</strong>
            <span class="muted small">{{ n.mountName }}{{ n.isDir ? ' · folder' : ` · ${humanBytes(n.sizeBytes)}` }}</span>
          </span>
          <button class="tf-node-x" type="button" :title="`Remove ${n.name}`" @click.stop="unpinNode(n.id)">✕</button>
        </div>

        <!-- provider nodes -->
        <div
          v-for="(p, pi) in providerNodes"
          v-show="!p.hidden"
          :key="p.configId"
          :ref="(el) => setProvEl(p.configId, el)"
          class="tf-node prov"
          :class="{ target: !!selectedFileId && p.mountId, dragging: dragId === `prov:${p.configId}`, 'drop-hover': hoverProv === p.configId }"
          :style="`transform: translate(${p.x}px, ${p.y}px); --d: ${Math.min(pi, 8) * 35}ms`"
          @pointerdown="onNodeDown($event, `prov:${p.configId}`)"
          @click="onProvClick(p)"
        >
          <ProviderLogo :provider="logoFor(p.backendType)" :size="30" />
          <span class="tf-dot" :class="p.mountId ? 'on' : 'off'" :title="p.mountId ? 'Mounted' : 'Not mounted'"></span>
          <span class="tf-node-meta">
            <strong class="tf-node-name">{{ p.name }}</strong>
            <span class="muted small">{{ p.mountId ? `${p.backendType} · mounted` : `${p.backendType} · not mounted` }}</span>
          </span>
          <button v-if="!p.mountId" class="tf-btn xs primary" type="button" @click.stop="mountProvider(p)">Mount</button>
          <button v-else class="tf-node-x" type="button" title="Remove from board" @click.stop="hideProvider(p.configId)">✕</button>
          <label v-if="p.mountId" class="tf-target" @pointerdown.stop @click.stop>
            <span class="muted small">→ folder</span>
            <input v-model="p.targetFolder" class="tf-input xs" placeholder="(root)" autocomplete="off" :aria-label="`Target folder on ${p.name}`" />
          </label>
        </div>

        <p v-if="!fileNodes.length" class="tf-canvas-hint">Pin files above with <strong>+ Board</strong>, drag them around, then click a file and a provider — or drop the file onto one.</p>
      </div>
    </section>

    <!-- ── plan rows ── -->
    <section v-if="edges.length" class="tf-plan" aria-label="Planned transfers" aria-live="polite">
      <div v-for="es in edges" :key="es.edge.id" class="tf-edge" :class="`st-${es.status}`">
        <div class="tf-edge-top">
          <span class="tf-edge-route" :title="`${es.edge.from.remotePath} → ${destPathFor(es.edge)}${es.detail ? ` — ${es.detail}` : ''}`">
            <strong>{{ es.edge.sourceName }}</strong>
            <span class="muted">{{ shortMount(es.edge.from.mountId) }} → {{ shortMount(es.edge.to.mountId) }}/{{ destPathFor(es.edge) }}</span>
          </span>
          <button class="tf-btn xs" :class="{ on: es.edge.op === 'mv' }" type="button" :disabled="running" :title="es.edge.op === 'cp' ? 'Copy — keep the source' : 'Move — verify each destination byte, then delete the source'" @click="flipOp(es.edge.id)">
            {{ es.edge.op === 'cp' ? 'CP' : 'MV' }}
          </button>
          <span v-if="es.status !== 'queued'" class="tf-edge-status" :class="`st-${es.status}`">{{ es.status === 'running' ? '…' : es.status }}</span>
          <button class="tf-node-x" type="button" :disabled="running" title="Remove from plan" @click="dropEdge(es.edge.id)">✕</button>
        </div>
        <p v-if="es.detail" class="tf-edge-detail" :class="{ warn: es.status === 'done' }">{{ es.detail }}</p>
      </div>
      <p v-if="edgeError" class="tf-note err">{{ edgeError }}</p>
    </section>

    <footer class="tf-foot">
      <span class="muted small">{{ planText }}</span>
      <span class="tf-foot-actions">
        <button v-if="hiddenCount" class="tf-link" type="button" @click="showAllProviders">Show hidden providers</button>
        <button class="tf-link" type="button" @click="tidyLayout">Tidy board</button>
      </span>
      <span class="muted small">mv re-reads and byte-compares every destination before deleting its source.</span>
    </footer>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import ProviderLogo from '@/components/ProviderLogo.vue'
import { computed, nextTick, onMounted, ref } from 'vue'
import { useAppStore } from '@/stores/app'
import { useWindowManager } from '@/composables/useWindowManager'
import {
  deleteVfsFile,
  getVfsMasterPassphrase,
  listVfsDir,
  listVfsMounts,
  readVfsFile,
  saveVfsMount,
  writeVfsFile,
  type ProviderMount,
  type VfsEntry,
} from '@/composables/useProviderCanal'
import { humanBytes } from '@/utils/format'
import {
  bytesEqual,
  clampPos,
  destPathFor,
  joinRemote,
  loadNodeLayout,
  makeEdge,
  overWriteCap,
  planKey,
  planSummaryText,
  saveNodeLayout,
  summarizePlan,
  toggleEdgeOp,
  TRANSFER_WRITE_CAP_BYTES,
  validateEdge,
  type TransferEdge,
} from '@/utils/transferPlan'

const store = useAppStore()
const wm = useWindowManager()

interface FileNode {
  id: string
  mountId: string
  mountName: string
  remotePath: string
  name: string
  isDir: boolean
  sizeBytes: number
  x: number
  y: number
}

interface ProviderNode {
  configId: string
  mountId: string | null
  name: string
  backendType: string
  targetFolder: string
  hidden: boolean
  x: number
  y: number
}

interface EdgeState {
  edge: TransferEdge
  status: 'queued' | 'running' | 'done' | 'error'
  detail: string
}

const mounts = ref<ProviderMount[]>([])
const fileNodes = ref<FileNode[]>([])
const providerNodes = ref<ProviderNode[]>([])
const edges = ref<EdgeState[]>([])
const selectedFileId = ref<string | null>(null)
const edgeError = ref('')

// browser
const browseMountId = ref('')
const browsePath = ref('')
const entries = ref<VfsEntry[]>([])
const browseLoading = ref(false)
const browseError = ref('')
const passphrase = ref('')

// apply
const running = ref(false)
const cancelled = ref(false)
const doneCount = ref(0)
const canvasRef = ref<HTMLElement | null>(null)
const provEls = new Map<string, HTMLElement | null>()

const FILE_W = 196
const PROV_W = 220

const browseMountName = computed(() => mounts.value.find(m => m.id === browseMountId.value)?.name ?? '')
const pathSegs = computed(() => browsePath.value.split('/').filter(Boolean))
const planText = computed(() => planSummaryText(summarizePlan(edges.value.map(e => e.edge))))
const applyPct = computed(() => (edges.value.length ? Math.round((doneCount.value / edges.value.length) * 100) : 0))

function logoFor(b: string): string {
  return b === 'local' ? 'local' : String(b)
}

function shortMount(mountId: string): string {
  return mounts.value.find(m => m.id === mountId)?.name ?? mountId.slice(0, 8)
}

function setProvEl(configId: string, el: unknown) {
  provEls.set(configId, el as HTMLElement | null)
}

function openAccounts() {
  wm.open('accounts', { tab: 'connections' })
}

// ── mounts + browser ────────────────────────────────────────────
async function refreshMounts() {
  mounts.value = await listVfsMounts().catch(() => [])
  if (!browseMountId.value || !mounts.value.some(m => m.id === browseMountId.value)) {
    browseMountId.value = mounts.value[0]?.id ?? ''
    browsePath.value = ''
  }
  syncProviderNodes()
}

function syncProviderNodes() {
  const layout = loadNodeLayout()
  const rows: ProviderNode[] = []
  const w = canvasRef.value?.clientWidth || 900
  store.syncConfigs.forEach((c, i) => {
    const prev = providerNodes.value.find(p => p.configId === c.id)
    const mount = mounts.value.find(m => m.configId === c.id) ?? null
    const key = `prov:${c.id}`
    const saved = layout[key]
    rows.push({
      configId: c.id,
      mountId: mount?.id ?? null,
      name: c.name || c.backendType,
      backendType: c.backendType,
      targetFolder: prev?.targetFolder ?? '',
      hidden: prev?.hidden ?? false,
      x: saved?.x ?? Math.max(280, w - PROV_W - 16),
      y: saved?.y ?? 16 + i * 148,
    })
  })
  providerNodes.value = rows
}

async function refreshEntries() {
  if (!browseMountId.value) {
    entries.value = []
    return
  }
  browseLoading.value = true
  browseError.value = ''
  try {
    entries.value = await listVfsDir(browseMountId.value, browsePath.value)
  } catch (e) {
    entries.value = []
    browseError.value = e instanceof Error ? e.message : String(e)
  } finally {
    browseLoading.value = false
  }
}

function enterDir(e: VfsEntry) {
  browsePath.value = joinRemote(browsePath.value, e.name)
  void refreshEntries()
}

function goUp() {
  const segs = pathSegs.value
  segs.pop()
  browsePath.value = segs.join('/')
  void refreshEntries()
}

function jumpTo(i: number) {
  browsePath.value = pathSegs.value.slice(0, i + 1).join('/')
  void refreshEntries()
}

async function mountProvider(p: ProviderNode) {
  try {
    const cfg = store.syncConfigs.find(c => c.id === p.configId)
    if (!cfg) return
    await saveVfsMount({ configId: cfg.id, name: cfg.name || cfg.backendType, backendType: cfg.backendType })
    store.notifySuccess(`Mounted ${p.name}`)
    await refreshMounts()
  } catch (e) {
    store.notifyError('Mount failed', e)
  }
}

const hiddenCount = computed(() => providerNodes.value.filter(p => p.hidden).length)

function hideProvider(configId: string) {
  const p = providerNodes.value.find(x => x.configId === configId)
  if (p) p.hidden = true
}

function showAllProviders() {
  providerNodes.value.forEach(p => {
    p.hidden = false
  })
}

/** Re-stack files left and providers right (clears handmade tangles). */
function tidyLayout() {
  const w = canvasRef.value?.clientWidth || 900
  fileNodes.value.forEach((f, i) => {
    f.x = 16
    f.y = 16 + (i % 8) * 84
  })
  providerNodes.value
    .filter(p => !p.hidden)
    .forEach((p, i) => {
      p.x = Math.max(300, w - PROV_W - 16)
      p.y = 16 + i * 148
    })
  persistLayout()
}

// ── file nodes ──────────────────────────────────────────────────
function nodeIdFor(mountId: string, remotePath: string): string {
  const clean = `${mountId}-${remotePath}`.replace(/[^a-zA-Z0-9-_]/g, '-').slice(0, 60)
  let id = `f-${clean}`
  let n = 2
  while (fileNodes.value.some(f => f.id === id)) id = `f-${clean}-${n++}`
  return id
}

function isPinned(e: VfsEntry): boolean {
  const rel = joinRemote(browsePath.value, e.name)
  return fileNodes.value.some(f => f.mountId === browseMountId.value && f.remotePath === rel)
}

function pinEntry(e: VfsEntry) {
  const rel = joinRemote(browsePath.value, e.name)
  const id = nodeIdFor(browseMountId.value, rel)
  const layout = loadNodeLayout()
  const saved = layout[id]
  const i = fileNodes.value.length
  fileNodes.value.push({
    id,
    mountId: browseMountId.value,
    mountName: browseMountName.value,
    remotePath: rel,
    name: e.name,
    isDir: e.isDir,
    sizeBytes: e.sizeBytes,
    x: saved?.x ?? 16,
    y: saved?.y ?? 16 + (i % 6) * 84,
  })
  persistLayout()
  selectedFileId.value = id
}

function unpinNode(id: string) {
  fileNodes.value = fileNodes.value.filter(f => f.id !== id)
  edges.value = edges.value.filter(es => (es.edge as TransferEdge & { nodeId?: string }).nodeId !== id)
  if (selectedFileId.value === id) selectedFileId.value = null
  persistLayout()
}

function persistLayout() {
  const layout = loadNodeLayout()
  for (const f of fileNodes.value) layout[f.id] = { x: f.x, y: f.y }
  for (const p of providerNodes.value) layout[`prov:${p.configId}`] = { x: p.x, y: p.y }
  saveNodeLayout(layout)
}

// ── drag + drop (rAF-batched: pointermove only schedules work, one frame
// applies it — the canvas stays smooth with dozens of nodes and wires) ──
const dragId = ref<string | null>(null)
const hoverProv = ref<string | null>(null)
let dragMoved = false
let dragStart = { x: 0, y: 0 }
let nodeStart = { x: 0, y: 0 }
let rafId = 0
let pendingXY: { x: number; y: number } | null = null

function onNodeDown(ev: PointerEvent, id: string) {
  const target = ev.target as HTMLElement
  if (target.closest('button') || target.closest('input') || target.closest('select') || target.closest('label')) return
  dragId.value = id
  dragMoved = false
  pendingXY = null
  hoverProv.value = null
  dragStart = { x: ev.clientX, y: ev.clientY }
  const n = findNode(id)
  nodeStart = n ? { x: n.x, y: n.y } : { x: 0, y: 0 }
}

function findNode(id: string): FileNode | ProviderNode | null {
  if (id.startsWith('prov:')) return providerNodes.value.find(p => `prov:${p.configId}` === id) ?? null
  return fileNodes.value.find(f => f.id === id) ?? null
}

/** Move the dragged node; hover-highlights the drop target for file drags. */
function moveDraggedTo(clientX: number, clientY: number): void {
  if (!dragId.value) return
  const dx = clientX - dragStart.x
  const dy = clientY - dragStart.y
  if (!dragMoved && Math.abs(dx) + Math.abs(dy) <= 4) return
  dragMoved = true
  const n = findNode(dragId.value)
  const canvas = canvasRef.value
  if (!n || !canvas) return
  const pos = clampPos(
    { x: nodeStart.x + dx, y: nodeStart.y + dy },
    canvas.clientWidth,
    canvas.clientHeight,
  )
  n.x = pos.x
  n.y = pos.y
  hoverProv.value = !dragId.value.startsWith('prov:')
    ? (hitProvider(clientX, clientY)?.configId ?? null)
    : null
}

function onCanvasMove(ev: PointerEvent) {
  if (!dragId.value) return
  pendingXY = { x: ev.clientX, y: ev.clientY }
  if (rafId) return
  rafId = requestAnimationFrame(() => {
    rafId = 0
    const pt = pendingXY
    pendingXY = null
    if (pt) moveDraggedTo(pt.x, pt.y)
  })
}

function onCanvasUp(ev: PointerEvent) {
  const id = dragId.value
  if (!id) return
  if (rafId) {
    cancelAnimationFrame(rafId)
    rafId = 0
  }
  dragId.value = null
  // A drag released before its frame ran still lands on the final pointer.
  if (pendingXY) {
    const pt = pendingXY
    pendingXY = null
    moveDraggedTo(pt.x, pt.y)
  }
  const wasDrag = dragMoved
  dragMoved = false
  const hovered = hoverProv.value
  hoverProv.value = null
  if (wasDrag) {
    persistLayout()
    // Drop a file node onto a provider node → plan the transfer there.
    if (!id.startsWith('prov:')) {
      const hit = (hovered && providerNodes.value.find(p => p.configId === hovered)) ||
        hitProvider(ev.clientX, ev.clientY)
      if (hit) addEdge(id, hit)
    }
    return
  }
  // Click (no drag): select files, or complete a pending selection on providers.
  if (id.startsWith('prov:')) {
    const p = providerNodes.value.find(x => `prov:${x.configId}` === id)
    if (p) onProvClick(p)
  } else {
    selectedFileId.value = selectedFileId.value === id ? null : id
  }
}

function hitProvider(clientX: number, clientY: number): ProviderNode | null {
  for (const p of providerNodes.value) {
    if (p.hidden || !p.mountId) continue
    const el = provEls.get(p.configId)
    if (!el) continue
    const r = el.getBoundingClientRect()
    if (clientX >= r.left && clientX <= r.right && clientY >= r.top && clientY <= r.bottom) return p
  }
  return null
}

function onProvClick(p: ProviderNode) {
  if (p.hidden || !p.mountId) return
  if (selectedFileId.value) addEdge(selectedFileId.value, p)
}

function addEdge(fileId: string, p: ProviderNode) {
  edgeError.value = ''
  const n = fileNodes.value.find(f => f.id === fileId)
  if (!n || !p.mountId) return
  const edge = makeEdge(`e-${Date.now().toString(36)}-${edges.value.length}`, {
    sourceName: n.name,
    sourceIsDir: n.isDir,
    from: { mountId: n.mountId, remotePath: n.remotePath },
    to: { mountId: p.mountId, remotePath: p.targetFolder },
  })
  const problem = validateEdge(edge)
  if (problem) {
    edgeError.value = problem
    return
  }
  if (edges.value.some(es => planKey(es.edge) === planKey(edge))) {
    edgeError.value = 'Already planned — flip it to MV or change the target folder instead.';
    return
  }
  ;(edge as TransferEdge & { nodeId?: string }).nodeId = fileId
  edges.value.push({ edge, status: 'queued', detail: '' })
  selectedFileId.value = null
}

function flipOp(id: string) {
  edges.value = edges.value.map(es => (es.edge.id === id ? { ...es, edge: toggleEdgeOp(es.edge) } : es))
}

function dropEdge(id: string) {
  edges.value = edges.value.filter(es => es.edge.id !== id)
}

function clearPlan() {
  if (running.value) return
  edges.value = []
  edgeError.value = ''
}

// ── wires ───────────────────────────────────────────────────────
function wirePath(edge: TransferEdge): string {
  const from = fileNodes.value.find(f => {
    const nid = (edge as TransferEdge & { nodeId?: string }).nodeId
    return nid ? f.id === nid : f.mountId === edge.from.mountId && f.remotePath === edge.from.remotePath
  })
  const to = providerNodes.value.find(p => {
    const m = mounts.value.find(x => x.id === edge.to.mountId)
    return m ? p.configId === m.configId : false
  })
  if (!from || !to) return ''
  const x1 = from.x + FILE_W
  const y1 = from.y + 30
  const x2 = to.x
  const y2 = to.y + 44
  const mx = (x1 + x2) / 2
  return `M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2 - 4} ${y2}`
}

// ── execution ───────────────────────────────────────────────────
interface WorkFile {
  srcPath: string
  destPath: string
  sizeBytes: number
}

/**
 * Fail the whole row before reading a single byte when files exceed the
 * canal write cap — never after megabytes are already in memory.
 */
function refuseOverCap(files: Array<{ name: string; sizeBytes: number }>): void {
  const big = files.filter(f => overWriteCap(f.sizeBytes))
  if (!big.length) return
  const names = big.slice(0, 3).map(f => `'${f.name}' (${humanBytes(f.sizeBytes)})`).join(', ')
  const more = big.length > 3 ? ` +${big.length - 3} more` : ''
  throw new Error(
    `too_large: ${names}${more} exceed${big.length === 1 ? 's' : ''} the ${humanBytes(TRANSFER_WRITE_CAP_BYTES)} per-file write cap — split the plan`,
  )
}

async function collectFiles(edge: TransferEdge, rootSizeBytes: number): Promise<WorkFile[]> {
  const out: WorkFile[] = []
  const root = edge.from.remotePath
  if (!edge.sourceIsDir) {
    refuseOverCap([{ name: edge.sourceName, sizeBytes: rootSizeBytes }])
    out.push({ srcPath: root, destPath: destPathFor(edge), sizeBytes: rootSizeBytes })
    return out
  }
  const stack = [root]
  while (stack.length) {
    const dir = stack.pop() as string
    const kids = await listVfsDir(edge.from.mountId, dir)
    refuseOverCap(kids.filter(k => !k.isDir))
    for (const k of kids) {
      const src = joinRemote(dir, k.name)
      if (k.isDir) {
        stack.push(src)
        continue
      }
      const rel = src.slice(root.length).replace(/^\/+/, '')
      out.push({ srcPath: src, destPath: joinRemote(edge.to.remotePath, edge.sourceName, rel), sizeBytes: k.sizeBytes })
    }
  }
  return out
}

async function effPassphrase(): Promise<string> {
  if (passphrase.value) return passphrase.value
  try {
    return await getVfsMasterPassphrase()
  } catch {
    return ''
  }
}

async function applyPlan() {
  if (running.value || !edges.value.length) return
  running.value = true
  cancelled.value = false
  doneCount.value = 0
  edgeError.value = ''
  const pass = await effPassphrase()
  try {
    for (const es of edges.value) {
      if (cancelled.value) {
        es.status = 'error'
        es.detail = 'cancelled — compare source and destination before retrying'
        doneCount.value += 1
        continue
      }
      es.status = 'running'
      es.detail = ''
      try {
        // Target folders stay editable after planning — re-validate now so a
        // row that converged onto its own source (or a duplicate) fails here
        // with its reason instead of running.
        const blocked = validateEdge(es.edge)
        if (blocked) throw new Error(blocked)
        if (edges.value.some(o => o !== es && o.status !== 'error' && planKey(o.edge) === planKey(es.edge))) {
          throw new Error('Duplicate plan row — remove one of them first.')
        }
        const nid = (es.edge as TransferEdge & { nodeId?: string }).nodeId
        const rootSize = fileNodes.value.find(f =>
          nid ? f.id === nid : f.mountId === es.edge.from.mountId && f.remotePath === es.edge.from.remotePath,
        )?.sizeBytes ?? 0
        // One walk feeds both phases, so the delete list can never drift
        // from what was actually copied.
        const files = await collectFiles(es.edge, rootSize)
        let decrypted = 0
        for (const f of files) {
          if (cancelled.value) throw new Error('cancelled — compare source and destination before retrying')
          const got = await readVfsFile(es.edge.from.mountId, f.srcPath, { passphrase: pass || undefined })
          if (got.magic === 'CYBE1') decrypted += 1
          await writeVfsFile(es.edge.to.mountId, f.destPath, got.bytes)
          if (es.edge.op === 'mv') {
            // Verify-before-delete: re-read the destination and compare
            // every byte. A mismatch keeps the source and fails the row.
            const back = await readVfsFile(es.edge.to.mountId, f.destPath)
            if (!bytesEqual(got.bytes, back.bytes)) {
              throw new Error(`integrity: '${f.destPath}' differs after write — source kept`)
            }
          }
        }
        if (es.edge.op === 'mv' && !cancelled.value) {
          for (const f of files) {
            if (cancelled.value) throw new Error('cancelled — compare source and destination before retrying')
            await deleteVfsFile(es.edge.from.mountId, f.srcPath)
          }
        }
        if (cancelled.value) throw new Error('cancelled — compare source and destination before retrying')
        es.status = 'done'
        es.detail = decrypted
          ? `${decrypted} encrypted file${decrypted === 1 ? '' : 's'} re-written decrypted at the destination`
          : ''
      } catch (e) {
        es.status = 'error'
        es.detail = e instanceof Error ? e.message : String(e)
      } finally {
        doneCount.value += 1
      }
    }
    const ok = edges.value.filter(e => e.status === 'done').length
    const bad = edges.value.filter(e => e.status === 'error').length
    if (cancelled.value) store.notifyError('Transfer cancelled', `${ok} finished before the cancel — verify both sides`)
    else if (bad) store.notifyError('Transfer finished with errors', `${ok} done, ${bad} failed — reasons on the rows`)
    else store.notifySuccess(`Transfer complete — ${ok} ${ok === 1 ? 'row' : 'rows'}`)
    await refreshEntries()
  } finally {
    running.value = false
  }
}

function cancelApply() {
  cancelled.value = true
}

onMounted(async () => {
  await Promise.allSettled([store.fetchSyncConfigs(), refreshMounts()])
  await nextTick()
  if (!browseMountId.value && mounts.value.length) browseMountId.value = mounts.value[0].id
  await refreshEntries()
})
</script>

<style scoped>
.tf {
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--ui-surface);
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 13px;
}
.tf-top {
  display: flex; align-items: center; justify-content: space-between; gap: 10px;
  padding: 12px 14px 10px; border-bottom: 1px solid var(--ui-border);
}
.tf-brand { display: flex; align-items: center; gap: 10px; min-width: 0; }
.tf-brand-mark {
  display: inline-flex; align-items: center; justify-content: center;
  width: 36px; height: 36px; border-radius: 10px; flex-shrink: 0;
  background: color-mix(in srgb, var(--ui-accent) 16%, transparent);
  color: var(--ui-accent);
  border: 1px solid color-mix(in srgb, var(--ui-accent) 40%, transparent);
}
.tf-title { margin: 0; font-size: 15px; letter-spacing: 0.4px; }
.tf-subtitle { margin: 1px 0 0; font-size: 11px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.tf-top-actions { display: flex; align-items: center; gap: 8px; }
.tf-progress { height: 6px; background: color-mix(in srgb, var(--ui-text) 10%, transparent); }
.tf-progress-fill { height: 100%; background: var(--ui-accent); transition: width 0.2s ease; }
.tf-browser { border-bottom: 1px solid var(--ui-border); padding: 10px 14px; display: flex; flex-direction: column; gap: 8px; max-height: 240px; overflow-y: auto; }
.tf-brow-row { display: flex; align-items: flex-end; gap: 10px; flex-wrap: wrap; }
.tf-field { display: flex; flex-direction: column; gap: 4px; font-size: 11px; }
.tf-field.inline { min-width: 160px; }
.tf-field.grow { flex: 1; min-width: 180px; }
.tf-field-label { font-size: 11px; font-weight: 600; color: var(--ui-text-2); }
.tf-input {
  background: var(--ui-surface); border: 1px solid var(--ui-border); border-radius: 8px;
  color: var(--ui-text); font-family: inherit; font-size: 12px; padding: 7px 10px; outline: none; min-width: 0;
}
.tf-input:focus { border-color: var(--ui-accent); box-shadow: var(--ui-focus-ring); }
.tf-input.xs { font-size: 11px; padding: 4px 8px; width: 100%; }
.tf-crumbs { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; font-size: 12px; padding-bottom: 7px; }
.tf-link { background: none; border: none; color: var(--ui-info); cursor: pointer; font: inherit; padding: 0; border-radius: 4px; }
.tf-link:hover { text-decoration: underline; }
.tf-link:focus-visible, .tf-btn:focus-visible, .tf-icon-btn:focus-visible, .tf-node-x:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--ui-accent) 75%, transparent);
  outline-offset: 2px;
}
.tf-icon-btn { background: none; border: 1px solid var(--ui-border); border-radius: 8px; color: color-mix(in srgb, var(--ui-text) 65%, transparent); cursor: pointer; padding: 6px; display: inline-flex; }
.tf-icon-btn:hover { color: var(--ui-text); border-color: var(--ui-border-strong); }
.tf-entries { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
.tf-entry { display: flex; align-items: center; gap: 8px; border: 1px solid var(--ui-border); border-radius: 8px; padding: 4px 6px 4px 10px; }
.tf-entry-main {
  flex: 1; display: flex; align-items: center; gap: 9px; min-width: 0;
  background: none; border: none; color: var(--ui-text); font: inherit; font-size: 12px;
  cursor: pointer; text-align: left; padding: 4px 2px; border-radius: 6px;
}
.tf-entry-main:disabled { cursor: default; }
.tf-entry-main:not(:disabled):hover { color: var(--ui-accent); }
.tf-entry-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tf-hint { margin: 0; font-size: 11.5px; line-height: 1.5; color: color-mix(in srgb, var(--ui-text) 60%, transparent); }
.tf-code { font-family: ui-monospace, monospace; font-size: 11px; color: var(--ui-info); }
.muted { color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.small { font-size: 11px; }
.tf-note { margin: 0; font-size: 11.5px; color: var(--ui-info); line-height: 1.5; }
.tf-note.err { color: var(--ui-danger); }
.tf-btn {
  display: inline-flex; align-items: center; gap: 6px;
  background: transparent; border: 1px solid var(--ui-border); border-radius: 8px;
  color: color-mix(in srgb, var(--ui-text) 75%, transparent);
  font-family: inherit; font-size: 12px; font-weight: 600; padding: 7px 12px; cursor: pointer; white-space: nowrap;
}
.tf-btn:hover:not(:disabled) { color: var(--ui-text); border-color: var(--ui-border-strong); }
.tf-btn:disabled { opacity: 0.45; cursor: not-allowed; }
.tf-btn.primary { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent); }
.tf-btn.primary:hover:not(:disabled) { background: var(--ui-accent); color: var(--ui-text); }
.tf-btn.danger { color: var(--ui-danger); border-color: color-mix(in srgb, var(--ui-danger) 50%, transparent); }
.tf-btn.xs { font-size: 10px; padding: 3px 8px; border-radius: 6px; }
.tf-btn.on { color: var(--ui-warning); border-color: color-mix(in srgb, var(--ui-warning) 60%, transparent); }
.tf-canvas-wrap { flex: 1; min-height: 300px; overflow: auto; padding: 12px 14px; }
.tf-canvas {
  position: relative; min-height: 420px; min-width: 640px;
  border: 1px dashed var(--ui-border); border-radius: 12px;
  background:
    radial-gradient(circle, color-mix(in srgb, var(--ui-text) 8%, transparent) 1px, transparent 1px);
  background-size: 22px 22px;
  touch-action: none;
}
.tf-wires { position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; overflow: visible; }
.tf-wire { fill: none; stroke-width: 2; stroke-linecap: round; animation: tf-wire-in 0.35s ease both; }
.tf-wire.op-cp { stroke: var(--ui-accent); color: var(--ui-accent); }
.tf-wire.op-mv { stroke: var(--ui-warning); color: var(--ui-warning); }
.tf-wire.st-done { stroke-dasharray: none; opacity: 0.85; }
.tf-wire.st-error { stroke: var(--ui-danger); color: var(--ui-danger); }
.tf-wire.st-running { stroke-dasharray: 6 4; animation: tf-dash 0.7s linear infinite; }
.tf-wire.st-queued { stroke-dasharray: 2 4; opacity: 0.75; }
@keyframes tf-dash { to { stroke-dashoffset: -10; } }
@keyframes tf-wire-in { from { opacity: 0; } }
@keyframes tf-node-in { from { opacity: 0; } }
@keyframes tf-target-glow {
  0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--ui-accent) 0%, transparent); }
  50% { box-shadow: 0 0 18px 2px color-mix(in srgb, var(--ui-accent) 35%, transparent); }
}
@keyframes tf-dot-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.45; }
}
.tf-node {
  position: absolute; left: 0; top: 0; display: flex; align-items: center; gap: 9px;
  width: 196px; padding: 9px 10px; border-radius: 10px;
  background: color-mix(in srgb, var(--ui-surface) 88%, transparent);
  backdrop-filter: blur(6px);
  border: 1px solid var(--ui-border);
  cursor: grab; user-select: none; z-index: 1;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
  animation: tf-node-in 0.3s ease both;
  animation-delay: var(--d, 0ms);
}
.tf-node:active { cursor: grabbing; }
.tf-node.dragging { z-index: 5; will-change: transform; box-shadow: 0 12px 32px rgb(0 0 0 / 0.45); }
.tf-node.file.selected { border-color: color-mix(in srgb, var(--ui-accent) 65%, transparent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--ui-accent) 15%, transparent); }
.tf-node.prov { width: 220px; flex-wrap: wrap; }
.tf-node.prov.target { border-style: dashed; border-color: color-mix(in srgb, var(--ui-accent) 60%, transparent); animation: tf-node-in 0.3s ease both, tf-target-glow 1.8s ease-in-out infinite; animation-delay: var(--d, 0ms), 0ms; }
.tf-node.prov.drop-hover { border-color: var(--ui-accent); border-style: solid; box-shadow: 0 0 22px 3px color-mix(in srgb, var(--ui-accent) 45%, transparent); }
.tf-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
.tf-dot.on { background: var(--ui-accent); animation: tf-dot-pulse 2s ease-in-out infinite; }
.tf-dot.off { background: color-mix(in srgb, var(--ui-text) 25%, transparent); }
.tf-node-icon { color: var(--ui-accent); flex-shrink: 0; display: inline-flex; }
.tf-node-meta { display: flex; flex-direction: column; flex: 1; min-width: 0; }
.tf-node-name { font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tf-node-x { background: none; border: none; color: color-mix(in srgb, var(--ui-text) 45%, transparent); cursor: pointer; padding: 2px 4px; border-radius: 4px; font-size: 11px; }
.tf-node-x:hover { color: var(--ui-danger); }
.tf-target { display: flex; align-items: center; gap: 6px; flex-basis: 100%; margin-top: 2px; }
.tf-canvas-hint {
  position: absolute; left: 50%; top: 50%; transform: translate(-50%, -50%);
  margin: 0; max-width: 340px; text-align: center; font-size: 12px; line-height: 1.6;
  color: color-mix(in srgb, var(--ui-text) 50%, transparent); pointer-events: none;
}
.tf-plan { border-top: 1px solid var(--ui-border); padding: 10px 14px; display: flex; flex-direction: column; gap: 6px; max-height: 190px; overflow-y: auto; }
.tf-edge {
  display: flex; flex-direction: column; gap: 2px;
  border: 1px solid var(--ui-border); border-radius: 8px; padding: 6px 8px 6px 12px; font-size: 12px;
  animation: tf-node-in 0.25s ease both;
}
.tf-edge-top { display: flex; align-items: center; gap: 8px; }
.tf-edge-detail { margin: 0; font-size: 11px; line-height: 1.5; color: var(--ui-danger); white-space: pre-wrap; word-break: break-word; }
.tf-edge-detail.warn { color: var(--ui-warning); }
.tf-edge.st-done { border-color: color-mix(in srgb, var(--ui-accent) 50%, transparent); }
.tf-edge.st-error { border-color: color-mix(in srgb, var(--ui-danger) 55%, transparent); }
.tf-edge-route { flex: 1; min-width: 0; display: flex; flex-direction: column; overflow: hidden; }
.tf-edge-route .muted { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tf-edge-status { font-size: 11px; font-weight: 600; }
.tf-edge-status.st-done { color: var(--ui-accent); }
.tf-edge-status.st-error { color: var(--ui-danger); }
.tf-edge-status.st-running { color: var(--ui-info); }
.tf-foot {
  display: flex; align-items: center; justify-content: space-between; gap: 10px;
  padding: 8px 14px; border-top: 1px solid var(--ui-border);
}
.tf-foot-actions { display: flex; align-items: center; gap: 12px; }
@media (prefers-reduced-motion: reduce) {
  .tf-progress-fill, .tf-btn, .tf-node, .tf-wire, .tf-wire.st-running, .tf-dot.on, .tf-node.prov.target { transition: none; animation: none; }
}
@media (max-width: 720px) {
  .tf-node { width: 170px; }
  .tf-node.prov { width: 190px; }
}
</style>
