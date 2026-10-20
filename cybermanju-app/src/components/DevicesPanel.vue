<!-- CyberManju OS — DevicesPanel (src/components/DevicesPanel.vue). -->
<!-- Role: Devices window — live hardware telemetry, pairing, capture, system. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/components/ui/*, @/composables/useSystemHardware, @/stores/app, @/utils/deviceStats. -->
<!-- Contracts: never-throw rendering — the wasm `top` twin answers -->
<!--   tasks/counts/uptime ONLY, so every numeric readout funnels through -->
<!--   deviceStats guards (a bare `.toFixed()` here crashed the window). -->
<template>
  <div class="dv">
    <header class="dv-hero">
      <div class="dv-brand">
        <span class="dv-brand-mark"><AppIcon name="solar:devices-bold" :size="22" /></span>
        <div class="dv-brand-text">
          <h2 class="dv-title">Devices</h2>
          <div class="dv-pills">
            <UiBadge :tone="hw.online.value ? 'success' : 'danger'" dot :pulse="hw.online.value" :label="netLine" />
            <UiBadge tone="info" icon="solar:battery-charge-bold" :label="powerLine" />
            <UiBadge tone="accent" icon="solar:usb-bold" :label="pluggedLabel" />
          </div>
        </div>
      </div>
      <div class="dv-top-actions">
        <UiButton size="sm" variant="ghost" icon="solar:map-point-bold" @click="locate">Locate</UiButton>
        <UiButton size="sm" icon="solar:refresh-bold" :loading="hw.refreshing.value" @click="hw.refreshDevices()">Scan</UiButton>
      </div>
    </header>

    <div v-if="hw.lastError.value" class="dv-error" role="alert">{{ hw.lastError.value }}</div>

    <UiSegmented :model-value="tab" :options="tabOptions" aria-label="Devices sections" class="dv-tabs" @update:model-value="tab = String($event)" />

    <!-- ── overview: live tiles + presence + screens ── -->
    <section v-if="tab === 'overview'" aria-label="Overview">
      <div class="dv-tiles">
        <UiCard pad="sm" class="dv-tile">
          <div class="dv-tile-top"><AppIcon name="solar:cpu-bold" :size="15" class="dv-tile-icon" /><span class="dv-tile-label">CPU</span></div>
          <div class="dv-tile-value">{{ formatCpuPercent(store.osTop) }}</div>
          <div class="dv-tile-sub">{{ cpuSub }}</div>
        </UiCard>
        <UiCard pad="sm" class="dv-tile">
          <div class="dv-tile-top"><AppIcon name="solar:layers-bold" :size="15" class="dv-tile-icon" /><span class="dv-tile-label">JS HEAP</span></div>
          <div class="dv-tile-value">{{ heapTile.value }}</div>
          <div class="dv-tile-sub">{{ heapTile.sub }}</div>
        </UiCard>
        <UiCard pad="sm" class="dv-tile">
          <div class="dv-tile-top"><AppIcon name="solar:battery-charge-bold" :size="15" class="dv-tile-icon" /><span class="dv-tile-label">BATTERY</span></div>
          <div class="dv-tile-value">{{ batteryValue }}</div>
          <div class="dv-tile-sub">{{ batterySub }}</div>
          <UiProgress v-if="hw.battery.isSupported.value" :value="batteryPct" :tone="batteryTone" :label="`Battery ${batteryPct} percent`" />
        </UiCard>
        <UiCard pad="sm" class="dv-tile">
          <div class="dv-tile-top"><AppIcon name="solar:global-bold" :size="15" class="dv-tile-icon" /><span class="dv-tile-label">NETWORK</span></div>
          <div class="dv-tile-value">{{ netValue }}</div>
          <div class="dv-tile-sub">{{ netSub }}</div>
        </UiCard>
        <UiCard pad="sm" class="dv-tile">
          <div class="dv-tile-top"><AppIcon name="solar:folder-bold" :size="15" class="dv-tile-icon" /><span class="dv-tile-label">STORAGE</span></div>
          <div class="dv-tile-value">{{ storageValue }}</div>
          <div class="dv-tile-sub">{{ storageSub }}</div>
          <UiProgress v-if="storageStat.percent != null" :value="storageStat.percent" :tone="storageTone" :label="`Storage ${storageStat.percent} percent used`" />
        </UiCard>
        <UiCard pad="sm" class="dv-tile">
          <div class="dv-tile-top"><AppIcon name="solar:map-point-bold" :size="15" class="dv-tile-icon" /><span class="dv-tile-label">LOCATION</span></div>
          <div class="dv-tile-value dv-tile-value--sm">{{ geoShort }}</div>
          <div class="dv-tile-sub">{{ geoTileSub }}</div>
        </UiCard>
      </div>
      <div class="dv-grid">
        <UiCard title="Presence" icon="solar:eye-bold" meta="multi-tab">
          <p class="dv-card-line">idle: {{ hw.idle.idle.value ? 'away (5min)' : 'active' }} · tab: {{ hw.visibility.value }} · focused: {{ hw.focused.value ? 'yes' : 'no' }}</p>
          <div class="dv-pair">
            <UiButton size="sm" :disabled="!hw.support.value.broadcast" @click="hw.pingTabs(`ping ${Date.now()}`)">Ping tabs</UiButton>
          </div>
          <p class="dv-muted">last bus: {{ busMsg || '—' }} · FileManager refreshes on broadcast.</p>
        </UiCard>
        <UiCard title="Screens" icon="solar:monitor-bold" :meta="hw.screens.value.length ? `(${hw.screens.value.length})` : ''">
          <p v-if="hw.screens.value.length" class="dv-card-line">{{ screenLine }}</p>
          <p v-else class="dv-muted">{{ hw.support.value.multiScreen ? 'No extended screens reported.' : 'Multi-screen API not exposed here.' }}</p>
          <div class="dv-pair">
            <UiButton v-if="hw.support.value.multiScreen" size="sm" @click="hw.refreshScreens()">Detect</UiButton>
            <UiButton v-if="hw.support.value.appBadge" size="sm" @click="badgeDemo">Badge ①</UiButton>
            <UiButton v-if="hw.support.value.appBadge" size="sm" variant="ghost" @click="hw.clearBadge()">Clear</UiButton>
          </div>
        </UiCard>
      </div>
    </section>

    <!-- ── devices: plugged hardware + pairing ── -->
    <section v-else-if="tab === 'devices'" aria-label="Plugged devices">
      <UiCard title="Plugged in" icon="solar:usb-bold" :meta="`${hw.pluggedDevices.value.length} visible`">
        <template #actions>
          <span class="dv-counts">
            <span v-for="(n, bus) in hw.deviceCountByBus.value" :key="bus" class="dv-chip">{{ bus }}:{{ n }}</span>
          </span>
        </template>
        <UiEmpty
          v-if="hw.pluggedDevices.value.length === 0"
          size="sm"
          icon="solar:plug-circle-bold"
          title="No devices visible"
          description="Grant camera/mic once, or pair USB / HID / Serial / Bluetooth below. Gamepads appear automatically."
        />
        <div v-else class="dv-list">
          <div v-for="d in hw.pluggedDevices.value" :key="d.id" class="dv-row">
            <span class="dv-bus" :data-bus="d.bus">{{ d.bus.toUpperCase() }}</span>
            <div class="dv-info">
              <span class="dv-label">{{ d.label }}</span>
              <span class="dv-kind">{{ d.kind }}</span>
            </div>
            <span class="dv-dot" :class="{ on: d.connected }" />
          </div>
        </div>
        <div class="dv-pair">
          <UiButton v-if="hw.support.value.usb" size="sm" icon="solar:usb-bold" @click="hw.requestUsb()">Pair USB</UiButton>
          <UiButton v-if="hw.support.value.hid" size="sm" icon="solar:gamepad-bold" @click="hw.requestHid()">Pair HID</UiButton>
          <UiButton v-if="hw.support.value.serial" size="sm" icon="solar:plug-circle-bold" @click="hw.requestSerial()">Pair Serial</UiButton>
          <UiButton v-if="hw.support.value.bluetooth" size="sm" icon="solar:bluetooth-bold" @click="hw.requestBluetooth()">Pair BLE</UiButton>
        </div>
        <p class="dv-hint">USB / HID / Serial / Bluetooth need a user gesture + HTTPS or localhost. Media labels stay generic until camera/mic permission is granted. Plugs refresh live via devicechange listeners.</p>
      </UiCard>
    </section>

    <!-- ── identity: device registry (§2B.1) + LAN peers (§2B.2) ── -->
    <section v-else-if="tab === 'identity'" aria-label="Identity">
      <div class="dv-stack">
        <UiCard title="This device" icon="solar:key-bold" meta="ML-DSA identity">
          <div class="dv-pair">
            <UiInput v-model="deviceName" placeholder="My laptop" aria-label="Device name" />
            <UiButton size="sm" :disabled="!deviceName.trim()" @click="enrollThis">Enroll this device</UiButton>
          </div>
          <p class="dv-muted">Enrollment generates a fresh ML-DSA-65 keypair; the private half stays sealed on this device, the public half registers here.</p>
        </UiCard>
        <UiCard title="Registry" icon="solar:users-group-rounded-bold" :meta="`${store.devices.length} known`">
          <div v-if="!store.devices.length" class="dv-muted">No devices enrolled yet.</div>
          <div v-for="d in store.devices" :key="d.id" class="dv-pair dv-row">
            <span class="dv-line">{{ d.name }} <span class="dv-muted">{{ d.id }}{{ d.revoked ? ' · REVOKED' : '' }}</span></span>
            <UiButton size="sm" variant="ghost" :disabled="d.revoked" @click="store.revokeDevice(d.id)">Revoke</UiButton>
            <UiButton size="sm" variant="ghost" @click="store.removeDevice(d.id)">Remove</UiButton>
          </div>
          <div class="dv-pair">
            <UiButton size="sm" variant="ghost" @click="store.fetchDevices()">Refresh</UiButton>
          </div>
        </UiCard>
        <UiCard title="LAN peers" icon="solar:global-bold" meta="mDNS scan">
          <div class="dv-pair">
            <UiButton size="sm" @click="store.fetchLanPeers()">Scan LAN</UiButton>
          </div>
          <div v-if="!store.lanPeers.length" class="dv-muted">No peers answering `_cybermanju._tcp.local` right now — same Wi-Fi, no server needed.</div>
          <div v-for="p in store.lanPeers" :key="p.deviceId" class="dv-pair dv-row">
            <span class="dv-line">{{ p.name || p.deviceId }} <span class="dv-muted">{{ p.host }}:{{ p.port }}</span></span>
            <UiButton size="sm" :disabled="!canSyncPeer(p)" :title="peerSyncTitle(p)" @click="store.syncWithPeer(p.deviceId)">Sync</UiButton>
          </div>
          <p v-if="store.lanSyncMsg" class="dv-line">{{ store.lanSyncMsg }}</p>
          <p class="dv-muted">Direct sync accepts only a signed ML-DSA envelope from an enrolled, non-revoked device. Unsigned, revoked, unknown, or wrong-owner peers fail closed and never merge.</p>
        </UiCard>
        <UiCard title="Presence (no-mDNS peers)" icon="solar:radar-bold" meta="announce · known">
          <div class="dv-pair">
            <UiButton size="sm" @click="store.fetchKnownPeers()">Refresh known</UiButton>
            <UiButton size="sm" variant="ghost" :disabled="!canAnnounce" @click="announceHere">Announce here</UiButton>
          </div>
          <div v-if="!store.knownPeers.length" class="dv-muted">No heartbeats yet — peers `POST /api/peers/announce` once per launch (5 min TTL); this is what mobile/docker read where multicast cannot go.</div>
          <div v-for="p in store.knownPeers" :key="p.deviceId" class="dv-pair dv-row">
            <span class="dv-line">{{ p.deviceId }} <span class="dv-muted">{{ p.host }}:{{ p.port }}</span></span>
            <UiButton size="sm" @click="store.syncWithPeer(p.deviceId)">Sync</UiButton>
          </div>
          <p v-if="store.presenceMsg" class="dv-line">{{ store.presenceMsg }}</p>
        </UiCard>
        <UiCard title="Pairing codes" icon="solar:key-bold" meta="wormhole-style · no server">
          <div class="dv-pair">
            <UiButton size="sm" @click="makeCode">Make code</UiButton>
            <UiInput v-model="claimInput" placeholder="482-oak-river" aria-label="Pairing code to claim" />
            <UiButton size="sm" variant="ghost" :disabled="!claimInput.trim()" @click="claimCode">Claim</UiButton>
          </div>
          <p v-if="store.pairingMsg" class="dv-line">{{ store.pairingMsg }}</p>
          <p v-else class="dv-muted">Read the code aloud — the other side claims <span class="dv-line">pair:&lt;code&gt;</span> from any relay. Same grammar as `pair make|drop|claim` in cybsh.</p>
        </UiCard>
        <UiCard title="Swarm demo" icon="solar:layers-bold" meta="RS + fountain · lossy-proof">
          <div class="dv-pair">
            <UiInput v-model="swarmText" placeholder="Some bytes to spray…" aria-label="Swarm demo text" />
            <UiButton size="sm" :disabled="!swarmText" @click="runSwarmDemo">Spray + collect</UiButton>
          </div>
          <p v-if="swarmResult" class="dv-line">{{ swarmResult }}</p>
          <p v-else class="dv-muted">Encodes via IPC twins, sprays symbols through the relay mailbox (dropping every third on purpose), decodes from the survivors. Same math as `swarm plan` in cybsh.</p>
        </UiCard>
        <UiCard title="WAN dial" icon="solar:rocket-bold" meta="http live · relay live">
          <div class="dv-pair">
            <UiInput v-model="wanInput" placeholder="phone-7@192.168.1.60:3456" aria-label="WAN endpoint" />
            <UiButton size="sm" :disabled="!wanInput.trim()" @click="planWan">Plan dial</UiButton>
            <UiButton size="sm" variant="ghost" :disabled="!wanInput.trim()" @click="probeWan">Probe</UiButton>
          </div>
          <div class="dv-pair">
            <UiInput v-model="relayUrl" placeholder="https://relay.example.com (optional)" aria-label="Relay URL" />
          </div>
          <p v-if="wanResult" class="dv-line">{{ wanResult }}</p>
          <p v-else class="dv-muted">Direct HTTP dial is live (readyz probe + op-log pull/push, 15 s abort); relay tickets rendezvous via the relay URL above (outbound HTTPS drop/poll, no inbound port). Same grammar as `wan plan|dial` in cybsh.</p>
        </UiCard>
      </div>
    </section>

    <!-- ── media: capture + local files + share ── -->
    <section v-else-if="tab === 'media'" aria-label="Media">
      <div class="dv-stack">
        <UiCard title="Capture" icon="solar:videocamera-bold" meta="face input · screen · voice">
          <div class="dv-pair">
            <UiButton size="sm" :disabled="!hw.support.value.camera" @click="toggleCam">{{ camOn ? 'Stop camera' : 'Start camera' }}</UiButton>
            <UiButton size="sm" :disabled="!hw.support.value.screenShare" @click="toggleShare">{{ shareOn ? 'Stop share' : 'Share screen' }}</UiButton>
            <UiButton size="sm" :disabled="!hw.support.value.speechSynthesis" @click="hw.speak('CyberManju OS ready')">Speak</UiButton>
            <select v-if="hw.support.value.speechRecognition" class="dv-select" :value="hw.speechLang.value" aria-label="Voice language" title="Voice language: English (EN) or Portuguese (PT-BR)" @change="hw.setSpeechLang(($event.target as HTMLSelectElement).value as 'en-US' | 'pt-BR')">
              <option value="en-US">EN</option>
              <option value="pt-BR">PT</option>
            </select>
            <UiButton v-if="hw.support.value.speechRecognition" size="sm" @click="toggleListen">{{ listening ? 'Stop listening' : 'Voice command' }}</UiButton>
          </div>
          <div v-if="camOn || shareOn" class="dv-vids">
            <video v-show="camOn" ref="camVideoRef" autoplay playsinline muted class="dv-vid" />
            <video v-show="shareOn" ref="shareVideoRef" autoplay playsinline muted class="dv-vid" />
          </div>
          <p v-if="transcript" class="dv-line">heard: “{{ transcript }}”</p>
          <p v-else class="dv-muted">camera feeds face grouping · screen share demos the vault · voice fills cybsh.</p>
        </UiCard>
        <UiCard title="Local files" icon="solar:folder-bold" meta="no Tauri needed">
          <div class="dv-pair">
            <UiButton size="sm" :disabled="!hw.support.value.fileSystemAccess" @click="openLocal">Open file</UiButton>
            <UiButton size="sm" :disabled="!hw.support.value.fileSystemAccess" @click="saveLocal">Save note</UiButton>
            <UiButton size="sm" @click="hw.pickFiles()">Picker ({{ pickedCount }})</UiButton>
            <UiButton size="sm" @click="copyRich">Rich copy</UiButton>
          </div>
          <p v-if="localName" class="dv-line">{{ localName }} — {{ localPreview }}</p>
          <p v-else class="dv-muted">Open reads text into the OS · Save writes back to disk · Picker uses a classic dialog fallback.</p>
        </UiCard>
        <UiCard title="Share & haptics" icon="solar:share-bold" meta="system integrations">
          <div class="dv-pair">
            <UiButton size="sm" :disabled="!hw.support.value.share" @click="shareNow">Share OS</UiButton>
            <UiButton size="sm" :disabled="!hw.support.value.vibrate" @click="buzz">Vibrate</UiButton>
            <UiButton v-if="hw.support.value.eyeDropper" size="sm" @click="pick">Pick color{{ picked ? `: ${picked}` : '' }}</UiButton>
          </div>
          <p class="dv-muted">share: {{ hw.support.value.share ? 'yes' : 'no' }} · vibrate: {{ hw.support.value.vibrate ? 'yes' : 'no' }}</p>
        </UiCard>
      </div>
    </section>

    <!-- ── system: telemetry, storage, identity, notifications ── -->
    <section v-else aria-label="System">
      <div class="dv-stack">
        <UiCard title="System load" icon="solar:chart-bold" meta="live">
          <p class="dv-card-line">{{ nativeStatsLine }}</p>
          <p class="dv-muted">WebView JavaScript heap: {{ memLine }}</p>
          <p class="dv-muted">{{ motionLine }}</p>
          <p class="dv-muted">{{ localeLine }} · {{ hw.windowSize.width.value }}×{{ hw.windowSize.height.value }} · {{ hw.preferredScheme.value }}</p>
        </UiCard>
        <div class="dv-grid">
          <UiCard title="Battery" icon="solar:battery-charge-bold">
            <p v-if="hw.battery.isSupported.value" class="dv-card-line">{{ batteryLine }}</p>
            <p v-else class="dv-muted">Not exposed by this browser.</p>
          </UiCard>
          <UiCard title="Network" icon="solar:global-bold">
            <p class="dv-card-line">{{ netDetail }}</p>
            <p class="dv-muted">online: {{ hw.online.value ? 'yes' : 'no' }}</p>
          </UiCard>
          <UiCard title="Location" icon="solar:map-point-bold">
            <p v-if="hasGeo" class="dv-card-line">lat {{ geoLat }}, lon {{ geoLon }} (±{{ geoAcc }}m)</p>
            <p v-else class="dv-muted">Permission: {{ hw.geoPermission.value ?? 'unknown' }}</p>
            <div class="dv-pair">
              <UiButton size="sm" @click="locate">Locate</UiButton>
              <UiButton size="sm" variant="ghost" @click="copyGeo" :disabled="!hasGeo">Copy</UiButton>
            </div>
          </UiCard>
          <UiCard title="Screen" icon="solar:monitor-smartphone-bold">
            <p class="dv-card-line">{{ orientationLine }}</p>
            <div class="dv-pair">
              <UiButton size="sm" @click="hw.fullscreen.toggle()">{{ hw.fullscreen.isFullscreen.value ? 'Exit fullscreen' : 'Fullscreen' }}</UiButton>
              <UiButton v-if="hw.support.value.wakeLock" size="sm" :variant="hw.wakeLock.isActive.value ? 'primary' : 'ghost'" @click="toggleWake">
                Wake {{ hw.wakeLock.isActive.value ? 'ON' : 'OFF' }}
              </UiButton>
            </div>
          </UiCard>
          <UiCard title="WebView storage quota" icon="solar:server-bold">
            <p v-if="storageMsg" class="dv-card-line">{{ storageMsg }}</p>
            <p v-else class="dv-muted">Loading…</p>
            <p v-if="store.osDf" class="dv-muted">CyberManju volume: {{ formatBytesDec(store.osDf.usedBytes) }} / {{ formatBytesDec(store.osDf.totalBytes) }} · {{ store.osDf.diskCount }} attached disk(s)</p>
            <p class="dv-muted">WebView quota is for app data, not the phone's total storage; neither reading needs a permission prompt.</p>
            <div class="dv-pair">
              <UiButton size="sm" @click="loadStorage">Refresh</UiButton>
              <UiButton size="sm" variant="ghost" @click="persist">Persist</UiButton>
            </div>
          </UiCard>
          <UiCard title="Identity & scan" icon="solar:lock-password-bold" meta="passkeys · NFC · contacts">
            <div class="dv-pair">
              <UiButton size="sm" :disabled="!hw.support.value.webAuthn" @click="passkeyInfo">Passkeys</UiButton>
              <UiButton v-if="hw.support.value.nfc" size="sm" @click="hw.requestNfcScan()">NFC scan</UiButton>
              <UiButton v-if="hw.support.value.contacts" size="sm" @click="pickContacts">Contacts</UiButton>
            </div>
            <p v-if="identityMsg" class="dv-line">{{ identityMsg }}</p>
            <p v-else-if="hw.nfcMessage.value" class="dv-line">{{ hw.nfcMessage.value }}</p>
            <p v-else class="dv-muted">WebAuthn: {{ hw.support.value.webAuthn ? 'ready' : 'n/a' }} · NFC: {{ hw.support.value.nfc ? 'ready' : 'n/a' }} · contacts: {{ hw.support.value.contacts ? 'ready' : 'n/a' }} · barcode: {{ hw.support.value.barcode ? 'ready' : 'n/a' }} · face: {{ hw.support.value.faceDetector ? 'ready' : 'n/a' }}</p>
          </UiCard>
          <UiCard title="Notifications" icon="solar:bell-bold" :meta="`permission: ${hw.notifyPermission.value ?? 'unknown'}`">
            <div class="dv-pair">
              <UiButton size="sm" :disabled="!hw.support.value.notifications" @click="askNotify">Enable</UiButton>
              <UiButton size="sm" :disabled="!canNotify" @click="testNotify">Test</UiButton>
            </div>
          </UiCard>
        </div>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useSystemHardware, getStorageEstimate, persistStorage } from '@/composables/useSystemHardware'
import { useAppStore } from '@/stores/app'
import {
  formatBytesDec,
  formatCoord,
  formatCpuPercent,
  formatHeapLine,
  formatTopLine,
  isFiniteNum,
} from '@/utils/deviceStats'
import type { TopSnapshot } from '@/utils/deviceStats'
import type { LanPeer } from '@/types'
import { isPeerSyncable, peerBlockReason } from '@/utils/lanSync'
import { isRelayAddr, parseWanEndpoint, relayDialSteps, wanDialPlan, wanDialSteps, wanHttpBase, wanHttpProbe, wanWayOut } from '@/utils/wanDial'

const hw = useSystemHardware()
const store = useAppStore()
const storageMsg = ref('')
const storageStat = ref<{ usage: number | null; quota: number | null; percent: number | null; persisted: boolean }>({
  usage: null, quota: null, percent: null, persisted: false,
})
const picked = ref('')
const identityMsg = ref('')
const deviceName = ref('')

async function enrollThis() {
  const name = deviceName.value.trim()
  if (!name) return
  const row = await store.registerThisDevice(name)
  if (row) deviceName.value = ''
}

/** Direct-sync gate: enrolled + non-revoked registry rows only (fail-closed). */
function canSyncPeer(p: LanPeer): boolean {
  return isPeerSyncable(
    { deviceId: p.deviceId, name: p.name, host: p.host, port: p.port },
    store.devices.filter((d) => !d.revoked).map((d) => d.id),
  )
}

