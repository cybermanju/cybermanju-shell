<!-- CyberManju OS — InstallPanel (src/components/InstallPanel.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/utils/installLinks, @/components/ui, vue. -->
<!-- Contracts: signals: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="ip">
    <header class="ip-top">
      <div class="ip-brand">
        <span class="ip-brand-mark"><AppIcon name="solar:download-bold" :size="20" /></span>
        <div>
          <h2 class="ip-title">Install the native app</h2>
          <p class="ip-subtitle">{{ reasonLine }}</p>
        </div>
      </div>
      <div class="ip-top-actions">
        <UiButton size="sm" icon="solar:refresh-bold" :loading="resolving" @click="resolveAssets()">Refresh links</UiButton>
      </div>
    </header>

    <div v-if="resolveError" class="ip-note">{{ resolveError }} — showing the release page instead.</div>

    <!-- ── platform cards: App × CLI with per-OS specifics ── -->
    <section class="ip-grid" aria-label="Install options">
      <article v-for="t in targets" :key="t.id" class="ip-card" :aria-label="t.title">
        <div class="ip-card-head">
          <span class="ip-logo" :data-logo="t.logo" aria-hidden="true" v-html="logos[t.logo]" />
          <div class="ip-head-t">
            <span class="ip-os">{{ t.os.toUpperCase() }}</span>
            <strong class="ip-name">{{ t.title }}</strong>
          </div>
        </div>
        <p class="ip-blurb">{{ t.blurb }}</p>

        <div v-if="t.app.length" class="ip-group">
          <span class="ip-group-h">APP</span>
          <div v-for="l in t.app" :key="l.label" class="ip-row">
            <a class="ip-link" :href="hrefFor(t.id, l)" target="_blank" rel="noopener">
              <AppIcon name="solar:download-bold" :size="12" /> {{ l.label }}
            </a>
            <span v-if="l.note" class="ip-note-sm">{{ l.note }}</span>
          </div>
        </div>

        <div v-if="t.cli.length" class="ip-group">
          <span class="ip-group-h">CLI · cyb</span>
          <div v-for="l in t.cli" :key="l.label" class="ip-row">
            <a class="ip-link ghost" :href="hrefFor(t.id, l)" target="_blank" rel="noopener">
              <AppIcon name="solar:share-bold" :size="12" /> {{ l.label }}
            </a>
            <span v-if="l.note" class="ip-cmd" :title="l.note">{{ l.note }}</span>
            <button
              v-if="isCommand(l.note)"
              class="ip-copy"
              type="button"
              :title="`Copy: ${l.note}`"
              @click="copyCmd(l.note || '')"
            >
              <AppIcon name="solar:copy-bold" :size="11" /> {{ copied === l.note ? 'copied' : 'copy' }}
            </button>
          </div>
        </div>

        <ul class="ip-specs">
          <li v-for="(s, i) in t.specifics" :key="i">{{ s }}</li>
        </ul>
      </article>
    </section>

    <footer class="ip-foot">
      <span>All downloads live on the <a :href="releasePage" target="_blank" rel="noopener">release page</a> — verify with <span class="mono">SHA256SUMS</span> before running anything.</span>
      <a class="ip-link ghost" :href="shaUrl" target="_blank" rel="noopener"><AppIcon name="solar:share-bold" :size="12" /> Checksums</a>
    </footer>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import UiButton from '@/components/ui/UiButton.vue'
import { ref, computed, onMounted } from 'vue'
import {
  INSTALL_TAG_URLS,
  INSTALL_TARGETS,
  fetchLatestReleaseAssets,
  pickInstallAssetUrls,
  releasePageUrl,
} from '@/utils/installLinks'
import type { InstallLogo } from '@/utils/installLinks'

const props = defineProps<{ reason?: string }>()

const reasonLine = computed(() => {
  if (props.reason === 'host-unsupported') {
    return 'This browser has no folder picker, so Device files cannot load here. Install a native build below — or continue in Chrome/Edge over HTTPS.'
  }
  return 'Native builds unlock device files, the system shell and the full vault. Pick your platform below.'
})

// Brand marks are hand-drawn inline SVG (no icon-font dependency): one
// geometric glyph per OS in its brand color, decorative only.
const logos: Record<InstallLogo, string> = {
  android: `<svg viewBox="0 0 24 24" width="26" height="26" aria-hidden="true"><rect x="5" y="8" width="14" height="10" rx="4.5" fill="#3DDC84"/><circle cx="9.5" cy="12" r="1.1" fill="#0a0a0a"/><circle cx="14.5" cy="12" r="1.1" fill="#0a0a0a"/><rect x="3.4" y="10" width="1.8" height="6" rx="0.9" fill="#3DDC84"/><rect x="18.8" y="10" width="1.8" height="6" rx="0.9" fill="#3DDC84"/><rect x="7" y="18.5" width="2" height="3" rx="1" fill="#3DDC84"/><rect x="15" y="18.5" width="2" height="3" rx="1" fill="#3DDC84"/><rect x="6.6" y="4.6" width="1.7" height="3.4" rx="0.85" transform="rotate(-28 7.4 6.3)" fill="#3DDC84"/><rect x="15.7" y="4.6" width="1.7" height="3.4" rx="0.85" transform="rotate(28 16.6 6.3)" fill="#3DDC84"/></svg>`,
  ubuntu: `<svg viewBox="0 0 24 24" width="26" height="26" aria-hidden="true"><circle cx="12" cy="12" r="9.5" fill="#E95420"/><circle cx="12" cy="12" r="4" fill="none" stroke="#fff" stroke-width="1.6"/><circle cx="12" cy="4.6" r="1.7" fill="#fff"/><circle cx="18.4" cy="16.2" r="1.7" fill="#fff"/><circle cx="5.6" cy="16.2" r="1.7" fill="#fff"/></svg>`,
  arch: `<svg viewBox="0 0 24 24" width="26" height="26" aria-hidden="true"><rect width="24" height="24" rx="5" fill="#1793D1"/><path d="M12 5.5 18.5 18.5h-2.9L12 10.4 8.4 18.5H5.5Z" fill="#fff"/><rect x="10.9" y="15.4" width="2.2" height="3.1" fill="#fff"/></svg>`,
  windows: `<svg viewBox="0 0 24 24" width="26" height="26" aria-hidden="true"><rect x="3" y="4.2" width="8.4" height="7.2" fill="#00ADEF"/><rect x="12.6" y="4.2" width="8.4" height="7.2" fill="#00ADEF"/><rect x="3" y="12.6" width="8.4" height="7.2" fill="#00ADEF"/><rect x="12.6" y="12.6" width="8.4" height="7.2" fill="#00ADEF"/></svg>`,
  macos: `<svg viewBox="0 0 24 24" width="26" height="26" aria-hidden="true"><rect x="4" y="4" width="16" height="16" rx="4.5" fill="none" stroke="#fff" stroke-width="1.8"/><path d="M9.2 9.2h5.6v5.6H9.2z M12 6.8v10.4 M6.8 12h10.4" stroke="#fff" stroke-width="1.8" fill="none"/><circle cx="12" cy="12" r="1.4" fill="#fff"/></svg>`,
  docker: `<svg viewBox="0 0 24 24" width="26" height="26" aria-hidden="true"><rect x="4" y="5" width="7" height="4.4" rx="0.8" fill="#2496ED"/><rect x="13" y="5" width="7" height="4.4" rx="0.8" fill="#2496ED"/><rect x="4" y="10.6" width="7" height="4.4" rx="0.8" fill="#2496ED"/><rect x="13" y="10.6" width="7" height="4.4" rx="0.8" fill="#2496ED"/><rect x="8.5" y="16.2" width="7" height="3.4" rx="0.8" fill="#2496ED"/></svg>`,
}

const targets = INSTALL_TARGETS
const resolved = ref<Record<string, string>>({})
const releaseTag = ref('latest')
const resolving = ref(false)
const resolveError = ref('')
const copied = ref('')

const releasePage = computed(() => releasePageUrl(releaseTag.value))
const shaUrl = computed(() => INSTALL_TAG_URLS.sha256(releaseTag.value))

async function resolveAssets() {
  resolving.value = true
  resolveError.value = ''
  try {
    const { tag, assets } = await fetchLatestReleaseAssets()
    releaseTag.value = tag
    resolved.value = pickInstallAssetUrls(assets, tag)
  } catch (e) {
    resolveError.value = e instanceof Error ? e.message : String(e)
    resolved.value = {}
  } finally {
    resolving.value = false
  }
}

function hrefFor(targetId: string, link: { label: string; href: string }): string {
  if (link.href) return link.href
  return resolved.value[`${targetId}:${link.label}`] || releasePageUrl(releaseTag.value)
}

/** Notes that are runnable commands get a copy button (not prose). */
function isCommand(note?: string): boolean {
  const n = String(note ?? '')
  return /curl|irm|cyb |dpkg|makepkg|docker|sha256|shasum|chmod|export PATH/i.test(n)
}

async function copyCmd(text: string) {
  if (!text) return
  try {
    await navigator.clipboard.writeText(text)
    copied.value = text
    setTimeout(() => {
      if (copied.value === text) copied.value = ''
    }, 2000)
  } catch {
    // Clipboard unavailable (permissions) — the text stays selectable.
  }
}

onMounted(() => {
  void resolveAssets()
})
</script>

<style scoped>
.ip { display: flex; flex-direction: column; gap: 12px; height: 100%; overflow-y: auto; padding: 14px;
  background: var(--ui-surface); color: var(--ui-text); font-size: 12px; }
.ip-top { display: flex; align-items: flex-start; gap: 10px; }
.ip-brand { display: flex; gap: 10px; flex: 1; min-width: 0; }
.ip-brand-mark { display: flex; align-items: center; justify-content: center; width: 38px; height: 38px; border-radius: 12px; flex-shrink: 0;
  background: color-mix(in srgb, var(--ui-accent) 16%, transparent); color: var(--ui-accent); }
.ip-title { margin: 0; font-size: 16px; font-weight: 800; letter-spacing: 0.01em; }
.ip-subtitle { margin: 2px 0 0; font-size: 11.5px; color: var(--ui-text-2); line-height: 1.45; }
.ip-top-actions { display: flex; gap: 6px; flex-shrink: 0; }
.ip-note { font-size: 11px; color: var(--ui-text-3); border: 1px solid var(--ui-hairline); border-radius: 10px; padding: 7px 10px; background: color-mix(in srgb, var(--ui-text) 3%, transparent); }
.ip-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 10px; }
.ip-card { border: 1px solid var(--ui-border); border-radius: 14px; padding: 12px; background: var(--ui-glass);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate)); -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate)); }
.ip-card-head { display: flex; align-items: center; gap: 10px; margin-bottom: 6px; }
.ip-logo { display: flex; width: 34px; height: 34px; align-items: center; justify-content: center; border-radius: 10px; flex-shrink: 0;
  background: color-mix(in srgb, var(--ui-text) 5%, transparent); border: 1px solid var(--ui-hairline); }
