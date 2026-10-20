<!-- CyberManju OS — first-run setup wizard.
//
// Seven one-job steps, every one skippable: welcome → vault folder (pick one
// folder: auto-opens the `.cybermanju` inside, or creates it with your
// name + size, plus its `files/` local copies) → accounts (Google/GitHub/
// GitLab one-click sign-in, inline) → disks (one system disk per provider,
// with its cloud folder + files) → agent AI (optional) → appearance (theme
// + wallpaper, live preview) → done summary.
// Shown once on first launch; Help → "Setup wizard" re-opens it any time.
// Closing via ✕ or Escape leaves the "seen" flag unset, so the wizard
// returns next launch until finish/skip. -->
<template>
  <div class="sw-overlay" role="presentation" @keydown.esc="close">
    <div
      ref="cardRef"
      class="sw-card"
      role="dialog"
      aria-modal="true"
      aria-labelledby="sw-title"
      tabindex="-1"
    >
      <header class="sw-head">
        <div class="sw-brand">
          <span class="sw-mark"><img src="/bhumisparsha.png" alt="Bhumisparsha" width="36" height="36" /></span>
          <div>
            <h2 id="sw-title" class="sw-title">Set up CyberManju OS</h2>
            <p class="sw-sub">{{ SETUP_STEP_LABELS[step] }} · {{ setupStepIndex(step) }} / {{ SETUP_STEPS.length }}</p>
          </div>
        </div>
        <button class="sw-icon-btn" type="button" title="Close (returns next launch)" aria-label="Close setup wizard" @click="close">
          <AppIcon name="solar:close-circle-bold" :size="18" />
        </button>
      </header>

      <nav class="sw-dots" aria-label="Setup progress">
        <button
          v-for="s in SETUP_STEPS"
          :key="s"
          type="button"
          class="sw-dot"
          :class="{ on: s === step, past: setupStepIndex(s) < setupStepIndex(step) }"
          :title="SETUP_STEP_LABELS[s]"
          :aria-label="SETUP_STEP_LABELS[s]"
          @click="go(s)"
        />
      </nav>

      <main class="sw-body">
        <!-- ── 1 · welcome ── -->
        <section v-if="step === 'welcome'" class="sw-section">
          <p class="sw-lead">A few quick things. Skip anything.</p>
          <ul class="sw-list">
            <li><AppIcon name="solar:folder-bold" :size="16" /><span><strong>Vault folder</strong> — one folder holds the vault + its files.</span></li>
            <li><AppIcon name="solar:ssd-square-bold" :size="16" /><span><strong>Disks</strong> — a system disk per provider, with its cloud folder + files.</span></li>
            <li><AppIcon name="solar:bot-bold" :size="16" /><span><strong>Agent</strong> <em class="sw-opt">optional</em> — chat + automation.</span></li>
            <li><AppIcon name="solar:monitor-bold" :size="16" /><span><strong>Appearance</strong> — theme + wallpaper, previewed live.</span></li>
          </ul>
          <div class="sw-actions">
            <button class="sw-btn primary" type="button" @click="go('vault')">Get started</button>
            <button class="sw-link" type="button" @click="skipAll">Skip all</button>
          </div>
        </section>

        <!-- ── 2 · vault folder (merged: one folder pick does it all) ── -->
        <section v-if="step === 'vault'" class="sw-section">
          <div class="sw-status" :class="disk.bound ? 'ok' : 'idle'">
            <AppIcon :name="disk.bound ? 'solar:check-circle-bold' : 'solar:folder-bold'" :size="16" />
            <span>{{ disk.bound ? `${disk.name} · ${humanBytes(disk.savedBytes)}` : 'No vault folder yet — session only' }}</span>
          </div>
          <div v-if="store.syncConfigs.length" class="sw-status ok">
            <AppIcon name="solar:check-circle-bold" :size="16" />
            <span>{{ store.syncConfigs.length }} folder{{ store.syncConfigs.length === 1 ? '' : 's' }} connected</span>
          </div>

          <button class="sw-picker" type="button" :disabled="disk.busy || oneFolderBusy" @click="setupVaultFolder">
            <AppIcon name="solar:folder-bold" :size="20" />
            <span>
              <strong>{{ folderLabel || 'Select folder…' }}</strong>
              <small>{{ oneFolderBusy ? 'Setting up…' : 'Auto-opens the .cybermanju inside, or creates it' }}</small>
            </span>
          </button>

          <div class="sw-row">
            <label class="sw-field grow">
              <span class="sw-label">Vault file name</span>
              <input v-model="vaultFileName" class="sw-input" placeholder="vault.cybermanju" autocomplete="off" />
            </label>
            <label class="sw-field" style="max-width: 130px">
              <span class="sw-label">Size (MB)</span>
              <input v-model.number="vaultSizeMb" class="sw-input" type="number" min="64" max="8192" step="64" />
            </label>
          </div>
          <p class="sw-hint">Pick one folder: if it already holds a <code>.cybermanju</code> file it opens; otherwise a new one is created with the name + size above, plus a <code>files/</code> subfolder for local copies (never synced itself). The size becomes the default for the Disks step.</p>
          <p v-if="oneFolderMsg" class="sw-note" :class="oneFolderOk === false ? 'err' : oneFolderOk ? 'ok' : ''">{{ oneFolderMsg }}
            <button v-if="oneFolderNoPicker" class="sw-link" type="button" @click="openInstall">Install options</button>
          </p>
          <p v-if="wizardDirNote && !isTauri()" class="sw-note" :class="wizardDirOk === false ? 'err' : wizardDirOk ? 'ok' : ''">
            {{ wizardDirNote }}
            <button v-if="wizardDirReallow" class="sw-link" type="button" @click="reallowWizardDir">
              {{ wizardDirBusy ? 'Allowing…' : 'Re-allow access' }}
            </button>
          </p>
          <p v-if="disk.lastError" class="sw-note err">{{ disk.lastError }}</p>
          <p v-else-if="disk.lastMessage" class="sw-note ok">{{ disk.lastMessage }}</p>
          <p v-if="insecureContext && !isTauri()" class="sw-note">
            Plain-HTTP page (Docker over LAN): the browser hides its file pickers here, so the
            vault lives on the server — create disks and files normally, they persist in /data
            with no binding step. Serve over HTTPS (or open Pages) for local file binding.
          </p>

          <details v-if="showVaultFallback" class="sw-details">
            <summary>No picker? Use download / upload</summary>
            <div class="sw-row">
              <button class="sw-btn" type="button" :disabled="disk.busy" @click="exportVault">Export</button>
              <button class="sw-btn" type="button" :disabled="disk.busy" @click="triggerImport">Import</button>
              <input ref="importInput" type="file" accept=".cybermanju,application/octet-stream" class="sw-hidden-file" aria-label="Import .cybermanju file" @change="onImportPicked" />
            </div>
          </details>

          <details class="sw-details">
            <summary>Advanced — pick a .cybermanju file directly</summary>
            <div class="sw-bigrow">
              <button class="sw-choice" type="button" :disabled="disk.busy" @click="createVault">
                <AppIcon name="solar:add-circle-bold" :size="20" />
                <span><strong>New vault file</strong><small>File picker + name</small></span>
              </button>
              <button class="sw-choice" type="button" :disabled="disk.busy" @click="openVault">
                <AppIcon name="solar:folder-open-bold" :size="20" />
                <span><strong>Open existing file</strong><small>Pick a .cybermanju file</small></span>
              </button>
            </div>
          </details>

          <div class="sw-actions">
            <button class="sw-btn" type="button" @click="go('welcome')">Back</button>
            <button class="sw-btn primary" type="button" @click="go('cloud')">Next</button>
            <button class="sw-link" type="button" @click="go('cloud')">Skip</button>
          </div>
        </section>

        <!-- ── 3 · accounts (inline OAuth, no detour needed) ── -->
        <section v-if="step === 'cloud'" class="sw-section">
          <div v-if="oauthIdentity" class="sw-status ok">
            <AppIcon name="solar:check-circle-bold" :size="16" />
            <span>{{ oauthIdentity.name || oauthIdentity.email }} · {{ oauthIdentity.provider }}</span>
          </div>
          <div class="sw-oauth-grid">
            <button
              v-for="p in LOGIN_CARDS"
              :key="p.id"
              class="sw-oauth"
              type="button"
              :disabled="signInBusy === p.id || !sbConfigured"
              :title="sbConfigured ? `Continue with ${p.label}` : 'OAuth broker not set — add it in Settings → OAuth broker'"
              @click="signIn(p.id)"
            >
              <ProviderLogo :provider="p.logo" :size="30" />
              <span class="sw-oauth-meta">
                <strong>{{ signInBusy === p.id ? 'Opening…' : `Continue with ${p.label}` }}</strong>
                <small>{{ p.sub }}</small>
              </span>
            </button>
          </div>
          <p v-if="signInMsg" class="sw-note" :class="signInOk === false ? 'err' : signInOk ? 'ok' : ''">{{ signInMsg }}</p>
          <p v-if="ensureMsg" class="sw-note" :class="ensureOk === false ? 'err' : ensureOk ? 'ok' : ''">{{ ensureMsg }}</p>

          <!-- substep · after sign-in OK: folder + .cybermanju size per logged provider -->
          <div v-if="signInOk || oauthIdentity" class="sw-substep">
            <h4 class="sw-substep-title">Vault home per provider</h4>
            <p class="sw-hint">Signed in — pick where each provider keeps its <code>.cybermanju</code> files and how big the disk is. Drive gets a folder, GitHub/GitLab a private repo (created when missing).</p>
            <div v-for="sp in signedProviders" :key="sp.backend" class="sw-prov-card">
              <div class="sw-prov-head">
                <ProviderLogo :provider="sp.logo" :size="26" />
                <div class="sw-prov-meta">
                  <strong>{{ sp.label }}</strong>
                  <small>{{ sp.accountName }}</small>
                </div>
                <span v-if="cloudDone[sp.backend]" class="sw-note ok" style="margin:0">ready ✓</span>
              </div>
              <div v-if="sp.config" class="sw-row">
                <label class="sw-field grow">
                  <span class="sw-label">Folder</span>
                  <input v-model="cloudFolders[sp.backend]" class="sw-input" placeholder="cybermanju-vault" autocomplete="off" />
                </label>
                <label class="sw-field" style="max-width: 110px">
                  <span class="sw-label">Size (MB)</span>
                  <input v-model.number="cloudSizes[sp.backend]" class="sw-input" type="number" min="64" max="8192" step="64" placeholder="512" />
                </label>
              </div>
              <div v-if="sp.config" class="sw-row">
                <label class="sw-field grow">
                  <span class="sw-label">Disk passphrase (required — encrypts the disk)</span>
                  <input v-model="cloudPassphrases[sp.backend]" class="sw-input" type="password" placeholder="required" autocomplete="new-password" />
                </label>
              </div>
              <div v-if="sp.config" class="sw-row">
                <button
                  class="sw-btn primary"
                  type="button"
                  :disabled="!!cloudBusy[sp.backend] || cloudDone[sp.backend]"
                  @click="provisionCloudDisk(sp.backend)"
                >{{ cloudBusy[sp.backend] ? 'Creating…' : cloudDone[sp.backend] ? 'Created ✓' : `Create in ${sp.label}` }}</button>
              </div>
              <p v-else class="sw-note">No {{ sp.label }} connection yet — sign in again above to create it.</p>
              <p v-if="cloudMsgs[sp.backend]" class="sw-note" :class="cloudMsgs[sp.backend].ok ? 'ok' : 'err'">{{ cloudMsgs[sp.backend].text }}</p>
            </div>
          </div>
          <p v-if="!sbConfigured" class="sw-note err">OAuth broker is not set — sign-in stays off until you add the URL + key in Settings → OAuth broker.</p>
          <p class="sw-hint">Each sign-in also creates its provider connection below, so Disks can use it right away. Advanced switching stays in Accounts.</p>
          <div class="sw-row">
            <button class="sw-btn" type="button" @click="openAccounts">Open Account Manager</button>
          </div>

          <div class="sw-actions">
            <button class="sw-btn" type="button" @click="go('vault')">Back</button>
            <button class="sw-btn primary" type="button" @click="go('disks')">Next</button>
            <button class="sw-link" type="button" @click="go('disks')">Skip</button>
          </div>
        </section>

        <!-- ── 4 · disks (substep: one system disk per provider) ── -->
        <section v-if="step === 'disks'" class="sw-section">
          <div v-if="store.disks.length" class="sw-status ok">
            <AppIcon name="solar:check-circle-bold" :size="16" />
            <span>{{ store.disks.length }} disk{{ store.disks.length === 1 ? '' : 's' }} attached</span>
          </div>
          <p v-else class="sw-hint">No system disk yet — pick a provider below. Drive gets a <code>cybermanju-disks/&lt;disk&gt;</code> folder with its <code>.cybermanju</code> files; GitHub/GitLab get a private repo (created when missing) with the same seed.</p>

          <label class="sw-field">
            <span class="sw-label">Provider</span>
            <select v-model="diskConfigId" class="sw-input">
              <option value="">Pick a connected provider…</option>
              <option v-for="c in store.syncConfigs" :key="c.id" :value="c.id">{{ c.name || c.backendType }} ({{ c.backendType }})</option>
            </select>
          </label>

          <div class="sw-row">
            <label class="sw-field">
              <span class="sw-label">Size (MB)</span>
              <input v-model.number="diskSizeMb" class="sw-input" type="number" min="64" max="8192" step="64" />
            </label>
            <label class="sw-field grow">
              <span class="sw-label">Passphrase (encrypts the disk)</span>
              <input v-model="diskPassphrase" class="sw-input" type="password" placeholder="required — encrypts the disk" autocomplete="new-password" />
            </label>
          </div>

          <div class="sw-row">
            <button class="sw-btn primary" type="button" :disabled="diskBusy || !diskConfigId" @click="createDiskStep">
              {{ diskBusy ? 'Creating…' : 'Create & attach disk' }}
            </button>
          </div>
          <p v-if="diskMsg" class="sw-note" :class="diskOk === false ? 'err' : diskOk ? 'ok' : ''">{{ diskMsg }}</p>

          <details class="sw-details" :open="store.syncConfigs.length > 1">
            <summary>Unified disk — single copy home, mirrors opt-in, key holder</summary>
            <p class="sw-hint">Files live on ONE home provider by default. Tick mirror only for providers that must hold a duplicate. Pick which provider's <code>.cybermanju</code> unwraps the other disks.</p>
            <div v-for="c in store.syncConfigs" :key="`place-${c.id}`" class="sw-row">
              <strong>{{ c.name || c.backendType }}</strong>
              <label class="sw-check"><input type="checkbox" :checked="!!c.mirror" @change="setMirror(c, ($event.target as HTMLInputElement).checked)" /> mirror</label>
              <label class="sw-check"><input type="radio" name="sw-keyholder" :checked="!!c.keyHolder" @change="setKeyHolder(c.id)" /> key holder</label>
            </div>
            <p v-if="placementMsg" class="sw-note ok">{{ placementMsg }}</p>
          </details>

          <div class="sw-actions">
            <button class="sw-btn" type="button" @click="go('cloud')">Back</button>
            <button class="sw-btn primary" type="button" @click="go('agent')">Next</button>
            <button class="sw-link" type="button" @click="go('agent')">Skip</button>
          </div>
        </section>

        <!-- ── 5 · agent (optional) ── -->
        <section v-if="step === 'agent'" class="sw-section">
          <div v-if="isStatic" class="sw-banner info">
            <AppIcon name="solar:info-circle-bold" :size="15" />
            <span>Browser build — saved locally in this browser, key held in memory only (re-enter after reload).</span>
          </div>
          <template v-if="canAgent">
            <div v-if="agentProviderList.length" class="sw-presets" role="radiogroup" aria-label="Model provider">
              <button
                v-for="p in agentProviderList"
                :key="p.id"
                type="button"
                role="radio"
                :aria-checked="providerId === p.id"
                class="sw-preset"
                :class="{ on: providerId === p.id }"
                :title="`${p.baseUrl} · ${p.defaultModel}`"
                @click="pickPreset(p.id)"
              >
                <strong>{{ p.label }}</strong>
                <span class="muted">{{ p.defaultModel }}</span>
                <span v-if="p.keyless" class="sw-free">No key needed</span>
              </button>
            </div>
            <div class="sw-fields">
              <label class="sw-field">
                <span class="sw-label">Name</span>
                <input v-model="agentName" class="sw-input" placeholder="My assistant" autocomplete="off" />
              </label>
              <label class="sw-field grow">
                <span class="sw-label">Model</span>
                <input v-model="model" class="sw-input" :placeholder="presetDefault" autocomplete="off" />
              </label>
              <label v-if="!presetKeyless" class="sw-field grow">
                <span class="sw-label">API key</span>
                <input v-model="agentKey" class="sw-input" type="password" placeholder="Paste key (sealed, never shown back)" autocomplete="off" />
              </label>
            </div>
            <div class="sw-row">
              <button class="sw-btn primary" type="button" :disabled="agentBusy || !canSaveAgent" @click="saveAssistant">{{ agentBusy ? 'Saving…' : savedAgentId ? 'Save again' : 'Save assistant' }}</button>
              <span v-if="savedAgentName" class="sw-note ok" role="status">“{{ savedAgentName }}” ready{{ agentKeySealed ? ', key sealed' : '' }}.</span>
            </div>
            <p v-if="agentMsg" class="sw-note" :class="agentOk === false ? 'err' : ''">{{ agentMsg }}</p>
          </template>
          <div class="sw-actions">
            <button class="sw-btn" type="button" @click="go('disks')">Back</button>
            <button class="sw-btn primary" type="button" @click="go('appearance')">Next</button>
            <button class="sw-link" type="button" @click="go('appearance')">Skip</button>
          </div>
        </section>

        <!-- ── 6 · appearance (theme + wallpaper, live preview) ── -->
        <section v-if="step === 'appearance'" class="sw-section">
          <p class="sw-hint">Pick a look — it applies instantly, and Settings → Appearance can change it later.</p>
          <span class="sw-label">Theme</span>
          <div class="sw-presets" role="radiogroup" aria-label="Theme">
            <button
              v-for="t in appearanceChoices"
              :key="t.id"
              type="button"
              role="radio"
              :aria-checked="effectiveAppearance === t.id"
              class="sw-preset"
              :class="{ on: effectiveAppearance === t.id }"
              :title="t.blurb"
              @click="pickAppearance(t.id)"
            >
              <span class="sw-theme-swatch" :style="t.swatch" aria-hidden="true" />
              <strong>{{ t.label }}</strong>
              <span class="muted">{{ t.blurb }}</span>
            </button>
          </div>
          <span class="sw-label">Wallpaper</span>
          <div class="sw-presets" role="radiogroup" aria-label="Wallpaper">
            <button
              v-for="w in wallpaperChoices"
              :key="w.id"
              type="button"
              role="radio"
              :aria-checked="activeWallpaper === w.id"
              class="sw-preset"
              :class="{ on: activeWallpaper === w.id }"
              :title="w.label"
              @click="pickWallpaper(w.id)"
            >
              <span class="sw-wp-swatch" :class="`sw-wp-${w.id}`" aria-hidden="true" />
              <strong>{{ w.label }}</strong>
            </button>
          </div>
          <p v-if="customWallpaper.kind.value" class="sw-note">Custom image active — picking a preset above switches back to it.</p>
          <p v-else class="sw-hint">A custom image (URL or file) stays in Settings → Appearance.</p>
          <div class="sw-actions">
            <button class="sw-btn" type="button" @click="go('agent')">Back</button>
            <button class="sw-btn primary" type="button" @click="go('done')">Next</button>
            <button class="sw-link" type="button" @click="go('done')">Skip</button>
          </div>
        </section>

        <!-- ── 7 · done ── -->
        <section v-if="step === 'done'" class="sw-section">
          <ul class="sw-summary">
            <li>
              <AppIcon :name="disk.bound ? 'solar:check-circle-bold' : 'solar:minus-circle-bold'" :size="15" />
              <span>Vault: <strong>{{ disk.bound ? disk.name : 'skipped' }}</strong></span>
            </li>
            <li>
              <AppIcon :name="store.syncConfigs.length ? 'solar:check-circle-bold' : 'solar:minus-circle-bold'" :size="15" />
              <span>Sync: <strong>{{ store.syncConfigs.length ? `${store.syncConfigs.length} connected` : 'skipped' }}</strong></span>
            </li>
            <li>
              <AppIcon :name="oauthIdentity ? 'solar:check-circle-bold' : 'solar:minus-circle-bold'" :size="15" />
              <span>Cloud: <strong>{{ oauthIdentity ? `${oauthIdentity.provider} signed in` : 'skipped' }}</strong></span>
            </li>
            <li>
              <AppIcon :name="store.disks.length ? 'solar:check-circle-bold' : 'solar:minus-circle-bold'" :size="15" />
              <span>Disks: <strong>{{ store.disks.length ? `${store.disks.length} attached` : 'skipped' }}</strong></span>
            </li>
            <li>
              <AppIcon :name="savedAgentName ? 'solar:check-circle-bold' : 'solar:minus-circle-bold'" :size="15" />
              <span>Agent: <strong>{{ savedAgentName ? `“${savedAgentName}”` : 'skipped' }}</strong></span>
            </li>
            <li>
              <AppIcon name="solar:check-circle-bold" :size="15" />
              <span>Appearance: <strong>{{ appearanceSummary }}</strong></span>
            </li>
          </ul>
          <div class="sw-actions">
            <button class="sw-btn" type="button" @click="go('appearance')">Back</button>
            <button class="sw-btn primary" type="button" @click="finish">Finish</button>
          </div>
        </section>
      </main>
    </div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import ProviderLogo from '@/components/ProviderLogo.vue'
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { isStaticHost, isTauri } from '@/composables/useTauri'
import {
  connectedAccounts,
  identity as supabaseIdentity,
  refreshIdentity,
  signInWithPopup,
  supabaseConfigured,
  supabaseSession,
  supabaseSessionProvider,
  type OAuthBackend,
} from '@/composables/useSupabase'
import {
  createCyberManjuFile,
  createCyberManjuFileInDirectory,
  disk,
  exportCyberManjuFile,
  importCyberManjuFile,
  openCyberManjuFile,
} from '@/composables/useCyberManjuFile'
import {
  SYNC_SUBDIR,
  VAULT_FILENAME,
  oneFolderLayout,
} from '@/utils/vaultFolder'
import { backendLabel, syncConfigDefaults } from '@/utils/providers'
import { useTheme } from '@/composables/useTheme'
import { useCustomWallpaper } from '@/composables/useCustomWallpaper'
import { THEMES, WALLPAPERS, type ThemeId } from '@/ui/tokens'
import { agentPermissionPreset, defaultMcpServers } from '@/types'
import type { AgentConfig, ProviderPreset, SyncBackendType, SyncConfig } from '@/types'
import { humanBytes } from '@/utils/format'
import { requestInstallWindow } from '@/utils/installLinks'
import {
  SETUP_STEPS,
  SETUP_STEP_LABELS,
  clampVaultSizeMb,
  clearSetupResumeStep,
  markSetupSeen,
  saveSetupResumeStep,
  setupStepIndex,
  signedProviderCards,
  takeSetupResumeStep,
  type SetupStep,
} from '@/utils/setupWizard'

