<!-- CyberManju OS — AccountManagerPanel (src/components/AccountManagerPanel.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="am">
    <!-- ── header ─────────────────────────────────────────── -->
    <header class="am-top">
      <div class="am-brand">
        <span class="am-brand-mark"><AppIcon name="solar:user-circle-bold" :size="20" /></span>
        <div>
          <h2 class="am-title">Accounts</h2>
          <p class="am-subtitle">{{ store.syncConfigs.length }} providers · {{ attachedDisks }} disks · {{ identity ? identity.name : 'signed out' }}</p>
        </div>
      </div>
      <div class="am-top-actions">
        <span v-if="dbBackend" class="am-chip" :class="dbBackend === 'memory' ? 'is-warn' : 'is-ok'">
          <AppIcon name="solar:database-bold" :size="12" /> {{ dbBackend === 'memory' ? 'Session database' : dbBackend + ' database' }}
        </span>
        <button class="am-btn" type="button" :disabled="refreshing" @click="refresh">
          <AppIcon name="solar:restart-bold" :size="13" /> {{ refreshing ? 'Refreshing…' : 'Refresh' }}
        </button>
      </div>
    </header>

    <!-- ── merged volume hero ─────────────────────────────── -->
    <section class="am-hero">
      <div class="am-hero-row">
        <div class="am-hero-stat">
          <span class="am-hero-label">Merged volume</span>
          <strong class="am-hero-value">{{ humanBytes(volumeView?.usedBytes ?? 0) }} <span>of {{ humanBytes(volumeView?.totalBytes ?? 0) }}</span></strong>
        </div>
        <div class="am-hero-stat right">
          <span class="am-hero-label">{{ volumeView?.diskCount ?? 0 }} disks attached</span>
          <strong class="am-hero-value">{{ volumePct.toFixed(1) }}<span>% used</span></strong>
        </div>
      </div>
      <div class="am-bar" role="img" :aria-label="`Volume ${volumePct.toFixed(1)} percent used`">
        <div class="am-bar-fill" :style="{ width: `${volumePct}%` }"></div>
      </div>
      <div class="am-hero-legend">
        <span>Free {{ humanBytes(volumeView?.freeBytes ?? 0) }}</span>
        <span v-if="staticHost">Offline demo vault — providers, disks, users and files run here; only provider network sync needs the server.</span>
      </div>
    </section>

    <!-- ── tabs ───────────────────────────────────────────── -->
    <nav class="am-tabs" role="tablist" aria-label="Account sections" @keydown="onTabsKey">
      <button
        v-for="t in TABS"
        :key="t.id"
        :id="`am-tab-${t.id}`"
        role="tab"
        :aria-controls="`am-panel-${t.id}`"
        :aria-selected="activeTab === t.id"
        :tabindex="activeTab === t.id ? 0 : -1"
        class="am-tab"
        :class="{ active: activeTab === t.id }"
        type="button"
        @click="activeTab = t.id"
      >
        <AppIcon :name="t.icon" :size="14" /> {{ t.label }}
        <span v-if="t.id === 'connections' && store.syncConfigs.length" class="am-tab-count">{{ store.syncConfigs.length }}</span>
        <span v-if="t.id === 'connections' && identity" class="am-dot" aria-hidden="true"></span>
        <span v-if="t.id === 'vault' && disk.dirty" class="am-dot warn" aria-hidden="true"></span>
      </button>
    </nav>

    <main class="am-body">
      <!-- ══ CONNECTIONS — icon rail (session) + provider workspace ══ -->
      <section v-if="activeTab === 'connections'" id="am-panel-connections" class="am-section am-conn-section" role="tabpanel" aria-labelledby="am-tab-connections" tabindex="0">
        <div class="am-conn" :class="{ 'rail-pinned': railPinned }">
          <!-- ── left icon rail: identity + OAuth logins ── -->
          <aside class="am-rail" aria-label="Session and sign-in">
            <button
              class="am-rail-btn avatar"
              type="button"
              :title="identity ? `${identity.name} — open session` : 'Signed out — open sign-in'"
              :aria-expanded="railPinned"
              @click="railPinned = !railPinned"
            >
              <img v-if="identity?.avatarUrl" class="am-rail-avatar" :src="identity.avatarUrl" alt="" />
              <ProviderLogo v-else-if="identity" :provider="identity.provider" :size="26" />
              <AppIcon v-else name="solar:user-circle-bold" :size="26" />
              <span class="am-rail-dot" :class="identity ? 'is-ok' : ''" aria-hidden="true"></span>
            </button>
            <span class="am-rail-sep" aria-hidden="true"></span>
            <button
              v-for="p in LOGIN_CARDS"
              :key="p.id"
              class="am-rail-btn"
              type="button"
              :disabled="signInBusy === p.id"
              :title="railHint(p)"
              :aria-label="railHint(p)"
              @click="railProviderClick(p)"
            >
              <ProviderLogo :provider="p.logo" :size="26" />
              <span class="am-rail-dot" :class="railTone(p)" aria-hidden="true"></span>
            </button>
            <span style="flex:1" aria-hidden="true"></span>
            <button
              class="am-rail-btn pin"
              type="button"
              :title="railPinned ? 'Unpin session panel' : 'Pin session panel open'"
              :aria-pressed="railPinned"
              @click="railPinned = !railPinned"
            >
              <AppIcon name="solar:alt-arrow-down-bold" :size="16" class="am-rail-pin-icon" :class="{ open: railPinned }" />
            </button>
          </aside>
          <button
            v-if="railPinned"
            class="am-flyout-backdrop"
            type="button"
            aria-label="Close session panel"
            @click="railPinned = false"
          />
          <!-- ── session flyout: opens on rail hover, stays when pinned ── -->
          <div class="am-flyout" role="dialog" aria-label="Session">
            <div class="am-flyout-head">
              <strong>Session</strong>
              <span class="muted small">{{ identity ? identity.name : 'Signed out' }}</span>
              <button class="am-icon-btn" type="button" title="Close session panel" aria-label="Close session panel" @click="railPinned = false">
                <AppIcon name="solar:close-bold" :size="14" />
              </button>
            </div>
            <div class="am-flyout-scroll">
        <!-- ── who you are ── -->
        <div v-if="identity" class="am-card am-identity">
          <div class="am-identity-row">
            <img v-if="identity.avatarUrl" class="am-avatar" :src="identity.avatarUrl" alt="" />
            <ProviderLogo v-else :provider="identity.provider" :size="40" />
            <div class="am-identity-meta">
              <strong>{{ identity.name }}</strong>
              <span class="muted">{{ identity.email || identity.provider }}</span>
            </div>
            <span class="am-status is-ok"><ProviderLogo :provider="identity.provider" :size="18" /> {{ identity.provider }}</span>
            <button class="am-btn sm" type="button" :disabled="signingOut || revoking" @click="signOut">{{ signingOut ? '…' : 'Sign out' }}</button>
            <button
              class="am-btn sm danger"
              type="button"
              :disabled="signingOut || revoking"
              title="Revoke the provider OAuth grant + sign out + forget this login so it cannot auto-login again"
              @click="revokeSession"
            >{{ revoking ? '…' : 'Revoke' }}</button>
          </div>
          <p class="am-hint">Signed in via the broker — no password stored. This unlocks one-click OAuth below.</p>
        </div>

        <button
          v-if="identity"
          class="am-accounts-toggle"
          type="button"
          :aria-expanded="accountsOpen"
          @click="accountsOpen = !accountsOpen"
        >
          <AppIcon :name="accountsOpen ? 'solar:alt-arrow-up-bold' : 'solar:alt-arrow-down-bold'" :size="13" />
          {{ accountsOpen ? 'Hide sign-in options' : `Switch or add account${connectedAccounts.length > 1 ? ` (${connectedAccounts.length})` : ''}` }}
        </button>

        <div v-if="connectedAccounts.length && (!identity || accountsOpen)" class="am-card">
          <h3 class="am-card-title">Connected accounts ({{ connectedAccounts.length }})</h3>
          <p class="am-hint">One active session — the rest stay remembered for one-click switching. Each sign-in creates its provider connection below. Sign-in is per device — repeat it on each device; saved provider tokens then sync across.</p>
          <div v-for="acc in connectedAccounts" :key="acc.id" class="am-identity-row">
            <img v-if="acc.avatarUrl" class="am-avatar" :src="acc.avatarUrl" alt="" />
            <ProviderLogo v-else :provider="acc.provider" :size="32" />
            <div class="am-identity-meta">
              <strong>{{ acc.name }}</strong>
              <span class="muted">{{ acc.email || acc.provider }}</span>
            </div>
            <span class="am-status sm" :class="providerBadgeTone(acc)">{{ providerBadge(acc) }}</span>
            <span class="am-status sm" :class="acc.id === identity?.id ? 'is-ok' : ''">{{ acc.id === identity?.id ? 'Active' : acc.provider }}</span>
            <button v-if="needsProviderFor(acc.provider)" class="am-btn xs primary" type="button" :disabled="!!signInBusy || ensuringProvider" :title="`Create a ${backendLabel(backendForOAuth(acc.provider as OAuthBackend))} provider connection for this login`" @click="connectAccountToProvider(acc)">{{ ensuringProvider ? '…' : 'Connect' }}</button>
            <button v-else-if="configsForBackendSafe(acc.provider).length" class="am-btn xs danger" type="button" :title="`Delete the ${providerBackendLabel(acc.provider)} provider connection(s) — keeps this login remembered`" @click="disconnectAccountProvider(acc)">Disconnect</button>
            <button v-if="acc.id !== identity?.id" class="am-btn xs" type="button" :disabled="!!signInBusy || revoking" @click="switchAccount(acc)">Switch</button>
            <button class="am-btn xs danger" type="button" :disabled="revoking" :title="`Revoke OAuth grant + forget ${acc.name} (stops auto-login)`" @click="revokeAccount(acc)">Revoke</button>
            <button class="am-btn xs" type="button" :title="`Forget ${acc.name}`" @click="forgetAccount(acc.id)">Forget</button>
          </div>
          <div v-if="missingProviderBackends.length" class="am-banner info">
            <AppIcon name="solar:link-bold" :size="15" />
            <span>Signed in with {{ missingProviderBackends.map(backendLabel).join(', ') }} but no provider connection exists yet — one click creates {{ missingProviderBackends.length > 1 ? 'them' : 'it' }} below.</span>
            <button class="am-btn sm primary" type="button" :disabled="ensuringProvider" @click="ensureMissingFromAccounts()">{{ ensuringProvider ? 'Creating…' : `Create ${missingProviderBackends.length} connection${missingProviderBackends.length > 1 ? 's' : ''}` }}</button>
          </div>
          <p v-if="ensureMsg" class="am-note" :class="ensureOk === false ? 'err' : ensureOk === true ? 'ok' : ''">{{ ensureMsg }}</p>
        </div>

        <div v-if="!identity || accountsOpen" class="am-card">
          <h3 class="am-card-title">{{ identity ? 'Add another account' : 'Sign in' }}</h3>
          <p class="am-hint">Approve at the provider — no password typed here. Each login also creates its provider connection below.</p>
          <div class="am-login-grid">
            <button
              v-for="p in LOGIN_CARDS"
              :key="p.id"
              class="am-login"
              type="button"
              :disabled="signInBusy === p.id || !sbConfigured"
              :title="sbConfigured ? `Continue with ${p.label}` : 'OAuth broker not set — add it in Settings → OAuth broker'"
              @click="signIn(p.id)"
            >
              <ProviderLogo :provider="p.logo" :size="36" />
              <span class="am-login-label">{{ signInBusy === p.id ? 'Opening…' : `Continue with ${p.label}` }}</span>
              <span class="am-login-sub">{{ p.sub }}</span>
            </button>
          </div>
          <p v-if="signInMsg" class="am-note">{{ signInMsg }}</p>
          <div v-if="!sbConfigured" class="am-banner warn">
            <AppIcon name="solar:key-bold" :size="15" />
            <span>OAuth broker is not set — sign-in stays off until you add the URL + key in Settings → OAuth broker.</span>
            <button class="am-btn sm primary" type="button" @click="openBrokerSettings">Configure</button>
          </div>
        </div>

            </div><!-- /flyout scroll -->
          </div><!-- /flyout -->

          <!-- ── provider workspace: connections + detail, full width ── -->
          <div class="am-work">
            <header class="am-work-head">
              <div class="am-work-titles">
                <h3 class="am-work-title">Provider connections
                  <span v-if="store.syncConfigs.length" class="am-tab-count">{{ store.syncConfigs.length }}</span>
                </h3>
                <p class="am-hint">{{ selectedCfg ? `Selected: ${selectedCfg.name || backendLabel(selectedCfg.backendType)}` : 'Each connection syncs one backend into the merged volume' }}</p>
              </div>
              <button class="am-btn sm primary" type="button" @click="wizOpen = !wizOpen">
                <AppIcon name="solar:add-bold" :size="12" /> {{ wizOpen ? 'Close' : 'Add' }}
              </button>
            </header>
            <!-- list -->
            <div class="am-list-col">
              <div class="am-list-head">
                <span class="muted small">{{ filteredConfigs.length }} of {{ store.syncConfigs.length }}</span>
              </div>
            <label v-if="store.syncConfigs.length > 3" class="am-search">
              <AppIcon name="solar:magnifier-bold" :size="13" />
              <input v-model="provFilter" class="am-search-input" type="search" placeholder="Filter providers…" aria-label="Filter providers" />
            </label>
            <div
              v-for="cfg in filteredConfigs"
              :key="cfg.id"
              class="am-prov-row"
              :class="{ active: selectedId === cfg.id }"
            >
              <button
                type="button"
                class="am-prov"
                :class="{ active: selectedId === cfg.id }"
                :aria-label="`Select ${cfg.name || backendLabel(cfg.backendType)}`"
                @click="selectedId = cfg.id"
              >
                <ProviderLogo :provider="logoKindFor(cfg.backendType)" :size="34" />
                <span class="am-prov-meta">
                  <strong class="am-prov-name">{{ cfg.name || backendLabel(cfg.backendType) }}<span v-if="isDraftDirty(cfg)" class="am-unsaved" title="Unsaved edits" aria-label="Unsaved edits">●</span></strong>
                  <span class="muted small">{{ backendLabel(cfg.backendType) }} · {{ disksFor(cfg.id).length }} disks</span>
                </span>
                <span class="am-status sm" :class="statusTone(cfg.id)">{{ authLabel(cfg.id) }}</span>
                <span class="am-switch" :class="{ on: cfg.enabled }" :title="cfg.enabled ? 'Enabled' : 'Disabled'"></span>
              </button>
              <button
                class="am-prov-del"
                type="button"
                :title="`Delete ${cfg.name || backendLabel(cfg.backendType)} connection`"
                :aria-label="`Delete ${cfg.name || backendLabel(cfg.backendType)} provider connection`"
                @click="removeProvider(cfg.id)"
              >
                <AppIcon name="solar:trash-bin-trash-bold" :size="13" />
              </button>
            </div>
            <p v-if="store.syncConfigs.length && !filteredConfigs.length" class="am-hint">No providers match “{{ provFilter }}”. <button class="am-link" type="button" @click="provFilter = ''">Clear filter</button></p>
            <div v-if="!store.syncConfigs.length && !wizOpen" class="am-card">
              <UiEmpty
                icon="solar:cloud-bold"
                title="No providers yet"
                description="Connect GitHub, GitLab, Google Drive or a local folder to start syncing."
              >
                <template #actions>
                  <button class="am-btn sm primary" type="button" @click="wizOpen = true"><AppIcon name="solar:add-bold" :size="12" /> Add provider</button>
                </template>
              </UiEmpty>
            </div>

            <!-- add-provider wizard -->
            <div v-if="wizOpen" class="am-card am-wiz">
              <h3 class="am-card-title">Add provider</h3>
              <div class="am-logo-picker" role="radiogroup" aria-label="Provider type">
                <button
                  v-for="b in BACKEND_ORDER"
                  :key="b"
                  type="button"
                  role="radio"
                  :aria-checked="wiz.backendType === b"
                  class="am-logo-pick"
                  :class="{ active: wiz.backendType === b }"
                  :title="backendLabel(b)"
                  @click="wiz.backendType = b"
                >
                  <ProviderLogo :provider="logoKindFor(b)" :size="30" />
                  <span>{{ shortName(b) }}</span>
                </button>
              </div>
              <p class="am-hint">{{ backendBlurb(wiz.backendType) }}</p>
              <label class="am-field">
                <span class="am-field-label">Display name</span>
                <input v-model="wiz.name" class="am-input" placeholder="e.g. Work GitHub" aria-label="Display name" />
              </label>
              <div class="am-fields">
                <label v-for="f in fieldsFor(wiz.backendType)" :key="f.key" class="am-field" :class="{ grow: f.grow }">
                  <span class="am-field-label">{{ f.label }}</span>
                  <span class="am-input-wrap" v-if="f.secret">
                    <input v-model="(wiz as any)[f.key]" class="am-input" :type="showWizToken ? 'text' : 'password'" :placeholder="f.placeholder" autocomplete="off" />
                    <button class="am-icon-btn" type="button" @click="showWizToken = !showWizToken" :title="showWizToken ? 'Hide' : 'Show'" :aria-label="showWizToken ? 'Hide token' : 'Show token'"><AppIcon :name="showWizToken ? 'solar:eye-closed-bold' : 'solar:eye-bold'" :size="14" /></button>
                  </span>
                  <span class="am-input-wrap" v-else-if="f.key === 'basePath' && wiz.backendType === 'local'">
                    <input v-model="(wiz as any)[f.key]" class="am-input" :placeholder="f.placeholder" autocomplete="off" />
                    <button v-if="localDirSupported" class="am-btn xs" type="button" title="Pick a folder with the browser" @click="pickWizLocalDir()">Browse…</button>
                  </span>
                  <input v-else v-model="(wiz as any)[f.key]" class="am-input" :placeholder="f.placeholder" autocomplete="off" />
                  <span v-if="f.hint" class="am-field-hint">{{ f.hint }}</span>
                </label>
              </div>
              <p class="am-hint">{{ authGuidance(wiz.backendType) }}</p>
              <div v-if="isOauthCapable(wiz.backendType) && !wiz.token.trim() && !wizardSessionMatchesProvider" class="am-banner info am-auth-required">
                <AppIcon name="solar:lock-keyhole-bold" :size="15" />
                <span>Sign in with this provider or enter a personal access token before adding the connection. Empty cloud accounts cannot be saved.</span>
                <button class="am-btn sm primary" type="button" @click="railPinned = true">Open sign-in</button>
              </div>
              <p v-if="wiz.backendType === 'github' || wiz.backendType === 'gitlab'" class="am-hint">No repo yet? Save with just a token (skip the repo field), then use <strong>New private vault repo</strong> in the provider detail to create + sync one.</p>
              <div class="am-row">
                <button class="am-btn sm primary" type="button" :disabled="wizBusy || !wizardCanSave" @click="addProvider(true)">{{ wizBusy ? 'Verifying…' : 'Save & verify' }}</button>
                <button class="am-btn sm" type="button" :disabled="wizBusy || !wizardCanSave" @click="addProvider(false)">Save</button>
                <button class="am-btn sm" type="button" :disabled="wizBusy" @click="resetWizard()">Reset</button>
                <span v-if="wizMsg" class="am-note" :class="wizOk === false ? 'err' : wizOk === true ? 'ok' : ''" style="margin:0;" role="status">{{ wizMsg }}
                  <button v-if="isNoPickerNote(wizMsg)" class="am-btn xs" type="button" @click="openInstall">Install options</button>
                </span>
              </div>
            </div>
          </div>

          <!-- detail -->
          <div v-if="selectedCfg" class="am-detail-col">
            <article class="am-card am-detail">
              <header class="am-card-head">
                <div class="am-card-head-left">
                  <ProviderLogo :provider="logoKindFor(selectedCfg.backendType)" :size="40" />
                  <div>
                    <h3 class="am-card-title">{{ selectedCfg.name || backendLabel(selectedCfg.backendType) }}</h3>
                    <p class="am-hint">{{ backendLabel(selectedCfg.backendType) }} · {{ backendBlurb(selectedCfg.backendType) }}</p>
                  </div>
                </div>
                <span class="am-status" :class="statusTone(selectedCfg.id)">{{ authLabel(selectedCfg.id) }}</span>
              </header>

              <div class="am-row wrap">
                <button class="am-btn sm" :class="{ on: selectedCfg.enabled }" type="button" @click="toggleEnabled(selectedCfg!)">{{ selectedCfg.enabled ? 'Enabled' : 'Disabled' }}</button>
                <button class="am-btn sm" type="button" :disabled="probing.has(selectedCfg.id)" @click="probe(selectedCfg!)">{{ probing.has(selectedCfg.id) ? 'Testing…' : 'Test connection' }}</button>
                <button class="am-btn sm" type="button" @click="quota(selectedCfg!)">Quota</button>
                <button class="am-btn sm danger" type="button" @click="removeProvider(selectedCfg!.id)">Delete</button>
                <span v-if="quotaMsg[selectedCfg.id]" class="muted small">{{ quotaMsg[selectedCfg.id] }}</span>
              </div>
              <p v-if="authDetail(selectedCfg.id)" class="am-note" :class="authState[selectedCfg.id]?.ok === false ? 'err' : ''" :title="authHint(selectedCfg.id)">{{ authDetail(selectedCfg.id) }}</p>

              <!-- step 1: connect — one button, best transport first -->
              <div v-if="isOauthCapable(selectedCfg.backendType)" class="am-step">
                <h4 class="am-step-title"><span class="am-step-n">1</span> Connect <span class="am-method">{{ staticHost ? 'via Supabase broker' : 'via dashboard' }}</span></h4>
                <div class="am-connect">
                  <ProviderLogo :provider="logoKindFor(selectedCfg.backendType)" :size="28" />
                  <div class="am-connect-meta">
                    <strong class="small">{{ connectTitle(selectedCfg) }}</strong>
                    <span class="muted small">{{ connectSubtitle(selectedCfg) }}</span>
                  </div>
                  <button
                    v-if="connectBusy !== selectedCfg.id"
                    class="am-btn sm primary"
                    type="button"
                    :disabled="!connectAvailable(selectedCfg)"
                    :title="connectCtaHint(selectedCfg)"
                    @click="connectWithOAuth(selectedCfg!)"
                  >{{ connectCta(selectedCfg) }}</button>
                  <button v-else class="am-btn sm" type="button" @click="cancelConnect">Cancel</button>
                  <button
                    v-if="connectBusy !== selectedCfg.id"
                    class="am-btn sm danger"
                    type="button"
                    :disabled="revokingCfg === selectedCfg.id || revoking"
                    :title="`Revoke the ${shortName(selectedCfg.backendType)} OAuth token + clear the saved secret so auto-login stops`"
                    @click="revokeProviderToken(selectedCfg!)"
                  >{{ revokingCfg === selectedCfg.id ? 'Revoking…' : 'Revoke' }}</button>
                </div>
                <div v-if="connectBusy === selectedCfg.id" class="am-progress" role="status" aria-live="polite"><div class="am-progress-fill"></div></div>
                <p v-if="connectMsg[selectedCfg.id]" class="am-note" :class="authState[selectedCfg.id]?.ok === false ? 'err' : ''">{{ connectMsg[selectedCfg.id] }}</p>
                <p v-if="connectUrl[selectedCfg.id]" class="am-note">Popup blocked? <button class="am-link" type="button" @click="copyConnectUrl(selectedCfg!)">Copy link</button> or open manually: <span class="am-code">{{ connectUrl[selectedCfg.id] }}</span></p>
                <div v-if="!connectAvailable(selectedCfg)" class="am-banner warn">
                  <AppIcon name="solar:key-bold" :size="15" />
                  <span>{{ connectUnavailableReason(selectedCfg) }}</span>
                </div>
                <p v-else class="am-hint">Prefer a token? Paste it in step 2 instead — OAuth stays optional.</p>
              </div>

              <!-- step 2: configure -->
              <div class="am-step">
                <h4 class="am-step-title"><span class="am-step-n">2</span> Details <span class="muted">optional — OAuth already filled the secret</span></h4>
                <div class="am-fields">
                  <label class="am-field">
                    <span class="am-field-label">Display name</span>
                    <input v-model="draft(selectedCfg!).name" class="am-input" placeholder="Display name" autocomplete="off" />
                  </label>
                  <label v-for="f in fieldsFor(selectedCfg.backendType)" :key="f.key" class="am-field" :class="{ grow: f.grow }">
                    <span class="am-field-label">{{ f.label }}</span>
                    <span class="am-input-wrap" v-if="f.secret">
                      <input v-model="(draft(selectedCfg!) as any)[f.key]" class="am-input" :type="showTokens[selectedCfg.id] ? 'text' : 'password'" :placeholder="f.placeholder" autocomplete="off" />
                      <button class="am-icon-btn" type="button" @click="showTokens[selectedCfg.id] = !showTokens[selectedCfg.id]" :title="showTokens[selectedCfg.id] ? 'Hide' : 'Show'"><AppIcon :name="showTokens[selectedCfg.id] ? 'solar:eye-closed-bold' : 'solar:eye-bold'" :size="14" /></button>
                    </span>
                    <span class="am-input-wrap" v-else-if="f.key === 'basePath' && selectedCfg.backendType === 'local'">
                      <input v-model="(draft(selectedCfg!) as any)[f.key]" class="am-input" :placeholder="f.placeholder" autocomplete="off" />
                      <button v-if="localDirSupported" class="am-btn xs" type="button" title="Pick a folder with the browser — fills the path automatically" @click="pickLocalDir(selectedCfg!)">Browse…</button>
                    </span>
                    <input v-else v-model="(draft(selectedCfg!) as any)[f.key]" class="am-input" :placeholder="f.placeholder" autocomplete="off" />
                    <span v-if="f.hint" class="am-field-hint">{{ f.hint }}</span>
                    <span v-if="f.key === 'basePath' && selectedCfg.backendType === 'local'" class="am-field-hint">{{ localDirSupported ? 'Browse… fills this automatically (browser folder picker).' : 'No browser folder picker here — type the absolute path (desktop app).' }}
                      <button v-if="!localDirSupported" class="am-btn xs" type="button" @click="openInstall">Install options</button>
                    </span>
                  </label>
                </div>
                <p v-if="localPickMsg[selectedCfg.id]" class="am-note">{{ localPickMsg[selectedCfg.id] }}
                  <button v-if="isNoPickerNote(localPickMsg[selectedCfg.id])" class="am-btn xs" type="button" @click="openInstall">Install options</button>
                </p>
                <p class="am-hint">{{ authGuidance(selectedCfg.backendType) }}</p>

                <!-- git vault repo: pick an existing repo, or create one in step 5 -->
                <div v-if="selectedCfg.backendType === 'github' || selectedCfg.backendType === 'gitlab'" class="am-picker">
                  <div class="am-row between">
                    <strong class="small">Existing {{ selectedCfg.backendType === 'github' ? 'repos' : 'projects' }} ({{ repoChoices.length }})</strong>
                    <span class="am-row">
                      <button class="am-btn xs" type="button" :disabled="repoLoading" @click="loadRepoChoices(selectedCfg!)">{{ repoLoading ? 'Loading…' : repoChoices.length ? 'Refresh' : 'List mine' }}</button>
                      <button class="am-btn xs primary" type="button" :disabled="repoLoading || !repoSelected" @click="useRepoChoice(selectedCfg!)">Use selected</button>
                    </span>
                  </div>
                  <p class="am-hint">Select a vault repo instead of typing it — lists what your token/OAuth can see. Nothing yet? Create one in step 5.</p>
                  <select v-if="repoChoices.length" v-model="repoSelected" class="am-input" :aria-label="`Existing ${selectedCfg.backendType} repos`">
                    <option value="" disabled>Pick a repo…</option>
                    <option v-for="r in repoChoices" :key="r.value" :value="r.value">{{ r.label }}{{ r.isPrivate ? ' (private)' : '' }}</option>
                  </select>
                  <p v-if="repoMsg" class="am-note" :class="repoChoices.length ? '' : 'err'">{{ repoMsg }}</p>
                </div>

                <!-- drive folder: pick or create -->
                <div v-if="selectedCfg.backendType === 'googleDrive'" class="am-picker">
                  <div class="am-row between">
                    <strong class="small">Drive folders ({{ driveFolders.length }})</strong>
                    <span class="am-row">
                      <button class="am-btn xs" type="button" :disabled="driveLoading" @click="loadDriveFolders(selectedCfg!)">{{ driveLoading ? 'Loading…' : driveFolders.length ? 'Refresh' : 'List folders' }}</button>
                      <button class="am-btn xs primary" type="button" :disabled="driveLoading" @click="useDriveFolder(selectedCfg!, '')">Use app root</button>
                      <button class="am-btn xs primary" type="button" :disabled="driveLoading || !driveSelected" @click="useDriveFolder(selectedCfg!, driveSelected)">Use selected</button>
                    </span>
                  </div>
                  <p class="am-hint">Pick the folder this vault syncs into — or create one. Empty folder ID = app root.</p>
                  <select v-if="driveFolders.length" v-model="driveSelected" class="am-input" aria-label="Existing Drive folders">
                    <option value="" disabled>Pick a folder…</option>
                    <option v-for="f in driveFolders" :key="f.id" :value="f.id">{{ f.name }}</option>
                  </select>
                  <div class="am-row">
                    <input v-model="driveNewName" class="am-input" placeholder="New folder name" aria-label="New Drive folder name" style="max-width:220px;" />
                    <button class="am-btn xs" type="button" :disabled="driveLoading || !driveNewName.trim()" @click="createDriveFolderFlow(selectedCfg!)">{{ driveLoading ? '…' : 'Create folder' }}</button>
                  </div>
                  <p v-if="driveMsg" class="am-note" :class="driveFolders.length ? '' : 'err'">{{ driveMsg }}</p>
                </div>
              </div>

              <!-- sync behavior (encrypt / compress / placement) -->
              <div class="am-step">
                <h4 class="am-step-title"><span class="am-step-n">3</span> Sync behavior</h4>
                <div class="am-row wrap">
                  <label class="small"><input v-model="behavior(selectedCfg!).encryptBeforeUpload" type="checkbox" @change="saveBehavior(selectedCfg!)" /> Encrypt uploads</label>
                  <label class="small"><input v-model="behavior(selectedCfg!).compressBeforeUpload" type="checkbox" @change="saveBehavior(selectedCfg!)" /> Compress uploads</label>
                  <label class="small" title="Refuse the upload when no master passphrase exists, instead of uploading plaintext with a warning"><input v-model="behavior(selectedCfg!).requireEncryption" type="checkbox" @change="saveBehavior(selectedCfg!)" /> Require encryption</label>
                  <label class="small" title="Hash basenames into remote locators so providers never see real filenames"><input v-model="behavior(selectedCfg!).obfuscateNames" type="checkbox" @change="saveBehavior(selectedCfg!)" /> Hide filenames</label>
                </div>
                <div class="am-fields">
                  <label class="am-field">
                    <span class="am-field-label">Placement</span>
                    <select v-model="behavior(selectedCfg!).placement" class="am-input" aria-label="Chunk placement" @change="saveBehavior(selectedCfg!)">
                      <option value="whole">Whole file → this provider</option>
                      <option value="striped">Striped → all enabled providers</option>
                    </select>
                  </label>
                  <label class="am-field">
                    <span class="am-field-label">Parity replicas (striped)</span>
                    <input v-model.number="behavior(selectedCfg!).parity" class="am-input xs-num" type="number" min="0" max="4" step="1" aria-label="Striped parity replicas" @change="saveBehavior(selectedCfg!)" />
                  </label>
                </div>
                <p class="am-hint">Striped spreads 4 MiB chunks across every enabled provider (needs ≥2); parity adds replicas per chunk. Hiding filenames keeps the originals in the local sync record for restore.</p>
              </div>

              <!-- verify -->
              <div class="am-step">
                <h4 class="am-step-title"><span class="am-step-n">4</span> Verify &amp; save</h4>
                <div class="am-row">
                  <button class="am-btn sm primary" type="button" :disabled="saving === selectedCfg.id" @click="saveCreds(selectedCfg!)">{{ saving === selectedCfg.id ? 'Saving…' : 'Save & verify' }}</button>
                  <span class="muted small">Uses saved credentials — save first if you just pasted a token.</span>
                </div>
              </div>

              <!-- new private vault repos (github/gitlab only) -->
              <div v-if="selectedCfg.backendType === 'github' || selectedCfg.backendType === 'gitlab'" class="am-step">
                <h4 class="am-step-title"><span class="am-step-n">5</span> New private vault repos <span class="muted">optional</span></h4>
                <p class="am-hint">Create 1–8 private <code class="am-code">{{ selectedCfg.backendType }}</code> repos at once — each gets README + manifest + your encrypted <code class="am-code">vault.cybermanju</code> plus its own system disk, and all disks merge into one virtual volume ({{ vaultRepoCount }} × {{ vaultDiskMb }} MB).</p>
                <div class="am-fields">
                  <label class="am-field grow">
                    <span class="am-field-label">Base repo name (private)</span>
                    <input v-model="vaultRepoName" class="am-input" placeholder="cybermanju-vault" autocomplete="off" :aria-label="`Base name for new private repos on ${selectedCfg.backendType}`" />
                  </label>
                  <label class="am-field">
                    <span class="am-field-label">Repos (1–8)</span>
                    <input v-model.number="vaultRepoCount" class="am-input xs-num" type="number" min="1" max="8" step="1" aria-label="Number of repos to create" />
                  </label>
                  <label class="am-field">
                    <span class="am-field-label">Disk each (MB)</span>
                    <input v-model.number="vaultDiskMb" class="am-input xs-num" type="number" min="64" max="8192" step="64" aria-label="System disk size MB per repo" />
                  </label>
                </div>
                <div class="am-fields">
                  <label class="am-field grow">
                    <span class="am-field-label">Vault passphrase <span class="muted">(encrypts the seeded file + every disk)</span></span>
                    <input v-model="vaultPassphrase" class="am-input" type="password" placeholder="encrypts vault.cybermanju + disks" autocomplete="new-password" aria-label="Vault passphrase" />
                  </label>
                  <label class="am-field grow">
                    <span class="am-field-label">Token for repo creation <span class="muted">(or reuse saved)</span></span>
                    <input v-model="vaultRepoToken" class="am-input" type="password" placeholder="paste PAT — or leave empty" autocomplete="off" aria-label="Token for repo creation" />
                  </label>
                </div>
                <div class="am-row">
                  <label class="small muted"><input v-model="vaultIncludeFile" type="checkbox" /> Seed <code class="am-code">vault.cybermanju</code> (encrypted + compressed mirror in every repo)</label>
                  <label class="small muted"><input v-model="vaultCompress" type="checkbox" /> Compress sync uploads</label>
                </div>
                <div class="am-row">
                  <button class="am-btn sm primary" type="button" :disabled="vaultBusy" @click="createVaultRepo(selectedCfg!)">{{ vaultBusy ? 'Creating…' : vaultRepoCount > 1 ? `Create ${vaultRepoCount} repos + merge disks` : 'Create private repo + sync vault' }}</button>
                  <span v-if="vaultMsg" class="am-note" :class="vaultOk === false ? 'err' : vaultOk === true ? 'ok' : ''" style="margin:0;" role="status">{{ vaultMsg }}</span>
                </div>
                <ul v-if="vaultSteps.length" class="am-warnings-list">
                  <li v-for="(s, i) in vaultSteps.slice(-8)" :key="i" class="am-warning" :class="s.startsWith('failed') ? 'is-error' : 'is-info'">
                    <span>{{ s }}</span>
                  </li>
                </ul>
                <p v-if="vaultUrl" class="am-note">First repo live: <span class="am-code">{{ vaultUrl }}</span></p>
              </div>

              <!-- cloud disks on this provider (≠ the local vault file tab) -->
              <div class="am-step">
                <h4 class="am-step-title"><span class="am-step-n">6</span> Cloud disks on this provider ({{ disksFor(selectedCfg.id).length }})</h4>
                <p class="am-hint">Cloud-side compute + space. The <strong>Vault file</strong> tab next door is the local <code class="am-code">.cybermanju</code> file — different store, same merged volume.</p>
                <p v-if="disksFor(selectedCfg.id).length === 0" class="am-hint">No system disk yet — provision one below and this provider contributes space + compute to the merged volume.</p>
                <div v-for="d in disksFor(selectedCfg.id)" :key="d.id" class="am-disk">
                  <div class="am-row between">
                    <strong class="small">{{ d.name || d.id.slice(0, 8) }} · <span class="muted">{{ d.state }} / {{ d.health }}</span></strong>
                    <span class="am-row">
                      <button v-if="d.state !== 'attached'" class="am-btn xs primary" type="button" :disabled="diskBusy === d.id" @click="attachDisk(d.id, selectedCfg!.id)">Attach</button>
                      <button v-else class="am-btn xs" type="button" :disabled="diskBusy === d.id" @click="store.detachDisk(d.id)">Detach</button>
                      <button class="am-btn xs" type="button" :disabled="diskBusy === d.id" @click="store.checkDisk(d.id)">Check</button>
                    </span>
                  </div>
                  <div class="muted small truncate" :title="d.containerPath">FILE {{ d.containerPath }}</div>
                  <div class="am-mini"><div class="am-mini-fill" :style="{ width: `${diskPct(d.usedBytes, d.capacityBytes)}%` }"></div></div>
                  <div class="am-row between small">
                    <span>{{ humanBytes(d.usedBytes) }} / {{ humanBytes(d.capacityBytes) }}</span>
                    <span class="am-row">
                      <input v-model.number="resizeMb[d.id]" class="am-input xs-num" type="number" min="64" max="8192" step="64" :placeholder="String(Math.max(64, Math.round(d.capacityBytes / 1048576)))" :aria-label="`New size MB for ${d.name}`" />
                      <span class="muted">MB</span>
                      <button class="am-btn xs" type="button" :disabled="diskBusy === d.id" @click="applyResize(d.id)">Apply size</button>
                    </span>
                  </div>
                </div>
                <div class="am-newdisk">
                  <label class="small muted">{{ disksFor(selectedCfg.id).length === 0 ? 'Provision system disk' : 'New disk' }} — {{ newDiskMb[selectedCfg.id] ?? 512 }} MB</label>
                  <input v-model.number="newDiskMb[selectedCfg.id]" class="am-slider" type="range" min="64" max="8192" step="64" :aria-label="`New disk size for ${selectedCfg.name || selectedCfg.backendType}`" :style="{ '--p': (((newDiskMb[selectedCfg.id] ?? 512) - 64) / 8128 * 100).toFixed(1) + '%' }" />
                  <input v-model="newDiskPass[selectedCfg.id]" class="am-input" type="password" placeholder="Passphrase (also unlocks)" autocomplete="off" :aria-label="`Passphrase for new disk on ${selectedCfg.name || selectedCfg.backendType}`" />
                  <button class="am-btn sm primary" type="button" :disabled="diskBusy === selectedCfg.id" @click="createDisk(selectedCfg!.id)">{{ diskBusy === selectedCfg.id ? 'Creating…' : disksFor(selectedCfg.id).length === 0 ? 'Provision system disk' : 'Create & attach' }}</button>
                  <p v-if="diskMsg[selectedCfg.id]" class="am-note" role="status">{{ diskMsg[selectedCfg.id] }}</p>
                  <p v-else-if="selectedCfg.backendType === 'googleDrive'" class="am-hint">Also creates a <code class="am-code">cybermanju-disks/&lt;disk&gt;</code> Drive folder with its <code class="am-code">.cybermanju</code> files.</p>
                  <p v-else-if="selectedCfg.backendType === 'github' || selectedCfg.backendType === 'gitlab'" class="am-hint">Missing repo? A private one is created first, then seeded with the disk's <code class="am-code">.cybermanju</code> files.</p>
                </div>
              </div>
            </article>
          </div><!-- /detail -->
          </div><!-- /work -->
        </div><!-- /conn -->
      </section>

      <!-- ══ VAULT FILE (local .cybermanju file — cloud disks live per-provider in Connections) ══ -->
      <section v-if="activeTab === 'vault'" id="am-panel-vault" class="am-section" role="tabpanel" aria-labelledby="am-tab-vault" tabindex="0">
        <p class="am-hint" style="margin:0;">Local file on this machine — cloud disks are managed per-provider in Connections step 6 and merge into the same volume.</p>
        <div class="am-card" :class="{ 'is-attached': disk.attached }">
          <div class="am-card-head">
            <div class="am-card-head-left">
              <ProviderLogo provider="local" :size="34" />
              <div>
                <h3 class="am-card-title">{{ disk.attached ? disk.name : 'No vault file attached' }}</h3>
                <p class="am-hint">
                  <template v-if="disk.attached">
                    <span class="am-status sm" :class="disk.dirty ? 'is-warn' : 'is-ok'">{{ disk.dirty ? 'Unsaved changes' : `Saved ${timeOf(disk.savedAt)}` }}</span>
                    <span class="muted"> · {{ humanBytes(disk.savedBytes) }}</span>
                  </template>
                  <template v-else>The vault lives in this browser session only. Create or open a <code class="am-code">.cybermanju</code> file to keep it on your disk.</template>
                </p>
              </div>
            </div>
            <span class="am-status sm" :class="disk.bound ? 'is-ok' : ''">{{ disk.bound ? 'BOUND' : 'SESSION ONLY' }}</span>
          </div>

          <div class="am-btn-grid">
            <button class="am-btn primary" type="button" :disabled="disk.busy" @click="openDiskFile"><AppIcon name="solar:folder-open-bold" :size="13" /> {{ disk.busy ? '…' : 'Open' }}</button>
            <button class="am-btn primary" type="button" :disabled="disk.busy" @click="createDiskFile"><AppIcon name="solar:add-bold" :size="13" /> Create</button>
            <button class="am-btn" type="button" :disabled="disk.busy || !disk.bound" @click="saveDiskFile"><AppIcon name="solar:diskette-bold" :size="13" /> Save now</button>
            <button class="am-btn" type="button" :disabled="disk.busy" @click="exportDiskFile"><AppIcon name="solar:download-bold" :size="13" /> Export</button>
            <button class="am-btn" type="button" :disabled="disk.busy" @click="pickImport"><AppIcon name="solar:upload-bold" :size="13" /> Import</button>
            <button v-if="disk.bound" class="am-btn danger" type="button" :disabled="disk.busy" @click="detachDiskFile">Detach</button>
          </div>
          <input ref="importInput" type="file" accept=".cybermanju,application/octet-stream" class="am-hidden" @change="onImportFile" />

          <div class="am-fields">
            <label class="am-field grow">
              <span class="am-field-label">Passphrase <span class="muted">(optional — encrypts the file, never stored)</span></span>
              <span class="am-input-wrap">
                <input
                  v-model="diskPassphrase"
                  class="am-input"
                  :type="showDiskPass ? 'text' : 'password'"
                  placeholder="Passphrase for create / unlock"
                  autocomplete="new-password"
                  aria-label="File passphrase"
                />
                <button class="am-icon-btn" type="button" :title="showDiskPass ? 'Hide' : 'Show'" @click="showDiskPass = !showDiskPass">
                  <AppIcon :name="showDiskPass ? 'solar:eye-closed-bold' : 'solar:eye-bold'" :size="14" />
                </button>
              </span>
            </label>
            <label class="am-field">
              <span class="am-field-label">Size</span>
              <span class="am-input" aria-live="polite">{{ disk.attached ? humanBytes(disk.savedBytes) : '—' }}</span>
              <span class="am-field-hint">The file grows automatically with your data — no size to pick. Cloud disks are sized per-provider in Connections step 6.</span>
            </label>
          </div>
          <p class="am-hint">{{ disk.supported ? 'File System Access API available — saves go straight to your disk.' : 'No File System Access API in this browser — use Export / Import instead.' }}</p>

          <div v-if="disk.needsPassphrase" class="am-banner warn">
            <AppIcon name="solar:lock-bold" :size="15" />
            <span><strong>{{ disk.name }}</strong> is encrypted — enter its passphrase to open it.</span>
            <button class="am-btn sm primary" type="button" :disabled="disk.busy" @click="unlockDisk">Unlock &amp; open</button>
          </div>
          <div v-else-if="disk.needsPermission" class="am-banner info">
            <AppIcon name="solar:cursor-bold" :size="15" />
            <span><strong>{{ disk.name }}</strong> is remembered — the browser wants one click to re-open it.</span>
            <button class="am-btn sm primary" type="button" :disabled="disk.busy" @click="unlockDisk">Allow &amp; open</button>
          </div>

          <p v-if="disk.lastMessage" class="am-note">{{ disk.lastMessage }}</p>
          <p v-if="disk.lastError" class="am-note err">{{ disk.lastError }}</p>
          <p v-if="disk.bound && disk.dirty" class="am-note warn">Changes write back automatically — Save now forces it immediately.</p>
        </div>
      </section>

      <!-- ══ LOCAL USERS (merged user management) ══ -->
      <section v-if="activeTab === 'users'" id="am-panel-users" class="am-section" role="tabpanel" aria-labelledby="am-tab-users" tabindex="0">
        <div class="am-card">
          <h3 class="am-card-title">Registered users ({{ store.users.length }})</h3>
          <p class="am-hint">Per-file username + password auth with Argon2 hashing. Roles: admin, user, viewer. This is the local user list — cloud sign-in lives at the top of the Connections tab.</p>
          <p v-if="!store.users.length" class="am-hint">No users registered yet.</p>
          <div v-for="user in store.users" :key="user.id" class="am-identity-row">
            <div class="am-identity-meta">
              <strong>{{ user.username }}</strong>
              <span class="muted">{{ user.role }} · {{ user.isActive ? 'active' : 'inactive' }}</span>
            </div>
            <span style="flex:1" />
            <button class="am-btn xs" type="button" :title="`Toggle ${user.username} role`" @click="handleUserRole(user.id, user.role === 'admin' ? 'user' : 'admin')">Make {{ user.role === 'admin' ? 'user' : 'admin' }}</button>
            <button class="am-btn xs danger" type="button" :title="`Delete ${user.username}`" @click="handleUserDelete(user.id)">Delete</button>
          </div>
        </div>
        <div class="am-card">
          <h3 class="am-card-title">Create user</h3>
          <div class="am-row">
            <input v-model="newUsername" class="am-input" placeholder="Username" aria-label="Username" @keyup.enter="handleUserCreate" />
            <input v-model="newPassword" class="am-input" type="password" placeholder="Password" aria-label="Password" autocomplete="new-password" @keyup.enter="handleUserCreate" />
            <select v-model="newRole" class="am-input" aria-label="Role">
              <option value="user">USER</option>
              <option value="admin">ADMIN</option>
              <option value="viewer">VIEWER</option>
            </select>
            <button class="am-btn sm primary" type="button" @click="handleUserCreate">Create</button>
          </div>
        </div>
      </section>
    </main>

    <!-- ── warnings footer ────────────────────────────────── -->
    <footer class="am-warnings" aria-live="polite">
      <div class="am-warnings-head">
        <AppIcon :name="warnings.some(w => w.level === 'error') ? 'solar:danger-triangle-bold' : warnings.length ? 'solar:info-circle-bold' : 'solar:check-circle-bold'" :size="14" />
        <strong>Warnings</strong>
        <span class="am-tab-count" :class="{ 'is-err': warnings.some(w => w.level === 'error') }">{{ warnings.length }}</span>
      </div>
      <ul v-if="warnings.length" class="am-warnings-list">
        <li v-for="(w, i) in warnings" :key="i" class="am-warning" :class="`is-${w.level}`">
          <AppIcon :name="w.level === 'error' ? 'solar:danger-triangle-bold' : 'solar:info-circle-bold'" :size="13" />
          <span>{{ w.text }}</span>
        </li>
      </ul>
      <p v-else class="am-warnings-empty">All clear — vault file bound, providers verified, session healthy.</p>
    </footer>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import ProviderLogo from '@/components/ProviderLogo.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useWindowManager } from '@/composables/useWindowManager'
import { isStaticHost, isTauri, isTauriMobile } from '@/composables/useTauri'
import { wasmDbBackend } from '@/composables/useWasmBackend'
import {
  connectedAccounts,
  forgetConnectedAccount,
  getPendingOAuthConfig,
  identity,
  installMobileOAuthDeepLinks,
  refreshIdentity,
  revokeIdentitySession,
  revokeProviderGrant,
  setPendingOAuthConfig,
  signInWithPopup,
  signOutIdentity,
  startSupabaseOAuth,
  supabaseConfigured,
  supabaseProviderFor,
  supabaseSession,
  supabaseSessionProvider,
  takeProviderTokenStash,
  type ConnectedAccount,
  type OAuthBackend,
} from '@/composables/useSupabase'
import {
  createCyberManjuFile,
  detachCyberManjuFile,
  disk,
  diskSupported,
  exportCyberManjuFile,
  importCyberManjuFile,
  openCyberManjuFile,
  reattachCyberManjuDisk,
  saveCyberManjuFile,
} from '@/composables/useCyberManjuFile'
import { SYNC_BACKEND_INFO, describeSyncError, isOauthCapable } from '@/types'
import type { DiskRow, SyncBackendType, SyncConfig } from '@/types'
import { humanBytes, diskPct } from '@/utils/format'
import { requestInstallWindow } from '@/utils/installLinks'
import {
  authGuidance,
  backendLabel,
  blankCredentialDraft,
  draftToSave,
  overlayDraft,
  refreshDraftFromSaved,
  syncConfigDefaults,
  tokenLabel,
} from '@/utils/providers'
import type { CredentialDraft } from '@/utils/providers'
import { pollUntilTrue } from '@/utils/poll'
import { openExternalUrl } from '@/utils/openExternal'

const store = useAppStore()
const wm = useWindowManager()

/** Roving-tabindex arrow-key navigation for the tab bar. */
function onTabsKey(e: KeyboardEvent) {
  if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) return
  const order = TABS.map(t => t.id)
  let i = order.indexOf(activeTab.value)
  if (e.key === 'ArrowRight') i = (i + 1) % order.length
  else if (e.key === 'ArrowLeft') i = (i - 1 + order.length) % order.length
  else if (e.key === 'Home') i = 0
  else i = order.length - 1
  e.preventDefault()
  activeTab.value = order[i]
  const el = (e.currentTarget as HTMLElement).querySelector<HTMLElement>(`#am-tab-${order[i]}`)
  el?.focus()
}

