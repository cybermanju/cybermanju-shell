<!-- CyberManju OS — SettingsPage (src/components/SettingsPage.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="st" :class="{ 'st--mobile': isMobile }">
    <!-- ── header ──────────────────────────────────────────────
         Desktop: one scroll page (brand header · status strip · jump rail).
         Phone: the standard settings STACK instead — a large-title root that
         lists the sections, and a pushed detail behind a back bar. The sheet
         drops its generic titlebar there (AppWindow `--bare-title`), so this
         header is the only chrome under the status strip. -->
    <header v-if="isMobile && mobileSection" class="st-top st-top--mobile is-detail">
      <button class="st-back" type="button" @click="closeMobileSection">
        <AppIcon name="solar:alt-arrow-left-bold" :size="17" />
        <span>Settings</span>
      </button>
      <h2 class="st-detail-title">{{ mobileSection.title }}</h2>
      <UiButton size="sm" icon="solar:restart-bold" aria-label="Refresh all data" @click="handleRefresh" />
    </header>

    <!-- ── header (same visual language as Accounts) ─────────── -->
    <header v-else class="st-top" :class="{ 'st-top--mobile': isMobile }">
      <div class="st-brand">
        <span class="st-brand-mark"><AppIcon name="solar:settings-bold" :size="20" /></span>
        <div class="st-brand-text">
          <h2 class="st-title">Settings</h2>
          <p class="st-subtitle">Workspace preferences · connection · OAuth broker</p>
        </div>
      </div>
      <div class="st-top-actions">
        <UiBadge :label="transportShort" :tone="transportTone" icon="solar:server-bold" />
        <UiButton size="sm" icon="solar:restart-bold" @click="handleRefresh">Refresh</UiButton>
      </div>
    </header>

    <!-- ── one-line status strip (connection · broker · endpoint) ─
         Desktop keeps it pinned above the jump rail; phones fold the same
         summary into the root list as its own group (rendered inside main). -->
    <section v-if="!isMobile" class="st-strip" aria-label="Connection summary">
      <span class="st-strip-k">Connection</span>
      <span class="st-strip-v">{{ activeTransport }}</span>
      <UiBadge
        :label="supabaseConfiguredNow ? 'Broker ready' : 'Broker not set'"
        :tone="supabaseConfiguredNow ? 'success' : 'warning'"
        :dot="true"
      />
      <span class="st-mono st-strip-url">{{ effectiveApiUrl }}</span>
    </section>

    <!-- ── section jump bar (always visible, works in small windows) ─
         Desktop only: phones navigate through the root list below. -->
    <nav v-if="!isMobile" class="st-jumps" aria-label="Settings sections">
      <input
        v-model="sectionQuery"
        class="st-jump-search"
        type="search"
        placeholder="Search settings…"
        aria-label="Search settings sections"
      />
      <button
        v-for="s in filteredSections"
        :key="s.id"
        type="button"
        class="st-jump"
        :class="{ 'is-active': activeSection === s.id }"
        :aria-current="activeSection === s.id ? 'true' : undefined"
        @click="scrollToSection(s)"
      >
        {{ s.label }}
      </button>
    </nav>

    <main
      ref="bodyEl"
      class="st-body"
      :class="{ 'is-detail': isMobile && !!mobileSection }"
      @scroll.passive="onBodyScroll"
    >
      <!-- ── phone root: section index (iOS/Android Settings list) ── -->
      <template v-if="isMobile && !mobileSection">
        <div class="st-search">
          <AppIcon name="solar:magnifer-bold" :size="16" class="st-search-icon" />
          <input
            v-model="sectionQuery"
            type="search"
            placeholder="Search"
            aria-label="Search settings sections"
          />
        </div>

        <ul class="st-list" role="list">
          <li v-for="s in filteredSections" :key="s.id">
            <button class="st-list-row" type="button" @click="openMobileSection(s)">
              <span class="st-list-icon"><AppIcon :name="s.icon" :size="17" /></span>
              <span class="st-list-text">
                <span class="st-list-title">{{ s.title }}</span>
                <span class="st-list-sub">{{ s.sub }}</span>
              </span>
              <span class="st-list-value">{{ sectionValue(s.id) }}</span>
              <AppIcon class="st-list-chevron" name="solar:arrow-right-bold" :size="15" />
            </button>
          </li>
        </ul>

        <div class="st-strip st-strip--mobile" aria-label="Connection summary">
          <span class="st-strip-k">Connection</span>
          <span class="st-strip-v">{{ activeTransport }}</span>
          <UiBadge
            :label="supabaseConfiguredNow ? 'Broker ready' : 'Broker not set'"
            :tone="supabaseConfiguredNow ? 'success' : 'warning'"
            :dot="true"
          />
          <span class="st-mono st-strip-url">{{ effectiveApiUrl }}</span>
        </div>

        <p class="st-foot">CyberManju OS · {{ APP_VERSION }}</p>
      </template>

      <!-- ── appearance ── -->
      <UiCard v-show="sectionVisible('appearance') && inSection('appearance')" id="st-sec-appearance" title="Appearance" icon="solar:monitor-bold" meta="View">
        <div class="st-row">
          <UiText as="span" variant="label" tone="muted">Appearance</UiText>
          <UiSegmented
            :model-value="theme.settings.followSystem ? 'auto' : theme.settings.theme"
            :options="[
              { label: 'Light', value: 'os-light' },
              { label: 'Dark', value: 'os-dark' },
              { label: 'Graphite', value: 'os-graphite' },
              { label: 'Plasma light', value: 'plasma-light' },
              { label: 'Plasma dark', value: 'plasma-dark' },
              { label: 'Auto', value: 'auto' },
            ]"
            aria-label="Appearance"
            @update:model-value="theme.setAppearance($event as 'auto' | 'os-dark' | 'os-light' | 'os-graphite' | 'plasma-dark' | 'plasma-light')"
          />
        </div>
        <div class="st-row">
          <UiText as="span" variant="label" tone="muted">Shell style</UiText>
          <UiSegmented
            :model-value="theme.shellStyle.value"
            :options="[
              { label: 'macOS', value: 'macos' },
              { label: 'Plasma', value: 'plasma' },
            ]"
            aria-label="Shell style"
            @update:model-value="theme.setShellStyle($event as ShellStyle)"
          />
        </div>
        <div class="st-row">
          <UiText as="span" variant="label" tone="muted">Wallpaper</UiText>
          <UiSelect
            :model-value="customWallpaper.kind.value ? 'custom' : theme.settings.wallpaper"
            :options="wallpaperOptions"
            aria-label="Wallpaper"
            :disabled="customWallpaper.busy.value"
            @update:model-value="setWallpaperChoice($event)"
          />
        </div>
        <div class="st-wallpaper-custom">
          <div class="st-wallpaper-custom__heading">
            <UiText as="span" variant="label" tone="muted">Custom wallpaper</UiText>
            <span v-if="customWallpaper.kind.value" class="st-wallpaper-custom__status">
              {{ customWallpaper.kind.value === 'url' ? 'Using image URL' : 'Using local image' }}
            </span>
          </div>
          <div class="st-field-row">
            <UiInput
              v-model="wallpaperUrlDraft"
              type="url"
              label="Image URL"
              placeholder="https://example.com/wallpaper.jpg"
              autocomplete="url"
              :disabled="customWallpaper.busy.value"
              @enter="applyWallpaperUrl"
            />
            <div class="st-actions">
              <UiButton
                variant="primary"
                size="sm"
                :loading="customWallpaper.busy.value"
                :disabled="customWallpaper.busy.value || !wallpaperUrlDraft.trim()"
                @click="applyWallpaperUrl"
              >Use URL</UiButton>
            </div>
          </div>
          <div class="st-wallpaper-actions">
            <input
              ref="wallpaperFileInput"
              type="file"
              accept="image/avif,image/gif,image/jpeg,image/png,image/webp"
              class="st-hidden"
              aria-label="Choose custom wallpaper file"
              @change="handleWallpaperFile"
            />
            <UiButton
              size="sm"
              icon="solar:upload-bold"
              :disabled="customWallpaper.busy.value"
              @click="openWallpaperPicker"
            >Choose image</UiButton>
            <UiButton
              v-if="customWallpaper.kind.value"
              size="sm"
              variant="ghost"
              :disabled="customWallpaper.busy.value"
              @click="clearCustomWallpaper"
            >Use preset</UiButton>
          </div>
          <p class="st-wallpaper-note">
            PNG, JPEG, WebP, GIF, or AVIF · up to 20 MiB. Files stay on this device; URL images load directly from their source.
          </p>
          <p v-if="customWallpaper.error.value" class="st-wallpaper-error" role="alert" aria-live="polite">
            {{ customWallpaper.error.value }}
          </p>
        </div>
        <div class="st-row">
          <UiText as="span" variant="label" tone="muted">Panel position</UiText>
          <UiSegmented
            :model-value="panelDocked === '1' ? 'docked' : 'floating'"
            :options="[
              { label: 'Floating', value: 'floating' },
              { label: 'Docked', value: 'docked' },
            ]"
            aria-label="Panel position (plasma)"
            @update:model-value="setPanelDocked($event === 'docked' ? '1' : '0')"
          />
        </div>
        <div class="st-row">
          <UiText as="span" variant="label" tone="muted">Accent</UiText>
          <div class="st-accents" role="group" aria-label="Accent colour">
            <button
              v-for="c in accentSwatches"
              :key="c.id"
              type="button"
              class="st-accent-dot"
              :class="{ 'is-active': (theme.settings.accent ?? '') === c.value }"
              :style="{ '--swatch-color': c.value || theme.themes[theme.settings.theme].palette.accent }"
              :title="c.label"
              :aria-label="c.label"
              :aria-pressed="(theme.settings.accent ?? '') === c.value"
              @click="theme.setAccent(c.value || null)"
            />
          </div>
        </div>
        <div class="st-row">
          <UiText as="span" variant="label" tone="muted">Density</UiText>
          <UiSelect
            :model-value="theme.settings.density"
            :options="[{ label: 'Comfortable', value: 'comfortable' }, { label: 'Compact', value: 'compact' }]"
            aria-label="Density"
            @update:model-value="theme.setDensity($event as 'comfortable' | 'compact')"
          />
        </div>
        <div class="st-row">
          <UiText as="span" variant="label" tone="muted">Chrome</UiText>
          <UiSegmented
            :model-value="theme.settings.glass === 0 ? 'solid' : 'translucent'"
            :options="[
              { label: 'Translucent', value: 'translucent' },
              { label: 'Solid', value: 'solid' },
            ]"
            aria-label="Window chrome"
            @update:model-value="theme.setGlass($event === 'solid' ? 0 : 2)"
          />
        </div>
        <div class="st-row">
          <UiText as="span" variant="label" tone="muted">Motion</UiText>
          <UiSelect
            :model-value="theme.settings.motion"
            :options="[{ label: 'Auto', value: 'auto' }, { label: 'Full', value: 'full' }, { label: 'Reduced', value: 'reduced' }]"
            aria-label="Motion"
            @update:model-value="theme.setMotion($event as 'auto' | 'full' | 'reduced')"
          />
        </div>
        <div class="st-row">
          <UiText as="span" variant="label" tone="muted">Wallpaper effects</UiText>
          <UiToggle v-model="store.matrixRainEnabled" aria-label="Wallpaper effects (opt-in canvas)" />
        </div>
        <div class="st-row">
          <UiText as="span" variant="label" tone="muted">Default view</UiText>
          <UiSelect
            :model-value="store.viewMode"
            :options="['grid', 'list', 'masonry']"
            aria-label="Default view"
            @update:model-value="store.viewMode = $event as 'grid' | 'list' | 'masonry'"
          />
        </div>
        <div class="st-row">
          <UiText as="span" variant="label" tone="muted">Sidebar expanded</UiText>
          <UiToggle
            :model-value="!store.sidebarCollapsed"
            aria-label="Sidebar expanded"
            @update:model-value="store.sidebarCollapsed = !$event"
          />
        </div>
        <div class="st-appearance-export">
          <UiButton size="sm" icon="solar:download-bold" :disabled="customWallpaper.busy.value" @click="exportAppearanceProfile">
            Export appearance profile
          </UiButton>
          <input
            ref="appearanceImportInput"
            type="file"
            accept="application/json,.json"
            class="st-hidden"
            aria-label="Choose appearance profile JSON"
            @change="handleAppearanceProfileFile"
          />
          <div class="st-actions">
            <UiButton size="sm" icon="solar:upload-bold" :disabled="customWallpaper.busy.value || appearanceImportBusy" @click="openAppearanceProfilePicker">
              Import appearance profile
            </UiButton>
            <UiButton
              v-if="pendingAppearanceProfile"
              size="sm"
              variant="primary"
              :loading="appearanceImportBusy"
              :disabled="customWallpaper.busy.value || appearanceImportBusy"
              @click="applyAppearanceProfile"
            >Apply preview</UiButton>
          </div>
          <p v-if="pendingAppearanceProfile" class="st-wallpaper-note">
            Preview: {{ pendingAppearanceProfile.settings.theme }} · {{ pendingAppearanceProfile.shellStyle }} shell ·
            {{ pendingAppearanceProfile.wallpaper.kind === 'url' ? 'URL wallpaper' : pendingAppearanceProfile.wallpaper.kind === 'preset' ? `preset ${pendingAppearanceProfile.wallpaper.id}` : 'local image (unsupported)' }}.
            Preferences apply only after the existing wallpaper service succeeds.
          </p>
          <p class="st-wallpaper-note">
            Versioned JSON keeps appearance preferences. The native GPUI consumer initially applies theme colors and accent only; density, shell style, panel position, wallpaper, and other settings are retained but not applied. This is not OS live sync or evidence of Asterinas GUI acceptance.
          </p>
          <p v-if="customWallpaper.kind.value === 'file'" class="st-wallpaper-note">
            The selected local file remains on this device and is not included in the export. Native GPUI import will report this wallpaper as unsupported.
          </p>
          <p v-if="appearanceExportStatus" class="st-wallpaper-note" role="status" aria-live="polite">
            {{ appearanceExportStatus }}
          </p>
          <p v-if="appearanceImportStatus" class="st-wallpaper-note" role="status" aria-live="polite">
            {{ appearanceImportStatus }}
          </p>
        </div>
        <div class="st-row">
          <UiText as="span" variant="label" tone="muted">{{ t('settings.language') }}</UiText>
          <UiSelect
            :model-value="locale"
            :options="LOCALES.map((l) => ({ label: l.label, value: l.id }))"
            aria-label="Interface language"
            @update:model-value="setLanguage($event as Locale)"
          />
        </div>
        <p class="st-hint text-muted">{{ t('settings.languageHint') }}</p>
      </UiCard>

      <!-- ── remote dashboard ── -->
      <UiCard v-show="sectionVisible('remote') && inSection('remote')" id="st-sec-remote" title="Remote dashboard" icon="solar:server-bold" meta="Connection">
        <div class="st-field-row">
          <UiInput
            v-model="serverUrlDraft"
            label="Dashboard URL"
            placeholder="https://my-server:3456 (empty = auto)"
            prefix-icon="solar:link-bold"
            clearable
            @enter="saveServerUrl"
          />
          <div class="st-actions">
            <UiButton variant="primary" size="sm" icon="solar:diskette-bold" @click="saveServerUrl">Save</UiButton>
            <UiButton v-if="serverUrlDraft || currentServerUrl" size="sm" icon="solar:close-bold" @click="clearServerUrl">Forget</UiButton>
          </div>
        </div>
        <UiText as="p" variant="small" tone="muted">
          Static build + your own server = full OAuth, sync + quota. Page reloads to reconnect.
        </UiText>
      </UiCard>

      <!-- ── supabase broker ── -->
      <UiCard
        v-show="sectionVisible('broker') && inSection('broker')"
        id="oauth-broker-card"
        :class="{ 'is-target': focusFlash }"
        title="OAuth broker"
        icon="solar:key-bold"
        meta="Supabase"
        :accent="!supabaseConfiguredNow"
      >
        <template #actions>
          <UiBadge
            :label="supabaseConfiguredNow ? 'Configured' : 'Not configured'"
            :tone="supabaseConfiguredNow ? 'success' : 'warning'"
            :dot="true"
            :title="supabaseUrlDraft || undefined"
          />
        </template>
        <div class="st-stack">
          <UiInput
            v-model="supabaseUrlDraft"
            label="Project URL"
            placeholder="https://xyzcompany.supabase.co"
            prefix-icon="solar:link-bold"
            autocomplete="off"
            clearable
            @update:model-value="brokerMsg = null"
          />
          <div class="st-field-row">
            <UiInput
              v-model="supabaseKeyDraft"
              label="Anon / publishable key"
              type="password"
              :placeholder="supabaseConfiguredNow ? '•••••• — stored, paste to rotate' : 'sb_publishable_… or eyJ…'"
              prefix-icon="solar:lock-bold"
              autocomplete="off"
              clearable
              @update:model-value="brokerMsg = null"
            />
            <div class="st-actions">
              <UiButton variant="primary" size="sm" icon="solar:diskette-bold" @click="saveSupabase">Save</UiButton>
              <UiButton v-if="supabaseConfiguredNow" size="sm" icon="solar:close-bold" @click="clearSupabase">Forget</UiButton>
            </div>
          </div>
          <p v-if="brokerMsg" class="st-feedback" :class="`is-${brokerMsg.tone}`" role="status">{{ brokerMsg.text }}</p>
          <UiText as="p" variant="small" tone="muted">
            Static-build OAuth broker: GitHub / Google / GitLab login without your own server.
            Enable the providers in Supabase → Authentication → Sign-in, and add this page's URL
            — plus <code>cybermanju://oauth/callback</code> for the desktop/mobile apps — to redirect URLs.
            <span v-if="supabaseConfiguredNow">Source: {{ supabaseSourceLabel }}.</span>
            The paste lives on this device only — repeat it on each device.
            Provider tokens you mint then reach your other devices through your encrypted sync.
            See the <a class="st-legal" :href="legalPageUrl('privacy.html')" target="_blank" rel="noopener">Privacy policy</a>
            and <a class="st-legal" :href="legalPageUrl('terms.html')" target="_blank" rel="noopener">Terms of service</a>
            for how provider data is handled.
          </UiText>
          <div v-if="!supabaseConfiguredNow" class="st-banner warn">
            <AppIcon name="solar:info-circle-bold" :size="15" />
            <span>Accounts sign-in and provider OAuth need this broker. Paste the URL + key, then enable providers in Supabase.</span>
          </div>
        </div>
      </UiCard>

      <!-- ── auto-refresh + data ── -->
      <UiCard v-show="sectionVisible('sync') && inSection('sync')" id="st-sec-sync" title="Sync behaviour" icon="solar:refresh-bold" meta="Refresh">
        <div class="st-row">
          <UiText as="span" variant="label" tone="muted">Auto-refresh</UiText>
          <UiSelect
            :model-value="String(store.autoRefreshInterval)"
            :options="REFRESH_OPTIONS"
            aria-label="Auto-refresh interval"
            @update:model-value="store.autoRefreshInterval = Number($event)"
          />
        </div>
        <UiDivider />
        <UiButton block icon="solar:refresh-bold" @click="handleRefresh">Refresh all data</UiButton>
        <UiText as="p" variant="small" tone="muted">Re-fetch files, accounts, collections, face groups and sync configs.</UiText>
        <UiDivider />
        <div v-if="nativeMirrorAvailable" class="st-mirror">
          <div class="st-row">
            <div class="st-stack">
              <UiText as="span" variant="label">Mobile Vault Mirror</UiText>
              <UiText as="span" variant="small" tone="muted">
                {{ mobileVaultMirrorState.configured ? `${mobileVaultMirrorState.folderName} / ${mobileVaultMirrorState.fileName}` : 'Scoped Storage · encrypted web-compatible .cybermanju' }}
              </UiText>
            </div>
            <UiBadge :label="mirrorStatusLabel" :tone="mirrorStatusTone" :dot="true" />
          </div>
          <UiText as="p" variant="small" tone="muted">
            {{ mobileVaultMirrorState.lastSyncedAt ? `Last synced ${formatMirrorTime(mobileVaultMirrorState.lastSyncedAt)}` : 'The selected folder is updated after successful database changes.' }}
          </UiText>
          <UiInput
            v-if="mobileVaultMirrorState.configured && !mobileVaultMirrorState.unlocked"
            v-model="mirrorPassphrase"
            label="Vault passphrase"
            type="password"
            autocomplete="current-password"
            placeholder="Enter the mirror passphrase"
            @enter="unlockMirror"
          />
          <div class="st-actions">
            <UiButton
              v-if="mobileVaultMirrorState.configured && !mobileVaultMirrorState.unlocked"
              variant="primary"
              size="sm"
              icon="solar:lock-keyhole-bold"
              :disabled="mirrorActionBusy || mobileVaultMirrorState.busy || !mirrorPassphrase"
              @click="unlockMirror"
            >Unlock</UiButton>
            <UiButton
              v-if="mobileVaultMirrorState.configured && mobileVaultMirrorState.unlocked"
              size="sm"
              icon="solar:refresh-bold"
              :disabled="mirrorActionBusy || mobileVaultMirrorState.busy"
              @click="syncMirrorNow"
            >{{ mobileVaultMirrorState.busy ? 'Syncing…' : 'Sync now' }}</UiButton>
            <UiButton
              v-if="mobileVaultMirrorState.configured && mobileVaultMirrorState.unlocked"
              size="sm"
              icon="solar:lock-bold"
              :disabled="mobileVaultMirrorState.busy"
              @click="lockMirror"
            >Lock</UiButton>
          </div>
          <UiText v-if="mobileVaultMirrorState.lastError" as="p" variant="small" tone="danger" role="alert">
            {{ mobileVaultMirrorState.lastError }}
          </UiText>
          <UiText v-else-if="!mobileVaultMirrorState.configured" as="p" variant="small" tone="muted">
            Configure a folder and passphrase from Setup → Providers → Local folder.
          </UiText>
        </div>
      </UiCard>

      <!-- ── gestures ── -->
      <UiCard v-if="touchConfig" v-show="sectionVisible('gestures') && inSection('gestures')" id="st-sec-gestures" title="Gestures" icon="solar:cursor-square-bold" :meta="touchMeta">
        <UiText as="p" variant="small" tone="muted">
          Device: {{ touchConfig.state.touchSupported ? 'touch enabled' : 'no touch' }} ·
          {{ touchConfig.state.isMobile ? 'mobile' : 'desktop' }}
        </UiText>
        <div class="st-table st-table--gestures" role="table" aria-label="Gesture bindings">
          <div v-for="gesture in touchConfig.getAllGestures()" :key="gesture" class="st-table-row" role="row">
            <UiText as="span" variant="small" tone="muted" truncate>{{ touchConfig.getGestureLabel(gesture) }}</UiText>
            <UiSelect
              :model-value="touchConfig.getAction(gesture)"
              :options="touchConfig.getAllActions().map(a => ({ label: touchConfig.getActionLabel(a), value: a }))"
              :aria-label="`Action for ${touchConfig.getGestureLabel(gesture)}`"
              @update:model-value="onGestureChange(gesture, $event)"
            />
            <UiButton size="xs" icon="solar:undo-left-round-bold" aria-label="Reset gesture" @click="onGestureReset(gesture)" />
          </div>
        </div>
        <div class="st-grid-2">
          <UiInput
            :model-value="String(touchConfig.state.threshold)"
            label="Swipe threshold"
            type="number"
            :min="0"
            :step="1"
            @update:model-value="onThreshold"
          />
          <UiInput
            :model-value="String(touchConfig.state.longPressThreshold)"
            label="Long press (ms)"
            type="number"
            :min="0"
            :step="1"
            @update:model-value="onLongPress"
          />
          <UiInput
            :model-value="String(touchConfig.state.edgeZoneSize)"
            label="Edge zone (px)"
            type="number"
            :min="0"
            :step="1"
            @update:model-value="onEdgeZone"
          />
          <UiInput
            :model-value="String(touchConfig.state.doubleTapTimeout)"
            label="Double tap (ms)"
            type="number"
            :min="0"
            :step="1"
            @update:model-value="onDoubleTap"
          />
        </div>
        <div class="st-actions">
          <UiButton size="sm" icon="solar:undo-left-round-bold" @click="touchConfig.resetAll()">Reset all</UiButton>
          <UiButton size="sm" icon="solar:download-bold" @click="exportTouchConfig">Export</UiButton>
          <UiButton size="sm" icon="solar:upload-bold" @click="importTouchConfig">Import</UiButton>
        </div>
        <input ref="touchImportRef" type="file" accept=".json" class="st-hidden" @change="handleTouchImport" />
      </UiCard>

      <!-- ── keyboard ── -->
      <UiCard v-if="shortcuts" v-show="sectionVisible('keys') && inSection('keys')" id="st-sec-keys" title="Keyboard bindings" icon="solar:keyboard-bold" :meta="keyMeta">
        <UiText as="p" variant="small" tone="muted">
          {{ isBrowserKeys ? 'Browser tab: Ctrl+T / Ctrl+W / Ctrl+Tab never reach the page — Alt+ fallbacks are listed.' : 'Tauri desktop: every binding fires, including Ctrl+T / Ctrl+W.' }}
          Window layout lives under WINDOWS / WORKSPACE · LAYOUT.
        </UiText>
        <UiInput
          v-model="keyFilter"
          label="Filter bindings"
          placeholder="Action, key or group…"
          prefix-icon="solar:magnifier-bold"
          clearable
        />
        <div class="st-table st-table--keys" role="table" aria-label="Keyboard bindings">
          <div v-for="sc in filteredShortcuts" :key="sc.action" class="st-table-row" role="row">
            <UiText as="span" variant="small" tone="muted" truncate>{{ sc.description }}</UiText>
            <input
              class="st-key-input"
              :value="getBindingDisplay(sc.action)"
              :placeholder="sc.keys"
              :aria-label="`Shortcut for ${sc.description}`"
              readonly
              @focus="startRebind(sc.action, $event)"
              @keydown="captureRebind($event)"
            />
            <span v-if="sc.blockedInBrowser" class="st-key-fb" :title="`Browser fallback: ${sc.fallback}`">→ {{ sc.fallback }}</span>
            <UiButton size="xs" icon="solar:undo-left-round-bold" aria-label="Reset to default" @click="resetBinding(sc.action)" />
          </div>
          <p v-if="filteredShortcuts.length === 0" class="st-empty" role="status">
            No binding matches “{{ keyFilter }}”. Clear the filter to see all {{ shortcuts.getAllShortcuts().length }}.
          </p>
        </div>
        <div class="st-actions">
          <UiButton size="sm" icon="solar:download-bold" @click="exportKeymap">Export</UiButton>
          <UiButton size="sm" icon="solar:upload-bold" @click="importKeymap">Import</UiButton>
          <UiButton size="sm" icon="solar:undo-left-round-bold" @click="resetAllBindings">Reset all</UiButton>
        </div>
        <input ref="importInputRef" type="file" accept=".kpl,.kpd,.json" class="st-hidden" @change="handleImportFile" />
      </UiCard>

      <!-- ── about ── -->
      <UiCard v-show="sectionVisible('about') && inSection('about')" id="st-sec-about" title="About" icon="solar:info-circle-bold" :meta="APP_VERSION">
        <dl class="st-info">
          <div class="st-info-row"><dt>Version</dt><dd>{{ APP_VERSION }}</dd></div>
          <div class="st-info-row"><dt>Framework</dt><dd>Vue 3 + Pinia</dd></div>
          <div class="st-info-row"><dt>Desktop</dt><dd>Tauri v2</dd></div>
          <div class="st-info-row"><dt>Search</dt><dd>Tantivy BM25</dd></div>
          <div class="st-info-row"><dt>Encryption</dt><dd>RustPQ (PQC)</dd></div>
          <div class="st-info-row"><dt>Database</dt><dd>redb</dd></div>
        </dl>
      </UiCard>
      <!-- iOS "No Results": the filter hid every group (root list on phones,
           the whole card stack on desktop). -->
      <p v-if="filteredSections.length === 0 && (!isMobile || !mobileSection)" class="st-empty" role="status">
        No results for “{{ sectionQuery }}”.
        <button class="st-link" type="button" @click="sectionQuery = ''">Clear search</button>
      </p>
    </main>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { ref, inject, computed, onMounted, onBeforeUnmount } from 'vue'