const emit = defineEmits<{ close: [] }>()

const store = useAppStore()
const cardRef = ref<HTMLElement | null>(null)

const step = ref<SetupStep>(takeSetupResumeStep() ?? 'welcome')

/** The browser (WASM/Pages) build runs the agent locally — same setup form,
// configs in localStorage, provider catalog from the wasm bundle, keys in
// memory only. Nothing here needs the desktop app or Docker server. */
const canAgent = true
const isStatic = isStaticHost()
const wasmPresets = ref<ProviderPreset[]>([])
const agentProviderList = computed(() =>
  isStatic ? wasmPresets.value : store.agentProviders,
)

/** Export/Import fallback only matters where the OS picker may be missing. */
const showVaultFallback = computed(() => isStatic || !disk.supported)

/**
 * Plain-HTTP LAN (e.g. a Docker dashboard at `http://nas:3456`) is not a
 * secure context, so Chromium hides the file pickers entirely. The vault
 * then lives on the server and needs no browser binding — say so instead
 * of showing a dead picker.
 */
const insecureContext = computed(() => {
  try {
    return typeof window !== 'undefined' && window.isSecureContext === false
  } catch {
    return false
  }
})

const oauthIdentity = computed(() => supabaseIdentity.value)

function openAccounts() {
  saveSetupResumeStep('disks')
  emit('close')
  window.dispatchEvent(new CustomEvent('cybermanju:open-accounts', { detail: { resumeSetup: true } }))
}

