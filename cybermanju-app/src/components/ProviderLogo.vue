<!-- CyberManju OS — ProviderLogo (src/components/ProviderLogo.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <span
    class="provider-logo"
    :class="[`provider-logo--${kind}`, { 'provider-logo--sm': size <= 20 }]"
    :style="{ width: `${size}px`, height: `${size}px`, borderRadius: `${Math.max(6, size * 0.28)}px` }"
    role="img"
    :aria-label="label"
    :title="label"
  >
    <svg
      v-if="path"
      :width="Math.round(size * 0.58)"
      :height="Math.round(size * 0.58)"
      viewBox="0 0 24 24"
      aria-hidden="true"
      focusable="false"
    >
      <path :d="path" :fill="glyph" />
    </svg>
    <AppIcon v-else :name="fallbackIcon" :size="Math.round(size * 0.58)" />
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import AppIcon from '@/components/AppIcon.vue'

// Brand glyphs: CC0 Simple Icons paths (github, gitlab, google,
// googledrive), rendered monochrome in the brand colour on a
// neutral tile so every provider is recognisable in dark and light themes.
const PATHS: Record<string, string> = {
  github:
    'M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12',
  gitlab:
    'm23.6004 9.5927-.0337-.0862L20.3.9814a.851.851 0 0 0-.3362-.405.8748.8748 0 0 0-.9997.0539.8748.8748 0 0 0-.29.4399l-2.2055 6.748H7.5375l-2.2057-6.748a.8573.8573 0 0 0-.29-.4412.8748.8748 0 0 0-.9997-.0537.8585.8585 0 0 0-.3362.4049L.4332 9.5015l-.0325.0862a6.0657 6.0657 0 0 0 2.0119 7.0105l.0113.0087.03.0213 4.976 3.7264 2.462 1.8633 1.4995 1.1321a1.0085 1.0085 0 0 0 1.2197 0l1.4995-1.1321 2.4619-1.8633 5.006-3.7489.0125-.01a6.0682 6.0682 0 0 0 2.0094-7.003z',
  google:
    'M12.48 10.92v3.28h7.84c-.24 1.84-.853 3.187-1.787 4.133-1.147 1.147-2.933 2.4-6.053 2.4-4.827 0-8.6-3.893-8.6-8.72s3.773-8.72 8.6-8.72c2.6 0 4.507 1.027 5.907 2.347l2.307-2.307C18.747 1.44 16.133 0 12.48 0 5.867 0 .307 5.387.307 12s5.56 12 12.173 12c3.573 0 6.267-1.173 8.373-3.36 2.16-2.16 2.84-5.213 2.84-7.667 0-.76-.053-1.467-.173-2.053H12.48z',
  googleDrive:
    'M12.01 1.485c-2.082 0-3.754.02-3.743.047.01.02 1.708 3.001 3.774 6.62l3.76 6.574h3.76c2.081 0 3.753-.02 3.742-.047-.005-.02-1.708-3.001-3.775-6.62l-3.76-6.574zm-4.76 1.73a789.828 789.861 0 0 0-3.63 6.319L0 15.868l1.89 3.298 1.885 3.297 3.62-6.335 3.618-6.33-1.88-3.287C8.1 4.704 7.255 3.22 7.25 3.214zm2.259 12.653-.203.348c-.114.198-.96 1.672-1.88 3.287a423.93 423.948 0 0 1-1.698 2.97c-.01.026 3.24.042 7.222.042h7.244l1.796-3.157c.992-1.734 1.85-3.23 1.906-3.323l.104-.167h-7.249z',
}

const GLYPHS: Record<string, string> = {
  github: '#f0f1f4',
  gitlab: '#fc6d26',
  google: '#4285f4',
  googleDrive: '#34a853',
}

const LABELS: Record<string, string> = {
  github: 'GitHub',
  gitlab: 'GitLab',
  google: 'Google',
  googleDrive: 'Google Drive',
  local: 'Local folder',
}

const props = withDefaults(
  defineProps<{
    provider?: string | null
    size?: number
  }>(),
  { provider: 'local', size: 32 },
)

function normalise(p: string): string {
  const v = (p || '').toLowerCase()
  if (v === 'googledrive' || v === 'google-drive' || v === 'drive') return 'googleDrive'
  if (v === 'google') return 'google'
  if (v === 'github') return 'github'
  if (v === 'gitlab') return 'gitlab'
  return 'local'
}

const kind = computed(() => normalise(props.provider ?? 'local'))
const path = computed(() => PATHS[kind.value] ?? null)
const glyph = computed(() => GLYPHS[kind.value] ?? '#f0f1f4')
const label = computed(() => LABELS[kind.value] ?? kind.value)
const fallbackIcon = computed(() =>
  kind.value === 'local' ? 'solar:ssd-square-bold' : 'solar:cloud-bold',
)
</script>

<style scoped>
.provider-logo {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  background: color-mix(in srgb, var(--ui-text) 8%, transparent);
  border: 1px solid var(--ui-border);
}
.provider-logo--github {
  background: #161b22;
  border-color: #30363d;
}
.provider-logo--gitlab {
  background: color-mix(in srgb, #fc6d26 14%, transparent);
}
.provider-logo--google,
.provider-logo--googleDrive {
  background: color-mix(in srgb, var(--ui-text) 10%, transparent);
}
</style>
