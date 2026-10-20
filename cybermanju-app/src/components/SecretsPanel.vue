<!-- CyberManju OS — SecretsPanel (src/components/SecretsPanel.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="sp">
    <!-- ── category sidebar ── -->
    <aside class="sp-side" aria-label="Vault categories">
      <button
        v-for="cat in categories"
        :key="cat.id"
        class="sp-cat"
        :class="{ active: activeCategory === cat.id }"
        type="button"
        @click="activeCategory = cat.id"
      >
        <AppIcon :name="cat.icon" :size="16" />
        <span class="sp-cat-label">{{ cat.label }}</span>
        <span class="sp-cat-count">{{ categoryCount(cat.id) }}</span>
      </button>
      <div class="sp-side-foot">
        <span class="sp-muted">{{ secrets.length }} stored · sealed at rest</span>
      </div>
    </aside>

    <!-- ── main ── -->
    <section class="sp-main">
      <header class="sp-top">
        <UiInput
          v-model="query"
          class="sp-search"
          placeholder="Search title, username, url, tags…"
          prefix-icon="solar:magnifier-bold"
          clearable
        />
        <UiButton size="sm" icon="solar:add-bold" @click="openCreate()">Add</UiButton>
        <UiButton size="sm" variant="ghost" icon="solar:upload-bold" @click="importOpen = true">Import</UiButton>
      </header>

      <UiEmpty
        v-if="!filtered.length"
        size="sm"
        icon="solar:wallet-bold"
        title="No secrets here"
        description="Store a password, card, note or API key — values are sealed at rest and never listed."
      />
      <div v-else class="sp-list" role="list">
        <button
          v-for="row in filtered"
          :key="row.id"
          class="sp-row"
          :class="{ selected: selected?.id === row.id }"
          type="button"
          role="listitem"
          @click="select(row)"
        >
          <AppIcon :name="kindIcon(row.kind)" :size="18" class="sp-row-icon" />
          <div class="sp-row-info">
            <span class="sp-row-title">
              <AppIcon v-if="row.favorite" name="solar:star-bold" :size="12" class="sp-fav" />
              {{ row.title }}
            </span>
            <span class="sp-row-sub">{{ row.username || row.url || kindLabel(row.kind) }}</span>
          </div>
          <span class="sp-row-tags">
            <span v-for="t in row.tags.slice(0, 2)" :key="t" class="sp-chip">{{ t }}</span>
          </span>
        </button>
      </div>
    </section>

    <!-- ── detail card ── -->
    <section v-if="selected" class="sp-detail" aria-label="Secret detail">
      <header class="sp-detail-head">
        <AppIcon :name="kindIcon(selected.kind)" :size="22" />
        <div class="sp-detail-title">
          <strong>{{ selected.title }}</strong>
          <span class="sp-muted">{{ kindLabel(selected.kind) }} · updated {{ agoLabel(selected.updatedAt) }}</span>
        </div>
        <div class="sp-detail-actions">
          <UiButton size="sm" icon="solar:pen-bold" @click="openEdit(selected)">Edit</UiButton>
          <UiButton size="sm" variant="danger" icon="solar:trash-bin-trash-bold" @click="confirmDelete(selected)">
            Delete
          </UiButton>
        </div>
      </header>

      <div v-if="selected.username" class="sp-field">
        <span class="sp-field-label">Username</span>
        <span class="sp-field-value">{{ selected.username }}</span>
        <UiButton size="sm" variant="ghost" icon="solar:copy-bold" @click="copyPlain(selected.username || '')">
          Copy
        </UiButton>
      </div>
      <div v-if="selected.url" class="sp-field">
        <span class="sp-field-label">URL</span>
        <span class="sp-field-value sp-break">{{ selected.url }}</span>
      </div>

      <div class="sp-field">
        <span class="sp-field-label">Password</span>
        <span class="sp-field-value sp-mono">
          <template v-if="revealed !== null">{{ revealed }}</template>
          <template v-else-if="selected.hasValue">••••••••••••</template>
          <template v-else class="sp-muted">(no value stored)</template>
        </span>
        <UiButton
          v-if="selected.hasValue"
          size="sm"
          :icon="revealed !== null ? 'solar:eye-closed-bold' : 'solar:eye-bold'"
          @click="toggleReveal"
        >
          {{ revealed !== null ? `Hide (${revealCountdown}s)` : 'Reveal' }}
        </UiButton>
        <UiButton
          v-if="selected.hasValue"
          size="sm"
          icon="solar:copy-bold"
          :disabled="revealed === null"
          @click="copyRevealed"
        >
          Copy
        </UiButton>
      </div>

      <div v-if="selected.notes" class="sp-notes">
        <span class="sp-field-label">Notes</span>
        <p class="sp-notes-body">{{ selected.notes }}</p>
      </div>
      <p class="sp-hint">
        Values are sealed at rest (Argon2id + ChaCha20Poly1305). Reveal is audited;
        Copy auto-clears the clipboard after 30 s.
      </p>
    </section>

    <!-- ── create / edit modal ── -->
    <UiModal
      :visible="editorOpen"
      :title="editing?.id ? 'Edit secret' : 'New secret'"
      size="md"
      @close="closeEditor"
    >
      <form class="sp-form" @submit.prevent="save">
        <UiInput v-model="form.title" label="Title" required placeholder="GitHub" />
        <div class="sp-form-row">
          <label class="sp-form-field">
            <span>Kind</span>
            <select v-model="form.kind" class="sp-select">
              <option value="login">Login</option>
              <option value="card">Card</option>
              <option value="note">Note</option>
              <option value="apiKey">API key</option>
            </select>
          </label>
          <UiInput v-model="form.username" label="Username" placeholder="octocat" />
        </div>
        <UiInput v-model="form.url" label="URL" placeholder="https://github.com" />
        <div class="sp-form-field sp-value-field">
          <span class="sp-field-label">Value</span>
          <div class="sp-value-row">
            <input
              v-model="form.value"
              class="sp-value-input"
              :type="formShowValue ? 'text' : 'password'"
              autocomplete="off"
              spellcheck="false"
              placeholder="password / number / token"
            />
            <UiButton size="sm" variant="ghost" :icon="formShowValue ? 'solar:eye-closed-bold' : 'solar:eye-bold'" @click="formShowValue = !formShowValue" type="button">
              {{ formShowValue ? 'Hide' : 'Show' }}
            </UiButton>
            <UiButton
              v-if="form.kind !== 'card'"
              size="sm"
              icon="solar:magic-wand-3-bold"
              type="button"
              @click="regenerate"
            >
              Generate
            </UiButton>
          </div>
          <div v-if="form.value && form.kind !== 'card'" class="sp-strength">
            <div class="sp-strength-bar">
              <span :data-strength="formStrength.strength" :style="{ width: formStrengthPct + '%' }" />
            </div>
            <span class="sp-strength-label" :data-strength="formStrength.strength">
              {{ formStrength.strength }} · {{ Math.round(formStrength.bits) }} bits
            </span>
          </div>
        </div>
        <UiInput v-model="form.category" label="Category" placeholder="work" />
        <UiInput v-model="form.tagsRaw" label="Tags" placeholder="comma, separated" />
        <label class="sp-form-check">
          <input v-model="form.favorite" type="checkbox" />
          <span>Favorite</span>
        </label>
        <UiInput v-model="form.notes" label="Notes" placeholder="optional" />
        <footer class="sp-form-foot">
          <UiButton type="button" variant="ghost" @click="closeEditor">Cancel</UiButton>
          <UiButton type="submit" :disabled="!form.title.trim() || saving" :loading="saving">
            Save
          </UiButton>
        </footer>
      </form>
    </UiModal>

    <!-- ── CSV import modal (§4.1: 1Password / Bitwarden) ── -->
    <UiModal
      :visible="importOpen"
      title="Import passwords"
      size="md"
      @close="importOpen = false"
    >
      <p class="sp-hint">
        Paste a 1Password or Bitwarden CSV export (unencrypted file only — delete
        it afterwards). Values seal on write; nothing echoes back.
      </p>
      <textarea
        v-model="csvText"
        class="sp-import-area"
        rows="8"
        spellcheck="false"
        placeholder="title,username,password,url…"
        aria-label="CSV to import"
      />
      <p v-if="importResult" class="sp-hint">
        {{ importResult.format }} · {{ importResult.rows.length }} rows
        <span v-if="importResult.skipped">· {{ importResult.skipped }} skipped (no title)</span>
        <span v-if="importResult.errors.length" class="sp-import-err"> · {{ importResult.errors.join('; ') }}</span>
      </p>
      <footer class="sp-form-foot">
        <UiButton type="button" variant="ghost" @click="importOpen = false">Cancel</UiButton>
        <UiButton :disabled="!importResult?.rows.length || importing" :loading="importing" @click="runImport">
          Import {{ importResult?.rows.length ?? 0 }}
        </UiButton>
      </footer>
    </UiModal>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref } from 'vue'