// ── `.cybermanju` import/export fallback (no File System Access API).
const importInput = ref<HTMLInputElement | null>(null)

async function exportVault() {
  await exportCyberManjuFile()
}

function triggerImport() {
  importInput.value?.click()
}

async function onImportPicked(e: Event) {
  const input = e.target as HTMLInputElement | null
  const file = input?.files?.[0]
  // Reset so picking the same file twice still fires `change`.
  if (input) input.value = ''
  if (!file) return
  await importCyberManjuFile(file)
}

// ── vault folder step (merged: one folder → vault file + files/ + local sync) ──
async function createVault() {
  await createCyberManjuFile()
}

async function openVault() {
  await openCyberManjuFile()
}

const oneFolderBusy = ref(false)
const oneFolderMsg = ref('')
const oneFolderOk = ref<boolean | null>(null)
/** The one-folder message is a missing-picker refusal — offer the installer. */
const oneFolderNoPicker = computed(() => /no folder picker/i.test(oneFolderMsg.value || ''))
/** Open the Install window (native builds per platform, App × CLI). */
function openInstall() {
  requestInstallWindow('host-unsupported')
}
const vaultFileName = ref(VAULT_FILENAME)
const vaultSizeMb = ref(512)
const folderLabel = ref('')
const localPath = ref('')

