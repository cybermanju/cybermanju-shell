<!-- CyberManju OS — AppIcon (src/components/AppIcon.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @iconify, @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <Icon
    v-if="resolved"
    :icon="resolved"
    :width="size"
    :height="size"
    :color="color"
    aria-hidden="true"
    focusable="false"
    class="app-icon"
  />
  <span v-else class="app-icon app-icon--missing" aria-hidden="true" />
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Icon } from '@iconify/vue/offline'
import { hasLocalIcon, registerLocalIcons } from '@/utils/iconify'

registerLocalIcons()

const props = withDefaults(
  defineProps<{
    /** Iconify name, e.g. "solar:trash-bin-trash-bold". Empty/unknown names render a spacer. */
    name?: string | null
    /** Any CSS length. Defaults to 1em so icons track the surrounding font-size. */
    size?: string | number
    /** Optional explicit colour; defaults to currentColor. */
    color?: string
  }>(),
  { name: '', size: '1em', color: undefined }
)

const resolved = computed(() => {
  const name = (props.name ?? '').trim()
  return name && hasLocalIcon(name) ? name : null
})
</script>

<style scoped>
.app-icon {
  display: inline-block;
  vertical-align: -0.15em;
  flex-shrink: 0;
}

.app-icon--missing {
  width: 1em;
  height: 1em;
}
</style>