function peerSyncTitle(p: LanPeer): string {
  const reason = peerBlockReason(
    { deviceId: p.deviceId, name: p.name, host: p.host, port: p.port },
    store.devices.filter((d) => !d.revoked).map((d) => d.id),
  )
  return reason || 'Sync op-log with this peer now'
}

const wanInput = ref('')
const wanResult = ref('')
const relayUrl = ref(
  typeof localStorage === 'undefined' ? '' : (localStorage.getItem('cybermanju.relayUrl') ?? ''),
)

watch(relayUrl, (v) => {
  try {
    if (typeof localStorage !== 'undefined') localStorage.setItem('cybermanju.relayUrl', v)
  } catch {
    // Private mode — the relay URL simply does not persist.
  }
})

const claimInput = ref('')
const swarmText = ref('')
const swarmResult = ref('')

async function makeCode() {
  await store.makePairingCode()
}

async function claimCode() {
  const code = claimInput.value.trim()
  if (!code) return
  await store.claimPairingCode(code)
  if (store.pairingMsg) claimInput.value = ''
}

/** Swarm demo: encode → spray (dropping every third symbol on purpose) →
 * collect → decode. Proves lossy-path reconstruction end to end. */
async function runSwarmDemo() {
  const text = swarmText.value
  if (!text) return
  try {
    swarmResult.value = 'encoding…'
    const bytes = new TextEncoder().encode(text)
    let bin = ''
    for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i])
    const b64 = btoa(bin)
    const symbols = (await store.swarmEncode(b64, 4, 2)) as Array<Record<string, unknown>> | null
    if (!symbols?.length) {
      swarmResult.value = 'encode needs the dashboard (swarm_encode IPC twin)'
      return
    }
    const me = store.devices[0]?.id ?? 'here'
    const kept = symbols.filter((_, i) => i % 3 !== 0)
    for (const s of kept) {
      await store.relayDropMsg(me, 'swarm-symbol', s, me)
    }
    const messages = await store.relayPollMsg(me)
    const got = messages
      .map((m) => m.payload)
      .filter((p) => p && typeof p === 'object')
    const back = await store.swarmDecode(got.length >= 4 ? got : kept)
    if (typeof back !== 'string') {
      swarmResult.value = 'decode needs the dashboard (swarm_decode IPC twin)'
      return
    }
    const out = atob(back)
    swarmResult.value =
      out === text
        ? `reconstructed byte-identically from ${got.length} of ${symbols.length} sprayed symbols ✓`
        : 'mismatch — the round trip did not survive (see relay log)'
  } catch (e) {
    swarmResult.value = e instanceof Error ? e.message : String(e)
  }
}