function sanitizedVaultName(): string {
  const raw = vaultFileName.value.trim().replace(/[\\/]/g, '').slice(0, 64) || VAULT_FILENAME
  return raw.toLowerCase().endsWith('.cybermanju') ? raw : `${raw}.cybermanju`
}

/** Best-effort local sync row for the picked folder (never fails the vault step). */
async function ensureVaultLocalSync(syncLabel: string, displayName: string): Promise<void> {
  try {
    const existing = store.syncConfigs.find(
      c => c.backendType === 'local' && (c.basePath === syncLabel || c.name === displayName),
    )
    if (existing) return
    const saved = await store.saveSyncConfig({
      ...syncConfigDefaults(),
      id: '',
      backendType: 'local',
      name: displayName,
      basePath: syncLabel || undefined,
    } as SyncConfig)
    if (!saved) {
      oneFolderMsg.value += ' (local sync row could not save — retry from Accounts.)'
      oneFolderOk.value = false
      return
    }
    await store.fetchSyncConfigs().catch(() => {})
    await store.probeSyncConnection(saved).catch(() => null)
  } catch {
    // Sync save is best-effort — the vault binding above already succeeded.
  }
}

async function setupVaultFolder() {
  if (oneFolderBusy.value || disk.busy) return
  oneFolderBusy.value = true
  oneFolderMsg.value = ''
  oneFolderOk.value = null
  try {
    // 1. Tauri desktop: native dir pick → vault path + files/ sync root.
    // The server DB stays authoritative; the vault file is the portable copy.
    if (isTauri()) {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const picked = await open({ directory: true, multiple: false })
      const path = typeof picked === 'string' ? picked : null
      if (!path) return
      const clean = path.replace(/\/+$/g, '')
      const leaf = clean.split('/').filter(Boolean).pop() ?? clean
      const want = sanitizedVaultName()
      vaultFileName.value = want
      folderLabel.value = leaf
      localPath.value = `${clean}/${SYNC_SUBDIR}`
      diskSizeMb.value = Math.min(8192, Math.max(64, Math.round(vaultSizeMb.value) || 512))
      await ensureVaultLocalSync(localPath.value, 'Local folder')
      oneFolderMsg.value = `Folder “${leaf}” picked — vault at ${clean}/${want} (${diskSizeMb.value} MB default for Disks), copies in ${localPath.value}, connected below.`
      oneFolderOk.value = true
      return
    }
    // 2. Chromium: one directory pick auto-detects or creates the vault —
    // no second picker, no typed path.
    const w = window as unknown as {
      showDirectoryPicker?: () => Promise<FileSystemDirectoryHandle>
    }
    if (typeof w.showDirectoryPicker !== 'function') {
      oneFolderMsg.value = 'No folder picker in this browser — use the file fallback below instead.'
      oneFolderOk.value = false
      return
    }
    const dir = await w.showDirectoryPicker()
    const dirName = String((dir as { name?: string }).name ?? 'vault')
    folderLabel.value = dirName
    const layout = oneFolderLayout(dirName)
    // Auto-detect: an existing `*.cybermanju` in the folder opens, otherwise
    // the custom name is created fresh.
    let target = sanitizedVaultName()
    let detected = ''
    try {
      const values = (dir as FileSystemDirectoryHandle & { values?: () => AsyncIterableIterator<FileSystemHandle> }).values?.()
      if (values) {
        for await (const entry of values) {
          const n = String((entry as { name?: string }).name ?? '')
          if (n.toLowerCase().endsWith('.cybermanju')) {
            detected = n
            break
          }
        }
      }
    } catch {
      // Listing is best-effort — fall through to create the custom name.
    }
    if (detected) {
      target = detected
      vaultFileName.value = detected
    } else {
      vaultFileName.value = target
    }
    const made = await createCyberManjuFileInDirectory(dir, target)
    if (!made) return
    // `files/` subdir beside the vault (created, handle remembered).
    let syncLabel = layout.syncVirtualPath
    try {
      const sub = await dir.getDirectoryHandle(layout.syncSubdir, { create: true })
      syncLabel = `/${made.dirName || layout.folderName}/${layout.syncSubdir}`
      try {
        const { rememberLocalDir, WIZARD_LOCAL_DIR_KEY } = await import('@/utils/localDir')
        await rememberLocalDir(WIZARD_LOCAL_DIR_KEY, sub)
      } catch {
        // Handle persistence is best-effort; the virtual path is what sync uses.
      }
    } catch {
      // Subdir creation is best-effort — the virtual path still guides setup.
    }
    localPath.value = syncLabel
    diskSizeMb.value = Math.min(8192, Math.max(64, Math.round(vaultSizeMb.value) || 512))
    await ensureVaultLocalSync(syncLabel, 'Local folder')
    oneFolderMsg.value = detected
      ? `Found ${detected} in “${made.dirName || dirName}” — opened, sync root ${syncLabel} connected.`
      : `Created ${target} in “${made.dirName || dirName}” (${diskSizeMb.value} MB default for Disks) + sync root ${syncLabel} — vault itself is never synced.`
    oneFolderOk.value = true
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') return
    oneFolderMsg.value = e instanceof Error ? e.message : String(e)
    oneFolderOk.value = false
  } finally {
    oneFolderBusy.value = false
  }
}