/**
 * Static WASM pack (GitHub Pages): no dashboard behind the page. Accounts,
 * providers, disks, users and files run offline against a real redb
 * `cybermanju.db` in this browser (OPFS-durable, in-memory fallback) via
 * the DB worker. Only provider *network* calls need the server.
 */
const staticHost = isStaticHost()
const dbBackend = ref<string | null>(null)

const refreshing = ref(false)
const saving = ref<string | null>(null)
const probing = ref<Set<string>>(new Set())
const diskBusy = ref<string | null>(null)
const wizBusy = ref(false)
const wizMsg = ref('')
const wizOk = ref<boolean | null>(null)
const provFilter = ref('')
// Unified OAuth-connect UI state (one button per provider: Supabase broker
// on static hosts, dashboard PKCE otherwise). The per-transport refs below
// stay as the cancellation handles; visible status lives in connectMsg/Url.
const connectBusy = ref<string | null>(null)
const connectAbort = ref<AbortController | null>(null)
const connectMsg = ref<Record<string, string>>({})
const connectUrl = ref<Record<string, string>>({})
const mobileOAuthErrors = ref<Record<string, string>>({})
const oauthAbort = ref<AbortController | null>(null)

const quotaMsg = ref<Record<string, string>>({})
const authState = ref<Record<string, { ok: boolean | null; detail: string }>>({})
const sbAbort = ref<AbortController | null>(null)
const sbConfigured = computed(() => supabaseConfigured())

