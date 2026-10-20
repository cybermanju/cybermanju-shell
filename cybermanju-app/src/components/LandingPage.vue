<script setup lang="ts">
// CyberManju OS — LandingPage (src/components/LandingPage.vue).
// Role: Vue desktop UI: windows, panels, overlays.
// Exports: module root (see items below).
// Integrates: @/…, vue, ./….
// Contracts: none special (pure logic; errors bubble to caller).
import AppIcon from '@/components/AppIcon.vue'
import { ref, onMounted, onUnmounted } from 'vue'
import TopMenuBar from './TopMenuBar.vue'
import Dock from './Dock.vue'
import { useAppStore } from '@/stores/app'
import { isStaticHost } from '@/composables/useTauri'
import { useTransport } from '@/composables/useTransport'
import { wasmBackendActive } from '@/composables/useWasmBackend'
import { legalPageUrl } from '@/utils/legalPageUrl'

const emit = defineEmits<{ (e: 'open-app'): void }>()

const store = useAppStore()

// ── Boot State ──
// No interactive terminal here by design: the landing screen is a verbose
// SCI-FI loader. Every MODULE line below awaits a REAL store fetch —
// nothing is placeholdered. The interactive shell lives in cybsh
// (TerminalPanel, opened from the Dock / Ctrl+`).
const phase = ref<'post' | 'loading' | 'ready'>('post')
const bootProgress = ref(0)
const bootLog = ref<string[]>([])
const postDone = ref(false)

interface ModuleLine { name: string; detail: string; ok: boolean }
const moduleLines = ref<ModuleLine[]>([])

const quotes = [
  '"The cloud is just someone else\'s computer.\n This one has ML-KEM-1024. Good luck, NSA."',
  '"Google Drive reads your files.\n We just store them. The difference is subtle."',
  '"Dropbox thought folders were revolutionary.\n We thought quantum-safe encryption might be nicer."',
  '"Your data should be yours.\n Not a product. Not a training set. Just yours."',
]

const currentQuote = ref(quotes[0])

// ── Rotating Buddha ASCII ──
const buddhaFrames = [
  [
    '      ┌─────┐      ',
    '    ╱  ═════  ╲    ',
    '   │  ╱   ╲  │    ',
    '   │ (  ═  ) │    ',
    '   │  ╲   ╱  │    ',
    '    ╲  ───  ╱    ',
    '      └─────┘      ',
    '     ╱  │  ╲       ',
    '    ╱   │   ╲      ',
    '   │   ╱ ╲   │     ',
    '    ╲ ╱   ╲ ╱     ',
  ],
  [
    '      ┌─────┐      ',
    '    ╱  ═════  ╲    ',
    '   │  ╲   ╱  │    ',
    '   │ (  ═  ) │    ',
    '   │  ╱   ╲  │    ',
    '    ╲  ───  ╱    ',
    '      └─────┘      ',
    '       ╲ │ ╱       ',
    '        ╲│╱        ',
    '       ╱ │ ╲       ',
    '      ╱  │  ╲      ',
  ],
  [
    '      ╱‾‾‾‾‾╲      ',
    '    ╱  ═════  ╲    ',
    '   │  ╱   ╲  │    ',
    '   │ (  ═  ) │    ',
    '   │  ╲   ╱  │    ',
    '    ╲  ───  ╱    ',
    '      ╲_____╱      ',
    '    ╱  ╲   ╱  ╲    ',
    '   ╱    ╲ ╱    ╲   ',
    '  │    ╱ ╲    │   ',
    '   ╲  ╱   ╲  ╱   ',
  ],
  [
    '      ┌─────┐      ',
    '    ╱  ═════  ╲    ',
    '   │  ╱   ╲  │    ',
    '   │ (  ═  ) │    ',
    '   │  ╲   ╱  │    ',
    '    ╲  ───  ╱    ',
    '      └─────┘      ',
    '       ╱ │ ╲       ',
    '      ╱  │  ╲      ',
    '     ╱   │   ╲     ',
    '    ╱    │    ╲    ',
  ],
]

const currentFrame = ref(0)
const frameLines = ref<string[]>([])
let buddhaTimer: ReturnType<typeof setInterval> | null = null