// ── remembered folder status (browser leg): a reloaded page keeps the
// handle but loses the grant — one click re-allows, never a re-pick.
const wizardDirNote = ref('')
const wizardDirOk = ref<boolean | null>(null)
const wizardDirReallow = ref(false)
const wizardDirBusy = ref(false)

async function refreshWizardDir() {
  wizardDirReallow.value = false
  if (isTauri()) return
  try {
    const { localDirs, ensureLocalDir, WIZARD_LOCAL_DIR_KEY } = await import('@/utils/localDir')
    const st = localDirs[WIZARD_LOCAL_DIR_KEY] ?? (await ensureLocalDir(WIZARD_LOCAL_DIR_KEY))
    if (st.status === 'ready') {
      wizardDirNote.value = `“${st.label}” still attached — files read and write without re-picking.`
      wizardDirOk.value = true
    } else if (st.status === 'needs-permission') {
      wizardDirNote.value = `“${st.label}” is remembered but the browser wants one click to reopen it.`
      wizardDirOk.value = null
      wizardDirReallow.value = true
    } else {
      wizardDirNote.value = ''
      wizardDirOk.value = null
    }
  } catch {
    wizardDirNote.value = ''
    wizardDirOk.value = null
  }
}

async function reallowWizardDir() {
  if (wizardDirBusy.value) return
  wizardDirBusy.value = true
  try {
    const { reallowLocalDir, WIZARD_LOCAL_DIR_KEY } = await import('@/utils/localDir')
    const ok = await reallowLocalDir(WIZARD_LOCAL_DIR_KEY)
    if (ok) {
      wizardDirNote.value = 'Access restored — the folder works without re-picking.'
      wizardDirOk.value = true
      wizardDirReallow.value = false
    } else {
      wizardDirNote.value = 'Still closed — allow access or pick the folder again.'
      wizardDirOk.value = false
    }
  } finally {
    wizardDirBusy.value = false
  }
}

// ── accounts step: inline OAuth (same broker as Account Manager) ──
const LOGIN_CARDS: Array<{ id: OAuthBackend; label: string; logo: string; sub: string }> = [
  { id: 'google', label: 'Google', logo: 'google', sub: 'Drive OAuth' },
  { id: 'github', label: 'GitHub', logo: 'github', sub: 'Repo Contents API' },
  { id: 'gitlab', label: 'GitLab', logo: 'gitlab', sub: 'Projects API v4' },
]
const sbConfigured = computed(() => supabaseConfigured())
const signInBusy = ref<OAuthBackend | null>(null)
const signInMsg = ref('')
const signInOk = ref<boolean | null>(null)
const ensureMsg = ref('')
const ensureOk = ref<boolean | null>(null)

function backendForOAuth(p: OAuthBackend | string): SyncBackendType {
  if (p === 'google') return 'googleDrive'
  if (p === 'gitlab') return 'gitlab'
  return 'github'
}

async function sessionTokenFor(backend: SyncBackendType): Promise<string> {
  try {
    const session = await supabaseSession()
    if (!session?.provider_token) return ''
    const prov = supabaseSessionProvider(session)
    if (!prov) return ''
    return backendForOAuth(prov) === backend ? (session.provider_token ?? '') : ''
  } catch {
    return ''
  }
}

async function ensureWizardProvider(backend: SyncBackendType, token: string, displayName: string): Promise<void> {
  const existing = store.syncConfigs.find(c => c.backendType === backend)
  if (existing) {
    if (token && !(existing as SyncConfig).token) {
      await store.saveSyncConfig({ ...existing, token }).catch(() => null)
      await store.fetchSyncConfigs().catch(() => {})
    }
    return
  }
  const saved = await store.saveSyncConfig({
    ...syncConfigDefaults(),
    id: '',
    backendType: backend,
    name: displayName,
    token: token.trim() || undefined,
  } as SyncConfig)
  if (saved) await store.fetchSyncConfigs().catch(() => {})
}

async function signIn(provider: OAuthBackend) {
  if (signInBusy.value) return
  signInBusy.value = provider
  signInMsg.value = ''
  signInOk.value = null
  ensureMsg.value = ''
  ensureOk.value = null
  try {
    const who = await signInWithPopup(provider)
    signInMsg.value = `Signed in as ${who.name} (${who.provider}).`
    signInOk.value = true
    // Same as Account Manager: a fresh login provisions its provider row so
    // the Disks step can use it immediately.
    try {
      const backend = backendForOAuth(provider)
      const token = await sessionTokenFor(backend)
      await ensureWizardProvider(backend, token, `${backendLabel(backend)} — ${who.name}`)
      await store.fetchSyncConfigs().catch(() => {})
      ensureMsg.value = token
        ? `${backendLabel(backend)} connection ready.`
        : `${backendLabel(backend)} connection created — finish OAuth in Accounts if it asks.`
      ensureOk.value = true
    } catch (e) {
      ensureMsg.value = e instanceof Error ? e.message : String(e)
      ensureOk.value = false
    }
  } catch (e) {
    signInMsg.value = e instanceof Error ? e.message : String(e)
    signInOk.value = false
  } finally {
    signInBusy.value = null
  }
}

// ── OAuth substep: after 200-OK sign-in, one folder + `.cybermanju`
// size card per logged provider (Drive folder, GitHub/GitLab private
// repo created when missing). The folder seeds the disk name, so the
// remote dir / repo visibly matches what the user picked.
const cloudFolders = ref<Record<string, string>>({})
const cloudSizes = ref<Record<string, number>>({})
const cloudPassphrases = ref<Record<string, string>>({})
const cloudBusy = ref<Record<string, boolean>>({})
const cloudDone = ref<Record<string, boolean>>({})
const cloudMsgs = ref<Record<string, { ok: boolean; text: string }>>({})

const signedProviders = computed(() => {
  const accounts = connectedAccounts.value.map(a => ({
    provider: a.provider,
    name: a.name,
    email: a.email,
  }))
  if (oauthIdentity.value) {
    accounts.unshift({
      provider: oauthIdentity.value.provider,
      name: oauthIdentity.value.name,
      email: oauthIdentity.value.email,
    })
  }
  return signedProviderCards(
    accounts,
    store.syncConfigs.map(c => ({ backendType: c.backendType })),
  ).map(card => ({
    ...card,
    config: store.syncConfigs.find(c => c.backendType === card.backend) ?? null,
  }))
})

async function provisionCloudDisk(backend: SyncBackendType) {
  if (cloudBusy.value[backend] || cloudDone.value[backend]) return
  const cfg = store.syncConfigs.find(c => c.backendType === backend) ?? null
  if (!cfg) {
    cloudMsgs.value[backend] = { ok: false, text: 'No connection yet — sign in again above to create it.' }
    return
  }
  cloudBusy.value[backend] = true
  delete cloudMsgs.value[backend]
  try {
    const folder = (cloudFolders.value[backend] ?? '').trim() || 'cybermanju-vault'
    const sizeMb = clampVaultSizeMb(cloudSizes.value[backend] ?? 512)
    const passphrase = (cloudPassphrases.value[backend] ?? '')
    if (!passphrase) {
      cloudMsgs.value[backend] = { ok: false, text: 'Type a disk passphrase first — the disk is passphrase-encrypted on every device.' }
      return
    }
    const out = await store.createDiskWithRemote(cfg, {
      sizeMb,
      passphrase,
      diskName: folder,
      token: await diskTokenFor(cfg),
    })
    if (!out?.disk) {
      cloudMsgs.value[backend] = { ok: false, text: 'Could not create the disk — retry.' }
      return
    }
    cloudDone.value[backend] = true
    if (out.remote) {
      const where = backend === 'googleDrive'
        ? `Drive folder \`${out.remote.remoteDir}\``
        : `private repo \`${out.remote.config.repoName}\``
      cloudMsgs.value[backend] = { ok: true, text: `“${folder}” live (${sizeMb} MB) — ${where} holds its .cybermanju files.` }
    } else if (out.remoteWarning) {
      cloudMsgs.value[backend] = { ok: false, text: `Disk attached, but the remote seed failed: ${out.remoteWarning}` }
    } else {
      cloudMsgs.value[backend] = { ok: true, text: `“${folder}” created (${sizeMb} MB).` }
    }
  } catch (e) {
    cloudMsgs.value[backend] = { ok: false, text: e instanceof Error ? e.message : String(e) }
  } finally {
    cloudBusy.value[backend] = false
  }
}