/** Jump to Settings → OAuth broker (the deep-link the Settings card listens for). */
function openBrokerSettings() {
  try {
    wm.open('settings')
  } catch {
    // Window manager unavailable (embedded contexts) — the banner text
    // already names the destination.
  }
  // SettingsPage mounts on open; let it mount before focusing the card.
  setTimeout(() => {
    try {
      window.dispatchEvent(new CustomEvent('cybermanju:settings-focus', { detail: 'oauth-broker' }))
    } catch {
      // Non-DOM context — nothing to focus.
    }
  }, 350)
}

// ── new interactive UI state ──────────────────────────────────
// Single "Connections" tab merges the old Sign-in + Providers tabs: the
// identity (who you are) sits on top of the provider list (what is
// connected) so OAuth sign-in and per-provider OAuth connect share one
// broker status, one button language and one troubleshooting path.
type TabId = 'connections' | 'vault' | 'users'
const TABS: Array<{ id: TabId; label: string; icon: string }> = [
  { id: 'connections', label: 'Connections', icon: 'solar:cloud-bold' },
  { id: 'vault', label: 'Vault file', icon: 'solar:diskette-bold' },
  { id: 'users', label: 'Users', icon: 'solar:users-group-rounded-bold' },
]
const activeTab = ref<TabId>('connections')

// Merged windows: `users` opens here on the users tab. Legacy deep-links
// (`signin`, `providers`) steer to `connections`.
const props = defineProps<{ tab?: string }>()
function normalizeTab(t: unknown): TabId | null {
  if (t === 'users' || t === 'vault' || t === 'connections') return t
  if (t === 'providers' || t === 'signin') return 'connections'
  return null
}
{
  const initial = normalizeTab(props.tab)
  if (initial) activeTab.value = initial
}
watch(() => props.tab, (t) => {
  const next = normalizeTab(t)
  if (next) activeTab.value = next
})
watch(activeTab, (t) => { if (t === 'users') void store.fetchUsers() })
const selectedId = ref<string | null>(null)
const showTokens = ref<Record<string, boolean>>({})
const showDiskPass = ref(false)
const showWizToken = ref(false)
const wizOpen = ref(false)

const BACKEND_ORDER: SyncBackendType[] = ['local', 'github', 'gitlab', 'googleDrive']

function logoKindFor(b: SyncBackendType | string): string {
  return b === 'local' ? 'local' : String(b)
}

function shortName(b: SyncBackendType): string {
  const n = backendLabel(b)
  return n.replace('Google ', '').replace('Local Storage', 'Local')
}

function backendBlurb(b: SyncBackendType): string {
  return SYNC_BACKEND_INFO[b]?.description ?? ''
}

const LOGIN_CARDS: Array<{ id: OAuthBackend; label: string; logo: string; sub: string }> = [
  { id: 'google', label: 'Google', logo: 'google', sub: 'Drive OAuth' },
  { id: 'github', label: 'GitHub', logo: 'github', sub: 'Repo Contents API' },
  { id: 'gitlab', label: 'GitLab', logo: 'gitlab', sub: 'Projects API v4' },
]
const signInBusy = ref<OAuthBackend | null>(null)
const signInMsg = ref('')
const signingOut = ref(false)
/** Left icon rail: hover peeks the session flyout, pin keeps it open. */
const railPinned = ref(false)
/**
 * OAuth revoke state: `revoking` covers the identity card + remembered
 * accounts (session-wide), `revokingCfg` the per-provider "Revoke" button
 * in step 1. Revoke = provider-side grant (best-effort) + session sign-out
 * + saved-secret clear, so auto-login cannot reuse the token afterwards.
 */