/** This device's first enrolled id (presence announce defaults to it). */
const canAnnounce = computed(() => store.devices.length > 0)

async function announceHere() {
  const id = store.devices[0]?.id ?? ''
  if (!id) return
  const host = typeof location !== 'undefined' ? location.hostname : '127.0.0.1'
  await store.announcePresence(id, host || '127.0.0.1', 3456)
}

/** WAN dial seam (§2B.5): live HTTP plan, live relay plan, honest fallback. */
function planWan() {
  const raw = wanInput.value.trim()
  if (!raw) {
    wanResult.value = 'usage: <device-id>@<addr> (e.g. phone-7@192.168.1.60:3456)'
    return
  }
  try {
    const endpoint = parseWanEndpoint(raw)
    const steps = isRelayAddr(endpoint.addr)
      ? relayDialSteps(endpoint, relayUrl.value, 0)
      : wanDialSteps(endpoint, 0)
    try {
      const plan = wanDialPlan(endpoint, 15, relayUrl.value)
      wanResult.value = [plan, ...steps].join('\n')
    } catch (e) {
      const plan = e instanceof Error ? e.message : String(e)
      wanResult.value = [plan, ...steps, ...wanWayOut(endpoint)].join('\n')
    }
  } catch (e) {
    wanResult.value = e instanceof Error ? e.message : String(e)
  }
}