import { useAppStore } from '@/stores/app'
import { useNotifications } from '@/composables/useNotifications'
import AppIcon from '@/components/AppIcon.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiModal from '@/components/ui/UiModal.vue'
import { copySecret, copyText } from '@/utils/clipboard'
import { generatePassword, passwordEntropyBits, passwordStrength, PASSWORD_DEFAULTS } from '@/utils/password'
import { parseSecretCsv } from '@/utils/secretImport'
import type { SecretKind, SecretMeta } from '@/types'

const store = useAppStore()
const { notify } = useNotifications()

const secrets = computed(() => store.secrets)
const query = ref('')
const activeCategory = ref<'all' | 'favorites' | SecretKind>('all')
const selected = ref<SecretMeta | null>(null)
const revealed = ref<string | null>(null)
const revealCountdown = ref(0)

const categories = [
  { id: 'all' as const, label: 'All', icon: 'solar:wallet-bold' },
  { id: 'favorites' as const, label: 'Favorites', icon: 'solar:star-bold' },
  { id: 'login' as const, label: 'Logins', icon: 'solar:key-bold' },
  { id: 'card' as const, label: 'Cards', icon: 'solar:card-bold' },
  { id: 'note' as const, label: 'Notes', icon: 'solar:document-text-bold' },
  { id: 'apiKey' as const, label: 'API keys', icon: 'solar:shield-keyhole-bold' },
]