const revoking = ref(false)
const revokingCfg = ref<string | null>(null)
/** Collapse the switch/add-account cards once signed in (compact Connections). */
const accountsOpen = ref(false)
const diskPassphrase = ref('')
const importInput = ref<HTMLInputElement | null>(null)

const newDiskMb = ref<Record<string, number>>({})
const newDiskPass = ref<Record<string, string>>({})
const resizeMb = ref<Record<string, number>>({})
/** Per-provider outcome of the last disk provisioning (remote folder/repo + file). */
const diskMsg = ref<Record<string, string>>({})

// ── private vault repo provisioning (github/gitlab) ──────────────
const vaultRepoName = ref('cybermanju-vault')
const vaultRepoToken = ref('')
const vaultRepoCount = ref(1)
const vaultDiskMb = ref(512)
const vaultPassphrase = ref('')
const vaultIncludeFile = ref(true)
const vaultCompress = ref(true)
const vaultBusy = ref(false)
const vaultMsg = ref('')
const vaultOk = ref<boolean | null>(null)
const vaultUrl = ref('')
const vaultSteps = ref<string[]>([])

const drafts = reactive<Record<string, CredentialDraft>>({})

// ── OAuth sign-in → provider-connection sync ────────────────────
// Signing in at the top used to leave "Provider connections" at 0: the
// identity list (who you are) and the sync-config list (what is connected)
// were separate stores. Every sign-in now ensures its matching provider
// row exists, and each remembered account offers a one-click Connect.
const ensuringProvider = ref(false)
const ensureMsg = ref('')
const ensureOk = ref<boolean | null>(null)

function backendForOAuth(p: OAuthBackend | string): SyncBackendType {
  if (p === 'google') return 'googleDrive'
  if (p === 'gitlab') return 'gitlab'
  return 'github'
}

function backendForAccountProvider(p: string): SyncBackendType | null {
  if (p === 'google' || p === 'googleDrive') return 'googleDrive'
  if (p === 'github') return 'github'
  if (p === 'gitlab') return 'gitlab'
  return null
}

const missingProviderBackends = computed<SyncBackendType[]>(() => {
  const have = new Set(store.syncConfigs.map(c => c.backendType))
  const missing = new Set<SyncBackendType>()
  for (const acc of connectedAccounts.value) {
    const b = backendForAccountProvider(acc.provider)
    if (b && !have.has(b)) missing.add(b)
  }
  if (identity.value) {
    const b = backendForAccountProvider(identity.value.provider)
    if (b && !have.has(b)) missing.add(b)
  }
  return [...missing]
})

function needsProviderFor(provider: string): boolean {
  const b = backendForAccountProvider(provider)
  if (!b) return false
  return !store.syncConfigs.some(c => c.backendType === b)
}

function providerBadge(acc: ConnectedAccount): string {
  const b = backendForAccountProvider(acc.provider)
  if (!b) return acc.provider
  return store.syncConfigs.some(c => c.backendType === b) ? 'Linked' : 'No provider'
}

function providerBadgeTone(acc: ConnectedAccount): string {
  const b = backendForAccountProvider(acc.provider)
  if (!b) return ''
  return store.syncConfigs.some(c => c.backendType === b) ? 'is-ok' : 'is-warn'
}

async function ensureProviderForBackend(
  backend: SyncBackendType,
  opts: { token?: string; displayName?: string } = {},
): Promise<SyncConfig | null> {
  const existing = store.syncConfigs.find(c => c.backendType === backend)
  if (existing) {
    selectedId.value = existing.id
    // Top-up a missing secret: an OAuth token that arrives after the row
    // was created (sign-in first, connect later) should not be dropped.
    // All callers here are explicit user actions (sign-in / Connect /
    // Create), so a fresh session token refreshes the stored secret.
    const token = (opts.token ?? '').trim()
    if (token && !(existing as SyncConfig).token) {
      const topped = await store.saveSyncConfig({ ...existing, token })
      if (topped) {
        const r = await store.probeSyncConnection({ ...topped, token })
        authState.value[topped.id] = { ok: r.ok, detail: r.detail }
        return topped
      }
    }
    return existing
  }
  const saved = await store.saveSyncConfig({
    ...syncConfigDefaults(),
    id: '',
    backendType: backend,
    name: opts.displayName || `${backendLabel(backend)} (${new Date().toLocaleDateString()})`,
    token: (opts.token ?? '').trim() || undefined,
  } as SyncConfig)
  if (!saved) {
    ensureMsg.value = 'Could not create the provider connection — retry.'
    ensureOk.value = false
    return null
  }
  selectedId.value = saved.id
  authState.value[saved.id] = { ok: null, detail: '' }
  const probeCfg = (opts.token ?? '').trim() ? { ...saved, token: opts.token!.trim() } : saved
  try {
    const r = await store.probeSyncConnection(probeCfg)
    authState.value[saved.id] = { ok: r.ok, detail: r.detail }
  } catch {
    // Probe failure must not delete the row — the token may still be fine
    // on a CORS-blocked static host; the card shows UNREACHABLE instead.
  }
  await store.fetchSyncConfigs().catch(() => {})
  return saved
}

/** Best-effort provider token for `backend` from the live Supabase session. */
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

async function ensureFromSignIn(provider: OAuthBackend, whoName: string): Promise<void> {
  ensuringProvider.value = true
  ensureMsg.value = ''
  ensureOk.value = null
  try {
    const backend = backendForOAuth(provider)
    const token = await sessionTokenFor(backend)
    const saved = await ensureProviderForBackend(backend, {
      token,
      displayName: `${backendLabel(backend)} — ${whoName}`,
    })
    if (saved) {
      ensureMsg.value = token
        ? `${backendLabel(backend)} connection ready — verified below.`
        : `${backendLabel(backend)} connection created — press Connect on its card to finish OAuth.`
      ensureOk.value = true
    }
  } finally {
    ensuringProvider.value = false
  }
}

async function connectAccountToProvider(acc: ConnectedAccount): Promise<void> {
  const b = backendForAccountProvider(acc.provider)
  if (!b) return
  ensuringProvider.value = true
  ensureMsg.value = ''
  ensureOk.value = null
  try {
    const token = await sessionTokenFor(b)
    const saved = await ensureProviderForBackend(b, { token, displayName: `${backendLabel(b)} — ${acc.name}` })
    if (saved) {
      ensureMsg.value = `${backendLabel(b)} connection ready for ${acc.name}.`
      ensureOk.value = true
      store.notifySuccess?.(`${backendLabel(b)} connection ready`)
    }
  } finally {
    ensuringProvider.value = false
  }
}

async function ensureMissingFromAccounts(): Promise<void> {
  if (!missingProviderBackends.value.length) return
  ensuringProvider.value = true
  ensureMsg.value = ''
  ensureOk.value = null
  try {
    for (const b of missingProviderBackends.value) {
      const token = await sessionTokenFor(b)
      await ensureProviderForBackend(b, { token, displayName: backendLabel(b) })
    }
    ensureMsg.value = 'Provider connections created — finish OAuth on each card if needed.'
    ensureOk.value = true
  } finally {
    ensuringProvider.value = false
  }
}

// ── repo / folder pickers + local dir picker ────────────────────
const repoChoices = ref<Array<{ value: string; label: string; url: string; isPrivate: boolean }>>([])
const repoLoading = ref(false)
const repoMsg = ref('')
const repoSelected = ref('')
const driveFolders = ref<Array<{ id: string; name: string }>>([])
const driveLoading = ref(false)
const driveMsg = ref('')
const driveSelected = ref('')
const driveNewName = ref('cybermanju-vault')
const localPickMsg = ref<Record<string, string>>({})

/** A picker note is a missing-picker refusal — offer the installer next to it. */
function isNoPickerNote(text: string): boolean {
  return /no browser folder picker/i.test(String(text ?? ''))
}
/** Open the Install window (native builds per platform, App × CLI). */
function openInstall() {
  requestInstallWindow('host-unsupported')
}

const localDirSupported = computed(() => {
  try {
    return typeof (window as unknown as { showDirectoryPicker?: unknown }).showDirectoryPicker === 'function'
  } catch {
    return false
  }
})

/** Token precedence for browsing: typed draft → saved row → live session. */
async function resolveProviderToken(cfg: SyncConfig): Promise<string> {
  const d = drafts[cfg.id]
  if (d?.token.trim()) return d.token.trim()
  const saved = (cfg as SyncConfig).token
  if (typeof saved === 'string' && saved.trim()) return saved.trim()
  return sessionTokenFor(cfg.backendType)
}

async function loadRepoChoices(cfg: SyncConfig): Promise<void> {
  repoLoading.value = true
  repoMsg.value = ''
  try {
    const token = await resolveProviderToken(cfg)
    if (!token) {
      repoMsg.value = 'Connect with OAuth (step 1) or paste a token first — the list needs it.'
      repoChoices.value = []
      return
    }
    const { listGithubRepos, listGitlabProjects } = await import('@/utils/gitProvision')
    const instanceUrl = drafts[cfg.id]?.basePath.trim() || cfg.basePath || undefined
    const list = cfg.backendType === 'gitlab'
      ? await listGitlabProjects(token, instanceUrl)
      : await listGithubRepos(token)
    repoChoices.value = list
    const current = drafts[cfg.id]?.repoName.trim() || cfg.repoName || ''
    repoSelected.value = list.some(r => r.value === current) ? current : (list[0]?.value ?? '')
    repoMsg.value = list.length
      ? `${list.length} found — pick one and press Use selected.`
      : 'No repos visible to this token — check scopes (GitHub: repo, GitLab: api), or create one in step 5.'
  } catch (e) {
    repoMsg.value = e instanceof Error ? e.message : String(e)
    repoChoices.value = []
  } finally {
    repoLoading.value = false
  }
}

async function useRepoChoice(cfg: SyncConfig): Promise<void> {
  if (!repoSelected.value) return
  draft(cfg).repoName = repoSelected.value
  draft(cfg).branch = draft(cfg).branch || cfg.branch || 'main'
  await saveCreds(cfg)
  repoMsg.value = `Using ${repoSelected.value} — saved + verified.`
}

async function loadDriveFolders(cfg: SyncConfig): Promise<void> {
  driveLoading.value = true
  driveMsg.value = ''
  try {
    const token = await resolveProviderToken(cfg)
    if (!token) {
      driveMsg.value = 'Connect with OAuth (step 1) first — Drive lists need it.'
      driveFolders.value = []
      return
    }
    const { listDriveFolders } = await import('@/utils/gitProvision')
    const folders = await listDriveFolders(token)
    driveFolders.value = folders
    const current = drafts[cfg.id]?.folderId.trim() || cfg.folderId || ''
    driveSelected.value = folders.some(f => f.id === current) ? current : ''
    driveMsg.value = folders.length
      ? `${folders.length} folders — pick one, or create a new one below.`
      : 'No folders yet — create one below, or use the app root.'
  } catch (e) {
    driveMsg.value = e instanceof Error ? e.message : String(e)
    driveFolders.value = []
  } finally {
    driveLoading.value = false
  }
}

async function useDriveFolder(cfg: SyncConfig, folderId: string): Promise<void> {
  draft(cfg).folderId = folderId.trim()
  if (folderId.trim()) {
    const hit = driveFolders.value.find(f => f.id === folderId.trim())
    if (hit) driveSelected.value = hit.id
  } else {
    driveSelected.value = ''
  }
  await saveCreds(cfg)
  driveMsg.value = folderId.trim()
    ? `Using Drive folder ${driveFolders.value.find(f => f.id === folderId.trim())?.name ?? folderId.trim()} — saved + verified.`
    : 'Using the Drive app root — saved + verified.'
}

async function createDriveFolderFlow(cfg: SyncConfig): Promise<void> {
  const name = driveNewName.value.trim()
  if (!name) return
  driveLoading.value = true
  driveMsg.value = ''
  try {
    const token = await resolveProviderToken(cfg)
    if (!token) {
      driveMsg.value = 'Connect with OAuth (step 1) first — creation needs it.'
      return
    }
    const { createDriveFolder } = await import('@/utils/gitProvision')
    const created = await createDriveFolder(token, name, driveSelected.value || undefined)
    await loadDriveFolders(cfg)
    driveSelected.value = created.id
    await useDriveFolder(cfg, created.id)
  } catch (e) {
    driveMsg.value = e instanceof Error ? e.message : String(e)
  } finally {
    driveLoading.value = false
  }
}

/**
 * Browser folder picker for `local` providers: fills the path from the
 * File System Access API (`showDirectoryPicker`) where available. Browsers
 * never reveal the absolute disk path, so we store `/<name>` plus the
 * directory handle (IndexedDB) for future reads — and say so in the note.
 */
async function pickLocalDir(cfg: SyncConfig): Promise<void> {
  const w = window as unknown as { showDirectoryPicker?: () => Promise<{ name: string }> }
  if (typeof w.showDirectoryPicker !== 'function') {
    localPickMsg.value[cfg.id] = 'No browser folder picker here — type the absolute path (desktop app).'
    // Picker-less browser — open install options alongside the note.
    requestInstallWindow('host-unsupported')
    return
  }
  try {
    const dir = await w.showDirectoryPicker()
    const name = String(dir?.name ?? '').trim()
    if (!name) return
    draft(cfg).basePath = `/${name}`
    localPickMsg.value[cfg.id] = `Picked “${name}” — path auto-filled. Save & verify to keep it.`
    try {
      const { rememberLocalDir, localDirKey } = await import('@/utils/localDir')
      await rememberLocalDir(localDirKey(cfg.id), dir)
    } catch {
      // Handle persistence is best-effort; the path is what sync uses.
    }
    await saveCreds(cfg)
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') return
    localPickMsg.value[cfg.id] = e instanceof Error ? e.message : String(e)
  }
}

async function pickWizLocalDir(): Promise<void> {
  const w = window as unknown as { showDirectoryPicker?: () => Promise<{ name: string }> }
  if (typeof w.showDirectoryPicker !== 'function') {
    wizMsg.value = 'No browser folder picker here — type the absolute path (desktop app).'
    wizOk.value = false
    // Picker-less browser — open install options alongside the note.
    requestInstallWindow('host-unsupported')
    return
  }
  try {
    const dir = await w.showDirectoryPicker()
    const name = String(dir?.name ?? '').trim()
    if (name) {
      wiz.basePath = `/${name}`
      wizMsg.value = `Picked “${name}” — path auto-filled.`
      wizOk.value = null
    }
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') return
    wizMsg.value = e instanceof Error ? e.message : String(e)
    wizOk.value = false
  }
}

// ── local users (merged user management: Argon2 username+password, roles) ──
const newUsername = ref('')
const newPassword = ref('')
const newRole = ref('user')

async function handleUserCreate() {
  if (!newUsername.value.trim() || !newPassword.value.trim()) return
  await store.createUser(newUsername.value.trim(), newPassword.value.trim(), newRole.value)
  newUsername.value = ''
  newPassword.value = ''
  newRole.value = 'user'
}

async function handleUserDelete(userId: string) {
  const u = store.users.find(x => x.id === userId)
  if (!window.confirm(`Delete user "${u?.username ?? userId}"?`)) return
  await store.deleteUser(userId)
}

async function handleUserRole(userId: string, role: string) {
  await store.updateUserRole(userId, role)
}

const wiz = reactive({
  backendType: 'local' as SyncBackendType,
  name: '',
  repoName: '',
  branch: 'main',
  token: '',
  folderId: '',
  basePath: '',
})
const wizardSessionMatchesProvider = computed(() =>
  !!identity.value && backendForAccountProvider(identity.value.provider) === wiz.backendType,
)
const wizardCanSave = computed(() =>
  !isOauthCapable(wiz.backendType) || !!wiz.token.trim() || wizardSessionMatchesProvider.value,
)

/** Restore the add-provider wizard to pristine defaults. */
function resetWizard() {
  wiz.backendType = 'local'
  wiz.name = ''
  wiz.repoName = ''
  wiz.branch = 'main'
  wiz.token = ''
  wiz.folderId = ''
  wiz.basePath = ''
  wizMsg.value = ''
  wizOk.value = null
  showWizToken.value = false
}

const filteredConfigs = computed(() => {
  const q = provFilter.value.trim().toLowerCase()
  if (!q) return store.syncConfigs
  return store.syncConfigs.filter(c => {
    const hay = `${c.name ?? ''} ${backendLabel(c.backendType)} ${c.backendType}`.toLowerCase()
    return hay.includes(q)
  })
})

/** True when the in-memory draft differs from the saved row (or holds an unsaved token). */
function isDraftDirty(cfg: SyncConfig): boolean {
  const d = drafts[cfg.id]
  if (!d) return false
  if (d.token.trim() !== '') return true
  const norm = (v: string | undefined) => (v ?? '').trim()
  return (
    norm(d.name) !== norm(cfg.name) ||
    norm(d.repoName) !== norm(cfg.repoName) ||
    norm(d.branch || 'main') !== norm(cfg.branch || 'main') ||
    norm(d.folderId) !== norm(cfg.folderId) ||
    norm(d.basePath) !== norm(cfg.basePath)
  )
}

const df = computed(() => store.osDf)

// In the static build the OS `df` covers the terminal's virtual volume, so
// the strip instead sums the real attached `.cybermanju` disks — the same
// merge the server reports, computed client-side from the same rows.
const diskVolume = computed(() => {
  const attached = store.disks.filter(d => d.state === 'attached')
  const total = attached.reduce((s, d) => s + (d.capacityBytes || 0), 0)
  const used = attached.reduce((s, d) => s + (d.usedBytes || 0), 0)
  return {
    totalBytes: total,
    usedBytes: used,
    freeBytes: Math.max(0, total - used),
    diskCount: attached.length,
    attachedBytes: total,
    scratchBytes: 0,
    root: '/',
  }
})
const volumeView = computed(() => (staticHost ? diskVolume.value : df.value))
const volumePct = computed(() => {
  const d = volumeView.value
  if (!d || d.totalBytes === 0) return 0
  return Math.min(100, (d.usedBytes / d.totalBytes) * 100)
})

const attachedDisks = computed(() => store.disks.filter(d => d.state === 'attached').length)
const selectedCfg = computed<SyncConfig | null>(() => {
  if (selectedId.value) {
    const found = store.syncConfigs.find(c => c.id === selectedId.value)
    if (found) return found
  }
  return store.syncConfigs[0] ?? null
})