/** Live HTTP probe: GET <base>/api/readyz with a 15 s abort (never hangs). */
async function probeWan() {
  const raw = wanInput.value.trim()
  if (!raw) {
    wanResult.value = 'usage: <device-id>@<addr> (e.g. phone-7@192.168.1.60:3456)'
    return
  }
  try {
    const endpoint = parseWanEndpoint(raw)
    wanResult.value = `probing ${wanHttpBase(endpoint)}/api/readyz …`
    await wanHttpProbe(wanHttpBase(endpoint))
    wanResult.value = [`reachable: ${wanHttpBase(endpoint)}`, ...wanDialSteps(endpoint, 0)].join('\n')
  } catch (e) {
    wanResult.value = e instanceof Error ? e.message : String(e)
  }
}
const camVideoRef = ref<HTMLVideoElement | null>(null)
const shareVideoRef = ref<HTMLVideoElement | null>(null)
const tab = ref('overview')
const tabOptions = [
  { label: 'Overview', value: 'overview', icon: 'solar:devices-bold' },
  { label: 'Devices', value: 'devices', icon: 'solar:usb-bold' },
  { label: 'Identity', value: 'identity', icon: 'solar:key-bold' },
  { label: 'Media', value: 'media', icon: 'solar:videocamera-bold' },
  { label: 'System', value: 'system', icon: 'solar:chart-bold' },
]