import { useAppStore } from '@/stores/app'
import { isTauri, getServerUrl, setServerUrl } from '@/composables/useTauri'
import { useTransport } from '@/composables/useTransport'
import { legalPageUrl } from '@/utils/legalPageUrl'
import { useCustomWallpaper } from '@/composables/useCustomWallpaper'
import {
  getSupabaseConfig,
  setSupabaseConfig,
  clearSupabaseConfig,
  supabaseConfigured,
  supabaseSignOut,
} from '@/composables/useSupabase'
import { ShortcutsKey } from '@/composables/shortcutsKey'
import { useTouchConfig, type GestureType, type TouchAction } from '@/composables/useTouchConfig'
import { useTheme } from '@/composables/useTheme'
import { ACCENT_CHOICES, WALLPAPERS, type ShellStyle } from '@/ui/tokens'
import {
  applyAppearanceProfileRestore,
  prepareAppearanceProfileRestore,
  serializeAppearanceProfileV1,
  type AppearanceProfileV1,
} from '@/utils/appearanceProfile'
import { lockMobileVaultMirror, mobileVaultMirrorState, supportsNativeScopedStorage, syncMobileVaultMirrorNow, unlockMobileVaultMirror } from '@/utils/mobileScopedStorage'
import { LOCALES, getLocale, t, useLocale, type Locale } from '@/utils/i18n'
import pkg from '../../package.json'