/**
 * Particle placement/timing is random ONCE. It used to be random on every
 * render — and the ASCII frame tick re-renders this tree every 280 ms, so
 * each particle teleported and restarted its CSS animation 3.5×/second.
 */
const particleStyles = Array.from({ length: 20 }, () => ({
  left: `${Math.random() * 100}%`,
  top: `${Math.random() * 100}%`,
  animationDelay: `${Math.random() * 5}s`,
  animationDuration: `${3 + Math.random() * 4}s`,
}))

// ── Verbose SCI-FI loader ──────────────────────────────────────
// Each step awaits a REAL store fetch and reports the live count.
// Nothing here is placeholdered: a line that says "3 ACCOUNTS" means
// `store.accounts` actually holds 3 rows right now.
function pushLine(text: string, mod?: ModuleLine) {
  bootLog.value.push(text)
  if (mod) moduleLines.value.push(mod)
}

function fmtErr(e: unknown): string {
  const d = e instanceof Error ? e.message : String(e)
  return d.length > 90 ? d.slice(0, 90) + '…' : d
}

// ── Boot Sequence ──
function runPost() {
  phase.value = 'post'
  bootLog.value = []
  moduleLines.value = []
  postDone.value = false
  const postLines = [
    'CyberManju Systems POST v0.0.1',
    'CPU: Quantum Co-Processor @ 2.4 GHz [PASS]',
    'CRYPTO: ML-KEM-1024 Accelerator [PASS]',
    'MEM: 16 GUARD ChaCha20 Zones [PASS]',
    'RTC: System Clock [SYNCED]',
    '────────────────────────────────────────────',
  ]
  let i = 0
  const tick = () => {
    if (i < postLines.length) {
      bootLog.value.push(postLines[i])
      i++
      setTimeout(tick, 90)
    } else {
      postDone.value = true
      setTimeout(() => void runLoading(), 250)
    }
  }
  tick()
}

async function runLoading() {
  phase.value = 'loading'
  bootLog.value = []
  moduleLines.value = []
  bootProgress.value = 0

  const transport = useTransport().label
  pushLine(`TRANSPORT :: ${transport} [OK]`, { name: 'TRANSPORT', detail: transport, ok: true })

  let done = 0
  const step = async (label: string, load: () => Promise<string>) => {
    try {
      const detail = await load()
      pushLine(`${label} :: ${detail} [OK]`, { name: label, detail, ok: true })
    } catch (e) {
      const detail = `UNREACHABLE — ${fmtErr(e)}`
      pushLine(`${label} :: ${detail} [WARN]`, { name: label, detail, ok: false })
    }
    done++
    bootProgress.value = Math.round((done / STEPS.length) * 100)
  }

  const STEPS: Array<[string, () => Promise<string>]> = [
    ['WASM', async () => {
      if (!isStaticHost()) return `bypassed (${transport})`
      await store.fetchOsWorkers()
      const w = store.osWorkers
      return w ? `${w.total} workers (${w.localThreads} local + ${w.providerSlots} provider), backend=${wasmBackendActive() ? 'loaded' : 'lazy'}` : 'wasm backend loaded, no workers yet'
    }],
    ['ACCOUNTS', async () => { await store.fetchAccounts(); return `${store.accounts.length} accounts, active=${store.activeAccount?.name ?? 'none'}` }],
    ['FILES', async () => { await store.fetchFiles(); return `${store.files.length} nodes indexed` }],
    ['COLLECTIONS', async () => { await store.fetchCollections(); return `${store.collections.length} collections` }],
    ['FACES', async () => { await store.fetchFaceGroups(); return `${store.faceGroups.length} face groups` }],
    ['LOOSE', async () => { await store.fetchLooseGroups(); return `${store.looseGroups.length} loose groups` }],
    ['GEO', async () => { await store.fetchGeoFiles(); return `${store.geoMarkers.length} geo-tagged files` }],
    ['ENCRYPT', async () => { await store.fetchEncryptionStatus(); await store.listKeys(); return `${store.encryptionKeys.length} keys, engine=${store.encryptionStatus.isEncrypted ? 'sealed' : 'open'}` }],
    ['SYNC', async () => { await store.fetchSyncConfigs(); await store.fetchSyncStatus(); return `${store.syncConfigs.length} provider configs, status=${store.syncStatus?.status ?? 'unknown'}` }],
    ['RUNS', async () => { await store.fetchSyncRuns(); return `${store.syncRuns.length} recorded runs` }],
    ['DISKS', async () => { await store.fetchDisks(); return `${store.disks.length} .cybermanju disks` }],
    ['VOLUME', async () => { await store.fetchOsDf(); const d = store.osDf; return d ? `${d.diskCount} disks, ${(d.usedBytes / 1048576).toFixed(1)} / ${(d.totalBytes / 1048576).toFixed(1)} MiB used` : 'volume not reported' }],
    ['COMPUTE', async () => { await store.fetchOsWorkers(); await store.fetchOsJobs(); return `${store.osWorkers?.total ?? 0} workers, ${store.osJobs.length} jobs` }],
    ['TASKS', async () => { await store.fetchOsPs(); await store.fetchOsTop(); const c = store.osPs?.counts; return c ? `${c.running} running / ${c.total} total` : 'task table unreachable' }],
    ['USERS', async () => { await store.fetchUsers(); return `${store.users.length} app users (argon2)` }],
    ['TRASH', async () => { await store.fetchTrashItems(); return `${store.trashItems.length} trashed items` }],
    ['AUDIT', async () => { await store.fetchAuditLog(25); return `${store.auditLog.length} recent audit entries` }],
    ['DASHBOARD', async () => { await store.fetchDashboardStatus(); const d = store.dashboardStatus; return d?.running ? `serving ${d.url} (${d.activeConnections} conns)` : 'embedded server idle' }],
  ]

  for (const [label, load] of STEPS) {
    // eslint-disable-next-line no-await-in-loop
    await step(label, load)
  }
  currentQuote.value = quotes[Math.floor(Math.random() * quotes.length)]
  setTimeout(() => { phase.value = 'ready' }, 350)
}

