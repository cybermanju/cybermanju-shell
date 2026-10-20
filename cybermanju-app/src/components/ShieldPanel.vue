<!-- CyberManju OS — ShieldPanel (src/components/ShieldPanel.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="shield-panel">
    <div class="panel-header">
      <div class="header-left">
        <span class="header-icon"><AppIcon name="solar:shield-check-bold" :size="16" /></span>
        <div class="header-text">
          <h2 class="panel-title">Security</h2>
          <p class="panel-subtitle">Quantum-resistant encryption · cascade compression</p>
        </div>
      </div>
      <UiButton size="sm" variant="ghost" icon="solar:close-bold" icon-only title="Close" aria-label="Close" @click="$emit('close')" />
    </div>

    <div class="sh-tabs" role="tablist" aria-label="Shield views">
      <button role="tab" :aria-selected="activeTab === 'shield'" :class="{ on: activeTab === 'shield' }" type="button" @click="activeTab = 'shield'">
        <AppIcon name="solar:shield-check-bold" :size="13" /> Shield
      </button>
      <button role="tab" :aria-selected="activeTab === 'compress'" :class="{ on: activeTab === 'compress' }" type="button" @click="activeTab = 'compress'">
        <AppIcon name="solar:archive-bold" :size="13" /> Compress
      </button>
      <button role="tab" :aria-selected="activeTab === 'recovery'" :class="{ on: activeTab === 'recovery' }" type="button" @click="activeTab = 'recovery'">
        <AppIcon name="solar:key-bold" :size="13" /> {{ t('recovery.title') }}
      </button>
    </div>

    <!-- ══ SHIELD (merged encryption panel) ══ -->
    <div v-if="activeTab === 'shield'" class="sh-shield">
      <div class="status-hero" :class="{ 'is-protected': encryptionStatus?.isEncrypted }">
        <div class="status-top">
          <UiBadge
            :tone="encryptionStatus?.isEncrypted ? 'success' : 'warning'"
            :icon="encryptionStatus?.isEncrypted ? 'solar:shield-check-bold' : 'solar:shield-warning-bold'"
            pulse
          >
            {{ encryptionStatus?.isEncrypted ? 'Protected' : 'Not protected' }}
          </UiBadge>
          <span v-if="encryptionStatus?.isEncrypted" class="status-algo">{{ encryptionStatus.algorithm || 'Unknown' }}</span>
        </div>

        <div v-if="encryptionStatus?.isEncrypted" class="status-meta">
          <div class="meta-row">
            <span class="meta-label">Key</span>
            <span class="mono meta-value">{{ encryptionStatus.keyId || '—' }}</span>
          </div>
          <div v-if="encryptionStatus.encryptedAt" class="meta-row">
            <span class="meta-label">Sealed</span>
            <span class="meta-value">{{ formatDate(encryptionStatus.encryptedAt) }}</span>
          </div>
        </div>
        <p v-else class="unprotected-msg">No encryption active yet. Generate a key below to protect your files.</p>

        <div class="nist-meter" :title="`NIST strength level ${encryptionStatus?.nistLevel || 0} of 5`">
          <span class="nist-label">NIST strength</span>
          <div class="nist-segs" aria-hidden="true">
            <span v-for="n in 5" :key="n" class="nist-seg" :class="{ filled: n <= (encryptionStatus?.nistLevel || 0) }" />
          </div>
          <span class="nist-num mono">{{ encryptionStatus?.nistLevel || 0 }}/5</span>
        </div>
      </div>

      <p class="sh-note" :class="webLocked ? 'is-warn' : 'is-info'">
        <AppIcon :name="webLocked ? 'solar:danger-circle-bold' : 'solar:info-circle-bold'" :size="13" />
        <span>{{ cryptoNote }}</span>
        <UiButton v-if="webLocked" size="sm" variant="ghost" icon="solar:download-bold" @click="openInstall">Get the native app</UiButton>
      </p>

      <section class="sh-section keygen-section" aria-label="Generate a key">
        <h3 class="section-title"><AppIcon name="solar:key-bold" :size="13" /> New key</h3>
        <div class="algo-grid">
          <div v-for="(info, algo) in ENCRYPTION_INFO" :key="algo" class="algo-card">
            <div class="algo-card-top">
              <span class="algo-card-name">{{ info.name }}</span>
              <UiBadge tone="accent" size="sm">L{{ info.nistLevel }}</UiBadge>
            </div>
            <p class="algo-card-desc">{{ info.description }}</p>
            <UiButton
              size="sm"
              variant="secondary"
              icon="solar:key-bold"
              block
              :disabled="webLocked"
              :title="webLocked ? 'Needs the desktop app or offline build' : `Generate ${info.name} keypair`"
              @click="handleGenerate(algo as EncryptionAlgo)"
            >Generate</UiButton>
          </div>
        </div>
      </section>

      <section v-if="encryptionKeys.length > 0" class="sh-section keys-section" aria-label="Stored keys">
        <h3 class="section-title"><AppIcon name="solar:key-bold" :size="13" /> Keys ({{ encryptionKeys.length }})</h3>
        <div class="keys-list">
          <div v-for="key in encryptionKeys" :key="key.id" class="key-card">
            <div class="key-header">
              <span class="key-algo">{{ key.algorithmDisplay }}</span>
              <UiBadge tone="neutral" size="sm">L{{ key.nistLevel }}</UiBadge>
            </div>
            <div class="key-pub mono">{{ key.publicKeyPreview.slice(0, 24) }}…</div>
            <div class="key-date">{{ formatDate(key.createdAt) }}</div>
          </div>
        </div>
      </section>

      <section v-if="selectedFile" class="sh-section encrypt-section" aria-label="Encrypt selected file">
        <h3 class="section-title"><AppIcon name="solar:lock-bold" :size="13" /> Encrypt selected file</h3>
        <p class="file-chip"><AppIcon name="solar:file-bold" :size="12" /> {{ selectedFile.name }}</p>
        <div class="action-row">
          <label class="field">
            <span class="field-label">Algorithm</span>
            <select v-model="encAlgo" class="sh-select" aria-label="Encryption algorithm">
              <option v-for="(info, algo) in ENCRYPTION_INFO" :key="algo" :value="algo">{{ info.name }} (L{{ info.nistLevel }})</option>
            </select>
          </label>
          <UiButton
            variant="primary"
            icon="solar:lock-bold"
            :disabled="webLocked"
            :title="webLocked ? 'Needs the desktop app or offline build' : 'Encrypt file'"
            @click="handleEncrypt"
          >Encrypt</UiButton>
        </div>
      </section>
    </div>

    <!-- ══ COMPRESS (merged compression panel) ══ -->
    <div v-else-if="activeTab === 'compress'" class="sh-compress">
      <section class="sh-section" aria-label="Compression algorithm">
        <h3 class="section-title"><AppIcon name="solar:archive-bold" :size="13" /> Algorithm</h3>
        <div class="algo-grid">
          <button
            v-for="(info, type) in COMPRESSION_INFO"
            :key="type"
            type="button"
            class="algo-pick"
            :class="{ selected: cmpAlgo === type }"
            :disabled="webLocked || !layerCapable(type as CompressionType)"
            :aria-pressed="cmpAlgo === type"
            :title="webLocked || !layerCapable(type as CompressionType) ? 'Not available in this build' : `Compress with ${info.name}`"
            @click="cmpAlgo = type as CompressionType"
          >
            <span class="algo-pick-check"><AppIcon name="solar:check-bold" :size="12" /></span>
            <span class="algo-pick-name">{{ info.name }}</span>
            <span class="algo-pick-speed">{{ info.speed }}</span>
            <span class="algo-pick-desc">{{ info.description }}</span>
          </button>
        </div>
      </section>

      <p class="sh-note" :class="webLocked ? 'is-warn' : 'is-info'">
        <AppIcon :name="webLocked ? 'solar:danger-circle-bold' : 'solar:info-circle-bold'" :size="13" />
        <span>{{ compressNote }}</span>
        <UiButton v-if="webLocked" size="sm" variant="ghost" icon="solar:download-bold" @click="openInstall">Get the native app</UiButton>
      </p>

      <section v-if="selectedFile" class="sh-section" aria-label="Compress selected file">
        <h3 class="section-title"><AppIcon name="solar:file-bold" :size="13" /> Selected file</h3>
        <p class="file-chip"><AppIcon name="solar:file-bold" :size="12" /> {{ selectedFile.name }}</p>
        <UiButton
          variant="primary"
          icon="solar:archive-bold"
          block
          :disabled="webLocked"
          :title="webLocked ? 'Needs the desktop app or offline build' : 'Compress file'"
          @click="handleCompress"
        >Compress with {{ algoName(cmpAlgo) }}</UiButton>
      </section>

      <section v-if="compressionStats" class="sh-section" aria-label="Compression result">
        <h3 class="section-title"><AppIcon name="solar:chart-bold" :size="13" /> Result</h3>
        <div class="stats-card">
          <div class="stat-row">
            <span class="stat-key">Original</span>
            <span class="stat-value">{{ humanBytes(compressionStats.originalSize) }}</span>
          </div>
          <div class="stat-row">
            <span class="stat-key">Compressed</span>
            <span class="stat-value">{{ humanBytes(compressionStats.compressedSize) }}</span>
          </div>
          <div class="stat-row">
            <span class="stat-key">Ratio</span>
            <span class="stat-value">{{ (compressionStats.ratio * 100).toFixed(1) }}%</span>
          </div>
          <div class="ratio-bar" aria-hidden="true">
            <span class="ratio-fill" :style="{ width: `${Math.max(2, Math.min(100, compressionStats.ratio * 100))}%` }" />
          </div>
          <div class="stat-row">
            <span class="stat-key">Time</span>
            <span class="stat-value">{{ compressionStats.durationMs }}ms</span>
          </div>
        </div>
      </section>
    </div>

    <!-- ══ RECOVERY (Shamir m-of-n escrow, ANALYSIS.md §3) ══ -->
    <div v-else class="sh-recovery">
      <p class="sh-note is-info">
        <AppIcon name="solar:info-circle-bold" :size="13" />
        <span>{{ t('recovery.hint') }}</span>
      </p>
      <section class="sh-section" aria-label="Split a secret">
        <h3 class="section-title"><AppIcon name="solar:key-bold" :size="13" /> Split</h3>
        <label class="rec-field">
          <span class="rec-label">PASSPHRASE</span>
          <input v-model="recSecret" class="rec-input" type="password" autocomplete="off" aria-label="Secret to split" />
        </label>
        <div class="rec-nm">
          <label class="rec-field">
            <span class="rec-label">SHARES (N)</span>
            <input v-model.number="recN" class="rec-input" type="number" min="2" max="16" aria-label="Total shares" />
          </label>
          <label class="rec-field">
            <span class="rec-label">NEEDED (M)</span>
            <input v-model.number="recM" class="rec-input" type="number" min="2" max="16" aria-label="Threshold shares" />
          </label>
          <UiButton variant="primary" icon="solar:key-bold" :disabled="!recSecret" @click="splitRecovery">{{ t('recovery.split') }}</UiButton>
        </div>
        <p v-if="recError" class="rec-error">{{ recError }}</p>
      </section>
      <section v-if="recShares.length" class="sh-section" aria-label="Recovery shares">
        <h3 class="section-title"><AppIcon name="solar:diskette-bold" :size="13" /> {{ recShares.length }} shares — store offline</h3>
        <div v-for="s in recShares" :key="s.x" class="share-row">
          <code class="mono share-hex" :title="`Share ${s.x}`">{{ s.x }}:{{ s.hex }}</code>
          <UiButton size="sm" variant="ghost" @click="copyShare(s)">Copy</UiButton>
        </div>
      </section>
      <section v-if="recShares.length" class="sh-section" aria-label="Verify combine">
        <h3 class="section-title"><AppIcon name="solar:check-bold" :size="13" /> {{ t('recovery.combine') }}</h3>
        <textarea v-model="recCombineText" class="rec-input rec-area" rows="4" spellcheck="false" placeholder="x:hex — one share per line" aria-label="Shares to combine" />
        <UiButton size="sm" icon="solar:check-bold" @click="combineRecovery">{{ t('recovery.combine') }}</UiButton>
        <p v-if="recVerdict" class="rec-verdict" :class="{ ok: recOk }">{{ recVerdict }}</p>
      </section>
      <section class="sh-section" aria-label="Key rotation">
        <h3 class="section-title"><AppIcon name="solar:refresh-bold" :size="13" /> Key rotation</h3>
        <p class="sh-note">Rotating re-wraps the master key, then re-cuts these escrow shares — old shares die with the old secret. Work the list in order.</p>
        <ol class="rot-list">
          <li v-for="step in rotationSteps" :key="step.id" class="rot-row">
            <label class="rot-label">
              <input v-model="rotationDone" type="checkbox" :value="step.id" />
              <span><strong>{{ step.title }}.</strong> {{ step.detail }}</span>
            </label>
          </li>
        </ol>
        <p class="rec-verdict" :class="{ ok: rotationDone.length >= rotationSteps.length }">{{ rotationSummary }}</p>
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import UiBadge from '@/components/ui/UiBadge.vue'
import UiButton from '@/components/ui/UiButton.vue'
import { ref, computed, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { humanBytes } from '@/utils/format'
import { isStaticHost } from '@/composables/useTauri'
import { compressionCapable } from '@/composables/useWasmCrypto'
import { requestInstallWindow } from '@/utils/installLinks'
import type { EncryptionAlgo, CompressionType } from '@/types'
import { ENCRYPTION_INFO, COMPRESSION_INFO } from '@/types'
import { combineText, splitText, type ShamirShare } from '@/utils/shamir'
import { rotationChecklist, rotationVerifySummary } from '@/utils/keyRotation'
import { t } from '@/utils/i18n'

const store = useAppStore()
const emit = defineEmits<{ close: [] }>()

/** Open the Install window (native builds per platform, App × CLI). */
function openInstall() {
  requestInstallWindow('needs-app')
}

// Merged shield window: `compression` opens here on the compress tab.
const props = defineProps<{ tab?: string }>()
type ShieldTab = 'shield' | 'compress' | 'recovery'
const activeTab = ref<ShieldTab>(
  props.tab === 'compress' ? 'compress' : props.tab === 'recovery' ? 'recovery' : 'shield',
)
watch(() => props.tab, (t) => { if (t === 'shield' || t === 'compress' || t === 'recovery') activeTab.value = t })

// ── Recovery (Shamir m-of-n escrow, ANALYSIS.md §3) ───────────────────
// Shares render once and are never stored anywhere (no localStorage, no DB,
// no transcript). Verification compares against the in-memory split source.
const recSecret = ref('')
const recN = ref(5)
const recM = ref(3)
const recShares = ref<ShamirShare[]>([])
const recSource = ref('')
const recCombineText = ref('')
const recError = ref('')
const recVerdict = ref('')
const recOk = ref(false)

// ── Key rotation checklist (ANALYSIS.md §3 escrow follow-up) ──────────
// Ordered + idempotent: re-running never invalidates a completed rotation.
const rotationSteps = rotationChecklist()
const rotationDone = ref<string[]>([])
const rotationSummary = computed(() => rotationVerifySummary(rotationDone.value))

function clampRecNM() {
  recN.value = Math.max(2, Math.min(16, Math.round(recN.value || 0)))
  recM.value = Math.max(2, Math.min(16, Math.round(recM.value || 0)))
}

function splitRecovery() {
  recError.value = ''
  recVerdict.value = ''
  recOk.value = false
  clampRecNM()
  try {
    recShares.value = splitText(recSecret.value, recN.value, recM.value)
    recSource.value = recSecret.value
    recCombineText.value = recShares.value.map((s) => `${s.x}:${s.hex}`).join('\n')
  } catch (e) {
    recShares.value = []
    recError.value = e instanceof Error ? e.message : 'split failed'
  }
}

function parseShareLines(text: string): ShamirShare[] | null {
  const out: ShamirShare[] = []
  for (const line of text.split('\n')) {
    const clean = line.trim()
    if (!clean) continue
    const m = /^(\d+)\s*:\s*([0-9a-fA-F]+)$/.exec(clean)
    if (!m) return null
    out.push({ x: Number(m[1]), hex: m[2].toLowerCase() })
  }
  return out
}

function combineRecovery() {
  recVerdict.value = ''
  const shares = parseShareLines(recCombineText.value)
  if (!shares || shares.length < 2) {
    recOk.value = false
    recVerdict.value = 'paste at least 2 shares as x:hex lines'
    return
  }
  try {
    const restored = combineText(shares)
    recOk.value = restored === recSource.value && recSource.value !== ''
    recVerdict.value = recOk.value ? t('recovery.roundTrip') : t('recovery.roundTripFail')
  } catch (e) {
    recOk.value = false
    recVerdict.value = e instanceof Error ? e.message : 'combine failed'
  }
}

async function copyShare(s: ShamirShare) {
  try {
    await navigator.clipboard.writeText(`${s.x}:${s.hex}`)
  } catch {
    /* clipboard unavailable — the hex stays visible for manual copy */
  }
}

// Static/offline build: the wasm pack runs the real ciphers, keys live in
// `.cybermanju`. Dashboard build: no crypto/compression endpoint exists there.
const staticWasm = isStaticHost()
const webLocked = computed(() => !staticWasm)
const cryptoNote = computed(() =>
  webLocked.value
    ? 'Encryption needs the desktop app or the offline browser build — this dashboard build serves no crypto endpoint. Status and key list above are live.'
    : 'Keys and per-file metadata live inside .cybermanju. Bytes are sealed with ChaCha20-Poly1305 (HKDF-derived from your keypair); the ML-KEM / Frodo / AES slots use the nearest WASM cipher and say so.',
)

// The wasm pack runs lz4 + brotli natively plus zstd/triple through the
// bundled `@dweb-browser/zstd-wasm` module (same standard frames as the
// desktop). The dashboard build has no compression endpoint at all.
const compressNote = computed(() => {
  if (webLocked.value) {
    return 'Compression needs the desktop app or the offline browser build — this dashboard build serves no compression endpoint.'
  }
  return 'LZ4 + ZSTD + Brotli + Triple (LZ4 → ZSTD → Brotli, the desktop .cyb3 order) all run in the offline browser build and write into .cybermanju.'
})

function layerCapable(type: CompressionType): boolean {
  return !staticWasm || compressionCapable(type)
}

function algoName(type: CompressionType): string {
  return COMPRESSION_INFO[type]?.name ?? String(type)
}

const encryptionStatus = computed(() => store.encryptionStatus)
const encryptionKeys = computed(() => store.encryptionKeys)
const selectedFile = computed(() => store.selectedFile)
const compressionStats = computed(() => store.compressionStats)

const encAlgo = ref<EncryptionAlgo>('kyber1024')
const cmpAlgo = ref<CompressionType>(staticWasm ? 'lz4' : 'zstd')

function formatDate(iso: string): string {
  if (!iso) return '—'
  const d = new Date(iso)
  return d.toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}

async function handleGenerate(algo: EncryptionAlgo) {
  await store.generateKeypair(algo)
}

async function handleEncrypt() {
  if (!store.selectedFileId) return
  await store.encryptFile(store.selectedFileId, encAlgo.value)
}

async function handleCompress() {
  if (!store.selectedFileId) return
  await store.compressFile(store.selectedFileId, cmpAlgo.value)
}
</script>

<style scoped>
.shield-panel {
  width: 100%;
  min-height: 100%;
  background: var(--ui-surface);
  color: var(--ui-text);
  font-family: var(--ui-font);
  overflow-y: auto;
  overscroll-behavior: contain;
  touch-action: pan-x pan-y;
  scrollbar-gutter: stable;
  padding: 16px;
  box-sizing: border-box;
  overflow-x: hidden;
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.shield-panel::-webkit-scrollbar { width: 10px; }
.shield-panel::-webkit-scrollbar-track { background: transparent; }
.shield-panel::-webkit-scrollbar-thumb {
  background: color-mix(in srgb, var(--ui-accent) 45%, transparent);
  border-radius: var(--ui-radius-full);
  border: 3px solid transparent;
  background-clip: content-box;
}
.shield-panel::-webkit-scrollbar-thumb:hover {
  background: var(--ui-accent);
  background-clip: content-box;
  border: 2px solid transparent;
}

/* ── header ── */
.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--ui-border);
}
.header-left { display: flex; align-items: center; gap: 10px; min-width: 0; }
.header-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  flex-shrink: 0;
  border-radius: var(--ui-radius-md);
  background: var(--ui-accent-soft);
  color: var(--ui-accent);
}
.header-text { min-width: 0; }
.panel-title { font-size: 13px; font-weight: 600; margin: 0; letter-spacing: 0; }
.panel-subtitle {
  margin: 2px 0 0;
  font-size: 11px;
  color: var(--ui-text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ── segmented tabs ── */
.sh-tabs {
  display: flex;
  gap: 2px;
  padding: 3px;
  border-radius: var(--ui-radius-full);
  background: color-mix(in srgb, var(--ui-text) 6%, transparent);
  border: 1px solid var(--ui-border);
}
.sh-tabs button {
  flex: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 7px 0;
  font-family: var(--ui-font);
  font-size: 12px;
  font-weight: 600;
  color: var(--ui-text-3);
  background: transparent;
  border: none;
  border-radius: var(--ui-radius-full);
  cursor: pointer;
  transition:
    background-color var(--ui-dur-fast) var(--ui-ease-out),
    color var(--ui-dur-fast) var(--ui-ease-out),
    box-shadow var(--ui-dur) var(--ui-ease-out);
}
.sh-tabs button.on {
  color: var(--ui-text);
  background: var(--ui-surface-3);
  box-shadow: var(--ui-shadow-1);
}
.sh-tabs button:focus-visible {
  outline: none;
  box-shadow: var(--ui-focus-ring);
}

.sh-shield, .sh-compress { display: flex; flex-direction: column; gap: 14px; min-height: 0; }

/* ── status hero ── */
.status-hero {
  border-radius: var(--ui-radius-lg);
  border: 1px solid var(--ui-border);
  background: var(--ui-surface);
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.status-hero.is-protected {
  border-color: color-mix(in srgb, var(--ui-success) 45%, transparent);
}
.status-top { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.status-algo { font-size: 12px; font-weight: 700; }
.status-meta { display: flex; flex-direction: column; gap: 4px; }
.meta-row { display: flex; align-items: baseline; gap: 8px; font-size: 11px; }
.meta-label {
  min-width: 52px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0;
  color: var(--ui-text-2);
}
.meta-value { min-width: 0; overflow-wrap: anywhere; }
.unprotected-msg { font-size: 12px; line-height: 1.5; color: var(--ui-text-2); margin: 0; }

/* ── NIST meter ── */
.nist-meter { display: flex; align-items: center; gap: 10px; padding-top: 10px; border-top: 1px solid var(--ui-hairline); }
.nist-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0;
  color: var(--ui-text-2);
  white-space: nowrap;
}
.nist-segs { flex: 1; display: flex; gap: 4px; }
.nist-seg {
  flex: 1;
  height: 6px;
  border-radius: var(--ui-radius-full);
  background: color-mix(in srgb, var(--ui-text) 10%, transparent);
}
.nist-seg.filled {
  background: var(--ui-accent);
}
.nist-num { font-size: 10px; font-weight: 700; color: var(--ui-text-3); }

/* ── inline notes ── */
.sh-note {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  margin: 0;
  padding: 10px 12px;
  border-radius: var(--ui-radius-md);
  font-size: 11.5px;
  line-height: 1.55;
  border: 1px solid var(--ui-border);
  background: color-mix(in srgb, var(--ui-text) 4%, transparent);
  color: var(--ui-text-2);
}
.sh-note :deep(svg), .sh-note > :first-child { flex-shrink: 0; margin-top: 1px; }
.sh-note.is-warn {
  border-color: color-mix(in srgb, var(--ui-warning) 40%, transparent);
  background: color-mix(in srgb, var(--ui-warning) 8%, transparent);
  color: var(--ui-text-2);
}
.sh-note.is-info {
  border-color: color-mix(in srgb, var(--ui-accent) 35%, transparent);
  background: var(--ui-accent-softer);
}

/* ── sections ── */
.sh-section { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
.section-title {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0;
  color: var(--ui-text-2);
  margin: 0;
  display: flex;
  align-items: center;
  gap: 6px;
  padding-bottom: 6px;
  border-bottom: 1px solid var(--ui-hairline);
}

/* ── keygen cards ── */
.algo-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 8px; }
.algo-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  border-radius: var(--ui-radius-md);
  border: 1px solid var(--ui-border);
  background: var(--ui-glass);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  box-shadow: var(--ui-shadow-1);
  transition: border-color var(--ui-dur-fast) var(--ui-ease-out), transform var(--ui-dur-fast) var(--ui-ease-out);
}
.algo-card:hover { border-color: var(--ui-border-hover); }
.algo-card-top { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.algo-card-name { font-size: 12px; font-weight: 700; }
.algo-card-desc { font-size: 11px; line-height: 1.45; color: var(--ui-text-3); margin: 0; flex: 1; }

/* ── stored keys ── */
.keys-list { display: flex; flex-direction: column; gap: 8px; }
.key-card {
  border-radius: var(--ui-radius-md);
  border: 1px solid var(--ui-border);
  background: var(--ui-glass);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.key-header { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.key-algo { font-size: 12px; font-weight: 700; }
.key-pub {
  font-size: 10.5px;
  color: var(--ui-text-3);
  background: color-mix(in srgb, var(--ui-text) 6%, transparent);
  border-radius: var(--ui-radius-sm);
  padding: 4px 8px;
  word-break: break-all;
  overflow-wrap: anywhere;
  min-width: 0;
}
.key-date { font-size: 10.5px; color: var(--ui-text-3); }

/* ── file chip + action rows ── */
.file-chip {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0;
  padding: 8px 10px;
  font-size: 12px;
  font-weight: 600;
  border-radius: var(--ui-radius-md);
  border: 1px solid var(--ui-hairline);
  background: color-mix(in srgb, var(--ui-text) 5%, transparent);
  overflow-wrap: anywhere;
}
.action-row { display: flex; gap: 8px; align-items: flex-end; flex-wrap: wrap; }
.field { flex: 1 1 160px; display: flex; flex-direction: column; gap: 4px; min-width: 0; }
.field-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0;
  color: var(--ui-text-2);
}
.sh-select {
  appearance: none;
  -webkit-appearance: none;
  width: 100%;
  min-height: var(--ui-control-h);
  padding: 6px 28px 6px 10px;
  font-family: var(--ui-font);
  font-size: 12px;
  color: var(--ui-text);
  background-color: var(--ui-surface-2);
  background-image:
    linear-gradient(45deg, transparent 50%, var(--ui-text-3) 50%),
    linear-gradient(135deg, var(--ui-text-3) 50%, transparent 50%);
  background-position: calc(100% - 14px) 55%, calc(100% - 9px) 55%;
  background-size: 5px 5px;
  background-repeat: no-repeat;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  cursor: pointer;
}
.sh-select:hover { border-color: var(--ui-border-hover); }
.sh-select:focus-visible { outline: none; border-color: var(--ui-accent); box-shadow: var(--ui-focus-ring); }

/* ── compress picker cards ── */
.algo-pick {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px 12px 12px 34px;
  text-align: left;
  font-family: var(--ui-font);
  color: var(--ui-text);
  border-radius: var(--ui-radius-md);
  border: 1px solid var(--ui-border);
  background: var(--ui-surface);
  cursor: pointer;
  transition: border-color var(--ui-dur-fast) ease-out;
}
.algo-pick:hover:not(:disabled) { border-color: var(--ui-border-hover); }
.algo-pick.selected {
  border-color: var(--ui-accent);
  box-shadow: var(--ui-focus-ring);
}
.algo-pick:disabled { opacity: 0.45; cursor: not-allowed; }
.algo-pick-check {
  position: absolute;
  left: 10px;
  top: 12px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border-radius: var(--ui-radius-full);
  border: 1px solid var(--ui-border-strong);
  color: transparent;
  transition:
    background-color var(--ui-dur-fast) var(--ui-ease-out),
    color var(--ui-dur-fast) var(--ui-ease-out),
    border-color var(--ui-dur-fast) var(--ui-ease-out);
}
.algo-pick.selected .algo-pick-check {
  background: var(--ui-accent);
  border-color: var(--ui-accent);
  color: var(--ui-on-accent);
}
.algo-pick-name { font-size: 12px; font-weight: 700; }
.algo-pick-speed { font-size: 10px; font-weight: 600; color: var(--ui-accent); }
.algo-pick-desc { font-size: 11px; line-height: 1.45; color: var(--ui-text-3); }

/* ── result card ── */
.stats-card {
  border-radius: var(--ui-radius-md);
  border: 1px solid var(--ui-border);
  background: var(--ui-glass);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  box-shadow: var(--ui-shadow-1);
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.stat-row { display: flex; justify-content: space-between; align-items: baseline; font-size: 12px; }
.stat-key { color: var(--ui-text-3); }
.stat-value { font-weight: 700; font-variant-numeric: tabular-nums; }
.ratio-bar {
  height: 8px;
  border-radius: var(--ui-radius-full);
  background: color-mix(in srgb, var(--ui-text) 10%, transparent);
  overflow: hidden;
}
.ratio-fill {
  display: block;
  height: 100%;
  border-radius: var(--ui-radius-full);
  background: linear-gradient(90deg, var(--ui-success), var(--ui-accent));
  box-shadow: 0 0 8px color-mix(in srgb, var(--ui-accent) 45%, transparent);
  transition: width var(--ui-dur-slow) var(--ui-ease-out);
}

.mono { font-family: var(--ui-font-mono); }

@media (max-width: 560px) {
  .shield-panel {
    min-height: 100dvh;
    padding: max(12px, env(safe-area-inset-top)) 12px max(16px, env(safe-area-inset-bottom));
    gap: 12px;
  }
  .panel-header { gap: 8px; }
  .panel-subtitle { white-space: normal; line-height: 1.35; }
  .sh-tabs button,
  .shield-panel :deep(.ui-btn) { min-height: 44px; }
  .sh-tabs button { padding: 10px 8px; }
  .status-hero { padding: 14px 12px; }
  .status-top { align-items: flex-start; flex-direction: column; gap: 8px; }
  .status-algo { overflow-wrap: anywhere; }
  .nist-meter { align-items: flex-start; flex-wrap: wrap; gap: 8px; }
  .nist-segs { flex-basis: 100%; order: 3; min-width: 0; }
  .nist-num { margin-left: auto; }
  .algo-grid { grid-template-columns: minmax(0, 1fr); }
  .action-row { align-items: stretch; flex-direction: column; }
  .action-row .field,
  .action-row .ui-btn { width: 100%; flex: none; }
  .sh-select { min-height: 44px; }
  .key-header { align-items: flex-start; }
  .key-algo { min-width: 0; overflow-wrap: anywhere; }
  .sh-shield > .status-hero { order: 0; }
  .sh-shield > .sh-note { order: 1; }
  .sh-shield > .encrypt-section { order: 2; }
  .sh-shield > .keys-section { order: 3; }
  .sh-shield > .keygen-section { order: 4; }
}

/* ── recovery (Shamir escrow) ── */
.sh-recovery { display: flex; flex-direction: column; gap: 2px; }
.rec-field { display: flex; flex-direction: column; gap: 4px; margin-bottom: 8px; }
.rec-label { font-size: 10px; letter-spacing: 1px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.rec-input {
  background: transparent;
  border: 1px solid var(--ui-border);
  color: var(--ui-text);
  font-family: inherit;
  font-size: 12px;
  padding: 6px 8px;
  min-width: 0;
  width: 100%;
  box-sizing: border-box;
}
.rec-nm { display: flex; gap: 8px; align-items: flex-end; flex-wrap: wrap; }
.rec-nm .rec-field { flex: 1; min-width: 90px; }
.rec-area { font-family: var(--ui-font-mono); resize: vertical; }
.rec-error { color: var(--ui-danger); font-size: 12px; }
.rec-verdict { font-size: 12px; }
.rec-verdict.ok { color: var(--ui-accent); }
.share-row { display: flex; gap: 8px; align-items: center; margin-bottom: 6px; }
.rot-list { margin: 4px 0 0; padding-left: 18px; display: flex; flex-direction: column; gap: 6px; }
.rot-row { font-size: 12px; }
.rot-label { display: flex; gap: 8px; align-items: flex-start; cursor: pointer; }
.share-hex {
  flex: 1;
  font-size: 11px;
  word-break: break-all;
  border: 1px solid var(--ui-border);
  padding: 6px 8px;
  min-width: 0;
}

@media (prefers-reduced-motion: reduce) {
  .shield-panel *,
  .shield-panel *::before,
  .shield-panel *::after {
    scroll-behavior: auto !important;
    transition-duration: 0.01ms !important;
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
  }
}
</style>