/** package.json is the single source of truth for the version (AGENTS §0) —
  About, the phone root footer and the list cell all read it from there. */
const APP_VERSION = pkg.version

/**
 * Active transport: tauri IPC, REST dashboard, or local WASM (GitHub Pages).
 * Single source of truth is `useTransport()` (tauri → static-host/wasm →
 * rest); `VITE_TRANSPORT` ('tauri'|'wasm'|'rest', aliases
 * 'desktop'/'static'/'pages'/'web') forces a value when set at build time.
 * `isStaticHost()` is the sync first-paint signal — never branch the label
 * on async `wasmBackendActive()` (P1-4 flicker).
 */
const activeTransport = computed(() => useTransport().label)

const transportShort = computed(() => {
  const short = useTransport().short
  return short === 'TAURI' ? 'Tauri' : short === 'WASM' ? 'WASM' : 'Web'
})

const transportTone = computed<'neutral' | 'accent' | 'success' | 'warning' | 'danger' | 'info'>(
  () => useTransport().tone,
)

const store = useAppStore()
const theme = useTheme()
// Interface language (i18n foundation, ANALYSIS.md §3) — new surfaces read
// it via t(); older panels migrate as they are touched.
const locale = ref<Locale>(getLocale())
function setLanguage(next: Locale) {
  locale.value = next
  useLocale(next)
}
const customWallpaper = useCustomWallpaper()
const wallpaperUrlDraft = ref('')
const wallpaperFileInput = ref<HTMLInputElement | null>(null)
const appearanceImportInput = ref<HTMLInputElement | null>(null)
const appearanceExportStatus = ref('')
const appearanceImportStatus = ref('')
const pendingAppearanceProfile = ref<AppearanceProfileV1 | null>(null)
const appearanceImportBusy = ref(false)
const nativeMirrorAvailable = supportsNativeScopedStorage()
const mirrorPassphrase = ref('')
const mirrorActionBusy = ref(false)
const mirrorStatusLabel = computed(() => {
  if (!mobileVaultMirrorState.configured) return 'Not configured'
  if (mobileVaultMirrorState.busy) return 'Syncing'
  return mobileVaultMirrorState.unlocked ? 'Live Sync on' : 'Locked'
})
const mirrorStatusTone = computed<'neutral' | 'success' | 'warning'>(() => {
  if (!mobileVaultMirrorState.configured) return 'neutral'
  return mobileVaultMirrorState.unlocked ? 'success' : 'warning'
})
function formatMirrorTime(timestamp: number): string {
  return new Date(timestamp).toLocaleString()
}
async function unlockMirror() {
  if (mirrorActionBusy.value || !mirrorPassphrase.value) return
  mirrorActionBusy.value = true
  try {
    await unlockMobileVaultMirror(mirrorPassphrase.value)
  } catch {
    // The mirror state exposes a safe, user-visible error; never log the passphrase.
  } finally {
    mirrorPassphrase.value = ''
    mirrorActionBusy.value = false
  }
}
async function syncMirrorNow() {
  if (mirrorActionBusy.value) return
  mirrorActionBusy.value = true
  try {
    await syncMobileVaultMirrorNow()
  } catch {
    // The mirror state exposes the failure inline.
  } finally {
    mirrorActionBusy.value = false
  }
}
function lockMirror() {
  lockMobileVaultMirror()
  mirrorPassphrase.value = ''
}
const accentSwatches = ACCENT_CHOICES
const wallpaperOptions = computed(() => [
  ...WALLPAPERS.map((wallpaper) => ({ label: wallpaper.label, value: wallpaper.id })),
  ...(customWallpaper.kind.value ? [{ label: 'Custom image', value: 'custom' }] : []),
])

