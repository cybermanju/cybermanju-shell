<!-- CyberManju OS — Own interactive home widgets; no external app data is implied. -->
<!-- Role: app-local Home UI only. Integrates: Vue and bounded persisted workspace state. -->
<template>
  <div class="local-widget">
    <template v-if="item.ref === 'clock'">
      <time class="widget-clock">{{ clock }}</time><span>{{ date }}</span>
    </template>
    <template v-else-if="item.ref === 'notes'">
      <label :for="`note-${item.id}`">Your scratchpad</label>
      <textarea :id="`note-${item.id}`" :value="String(data.text || '')" maxlength="2000" placeholder="An idea worth keeping…" @input="patch({ text: ($event.target as HTMLTextAreaElement).value })" />
    </template>
    <template v-else-if="item.ref === 'counter'">
      <strong class="widget-number">{{ Number(data.count || 0) }}</strong>
      <div class="widget-actions"><button type="button" aria-label="Decrease counter" @click="patch({ count: Number(data.count || 0) - 1 })">−</button><button type="button" aria-label="Increase counter" @click="patch({ count: Number(data.count || 0) + 1 })">+</button></div>
    </template>
    <template v-else-if="item.ref === 'pixel-pet'">
      <div class="pixel-pet" :class="{ happy: Number(data.energy || 0) % 2 === 1 }" aria-hidden="true"><i v-for="(pixel, index) in petPixels" :key="index" :class="`p${pixel}`" /></div>
      <span>{{ Number(data.energy || 0) ? `${data.energy} sparks collected` : 'A tiny friend. A quiet moment.' }}</span>
      <button type="button" @click="patch({ energy: Math.min(999, Number(data.energy || 0) + 1) })">Give a spark</button>
    </template>
    <template v-else-if="item.ref === 'focus'">
      <strong class="widget-number">{{ remaining }}</strong><span>Local focus timer · no app blocking</span>
      <button type="button" @click="patch({ endsAt: Number(data.endsAt || 0) > now ? 0 : Date.now() + 25 * 60 * 1000 })">{{ Number(data.endsAt || 0) > now ? 'Stop focus' : 'Start 25 minutes' }}</button>
    </template>
    <template v-else-if="item.ref === 'checklist'">
      <label :for="`task-${item.id}`">One small mission</label><input :id="`task-${item.id}`" :value="String(data.text || '')" maxlength="160" placeholder="What matters today?" @input="patch({ text: ($event.target as HTMLInputElement).value })" />
      <label class="mission-done"><input type="checkbox" :checked="data.done === true" @change="patch({ done: ($event.target as HTMLInputElement).checked })" /> Mission complete</label>
    </template>
    <template v-else><p>Jump straight into your app. No background access or simulated live data.</p><button v-if="appId" type="button" @click="$emit('open', appId)">Open app →</button></template>
  </div>
</template>
<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import type { HomeItem } from '@/utils/homeWorkspaces'
const props = defineProps<{ item: HomeItem; appId?: string }>()
const emit = defineEmits<{ update: [data: Record<string, string | number | boolean>]; open: [id: string] }>()
const data = computed(() => props.item.data || {})
const now = ref(Date.now())
let interval: ReturnType<typeof setInterval> | undefined
onMounted(() => { if (!['clock', 'focus'].includes(props.item.ref)) return; interval = setInterval(() => { now.value = Date.now() }, 1000) })
onUnmounted(() => clearInterval(interval))
const clock = computed(() => new Date(now.value).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }))
const date = computed(() => new Date(now.value).toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' }))
const remaining = computed(() => { const s = Math.max(0, Math.ceil((Number(data.value.endsAt || 0) - now.value) / 1000)); return Number(data.value.endsAt || 0) === 0 ? '25:00' : `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}` })
function patch(value: Record<string, string | number | boolean>) { emit('update', { ...data.value, ...value }) }
// Original 10×8 neon familiar sprite. Palette stays scoped to the selected Home identity.
const petPixels = '0100000010/0110000110/0111111110/0112112110/0111111110/0011331100/0001111000/0010000100'.replaceAll('/', '').split('')
</script>
<style scoped>
.local-widget { display: flex; flex-direction: column; gap: 10px; min-width: 0; font-size: 12px; }
.widget-clock,.widget-number { font-size: clamp(28px, 9vw, 42px); font-variant-numeric: tabular-nums; letter-spacing: -.05em; line-height: 1.1; }
.local-widget textarea,.local-widget input:not([type=checkbox]) { width: 100%; box-sizing: border-box; min-width: 0; border: 1px solid var(--hw-line); background: var(--hw-bg); color: inherit; border-radius: 8px; padding: 10px; font: inherit; }
.local-widget textarea { resize: vertical; min-height: 92px; }
.local-widget button { min-height: 44px; border: 1px solid var(--hw-line); border-radius: 10px; color: inherit; background: var(--hw-bg); padding: 8px 12px; font: inherit; cursor: pointer; }
.widget-actions { display: flex; gap: 8px; }.widget-actions button { flex: 1; font-size: 22px; }
.mission-done { display: flex; align-items: center; gap: 10px; min-height: 44px; }.mission-done input { width: 20px; height: 20px; accent-color: var(--hw-accent); }
.pixel-pet { width: 80px; height: 64px; display: grid; grid-template-columns: repeat(10, 8px); align-self: center; transition: transform 180ms ease; }.pixel-pet.happy { transform: translateY(-4px) rotate(-4deg); }.pixel-pet i { width: 8px; height: 8px; }.p1 { background: var(--hw-accent); }.p2 { background: var(--hw-bg); }.p3 { background: #f7a9df; }
.local-widget p { margin: 0; line-height: 1.5; }
@media (prefers-reduced-motion: reduce) { .pixel-pet { transition: none; } }
</style>