const netLine = computed(() => hw.online.value ? `online${hw.network.effectiveType.value ? ` · ${hw.network.effectiveType.value}` : ''}` : 'offline')
const netDetail = computed(() => {
  const parts = [`type: ${hw.network.type.value}`, `effective: ${hw.network.effectiveType.value || '—'}`]
  if (hw.network.downlink.value != null) parts.push(`${hw.network.downlink.value} Mbps`)
  if (hw.network.rtt.value != null) parts.push(`${hw.network.rtt.value}ms rtt`)
  if (hw.network.saveData.value) parts.push('save-data')
  return parts.join(' · ')
})
const netValue = computed(() => hw.online.value ? (hw.network.effectiveType.value || hw.network.type.value || 'online') : 'offline')
const netSub = computed(() => {
  if (!hw.online.value) return 'no connection'
  const parts: string[] = []
  if (hw.network.downlink.value != null) parts.push(`${hw.network.downlink.value} Mbps`)
  if (hw.network.rtt.value != null) parts.push(`${hw.network.rtt.value}ms`)
  if (hw.network.saveData.value) parts.push('save-data')
  return parts.length ? parts.join(' · ') : 'link up'
})
const pluggedLabel = computed(() => {
  const n = hw.pluggedDevices.value.length
  return `${n} plugged`
})
const batteryPct = computed(() => Math.round((hw.battery.level.value ?? 0) * 100))
const batteryValue = computed(() => hw.battery.isSupported.value ? `${batteryPct.value}%` : 'n/a')
const batterySub = computed(() => {
  if (!hw.battery.isSupported.value) return 'not exposed'
  return hw.battery.charging.value ? 'charging' : 'on battery'
})
const batteryTone = computed(() => {
  if (!hw.battery.isSupported.value || hw.battery.charging.value) return 'success' as const
  if (batteryPct.value < 20) return 'danger' as const
  if (batteryPct.value < 50) return 'warning' as const
  return 'success' as const
})
const powerLine = computed(() => {
  if (!hw.battery.isSupported.value) return 'power: n/a'
  return hw.battery.charging.value ? `charging ${Math.round((hw.battery.level.value ?? 0) * 100)}%` : `${Math.round((hw.battery.level.value ?? 0) * 100)}%${hw.battery.dischargingTime.value ? ` · ${Math.round(hw.battery.dischargingTime.value / 60)}min left` : ''}`
})
const batteryLine = computed(() => `${Math.round((hw.battery.level.value ?? 0) * 100)}% · ${hw.battery.charging.value ? 'charging' : 'on battery'}`)
const orientationLine = computed(() => {
  const raw = hw.orientation.orientation.value as unknown as string | { type?: string } | null | undefined
  const oType = typeof raw === 'string' ? raw : (raw?.type ?? (screen.orientation as ScreenOrientation | undefined)?.type ?? 'unknown')
  return `${oType} · ${window.innerWidth}×${window.innerHeight}`
})
const hasGeo = computed(() => isFiniteNum(hw.geo.coords.value?.latitude) && isFiniteNum(hw.geo.coords.value?.longitude))
const geoLat = computed(() => formatCoord(hw.geo.coords.value?.latitude))
const geoLon = computed(() => formatCoord(hw.geo.coords.value?.longitude))
const geoAcc = computed(() => Math.round(hw.geo.coords.value?.accuracy ?? 0))
const geoShort = computed(() => hasGeo.value ? `${formatCoord(hw.geo.coords.value?.latitude, 4)}, ${formatCoord(hw.geo.coords.value?.longitude, 4)}` : 'No fix')
const geoTileSub = computed(() => hasGeo.value ? `±${geoAcc.value} m accuracy` : `permission: ${hw.geoPermission.value ?? 'unknown'}`)
const cpuSub = computed(() => {
  const top = store.osTop as TopSnapshot | null
  if (!top || typeof top !== 'object') return 'no host telemetry'
  if (top.load?.source === 'proc' && isFiniteNum(top.load?.load1)) return `load ${top.load.load1.toFixed(2)}`
  return 'app process'
})
const heapTile = computed(() => {
  const m = hw.memory.memory.value as unknown as { usedJSHeapSize?: number | null; jsHeapSizeLimit?: number | null } | undefined
  const used = m?.usedJSHeapSize
  if (!isFiniteNum(used) || used <= 0) {
    return hw.support.value.memory
      ? { value: '…', sub: 'reading heap' }
      : { value: 'n/a', sub: 'not exposed' }
  }
  const lim = m?.jsHeapSizeLimit
  return {
    value: `${(used / 1048576).toFixed(0)} MB`,
    sub: isFiniteNum(lim) && lim > 0 ? `/ ${(lim / 1048576).toFixed(0)} MB limit` : 'live heap',
  }
})
const storageValue = computed(() => {
  const s = storageStat.value
  if (s.percent != null) return `${s.percent}%`
  if (s.quota != null) return formatBytesDec(s.usage)
  return s.persisted ? 'stored' : '—'
})
const storageSub = computed(() => {
  const s = storageStat.value
  if (s.quota != null) return `${formatBytesDec(s.usage)} of ${formatBytesDec(s.quota)}`
  return `persisted: ${s.persisted ? 'yes' : 'no'}`
})
const storageTone = computed(() => {
  const p = storageStat.value.percent ?? 0
  if (p > 90) return 'danger' as const
  if (p > 70) return 'warning' as const
  return 'accent' as const
})