watch(
  () => store.syncConfigs.map(c => c.id).join(','),
  () => {
    if (!selectedId.value || !store.syncConfigs.some(c => c.id === selectedId.value)) {
      selectedId.value = store.syncConfigs[0]?.id ?? null
    }
  },
)

// Switching providers resets the pickers — stale repo/folder lists from
// another backend must never leak into the new card.
watch(selectedId, () => {
  repoChoices.value = []
  repoMsg.value = ''
  repoSelected.value = ''
  driveFolders.value = []
  driveMsg.value = ''
  driveSelected.value = ''
})

// ── dynamic per-backend setup/config schema ───────────────────
interface FieldDef {
  key: 'repoName' | 'branch' | 'folderId' | 'basePath' | 'token'
  label: string
  placeholder: string
  hint?: string
  secret?: boolean
  grow?: boolean
}

function fieldsFor(backend: SyncBackendType): FieldDef[] {
  switch (backend) {
    case 'github':
      return [
        { key: 'repoName', label: 'Repo (owner/repo)', placeholder: 'owner/repo', hint: 'The repository that stores this vault. Needs the repo scope.' },
        { key: 'branch', label: 'Branch', placeholder: 'main' },
        { key: 'token', label: tokenLabel(backend), placeholder: 'paste — optional when using OAuth', secret: true, grow: true },
      ]
    case 'gitlab':
      return [
        { key: 'repoName', label: 'Project ID', placeholder: '12345678', hint: 'Numeric project id (Settings → General in GitLab).' },
        { key: 'branch', label: 'Branch', placeholder: 'main' },
        { key: 'basePath', label: 'Instance URL (self-hosted — empty = gitlab.com)', placeholder: 'https://gitlab.example.com', grow: true },
        { key: 'token', label: tokenLabel(backend), placeholder: 'paste — optional when using OAuth', secret: true, grow: true },
      ]
    case 'googleDrive':
      return [
        { key: 'folderId', label: 'Drive folder ID', placeholder: 'folder id (optional)', hint: 'Empty = app root folder.' },
        { key: 'token', label: tokenLabel(backend), placeholder: 'paste — optional when using OAuth', secret: true, grow: true },
      ]
    default:
      return [
        { key: 'basePath', label: 'Local path', placeholder: '/DATA/SYNC', hint: 'Directory on this machine. No login needed.', grow: true },
      ]
  }
}

// ── sticky warnings footer ────────────────────────────────────
interface Warning {
  level: 'error' | 'warn' | 'info'
  text: string
}

const warnings = computed<Warning[]>(() => {
  const out: Warning[] = []
  if (disk.lastError) out.push({ level: 'error', text: `Vault file: ${disk.lastError}` })
  if (disk.needsPassphrase) out.push({ level: 'warn', text: `${disk.name || 'Vault file'} is encrypted — enter its passphrase in the Vault file tab.` })
  if (disk.needsPermission) out.push({ level: 'warn', text: `${disk.name || 'Vault file'} is remembered — the browser needs one click to re-open it (Vault file tab).` })
  if (!disk.bound) out.push({ level: 'warn', text: 'No vault file bound — the vault lives in this browser session only. Create or open a .cybermanju file.' })
  if (disk.bound && disk.dirty) out.push({ level: 'warn', text: 'Unsaved vault changes — they write back automatically, or press Save now.' })
  if (dbBackend.value === 'memory') out.push({ level: 'warn', text: 'Database is in-memory (OPFS unavailable here) — bind a .cybermanju file and export regularly so nothing is lost on reload.' })
  if (!disk.supported) out.push({ level: 'info', text: 'This browser has no File System Access API — use Export / Import for the vault file.' })
  for (const cfg of store.syncConfigs) {
    const s = authState.value[cfg.id]
    if (!s || s.ok === null) {
      out.push({ level: 'warn', text: `${cfg.name || backendLabel(cfg.backendType)}: connection untested — press Test connection.` })
    } else if (!s.ok) {
      out.push({
        level: isUnreachable(s.detail) ? 'warn' : 'error',
        text: `${cfg.name || backendLabel(cfg.backendType)}: ${isUnreachable(s.detail) ? 'unreachable (network/CORS — token may still be fine)' : `auth failed — ${s.detail}`}`,
      })
    }
  }
  const needsBroker = store.syncConfigs.some(c => isOauthCapable(c.backendType))
  if ((needsBroker || !identity.value) && !sbConfigured.value) {
    out.push({ level: 'warn', text: 'OAuth broker is not set — sign-in and provider OAuth stay off until you add the URL + key in Settings → OAuth broker.' })
  }
  if (staticHost) out.push({ level: 'info', text: 'Offline demo vault — provider network calls need the server; everything else runs locally.' })
  if (!identity.value) out.push({ level: 'info', text: 'Not signed in — pick a provider at the top of the Connections tab.' })
  return out
})

function draft(cfg: SyncConfig): CredentialDraft {
  let d = drafts[cfg.id]
  if (!d) {
    d = blankCredentialDraft({
      repoName: cfg.repoName ?? '',
      branch: cfg.branch ?? 'main',
      folderId: cfg.folderId ?? '',
      basePath: cfg.basePath ?? '',
      name: cfg.name ?? '',
    })
    drafts[cfg.id] = d
  }
  return d
}

function disksFor(configId: string): DiskRow[] {
  return store.disks.filter(d => d.configId === configId)
}

function authLabel(id: string): string {
  const s = authState.value[id]
  if (!s || s.ok === null) return 'UNTESTED'
  if (s.ok) return 'CONNECTED'
  return isUnreachable(s.detail) ? 'UNREACHABLE' : 'AUTH FAILED'
}

function statusTone(id: string): string {
  const s = authState.value[id]
  if (!s || s.ok === null) return ''
  if (s.ok) return 'is-ok'
  return isUnreachable(s.detail) ? 'is-warn' : 'is-err'
}

/** Transport failure (offline / CORS-blocked / timeout) is not a verdict
 * on the token — label it so users don't rotate good credentials. */
function isUnreachable(detail: string): boolean {
  return /(^network:|\bCORS\b|blocked|abort|timed? ?out|Failed to fetch|Load failed)/i.test(detail)
}

function authDetail(id: string): string {
  return authState.value[id]?.detail ?? ''
}

function authHint(id: string): string {
  const d = authDetail(id)
  if (!d) return ''
  const { prefix, hint } = describeSyncError(d)
  return `${prefix}: ${hint}`
}

async function refresh() {
  refreshing.value = true
  await Promise.allSettled([
    store.fetchAccounts(),
    store.fetchSyncConfigs(),
    store.fetchDisks(),
    store.fetchOsDf(),
  ])
  refreshing.value = false
}

async function signIn(provider: OAuthBackend) {
  if (signInBusy.value) return
  signInBusy.value = provider
  signInMsg.value = ''
  try {
    const who = await signInWithPopup(provider)
    store.notifySuccess(`Signed in as ${who.name}`)
    // Keep the two lists in sync: a fresh login provisions its provider
    // row (with the session token when scopes allow) so Connections > 0.
    await ensureFromSignIn(provider, who.name).catch(() => {})
  } catch (e) {
    signInMsg.value = e instanceof Error ? e.message : String(e)
  } finally {
    signInBusy.value = null
  }
}

/**
 * Rail status dot per OAuth login: green when its backend is connected (or
 * the live session belongs to it), amber when a connection exists but is
 * untested/failing, none when signed out with nothing stored.
 */
function railTone(p: { id: OAuthBackend }): string {
  const backend = backendForOAuth(p.id)
  const cfg = store.syncConfigs.find(c => c.backendType === backend)
  if (cfg) {
    const s = authState.value[cfg.id]
    if (s?.ok) return 'is-ok'
    return 'is-warn'
  }
  if (identity.value?.provider === p.id) return 'is-ok'
  return ''
}

function railHint(p: { id: OAuthBackend; label: string }): string {
  if (signInBusy.value) return 'Working…'
  if (!identity.value) return `Sign in with ${p.label}`
  const backend = backendForOAuth(p.id)
  const cfg = store.syncConfigs.find(c => c.backendType === backend)
  if (cfg) return `Open ${cfg.name || backendLabel(backend)} connection`
  return `${p.label} — open session to connect`
}

/**
 * Rail icon click: signed out → straight into sign-in; signed in with a
 * matching connection → select it in the workspace; signed in without one
 * → pin the session flyout open where Connect/Create lives (no surprise
 * OAuth popup from an exploratory click).
 */
async function railProviderClick(p: { id: OAuthBackend }): Promise<void> {
  if (signInBusy.value) return
  if (!identity.value) {
    await signIn(p.id)
    return
  }
  const backend = backendForOAuth(p.id)
  const cfg = store.syncConfigs.find(c => c.backendType === backend)
  if (cfg) {
    wizOpen.value = false
    selectedId.value = cfg.id
  } else {
    railPinned.value = true
  }
}

async function signOut() {
  signingOut.value = true
  try {
    await signOutIdentity()
    store.notifySuccess('Signed out')
  } finally {
    signingOut.value = false
  }
}

/**
 * Clear every saved provider secret for one backend (sealed secret table:
 * an explicit empty token clears, an absent one leaves it untouched).
 * Keeps the connection rows so a fresh OAuth run can reuse them.
 */
async function clearSavedSecretsForBackend(backend: SyncBackendType): Promise<void> {
  for (const c of configsForBackend(backend)) {
    const d = drafts[c.id]
    if (d) d.token = ''
    await store.saveSyncConfig({ ...c, token: '' } as SyncConfig).catch(() => null)
    authState.value[c.id] = { ok: null, detail: 'token revoked — reconnect with OAuth or paste a token' }
  }
}

/**
 * Identity-level revoke: provider grant (best-effort) + global sign-out +
 * forget the remembered login + clear saved provider secrets, so the OAuth
 * token cannot auto-login again from this browser.
 */
async function revokeSession(): Promise<void> {
  if (revoking.value || signingOut.value) return
  const who = identity.value
  const label = who ? `${who.name} (${who.provider})` : 'the current session'
  if (!window.confirm(`Revoke the OAuth grant for "${label}", sign out, and forget this login so it cannot auto-login again?`)) return
  revoking.value = true
  try {
    const backend = who ? backendForAccountProvider(who.provider) : null
    const accountId = who?.id ?? null
    const grant = await revokeIdentitySession().catch(() => ({ revokedAtProvider: false, detail: 'revoke failed — cleared locally' }))
    if (accountId) forgetConnectedAccount(accountId)
    if (backend) await clearSavedSecretsForBackend(backend)
    signInMsg.value = `${grant.detail} — signed out, login forgotten.`
    store.notifySuccess?.('OAuth revoked — signed out')
  } finally {
    revoking.value = false
  }
}

/**
 * Per-account revoke: the active login gets the full provider-side revoke;
 * remembered (inactive) logins hold no live token, so revoke forgets them
 * and — when they were the last login on their backend — clears that
 * backend's saved secrets too.
 */
async function revokeAccount(acc: ConnectedAccount): Promise<void> {
  if (revoking.value) return
  if (identity.value && acc.id === identity.value.id) {
    await revokeSession()
    return
  }
  const backend = backendForAccountProvider(acc.provider)
  const othersRemain = backend
    ? connectedAccounts.value.some(a => a.id !== acc.id && backendForAccountProvider(a.provider) === backend)
    : true
  if (!window.confirm(`Revoke "${acc.name}"? This forgets the login so it cannot auto-login again${backend && !othersRemain ? ` and clears the saved ${backendLabel(backend)} token` : ''}. No live provider token exists for remembered logins — rotate the grant at the provider if it may have leaked.`)) return
  revoking.value = true
  try {
    forgetConnectedAccount(acc.id)
    if (backend && !othersRemain) await clearSavedSecretsForBackend(backend)
    ensureMsg.value = `"${acc.name}" revoked — login forgotten, auto-login stopped.`
    ensureOk.value = true
    store.notifySuccess?.('Account revoked — auto-login stopped')
  } finally {
    revoking.value = false
  }
}

/**
 * Per-provider revoke (step 1): best-effort provider-side revocation of
 * whatever token is in play (typed draft → saved secret → live session),
 * then clear the saved secret so probes/autologin stop using it.
 */
async function revokeProviderToken(cfg: SyncConfig): Promise<void> {
  if (revokingCfg.value || revoking.value) return
  const label = cfg.name || backendLabel(cfg.backendType)
  if (!window.confirm(`Revoke the OAuth token for "${label}" and clear its saved secret? Auto-login with this token stops.`)) return
  revokingCfg.value = cfg.id
  try {
    const token = await resolveProviderToken(cfg)
    const slug = supabaseProviderFor(cfg.backendType) ?? cfg.backendType
    const grant = token
      ? await revokeProviderGrant(slug, token).catch(() => ({ revokedAtProvider: false, detail: 'revoke failed — cleared locally' }))
      : { revokedAtProvider: false, detail: 'no token stored' }
    const d = drafts[cfg.id]
    if (d) d.token = ''
    await store.saveSyncConfig({ ...cfg, token: '' } as SyncConfig).catch(() => null)
    authState.value[cfg.id] = { ok: null, detail: 'token revoked — reconnect with OAuth or paste a token' }
    connectMsg.value[cfg.id] = `${grant.detail} — saved secret cleared.`
    store.notifySuccess?.(`"${label}" token revoked`)
  } finally {
    revokingCfg.value = null
  }
}

/** Switch to a remembered account: fresh login with its provider — the
 * provider's own chooser picks the account, so a second login on the SAME
 * provider works too. */
async function switchAccount(acc: ConnectedAccount) {
  if (signInBusy.value) return
  await signOutIdentity().catch(() => {})
  await signIn(acc.provider as OAuthBackend)
}

function forgetAccount(id: string) {
  const acc = connectedAccounts.value.find(a => a.id === id)
  const backend = acc ? backendForAccountProvider(acc.provider) : null
  // Orphan check BEFORE forgetting: remaining accounts on the same backend
  // (excluding the one being forgotten). Only the last login on a backend
  // can orphan provider rows, so only then do we offer the cascade.
  const othersRemain = backend
    ? connectedAccounts.value.some(a => a.id !== id && backendForAccountProvider(a.provider) === backend)
    : true
  const orphans = backend && !othersRemain ? configsForBackend(backend) : []
  if (acc && orphans.length) {
    const names = orphans.map(c => c.name || backendLabel(c.backendType)).join(', ')
    if (!window.confirm(
      `Forget "${acc.name}"? This is also the last ${backendLabel(backend!)} login — OK also deletes ${orphans.length} now-orphan provider connection(s) (${names}); Cancel keeps the provider(s).`,
    )) return
  } else if (acc) {
    if (!window.confirm(`Forget "${acc.name}"? You can sign in again any time.`)) return
  }
  forgetConnectedAccount(id)
  // Cascade second half: the account is gone, so its orphaned provider
  // rows go too (disks stay in the catalog — same guarantee as Delete).
  if (orphans.length) {
    void (async () => {
      for (const c of orphans) {
        purgeProviderUiState(c.id)
        await store.deleteSyncConfig(c.id)
      }
      ensureMsg.value = `${backendLabel(backend!)} login forgotten — its provider connection(s) were deleted too.`
      ensureOk.value = true
      store.notifySuccess?.('Account forgotten — orphan provider connection(s) deleted')
    })()
  }
}

/** All provider rows for one backend (vault sets create several per backend). */
function configsForBackend(backend: SyncBackendType): SyncConfig[] {
  return store.syncConfigs.filter(c => c.backendType === backend)
}

/** Template-safe: provider rows behind a remembered login's provider string. */
function configsForBackendSafe(provider: string): SyncConfig[] {
  const b = backendForAccountProvider(provider)
  return b ? configsForBackend(b) : []
}

/** Template-safe backend display name for a remembered login. */
function providerBackendLabel(provider: string): string {
  const b = backendForAccountProvider(provider)
  return b ? backendLabel(b) : provider
}

/**
 * Delete the provider connection(s) behind a remembered login, keeping the
 * login itself. This is the Sign-in-side mirror of the Connections-list
 * Delete button — the sync the panel was missing.
 */
async function disconnectAccountProvider(acc: ConnectedAccount): Promise<void> {
  const backend = backendForAccountProvider(acc.provider)
  if (!backend) return
  const targets = configsForBackend(backend)
  if (!targets.length) return
  const names = targets.map(c => c.name || backendLabel(c.backendType)).join(', ')
  if (!window.confirm(
    `Delete ${targets.length} ${backendLabel(backend)} provider connection(s) (${names}) for "${acc.name}"? The login stays remembered — disks stay in the catalog.`,
  )) return
  for (const c of targets) {
    purgeProviderUiState(c.id)
    await store.deleteSyncConfig(c.id)
  }
  ensureMsg.value = `${backendLabel(backend)} connection(s) deleted — press Connect on "${acc.name}" to recreate.`
  ensureOk.value = true
}

function timeOf(ts: number): string {
  return ts ? new Date(ts).toLocaleTimeString() : ''
}

async function openDiskFile() {
  await openCyberManjuFile()
}

async function createDiskFile() {
  await createCyberManjuFile(diskPassphrase.value)
}

async function saveDiskFile() {
  const ok = await saveCyberManjuFile()
  if (ok) store.notifySuccess(disk.lastMessage || 'Saved')
}

async function exportDiskFile() {
  await exportCyberManjuFile(diskPassphrase.value || undefined)
}

function pickImport() {
  importInput.value?.click()
}

async function onImportFile(ev: Event) {
  const input = ev.target as HTMLInputElement
  const file = input.files?.[0] ?? null
  input.value = ''
  if (!file) return
  await importCyberManjuFile(file)
  if (disk.attached) store.notifySuccess(`${disk.name} imported`)
}

async function unlockDisk() {
  await reattachCyberManjuDisk(diskPassphrase.value)
}

async function detachDiskFile() {
  if (!window.confirm('Detach this file? The vault keeps living in this browser session until the file is opened again.')) return
  await detachCyberManjuFile()
}

async function toggleEnabled(cfg: SyncConfig) {
  if (saving.value) return
  saving.value = cfg.id
  try {
    await store.saveSyncConfig({ ...cfg, enabled: !cfg.enabled })
  } finally {
    saving.value = null
  }
}

/** Sync-behavior flags with backend-matching defaults (filled in place). */
function behavior(cfg: SyncConfig): SyncConfig {
  if (cfg.encryptBeforeUpload === undefined) cfg.encryptBeforeUpload = true
  if (cfg.compressBeforeUpload === undefined) cfg.compressBeforeUpload = true
  if (cfg.requireEncryption === undefined) cfg.requireEncryption = false
  if (cfg.obfuscateNames === undefined) cfg.obfuscateNames = false
  if (cfg.placement === undefined) cfg.placement = 'whole'
  if (cfg.parity === undefined) cfg.parity = 1
  return cfg
}