.ip-head-t { display: flex; flex-direction: column; min-width: 0; }
.ip-os { font-size: 9px; font-weight: 800; letter-spacing: 0.12em; color: var(--ui-text-3); }
.ip-name { font-size: 13px; font-weight: 800; }
.ip-blurb { margin: 0 0 8px; font-size: 11px; color: var(--ui-text-2); line-height: 1.45; }
.ip-group { margin-bottom: 8px; }
.ip-group-h { display: block; font-size: 9px; font-weight: 800; letter-spacing: 0.12em; color: var(--ui-text-3); margin-bottom: 4px; }
.ip-row { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; padding: 3px 0; }
.ip-link { display: inline-flex; align-items: center; gap: 5px; padding: 4px 11px; border-radius: 999px; font-size: 11px; font-weight: 700;
  color: var(--ui-text); background: color-mix(in srgb, var(--ui-text) 6%, transparent); border: 1px solid var(--ui-border); text-decoration: none; }
.ip-link:hover { background: color-mix(in srgb, var(--ui-text) 10%, transparent); }
.ip-link.ghost { background: transparent; border-color: var(--ui-hairline); color: var(--ui-text-2); }
.ip-note-sm { font-size: 10px; color: var(--ui-text-3); }
.ip-cmd { font-family: var(--ui-font-mono, ui-monospace, monospace); font-size: 10px; color: var(--ui-text-2);
  background: color-mix(in srgb, var(--ui-text) 5%, transparent); border: 1px solid var(--ui-hairline); border-radius: 7px; padding: 2px 7px;
  max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ip-copy { display: inline-flex; align-items: center; gap: 4px; font-size: 10px; font-weight: 700; cursor: pointer;
  background: transparent; border: 1px solid var(--ui-hairline); color: var(--ui-text-3); border-radius: 7px; padding: 2px 8px; }
.ip-copy:hover { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 45%, transparent); }
.ip-specs { margin: 6px 0 0; padding-left: 16px; font-size: 10.5px; color: var(--ui-text-3); line-height: 1.5; }
.ip-specs li::marker { color: var(--ui-accent); }
.ip-foot { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; font-size: 11px; color: var(--ui-text-3);
  border-top: 1px solid var(--ui-hairline); padding-top: 10px; }
.ip-foot a { color: var(--ui-accent); }
.mono { font-family: var(--ui-font-mono, ui-monospace, monospace); }
</style>