// capture state
const camOn = computed(() => !!hw.userMedia.stream.value)
const shareOn = computed(() => !!hw.displayMedia.stream.value)
const listening = computed(() => hw.speechRecognition.isListening.value)
const transcript = computed(() => hw.speechRecognition.result.value)
const busMsg = computed(() => String(hw.bus.data.value ?? ''))
const pickedCount = computed(() => hw.fileDialog.files.value?.length ?? 0)
const localName = computed(() => hw.fsAccess.fileName.value ?? '')
const localPreview = computed(() => {
  const d = hw.fsAccess.data.value
  if (typeof d !== 'string') return ''
  return d.length > 90 ? `${d.slice(0, 90)}… (${d.length} chars)` : d || '(empty)'
})
const memLine = computed(() => formatHeapLine(
  hw.support.value.memory,
  hw.memory.memory.value as unknown as { usedJSHeapSize?: number | null; jsHeapSizeLimit?: number | null } | undefined,
))
// Crash fix: the wasm `top` twin answers tasks/counts/uptime ONLY —
// `top.cpuPercent.toFixed()` threw on every mount + 4s poll. formatTopLine
// renders real readings where present, 'n/a' where the sandbox has none.
const nativeStatsLine = computed(() => formatTopLine(store.osTop as TopSnapshot | null))
const motionLine = computed(() => {
  if (!hw.support.value.motion && !hw.support.value.deviceOrientation) return 'motion sensors not exposed here.'
  const o = hw.deviceOrientation
  const parts: string[] = []
  if (isFiniteNum(o.alpha.value)) parts.push(`α ${o.alpha.value.toFixed(0)}°`)
  if (isFiniteNum(o.beta.value)) parts.push(`β ${o.beta.value.toFixed(0)}°`)
  if (isFiniteNum(o.gamma.value)) parts.push(`γ ${o.gamma.value.toFixed(0)}°`)
  return parts.length ? `orientation ${parts.join(' ')}` : 'motion: no reading yet (needs gesture on iOS).'
})
const localeLine = computed(() => {
  const lang = hw.navigatorLanguage.language.value ?? 'unknown'
  const prefs = hw.preferredLanguages.value.slice(0, 2).join(', ')
  return `${lang}${prefs ? ` · pref ${prefs}` : ''}`
})
const screenLine = computed(() => hw.screens.value.map(s => `${s.label || 'screen'} ${s.width}×${s.height}${s.primary ? ' ★' : ''}`).join(' · '))
const canNotify = computed(() => hw.support.value.notifications && hw.notifyPermission.value === 'granted')