// ── disks step: one system disk per provider (explicit substep) ──
const diskConfigId = ref('')
const diskSizeMb = ref(512)
const diskPassphrase = ref('')
const diskBusy = ref(false)
const diskMsg = ref('')
const diskOk = ref<boolean | null>(null)

/** Saved secret first, live OAuth session second (same precedence as Accounts). */
async function diskTokenFor(cfg: SyncConfig): Promise<string> {
  if (typeof cfg.token === 'string' && cfg.token.trim()) return cfg.token.trim()
  try {
    const session = await supabaseSession()
    const prov = session ? supabaseSessionProvider(session) : null
    const { supabaseProviderFor } = await import('@/composables/useSupabase')
    if (prov && supabaseProviderFor(cfg.backendType) === prov) {
      return session?.provider_token ?? ''
    }
  } catch {
    // Session unreadable — the disk still gets created, remote warns.
  }
  return ''
}

async function createDiskStep() {
  const cfg = store.syncConfigs.find(c => c.id === diskConfigId.value)
  if (!cfg || diskBusy.value) {
    if (!cfg) {
      diskMsg.value = 'Pick a connected provider first (or add one in Accounts).'
      diskOk.value = false
    }
    return
  }
  diskBusy.value = true
  diskMsg.value = ''
  diskOk.value = null
  try {
    if (!diskPassphrase.value) {
      diskMsg.value = 'Type a disk passphrase first — the disk is passphrase-encrypted on every device.'
      diskOk.value = false
      return
    }
    const out = await store.createDiskWithRemote(cfg, {
      sizeMb: Math.min(8192, Math.max(64, Math.round(diskSizeMb.value) || 512)),
      passphrase: diskPassphrase.value,
      diskName: cfg.name || cfg.backendType,
      token: await diskTokenFor(cfg),
    })
    if (!out?.disk) {
      diskMsg.value = 'Could not create the disk — retry.'
      diskOk.value = false
      return
    }
    diskPassphrase.value = ''
    if (out.remote) {
      const where = cfg.backendType === 'googleDrive'
        ? `Drive folder \`${out.remote.remoteDir}\``
        : `private repo \`${out.remote.config.repoName}\``
      diskMsg.value = `Disk attached — ${where} holds its .cybermanju files. Add another, or continue.`
      diskOk.value = true
    } else if (out.remoteWarning) {
      diskMsg.value = `Disk attached, but the remote seed failed: ${out.remoteWarning}`
      diskOk.value = false
    } else {
      diskMsg.value = 'Disk attached.'
      diskOk.value = true
    }
  } catch (e) {
    diskMsg.value = e instanceof Error ? e.message : String(e)
    diskOk.value = false
  } finally {
    diskBusy.value = false
  }
}

// ── unified-disk placement: single-copy home + mirror opt-in + key holder ──
const placementMsg = ref('')

async function setMirror(cfg: SyncConfig, on: boolean) {
  placementMsg.value = ''
  const saved = await store.saveSyncConfig({ ...cfg, mirror: on })
  if (saved) {
    await store.fetchSyncConfigs()
    placementMsg.value = on
      ? `“${cfg.name || cfg.backendType}” will hold duplicates (mirror).`
      : `“${cfg.name || cfg.backendType}” holds single-copy homes only.`
  }
}

async function setKeyHolder(id: string) {
  placementMsg.value = ''
  const { designateKeyHolder } = await import('@/utils/filePlacement')
  const next = designateKeyHolder(store.syncConfigs, id)
  for (const c of next) {
    const prev = store.syncConfigs.find((x) => x.id === c.id)
    if (!prev || !!prev.keyHolder === !!c.keyHolder) continue
    await store.saveSyncConfig({ ...c })
  }
  await store.fetchSyncConfigs()
  const holder = next.find((c) => c.id === id)
  placementMsg.value = `“${holder?.name || holder?.backendType}” unwraps the other disks.`
}

// ── agent step ──
const providerId = ref('openrouter')
const agentName = ref('')
const model = ref('')
const agentKey = ref('')
const agentBusy = ref(false)
const agentMsg = ref('')
const agentOk = ref<boolean | null>(null)
const savedAgentId = ref('')
const savedAgentName = ref('')
const agentKeySealed = ref(false)

const preset = computed(() => agentProviderList.value.find(p => p.id === providerId.value) ?? null)
const presetDefault = computed(() => preset.value?.defaultModel ?? 'model id')
const presetKeyless = computed(() => preset.value?.keyless ?? false)
const canSaveAgent = computed(() => providerId.value !== '' && (model.value.trim() !== '' || presetDefault.value !== 'model id'))

function pickPreset(id: string) {
  providerId.value = id
  const p = agentProviderList.value.find(x => x.id === id)
  if (p) model.value = p.defaultModel
}

function newWizardId(prefix: string): string {
  try {
    return `${prefix}-${crypto.randomUUID().slice(0, 8)}`
  } catch {
    return `${prefix}-${Date.now().toString(36)}`
  }
}

async function saveAssistant() {
  if (agentBusy.value || !canSaveAgent.value) return
  agentBusy.value = true
  agentMsg.value = ''
  agentOk.value = null
  try {
    // Browser build: same shapes as Agent → Setup, kept in localStorage;
    // the key is shared in-memory so Agent can chat immediately.
    if (isStatic) {
      const { saveLocalConfig, setLocalKey } = await import('@/composables/useAgent')
      const now = new Date().toISOString()
      const id = savedAgentId.value || newWizardId('cfg')
      const cfg: AgentConfig = {
        id,
        name: agentName.value.trim() || 'My assistant',
        providerId: providerId.value,
        model: model.value.trim() || presetDefault.value,
        workingDir: '',
        agentKind: 'build',
        permission: agentPermissionPreset('balanced'),
        autoApprove: false,
        maxTurns: 25,
        mcpServers: defaultMcpServers(),
        hasKey: false,
        createdAt: now,
        updatedAt: now,
      }
      saveLocalConfig(cfg)
      savedAgentId.value = id
      savedAgentName.value = cfg.name
      const key = agentKey.value.trim()
      if (key && !presetKeyless.value) {
        setLocalKey(id, key)
        agentKey.value = ''
        agentKeySealed.value = true
        agentMsg.value = 'Saved locally — key held in memory, ready to chat in Agent.'
        agentOk.value = true
      } else {
        agentMsg.value = presetKeyless.value
          ? 'Saved locally — ready to chat in Agent.'
          : 'Saved locally — paste the key in Agent → Setup (memory-only).'
        agentOk.value = true
      }
      return
    }
    const saved = await store.saveAgentConfig({
      name: agentName.value.trim() || 'My assistant',
      providerId: providerId.value,
      model: model.value.trim() || presetDefault.value,
      workingDir: '',
      agentKind: 'build',
      permission: agentPermissionPreset('balanced'),
      autoApprove: false,
      maxTurns: 25,
      mcpServers: defaultMcpServers(),
    })
    if (!saved) {
      agentMsg.value = 'Could not save — is the server running?'
      agentOk.value = false
      return
    }
    savedAgentId.value = saved.id
    savedAgentName.value = saved.name
    const key = agentKey.value.trim()
    if (key && !presetKeyless.value) {
      const sealed = await store.saveAgentKey(saved.id, key)
      agentKeySealed.value = sealed
      agentKey.value = ''
      agentMsg.value = sealed ? 'Saved — key sealed.' : 'Saved, but sealing failed — seal it later in Agent.'
      agentOk.value = sealed
    } else {
      agentMsg.value = presetKeyless.value ? 'Saved — ready to chat.' : 'Saved — add the key later in Agent.'
      agentOk.value = true
    }
  } finally {
    agentBusy.value = false
  }
}

