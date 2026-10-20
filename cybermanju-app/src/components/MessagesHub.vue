<!-- CyberManju OS — MessagesHub (src/components/MessagesHub.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <Teleport to="body">
    <Transition name="msghub">
      <div
        v-if="launcher.messagesOpen"
        class="msg-veil"
        role="dialog"
        aria-modal="true"
        aria-label="Messages"
        @click.self="close"
      >
        <section class="msg-panel">
          <header class="msg-head">
            <div class="msg-title">
              <h2>Messages</h2>
              <span v-if="totalUnread > 0" class="msg-unread">{{ totalUnread > 99 ? '99+' : totalUnread }} new</span>
            </div>
            <button class="msg-close" type="button" aria-label="Close messages" @click="close">
              <AppIcon name="solar:close-circle-bold" :size="20" />
            </button>
          </header>

          <nav class="msg-tabs" aria-label="Message center tabs">
            <button
              class="msg-tab"
              :class="{ active: tab === 'messages' }"
              type="button"
              :aria-pressed="tab === 'messages'"
              @click="setTab('messages')"
            >
              Chats &amp; mails
              <span v-if="hub.supported && hub.unread.value > 0" class="msg-tab-badge">{{ hub.unread.value > 99 ? '99+' : hub.unread.value }}</span>
            </button>
            <button
              class="msg-tab"
              :class="{ active: tab === 'system' }"
              type="button"
              :aria-pressed="tab === 'system'"
              @click="setTab('system')"
            >
              System
              <span v-if="sysUnread > 0" class="msg-tab-badge">{{ sysUnread > 99 ? '99+' : sysUnread }}</span>
            </button>
          </nav>

          <!-- System notification history is per-device localStorage, so this
            pane renders on EVERY platform — only the chats pane is gated on
            the launcher's native-support flag (Android builds). -->
          <template v-if="tab === 'system'">
            <div v-if="sysHistory.length" class="msg-list">
              <button
                v-for="h in sysHistory"
                :key="h.id"
                class="msg-row"
                type="button"
                :title="h.read ? 'Read' : 'Mark as read'"
                @click="readRow(h.id)"
              >
                <span class="msg-row-tile" :class="`sys-${h.type}`">
                  <AppIcon :name="SYS_ICONS[h.type]" :size="18" />
                  <span v-if="!h.read" class="msg-dot" aria-label="Unread" />
                </span>
                <span class="msg-row-copy">
                  <span class="msg-row-top">
                    <strong>{{ h.title || SYS_TITLES[h.type] }}</strong>
                    <time>{{ formatMessageTime(h.createdAt) }}</time>
                  </span>
                  <span class="msg-row-text">{{ h.message }}</span>
                </span>
              </button>
            </div>
            <p v-else class="msg-state">No system notifications yet — toasts and warnings are kept here.</p>

            <footer class="msg-foot">
              <button class="msg-btn" type="button" :disabled="sysUnread === 0" @click="markAll">
                <AppIcon name="solar:check-circle-bold" :size="15" /> Mark all read
              </button>
              <button
                class="msg-btn"
                type="button"
                title="Remove the stored system notification history"
                :disabled="!sysHistory.length"
                @click="clearSystem"
              >
                <AppIcon name="solar:trash-bin-trash-bold" :size="15" /> Clear
              </button>
            </footer>
          </template>

          <template v-else>
            <p v-if="!hub.supported" class="msg-note">
              Stored chats and mails live in the native Android build. This device shows the CyberManju panels instead.
              <button class="msg-btn" type="button" @click="openInstall">Install options</button>
            </p>

            <template v-else>
              <div v-if="hub.notifState.value && !hub.notifState.value.enabled" class="msg-enable" role="alert">
                <AppIcon name="solar:chat-round-dots-bold" :size="22" />
                <p>{{ hub.notifState.value.detail }}</p>
                <button class="msg-btn primary" type="button" @click="enableAccess">Enable access</button>
              </div>

              <div v-if="hub.socialApps.value.length" class="msg-social" aria-label="Social apps">
                <button
                  v-for="app in hub.socialApps.value"
                  :key="app.packageName"
                  class="msg-soc"
                  type="button"
                  :title="app.packageName"
                  @click="openApp(app.packageName, app.label)"
                >
                  <span class="msg-soc-tile">
                    <img v-if="app.iconBase64" class="msg-img" :src="app.iconBase64" :alt="`${socialName(app)} icon`" loading="lazy" draggable="false" />
                    <span v-else class="msg-letter" aria-hidden="true">{{ socialName(app).slice(0, 1).toUpperCase() }}</span>
                  </span>
                  <span class="msg-soc-name">{{ socialName(app) }}</span>
                </button>
              </div>

              <div v-if="hub.status.value === 'loading'" class="msg-state" role="status">
                <span class="msg-spinner" aria-hidden="true" />
                <p>Loading stored messages…</p>
              </div>

              <div v-else-if="hub.status.value === 'error'" class="msg-state" role="alert">
                <p class="msg-error">{{ hub.lastError.value || 'Could not load messages.' }}</p>
                <button class="msg-btn" type="button" @click="retry">
                  <AppIcon name="solar:refresh-bold" :size="15" /> Retry
                </button>
              </div>

              <div v-else-if="hub.messages.value.length" class="msg-list">
                <button
                  v-for="m in hub.messages.value"
                  :key="m.key || `${m.packageName}-${m.timestamp}`"
                  class="msg-row"
                  type="button"
                  @click="openApp(m.packageName, m.appLabel)"
                >
                  <span class="msg-row-tile">
                    <img v-if="iconFor(m.packageName)" class="msg-img" :src="iconFor(m.packageName)" alt="" loading="lazy" draggable="false" />
                    <span v-else class="msg-letter sm" aria-hidden="true">{{ (m.appLabel || m.packageName).slice(0, 1).toUpperCase() }}</span>
                    <span v-if="m.timestamp > launcher.messagesSeenAt" class="msg-dot" aria-label="Unread" />
                  </span>
                  <span class="msg-row-copy">
                    <span class="msg-row-top">
                      <strong>{{ m.appLabel || m.packageName }}</strong>
                      <time>{{ formatMessageTime(m.timestamp) }}</time>
                    </span>
                    <span v-if="m.title" class="msg-row-title">{{ m.title }}</span>
                    <span v-if="m.text" class="msg-row-text">{{ m.text }}</span>
                  </span>
                </button>
              </div>
              <p v-else class="msg-state">No stored messages yet — chats and mails appear here once they notify.</p>

              <footer class="msg-foot">
                <button class="msg-btn" type="button" @click="retry">
                  <AppIcon name="solar:refresh-bold" :size="15" /> Refresh
                </button>
                <button
                  class="msg-btn"
                  type="button"
                  title="Dismiss all notifications and clear the stored history"
                  :disabled="!hub.messages.value.length"
                  @click="clearAll"
                >
                  <AppIcon name="solar:trash-bin-trash-bold" :size="15" /> Clear all
                </button>
              </footer>
            </template>
          </template>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { computed, watch } from 'vue'
import { useAndroidApps } from '@/composables/useAndroidApps'
import { useMessages } from '@/composables/useMessages'
import { useNotifications, type NotificationType } from '@/composables/useNotifications'
import { useLauncherStore } from '@/stores/launcher'
import { useAppStore } from '@/stores/app'
import { requestInstallWindow } from '@/utils/installLinks'
import { launcherKeyForApp, formatMessageTime } from '@/utils/launcher'
import type { AndroidApp } from '@/types'

const hub = useMessages()
const browser = useAndroidApps()
const launcher = useLauncherStore()
const store = useAppStore()
const sys = useNotifications()

const tab = computed(() => launcher.centerTab)
/** Switching tabs never marks anything read — only the explicit per-row /
 * "Mark all read" actions below clear the system badge. */
function setTab(t: 'system' | 'messages') {
  launcher.centerTab = t
}

const sysHistory = computed(() => sys.history.value)
const sysUnread = computed(() => sys.unreadCount.value)
const totalUnread = computed(() => sysUnread.value + (hub.supported ? hub.unread.value : 0))

const SYS_ICONS: Record<NotificationType, string> = {
  success: 'solar:check-circle-bold',
  error: 'solar:close-circle-bold',
  warning: 'solar:danger-triangle-bold',
  info: 'solar:info-circle-bold',
}

const SYS_TITLES: Record<NotificationType, string> = {
  success: 'CyberManju OS',
  error: 'CyberManju OS',
  warning: 'CyberManju OS',
  info: 'CyberManju OS',
}

function readRow(id: string) {
  sys.markRead(id)
}

function markAll() {
  sys.markAllRead()
}

function clearSystem() {
  sys.clearHistory()
  store.notifySuccess('System notifications cleared')
}

function close() {
  launcher.setMessagesOpen(false)
}

/** Open the Install window (native builds per platform, App × CLI). */
function openInstall() {
  requestInstallWindow('needs-app')
}

function socialName(app: AndroidApp): string {
  const alias = (launcher.overrides[launcherKeyForApp(app.packageName)]?.alias ?? '').trim()
  return alias || app.label
}

function iconFor(packageName: string): string {
  return hub.socialApps.value.find((a) => a.packageName === packageName)?.iconBase64 ?? ''
}

async function openApp(packageName: string, label: string) {
  // Row interaction is the read event: the digest badge stays until the
  // user actually opens a row (opening the center alone never clears it).
  hub.markSeen()
  try {
    await browser.openApp(packageName)
  } catch (e) {
    store.notifyError(`Could not open ${label}`, e)
  }
}

async function retry() {
  try {
    // Explicit Refresh button — the user's own action, so it may settle the
    // digest; a bare `messagesOpen` watch no longer does (see below).
    await hub.refresh(true)
    hub.markSeen()
  } catch (e) {
    store.notifyError('Could not load messages', e)
  }
}

async function clearAll() {
  try {
    // Clears the stored history AND dismisses the live shade notifications
    // (service `cancelAllNotifications`, best-effort).
    await hub.clear()
    store.notifySuccess('All notifications cleared')
  } catch (e) {
    store.notifyError('Could not clear notifications', e)
  }
}

async function enableAccess() {
  try {
    await hub.openSettings()
  } catch (e) {
    store.notifyError('Could not open notification settings', e)
  }
}

watch(
  () => launcher.messagesOpen,
  (open) => {
    if (!open) return
    // Opening the center NEVER marks anything read: the system badge clears
    // only via readRow()/markAll() and the message digest only via a row tap
    // or the explicit Refresh action (the "while you were away" digest must
    // survive an open/close). Data load only.
    if (!hub.supported) return
    void hub.refreshSocial().catch(() => {})
    void hub.refresh(true).catch(() => {})
  },
)
</script>

<style scoped>
.msg-veil {
  position: fixed;
  inset: 0;
  z-index: 9400;
  background: color-mix(in srgb, var(--ui-bg-deep) 55%, transparent);
  backdrop-filter: blur(6px);
  -webkit-backdrop-filter: blur(6px);
  display: flex;
  align-items: flex-start;
  justify-content: flex-end;
  padding: calc(10px + env(safe-area-inset-top, 0px)) 10px 10px;
}
.msg-panel {
  width: min(420px, calc(100vw - 20px));
  max-height: min(84dvh, 680px);
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px 14px calc(14px + env(safe-area-inset-bottom, 0px));
  border-radius: 20px;
  border: 1px solid var(--ui-border-strong);
  background: color-mix(in srgb, var(--ui-surface) 90%, var(--ui-bg));
  color: var(--ui-text);
  box-shadow: var(--ui-shadow-3);
  overflow: hidden;
}
.msg-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.msg-title {
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.msg-title h2 {
  margin: 0;
  font-size: 19px;
  font-weight: 750;
  letter-spacing: -0.02em;
}
.msg-unread {
  padding: 2px 9px;
  border-radius: 999px;
  background: var(--ui-accent);
  color: var(--ui-on-accent);
  font-size: 11px;
  font-weight: 750;
}
.msg-close {
  width: 34px;
  height: 34px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: 50%;
  background: var(--ui-surface-2);
  color: var(--ui-text-2);
  cursor: pointer;
}
.msg-tabs {
  display: flex;
  gap: 6px;
  padding: 4px;
  border-radius: 14px;
  background: var(--ui-surface-2);
}
.msg-tab {
  flex: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  min-height: 34px;
  padding: 0 12px;
  border: 0;
  border-radius: 10px;
  background: transparent;
  color: var(--ui-text-2);
  font: inherit;
  font-size: 13px;
  font-weight: 650;
  cursor: pointer;
}
.msg-tab.active {
  background: var(--ui-surface);
  color: var(--ui-text);
  box-shadow: var(--ui-shadow-1);
}
.msg-tab-badge {
  min-width: 20px;
  height: 18px;
  padding: 0 6px;
  border-radius: 999px;
  background: var(--ui-accent);
  color: var(--ui-on-accent);
  font-size: 11px;
  font-weight: 750;
  line-height: 18px;
  text-align: center;
}
.msg-row-tile.sys-error { color: var(--ui-danger); }
.msg-row-tile.sys-success { color: var(--ui-accent); }
.msg-row-tile.sys-warning { color: var(--ui-warning); }
.msg-row-tile.sys-info { color: var(--ui-info); }
.msg-note {
  margin: 0;
  color: var(--ui-text-3);
  font-size: 13px;
  text-align: center;
}
.msg-enable {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 12px;
  border: 1px solid color-mix(in srgb, var(--ui-accent) 45%, var(--ui-border));
  border-radius: 14px;
  background: color-mix(in srgb, var(--ui-accent) 8%, transparent);
  text-align: center;
}
.msg-enable p {
  margin: 0;
  font-size: 13px;
  color: var(--ui-text-2);
}
.msg-social {
  display: flex;
  gap: 8px;
  padding: 2px 2px 8px;
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
}
.msg-soc {
  flex: 0 0 auto;
  width: 62px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 5px;
  padding: 2px;
  border: 0;
  background: transparent;
  cursor: pointer;
  -webkit-tap-highlight-color: transparent;
}
.msg-soc:active {
  transform: scale(0.92);
}
.msg-soc-tile {
  position: relative;
  width: 46px;
  height: 46px;
  display: grid;
  place-items: center;
  border-radius: 14px;
  overflow: hidden;
  background: linear-gradient(145deg, var(--ui-surface-2), var(--ui-surface));
  border: 1px solid var(--ui-border);
  color: var(--ui-text);
  font-size: 20px;
  font-weight: 750;
}
.msg-img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
.msg-letter {
  user-select: none;
}
.msg-soc-name {
  max-width: 100%;
  overflow: hidden;
  color: var(--ui-text-2);
  font-size: 10px;
  font-weight: 500;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.msg-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 22px 12px;
  color: var(--ui-text-3);
  font-size: 13px;
  text-align: center;
}
.msg-error {
  color: var(--ui-danger);
}
.msg-spinner {
  width: 26px;
  height: 26px;
  border-radius: 50%;
  border: 3px solid var(--ui-border-strong);
  border-top-color: var(--ui-accent);
  animation: msg-spin 0.8s linear infinite;
}
@keyframes msg-spin {
  to { transform: rotate(360deg); }
}
.msg-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
  min-height: 0;
}
.msg-row {
  display: flex;
  gap: 10px;
  align-items: flex-start;
  padding: 9px 10px;
  border: 1px solid var(--ui-border);
  border-radius: 14px;
  background: var(--ui-surface);
  color: var(--ui-text);
  text-align: left;
  cursor: pointer;
  font: inherit;
}
.msg-row:active {
  transform: scale(0.99);
}
.msg-row-tile {
  position: relative;
  flex: 0 0 40px;
  width: 40px;
  height: 40px;
  display: grid;
  place-items: center;
  border-radius: 12px;
  overflow: hidden;
  background: var(--ui-surface-2);
  border: 1px solid var(--ui-border);
  color: var(--ui-text);
  font-size: 18px;
  font-weight: 750;
}
.msg-letter.sm {
  font-size: 18px;
}
.msg-dot {
  position: absolute;
  top: 4px;
  right: 4px;
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: var(--ui-accent);
  border: 2px solid var(--ui-surface);
}
.msg-row-copy {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.msg-row-top {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
}
.msg-row-top strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.msg-row-top time {
  flex: 0 0 auto;
  color: var(--ui-text-3);
  font-size: 11px;
}
.msg-row-title {
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}
.msg-row-text {
  color: var(--ui-text-2);
  font-size: 13px;
  overflow: hidden;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow-wrap: anywhere;
}
.msg-foot {
  display: flex;
  gap: 8px;
}
.msg-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 36px;
  padding: 0 12px;
  border: 1px solid var(--ui-border-strong);
  border-radius: 12px;
  background: var(--ui-surface);
  color: var(--ui-text);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}
.msg-btn.primary {
  background: var(--ui-accent);
  border-color: var(--ui-accent);
  color: var(--ui-on-accent);
}
.msg-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.msghub-enter-active,
.msghub-leave-active {
  transition: opacity 180ms ease;
}
.msghub-enter-active .msg-panel,
.msghub-leave-active .msg-panel {
  transition: transform 220ms cubic-bezier(0.32, 0.9, 0.35, 1), opacity 180ms ease;
}
.msghub-enter-from,
.msghub-leave-to {
  opacity: 0;
}
.msghub-enter-from .msg-panel,
.msghub-leave-to .msg-panel {
  transform: translateY(-14px);
  opacity: 0;
}
@media (max-width: 560px) {
  .msg-veil {
    align-items: stretch;
    justify-content: stretch;
  }
  .msg-panel {
    width: 100%;
    max-height: none;
    border-radius: 0;
    border: 0;
  }
}
</style>