/** Persist one behavior toggle (parity clamped to the 0–4 replica range). */
async function saveBehavior(cfg: SyncConfig) {
  if (saving.value) return
  saving.value = cfg.id
  try {
    const parity = Math.min(4, Math.max(0, Math.round(Number(cfg.parity) || 0)))
    const saved = await store.saveSyncConfig({ ...cfg, parity })
    if (saved) {
      cfg.parity = saved.parity
      store.notifySuccess('Sync behavior saved')
    }
  } finally {
    saving.value = null
  }
}

async function probe(cfg: SyncConfig) {
  if (probing.value.has(cfg.id)) return
  probing.value.add(cfg.id)
  authState.value[cfg.id] = { ok: null, detail: 'probing…' }
  try {
    // The stored row never carries the secret (sealed server-side / in the
    // vault) and the draft may be empty after an OAuth sign-in — so a probe
    // built from row + draft alone reports "has no token" even though the
    // live Supabase session holds a valid token. Attach it as a last resort
    // (probe-only, never saved here).
    const base = overlayDraft(cfg, drafts[cfg.id])
    let toProbe = base
    if (!(base as SyncConfig).token && isOauthCapable(cfg.backendType)) {
      const sessionToken = await sessionTokenFor(cfg.backendType)
      if (sessionToken) toProbe = { ...base, token: sessionToken }
    }
    const r = await store.probeSyncConnection(toProbe)
    authState.value[cfg.id] = { ok: r.ok, detail: r.detail }
  } finally {
    probing.value.delete(cfg.id)
  }
}

/** Drop every per-config UI cache entry so a deleted provider leaves no ghost state. */
function purgeProviderUiState(id: string) {
  delete drafts[id]
  delete authState.value[id]
  delete connectMsg.value[id]
  delete connectUrl.value[id]
  delete quotaMsg.value[id]
  delete showTokens.value[id]
  delete localPickMsg.value[id]
  delete diskMsg.value[id]
  if (revokingCfg.value === id) revokingCfg.value = null
  if (selectedId.value === id) selectedId.value = null
}

async function removeProvider(id: string) {
  const cfg = store.syncConfigs.find(c => c.id === id)
  const label = cfg?.name || (cfg ? backendLabel(cfg.backendType) : 'this provider')
  const diskCount = disksFor(id).length
  const linkedAccounts = cfg
    ? connectedAccounts.value.filter(a => backendForAccountProvider(a.provider) === cfg.backendType)
    : []
  const linkedNote = linkedAccounts.length
    ? ` ${linkedAccounts.length} remembered login(s) (${linkedAccounts.map(a => a.name).join(', ')}) stay signed in — recreate the connection with one click if needed.`
    : ''
  const diskNote = diskCount ? ` Its ${diskCount} disk(s) stay in the catalog.` : ' Its disks stay in the catalog.'
  if (!window.confirm(`Delete "${label}" provider connection?${diskNote}${linkedNote}`)) return
  purgeProviderUiState(id)
  await store.deleteSyncConfig(id)
  if (cfg) store.notifySuccess?.(`"${label}" provider deleted`)
}

async function saveCreds(cfg: SyncConfig) {
  if (saving.value) return
  const d = draft(cfg)
  saving.value = cfg.id
  try {
    // NOTE: no session-token injection into the draft here — an absent token
    // must leave the stored secret untouched (rename-only saves). The probe
    // below still falls back to the session token so Test-after-sign-in goes
    // green; persisting happens via Connect or the mount-time heal.
    const saved = await store.saveSyncConfig(draftToSave(cfg, d))
    if (!saved) return
    refreshDraftFromSaved(d, saved)
    const base = overlayDraft(saved, d)
    let toProbe = base
    if (!(base as SyncConfig).token && isOauthCapable(cfg.backendType)) {
      const sessionToken = await sessionTokenFor(cfg.backendType)
      if (sessionToken) toProbe = { ...base, token: sessionToken }
    }
    const r = await store.probeSyncConnection(toProbe)
    authState.value[cfg.id] = { ok: r.ok, detail: r.detail }
    if (r.ok) store.notifySuccess('Provider verified — credentials work')
  } finally {
    saving.value = null
  }
}

/**
 * Sealed secret behind a config (never on the row itself). Empty means the
 * row is truly tokenless — safe to top up from the live session without
 * clobbering a pasted PAT.
 */
async function sealedSecretFor(configId: string): Promise<string> {
  try {
    const { wasmDbDispatch } = await import('@/composables/useWasmBackend')
    const raw = await wasmDbDispatch('sync.secret', { configId }).catch(() => null)
    if (typeof raw === 'string') return raw
    if (raw && typeof raw === 'object') {
      const token = (raw as Record<string, unknown>).token
      if (typeof token === 'string') return token
    }
  } catch {
    // Non-static transports have no vault secret table — fall through.
  }
  return ''
}

async function quota(cfg: SyncConfig) {
  const u = await store.fetchSyncUsage(cfg.id)
  quotaMsg.value[cfg.id] = u
    ? `quota: ${[u.totalBytes != null ? `total ${humanBytes(u.totalBytes)}` : null, u.usedBytes != null ? `used ${humanBytes(u.usedBytes)}` : null, u.remainingRequests != null ? `${u.remainingRequests} req left` : null].filter(Boolean).join(' · ') || u.detail}`
    : 'quota unavailable'
}

/** One-button connect: Supabase broker on static hosts, dashboard PKCE otherwise. */
function connectAvailable(cfg: SyncConfig): boolean {
  if (!isOauthCapable(cfg.backendType)) return false
  if (staticHost) return supabaseConfigured()
  return true
}

function connectUnavailableReason(cfg: SyncConfig): string {
  if (staticHost && !supabaseConfigured()) {
    return 'OAuth broker is not set — add the URL + key in Settings → OAuth broker, then connect.'
  }
  return 'OAuth is not available for this provider — paste a token in step 2.'
}

function connectCta(cfg: SyncConfig): string {
  const s = authState.value[cfg.id]
  if (s && s.ok === false) return `Reconnect ${shortName(cfg.backendType)}`
  if (s && s.ok) return 'Reconnect'
  return `Connect ${shortName(cfg.backendType)}`
}

function connectCtaHint(cfg: SyncConfig): string {
  if (!connectAvailable(cfg)) return connectUnavailableReason(cfg)
  return staticHost
    ? 'Approve at the provider — the token lands here via the Supabase broker'
    : 'Approve at the provider — the dashboard finishes the exchange'
}

function connectTitle(cfg: SyncConfig): string {
  const s = authState.value[cfg.id]
  if (s?.ok) return `${shortName(cfg.backendType)} is connected`
  if (s && s.ok === false) return isUnreachable(s.detail) ? 'Last check could not reach the provider' : 'Last check failed — reconnect or paste a token'
  return `Connect ${backendLabel(cfg.backendType)}`
}

function connectSubtitle(cfg: SyncConfig): string {
  return staticHost
    ? 'Browser approval — no password typed here, token lands automatically.'
    : 'Browser approval — no password typed here, the dashboard completes it.'
}

async function connectWithOAuth(cfg: SyncConfig) {
  if (staticHost || isTauriMobile()) await supabaseConnect(cfg)
  else await oauthConnect(cfg)
}

function cancelConnect() {
  connectAbort.value?.abort()
  connectAbort.value = null
  oauthAbort.value?.abort()
  oauthAbort.value = null
  sbAbort.value?.abort()
  sbAbort.value = null
  connectBusy.value = null
}

async function copyConnectUrl(cfg: SyncConfig) {
  const url = connectUrl.value[cfg.id]
  if (!url) return
  try {
    await navigator.clipboard.writeText(url)
    connectMsg.value[cfg.id] = 'Link copied — open it in a browser where you are signed in, approve, then come back.'
  } catch {
    connectMsg.value[cfg.id] = 'Copy failed — select the link text manually.'
  }
}

/** PKCE OAuth: open the provider approval, then poll until credentials land. */
async function oauthConnect(cfg: SyncConfig) {
  // No dashboard behind the static build means no server-side callback to
  // land credentials in — say so immediately instead of polling for 2 min.
  // (If Settings → Remote Dashboard points at a server, this build is a
  // REST client and this branch never runs.)
  if (staticHost) {
    connectMsg.value[cfg.id] =
      'OAuth needs a dashboard for the server callback — set REMOTE DASHBOARD in Settings to your server for full OAuth here, or paste a token below for the offline vault.'
    return
  }
  cancelConnect()
  connectBusy.value = cfg.id
  connectMsg.value[cfg.id] = 'Opening provider approval…'
  let popup: Window | null = null
  try {
    popup = window.open('about:blank', 'cyb_oauth', 'width=620,height=720')
    if (popup === window) popup = null
  } catch { popup = null }
  let res: Awaited<ReturnType<typeof store.oauthStart>>
  try {
    res = await store.oauthStart(cfg.backendType, cfg.id)
  } catch (e) {
    try { if (popup && !popup.closed) popup.close() } catch { /* best effort */ }
    connectMsg.value[cfg.id] = e instanceof Error ? e.message : 'OAuth did not start — retry or paste a token.'
    connectBusy.value = null
    return
  }
  if (!res?.authorizeUrl) {
    try { if (popup && !popup.closed) popup.close() } catch { /* best effort */ }
    connectMsg.value[cfg.id] = 'OAuth did not start — paste a token below instead.'
    connectBusy.value = null
    return
  }
  if (popup) popup.location.href = res.authorizeUrl
  else {
    connectUrl.value[cfg.id] = res.authorizeUrl
    connectMsg.value[cfg.id] = 'Popup blocked — open the approval link below, then return here to verify.'
    connectBusy.value = null
    return
  }
  connectMsg.value[cfg.id] = 'Approve in the opened browser tab — waiting for the callback…'
  const abort = new AbortController()
  connectAbort.value = abort
  oauthAbort.value = abort
  try {
    const ok = await pollUntilTrue(
      async () => (await store.probeSyncConnection(cfg)).ok,
      {
        intervalMs: 3000,
        maxAttempts: 40,
        signal: abort.signal,
        onAttempt: (n) => {
          connectMsg.value[cfg.id] = `Approve in the browser tab — waiting… (${n * 3}s)`
        },
      },
    )
    if (ok) {
      authState.value[cfg.id] = { ok: true, detail: 'OAuth credentials verified' }
      connectMsg.value[cfg.id] = 'Connected — OAuth credentials verified.'
      store.notifySuccess(`${cfg.name || cfg.backendType}: OAuth connected`)
    } else {
      connectMsg.value[cfg.id] = 'Timed out waiting for approval (2 min). Retry, or paste a token below.'
    }
  } catch {
    connectMsg.value[cfg.id] = 'Cancelled.'
  } finally {
    connectAbort.value = null
    oauthAbort.value = null
    connectBusy.value = null
  }
}

/**
 * Supabase-brokered OAuth (static builds): Supabase's server does the
 * secret-holding exchange; the provider token lands in our session, we save
 * it into the provider config and probe — CONNECTED.
 */
async function supabaseConnect(cfg: SyncConfig) {
  cancelConnect()
  if (!supabaseConfigured()) {
    connectMsg.value[cfg.id] =
      'OAuth broker is not set — add the URL + key in Settings → OAuth broker, then connect.'
    return
  }
  setPendingOAuthConfig(cfg.id)
  const refreshGoogleGrant = cfg.backendType === 'googleDrive' && authState.value[cfg.id]?.ok === false
  const expected = supabaseProviderFor(cfg.backendType)
  connectBusy.value = cfg.id
  delete mobileOAuthErrors.value[cfg.id]
  let popup: Window | null = null
  // Any Tauri build authenticates in the system browser via the
  // cybermanju:// deep link — a popup WebView cannot navigate back to a
  // custom scheme on Linux ("scheme that is not HTTP(S)").
  const nativeApp = isTauri()
  if (!nativeApp) {
    try {
      // Reserve the browser window while the provider button still owns
      // a user gesture; opening after PKCE awaits is blocked on mobile Safari.
      popup = window.open('about:blank', 'cyb_sb_oauth', 'width=620,height=720')
      if (popup === window) popup = null
    } catch { popup = null }
  }
  let url = ''
  try {
    if (refreshGoogleGrant) await signOutIdentity().catch(() => {})
    if (nativeApp) await installMobileOAuthDeepLinks()
    ;({ url } = await startSupabaseOAuth(cfg.backendType, !!popup, cfg.backendType === 'googleDrive'))
    if (nativeApp) {
      // Platform opener: Android goes through the Intent bridge (the shell
      // opener has no `xdg-open` there — `Scoped shell IO error: … os error 2`),
      // desktop keeps the system browser + `cybermanju://` return.
      await openExternalUrl(url)
    }
  } catch (e) {
    try { if (popup && !popup.closed) popup.close() } catch { /* best effort */ }
    connectMsg.value[cfg.id] = e instanceof Error ? e.message : String(e)
    connectBusy.value = null
    return
  }
  if (nativeApp) {
    connectMsg.value[cfg.id] = 'Approve in your browser, then return to CyberManju OS…'
  } else if (!popup) {
    connectMsg.value[cfg.id] = 'Continuing securely in this tab…'
    window.location.assign(url)
    return
  } else {
    popup.location.href = url
    connectMsg.value[cfg.id] = 'Approve at the provider in the popup — waiting for the token…'
  }
  const abort = new AbortController()
  connectAbort.value = abort
  sbAbort.value = abort
  // Wake up early when the popup reports completion; the shared session
  // stays the source of truth.
  let msgDone = false
  const onMsg = (e: MessageEvent) => {
    try {
      if (e?.data?.type === 'cybermanju:oauth-done') msgDone = true
    } catch {
      // Ignore malformed messages.
    }
  }
  window.addEventListener('message', onMsg)
  try {
    const ok = await pollUntilTrue(
      async () => {
        if (popup?.closed && !msgDone) throw new Error('popup-closed')
        let token = ''
        try {
          const session = await supabaseSession()
          const prov = session ? supabaseSessionProvider(session) : null
          // Only accept the token minted for THIS provider — a stale session
          // from an earlier login with another provider must not close the
          // flow early with the wrong credentials.
          if (prov && prov === expected) token = session?.provider_token ?? ''
        } catch {
          token = ''
        }
        if (!token) return false
        try { popup?.close() } catch { /* already gone */ }
        await finalizeSupabaseToken(cfg, token)
        return true
      },
      {
        intervalMs: 2000,
        maxAttempts: 90,
        signal: abort.signal,
        onAttempt: (n) => {
          if (n % 10 === 0) connectMsg.value[cfg.id] = nativeApp
            ? `Approve at the provider in your browser — waiting… (${n * 2}s)`
            : `Approve at the provider in the popup — waiting… (${n * 2}s)`
        },
      },
    )
    if (!ok) connectMsg.value[cfg.id] = 'Timed out waiting for approval (3 min). Retry, or paste a token below.'
  } catch (e) {
    connectMsg.value[cfg.id] = mobileOAuthErrors.value[cfg.id] || (
      e instanceof Error && e.message === 'popup-closed'
        ? 'Popup closed before approval — retry, or paste a token below.'
        : 'Cancelled.'
    )
    delete mobileOAuthErrors.value[cfg.id]
  } finally {
    window.removeEventListener('message', onMsg)
    connectAbort.value = null
    sbAbort.value = null
    connectBusy.value = null
  }
}

async function finalizeSupabaseToken(cfg: SyncConfig, token: string) {
  const saved = await store.saveSyncConfig({ ...cfg, token })
  if (!saved) {
    connectMsg.value[cfg.id] = 'Token received, but saving it failed — retry.'
    setPendingOAuthConfig(null)
    return
  }
  const r = await store.probeSyncConnection({ ...saved, token })
  authState.value[cfg.id] = { ok: r.ok, detail: r.detail }
  if (r.ok) {
    connectMsg.value[cfg.id] = 'Connected — provider token verified.'
    store.notifySuccess(`${cfg.name || cfg.backendType}: OAuth connected via Supabase`)
  } else {
    connectMsg.value[cfg.id] = `Token saved, but verification failed: ${r.detail}`
  }
  setPendingOAuthConfig(null)
}

function handleMobileOAuthReturn(event: Event) {
  const result = (event as CustomEvent<{ ok?: boolean; configId?: string | null; message?: string }>).detail
  if (result?.ok !== false) return
  const id = result.configId || connectBusy.value
  if (!id) return
  const message = result.message || 'The provider did not complete sign-in.'
  mobileOAuthErrors.value[id] = message
  connectMsg.value[id] = message
  if (connectBusy.value === id) cancelConnect()
}

async function attachDisk(diskId: string, configId: string) {
  if (diskBusy.value) return
  diskBusy.value = diskId
  try {
    // The per-provider passphrase field doubles as the unlock secret —
    // no popup prompts. Empty means "no passphrase".
    await store.attachDisk(diskId, newDiskPass.value[configId] ?? '')
  } finally {
    diskBusy.value = null
  }
}

function clampDiskMb(mb: number | undefined, fallback: number): number {
  if (!mb || !Number.isFinite(mb)) return fallback
  return Math.min(8192, Math.max(64, Math.round(mb)))
}

async function applyResize(diskId: string) {
  const disk = store.disks.find(x => x.id === diskId)
  const currentMb = disk ? Math.max(64, Math.round(disk.capacityBytes / (1024 * 1024))) : 512
  const mb = clampDiskMb(resizeMb.value[diskId], currentMb)
  if (diskBusy.value) return
  diskBusy.value = diskId
  try {
    await store.resizeDisk(diskId, mb * 1024 * 1024)
  } finally {
    diskBusy.value = null
  }
}

