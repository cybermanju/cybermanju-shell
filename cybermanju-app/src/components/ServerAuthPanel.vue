<script setup lang="ts">
// CyberManju OS — ServerAuthPanel (src/components/ServerAuthPanel.vue).
// Role: Vue desktop UI: windows, panels, overlays.
// Exports: module root (see items below).
// Integrates: vue, @/….
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — Docker/web login gate.
//
// Shown on every device that reaches the dashboard (:3456) without a valid
// JWT: first run offers bootstrap registration, afterwards username+password
// sign-in. Provider OAuth (Google/GitHub/GitLab, Connections tab) is a
// SEPARATE system for sync tokens — it never authenticates /api calls, so
// the old "reconnect with OAuth" hint is gone on purpose.
import { computed, onMounted, ref } from 'vue'
import AppIcon from '@/components/AppIcon.vue'
import { useAppStore } from '@/stores/app'
import { useTheme } from '@/composables/useTheme'
import { getServerUrl } from '@/composables/useTauri'

const store = useAppStore()
const isPlasma = computed(() => useTheme().shellStyle.value === 'plasma')

function powerRestart() {
  window.location.reload()
}

function powerSetup() {
  window.dispatchEvent(new CustomEvent('cybermanju:open-setup'))
}

const mode = ref<'login' | 'register'>('login')
const username = ref('')
const password = ref('')
const confirm = ref('')
const displayName = ref('')

onMounted(async () => {
  await store.checkAuthStatus()
  // First run (no accounts yet): default to registration.
  if (store.needsSetup) mode.value = 'register'
})

const heading = computed(() =>
  mode.value === 'register' ? 'Create the first account' : 'Sign in to CyberManju OS'
)

const canSubmit = computed(() => {
  if (!username.value.trim() || !password.value) return false
  if (mode.value === 'register' && password.value !== confirm.value) return false
  return !store.authBusy
})

async function submit() {
  const user = username.value.trim()
  if (!user || !password.value) return
  if (mode.value === 'register') {
    if (password.value !== confirm.value) return
    const ok = await store.register(user, password.value, displayName.value.trim() || undefined)
    if (ok) {
      username.value = ''
      password.value = ''
      confirm.value = ''
      displayName.value = ''
    }
  } else {
    const ok = await store.login(user, password.value)
    if (ok) {
      username.value = ''
      password.value = ''
    }
  }
}

function switchMode(next: 'login' | 'register') {
  mode.value = next
  password.value = ''
  confirm.value = ''
}

const serverLabel = computed(() => {
  try {
    const configured = getServerUrl()
    if (configured) return configured
    if (typeof window !== 'undefined' && window.location?.port === '3456') {
      return window.location.origin
    }
  } catch {
    // Fall through to the default below.
  }
  return 'http://<server>:3456'
})
</script>