function kindIcon(kind: SecretKind): string {
  return kind === 'card' ? 'solar:card-bold'
    : kind === 'note' ? 'solar:document-text-bold'
    : kind === 'apiKey' ? 'solar:shield-keyhole-bold'
    : 'solar:key-bold'
}

function kindLabel(kind: SecretKind): string {
  return kind === 'card' ? 'Card'
    : kind === 'note' ? 'Note'
    : kind === 'apiKey' ? 'API key'
    : 'Login'
}

function categoryCount(id: string): number {
  if (id === 'all') return secrets.value.length
  if (id === 'favorites') return secrets.value.filter(s => s.favorite).length
  return secrets.value.filter(s => s.kind === id).length
}

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase()
  return secrets.value.filter(s => {
    if (activeCategory.value === 'favorites' && !s.favorite) return false
    if (activeCategory.value !== 'all' && activeCategory.value !== 'favorites' && s.kind !== activeCategory.value) return false
    if (!q) return true
    return (
      s.title.toLowerCase().includes(q) ||
      (s.username || '').toLowerCase().includes(q) ||
      (s.url || '').toLowerCase().includes(q) ||
      s.tags.some(t => t.toLowerCase().includes(q))
    )
  })
})

function agoLabel(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime()
  if (!Number.isFinite(ms) || ms < 0) return 'just now'
  const m = Math.floor(ms / 60_000)
  if (m < 1) return 'just now'
  if (m < 60) return `${m}m ago`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h}h ago`
  return `${Math.floor(h / 24)}d ago`
}

function select(row: SecretMeta) {
  selected.value = row
  hideReveal()
}

// ── reveal (30 s auto-hide) ─────────────────────────────────
let revealTimer: ReturnType<typeof setInterval> | null = null

async function toggleReveal() {
  if (revealed.value !== null) {
    hideReveal()
    return
  }
  if (!selected.value) return
  const value = await store.revealSecret(selected.value.id)
  if (value === null) return
  revealed.value = value
  revealCountdown.value = 30
  revealTimer = setInterval(() => {
    revealCountdown.value -= 1
    if (revealCountdown.value <= 0) hideReveal()
  }, 1000)
}

function hideReveal() {
  revealed.value = null
  if (revealTimer) {
    clearInterval(revealTimer)
    revealTimer = null
  }
}

onBeforeUnmount(hideReveal)

async function copyRevealed() {
  if (revealed.value === null) return
  await navigator.clipboard.writeText(revealed.value).catch(() => {})
  await copySecret(revealed.value, { writeText: t => navigator.clipboard.writeText(t) })
  notify('success', 'Copied — clipboard clears in 30 s')
}

async function copyPlain(text: string) {
  await copyText(text, { writeText: t => navigator.clipboard.writeText(t) })
  notify('success', 'Copied')
}

// ── editor ──────────────────────────────────────────────────
const editorOpen = ref(false)
const saving = ref(false)
const editing = ref<SecretMeta | null>(null)
const formShowValue = ref(false)

const form = reactive({
  title: '',
  kind: 'login' as SecretKind,
  username: '',
  url: '',
  category: '',
  tagsRaw: '',
  favorite: false,
  notes: '',
  value: '',
})

const formStrength = computed(() => {
  const bits = passwordEntropyBits({ ...PASSWORD_DEFAULTS, length: form.value.length })
  return { bits, strength: passwordStrength(bits) }
})

const formStrengthPct = computed(() => Math.min(100, (formStrength.value.bits / 128) * 100))

function openCreate(kind: SecretKind = 'login') {
  editing.value = null
  Object.assign(form, {
    title: '', kind, username: '', url: '', category: '', tagsRaw: '',
    favorite: false, notes: '', value: '',
  })
  formShowValue.value = false
  editorOpen.value = true
}

function openEdit(row: SecretMeta) {
  editing.value = row
  Object.assign(form, {
    title: row.title,
    kind: row.kind,
    username: row.username ?? '',
    url: row.url ?? '',
    category: row.category ?? '',
    tagsRaw: row.tags.join(', '),
    favorite: row.favorite,
    notes: row.notes ?? '',
    // The plaintext is never prefilled — edit leaves it blank (omitted
    // value keeps the sealed blob) unless the user types a new one.
    value: '',
  })
  formShowValue.value = false
  editorOpen.value = true
}

function closeEditor() {
  editorOpen.value = false
}

function regenerate() {
  try {
    form.value = generatePassword()
    formShowValue.value = true
  } catch (e) {
    notify('error', e instanceof Error ? e.message : String(e))
  }
}

async function save() {
  const title = form.title.trim()
  if (!title) return
  saving.value = true
  try {
    const tags = form.tagsRaw.split(',').map(t => t.trim()).filter(Boolean)
    const input = {
      kind: form.kind,
      title,
      username: form.username.trim() || null,
      url: form.url.trim() || null,
      category: form.category.trim() || null,
      tags,
      notes: form.notes.trim() || null,
      favorite: form.favorite,
      // Omitted/empty value on edit keeps the existing sealed blob.
      value: form.value || null,
    }
    const saved = editing.value
      ? await store.updateSecret(editing.value.id, input)
      : await store.createSecret(input)
    if (saved) {
      selected.value = saved
      editorOpen.value = false
      notify('success', editing.value ? 'Secret updated' : 'Secret stored')
    }
  } finally {
    saving.value = false
  }
}

function confirmDelete(row: SecretMeta) {
  if (!window.confirm(`Delete “${row.title}” from the vault?`)) return
  void store.deleteSecret(row.id)
  if (selected.value?.id === row.id) {
    selected.value = null
    hideReveal()
  }
  notify('success', 'Secret deleted')
}

// ── CSV import (§4.1: 1Password / Bitwarden exports) ──
const importOpen = ref(false)
const csvText = ref('')
const importing = ref(false)
const importResult = computed(() => (csvText.value.trim() ? parseSecretCsv(csvText.value) : null))

async function runImport() {
  const rows = importResult.value?.rows ?? []
  if (!rows.length || importing.value) return
  importing.value = true
  let saved = 0
  try {
    for (const row of rows) {
      const done = await store.createSecret(row.input)
      if (done) saved += 1
    }
    notify('success', `Imported ${saved} of ${rows.length} secrets`)
    if (saved > 0) {
      csvText.value = ''
      importOpen.value = false
    }
  } finally {
    importing.value = false
  }
}
</script>

<style scoped>
.sp {
  display: grid;
  grid-template-columns: 180px 1fr minmax(260px, 320px);
  height: 100%;
  min-height: 0;
  color: var(--ui-fg, #e8ecef);
  background: var(--ui-bg, #0a0f0f);
}
.sp-side {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 10px 8px;
  border-right: 1px solid var(--ui-border, rgba(255, 255, 255, 0.08));
  overflow-y: auto;
}
.sp-cat {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: inherit;
  cursor: pointer;
  text-align: left;
  font-size: 12.5px;
}
.sp-cat:hover { background: rgba(255, 255, 255, 0.05); }
.sp-cat.active { background: rgba(255, 255, 255, 0.09); font-weight: 600; }
.sp-cat-label { flex: 1; }
.sp-cat-count { font-size: 11px; opacity: 0.55; }
.sp-side-foot { margin-top: auto; padding: 8px 10px; }
.sp-main { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
.sp-top {
  display: flex;
  gap: 8px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--ui-border, rgba(255, 255, 255, 0.08));
}
.sp-search { flex: 1; }
.sp-list { flex: 1; overflow-y: auto; padding: 6px; }
.sp-row {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 10px;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: inherit;
  cursor: pointer;
  text-align: left;
}
.sp-row:hover { background: rgba(255, 255, 255, 0.05); }
.sp-row.selected { background: rgba(255, 255, 255, 0.09); }
.sp-row-icon { opacity: 0.8; flex-shrink: 0; }
.sp-row-info { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.sp-row-title { display: flex; align-items: center; gap: 4px; font-size: 13px; font-weight: 600; }
.sp-fav { color: #f5c542; }
.sp-row-sub { font-size: 11.5px; opacity: 0.6; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.sp-row-tags { display: flex; gap: 4px; }
.sp-chip {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.08);
  opacity: 0.8;
}
.sp-detail {
  border-left: 1px solid var(--ui-border, rgba(255, 255, 255, 0.08));
  padding: 14px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.sp-detail-head { display: flex; align-items: flex-start; gap: 10px; }
.sp-detail-title { flex: 1; display: flex; flex-direction: column; gap: 2px; }
.sp-detail-actions { display: flex; gap: 6px; }
.sp-field {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.04);
  font-size: 12.5px;
}
.sp-field-label { min-width: 72px; opacity: 0.6; font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; }
.sp-field-value { flex: 1; min-width: 0; }
.sp-mono { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
.sp-break { word-break: break-all; }
.sp-notes-body { margin: 4px 0 0; font-size: 12.5px; line-height: 1.5; white-space: pre-wrap; }
.sp-hint { margin: 0; font-size: 11px; opacity: 0.5; line-height: 1.5; }
.sp-muted { font-size: 11px; opacity: 0.55; }
.sp-form { display: flex; flex-direction: column; gap: 10px; }
.sp-form-row { display: grid; grid-template-columns: 140px 1fr; gap: 10px; }
.sp-form-field { display: flex; flex-direction: column; gap: 4px; font-size: 12px; }
.sp-form-field > span { opacity: 0.7; }
.sp-select {
  padding: 7px 10px;
  border-radius: 8px;
  border: 1px solid var(--ui-border, rgba(255, 255, 255, 0.12));
  background: var(--ui-bg-elevated, #101818);
  color: inherit;
  font-size: 13px;
}
.sp-value-row { display: flex; gap: 6px; align-items: center; }
.sp-value-input {
  flex: 1;
  padding: 7px 10px;
  border-radius: 8px;
  border: 1px solid var(--ui-border, rgba(255, 255, 255, 0.12));
  background: var(--ui-bg-elevated, #101818);
  color: inherit;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 13px;
}
.sp-strength { display: flex; align-items: center; gap: 8px; margin-top: 4px; }
.sp-strength-bar { flex: 1; height: 4px; border-radius: 999px; background: rgba(255, 255, 255, 0.08); overflow: hidden; }
.sp-strength-bar > span { display: block; height: 100%; border-radius: 999px; background: #666; transition: width 0.15s ease; }
.sp-strength-bar > span[data-strength='fair'] { background: #d8a13a; }
.sp-strength-bar > span[data-strength='good'] { background: #4aa3df; }
.sp-strength-bar > span[data-strength='strong'] { background: #3ecf8e; }
.sp-strength-label { font-size: 11px; opacity: 0.7; }
.sp-strength-label[data-strength='strong'] { color: #3ecf8e; }
.sp-form-check { display: flex; align-items: center; gap: 8px; font-size: 13px; }
.sp-form-foot { display: flex; justify-content: flex-end; gap: 8px; margin-top: 4px; }
.sp-import-area {
  width: 100%;
  box-sizing: border-box;
  font-family: var(--ui-font-mono);
  font-size: 11px;
  background: transparent;
  color: var(--ui-text);
  border: 1px solid var(--ui-border, rgba(255, 255, 255, 0.08));
  border-radius: 6px;
  padding: 8px;
  resize: vertical;
}
.sp-import-err { color: var(--ui-danger); }
@media (max-width: 820px) {
  .sp { grid-template-columns: 1fr; }
  .sp-side { flex-direction: row; overflow-x: auto; border-right: 0; border-bottom: 1px solid var(--ui-border, rgba(255, 255, 255, 0.08)); }
  .sp-detail { border-left: 0; border-top: 1px solid var(--ui-border, rgba(255, 255, 255, 0.08)); }
}
</style>