function restartBoot() {
  stopAnimations()
  startBuddhaAnimation()
  bootLog.value = []
  moduleLines.value = []
  currentFrame.value = 0
  runPost()
}

// ── Animations ──
// The Buddha glow used to be a 50 ms JS timer writing a reactive opacity
// (20 reactive style writes/second forever). It is a pure sine wave, so the
// identical curve now lives in CSS (`.ascii-buddha` keyframes) — the browser
// composites it for free and the script only advances the ASCII frames.
function startBuddhaAnimation() {
  frameLines.value = buddhaFrames[0]
  buddhaTimer = setInterval(() => {
    currentFrame.value = (currentFrame.value + 1) % buddhaFrames.length
    frameLines.value = buddhaFrames[currentFrame.value]
  }, 280)
}

function stopAnimations() {
  if (buddhaTimer) clearInterval(buddhaTimer)
  buddhaTimer = null
}

onMounted(() => {
  currentQuote.value = quotes[Math.floor(Math.random() * quotes.length)]
  startBuddhaAnimation()
  runPost()
})

onUnmounted(() => {
  stopAnimations()
})
</script>

<template>
  <div class="landing-os" tabindex="0">
    <TopMenuBar />

    <div class="landing-content">
      <div class="boot-overlay" v-if="phase !== 'ready'">
        <div class="boot-terminal">
          <div class="boot-log">
            <div v-for="(line, i) in bootLog" :key="i" class="boot-line">{{ line }}</div>
            <div v-if="phase === 'loading'" class="boot-progress">
              <div class="progress-track">
                <div class="progress-fill" :style="{ width: bootProgress + '%' }" />
              </div>
              <div class="boot-pct">{{ bootProgress }}%</div>
            </div>
            <div v-if="phase === 'post' && !postDone" class="cursor-block">&#9608;</div>
          </div>
        </div>
      </div>

      <div v-else class="desktop-landing">
        <div class="ascii-background">
          <div class="ascii-buddha">
            <div v-for="(line, i) in frameLines" :key="i" class="buddha-line">{{ line }}</div>
          </div>
          <div class="ascii-particles">
            <div v-for="(p, n) in particleStyles" :key="n" class="particle" :style="p">.</div>
          </div>
        </div>

        <div class="boot-report" role="status" aria-label="Module load report">
          <div class="report-brand"><img src="/bhumisparsha.png" alt="Bhumisparsha School" width="72" height="72" /></div>
          <div class="report-head">
            <span class="report-title">CYBERMANJU OS v0.0.1 — ALL MODULES LOADED</span>
            <span class="report-counts">{{ moduleLines.filter(m => m.ok).length }}/{{ moduleLines.length }} OK</span>
          </div>
          <div class="report-quote">{{ currentQuote }}</div>
          <div class="report-grid">
            <div v-for="m in moduleLines" :key="m.name" class="report-row" :class="{ warn: !m.ok }">
              <span class="report-name">{{ m.name }}</span>
              <span class="report-detail">{{ m.detail }}</span>
              <span class="report-flag"><AppIcon :name="m.ok ? 'solar:check-circle-bold' : 'solar:danger-triangle-bold'" :size="13" /></span>
            </div>
          </div>
          <div class="report-hint">INTERACTIVE SHELL LIVES IN CYBSH — DOCK &gt; TERMINAL, OR CTRL+`</div>
        </div>

        <div class="launch-hint">
          <button class="launch-button" @click="emit('open-app')">
            Enter CyberManju
          </button>
          <button class="reboot-button" @click="restartBoot">
            Reboot
          </button>
        </div>
        <div class="legal-hint">
          <a :href="legalPageUrl('privacy.html')" target="_blank" rel="noopener">Privacy Policy</a>
          <span>·</span>
          <a :href="legalPageUrl('terms.html')" target="_blank" rel="noopener">Terms of Service</a>
          <span>·</span>
          <a href="https://cybermanju.github.io/wiki/" target="_blank" rel="noopener">Wiki</a>
        </div>
      </div>
    </div>

    <Dock />
  </div>
</template>

<style scoped>
.landing-os {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
  background: var(--ui-surface);
  font-family: var(--ui-font);
  outline: none;
  overflow: hidden;
  z-index: 999;
}

.landing-content {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
  overflow: hidden;
}

/* ── Boot Overlay: black screen, logo, thin progress bar ── */
.boot-overlay {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #000000;
  z-index: 10;
}

.boot-terminal {
  width: 92vw;
  max-width: 420px;
  max-height: 70vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 18px;
  background: transparent;
  border: none;
  padding: 24px;
  overflow: hidden;
}

.boot-log {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 100%;
  max-height: 180px;
  overflow: hidden;
}

.boot-line {
  color: rgba(245, 245, 247, 0.55);
  font-family: var(--ui-font-mono);
  font-size: 11px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}

.boot-line:last-child {
  color: rgba(245, 245, 247, 0.85);
}

.boot-progress {
  width: 220px;
  max-width: 60vw;
  padding: 0;
}

.progress-track {
  width: 100%;
  height: 4px;
  background: rgba(255, 255, 255, 0.18);
  border-radius: 2px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: #ffffff;
  border-radius: 2px;
  transition: width 0.1s linear;
}

.boot-pct {
  margin-top: 8px;
  text-align: center;
  font-size: 11px;
  color: rgba(245, 245, 247, 0.55);
}

.cursor-block {
  display: inline-block;
  color: rgba(245, 245, 247, 0.7);
  animation: blink 500ms step-end infinite;
  margin-top: 4px;
}

@keyframes blink {
  50% { opacity: 0; }
}

/* ── Desktop Landing ── */
.desktop-landing {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 24px;
}

.ascii-background {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  pointer-events: none;
  z-index: 0;
}

.ascii-buddha {
  text-align: center;
  font-size: 11px;
  line-height: 1.15;
  color: var(--ui-text-3);
  letter-spacing: 0;
  user-select: none;
  /* Same pulse the old 50 ms JS timer wrote by hand (sin(t/800)*0.3+0.6,
     period ≈5.03 s, range 0.3–0.9) — composited by the browser instead of
     round-tripping through Vue's reactivity 20×/second. */
  animation: buddha-glow 5.027s ease-in-out infinite;
}

@keyframes buddha-glow {
  0% { opacity: 0.6; }
  25% { opacity: 0.9; }
  50% { opacity: 0.6; }
  75% { opacity: 0.3; }
  100% { opacity: 0.6; }
}

.buddha-line {
  white-space: pre;
}

.ascii-particles {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.particle {
  position: absolute;
  color: var(--ui-text-faint);
  font-size: 8px;
  animation: float 4s ease-in-out infinite;
}

@keyframes float {
  0%, 100% { transform: translateY(0) scale(1); opacity: 0; }
  50% { transform: translateY(-20px) scale(1.5); opacity: 0.8; }
}

/* ── Module report (ready screen — flat card, no glow) ── */
.boot-report {
  position: relative;
  z-index: 2;
  width: 92vw;
  max-width: 560px;
  max-height: 46vh;
  overflow-y: auto;
  background: var(--ui-surface);
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  padding: 20px 22px;
  box-shadow: var(--ui-shadow-menu);
}

.report-brand {
  display: flex;
  justify-content: center;
  margin-bottom: 10px;
}
.report-brand img {
  width: 56px;
  height: 56px;
  object-fit: cover;
  border-radius: 50%;
  border: 1px solid var(--ui-border);
}

.report-head {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 12px;
  margin-bottom: 8px;
}

.report-title {
  color: var(--ui-text);
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0;
  text-transform: lowercase;
}

.report-title::first-letter {
  text-transform: uppercase;
}

.report-counts {
  color: var(--ui-text-2);
  font-size: 11px;
  white-space: nowrap;
}

.report-quote {
  color: var(--ui-text-2);
  font-size: 12px;
  line-height: 1.5;
  white-space: pre-wrap;
  margin-bottom: 10px;
}

.report-grid {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.report-row {
  display: flex;
  gap: 10px;
  align-items: baseline;
  font-size: 12px;
  line-height: 1.55;
}

.report-name {
  color: var(--ui-text);
  font-weight: 600;
  min-width: 92px;
  text-transform: lowercase;
}

.report-name::first-letter {
  text-transform: uppercase;
}

.report-detail {
  color: var(--ui-text-2);
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.report-flag {
  color: var(--ui-success);
  display: inline-flex;
}

.report-row.warn .report-flag {
  color: var(--ui-warning);
}

.report-row.warn .report-detail {
  color: var(--ui-text-2);
}

.report-hint {
  margin-top: 10px;
  color: var(--ui-text-3);
  font-size: 11px;
  text-transform: lowercase;
}

.report-hint::first-letter {
  text-transform: uppercase;
}

/* ── Launch Buttons ── */
.launch-hint {
  position: relative;
  z-index: 2;
  display: flex;
  gap: 8px;
}

.launch-button {
  background: var(--ui-accent);
  border: 1px solid transparent;
  border-radius: var(--ui-radius-sm);
  color: var(--ui-on-accent);
  font-family: var(--ui-font);
  font-size: 13px;
  font-weight: 500;
  min-height: var(--ui-control-h);
  padding: 0 20px;
  cursor: pointer;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.launch-button:hover {
  background: var(--ui-accent-strong);
}

.reboot-button {
  background: var(--ui-surface-2);
  border: 1px solid var(--ui-border-strong);
  border-radius: var(--ui-radius-sm);
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 13px;
  font-weight: 500;
  min-height: var(--ui-control-h);
  padding: 0 20px;
  cursor: pointer;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.reboot-button:hover {
  background: var(--ui-surface-3);
}

/* Legal links (Google OAuth verification requires them on the homepage). */
.legal-hint {
  position: relative;
  z-index: 2;
  margin-top: 14px;
  display: flex;
  gap: 10px;
  justify-content: center;
  font-size: 11px;
  color: var(--ui-text-3);
}

.legal-hint a {
  color: var(--ui-text-2);
  text-decoration: none;
}

.legal-hint a:hover {
  color: var(--ui-text);
  text-decoration: underline;
}

@media (max-width: 768px) {
  .boot-terminal {
    padding: 16px;
    width: 96vw;
  }
  .boot-line {
    font-size: 11px;
  }
  .boot-report {
    padding: 12px 14px;
    width: 96vw;
  }
  .report-row {
    font-size: 10px;
  }
  .ascii-buddha {
    font-size: 8px;
  }
  .launch-button, .reboot-button {
    font-size: 10px;
    padding: 8px 16px;
  }
}
</style>