async function createDisk(configId: string) {
  if (diskBusy.value) return
  const cfg = store.syncConfigs.find(c => c.id === configId)
  if (!cfg) return
  diskBusy.value = configId
  diskMsg.value[configId] = ''
  try {
    const mb = clampDiskMb(newDiskMb.value[configId], 512)
    const pass = newDiskPass.value[configId] ?? ''
    if (!pass) {
      diskMsg.value[configId] = 'Type a passphrase first — the disk is passphrase-encrypted on every device.'
      return
    }
    if (cfg.backendType === 'local') {
      await store.createDisk(configId, mb * 1024 * 1024, pass)
      newDiskPass.value[configId] = ''
      return
    }
    // Cloud disk: local sealed container first, then the provider-visible
    // side — Drive folder + `.cybermanju` files, or a private repo (created
    // when missing) + the same seed. The disk survives a remote failure.
    const token = await resolveProviderToken(cfg)
    let vaultBytes: Uint8Array | null = null
    try {
      const { wasmExportDisk } = await import('@/composables/useWasmBackend')
      const out = await wasmExportDisk(pass || undefined).catch(() => null) as { bytes?: Uint8Array } | null
      if (out?.bytes && out.bytes.length > 0 && out.bytes.length <= 5 * 1024 * 1024) vaultBytes = out.bytes
    } catch {
      vaultBytes = null
    }
    const out = await store.createDiskWithRemote(cfg, {
      sizeMb: mb,
      passphrase: pass,
      token,
      vaultBytes,
    })
    if (out?.remote) {
      diskMsg.value[configId] = cfg.backendType === 'googleDrive'
        ? `Drive folder \`${out.remote.remoteDir}\` holds this disk's .cybermanju files.`
        : `Private repo \`${out.remote.config.repoName}\` holds this disk's .cybermanju files.`
    } else if (out?.remoteWarning) {
      diskMsg.value[configId] = `Disk created, but the remote seed failed: ${out.remoteWarning}`
    }
    newDiskPass.value[configId] = ''
  } catch (e) {
    diskMsg.value[configId] = e instanceof Error ? e.message : String(e)
  } finally {
    diskBusy.value = null
  }
}

/**
 * One-click private vault repo set (1–8 repos on one provider): each repo is
 * created private, then saved as a provider config, seeded with README +
 * manifest + the encrypted + compressed `vault.cybermanju` mirror, probed,
 * and given its own encrypted system disk. All disks attach, so the merged
 * volume grows by count × size. Token precedence: the repo-creation field,
 * then the configure-step draft, then the saved secret (OAuth).
 */
async function createVaultRepo(cfg: SyncConfig) {
  if (vaultBusy.value) return
  const { validateRepoName, validateRepoCount, provisionVaultRepoSet } = await import('@/utils/gitProvision')
  const { isStaticHost: checkStatic } = await import('@/composables/useTauri')
  vaultBusy.value = true
  vaultMsg.value = ''
  vaultOk.value = null
  vaultUrl.value = ''
  vaultSteps.value = []
  try {
    const problem = validateRepoName(vaultRepoName.value)
    if (problem) {
      vaultMsg.value = problem
      vaultOk.value = false
      return
    }
    const count = validateRepoCount(vaultRepoCount.value)
    vaultRepoCount.value = count
    const d = drafts[cfg.id]
    const token = vaultRepoToken.value.trim() || d?.token.trim() || ''
    if (cfg.backendType !== 'github' && cfg.backendType !== 'gitlab') {
      vaultMsg.value = 'Vault repos need GitHub or GitLab.'
      vaultOk.value = false
      return
    }
    const passphrase = vaultPassphrase.value
    // Export the live vault THROUGH the passphrase: the seeded
    // `vault.cybermanju` is then encrypted (ChaCha20-Poly1305) and
    // compressed (LZ4) inside the container codec. Empty passphrase still
    // seeds a compressed-but-plaintext container.
    let vaultBytes: Uint8Array | null = null
    if (vaultIncludeFile.value) {
      try {
        const { wasmExportDisk } = await import('@/composables/useWasmBackend')
        const out = await wasmExportDisk(passphrase || undefined).catch(() => null) as { bytes?: Uint8Array } | null
        if (out?.bytes && out.bytes.length > 0) vaultBytes = out.bytes
      } catch {
        vaultBytes = null
      }
    }
    const staticHostNow = checkStatic()
    const branch = d?.branch.trim() || cfg.branch || 'main'
    const instanceUrl = d?.basePath.trim() || cfg.basePath || undefined
    const baseName = vaultRepoName.value.trim()
    vaultMsg.value = count > 1 ? `Creating ${count} private repos…` : 'Creating private repo…'
    const set = await provisionVaultRepoSet({
      backendType: cfg.backendType as 'github' | 'gitlab',
      baseName,
      count,
      token,
      description: 'Private CyberManju OS vault (.cybermanju, encrypted + compressed)',
      branch,
      instanceUrl,
      displayPrefix: baseName,
      diskSizeMb: clampDiskMb(vaultDiskMb.value, 512),
      diskPassphrase: passphrase,
      compressBeforeUpload: vaultCompress.value,
      vaultBytes,
      createRepo: (input) => store.createProviderRepo({
        backendType: input.backendType,
        configId: token ? undefined : cfg.id,
        token: token || undefined,
        name: input.name,
        private: true,
        description: input.description,
        branch: input.branch,
        basePath: instanceUrl,
      }).then((repo) => {
        if (!repo) throw new Error('Repo creation failed — see the toast for the error prefix.')
        return { backend: repo.backend, repoName: repo.repoName, fullName: repo.fullName, branch: repo.branch, url: repo.url, projectId: repo.projectId ?? null }
      }),
      saveConfig: (c) => store.saveSyncConfig({ ...c, id: '' } as SyncConfig),
      seedViaBackend: (c, files) => store.seedRepoFiles(c, files).then((r) => r ?? []),
      probe: (c) => store.probeSyncConnection(c),
      createDisk: async (configId, sizeBytes, pass) => {
        const row = await store.createDisk(configId, sizeBytes, pass)
        return row ? { id: (row as { id: string }).id } : null
      },
      attachDisk: (diskId, pass) => store.attachDisk(diskId, pass),
      useDirectSeed: staticHostNow,
      onProgress: (_done, _total, stage, name) => {
        vaultSteps.value = [...vaultSteps.value, `${name}: ${stage}`].slice(-8)
      },
    })
    await Promise.allSettled([store.fetchSyncConfigs(), store.fetchDisks(), store.fetchOsDf()])
    const first = set.configs[0]
    if (first) {
      selectedId.value = first.id
      authState.value[first.id] = { ok: true, detail: `vault set ${baseName} verified (${set.repos.length}/${count})` }
    }
    vaultUrl.value = set.repos[0]?.url ?? ''
    vaultRepoToken.value = ''
    const mergedMb = set.diskIds.filter(Boolean).length * clampDiskMb(vaultDiskMb.value, 512)
    const failNote = set.failures.length ? ` (${set.failures.length} failed: ${set.failures.map(f => `${f.name}: ${f.error.slice(0, 60)}`).join('; ')})` : ''
    const encNote = passphrase ? 'encrypted' : 'plaintext'
    vaultMsg.value = set.repos.length === 1 && count === 1
      ? `Synced — ${set.repos[0].fullName} mirrors the ${encNote} vault + a ${vaultDiskMb.value} MB disk is attached.${failNote}`
      : `Synced ${set.repos.length}/${count} repos — merged +${mergedMb} MB across ${set.diskIds.filter(Boolean).length} disks (${encNote} vault mirror).${failNote}`
    vaultOk.value = set.failures.length === 0
    store.notifySuccess(`Vault set synced: ${set.repos.length}/${count} repos, +${mergedMb} MB merged`)
  } catch (e) {
    vaultMsg.value = e instanceof Error ? e.message : String(e)
    vaultOk.value = false
  } finally {
    vaultBusy.value = false
  }
}


function wizToConfig(): Omit<SyncConfig, 'id' | 'createdAt' | 'updatedAt'> {
  return {
    ...syncConfigDefaults(),
    backendType: wiz.backendType,
    name: wiz.name.trim() || undefined,
    basePath: wiz.basePath.trim() || undefined,
    repoName: wiz.repoName.trim() || undefined,
    branch: wiz.branch.trim() || undefined,
    token: wiz.token.trim() || undefined,
    folderId: wiz.folderId.trim() || undefined,
  }
}

async function addProvider(verify: boolean) {
  if (wizBusy.value) return
  wizBusy.value = true
  wizMsg.value = ''
  wizOk.value = null
  try {
    // A signed-in session already holds a provider token (Google sign-in
    // mints a Drive-capable one) — use it when the wizard's token field is
    // empty so "Save & verify" works without a manual PAT paste.
    const token = wiz.token.trim() || await sessionTokenFor(wiz.backendType)
    if (!token && isOauthCapable(wiz.backendType)) {
      wizOpen.value = true
      railPinned.value = true
      wizMsg.value = `Sign in with ${backendLabel(wiz.backendType)} above or paste a personal access token before adding this connection.`
      wizOk.value = false
      return
    }
    const saved = await store.saveSyncConfig({
      ...((token ? { ...wizToConfig(), token } : wizToConfig()) as SyncConfig),
      id: '',
    })
    if (!saved) {
      wizMsg.value = 'Could not save — check the connection and retry.'
      wizOk.value = false
      return
    }
    selectedId.value = saved.id
    activeTab.value = 'connections'
    if (verify) {
      wizMsg.value = 'Verifying…'
      const r = await store.probeSyncConnection(token ? { ...saved, token } : saved)
      authState.value[saved.id] = { ok: r.ok, detail: r.detail }
      if (r.ok) {
        wizMsg.value = 'Verified — provider connected.'
        wizOk.value = true
        store.notifySuccess('Provider verified — credentials work')
        wizOpen.value = false
        resetWizard()
      } else {
        // Keep the wizard open so the failure stays visible and editable.
        wizOpen.value = true
        wizMsg.value = !token && isOauthCapable(wiz.backendType)
          ? `Saved, but no token yet: sign in at the top, then press Connect on the “${saved.name || backendLabel(saved.backendType)}” card — or paste a token. (${r.detail})`
          : `Saved, but verification failed: ${r.detail}`
        wizOk.value = false
      }
    } else {
      authState.value[saved.id] = { ok: null, detail: '' }
      wizMsg.value = 'Saved.'
      wizOk.value = true
      store.notifySuccess('Provider saved')
      wizOpen.value = false
      resetWizard()
    }
  } finally {
    wizBusy.value = false
  }
}

onMounted(() => {
  disk.supported = diskSupported()
  window.addEventListener('cybermanju:oauth-return', handleMobileOAuthReturn)
  void refreshIdentity()
  void (async () => {
    await refresh()
    // Heal on every open: (a) any remembered account whose backend has no
    // row gets one; (b) any OAuth-capable row with an empty sealed secret
    // gets topped up from the live session (Google sign-in mints the token,
    // but the row may have been created before the session landed). Silent
    // unless the user presses Create — never overwrites a stored secret.
    if (connectedAccounts.value.length) {
      let touched = false
      for (const acc of connectedAccounts.value) {
        const b = backendForAccountProvider(acc.provider)
        if (b && !store.syncConfigs.some(c => c.backendType === b)) {
          const token = await sessionTokenFor(b)
          await ensureProviderForBackend(b, { token, displayName: `${backendLabel(b)} — ${acc.name}` })
          touched = true
        }
      }
      // (b) runs on static hosts only: there the sealed secret lives in the
      // vault (`sync.secret`) so emptiness is knowable — and a silent heal
      // must never overwrite a pasted PAT. On desktop/REST the secret is
      // server-side and out of reach here; those rows heal via Connect.
      if (staticHost) {
        for (const cfg of [...store.syncConfigs]) {
          if (!isOauthCapable(cfg.backendType)) continue
          if (await sealedSecretFor(cfg.id)) continue
          const draftToken = drafts[cfg.id]?.token.trim()
          if (draftToken) continue
          const sessionToken = await sessionTokenFor(cfg.backendType)
          if (!sessionToken) continue
          const topped = await store.saveSyncConfig({ ...cfg, token: sessionToken })
          if (topped) {
            const r = await store.probeSyncConnection({ ...topped, token: sessionToken })
            authState.value[topped.id] = { ok: r.ok, detail: r.detail }
            touched = true
          }
        }
      }
      if (touched) await refresh().catch(() => {})
    }
    if (!staticHost) return
    void wasmDbBackend().then((b) => {
      dbBackend.value = b
    })
    // Full-redirect resume: the return already stashed the provider token
    // (App boot exchanges ?code=) — finish the link now configs are loaded.
    const stash = takeProviderTokenStash()
    const pending = getPendingOAuthConfig()
    if (stash && pending) {
      const cfg = store.syncConfigs.find(c => c.id === pending)
      if (cfg && supabaseProviderFor(cfg.backendType) === stash.backend) {
        connectMsg.value[cfg.id] = 'Approval received — verifying…'
        await finalizeSupabaseToken(cfg, stash.providerToken)
      } else {
        setPendingOAuthConfig(null)
      }
    }
  })()
})

onBeforeUnmount(() => {
  window.removeEventListener('cybermanju:oauth-return', handleMobileOAuthReturn)
  cancelConnect()
})
</script>

<style scoped>
.am {
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  container-type: inline-size;
  container-name: account-manager;
  background: var(--ui-surface);
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 13px;
}

/* header */
.am-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--ui-border);
}
.am-brand { display: flex; align-items: center; gap: 8px; min-width: 0; }
.am-brand-mark {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--ui-text) 6%, transparent);
  color: var(--ui-text-2);
  border: 1px solid var(--ui-border);
}
.am-title { margin: 0; font-size: 13px; font-weight: 600; }
.am-subtitle { margin: 0; font-size: 11px; color: var(--ui-text-3); }
.am-top-actions { display: flex; align-items: center; gap: 8px; }
.am-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  font-weight: 500;
  border: 1px solid var(--ui-border);
  border-radius: 20px;
  padding: 2px 9px;
  color: var(--ui-text-2);
}
.am-chip.is-ok { color: var(--ui-success); }
.am-chip.is-warn { color: var(--ui-warning); }

/* hero: single quiet storage summary */
.am-hero {
  margin: 10px 12px 0;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  padding: 8px 12px;
  background: var(--ui-surface-2);
}
.am-hero-row { display: flex; align-items: flex-end; justify-content: space-between; gap: 10px; }
.am-hero-stat { display: flex; flex-direction: column; gap: 2px; }
.am-hero-stat.right { text-align: right; }
.am-hero-label { font-size: 11px; font-weight: 600; color: var(--ui-text-3); }
.am-hero-value { font-size: 16px; font-weight: 600; }
.am-hero-value span { font-size: 12px; font-weight: 400; color: var(--ui-text-3); }
.am-bar { height: 6px; border-radius: 4px; margin-top: 8px; background: color-mix(in srgb, var(--ui-text) 10%, transparent); overflow: hidden; }
.am-bar-fill { height: 100%; border-radius: 4px; background: var(--ui-accent); transition: width 0.3s ease; }
.am-hero-legend { display: flex; justify-content: space-between; gap: 10px; margin-top: 6px; font-size: 11px; color: var(--ui-text-3); }

/* tabs: quiet segmented row */
.am-tabs {
  display: flex;
  gap: 4px;
  padding: 10px 12px 0;
  overflow-x: auto;
  scrollbar-width: thin;
}
.am-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--ui-radius-sm);
  color: var(--ui-text-3);
  font-family: inherit;
  font-size: 12px;
  font-weight: 500;
  padding: 6px 10px;
  cursor: pointer;
  transition:
    color var(--ui-dur-fast) var(--ui-ease-out),
    background-color var(--ui-dur-fast) var(--ui-ease-out);
  flex-shrink: 0;
}
.am-tab:hover { color: var(--ui-text); background: color-mix(in srgb, var(--ui-text) 6%, transparent); }
.am-tab.active { color: var(--ui-text); background: color-mix(in srgb, var(--ui-text) 9%, transparent); font-weight: 600; }
.am-tab:focus-visible {
  outline: none;
  box-shadow: var(--ui-glow-soft);
}
.am-tab-count {
  font-size: 10px;
  min-width: 18px;
  text-align: center;
  border-radius: 10px;
  padding: 1px 5px;
  background: color-mix(in srgb, var(--ui-text) 12%, transparent);
}
.am-tab-count.is-err { background: color-mix(in srgb, var(--ui-danger) 25%, transparent); color: var(--ui-danger); }
.am-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--ui-accent); flex-shrink: 0; }
.am-dot.warn { background: var(--ui-warning); }
.am-unsaved {
  display: inline-block;
  margin-left: 6px;
  font-size: 9px;
  line-height: 1;
  color: var(--ui-warning);
  vertical-align: super;
}

/* body */
.am-body { flex: 1; overflow-y: auto; padding: 12px 14px; }
.am-section { display: flex; flex-direction: column; gap: 10px; }
.am-section:focus { outline: none; }
.am-section:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--ui-accent) 60%, transparent);
  outline-offset: -2px;
  border-radius: 8px;
}
.am-card {
  border: 1px solid var(--ui-border);
  border-radius: 12px;
  padding: 14px;
  background: color-mix(in srgb, var(--ui-text) 2%, transparent);
}
.am-card.is-attached { border-color: color-mix(in srgb, var(--ui-accent) 45%, transparent); }
.am-card-title { margin: 0; font-size: 13px; }
.am-card-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
.am-card-head-left { display: flex; gap: 10px; align-items: flex-start; min-width: 0; flex: 1 1 200px; }
.am-hint { margin: 6px 0 0; font-size: 11.5px; line-height: 1.5; color: color-mix(in srgb, var(--ui-text) 60%, transparent); }
.am-code { font-family: ui-monospace, monospace; font-size: 11px; color: var(--ui-info); }
.muted { color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.small { font-size: 11px; }
label.small:has(input[type='checkbox']) {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  cursor: pointer;
}
.truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

/* login grid */
.am-login-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 10px; margin-top: 12px; }
.am-login {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 8px;
  padding: 14px;
  border-radius: 12px;
  border: 1px solid var(--ui-border);
  background: var(--ui-surface);
  color: var(--ui-text);
  font-family: inherit;
  cursor: pointer;
  text-align: left;
  transition: transform 0.08s ease, border-color 0.15s ease, box-shadow 0.15s ease;
}
.am-login:hover:not(:disabled) { border-color: var(--ui-accent); box-shadow: 0 4px 18px rgb(0 0 0 / 0.25); transform: translateY(-1px); }
.am-login:disabled { opacity: 0.55; cursor: not-allowed; }
.am-login-label { font-size: 13px; font-weight: 700; }
.am-login-sub { font-size: 11px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.am-identity-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.am-accounts-toggle {
  display: inline-flex; align-items: center; gap: 7px; align-self: flex-start;
  background: transparent; border: 1px solid var(--ui-border); border-radius: 8px;
  color: color-mix(in srgb, var(--ui-text) 70%, transparent);
  font-family: inherit; font-size: 11.5px; font-weight: 600;
  padding: 6px 11px; cursor: pointer;
}
.am-accounts-toggle:hover { color: var(--ui-text); border-color: var(--ui-border-strong); }
.am-accounts-toggle:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--ui-accent) 75%, transparent);
  outline-offset: 2px;
}
.am-identity-meta { display: flex; flex-direction: column; flex: 1; min-width: 120px; }
.am-avatar { width: 40px; height: 40px; border-radius: 50%; border: 1px solid var(--ui-border); }