async function toggleCam() {
  if (camOn.value) hw.stopCamera()
  else await hw.startCamera()
}
async function toggleShare() {
  if (shareOn.value) hw.stopScreenShare()
  else await hw.startScreenShare()
}
function toggleListen() {
  try {
    if (listening.value) hw.speechRecognition.stop()
    else hw.speechRecognition.start()
  } catch { /* denied */ }
}
async function openLocal() { await hw.openLocalFile() }
async function saveLocal() { await hw.saveLocalFile(`CyberManju OS note — ${new Date().toISOString()}\n`, 'cybermanju-note.txt') }
async function copyRich() {
  await hw.copyRichHtml('<b>CyberManju OS</b> — web OS inside an OS', 'CyberManju OS — web OS inside an OS')
}
async function badgeDemo() { await hw.setBadge(1); setTimeout(() => void hw.clearBadge(), 4000) }
async function pickContacts() {
  const list = await hw.pickContacts()
  identityMsg.value = list.length ? `${list.length} contact(s) picked (kept in memory, never stored).` : 'No contacts picked.'
}
function passkeyInfo() {
  identityMsg.value = hw.webAuthnSupported()
    ? 'Passkeys ready — bind WebAuthn in the Tauri/desktop auth settings; this browser can create platform credentials.'
    : 'WebAuthn not available here.'
}
async function askNotify() {
  try { await hw.webNotification.ensurePermissions() } catch { /* denied */ }
}
function testNotify() {
  try { hw.webNotification.show({ body: 'CyberManju OS notifications work.', tag: 'cybermanju-test' }) } catch { /* noop */ }
}

async function locate() { try { await hw.geo.resume() } catch { /* denied */ } }
async function copyGeo() { if (hasGeo.value) await hw.copy(`${geoLat.value},${geoLon.value}`) }
async function toggleWake() {
  try {
    if (hw.wakeLock.isActive.value) await hw.wakeLock.release()
    else await hw.wakeLock.request('screen')
  } catch { /* not allowed */ }
}
async function loadStorage() {
  const est = await getStorageEstimate()
  storageStat.value = {
    usage: est.usage ?? null,
    quota: est.quota ?? null,
    percent: est.percent ?? null,
    persisted: !!est.persisted,
  }
  storageMsg.value = est.quota
    ? `${formatBytesDec(est.usage ?? 0)} / ${formatBytesDec(est.quota)} (${est.percent ?? 0}%) · persisted: ${est.persisted ? 'yes' : 'no'}`
    : `persisted: ${est.persisted ? 'yes' : 'no'} (quota hidden by browser)`
  await store.fetchOsDf()
}
async function persist() { await persistStorage(); await loadStorage() }
async function shareNow() {
  try { await hw.share({ title: 'CyberManju OS', text: 'Web OS inside an OS', url: location.href }) } catch { /* dismissed */ }
}
function buzz() { try { hw.vibrate(40) } catch { /* noop */ } }
async function pick() {
  try { const r = await hw.eyeDropper.open(); if (r?.sRGBHex) picked.value = r.sRGBHex } catch { /* dismissed */ }
}

watch(() => hw.userMedia.stream.value, (s) => {
  if (camVideoRef.value) {
    try { (camVideoRef.value as HTMLVideoElement).srcObject = s ?? null } catch { /* noop */ }
  }
}, { immediate: true })
watch(() => hw.displayMedia.stream.value, (s) => {
  if (shareVideoRef.value) {
    try { (shareVideoRef.value as HTMLVideoElement).srcObject = s ?? null } catch { /* noop */ }
  }
}, { immediate: true })

let statsPoll = 0
onMounted(() => {
  void hw.refreshDevices()
  void loadStorage()
  void store.fetchOsTop()
  void store.fetchDevices()
  statsPoll = window.setInterval(() => {
    if (document.hidden) return
    void store.fetchOsTop()
  }, 4000)
})
onUnmounted(() => { if (statsPoll) window.clearInterval(statsPoll) })
</script>