<template>
  <div class="auth-gate" role="dialog" aria-modal="true" aria-labelledby="auth-gate-title">
    <div class="auth-card">
      <div class="auth-brand">
        <img class="auth-logo" src="/bhumisparsha.png" alt="Bhumisparsha" width="64" height="64" />
        <h1 id="auth-gate-title" class="auth-title">{{ heading }}</h1>
        <p class="auth-sub">{{ serverLabel }}</p>
      </div>

      <p v-if="mode === 'register'" class="auth-hint">
        No dashboard accounts exist yet — this creates the first one. Public
        registration closes afterwards by design; further accounts are created
        by an admin in Accounts → Users.
      </p>
      <p v-else class="auth-hint">
        Dashboard username + password. This signs in <em>this browser</em> —
        every new device signs in once the same way. Cloud OAuth (Google /
        GitHub / GitLab) lives in Accounts → Connections and is separate.
      </p>

      <form class="auth-form" @submit.prevent="submit">
        <label class="auth-field">
          <span>Username</span>
          <input
            v-model="username"
            class="auth-input"
            type="text"
            autocomplete="username"
            placeholder="e.g. admin"
            aria-label="Username"
          />
        </label>
        <label v-if="mode === 'register'" class="auth-field">
          <span>Display name (optional)</span>
          <input
            v-model="displayName"
            class="auth-input"
            type="text"
            autocomplete="nickname"
            placeholder="e.g. NAS Owner"
            aria-label="Display name"
          />
        </label>
        <label class="auth-field">
          <span>Password</span>
          <input
            v-model="password"
            class="auth-input"
            type="password"
            :autocomplete="mode === 'register' ? 'new-password' : 'current-password'"
            placeholder="••••••••"
            aria-label="Password"
          />
        </label>
        <label v-if="mode === 'register'" class="auth-field">
          <span>Confirm password</span>
          <input
            v-model="confirm"
            class="auth-input"
            type="password"
            autocomplete="new-password"
            placeholder="••••••••"
            aria-label="Confirm password"
          />
        </label>

        <p v-if="store.authError" class="auth-error" role="alert">{{ store.authError }}</p>

        <button class="auth-submit" type="submit" :disabled="!canSubmit">
          <AppIcon :name="mode === 'register' ? 'solar:user-plus-bold' : 'solar:login-bold'" :size="15" />
          {{ store.authBusy ? 'Working…' : mode === 'register' ? 'Create account & sign in' : 'Sign in' }}
        </button>
      </form>

      <div class="auth-switch">
        <button
          v-if="mode === 'login'"
          class="auth-link"
          type="button"
          @click="switchMode('register')"
        >
          No account yet? Create the first one
        </button>
        <button
          v-else
          class="auth-link"
          type="button"
          @click="switchMode('login')"
        >
          Already have an account? Sign in
        </button>
        <button class="auth-link muted" type="button" @click="store.checkAuthStatus()">
          Re-check server
        </button>
      </div>

      <p class="auth-foot">
        Headless install? Set
        <code>CYBERMANJU_ADMIN_USERNAME</code> +
        <code>CYBERMANJU_ADMIN_PASSWORD</code> once to provision an admin,
        then remove them after the first login.
      </p>
    </div>
    <div v-if="isPlasma" class="auth-power">
      <button type="button" class="auth-pow" title="Restart" @click="powerRestart">
        <AppIcon name="solar:restart-bold" :size="15" />
      </button>
      <button type="button" class="auth-pow" title="Setup" @click="powerSetup">
        <AppIcon name="solar:settings-bold" :size="15" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.auth-gate {
  position: fixed;
  inset: 0;
  z-index: 200;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
  background: rgba(0, 0, 0, 0.45);
}
[data-ui-mode='light'] .auth-gate {
  background: rgba(236, 236, 236, 0.55);
}
.auth-card {
  width: min(340px, 100%);
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  background: var(--ui-glass-2);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  box-shadow: var(--ui-shadow-menu);
  color: var(--ui-text);
  padding: 28px 24px 18px;
}
.auth-brand {
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: center;
  text-align: center;
  margin-bottom: 14px;
}
.auth-logo {
  width: 64px;
  height: 64px;
  object-fit: cover;
  border-radius: 50%;
  flex-shrink: 0;
  border: 1px solid var(--ui-border);
}
.auth-title {
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0;
  margin: 0;
}
.auth-sub {
  margin: 0;
  font-size: 12px;
  color: var(--ui-text-2);
  font-family: var(--ui-font);
}
.auth-hint {
  font-size: 12.5px;
  line-height: 1.5;
  color: var(--ui-text-2);
  margin: 8px 0 14px;
}
.auth-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.auth-field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  font-size: 13px;
  font-weight: 400;
}
.auth-field > span {
  font-size: 12px;
  color: var(--ui-text-2);
}
.auth-input {
  border: 1px solid var(--ui-border-strong);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface-2);
  color: var(--ui-text);
  padding: 7px 10px;
  min-height: var(--ui-control-h);
  font-size: 13px;
  font-family: var(--ui-font);
  outline: none;
  transition: border-color var(--ui-dur-fast) ease-out, box-shadow var(--ui-dur-fast) ease-out;
}
.auth-input:focus {
  border-color: var(--ui-accent);
  box-shadow: var(--ui-focus-ring);
}
.auth-error {
  font-size: 12.5px;
  color: var(--ui-danger);
  margin: 0;
  word-break: break-word;
}
.auth-submit {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  margin-top: 4px;
  border: 1px solid transparent;
  border-radius: var(--ui-radius-sm);
  min-height: var(--ui-control-h);
  padding: 0 14px;
  font-size: 13px;
  font-weight: 500;
  font-family: var(--ui-font);
  cursor: pointer;
  color: var(--ui-on-accent);
  background: var(--ui-accent);
  transition: background-color var(--ui-dur-fast) ease-out;
}
.auth-submit:hover:not(:disabled) {
  background: var(--ui-accent-strong);
}
.auth-submit:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.auth-switch {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 12px;
  gap: 8px;
  flex-wrap: wrap;
}
.auth-link {
  background: none;
  border: none;
  padding: 0;
  color: var(--ui-accent);
  font-size: 12.5px;
  font-family: var(--ui-font);
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 3px;
}
.auth-link.muted {
  color: var(--ui-text-3);
}
.auth-foot {
  margin: 14px 0 0;
  font-size: 11.5px;
  line-height: 1.5;
  color: var(--ui-text-3);
}
.auth-foot code {
  font-family: var(--ui-font-mono);
  font-size: 11px;
}

.auth-power {
  display: none;
}

/* SDDM style in plasma: wallpaper shows through, power buttons bottom-left. */
[data-ui-shell='plasma'] .auth-gate {
  background: transparent;
}

[data-ui-shell='plasma'] .auth-card {
  background: var(--ui-panel);
  box-shadow: var(--ui-shadow-popup);
}

[data-ui-shell='plasma'] .auth-power {
  position: fixed;
  left: 16px;
  bottom: 16px;
  display: flex;
  gap: 6px;
}

[data-ui-shell='plasma'] .auth-pow {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: var(--ui-radius-sm);
  background: var(--ui-panel);
  backdrop-filter: blur(20px) saturate(var(--ui-saturate));
  border: 1px solid var(--ui-border);
  color: var(--ui-text-2);
  cursor: pointer;
}

[data-ui-shell='plasma'] .auth-pow:hover {
  background: var(--ui-hover);
  color: var(--ui-text);
}
</style>