function setWallpaperChoice(value: string) {
  if (value === 'custom') return
  theme.setWallpaper(value)
  if (customWallpaper.kind.value) void customWallpaper.clear()
}

async function applyWallpaperUrl() {
  if (customWallpaper.busy.value) return
  if (await customWallpaper.setUrl(wallpaperUrlDraft.value)) {
    wallpaperUrlDraft.value = customWallpaper.url.value
  }
}

function openWallpaperPicker() {
  if (!customWallpaper.busy.value) wallpaperFileInput.value?.click()
}

async function handleWallpaperFile(event: Event) {
  const input = event.currentTarget as HTMLInputElement
  const file = input.files?.[0]
  if (!file || customWallpaper.busy.value) {
    input.value = ''
    return
  }
  try {
    if (await customWallpaper.setFile(file)) wallpaperUrlDraft.value = ''
  } finally {
    input.value = ''
  }
}

function clearCustomWallpaper() {
  void customWallpaper.clear()
}

const panelDocked = ref('0')
try {
  panelDocked.value = localStorage.getItem('cybermanju_panel_docked') || '0'
} catch {
  panelDocked.value = '0'
}
function setPanelDocked(v: string | number) {
  panelDocked.value = String(v)
  try {
    localStorage.setItem('cybermanju_panel_docked', panelDocked.value)
  } catch { /* session-only */ }
}

function exportAppearanceProfile() {
  try {
    const custom = customWallpaper.kind.value === 'url'
      ? { kind: 'url' as const, url: customWallpaper.url.value }
      : customWallpaper.kind.value === 'file'
        ? {
            kind: 'file' as const,
            name: customWallpaper.fileName.value,
            mimeType: customWallpaper.fileMimeType.value,
            size: customWallpaper.fileSize.value,
          }
        : { kind: null as null }
    const json = serializeAppearanceProfileV1({
      settings: theme.settings,
      shellStyle: theme.shellStyle.value,
      panelDocked: panelDocked.value === '1',
      customWallpaper: custom,
    })
    const data = new Blob([json], { type: 'application/json' })
    const objectUrl = URL.createObjectURL(data)
    const link = document.createElement('a')
    link.href = objectUrl
    link.download = 'cybermanju-appearance-v1.json'
    link.click()
    window.setTimeout(() => URL.revokeObjectURL(objectUrl), 0)
    appearanceExportStatus.value = 'Appearance profile exported.'
  } catch (cause) {
    appearanceExportStatus.value = cause instanceof Error
      ? `Profile export failed: ${cause.message}`
      : 'Profile export failed because appearance state could not be validated.'
  }
}

function openAppearanceProfilePicker() {
  if (!customWallpaper.busy.value && !appearanceImportBusy.value) {
    appearanceImportInput.value?.click()
  }
}

async function handleAppearanceProfileFile(event: Event) {
  const input = event.currentTarget as HTMLInputElement
  const file = input.files?.[0]
  pendingAppearanceProfile.value = null
  if (!file) return
  try {
    if (file.size > 64 * 1024) throw new Error('Appearance profile exceeds the 64 KiB limit.')
    const profile = prepareAppearanceProfileRestore(await file.text())
    pendingAppearanceProfile.value = profile
    appearanceImportStatus.value = 'Profile validated. Review its preview, then explicitly apply it.'
  } catch (cause) {
    appearanceImportStatus.value = cause instanceof Error
      ? `Profile import refused: ${cause.message}`
      : 'Profile import refused because it could not be validated.'
  } finally {
    input.value = ''
  }
}