<style scoped>
.dv { height: 100%; min-width: 0; overflow-x: hidden; overflow-y: auto; padding: 12px 14px; padding-bottom: max(12px, env(safe-area-inset-bottom)); display: flex; flex-direction: column; gap: 10px; background: var(--ui-surface); color: var(--ui-text); font-family: var(--ui-font); font-size: 13px; }
.dv-hero { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 12px 14px; border: 1px solid var(--ui-border); border-radius: 16px; background: linear-gradient(135deg, color-mix(in srgb, var(--ui-accent) 14%, transparent), color-mix(in srgb, var(--ui-accent) 3%, transparent) 55%, transparent); }
.dv-brand { display: flex; align-items: center; gap: 12px; min-width: 0; }
.dv-brand-mark { display: inline-flex; align-items: center; justify-content: center; width: 42px; height: 42px; border-radius: 13px; background: linear-gradient(135deg, var(--ui-accent), color-mix(in srgb, var(--ui-accent) 55%, transparent)); color: #fff; box-shadow: 0 4px 14px color-mix(in srgb, var(--ui-accent) 45%, transparent); flex-shrink: 0; }
.dv-brand-text { min-width: 0; }
.dv-title { margin: 0; font-size: 16px; letter-spacing: 0.4px; }
.dv-pills { display: flex; gap: 6px; flex-wrap: wrap; margin-top: 5px; }
.dv-top-actions { display: flex; gap: 8px; flex-shrink: 0; }
.dv-error { border: 1px solid color-mix(in srgb, var(--ui-danger) 55%, transparent); background: color-mix(in srgb, var(--ui-danger) 10%, transparent); border-radius: 10px; padding: 8px 12px; font-size: 12px; }
.dv-tabs { align-self: flex-start; max-width: 100%; overflow-x: auto; }
.dv-tiles { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 10px; margin-bottom: 10px; }
.dv-tile-top { display: flex; align-items: center; gap: 6px; }
.dv-tile-icon { color: var(--ui-accent); flex-shrink: 0; }
.dv-tile-label { font-size: 10px; font-weight: 700; letter-spacing: 1.2px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.dv-tile-value { font-size: 22px; font-weight: 750; letter-spacing: 0.2px; margin-top: 6px; font-variant-numeric: tabular-nums; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.dv-tile-value--sm { font-size: 15px; }
.dv-tile-sub { font-size: 11px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); margin: 2px 0 6px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.dv-stack { display: flex; flex-direction: column; gap: 10px; }
.dv-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 10px; }
.dv-card-line { margin: 0; font-size: 12.5px; line-height: 1.5; overflow-wrap: anywhere; }
.dv-counts { display: flex; gap: 4px; flex-wrap: wrap; }
.dv-chip { font-family: var(--ui-font-mono); font-size: 9.5px; padding: 1px 8px; border-radius: 99px; border: 1px solid var(--ui-border); background: var(--ui-glass); }
.dv-list { display: flex; flex-direction: column; gap: 6px; max-height: 300px; min-width: 0; overflow-y: auto; }
.dv-row { display: flex; align-items: center; gap: 10px; min-width: 0; min-height: 44px; padding: 7px 8px; border: 1px solid var(--ui-hairline); border-radius: 8px; background: var(--ui-glass); }
.dv-bus { font-family: var(--ui-font-mono); font-size: 8.5px; font-weight: 700; padding: 2px 7px; border-radius: 99px; border: 1px solid color-mix(in srgb, var(--ui-accent) 40%, transparent); color: var(--ui-accent); min-width: 64px; text-align: center; }
.dv-info { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.dv-label { font-size: 12px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.dv-kind { font-size: 10px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.dv-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--ui-border-strong); flex-shrink: 0; }
.dv-dot.on { background: var(--ui-success, #22c55e); box-shadow: 0 0 8px color-mix(in srgb, var(--ui-success, #22c55e) 70%, transparent); }
.dv-pair { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; margin-top: 8px; }
.dv-select {
  font-size: 11px; font-weight: 700; padding: 5px 6px; border-radius: 8px;
  border: 1px solid var(--ui-border); background: var(--ui-glass); color: var(--ui-text);
  cursor: pointer;
}
.dv-hint, .dv-muted { min-width: 0; overflow-wrap: anywhere; font-size: 11px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); margin: 8px 0 0; }
.dv-line { min-width: 0; font-size: 12px; margin: 8px 0 0; overflow: hidden; overflow-wrap: anywhere; text-overflow: ellipsis; }
.dv-vids { display: flex; gap: 8px; flex-wrap: wrap; margin-top: 8px; }
.dv-vid { width: 220px; max-width: 100%; aspect-ratio: 16/10; border-radius: 10px; border: 1px solid var(--ui-border); background: #000; }

@media (max-width: 600px) {
  .dv { padding: max(10px, env(safe-area-inset-top)) 10px max(14px, env(safe-area-inset-bottom)); gap: 8px; font-size: 14px; }
  .dv-hero { align-items: flex-start; padding: 12px; border-radius: 18px; }
  .dv-title { font-size: 18px; }
  .dv-top-actions { margin-top: -2px; flex-direction: column; }
  .dv-tiles { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
  .dv-tile-value { font-size: 19px; }
  .dv-grid { grid-template-columns: minmax(0, 1fr); gap: 8px; }
  .dv-list { max-height: none; gap: 8px; }
  .dv-row { border-radius: 12px; padding: 8px 10px; }
  .dv-pair { gap: 8px; }
  .dv-pair :deep(button), .dv-top-actions :deep(button) { min-height: 44px; }
  .dv-pair :deep(button) { flex: 1 1 auto; }
  .dv-card-line { overflow-wrap: anywhere; line-height: 1.5; }
  .dv-hint, .dv-muted { line-height: 1.45; }
}

@media (prefers-reduced-motion: reduce) {
  .dv *, .dv *::before, .dv *::after { scroll-behavior: auto !important; transition-duration: 0.01ms !important; animation-duration: 0.01ms !important; }
}
</style>