// ── appearance step: theme + wallpaper (same store as Settings) ──
const theme = useTheme()
const customWallpaper = useCustomWallpaper()

const appearanceChoices = computed(() => [
  { id: 'auto', label: 'Auto', blurb: 'Follows the OS light / dark scheme', swatch: 'background: linear-gradient(90deg, #000000 50%, #ECECEC 50%)' },
  ...Object.values(THEMES).map(t => ({
    id: t.id as string,
    label: t.label,
    blurb: t.blurb,
    swatch: `background: linear-gradient(135deg, ${t.palette.bg} 55%, ${t.palette.accent} 55%)`,
  })),
])
const effectiveAppearance = computed(() => (theme.settings.followSystem ? 'auto' : theme.settings.theme))

function pickAppearance(id: string) {
  if (id === 'auto') theme.setAppearance('auto')
  else theme.setAppearance(id as ThemeId)
}

const wallpaperChoices = WALLPAPERS
const activeWallpaper = computed(() => (customWallpaper.kind.value ? 'custom' : theme.settings.wallpaper))
const appearanceSummary = computed(() => {
  const t = appearanceChoices.value.find(c => c.id === effectiveAppearance.value)
  const w = wallpaperChoices.find(wp => wp.id === theme.settings.wallpaper)
  const wpLabel = customWallpaper.kind.value ? 'Custom image' : (w?.label ?? theme.settings.wallpaper)
  return `${t?.label ?? effectiveAppearance.value} · ${wpLabel}`
})

function pickWallpaper(id: string) {
  theme.setWallpaper(id)
  if (customWallpaper.kind.value) void customWallpaper.clear()
}

// ── navigation ──
function go(s: SetupStep) {
  step.value = s
}

function skipAll() {
  clearSetupResumeStep()
  markSetupSeen()
  emit('close')
}

function finish() {
  clearSetupResumeStep()
  markSetupSeen()
  emit('close')
}

function close() {
  // Unmarked close: the wizard returns next launch.
  emit('close')
}

onMounted(async () => {
  cardRef.value?.focus()
  await nextTick()
  await refreshIdentity().catch(() => {})
  await Promise.allSettled([store.fetchSyncConfigs(), store.fetchDisks(), store.fetchAgentConfigs()])
  if (isStatic) {
    try {
      const { wasmAgentCatalog } = await import('@/composables/useWasmBackend')
      const presets = (await wasmAgentCatalog()) as ProviderPreset[]
      if (presets.length) wasmPresets.value = presets
    } catch {
      wasmPresets.value = []
    }
    // Prefer an existing local assistant so re-opening the wizard edits it.
    try {
      const { listLocalConfigs } = await import('@/composables/useAgent')
      const existing = listLocalConfigs()
      if (existing.length && !existing.some(c => c.id === savedAgentId.value)) {
        const first = existing[0]
        savedAgentId.value = first.id
        savedAgentName.value = first.name
        providerId.value = first.providerId
        model.value = first.model
        agentName.value = first.name
      }
    } catch {
      // localStorage unavailable — presets still render.
    }
  } else {
    await store.fetchAgentProviders().catch(() => {})
  }
  if (agentProviderList.value.length && !agentProviderList.value.some(p => p.id === providerId.value)) {
    pickPreset(agentProviderList.value[0].id)
  } else {
    const p = agentProviderList.value.find(x => x.id === providerId.value)
    if (p && !model.value) model.value = p.defaultModel
  }
  // Folder attach follows the step: entering vault re-checks the remembered
  // directory without prompting (lapsed grants show the one-click re-allow).
  watch(step, (s) => {
    if (s === 'vault') void refreshWizardDir()
  })
  void refreshWizardDir()
})
</script>