/* status pills */
.am-status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.6px;
  border: 1px solid var(--ui-border);
  border-radius: 20px;
  padding: 3px 9px;
  color: color-mix(in srgb, var(--ui-text) 60%, transparent);
  white-space: nowrap;
}
.am-status.sm { font-size: 9.5px; padding: 2px 8px; }
.am-status.is-ok { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent); }
.am-status.is-warn { color: var(--ui-warning); border-color: color-mix(in srgb, var(--ui-warning) 65%, transparent); }
.am-status.is-err { color: var(--ui-danger); border-color: color-mix(in srgb, var(--ui-danger) 60%, transparent); }

/* buttons */
.am-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: transparent;
  border: 1px solid var(--ui-border);
  border-radius: 8px;
  color: color-mix(in srgb, var(--ui-text) 75%, transparent);
  font-family: inherit;
  font-size: 12px;
  font-weight: 600;
  padding: 7px 12px;
  cursor: pointer;
  white-space: nowrap;
}
.am-btn:hover:not(:disabled) { color: var(--ui-text); border-color: var(--ui-border-strong); }
.am-btn:disabled { opacity: 0.45; cursor: not-allowed; }
.am-btn.primary { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent); }
.am-btn.primary:hover:not(:disabled) { background: var(--ui-accent); color: var(--ui-text); }
.am-btn.danger { color: var(--ui-danger); border-color: color-mix(in srgb, var(--ui-danger) 50%, transparent); }
.am-btn.danger:hover:not(:disabled) { background: var(--ui-danger); color: #fff; }
.am-btn.sm { font-size: 11px; padding: 5px 10px; }
.am-btn.xs { font-size: 10px; padding: 3px 8px; border-radius: 6px; }
.am-btn.on { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent); }
.am-icon-btn { background: none; border: none; color: color-mix(in srgb, var(--ui-text) 55%, transparent); cursor: pointer; padding: 4px; display: inline-flex; border-radius: 6px; }
.am-icon-btn:hover { color: var(--ui-text); }
.am-icon-btn:focus-visible, .am-link:focus-visible, .am-btn:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--ui-accent) 75%, transparent);
  outline-offset: 2px;
}
.am-link { background: none; border: none; color: var(--ui-info); cursor: pointer; font: inherit; text-decoration: underline; padding: 0; border-radius: 4px; }
.am-btn-grid { display: flex; flex-wrap: wrap; gap: 8px; margin: 10px 0; }
.am-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-top: 8px; }
.am-row.between { justify-content: space-between; }
.am-row.wrap { flex-wrap: wrap; }

/* fields */
.am-fields { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 10px; margin-top: 10px; }
.am-field { display: flex; flex-direction: column; gap: 5px; font-size: 11px; min-width: 0; }
.am-field.grow { grid-column: 1 / -1; }
.am-field-label { font-size: 11px; font-weight: 600; color: var(--ui-text-2); }
.am-field-hint { font-size: 10.5px; color: color-mix(in srgb, var(--ui-text) 50%, transparent); line-height: 1.4; }
.am-input {
  background: var(--ui-surface);
  border: 1px solid var(--ui-border);
  border-radius: 8px;
  color: var(--ui-text);
  font-family: inherit;
  font-size: 12px;
  padding: 8px 10px;
  outline: none;
  min-width: 0;
  width: 100%;
}
.am-input:focus { border-color: var(--ui-accent); box-shadow: var(--ui-focus-ring); }
.am-input.xs-num { width: 76px; }
.am-input-wrap { display: flex; align-items: center; gap: 4px; }
.am-input-wrap .am-input { flex: 1; }
.am-search {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px;
  min-height: 32px;
  border: 1px solid var(--ui-border);
  border-radius: 8px;
  background: var(--ui-surface);
  color: color-mix(in srgb, var(--ui-text) 55%, transparent);
  transition: border-color var(--ui-dur-fast) var(--ui-ease-out), box-shadow var(--ui-dur-fast) var(--ui-ease-out);
}
.am-search:focus-within {
  border-color: color-mix(in srgb, var(--ui-accent) 65%, transparent);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--ui-accent) 14%, transparent);
  color: var(--ui-text);
}
.am-search-input {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: none;
  outline: none;
  color: var(--ui-text);
  font-family: inherit;
  font-size: 12px;
  padding: 7px 0;
}
.am-search-input::placeholder { color: color-mix(in srgb, var(--ui-text) 40%, transparent); }
.am-search-input::-webkit-search-cancel-button { cursor: pointer; }
.am-slider { flex: 1; min-width: 140px; }
.am-hidden { position: absolute; width: 1px; height: 1px; opacity: 0; pointer-events: none; }

/* banners + notes */
.am-banner {
  display: flex;
  align-items: center;
  gap: 10px;
  border-radius: 10px;
  padding: 10px 12px;
  margin-top: 10px;
  font-size: 12px;
  line-height: 1.5;
  border: 1px solid;
}
/* text owns the free space so icon + message + action can never collide */
.am-banner > span:not(.am-btn):not(.app-icon) { flex: 1 1 auto; min-width: 0; }
.am-banner.warn { border-color: color-mix(in srgb, var(--ui-warning) 55%, transparent); background: color-mix(in srgb, var(--ui-warning) 10%, transparent); }
.am-banner.info { border-color: color-mix(in srgb, var(--ui-info) 50%, transparent); background: color-mix(in srgb, var(--ui-info) 8%, transparent); }
.am-banner .am-btn { margin-left: auto; flex-shrink: 0; }
.am-note { margin: 8px 0 0; font-size: 11.5px; color: var(--ui-info); white-space: pre-wrap; word-break: break-word; line-height: 1.5; }
.am-note.warn { color: var(--ui-warning); }
.am-note.err { color: var(--ui-danger); }
.am-note.ok { color: var(--ui-accent); }

/* connections layout: icon rail + session flyout + provider workspace */
.am-conn-section { position: relative; }
.am-conn { position: relative; display: flex; align-items: stretch; gap: 0; min-height: 100%; }
/* slim icon rail — always visible, hover peeks the flyout */
.am-rail {
  width: 60px;
  flex-shrink: 0;
  align-self: stretch;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 12px 0;
  border-right: 1px solid var(--ui-border);
  background: color-mix(in srgb, var(--ui-text) 3%, transparent);
  border-radius: 12px 0 0 12px;
  position: sticky;
  top: 0;
  max-height: 100%;
  z-index: 6;
}
.am-rail-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  border-radius: 12px;
  border: 1px solid var(--ui-border);
  background: var(--ui-surface);
  color: var(--ui-text);
  cursor: pointer;
  transition: border-color var(--ui-dur-fast) var(--ui-ease-out), transform 0.08s ease;
}
.am-rail-btn:hover:not(:disabled) { border-color: var(--ui-accent); transform: translateY(-1px); }
.am-rail-btn:disabled { opacity: 0.55; cursor: not-allowed; }
.am-rail-btn:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--ui-accent) 75%, transparent);
  outline-offset: 2px;
}
.am-rail-btn.avatar { border-radius: 50%; overflow: visible; }
.am-rail-avatar { width: 26px; height: 26px; border-radius: 50%; }
.am-rail-sep { width: 24px; height: 1px; background: var(--ui-border); }
.am-rail-dot {
  position: absolute;
  right: -2px;
  bottom: -2px;
  width: 11px;
  height: 11px;
  border-radius: 50%;
  border: 2px solid var(--ui-surface);
  background: color-mix(in srgb, var(--ui-text) 30%, transparent);
}
.am-rail-dot.is-ok { background: var(--ui-accent); }
.am-rail-dot.is-warn { background: var(--ui-warning); }
.am-rail-btn.pin { margin-top: 2px; width: 32px; height: 32px; border-radius: 9px; }
.am-rail-pin-icon { transform: rotate(-90deg); transition: transform 0.15s ease; }
.am-rail-pin-icon.open { transform: rotate(90deg); color: var(--ui-accent); }
/* session flyout — slides out right of the rail on hover, locks when pinned */
.am-flyout {
  position: absolute;
  left: 68px;
  top: 0;
  bottom: 0;
  width: min(330px, calc(100% - 76px));
  max-height: 100%;
  z-index: 5;
  display: flex;
  flex-direction: column;
  background: var(--ui-surface-2, var(--ui-surface));
  border: 1px solid var(--ui-border);
  border-radius: 12px;
  box-shadow: 0 12px 40px rgb(0 0 0 / 0.35);
  opacity: 0;
  transform: translateX(-10px);
  pointer-events: none;
  transition: opacity 0.16s ease, transform 0.16s ease;
}
.am-flyout-backdrop { display: none; }
.am-rail:hover ~ .am-flyout,
.am-flyout:hover,
.am-conn.rail-pinned .am-flyout {
  opacity: 1;
  transform: none;
  pointer-events: auto;
}
.am-flyout-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 8px 10px 12px;
  border-bottom: 1px solid var(--ui-border);
  font-size: 12px;
}
.am-flyout-head .muted { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.am-flyout-scroll {
  overflow-y: auto;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
/* provider workspace — the rest of the page */
.am-work { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 10px; padding-left: 12px; }
.am-work-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
.am-work-titles { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.am-work-title { margin: 0; font-size: 13px; font-weight: 700; display: flex; align-items: center; gap: 8px; }
.am-work-title .am-hint { margin: 0; }
.am-connect { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.am-connect-meta { display: flex; flex-direction: column; gap: 2px; flex: 1 1 140px; min-width: 0; }
.am-method {
  font-size: 11px;
  font-weight: 500;
  color: var(--ui-text-2);
  border: 1px solid var(--ui-border);
  border-radius: 12px;
  padding: 2px 8px;
}
.am-progress {
  height: 6px;
  border-radius: 4px;
  margin-top: 10px;
  background: color-mix(in srgb, var(--ui-text) 10%, transparent);
  overflow: hidden;
}
.am-progress-fill {
  height: 100%;
  width: 40%;
  border-radius: 4px;
  background: var(--ui-accent);
  animation: am-slide 1.2s ease-in-out infinite alternate;
}
@keyframes am-slide { from { margin-left: -10%; } to { margin-left: 70%; } }
.am-list-col { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.am-list-head { display: flex; align-items: center; justify-content: space-between; }
.am-prov-row {
  display: flex;
  align-items: stretch;
  gap: 6px;
}
.am-prov-row.active .am-prov { border-color: color-mix(in srgb, var(--ui-accent) 60%, transparent); background: color-mix(in srgb, var(--ui-accent) 8%, transparent); }
.am-prov {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  min-width: 0;
  flex: 1;
  text-align: left;
  padding: 9px 10px;
  border-radius: 10px;
  border: 1px solid var(--ui-border);
  background: transparent;
  color: var(--ui-text);
  font-family: inherit;
  cursor: pointer;
}
.am-prov-del {
  flex-shrink: 0;
  align-self: center;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: 8px;
  border: 1px solid transparent;
  background: transparent;
  color: color-mix(in srgb, var(--ui-text) 45%, transparent);
  cursor: pointer;
}
.am-prov-del:hover { color: var(--ui-danger); border-color: color-mix(in srgb, var(--ui-danger) 55%, transparent); background: color-mix(in srgb, var(--ui-danger) 8%, transparent); }
.am-prov-del:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--ui-danger) 75%, transparent);
  outline-offset: 2px;
}
.am-prov:hover { border-color: var(--ui-border-strong); }
.am-prov:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--ui-accent) 75%, transparent);
  outline-offset: 2px;
}
.am-prov.active { border-color: color-mix(in srgb, var(--ui-accent) 60%, transparent); background: color-mix(in srgb, var(--ui-accent) 8%, transparent); }
.am-prov-meta { display: flex; flex-direction: column; flex: 1; min-width: 0; }
.am-prov-name { font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.am-switch { width: 9px; height: 9px; border-radius: 50%; border: 1px solid var(--ui-border-strong); flex-shrink: 0; }
.am-switch.on { background: var(--ui-accent); border-color: var(--ui-accent); }
.am-detail-col { min-width: 0; }
.am-step { border-top: 1px dashed var(--ui-border); padding-top: 12px; margin-top: 12px; }
.am-step-title { margin: 0 0 8px; font-size: 12px; font-weight: 600; color: var(--ui-text); display: flex; align-items: center; gap: 8px; }
.am-step-n {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  font-size: 11px;
  background: color-mix(in srgb, var(--ui-accent) 18%, transparent);
  color: var(--ui-accent);
  border: 1px solid color-mix(in srgb, var(--ui-accent) 50%, transparent);
}

/* logo picker */
.am-logo-picker { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; margin-top: 10px; }
.am-logo-pick {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 10px 4px;
  font-size: 10px;
  font-family: inherit;
  color: color-mix(in srgb, var(--ui-text) 70%, transparent);
  background: transparent;
  border: 1px solid var(--ui-border);
  border-radius: 10px;
  cursor: pointer;
}
.am-logo-pick:hover { border-color: var(--ui-border-strong); color: var(--ui-text); }
.am-logo-pick:focus-visible, .am-login:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--ui-accent) 75%, transparent);
  outline-offset: 2px;
}
.am-logo-pick.active { border-color: color-mix(in srgb, var(--ui-accent) 60%, transparent); color: var(--ui-text); background: color-mix(in srgb, var(--ui-accent) 8%, transparent); }
.am-wiz { margin-top: 4px; }
.am-picker {
  margin-top: 12px;
  border: 1px solid var(--ui-border);
  border-radius: 10px;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  background: color-mix(in srgb, var(--ui-text) 2%, transparent);
}

/* disks */
.am-disk { border: 1px solid var(--ui-border); border-radius: 10px; padding: 10px; margin-top: 8px; display: flex; flex-direction: column; gap: 8px; }
.am-mini { height: 8px; border-radius: 4px; background: color-mix(in srgb, var(--ui-text) 8%, transparent); overflow: hidden; }
.am-mini-fill { height: 100%; background: var(--ui-accent); border-radius: 4px; }
.am-newdisk { display: flex; flex-direction: column; gap: 8px; margin-top: 10px; border: 1px dashed var(--ui-border); border-radius: 10px; padding: 10px; }

/* warnings footer */
.am-warnings {
  border-top: 1px solid var(--ui-border);
  background: color-mix(in srgb, var(--ui-warning) 5%, transparent);
  padding: 10px 14px;
  max-height: 168px;
  overflow-y: auto;
}
.am-warnings-head { display: flex; align-items: center; gap: 8px; font-size: 12px; font-weight: 600; color: var(--ui-text-2); }
.am-warnings-list { list-style: none; margin: 8px 0 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
.am-warning { display: flex; align-items: flex-start; gap: 8px; font-size: 11.5px; line-height: 1.45; }
.am-warning.is-error { color: var(--ui-danger); }
.am-warning.is-warn { color: var(--ui-warning); }
.am-warning.is-info { color: color-mix(in srgb, var(--ui-text) 65%, transparent); }
.am-warnings-empty { margin: 8px 0 0; font-size: 11.5px; color: var(--ui-accent); }

/* motion + density polish: respect reduced motion, keep narrow windows usable */
@media (prefers-reduced-motion: reduce) {
  .am-bar-fill, .am-tab, .am-login, .am-prov, .am-btn, .am-logo-pick, .am-progress-fill, .am-flyout, .am-rail-btn, .am-rail-pin-icon { transition: none; animation: none; }
}
@media (max-width: 560px) {
  .am-top { flex-wrap: wrap; }
  .am-hero-legend { flex-direction: column; gap: 4px; }
  .am-btn-grid { gap: 6px; }
  .am-login-grid { grid-template-columns: 1fr; }
  .am-logo-picker { grid-template-columns: repeat(2, 1fr); }
}
/* Half-screen density: tighter cards + login grid that fits ~700px. */
@media (max-width: 900px) {
  .am-body { padding: 10px; }
  .am-card { padding: 10px; }
  .am-login-grid { grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); }
  .am-hero { margin: 10px 10px 0; }
  .am-tabs { padding: 10px 10px 0; }
  .am-work { padding-left: 8px; }
  .am-flyout { left: 62px; width: min(300px, calc(100% - 70px)); }
}

/* Mobile and narrow sheets: keep the provider workspace full-width; the
   identity rail becomes a touch strip and its session panel a real drawer. */
@container account-manager (max-width: 700px) {
  .am-top { padding: 8px 10px; }
  .am-top-actions .am-chip { display: none; }
  .am-hero { margin: 8px 10px 0; }
  .am-body { min-height: 0; padding: 8px; }
  .am-conn { flex-direction: column; gap: 8px; min-height: 0; }
  .am-rail {
    position: sticky; top: 0; z-index: 10;
    width: 100%; height: 58px; max-height: none; box-sizing: border-box;
    flex-direction: row; justify-content: flex-start; gap: 8px; padding: 6px 8px;
    overflow-x: auto; overflow-y: hidden; border-right: 0;
    border-bottom: 1px solid var(--ui-border); border-radius: 12px;
    -webkit-overflow-scrolling: touch;
  }
  .am-rail-btn { flex: 0 0 44px; width: 44px; height: 44px; }
  .am-rail-btn.pin { flex: 0 0 44px; width: 44px; height: 44px; margin: 0; }
  .am-rail-sep { flex: 0 0 1px; width: 1px; height: 24px; }
  .am-work { min-width: 0; padding-left: 0; }
  .am-work-head { align-items: center; }
  .am-prov-row { min-width: 0; }
  .am-flyout-backdrop {
    display: block; position: absolute; inset: 0; z-index: 15;
    width: 100%; height: 100%; padding: 0; border: 0; border-radius: 0;
    background: rgb(8 12 20 / 42%);
  }
  .am-flyout {
    left: 0; right: 0; top: 0; bottom: auto; z-index: 20;
    width: 100%; max-height: min(72dvh, 580px); box-sizing: border-box;
    border-radius: 16px; transform: translateY(-8px);
  }
  .am-rail:hover ~ .am-flyout,
  .am-flyout:hover,
  .am-conn.rail-pinned .am-flyout {
    opacity: 0; transform: translateY(-8px); pointer-events: none;
  }
  .am-conn.rail-pinned .am-flyout {
    opacity: 1; transform: none; pointer-events: auto;
  }
  .am-card { padding: 10px; }
  .am-identity-row { align-items: flex-start; }
  .am-identity-meta { min-width: 0; }
  .am-auth-required { align-items: flex-start; flex-wrap: wrap; }
  .am-auth-required .am-btn { margin-left: 0; }
}
</style>
