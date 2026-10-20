<!-- CyberManju OS — NotificationStack (src/components/NotificationStack.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <Teleport to="body">
    <div class="notification-stack" role="status" aria-live="polite">
      <TransitionGroup name="nstack">
        <div
          v-for="n in notifications"
          :key="n.id"
          class="notification-item"
          :class="'notif-' + n.type"
          @click="dismiss(n.id)"
        >
          <span class="notif-icon"><AppIcon :name="ICONS[n.type]" :size="16" /></span>
          <span class="notif-body">
            <span class="notif-title">{{ n.title || TITLES[n.type] }}</span>
            <span class="notif-msg">{{ n.message }}</span>
          </span>
          <button class="notif-close" @click.stop="dismiss(n.id)" aria-label="Dismiss"><AppIcon name="solar:close-bold" :size="12" /></button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { useNotifications, type NotificationType } from '@/composables/useNotifications'

const { notifications, dismiss } = useNotifications()

const ICONS: Record<NotificationType, string> = {
  success: 'solar:check-circle-bold',
  error: 'solar:close-circle-bold',
  warning: 'solar:danger-triangle-bold',
  info: 'solar:info-circle-bold',
}

const TITLES: Record<NotificationType, string> = {
  success: 'CyberManju OS',
  error: 'CyberManju OS',
  warning: 'CyberManju OS',
  info: 'CyberManju OS',
}
</script>

<style scoped>
.notification-stack {
  position: fixed;
  top: calc(24px + 10px);
  right: 12px;
  /* Above modals/palette (10000) so a toast fired mid-dialog stays visible
     and tappable; below the first-run wizard (11000) on purpose. */
  z-index: 10500;
  display: flex;
  flex-direction: column;
  gap: 8px;
  pointer-events: none;
  width: 340px;
  max-width: calc(100vw - 24px);
}

/* Phone: drop toasts below the slim header band (notch inset + header) so
   they never paint across the connectivity/clock strip — and go full width
   with a solid fill, since phones run without backdrop blur. */
@media (max-width: 768px) {
  .notification-stack {
    top: calc(env(safe-area-inset-top, 0px) + var(--mobile-header-h, 26px) + 10px);
    right: 8px;
    width: min(380px, calc(100vw - 16px));
    max-width: calc(100vw - 16px);
  }
  .notification-item {
    background: color-mix(in srgb, var(--ui-surface) 94%, var(--ui-bg));
  }
}

.notification-item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 12px;
  background: var(--ui-glass-2);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  box-shadow: var(--ui-shadow-menu);
  color: var(--ui-text);
  font-family: var(--ui-font);
  cursor: pointer;
  pointer-events: auto;
  overflow: hidden;
}

.notif-error {
  border-left-color: var(--ui-danger);
}

.notif-error .notif-icon {
  color: var(--ui-danger);
}

.notif-success {
  border-left-color: var(--ui-accent);
}

.notif-success .notif-icon {
  color: var(--ui-accent);
}

.notif-warning {
  border-left-color: var(--ui-warning);
}

.notif-warning .notif-icon {
  color: var(--ui-warning);
}

.notif-info .notif-icon {
  color: var(--ui-info);
}

.notif-icon {
  flex-shrink: 0;
  display: flex;
  margin-top: 1px;
}

.notif-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.notif-title {
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0;
  color: var(--ui-text);
}

.notif-msg {
  font-size: 12px;
  font-weight: 400;
  line-height: 1.4;
  color: var(--ui-text-2);
  overflow-wrap: anywhere;
}

.notif-close {
  background: none;
  border: none;
  border-radius: var(--ui-radius-sm);
  color: var(--ui-text-3);
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  flex-shrink: 0;
  transition:
    background-color var(--ui-dur-fast) var(--ui-ease-out),
    color var(--ui-dur-fast) var(--ui-ease-out);
}

.notif-close:hover {
  background: color-mix(in srgb, var(--ui-danger) 16%, transparent);
  color: var(--ui-danger);
}

.nstack-enter-active,
.nstack-leave-active {
  transition: all var(--ui-dur) var(--ui-ease-out);
}

.nstack-enter-from {
  opacity: 0;
  transform: translateX(40px);
}

.nstack-leave-to {
  opacity: 0;
  transform: translateX(40px);
}

/* Plasma: bottom-right popups above the panel, 360px wide. */
[data-ui-shell='plasma'] .notification-stack {
  top: auto;
  bottom: calc(var(--ui-panel-h, 44px) + 10px);
  width: 360px;
}

[data-ui-shell='plasma'] .notification-item {
  border-radius: var(--ui-radius-lg);
  box-shadow: var(--ui-shadow-popup);
}
</style>