async function applyAppearanceProfile() {
  const profile = pendingAppearanceProfile.value
  if (!profile || customWallpaper.busy.value || appearanceImportBusy.value) return
  appearanceImportBusy.value = true
  appearanceImportStatus.value = ''
  try {
    const applied = await applyAppearanceProfileRestore(profile, {
      async applyWallpaper(wallpaper) {
        if (wallpaper.kind === 'url') return customWallpaper.setUrl(wallpaper.url)
        await customWallpaper.load()
        return customWallpaper.kind.value ? customWallpaper.clear() : true
      },
      applyPreferences(validated) {
        Object.assign(theme.settings, validated.settings)
        theme.setShellStyle(validated.shellStyle)
        setPanelDocked(validated.panelDocked ? '1' : '0')
        wallpaperUrlDraft.value = customWallpaper.kind.value === 'url' ? customWallpaper.url.value : ''
      },
    })
    if (!applied) {
      appearanceImportStatus.value = 'Restore not applied: wallpaper storage failed. Existing preferences were left unchanged.'
      return
    }
    pendingAppearanceProfile.value = null
    appearanceImportStatus.value = 'Applied to this session through the existing preference and wallpaper services. Preference persistence remains subject to each existing store’s storage availability and limits.'
  } catch (cause) {
    appearanceImportStatus.value = cause instanceof Error
      ? `Restore failed before preference changes: ${cause.message}`
      : 'Restore failed before preference changes.'
  } finally {
    appearanceImportBusy.value = false
  }
}

onMounted(async () => {
  await customWallpaper.load()
  if (!wallpaperUrlDraft.value && customWallpaper.url.value) {
    wallpaperUrlDraft.value = customWallpaper.url.value
  }
})

const shortcuts = inject(ShortcutsKey, null)
const isBrowserKeys = computed(() => !isTauri())
const touchConfig = useTouchConfig()
const touchMeta = computed(() =>
  touchConfig.state.touchSupported ? (touchConfig.state.isMobile ? 'Touch · mobile' : 'Touch · desktop') : 'No touch',
)

const REFRESH_OPTIONS = [
  { label: 'Disabled', value: '0' },
  { label: 'Every 10 seconds', value: '10' },
  { label: 'Every 30 seconds', value: '30' },
  { label: 'Every minute', value: '60' },
  { label: 'Every 5 minutes', value: '300' },
]

const currentServerUrl = computed(() => getServerUrl())
const serverUrlDraft = ref(getServerUrl())
const effectiveApiUrl = computed(() => {
  // Preserve case: upper-casing breaks copy-paste (HTTPS://…).
  if (currentServerUrl.value) return currentServerUrl.value
  if (isTauri()) return 'TAURI IPC + http://localhost:3456'
  return 'http://localhost:3456'
})

function saveServerUrl() {
  setServerUrl(serverUrlDraft.value.trim())
  window.location.reload()
}

function clearServerUrl() {
  serverUrlDraft.value = ''
  setServerUrl('')
  window.location.reload()
}

const supabaseUrlDraft = ref(getSupabaseConfig().url)
const supabaseKeyDraft = ref('')
const supabaseConfiguredNow = computed(() => supabaseConfigured())
/** Where the broker credentials came from — a Settings paste or the baked build pair. */
const supabaseSource = computed(() => getSupabaseConfig().source)
const supabaseSourceLabel = computed(() =>
  supabaseSource.value === 'settings'
    ? 'saved in Settings on this device'
    : supabaseSource.value === 'build-env'
      ? 'baked into this build'
      : 'none',
)
/** Inline result of the last Save / Forget (clears as soon as a field is edited). */
const brokerMsg = ref<{ text: string; tone: 'ok' | 'err' } | null>(null)

function saveSupabase() {
  const url = supabaseUrlDraft.value.trim()
  // Keep the already-stored/baked key when rotating only the URL — the
  // password field stays empty by design (we never echo the secret back).
  const key = supabaseKeyDraft.value.trim() || getSupabaseConfig().key
  const err = setSupabaseConfig(url, key)
  if (err) {
    brokerMsg.value = { text: err, tone: 'err' }
    return
  }
  supabaseUrlDraft.value = getSupabaseConfig().url
  supabaseKeyDraft.value = ''
  brokerMsg.value = { text: 'Broker saved — this window, the Accounts panel and OAuth sign-in pick it up right away.', tone: 'ok' }
  store.notifySuccess('OAuth broker configured')
}

async function clearSupabase() {
  await supabaseSignOut().catch(() => {})
  clearSupabaseConfig()
  supabaseUrlDraft.value = getSupabaseConfig().url
  supabaseKeyDraft.value = ''
  brokerMsg.value = getSupabaseConfig().url || getSupabaseConfig().key
    ? { text: 'Settings paste forgotten — fell back to the pair baked into this build.', tone: 'ok' }
    : { text: 'Broker forgotten — OAuth sign-in stays off until you save credentials again.', tone: 'ok' }
}

// ── section jump bar + scroll spy ──────────────────────────────
/** One settings group: `label` is the short chip name (desktop jump rail),
  `title` the full row/nav name, `icon`/`sub` the phone list cell. */
interface Section {
  id: string
  el: string
  label: string
  title: string
  icon: string
  sub: string
}

const SECTIONS: Section[] = [
  { id: 'appearance', el: 'st-sec-appearance', label: 'Appearance', title: 'Appearance', icon: 'solar:monitor-bold', sub: 'Theme · wallpaper · language' },
  { id: 'remote', el: 'st-sec-remote', label: 'Dashboard', title: 'Remote dashboard', icon: 'solar:server-bold', sub: 'Server URL · OAuth redirect' },
  { id: 'broker', el: 'oauth-broker-card', label: 'Broker', title: 'OAuth broker', icon: 'solar:key-bold', sub: 'Sign-in without your own server' },
  { id: 'sync', el: 'st-sec-sync', label: 'Sync', title: 'Sync behaviour', icon: 'solar:refresh-bold', sub: 'Auto-refresh · vault mirror' },
  { id: 'gestures', el: 'st-sec-gestures', label: 'Gestures', title: 'Gestures', icon: 'solar:cursor-square-bold', sub: 'Touch, swipe, long press' },
  { id: 'keys', el: 'st-sec-keys', label: 'Keys', title: 'Keyboard bindings', icon: 'solar:keyboard-bold', sub: 'Shortcuts and rebinds' },
  { id: 'about', el: 'st-sec-about', label: 'About', title: 'About', icon: 'solar:info-circle-bold', sub: 'Version & stack' },
]

const bodyEl = ref<HTMLElement | null>(null)
const activeSection = ref<string>('appearance')
/** iOS-Settings style search: matches section names AND their row content,
  so "theme" finds Appearance and "oauth" finds the broker card. */
const SECTION_KEYWORDS: Record<string, string[]> = {
  appearance: ['theme', 'wallpaper', 'accent', 'density', 'chrome', 'translucent', 'motion', 'matrix', 'sidebar', 'view', 'shell', 'plasma', 'panel'],
  remote: ['server', 'url', 'dashboard', 'endpoint', 'connection', 'static'],
  broker: ['oauth', 'supabase', 'github', 'google', 'gitlab', 'login', 'sign-in', 'key', 'privacy', 'terms'],
  sync: ['refresh', 'auto', 'mirror', 'vault', 'passphrase', 'interval', 'fetch'],
  gestures: ['touch', 'swipe', 'tap', 'press', 'edge', 'mobile'],
  keys: ['keyboard', 'shortcut', 'binding', 'rebind', 'hotkey', 'kpl'],
  about: ['version', 'framework', 'tauri', 'tantivy', 'encryption', 'redb', 'vue'],
}
const sectionQuery = ref('')
const filteredSections = computed(() => {
  const q = sectionQuery.value.trim().toLowerCase()
  if (!q) return SECTIONS
  return SECTIONS.filter((s) =>
    s.label.toLowerCase().includes(q) ||
    (SECTION_KEYWORDS[s.id] ?? []).some((k) => k.includes(q) || q.includes(k)),
  )
})
/** Card-level filter twin of the chip filter: searching hides whole groups,
  like iOS Settings search, instead of only filtering the jump chips. */
function sectionVisible(id: string): boolean {
  if (!sectionQuery.value.trim()) return true
  return filteredSections.value.some((s) => s.id === id)
}

// ── phone stack: root section index → pushed detail ─────────────
/**
 * On phones the sheet is full-screen, so it uses the standard settings
 * navigation every platform app uses: a root list of groups, and exactly one
 * pushed detail behind a back bar (`.st-top.is-detail`) — instead of the
 * desktop jump rail + one long scroll. Driven by the same ≤768px viewport
 * query as the shell, so a resized desktop window keeps the full page.
 */
const mobileShellQuery =
  typeof window !== 'undefined' ? window.matchMedia?.('(max-width: 768px)') ?? null : null
const isMobile = ref(mobileShellQuery?.matches ?? false)
/** Currently pushed section id; `null` = root index. */
const mobileSectionId = ref<string | null>(null)
const mobileSection = computed(() => SECTIONS.find((s) => s.id === mobileSectionId.value) ?? null)

function onMobileShellChange(event: MediaQueryListEvent) {
  isMobile.value = event.matches
  // Leaving/entering the phone layout re-roots the stack (desktop has no stack).
  if (!event.matches) mobileSectionId.value = null
}

/** One card is in the DOM flow only where its section is reachable. */
function inSection(id: string): boolean {
  return !isMobile.value || mobileSectionId.value === id
}

function openMobileSection(s: Section) {
  mobileSectionId.value = s.id
  activeSection.value = s.id
  // Every push starts at the top of the group, like a native detail page.
  bodyEl.value?.scrollTo({ top: 0 })
}

function closeMobileSection() {
  mobileSectionId.value = null
  bodyEl.value?.scrollTo({ top: 0 })
}