<style scoped>
.sw-overlay {
  position: fixed;
  inset: 0;
  z-index: 2000;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: 8vh 20px 20px;
  background: rgba(0, 0, 0, 0.35);
}
/* Sheet: slides down from the top edge. */
.sw-card {
  width: min(480px, 100%);
  max-height: min(86vh, 720px);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border-radius: var(--ui-radius-lg);
  border: 1px solid var(--ui-border);
  background: var(--ui-glass-2);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 13px;
  box-shadow: var(--ui-shadow-menu);
  outline: none;
  animation: ui-fade-in var(--ui-dur) ease-out both;
}
.sw-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 10px;
  padding: 14px 16px 10px;
  border-bottom: 1px solid var(--ui-border);
}
.sw-brand { display: flex; align-items: center; gap: 10px; min-width: 0; }
.sw-mark {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: var(--ui-radius-md);
  flex-shrink: 0;
  overflow: hidden;
  background: var(--ui-bg-deep);
  border: 1px solid var(--ui-border);
}
.sw-mark img {
  display: block;
  width: 36px;
  height: 36px;
  object-fit: cover;
  border-radius: var(--ui-radius-md);
}
.sw-title { margin: 0; font-size: 15px; letter-spacing: 0.4px; }
.sw-sub { margin: 1px 0 0; font-size: 11px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.sw-icon-btn { background: none; border: none; color: color-mix(in srgb, var(--ui-text) 55%, transparent); cursor: pointer; padding: 4px; border-radius: var(--ui-radius-sm); display: inline-flex; }
.sw-icon-btn:hover { color: var(--ui-text); }
.sw-icon-btn:focus-visible, .sw-btn:focus-visible, .sw-link:focus-visible, .sw-preset:focus-visible, .sw-choice:focus-visible, .sw-picker:focus-visible, .sw-oauth:focus-visible, .sw-dot:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--ui-accent) 75%, transparent);
  outline-offset: 2px;
}
.sw-dots { display: flex; gap: 6px; padding: 10px 16px 0; }
.sw-dot { height: 4px; flex: 1; border-radius: var(--ui-radius-full); border: none; padding: 0; cursor: pointer; background: color-mix(in srgb, var(--ui-text) 12%, transparent); }
.sw-dot.past { background: color-mix(in srgb, var(--ui-accent) 55%, transparent); }
.sw-dot.on { background: var(--ui-accent); }
.sw-body { overflow-y: auto; padding: 12px 16px 16px; }
.sw-section { display: flex; flex-direction: column; gap: 10px; }
.sw-lead { margin: 0; font-size: 13px; line-height: 1.55; }
.sw-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
.sw-list li {
  display: flex; gap: 10px; align-items: center;
  border: 1px solid var(--ui-border); border-radius: var(--ui-radius-md); padding: 10px 12px;
  font-size: 12.5px; line-height: 1.5;
}
.sw-list li > :first-child { color: var(--ui-accent); flex-shrink: 0; }
.sw-opt { color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.sw-status {
  display: flex; align-items: center; gap: 8px;
  border: 1px solid var(--ui-border); border-radius: var(--ui-radius-md); padding: 8px 12px;
  font-size: 12px;
}
.sw-status.ok { border-color: color-mix(in srgb, var(--ui-accent) 50%, transparent); }
.sw-status.ok > :first-child { color: var(--ui-accent); }
.sw-status.idle > :first-child { color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.sw-bigrow { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
@media (max-width: 440px) { .sw-bigrow { grid-template-columns: 1fr; } }
.sw-choice {
  display: flex; gap: 10px; align-items: center; text-align: left;
  padding: 12px; border-radius: var(--ui-radius-md); border: 1px solid var(--ui-border);
  background: transparent; color: var(--ui-text); font-family: inherit; cursor: pointer;
  transition: border-color var(--ui-dur-fast) var(--ui-ease-out), background-color var(--ui-dur-fast) var(--ui-ease-out), transform var(--ui-dur-fast) var(--ui-ease-out);
}
.sw-choice:hover:not(:disabled) { border-color: var(--ui-border-hover); background: var(--ui-glass); transform: translateY(-1px); }
.sw-choice:disabled { opacity: 0.45; cursor: not-allowed; }
.sw-choice > :first-child { color: var(--ui-accent); flex-shrink: 0; }
.sw-choice strong { display: block; font-size: 13px; }
.sw-choice small { display: block; font-size: 11px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.sw-picker {
  display: flex; gap: 10px; align-items: center; text-align: left; width: 100%;
  padding: 12px; border-radius: var(--ui-radius-md); border: 1px dashed var(--ui-border-strong);
  background: transparent; color: var(--ui-text); font-family: inherit; cursor: pointer;
}
.sw-picker:hover { border-color: var(--ui-accent); }
.sw-picker > :first-child { color: var(--ui-accent); flex-shrink: 0; }
.sw-picker strong { display: block; font-size: 13px; }
.sw-picker small { display: block; font-size: 11px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.sw-details { border: 1px solid var(--ui-border); border-radius: var(--ui-radius-md); padding: 8px 12px; font-size: 12px; }
.sw-details summary { cursor: pointer; color: color-mix(in srgb, var(--ui-text) 70%, transparent); }
.sw-details .sw-field { margin-top: 8px; }
.sw-details .sw-row { margin-top: 8px; }
.sw-hint { margin: 0; font-size: 12px; line-height: 1.55; color: color-mix(in srgb, var(--ui-text) 65%, transparent); }
.muted { color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.sw-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.sw-check { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: color-mix(in srgb, var(--ui-text) 75%, transparent); cursor: pointer; }
.sw-actions { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-top: 4px; padding-top: 10px; border-top: 1px dashed var(--ui-border); }
.sw-fields { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 10px; }
.sw-field { display: flex; flex-direction: column; gap: 5px; font-size: 11px; min-width: 0; }
.sw-field.grow { grid-column: 1 / -1; }
.sw-label { font-size: 11px; font-weight: 600; letter-spacing: 0; color: var(--ui-text-2); }
.sw-input {
  background: var(--ui-surface); border: 1px solid var(--ui-border); border-radius: var(--ui-radius-sm);
  color: var(--ui-text); font-family: inherit; font-size: 12px; padding: 8px 10px; outline: none; width: 100%;
}
.sw-input:focus { border-color: var(--ui-accent); box-shadow: var(--ui-focus-ring); }
.sw-btn {
  display: inline-flex; align-items: center; gap: 6px;
  background: transparent; border: 1px solid var(--ui-border); border-radius: var(--ui-radius-sm);
  color: color-mix(in srgb, var(--ui-text) 75%, transparent);
  font-family: inherit; font-size: 12px; font-weight: 600; padding: 7px 12px; cursor: pointer; white-space: nowrap;
}
.sw-btn:hover:not(:disabled) { color: var(--ui-text); border-color: var(--ui-border-strong); }
.sw-btn:disabled { opacity: 0.45; cursor: not-allowed; }
.sw-btn.primary { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent); }
.sw-btn.primary:hover:not(:disabled) { background: var(--ui-accent); border-color: var(--ui-accent); color: var(--ui-on-accent); box-shadow: var(--ui-shadow-1); }
.sw-link { background: none; border: none; color: var(--ui-info); cursor: pointer; font: inherit; font-size: 12px; text-decoration: underline; padding: 0; border-radius: var(--ui-radius-xs); margin-left: auto; }
.sw-hidden-file { position: absolute; width: 1px; height: 1px; opacity: 0; pointer-events: none; }
.sw-note { margin: 0; font-size: 11.5px; color: var(--ui-info); line-height: 1.5; }
.sw-note.err { color: var(--ui-danger); }
.sw-note.ok { color: var(--ui-accent); }
.sw-banner {
  display: flex; align-items: flex-start; gap: 10px; border-radius: var(--ui-radius-md);
  padding: 10px 12px; font-size: 12px; line-height: 1.5; border: 1px solid;
}
.sw-banner.info { border-color: color-mix(in srgb, var(--ui-info) 50%, transparent); background: color-mix(in srgb, var(--ui-info) 8%, transparent); }
.sw-presets { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 8px; }
.sw-preset {
  display: flex; flex-direction: column; align-items: flex-start; gap: 3px;
  padding: 10px; border-radius: var(--ui-radius-md); border: 1px solid var(--ui-border);
  background: transparent; color: var(--ui-text); font-family: inherit; font-size: 12px; cursor: pointer; text-align: left;
  transition: border-color var(--ui-dur-fast) var(--ui-ease-out), background-color var(--ui-dur-fast) var(--ui-ease-out), transform var(--ui-dur-fast) var(--ui-ease-out), box-shadow var(--ui-dur) var(--ui-ease-out);
}
.sw-preset:hover { border-color: var(--ui-border-hover); transform: translateY(-1px); }
.sw-preset.on { border-color: var(--ui-accent); background: var(--ui-accent-softer); box-shadow: var(--ui-focus-ring); }
.sw-preset .muted { font-size: 11px; }
.sw-theme-swatch {
  width: 100%; height: 26px; border-radius: var(--ui-radius-sm);
  border: 1px solid var(--ui-border);
}
.sw-wp-swatch {
  width: 100%; height: 26px; border-radius: var(--ui-radius-sm);
  border: 1px solid var(--ui-border);
}
.sw-wp-slopes-dark { background: linear-gradient(180deg, #2b3136 0%, #1b1e20 55%, #101315 100%); }
.sw-wp-slopes-light { background: linear-gradient(180deg, #f4f5f6 0%, #dcdfe3 60%, #c9ced4 100%); }
.sw-wp-dunes { background: radial-gradient(120% 90% at 80% 110%, rgba(246, 116, 0, 0.35), transparent 55%), linear-gradient(180deg, #232629 0%, #0e1113 100%); }
.sw-free {
  font-size: 11px; font-weight: 600; letter-spacing: 0;
  color: var(--ui-accent); border: 1px solid color-mix(in srgb, var(--ui-accent) 50%, transparent);
  border-radius: var(--ui-radius-xs); padding: 1px 7px; margin-top: 3px;
}
.sw-oauth-grid { display: grid; grid-template-columns: 1fr; gap: 8px; }
.sw-oauth {
  display: flex; gap: 10px; align-items: center; text-align: left; width: 100%;
  padding: 10px 12px; border-radius: var(--ui-radius-md); border: 1px solid var(--ui-border);
  background: transparent; color: var(--ui-text); font-family: inherit; font-size: 12.5px; cursor: pointer;
}
.sw-oauth:hover:not(:disabled) { border-color: var(--ui-accent); }
.sw-oauth:disabled { opacity: 0.45; cursor: not-allowed; }
.sw-oauth-meta { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
.sw-oauth-meta small { font-size: 11px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.sw-substep {
  display: flex; flex-direction: column; gap: 8px;
  border: 1px dashed var(--ui-border-strong); border-radius: var(--ui-radius-md);
  padding: 10px 12px;
}
.sw-substep-title { margin: 0; font-size: 12.5px; }
.sw-prov-card {
  display: flex; flex-direction: column; gap: 8px;
  border: 1px solid var(--ui-border); border-radius: var(--ui-radius-md);
  padding: 10px 12px;
}
.sw-prov-head { display: flex; align-items: center; gap: 8px; }
.sw-prov-meta { display: flex; flex-direction: column; gap: 1px; min-width: 0; flex: 1; }
.sw-prov-meta small { font-size: 11px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.sw-hint code { font-family: var(--ui-font-mono); font-size: 11px; color: var(--ui-info); }
.sw-summary { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
.sw-summary li {
  display: flex; gap: 10px; align-items: center;
  border: 1px solid var(--ui-border); border-radius: var(--ui-radius-md); padding: 10px 12px; font-size: 12.5px;
}
@media (prefers-reduced-motion: reduce) {
  .sw-btn, .sw-preset, .sw-choice, .sw-picker { transition: none; }
}
</style>