/** Human label for each appearance preset (root list right-hand value). */
const APPEARANCE_LABELS: Record<string, string> = {
  'os-light': 'Light',
  'os-dark': 'Dark',
  'os-graphite': 'Graphite',
  'plasma-light': 'Plasma light',
  'plasma-dark': 'Plasma dark',
  auto: 'Auto',
}

/** Right-hand value shown on the root list cell (iOS chevron row). */
function sectionValue(id: string): string {
  switch (id) {
    case 'appearance':
      return theme.settings.followSystem
        ? 'Auto'
        : APPEARANCE_LABELS[theme.settings.theme] ?? 'Custom'
    case 'remote':
      return currentServerUrl.value ? 'Custom URL' : 'Auto'
    case 'broker':
      return supabaseConfiguredNow.value ? 'Configured' : 'Not set'
    case 'sync':
      return store.autoRefreshInterval === 0
        ? 'Off'
        : store.autoRefreshInterval >= 60
          ? `${store.autoRefreshInterval / 60}m`
          : `${store.autoRefreshInterval}s`
    case 'gestures':
      return touchMeta.value
    case 'keys':
      return `${shortcuts?.getAllShortcuts().length ?? 0} bindings`
    default:
      return APP_VERSION
  }
}

/** Android system back / Esc: pop the detail first, never the whole sheet. */
function handleMobileBack(e: Event) {
  if (!isMobile.value || !mobileSectionId.value) return
  // Background sheets stay mounted: only the sheet on top claims the press.
  if ((e as CustomEvent<{ panel?: string }>).detail?.panel !== 'settings') return
  e.preventDefault()
  closeMobileSection()
}

function handleStackEscape(e: KeyboardEvent) {
  if (e.key !== 'Escape' || !isMobile.value || !mobileSectionId.value) return
  closeMobileSection()
}

onMounted(() => {
  mobileShellQuery?.addEventListener?.('change', onMobileShellChange)
  window.addEventListener('cybermanju:mobile-back', handleMobileBack)
  document.addEventListener('keydown', handleStackEscape)
})
onBeforeUnmount(() => {
  mobileShellQuery?.removeEventListener?.('change', onMobileShellChange)
  window.removeEventListener('cybermanju:mobile-back', handleMobileBack)
  document.removeEventListener('keydown', handleStackEscape)
})

function prefersReducedMotion(): boolean {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

function scrollToSection(s: Section, smooth = true): boolean {
  const el = document.getElementById(s.el)
  if (!el) return false
  activeSection.value = s.id
  el.scrollIntoView({ block: 'start', behavior: smooth && !prefersReducedMotion() ? 'smooth' : 'auto' })
  return true
}

function onBodyScroll() {
  // The phone stack picks the section explicitly (push/back), and its root
  // list has no cards to spy on.
  if (isMobile.value) return
  const body = bodyEl.value
  if (!body) return
  const base = body.getBoundingClientRect().top
  let current = SECTIONS[0].id
  for (const s of SECTIONS) {
    const el = document.getElementById(s.el)
    // v-show-hidden cards (search filter) report zero rects — skip them so
    // the spy never parks on an invisible section.
    if (!el || el.offsetParent === null) continue
    if (el.getBoundingClientRect().top - base <= 56) current = s.id
  }
  activeSection.value = current
}

// ── deep link: Accounts → "Configure" lands on this card ───────
const focusFlash = ref(false)
let flashTimer: ReturnType<typeof setTimeout> | null = null

function focusSection(e: Event) {
  if ((e as CustomEvent<string>).detail !== 'oauth-broker') return
  const broker = SECTIONS.find(s => s.id === 'broker')
  if (!broker) return
  // Phones have no jump rail to scroll: push the detail instead.
  if (isMobile.value) openMobileSection(broker)
  else if (!scrollToSection(broker)) return
  if (flashTimer) clearTimeout(flashTimer)
  focusFlash.value = false
  // Reset first so the glow replays when this card is requested twice.
  requestAnimationFrame(() => {
    focusFlash.value = true
    flashTimer = setTimeout(() => {
      focusFlash.value = false
    }, 2400)
  })
}

onMounted(() => window.addEventListener('cybermanju:settings-focus', focusSection))
onMounted(async () => {
  await customWallpaper.load()
  if (!wallpaperUrlDraft.value && customWallpaper.url.value) {
    wallpaperUrlDraft.value = customWallpaper.url.value
  }
})
onBeforeUnmount(() => {
  window.removeEventListener('cybermanju:settings-focus', focusSection)
  if (flashTimer) clearTimeout(flashTimer)
})

const rebindInputs: Record<string, HTMLInputElement> = {}
const rebindingAction = ref<string | null>(null)
const importInputRef = ref<HTMLInputElement | null>(null)

/** 98 bindings is too many to scan in a small window — filter by action, label or keys. */
const keyFilter = ref('')

const filteredShortcuts = computed(() => {
  const all = shortcuts?.getAllShortcuts() ?? []
  const q = keyFilter.value.trim().toLowerCase()
  if (!q) return all
  return all.filter(sc =>
    sc.description.toLowerCase().includes(q) ||
    sc.action.toLowerCase().includes(q) ||
    sc.group.toLowerCase().includes(q) ||
    sc.keys.toLowerCase().includes(q) ||
    getBindingDisplay(sc.action).toLowerCase().includes(q),
  )
})

const keyMeta = computed(() => {
  const total = shortcuts?.getAllShortcuts().length ?? 0
  const shown = filteredShortcuts.value.length
  return shown === total ? `${total} bindings` : `${shown} of ${total} bindings`
})

function getBindingDisplay(action: string): string {
  return shortcuts?.getShortcut(action)?.replace(/,/g, ', ') || ''
}

function startRebind(action: string, e: FocusEvent) {
  rebindingAction.value = action
  const input = e.target as HTMLInputElement
  rebindInputs[action] = input
  input.value = 'Press keys…'
  input.select()
}

function captureRebind(e: KeyboardEvent) {
  const action = rebindingAction.value
  if (!action) return
  e.preventDefault()
  e.stopPropagation()
  // Escape / Enter cancel instead of being bound as shortcuts.
  if (e.key === 'Escape' || e.key === 'Enter') {
    const el = rebindInputs[action]
    if (el) el.value = getBindingDisplay(action)
    rebindingAction.value = null
    ;(e.target as HTMLInputElement).blur()
    return
  }
  const parts: string[] = []
  if (e.ctrlKey) parts.push('Ctrl')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')
  if (e.metaKey) parts.push('Meta')
  const key = e.key
  if (!['Control', 'Alt', 'Shift', 'Meta'].includes(key)) {
    parts.push(key === ' ' ? 'Space' : key.length === 1 ? key.toUpperCase() : key)
  }
  if (parts.length === 0) return
  const seq = parts.join('+')
  const el = rebindInputs[action]
  if (el) el.value = seq
  saveOverride(action, seq)
  rebindingAction.value = null
  ;(e.target as HTMLInputElement).blur()
}

function saveOverride(action: string, keys: string) {
  try {
    const raw = localStorage.getItem('cybermanju_keybindings')
    const overrides = raw ? JSON.parse(raw) : {}
    overrides[action] = keys
    localStorage.setItem('cybermanju_keybindings', JSON.stringify(overrides))
    window.location.reload()
  } catch {}
}

function resetBinding(action: string) {
  try {
    const raw = localStorage.getItem('cybermanju_keybindings')
    const overrides = raw ? JSON.parse(raw) : {}
    delete overrides[action]
    localStorage.setItem('cybermanju_keybindings', JSON.stringify(overrides))
    window.location.reload()
  } catch {}
}

function resetAllBindings() {
  localStorage.removeItem('cybermanju_keybindings')
  window.location.reload()
}

function exportKeymap() {
  const all = shortcuts?.getAllShortcuts() || []
  const lines = ['[Global]', 'name=Cybermanju Exported', 'version=1.0', 'description=Exported from Settings', '']
  lines.push('[Global Shortcuts]')
  for (const s of all) {
    const keys = shortcuts?.getShortcut(s.action) || s.keys
    lines.push(`${s.action}=${keys.replace(/,\s*/g, ',')}`)
  }
  const blob = new Blob([lines.join('\n')], { type: 'text/plain' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = 'cybermanju-shortcuts.kpl'
  a.click()
  URL.revokeObjectURL(url)
}

function importKeymap() {
  importInputRef.value?.click()
}

function handleImportFile(e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  const reader = new FileReader()
  reader.onload = () => {
    const text = reader.result as string
    try {
      const parsed = JSON.parse(text)
      localStorage.setItem('cybermanju_keybindings', JSON.stringify(parsed))
      window.location.reload()
    } catch {
      const overrides: Record<string, string> = {}
      for (const line of text.split('\n')) {
        const eqIdx = line.indexOf('=')
        if (eqIdx === -1 || line.startsWith('[') || line.startsWith('#') || line.startsWith(';')) continue
        const key = line.slice(0, eqIdx).trim()
        const val = line.slice(eqIdx + 1).trim()
        if (key && val) overrides[key] = val
      }
      localStorage.setItem('cybermanju_keybindings', JSON.stringify(overrides))
      window.location.reload()
    }
  }
  reader.readAsText(file)
}

const touchImportRef = ref<HTMLInputElement | null>(null)

function onThreshold(v: string) { touchConfig.setThreshold(Number(v)) }
function onLongPress(v: string) { touchConfig.setLongPressThreshold(Number(v)) }
function onEdgeZone(v: string) { touchConfig.setEdgeZoneSize(Number(v)) }
function onDoubleTap(v: string) { touchConfig.setDoubleTapTimeout(Number(v)) }

function onGestureChange(gesture: GestureType, action: string) {
  touchConfig.setAction(gesture, action as TouchAction)
}

function onGestureReset(gesture: GestureType) {
  touchConfig.resetGesture(gesture)
}

function exportTouchConfig() {
  const json = touchConfig.exportConfig()
  const blob = new Blob([json], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = 'cybermanju-touch-config.json'
  a.click()
  URL.revokeObjectURL(url)
}

function importTouchConfig() {
  touchImportRef.value?.click()
}

function handleTouchImport(e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  const reader = new FileReader()
  reader.onload = () => {
    const text = reader.result as string
    if (touchConfig.importConfig(text)) {
      window.location.reload()
    }
  }
  reader.readAsText(file)
}

async function handleRefresh() {
  await Promise.allSettled([
    store.fetchFiles(),
    store.fetchAccounts(),
    store.fetchCollections(),
    store.fetchFaceGroups(),
    store.fetchLooseGroups(),
    store.fetchEncryptionStatus(),
    store.fetchSyncConfigs(),
  ])
}
</script>

<style scoped>
.st {
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--ui-surface);
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 13px;
}

/* header — mirrors Accounts; wraps instead of overflowing a narrow window */
.st-top {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px 10px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--ui-border);
  flex-shrink: 0;
}
.st-brand { display: flex; align-items: center; gap: 8px; min-width: 0; }
.st-brand-text { min-width: 0; }
.st-brand-mark {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--ui-text) 6%, transparent);
  color: var(--ui-text-2);
  border: 1px solid var(--ui-border);
  flex-shrink: 0;
}
.st-title { margin: 0; font-size: 13px; font-weight: 600; }
.st-subtitle { margin: 0; font-size: 11px; color: var(--ui-text-3); }
.st-top-actions { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }

/* status strip — connection · broker · endpoint on one wrapping line */
.st-strip {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px 10px;
  margin: 10px 14px 0;
  padding: 8px 12px;
  border: 1px solid var(--ui-border);
  border-radius: 10px;
  background: color-mix(in srgb, var(--ui-text) 3%, transparent);
  font-size: 12px;
}
.st-strip-k {
  font-size: 11px;
  font-weight: 600;
  color: var(--ui-text-3);
}
.st-strip-v { font-weight: 600; min-width: 0; overflow-wrap: anywhere; }
.st-strip-url { margin-left: auto; }

/* jump bar — search plus one chip per card, always visible above the
   scroller. Wraps to a second row instead of relying on a hidden
   horizontal scroll (the chips overflow a default 560px window otherwise). */
.st-jumps {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  padding: 10px 14px 0;
}
.st-jump-search {
  flex: 1 1 140px;
  min-width: 120px;
  max-width: 220px;
  min-height: var(--ui-control-h);
  padding: 0 10px;
  background: var(--ui-surface-2);
  border: 1px solid var(--ui-border-strong);
  border-radius: var(--ui-radius-sm);
  color: var(--ui-text);
  font-size: 13px;
  outline: none;
}
.st-jump-search:focus {
  border-color: var(--ui-accent);
  box-shadow: var(--ui-focus-ring);
}
.st-jump-search::placeholder {
  color: var(--ui-text-3);
}
.st-jump {
  appearance: none;
  flex: 0 0 auto;
  border: 1px solid transparent;
  background: transparent;
  color: var(--ui-text-3);
  font: inherit;
  font-size: 12px;
  font-weight: 500;
  padding: 5px 9px;
  border-radius: var(--ui-radius-sm);
  cursor: pointer;
  white-space: nowrap;
  transition:
    color 0.15s var(--ui-ease-out),
    background 0.15s var(--ui-ease-out);
}
.st-jump:hover {
  color: var(--ui-text);
  background: color-mix(in srgb, var(--ui-text) 6%, transparent);
}
.st-jump.is-active {
  background: color-mix(in srgb, var(--ui-text) 9%, transparent);
  color: var(--ui-text);
  font-weight: 600;
}

/* body — BLOCK flow (cards are never flex children, so they can never be
   shrunk to slivers: their contents used to spill over every other card).
   Wraps to a second row instead of relying on a hidden horizontal scroll
   (the category chips overflow a default 560px window otherwise). */
.st-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 10px 14px 24px;
}
.st-body > * + * { margin-top: 10px; }
/* give every card body a consistent 10px rhythm (its own .ui-card__body has none) */
.st-body > * > :deep(.ui-card__body) {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.st-body [id] { scroll-margin-top: 4px; }

.st-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}
.st-row + .st-row { border-top: 1px dashed var(--ui-hairline); }
.st-stack { display: flex; flex-direction: column; gap: 10px; }

/* field + actions: wrap when tight, buttons bottom-align with the control */
.st-field-row { display: flex; flex-wrap: wrap; gap: 8px; align-items: flex-end; }
.st-field-row > :first-child { flex: 1 1 220px; min-width: 0; }
.st-actions { display: flex; flex-wrap: wrap; gap: 6px; flex-shrink: 0; }
.st-wallpaper-custom {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 0 2px;
  border-top: 1px dashed var(--ui-hairline);
}
.st-wallpaper-custom__heading,
.st-wallpaper-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 8px;
}
.st-wallpaper-custom__status,
.st-wallpaper-note { color: var(--ui-text-3); font-size: 11px; }
.st-wallpaper-note { margin: 0; line-height: 1.45; }
.st-wallpaper-error { margin: 0; color: var(--ui-danger); font-size: 12px; line-height: 1.4; }

.st-banner {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  border-radius: 10px;
  padding: 10px 12px;
  font-size: 12px;
  line-height: 1.5;
  border: 1px solid;
}
.st-banner.warn {
  border-color: color-mix(in srgb, var(--ui-warning) 55%, transparent);
  background: color-mix(in srgb, var(--ui-warning) 10%, transparent);
}
.st-feedback {
  margin: -2px 0 0;
  font-size: 11px;
  line-height: 1.45;
}
.st-feedback.is-ok { color: var(--ui-success); }
.st-feedback.is-err { color: var(--ui-danger); }

/* Legal links inside muted UiText (inherits size/tone, accent on hover). */
.st-legal { color: var(--ui-accent); text-decoration: none; }
.st-legal:hover { text-decoration: underline; }

/* Accounts → Configure points here: scroll + flash the broker card. */
#oauth-broker-card.is-target {
  border-color: var(--ui-accent);
  animation: st-card-target 2.4s ease-out 1;
}
@keyframes st-card-target {
  0% {
    box-shadow: var(--ui-focus-ring);
  }
  70% {
    box-shadow: var(--ui-focus-ring);
  }
  100% { box-shadow: none; }
}

.st-grid-2 { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 10px; }

/* accent swatches — flat colour dots, active gets a focus ring */
.st-accents {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
}
.st-accent-dot {
  appearance: none;
  position: relative;
  display: grid;
  flex: 0 0 44px;
  place-items: center;
  width: 44px;
  height: 44px;
  min-width: 44px;
  min-height: 44px;
  padding: 0;
  border-radius: 50%;
  border: 0;
  background: transparent;
  cursor: pointer;
  touch-action: manipulation;
}
.st-accent-dot::before {
  content: '';
  display: block;
  width: 24px;
  height: 24px;
  box-sizing: border-box;
  border-radius: 50%;
  border: 1px solid color-mix(in srgb, var(--ui-text) 24%, transparent);
  background: var(--swatch-color);
  transition: transform var(--ui-dur-fast) ease-out;
}
.st-accent-dot:hover,
.st-accent-dot:active { background: transparent; }
.st-accent-dot:hover::before { transform: scale(1.08); }
.st-accent-dot.is-active::before {
  outline: 2px solid var(--ui-accent);
  outline-offset: 3px;
}
.st-accent-dot:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--ui-accent) 55%, transparent);
  outline-offset: 2px;
}

/* tables: rows wrap instead of relying on viewport media queries (which never
   fire inside a windowed panel — windows resize, not the browser viewport). */
.st-table {
  border: 1px solid var(--ui-hairline);
  border-radius: 10px;
  overflow: hidden;
  max-height: 280px;
  overflow-y: auto;
}
.st-table-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 8px;
  padding: 6px 8px;
  border-bottom: 1px solid var(--ui-hairline);
}
.st-table-row:last-child { border-bottom: none; }
.st-table-row > :first-child { flex: 1 1 150px; min-width: 0; }
.st-table-row > :not(:first-child) { flex-shrink: 0; }
.st-table--gestures .st-table-row > :nth-child(2) { flex: 1 1 130px; min-width: 110px; }
.st-table--keys .st-table-row > :first-child { flex: 1 1 170px; }
.st-empty { margin: 0; padding: 14px 10px; font-size: 12px; text-align: center; color: var(--ui-text-3); }
.st-link { padding: 6px 4px; border: 0; background: none; color: var(--ui-accent); font: inherit; font-weight: 650; cursor: pointer; }
.st-key-fb {
  font-family: var(--ui-font-mono);
  font-size: 9px;
  color: var(--ui-warning);
  white-space: nowrap;
}
.st-key-input {
  flex: 0 0 132px;
  width: 132px;
  background: var(--ui-surface);
  border: 1px solid var(--ui-border);
  border-radius: 8px;
  color: var(--ui-text);
  font-family: inherit;
  font-size: 11px;
  padding: 6px 8px;
  text-align: center;
  cursor: pointer;
  outline: none;
}
.st-key-input:focus {
  border-color: color-mix(in srgb, var(--ui-accent) 65%, transparent);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--ui-accent) 14%, transparent);
}
.st-info { margin: 0; display: flex; flex-direction: column; gap: 6px; }
.st-info-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 10px;
  font-size: 11px;
}
.st-info-row dt { color: var(--ui-text-3); font-size: 11px; font-weight: 500; }
.st-info-row dd { margin: 0; font-weight: 700; }
.st-hidden { position: absolute; width: 1px; height: 1px; opacity: 0; pointer-events: none; }

/* ── phone settings stack (root section index → pushed detail) ──────────
   Gated on the `.st--mobile` root class (same ≤768px JS query as the shell),
   so a narrow DESKTOP window keeps the jump rail + one scroll page above.
   The ≤560px block below layers the iOS voice (SF stack, 44px rhythm) on top. */
.st--mobile .st-top--mobile { padding: 12px 14px 10px; gap: 10px; }

/* Large title on the root; the pushed detail is a compact 44px nav bar. */
.st--mobile .st-top--mobile:not(.is-detail) .st-title {
  font-size: 26px;
  line-height: 1.15;
  font-weight: 800;
  letter-spacing: -0.025em;
}
.st--mobile .st-top--mobile:not(.is-detail) .st-subtitle { font-size: 12px; }

.st--mobile .st-top.is-detail {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  justify-content: normal;
  gap: 8px;
  min-height: 44px;
  padding: 4px 8px;
}
.st-back {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  min-height: 36px;
  padding: 0 8px 0 4px;
  border: 0;
  border-radius: 10px;
  background: transparent;
  color: var(--ui-accent);
  font-family: inherit;
  font-size: 15px;
  cursor: pointer;
  -webkit-tap-highlight-color: transparent;
}
.st-back:active {
  background: color-mix(in srgb, var(--ui-accent) 12%, transparent);
}
.st-detail-title {
  margin: 0;
  min-width: 0;
  text-align: center;
  font-size: 15px;
  font-weight: 700;
  color: var(--ui-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Grouped background on both levels of the stack (tinted scroller, groups
   stay surface) — matches the ≤560px iOS block at tablet phone widths too. */
.st--mobile .st-body {
  padding: 12px 12px 28px;
  background: color-mix(in srgb, var(--ui-text) 5%, var(--ui-surface));
}
.st--mobile .st-body > * + * { margin-top: 14px; }
.st--mobile .st-body > :deep(.ui-card) {
  background: var(--ui-surface);
  border-radius: 14px;
}

/* iOS-style rounded search field; 16px input keeps iOS from zooming. */
.st-search {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 40px;
  padding: 0 10px;
  background: var(--ui-surface-2);
  border: 1px solid var(--ui-border);
  border-radius: 12px;
}
.st-search:focus-within {
  border-color: var(--ui-accent);
  box-shadow: var(--ui-focus-ring);
}
.st-search-icon { color: var(--ui-text-3); flex-shrink: 0; }
.st-search input {
  flex: 1;
  min-width: 0;
  border: 0;
  background: transparent;
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 16px;
  outline: none;
}
.st-search input::placeholder { color: var(--ui-text-3); }

/* Section index: one inset-rounded group of ≥52px cells (icon tile ·
   title/sub · live value · chevron) — the standard app settings row. */
.st-list {
  list-style: none;
  margin: 0;
  padding: 0;
  background: var(--ui-surface);
  border: 1px solid var(--ui-border);
  border-radius: 14px;
  overflow: hidden;
  animation: ui-rise var(--ui-dur-slow) var(--ui-ease-out) both;
}
.st-list-row {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  min-height: 52px;
  padding: 8px 12px;
  border: 0;
  border-top: 1px solid var(--ui-hairline);
  background: transparent;
  color: var(--ui-text);
  font-family: inherit;
  font-size: 15px;
  text-align: left;
  cursor: pointer;
  -webkit-tap-highlight-color: transparent;
}
.st-list li:first-child .st-list-row { border-top: 0; }
.st-list-row:active { background: var(--ui-hover); }
.st-list-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 30px;
  height: 30px;
  border-radius: 8px;
  background: linear-gradient(145deg, color-mix(in srgb, var(--ui-accent) 78%, #fff), var(--ui-accent));
  color: #fff;
}
.st-list-text { display: flex; flex-direction: column; gap: 1px; flex: 1; min-width: 0; }
.st-list-title {
  font-size: 15px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.st-list-sub {
  font-size: 12px;
  color: var(--ui-text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.st-list-value {
  flex-shrink: 1;
  max-width: 42%;
  font-size: 13px;
  color: var(--ui-text-3);
  text-align: right;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.st-list-chevron { flex-shrink: 0; color: var(--ui-text-3); }

/* Root-only helpers: the connection summary becomes the last group and the
   version footer replaces the desktop strip that lives outside <main>.
   Scoped through `.st--mobile` so the ≤560px block's `.st-strip` margins
   (which come later in this file) cannot re-inset it inside the body. */
.st--mobile .st-strip--mobile { margin: 0; }
.st--mobile .st-strip-url { margin-left: 0; flex-basis: 100%; }
.st-foot { margin: 0; text-align: center; font-size: 11px; color: var(--ui-text-3); }

/* Push transition: detail slides in from the right (native stack feel). */
@keyframes st-push-in {
  from { opacity: 0; transform: translateX(18px); }
  to { opacity: 1; transform: none; }
}
.st--mobile .st-body.is-detail > :deep(.ui-card) {
  animation: st-push-in 200ms var(--ui-ease-out) both;
}

@media (max-width: 560px) {
  /* iOS Settings voice: SF stack, large title, grouped-list rhythm. */
  .st { font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", var(--ui-font), sans-serif; }
  .st-top { padding: 10px 12px 8px; }
  .st-brand-mark { width: 28px; height: 28px; }
  /* Large-title homage (sheet scale: 26px under the 52px top bar). */
  .st-title { font-size: 26px; line-height: 1.15; font-weight: 800; letter-spacing: -0.025em; }
  .st-subtitle { font-size: 12px; }

  /* Grouped background (iOS light-grey / pure-black split, theme-aware):
     the scroll area tints toward the text colour while groups stay surface. */
  .st-body {
    padding: 12px 12px 24px;
    background: color-mix(in srgb, var(--ui-text) 5%, var(--ui-surface));
  }
  .st-body > * + * { margin-top: 14px; }
  .st-strip { margin: 10px 12px 0; border-radius: 14px; }
  .st-strip-url { margin-left: 0; flex-basis: 100%; }

  /* iOS groups: solid inset-rounded cards. (Phones disable backdrop-blur
     globally, so glass fills would go muddy — solid surface instead.) */
  .st-body > :deep(.ui-card) {
    background: var(--ui-surface);
    border-radius: 14px;
  }
  /* Section icon tiles (iOS 29px squircle homage) + stronger titles. */
  .st-body > :deep(.ui-card__header) { padding: 10px 14px; }
  .st-body > :deep(.ui-card__heading) { gap: 10px; }
  .st-body > :deep(.ui-card__icon) {
    width: 28px;
    height: 28px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    background: linear-gradient(145deg, color-mix(in srgb, var(--ui-accent) 78%, #fff), var(--ui-accent));
    color: #fff;
  }
  .st-body > :deep(.ui-card__title) { font-size: 15px; font-weight: 700; }
  .st-body > :deep(.ui-card__body) { padding: 4px 14px 12px; }

  /* Jump chips become a single snap rail (no 3-row wrap). */
  .st-jumps {
    padding: 8px 12px 0;
    flex-wrap: nowrap;
    overflow-x: auto;
    scrollbar-width: none;
    scroll-snap-type: x proximity;
    -webkit-overflow-scrolling: touch;
  }
  .st-jumps::-webkit-scrollbar { display: none; }
  .st-jump-search {
    flex: 0 0 150px;
    min-height: 40px;
    border-radius: 10px;
    font-size: 16px;
  }
  .st-jump {
    min-height: 40px;
    padding: 8px 13px;
    border-radius: 11px;
    font-size: 13px;
    scroll-snap-align: start;
  }

  /* Rows stay label-left / control-right at 44px (iOS cells), wrapping
     wide controls (segmented) onto their own line instead of stacking
     every row. Separators go solid hairline, like iOS. */
  .st-row {
    flex-direction: row;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 10px;
    min-height: 44px;
    padding: 8px 0;
  }
  .st-row > :first-child { flex: 1 1 130px; min-width: 0; }
  .st-row + .st-row { border-top: 1px solid var(--ui-hairline); }
  .st-field-row > :not(:first-child) { flex: 1 1 auto; }
  .st-actions .ui-btn { min-height: 40px; }

  /* Tables lose their nested box inside the group; rows hit 44px. */
  .st-table { border: 0; border-radius: 0; max-height: 320px; }
  .st-table-row { min-height: 44px; padding: 8px 0; }
  .st-key-input { flex: 1 1 150px; width: auto; min-height: 40px; font-size: 13px; }
  .st-info-row { align-items: flex-start; flex-wrap: wrap; }
}

@media (prefers-reduced-motion: reduce) {
  .st-jump { transition: none; }
  #oauth-broker-card.is-target { animation: none; }
  .st-list,
  .st-body.is-detail > :deep(.ui-card) { animation: none; }
}
</style>
