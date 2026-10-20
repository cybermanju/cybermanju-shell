<!-- CyberManju OS — AgentPanel (src/components/AgentPanel.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: signals: engine. -->
<template>
  <div class="agent" :class="{ 'agent--sidebar-closed': !sidebarOpen }">
    <!-- ── Header: identity + transport + context + YOLO + setup ── -->
    <header class="agent-header">
      <button
        class="agent-nav-toggle"
        type="button"
        :title="sidebarOpen ? 'Hide sidebar' : 'Show sidebar'"
        :aria-label="sidebarOpen ? 'Hide sidebar' : 'Show sidebar'"
        :aria-expanded="sidebarOpen"
        aria-controls="agent-session-nav"
        @click="sidebarOpen = !sidebarOpen"
      >
        <AppIcon name="solar:hamburger-menu-bold" :size="18" />
      </button>
      <span class="agent-avatar" aria-hidden="true">
        <AppIcon name="solar:bot-bold" :size="16" />
        <span v-if="jobActive" class="agent-avatar-pulse" />
      </span>
      <div class="agent-title">
        <h2>Assistant</h2>
        <p class="agent-subtitle">
          {{ friendlyTransport }}
          <span v-if="chatConfig" class="agent-subtitle-sep">·</span>
          <span v-if="chatConfig">{{ chatConfig.name }} · {{ caps.model }}</span>
        </p>
      </div>
      <div class="agent-header-meta">
        <UiButton
          size="sm"
          :icon="yoloOn ? 'solar:fire-bold' : 'solar:shield-check-bold'"
          :variant="yoloOn ? 'danger' : 'secondary'"
          :active="yoloOn"
          :loading="yoloBusy"
          :disabled="!chatConfig || yoloBusy"
          :title="yoloTitle"
          aria-label="Toggle YOLO mode"
          @click="toggleYolo"
        >{{ yoloOn ? 'YOLO on' : 'YOLO' }}</UiButton>
        <div class="agent-tabs" role="tablist" aria-label="Agent views">
          <button
            role="tab"
            type="button"
            :aria-selected="activeTab === 'chat'"
            :class="{ on: activeTab === 'chat' }"
            @click="activeTab = 'chat'"
          >
            <AppIcon name="solar:chat-round-dots-bold" :size="13" /> Chat
          </button>
          <button
            role="tab"
            type="button"
            :aria-selected="activeTab === 'setup'"
            :class="{ on: activeTab === 'setup' }"
            @click="openSetup"
          >
            <AppIcon name="solar:settings-bold" :size="13" /> Setup
          </button>
          <button
            role="tab"
            type="button"
            :aria-selected="activeTab === 'controls'"
            :class="{ on: activeTab === 'controls' }"
            @click="activeTab = 'controls'"
          >
            <AppIcon name="solar:shield-check-bold" :size="13" /> Controls
          </button>
        </div>
      </div>
    </header>

    <div class="agent-body">
      <button
        v-if="sidebarOpen"
        class="agent-sidebar-backdrop"
        type="button"
        aria-label="Close navigation"
        @click="sidebarOpen = false"
      />
      <!-- ── Sidebar: conversations + assistants ── -->
      <aside id="agent-session-nav" v-show="sidebarOpen" class="agent-sidebar" aria-label="Conversations and assistants">
        <nav class="agent-mobile-nav" aria-label="Agent navigation">
          <button type="button" :class="{ on: activeTab === 'chat' }" @click="activeTab = 'chat'; sidebarOpen = false"><AppIcon name="solar:chat-round-dots-bold" :size="16" /> Chat</button>
          <button type="button" :class="{ on: activeTab === 'setup' }" @click="activeTab = 'setup'; sidebarOpen = false"><AppIcon name="solar:settings-bold" :size="16" /> Assistant setup</button>
          <button type="button" :class="{ on: activeTab === 'controls' }" @click="activeTab = 'controls'; sidebarOpen = false"><AppIcon name="solar:shield-check-bold" :size="16" /> Controls</button>
        </nav>
        <div class="agent-side-section">
          <div class="agent-side-head">
            <h3><AppIcon name="solar:chat-square-bold" :size="13" /> Conversations</h3>
            <div class="agent-side-actions">
              <UiButton size="xs" icon="solar:add-bold" :disabled="!chatConfigId" title="Start a new conversation" @click="newSession(); sidebarOpen = false">New</UiButton>
              <UiButton size="xs" icon="solar:download-bold" icon-only title="Import a session file" aria-label="Import session" @click="importClick" />
              <input ref="importEl" type="file" accept="application/json" hidden @change="importFile" />
            </div>
          </div>
          <UiInput v-model="sessionSearch" placeholder="Search conversations…" aria-label="Search conversations" />
          <div class="agent-side-list">
            <button
              v-for="s in filteredSessions"
              :key="s.id"
              type="button"
              class="agent-side-item"
              :class="{ on: viewing?.id === s.id }"
              :title="`${s.title} · ${s.messages.length} msgs`"
              @click="loadSession(s.id); sidebarOpen = false"
            >
              <span class="agent-side-item-text">
                <span class="agent-side-item-title">{{ s.title }}</span>
                <span class="agent-side-item-meta">{{ s.messages.length }} msgs</span>
              </span>
              <span class="agent-side-item-btns" @click.stop>
                <UiButton size="xs" icon="solar:download-bold" icon-only title="Export conversation" aria-label="Export conversation" @click="exportSession(s.id)" />
                <UiButton size="xs" variant="danger" icon="solar:close-bold" icon-only title="Close conversation" aria-label="Close conversation" @click="removeSession(s.id)" />
              </span>
            </button>
            <UiEmpty
              v-if="!filteredSessions.length"
              size="sm"
              icon="solar:chat-round-dots-bold"
              title="No conversations yet"
              description="Pick an assistant below, then say hello."
            />
          </div>
          <div class="agent-side-row">
            <UiSelect
              aria-label="Assistant configuration"
              :model-value="chatConfigId"
              :options="sessionConfigOptions"
              @update:model-value="chatConfigId = $event"
            />
          </div>
          <div class="agent-side-row">
            <UiButton size="sm" :disabled="!chatConfigId || jobActive" title="Analyze the repo and write AGENTS.md with a detached run" @click="initRepo">Analyze repo</UiButton>
          </div>
        </div>

        <div class="agent-side-section">
          <div class="agent-side-head">
            <h3><AppIcon name="solar:cpu-bold" :size="13" /> Assistants ({{ configs.length }})</h3>
            <UiButton size="xs" icon="solar:add-bold" @click="openSetup">New</UiButton>
          </div>
          <div class="agent-side-list">
            <button
              v-for="cfg in configs"
              :key="cfg.id"
              type="button"
              class="agent-side-item"
              :class="{ on: chatConfigId === cfg.id }"
              :title="`${cfg.name} · ${cfg.providerId} · ${cfg.model}`"
              @click="chatConfigId = cfg.id; sidebarOpen = false"
            >
              <span class="agent-side-item-text">
                <span class="agent-side-item-title">{{ cfg.name }}</span>
                <span class="agent-side-item-meta">{{ cfg.providerId }} · {{ cfg.model }}</span>
              </span>
              <UiBadge :tone="cfg.hasKey || isKeyless(cfg) ? 'accent' : 'neutral'" size="sm" class="agent-side-item-badge">
                {{ cfg.hasKey || isKeyless(cfg) ? 'Ready' : 'No key' }}
              </UiBadge>
              <span class="agent-side-item-btns" @click.stop>
                <UiButton size="xs" variant="danger" @click="removeCfg(cfg.id)">Delete</UiButton>
              </span>
            </button>
            <UiEmpty
              v-if="!configs.length"
              size="sm"
              icon="solar:bot-bold"
              title="No assistants yet"
              description="Open Setup, pick a provider, save — it takes a minute."
            />
          </div>
        </div>
      </aside>

      <!-- ── Main column ── -->
      <main class="agent-main">
        <!-- Context line: one quiet summary row, details expand on demand -->
        <section v-if="capVisible" class="agent-caps" :class="{ open: capsOpen }" aria-label="What this assistant can do">
          <button type="button" class="agent-caps-summary" :aria-expanded="capsOpen" @click="capsOpen = !capsOpen">
            <span class="agent-caps-dot" :class="{ warn: !caps.hasKey }" aria-hidden="true" />
            <span class="agent-caps-text">{{ caps.model }} · {{ caps.persona }} · {{ caps.permission }}</span>
            <span v-if="!caps.hasKey" class="agent-caps-key">No key</span>
            <AppIcon :name="capsOpen ? 'solar:alt-arrow-up-bold' : 'solar:alt-arrow-down-bold'" :size="12" />
          </button>
          <div v-if="capsOpen" class="agent-caps-detail">
            <span class="agent-cap" :title="caps.personaHint">
              <AppIcon name="solar:cpu-bold" :size="12" /> {{ caps.model }} · {{ caps.persona }}
            </span>
            <span class="agent-cap" :title="'Working directory inside your volume'">
              <AppIcon name="solar:folder-bold" :size="12" /> {{ caps.workingDir }}
            </span>
            <span v-if="!wasmMode" class="agent-cap" :title="'Which shell the bash tool runs'">
              <AppIcon name="solar:file-terminal-bold" :size="12" /> {{ caps.shell }}
            </span>
            <span class="agent-cap" :title="'Permission ruleset: ' + caps.permission">
              <AppIcon name="solar:shield-check-bold" :size="12" /> {{ caps.permission }}
            </span>
            <span class="agent-cap" :class="{ warn: !caps.hasKey }" :title="caps.keyHint">
              <AppIcon :name="caps.hasKey ? 'solar:key-bold' : 'solar:lock-bold'" :size="12" />
              {{ caps.hasKey ? 'Key ready' : 'No key' }}
            </span>
            <button type="button" class="agent-cap-link" @click="activeTab = 'controls'">
              Open Controls
            </button>
            <div class="agent-cap-tools" role="list" aria-label="Per-tool defaults">
              <span v-for="t in toolPerms" :key="t.tool" class="agent-cap-tool" :class="`act-${t.action}`" role="listitem" :title="toolHelp(t)">
                <AppIcon :name="toolMeta(t.tool).icon" :size="11" />
                {{ t.tool }} · {{ t.unsupported ? 'n/a' : t.action }}
              </span>
            </div>
            <p v-if="wasmMode" class="agent-note">Browser volume — cybsh-subset shell, depth-1 subagents, HTTP MCP. Device shell + stdio MCP need the dashboard and answer <span class="mono">unsupported:</span> there.</p>
          </div>
        </section>

        <!-- Setup tab: guided, 3 steps instead of one long form -->
        <section v-if="activeTab === 'setup'" class="agent-card" aria-label="Assistant setup">
          <h3 class="agent-card-title"><AppIcon name="solar:add-bold" :size="13" /> 1 · Pick a provider ({{ filteredPresets.length }}/{{ allPresets.length }})</h3>
          <div class="preset-search">
            <UiInput v-model="providerSearch" placeholder="Search providers or models…" aria-label="Search providers" clearable />
          </div>
          <div class="preset-grid">
            <button
              v-for="p in filteredPresets"
              :key="p.id"
              class="preset-card"
              :class="{ on: form.providerId === p.id }"
              :title="`${p.baseUrl} · ${p.defaultModel}`"
              @click="pickPreset(p)"
            >
              <span class="preset-name">{{ p.label }}</span>
              <span class="preset-meta">{{ p.family }} · {{ p.defaultModel }}</span>
              <span v-if="p.keyless" class="preset-free">No key needed</span>
            </button>
            <p v-if="!filteredPresets.length" class="agent-note">No provider matches “{{ providerSearch }}”.</p>
          </div>

          <h3 class="agent-card-title"><AppIcon name="solar:key-bold" :size="13" /> 2 · Model &amp; key</h3>
          <div class="w-field">
            <span class="w-label">Assistant name</span>
            <UiInput v-model="form.name" placeholder="My assistant" aria-label="Assistant name" />
          </div>
          <div class="w-row">
            <div class="w-field">
              <span class="w-label">Model</span>
              <div class="model-row">
                <UiInput v-model="form.model" placeholder="Model id" list="agent-models" aria-label="Model id" />
                <UiButton
                  size="sm"
                  icon="solar:refresh-bold"
                  icon-only
                  :disabled="modelBusy || !form.providerId"
                  :loading="modelBusy"
                  title="Refresh the model list from the provider"
                  aria-label="Refresh model list"
                  @click="refreshModels"
                />
              </div>
              <datalist id="agent-models">
                <option v-for="m in models" :key="m" :value="m" />
              </datalist>
            </div>
            <div class="w-field">
              <span class="w-label">Style</span>
              <UiSelect
                aria-label="Agent mode"
                :model-value="form.agentKind"
                :options="[
                  { label: 'Build — can edit & run', value: 'build' },
                  { label: 'Plan — read-only', value: 'plan' },
                ]"
                @update:model-value="form.agentKind = $event as 'build' | 'plan'"
              />
            </div>
          </div>
          <div class="w-field">
            <span class="w-label">Endpoint override (optional)</span>
            <UiInput v-model="form.baseUrlOverride" :placeholder="presetBase" aria-label="Endpoint override" />
          </div>
          <div v-if="isCustom" class="w-row">
            <div class="w-field">
              <span class="w-label">Dialect</span>
              <UiSelect
                aria-label="API dialect"
                :model-value="form.dialectOverride"
                :options="[
                  { label: 'OpenAI-compatible', value: 'openAi' },
                  { label: 'Anthropic', value: 'anthropic' },
                ]"
                @update:model-value="form.dialectOverride = $event as 'openAi' | 'anthropic'"
              />
            </div>
            <div class="w-field">
              <span class="w-label">Auth</span>
              <UiSelect
                aria-label="Authentication scheme"
                :model-value="form.authSchemeOverride"
                :options="[
                  { label: 'Bearer', value: 'bearer' },
                  { label: 'Header', value: 'header' },
                  { label: 'Query (?key=)', value: 'query' },
                  { label: 'None', value: 'none' },
                ]"
                @update:model-value="form.authSchemeOverride = $event as 'bearer' | 'header' | 'query' | 'none'"
              />
            </div>
            <div v-if="form.authSchemeOverride === 'header' || form.authSchemeOverride === 'query'" class="w-field">
              <span class="w-label">Auth name</span>
              <UiInput v-model="form.authNameOverride" placeholder="x-api-key" aria-label="Auth header name" />
            </div>
          </div>

          <h3 class="agent-card-title"><AppIcon name="solar:shield-check-bold" :size="13" /> 3 · Permissions &amp; workspace</h3>
          <div class="w-row">
            <div class="w-field">
              <span class="w-label">Working dir (in your volume, empty = root)</span>
              <UiInput v-model="form.workingDir" placeholder="/" aria-label="Working directory" />
            </div>
            <div class="w-field w-field-narrow">
              <span class="w-label">Max turns</span>
              <UiInput
                :model-value="String(form.maxTurns)"
                type="number"
                :min="1"
                :max="50"
                aria-label="Max turns"
                @update:model-value="form.maxTurns = Number($event)"
              />
            </div>
            <div v-if="!wasmMode" class="w-field">
              <span class="w-label">Shell</span>
              <UiSelect
                aria-label="Shell mode"
                :model-value="form.shellMode"
                :options="[
                  { label: 'Auto (recommended)', value: 'auto' },
                  { label: 'Volume shell only', value: 'cybsh' },
                  { label: 'Device shell only', value: 'device' },
                ]"
                @update:model-value="form.shellMode = $event as 'auto' | 'cybsh' | 'device'"
              />
            </div>
          </div>
          <div class="w-row">
            <div class="w-field">
              <span class="w-label">Permissions</span>
              <UiSelect
                aria-label="Permission preset"
                :model-value="permPreset"
                :options="[
                  { label: 'Strict — ask for everything', value: 'strict' },
                  { label: 'Balanced — reads auto, changes ask', value: 'balanced' },
                  { label: 'YOLO — allow everything', value: 'yolo' },
                ]"
                @update:model-value="permPreset = $event as 'strict' | 'balanced' | 'yolo'"
              />
            </div>
            <div class="w-field check-field">
              <UiCheckbox v-model="form.autoApprove" label="Auto-approve requests" />
            </div>
          </div>
          <div class="w-field">
            <span class="w-label">{{ wasmMode ? 'API key (kept in memory, cleared on reload)' : 'API key (sealed server-side, never shown back)' }}</span>
            <div class="model-row">
              <UiInput v-model="keyInput" type="password" placeholder="Paste key" aria-label="API key" autocomplete="off" />
              <UiButton size="sm" :disabled="!effectiveKeyConfigId || !keyInput.trim() || keyBusy" :loading="keyBusy" :title="!effectiveKeyConfigId ? 'Save the assistant first, or pick one in the sidebar' : ''" @click="saveKey">Save key</UiButton>
            </div>
          </div>
          <div class="w-actions">
            <UiButton size="sm" variant="primary" :disabled="busy || !canSave" :loading="busy" @click="saveConfig">Save assistant</UiButton>
            <UiButton size="sm" variant="ghost" @click="activeTab = 'chat'">Back to chat</UiButton>
          </div>
          <div v-if="setupMsg" class="w-msg">{{ setupMsg }}</div>
          <p class="agent-note">Custom provider: pick the Custom preset, set endpoint + dialect + auth. Model-list refresh needs a saved key (Anthropic has no list API — enter the model by hand).</p>
          <p class="agent-note">Second account or spare key? Save another assistant on the same provider, then chain it under Controls → Fallback route — throttled/empty/dead keys fail over automatically, in order.</p>
        </section>

        <!-- Controls tab: MCP servers + per-tool permissions -->
        <section v-if="activeTab === 'controls'" class="agent-controls" aria-label="Agent controls">
          <div v-if="chatConfig" class="agent-card" :class="{ open: controlsOpen === 'tools' }">
            <button type="button" class="agent-card-toggle" :aria-expanded="controlsOpen === 'tools'" @click="controlsOpen = controlsOpen === 'tools' ? 'perms' : 'tools'">
              <AppIcon name="solar:plug-circle-bold" :size="13" /> Connected tools · {{ mcpEntries.length }}
              <AppIcon :name="controlsOpen === 'tools' ? 'solar:alt-arrow-up-bold' : 'solar:alt-arrow-down-bold'" :size="12" />
            </button>
            <div v-show="controlsOpen === 'tools'" class="agent-card-fold">
            <div class="config-list">
              <div v-for="m in mcpEntries" :key="m.name" class="config-card">
                <div class="cfg-header">
                  <span class="cfg-name">{{ m.name }}</span>
                  <span class="cfg-type">{{ m.cfg.transport }}{{ m.cfg.enabled ? '' : ' · off' }}</span>
                </div>
                <div class="cfg-meta">
                  <span v-if="m.cfg.transport === 'stdio'">{{ m.cfg.command }} {{ (m.cfg.args || []).join(' ') }}</span>
                  <span v-else>{{ m.cfg.url }}</span>
                </div>
                <div class="cfg-actions">
                  <UiButton size="xs" variant="danger" @click="removeMcp(m.name)">Disconnect</UiButton>
                </div>
              </div>
            </div>
            <div class="w-row">
              <div class="w-field grow">
                <UiInput v-model="mcpForm.name" placeholder="server-name" aria-label="MCP server name" spellcheck="false" />
              </div>
              <div class="w-field grow">
                <UiSelect
                  aria-label="MCP transport"
                  :model-value="mcpForm.transport"
                  :options="[
                    { label: 'Local command', value: 'stdio' },
                    { label: 'HTTP endpoint', value: 'http' },
                  ]"
                  @update:model-value="mcpForm.transport = $event"
                />
              </div>
            </div>
            <div class="w-row">
              <div v-if="mcpForm.transport === 'stdio'" class="w-field grow">
                <UiInput v-model="mcpForm.command" placeholder="command on PATH (e.g. npx)" aria-label="MCP command" spellcheck="false" />
              </div>
              <div v-if="mcpForm.transport === 'stdio'" class="w-field grow">
                <UiInput v-model="mcpForm.args" placeholder="args, space-separated" aria-label="MCP args" spellcheck="false" />
              </div>
              <div v-if="mcpForm.transport === 'http'" class="w-field grow">
                <UiInput v-model="mcpForm.url" placeholder="https://…/mcp" aria-label="MCP URL" spellcheck="false" />
              </div>
              <UiButton size="sm" :disabled="mcpBusy || !mcpForm.name.trim()" :loading="mcpBusy" @click="addMcp">Connect</UiButton>
              <UiButton size="sm" :disabled="mcpBusy || !chatConfigId" @click="refreshMcpTools">List tools</UiButton>
            </div>
            <div v-if="mcpTools.length" class="remote-list">
              <div v-for="t in mcpTools" :key="t.name" class="remote-row">
                <span>{{ t.name }}</span><span class="dim">{{ (t.description || '').slice(0, 80) }}</span>
              </div>
            </div>
            <div v-if="mcpMsg" class="w-msg">{{ mcpMsg }}</div>
            <p class="agent-note">Connecting tools needs admin (local commands spawn processes). Tools appear as <span class="mono">mcp__server__tool</span> and follow the same ask / deny rules.</p>
            </div>
          </div>

          <div v-if="chatConfig" class="agent-card" :class="{ open: controlsOpen === 'fallback' }">
            <button type="button" class="agent-card-toggle" :aria-expanded="controlsOpen === 'fallback'" @click="controlsOpen = controlsOpen === 'fallback' ? 'perms' : 'fallback'">
              <AppIcon name="solar:refresh-bold" :size="13" /> Fallback route · {{ fallbackList.length }}
              <AppIcon :name="controlsOpen === 'fallback' ? 'solar:alt-arrow-up-bold' : 'solar:alt-arrow-down-bold'" :size="12" />
            </button>
            <div v-show="controlsOpen === 'fallback'" class="agent-card-fold">
            <div v-if="fallbackList.length" class="config-list">
              <div v-for="(fid, i) in fallbackList" :key="fid" class="config-card">
                <div class="cfg-header">
                  <span class="cfg-name">{{ i + 1 }} · {{ fallbackName(fid) }}</span>
                  <UiBadge :tone="fallbackReady(fid) ? 'accent' : 'neutral'" size="sm">{{ fallbackReady(fid) ? 'Ready' : 'No key' }}</UiBadge>
                </div>
                <div class="cfg-actions">
                  <UiButton size="xs" :disabled="i === 0" @click="moveFallback(i, -1)">Up</UiButton>
                  <UiButton size="xs" :disabled="i === fallbackList.length - 1" @click="moveFallback(i, 1)">Down</UiButton>
                  <UiButton size="xs" variant="danger" @click="removeFallback(i)">Remove</UiButton>
                </div>
              </div>
            </div>
            <div class="w-row">
              <div class="w-field grow">
                <UiSelect
                  aria-label="Fallback assistant"
                  :model-value="fallbackPick"
                  :options="fallbackOptions"
                  @update:model-value="fallbackPick = $event"
                />
              </div>
              <UiButton size="sm" :disabled="fallbackBusy || !fallbackPick" @click="addFallback">Add</UiButton>
            </div>
            <div class="w-actions">
              <UiButton size="sm" variant="primary" :disabled="fallbackBusy" :loading="fallbackBusy" @click="saveFallbacks">Save route</UiButton>
              <UiButton size="sm" variant="ghost" @click="activeTab = 'chat'">Back to chat</UiButton>
            </div>
            <div v-if="fallbackMsg" class="w-msg">{{ fallbackMsg }}</div>
            <p class="agent-note">When {{ chatConfig.name }}'s provider call fails, the run continues on each fallback in order — same transcript, no retype. Max 4 (5 routes total).</p>
            </div>
          </div>

          <div v-if="chatConfig" class="agent-card" :class="{ open: controlsOpen === 'perms' }">
            <button type="button" class="agent-card-toggle" :aria-expanded="controlsOpen === 'perms'" @click="controlsOpen = controlsOpen === 'perms' ? 'fallback' : 'perms'">
              <AppIcon name="solar:shield-check-bold" :size="13" /> Permissions
              <AppIcon :name="controlsOpen === 'perms' ? 'solar:alt-arrow-up-bold' : 'solar:alt-arrow-down-bold'" :size="12" />
            </button>
            <div v-show="controlsOpen === 'perms'" class="agent-card-fold">
            <div class="perm-grid">
              <div v-for="t in permEditorTools" :key="t.tool" class="perm-row">
                <AppIcon :name="toolMeta(t.tool).icon" :size="12" />
                <span class="ct-name">{{ t.tool }}</span>
                <UiSelect
                  :aria-label="'Permission for ' + t.tool"
                  :model-value="t.action"
                  :options="[
                    { label: 'Allow', value: 'allow' },
                    { label: 'Ask me', value: 'ask' },
                    { label: 'Never', value: 'deny' },
                  ]"
                  @update:model-value="setPermTool(t.tool, $event as 'allow' | 'ask' | 'deny')"
                />
              </div>
            </div>
            <div class="w-actions">
              <UiButton size="sm" variant="primary" :disabled="permBusy" :loading="permBusy" @click="savePermRules">Save rules</UiButton>
              <UiButton size="sm" :disabled="permBusy" @click="resetPermRules">Reset to balanced</UiButton>
              <UiButton size="sm" variant="ghost" @click="activeTab = 'chat'">Back to chat</UiButton>
            </div>
            <div v-if="permMsg" class="w-msg">{{ permMsg }}</div>
            <p class="agent-note">Denied tools are also hidden from the assistant, so it stops trying them instead of failing.</p>
            </div>
          </div>
          <UiEmpty
            v-if="!chatConfig"
            size="sm"
            icon="solar:shield-check-bold"
            title="No assistant selected"
            description="Pick or create an assistant in the sidebar first."
          />
        </section>

        <!-- Chat tab -->
        <section v-show="activeTab === 'chat'" class="agent-chat" aria-label="Conversation">
          <!-- Onboarding when there is nothing to show yet -->
          <div v-if="!viewing" class="agent-welcome">
            <span class="agent-welcome-avatar"><AppIcon name="solar:bot-bold" :size="22" /></span>
            <h3>{{ configs.length ? 'What should we work on?' : 'Meet your assistant' }}</h3>
            <p v-if="!configs.length">
              Create an assistant in <button type="button" class="agent-link" @click="openSetup">Setup</button>
              (pick a provider, save, add your key), then come back here.
            </p>
            <p v-else-if="!sessions.length">
              Start a new conversation from the sidebar — or try one of these:
            </p>
            <p v-else>Pick a conversation on the left, or start fresh:</p>
            <div v-if="configs.length" class="agent-quick">
              <button
                v-for="q in quickPrompts"
                :key="q.label"
                type="button"
                class="agent-quick-card"
                :disabled="!chatConfigId"
                @click="useQuick(q.prompt)"
              >
                <AppIcon :name="q.icon" :size="14" />
                <span class="agent-quick-label">{{ q.label }}</span>
                <span class="agent-quick-hint">{{ q.prompt.slice(0, 64) }}…</span>
              </button>
            </div>
            <div v-if="configs.length && !chatConfigId" class="w-msg">Select an assistant in the sidebar first.</div>
          </div>

          <template v-else>
            <div class="agent-thread-head" :title="`Context ${fmtTokens(contextTokens)} / ${fmtTokens(contextWindow)} · In ${fmtTokens(viewing.usage.inputTokens)} · Out ${fmtTokens(viewing.usage.outputTokens)}${costUsd != null ? ` · ~$${costLabel}` : ''}`">
              <div class="agent-thread-title">
                <strong>{{ viewing.title }}</strong>
                <span class="dim">{{ viewing.model }} · {{ viewing.agentKind === 'plan' ? 'Plan' : 'Build' }}</span>
              </div>
              <span class="meter" aria-hidden="true">
                <span class="ctx-bar"><span class="ctx-fill" :class="ctxTone" :style="{ width: contextPct + '%' }" /></span>
                <span class="meter-label">{{ contextPct }}%</span>
              </span>
              <UiButton size="xs" :disabled="!viewing.messages.length || jobActive" title="Summarize into a fresh conversation (keeps this one)" @click="compactThread">Summarize</UiButton>
            </div>

            <div ref="messagesEl" class="agent-messages">
              <template v-for="row in threadRows" :key="row.key">
                <div v-if="row.kind === 'message'" class="msg" :class="`role-${row.message.role}`">
                  <span
                    class="msg-avatar"
                    :class="`avatar-${row.message.role}`"
                    aria-hidden="true"
                  >
                    <AppIcon
                      :name="row.message.role === 'user' ? 'solar:user-circle-bold' : row.message.role === 'tool' ? 'solar:toolbox-bold' : 'solar:bot-bold'"
                      :size="13"
                    />
                  </span>
                  <div class="msg-main">
                    <div class="msg-role">
                      {{ roleLabel(row.message) }}
                      <button
                        v-if="row.message.content"
                        type="button"
                        class="msg-copy"
                        :title="copiedKey === row.key ? 'Copied' : 'Copy message'"
                        @click="copyRow(row.message.content ?? '', row.key)"
                      >
                        <AppIcon :name="copiedKey === row.key ? 'solar:check-bold' : 'solar:copy-bold'" :size="11" />
                      </button>
                    </div>
                    <div v-if="row.message.role === 'assistant' && row.message.content" class="msg-body md" v-html="renderMarkdown(row.message.content)"></div>
                    <div v-else-if="row.message.content" class="msg-body">{{ row.message.content }}</div>
                    <div v-else-if="row.message.toolName || row.message.toolInput" class="tool-block">
                      <span class="tool-name"><AppIcon name="solar:toolbox-bold" :size="12" /> {{ row.message.toolName ?? toolNameOf(row.message) }}</span>
                      <pre class="tool-input">{{ prettyInput(row.message) }}</pre>
                    </div>
                    <div v-if="row.key === lastAssistantKey" class="turn-footer">{{ footerLine }}</div>
                  </div>
                </div>

                <div v-else class="tool-group">
                  <div v-if="row.lead" class="msg role-assistant_tool lead">
                    <div class="msg-body md" v-html="renderMarkdown(row.lead)"></div>
                  </div>
                  <div
                    v-for="t in row.rows"
                    :key="t.key"
                    class="tool-row"
                    :class="[`is-${t.state}`, { open: isOpen(t.key) }]"
                  >
                    <button
                      class="tool-head"
                      type="button"
                      :aria-expanded="isOpen(t.key)"
                      :title="t.state === 'running' ? 'Running…' : 'Show input and result'"
                      @click="toggleRow(t.key)"
                    >
                      <UiSpinner v-if="t.state === 'running'" size="xs" />
                      <AppIcon v-else :name="toolMeta(t.name).icon" :size="12" />
                      <span class="tool-title">{{ t.title }}</span>
                      <UiBadge v-if="t.state === 'denied'" tone="danger" size="sm">Needs you</UiBadge>
                      <UiBadge v-else-if="t.state === 'error'" tone="warning" size="sm">Failed</UiBadge>
                      <span class="tool-chev" aria-hidden="true">›</span>
                    </button>
                    <div v-if="isOpen(t.key) || t.state === 'running'" class="tool-detail">
                      <pre class="tool-input">{{ prettyInput({ toolName: t.name, toolInput: t.input }) }}</pre>
                      <pre v-if="t.result" class="tool-result" :class="{ bad: t.state === 'error' || t.state === 'denied' }">{{ t.result }}</pre>
                      <div v-else class="dim tool-wait">Waiting for result…</div>
                    </div>
                  </div>
                </div>
              </template>
              <div v-if="!viewing.messages.length" class="agent-thread-empty">
                No messages yet — ask below. Try <button type="button" class="agent-link" @click="useQuick(quickPrompts[0].prompt)">“{{ quickPrompts[0].label }}”</button>.
              </div>
            </div>

            <div v-if="pendingApproval" class="approval attention" role="alertdialog" aria-label="Assistant needs your approval">
              <div class="approval-title">
                <AppIcon :name="pendingApproval.question ? 'solar:question-circle-bold' : 'solar:shield-check-bold'" :size="14" />
                {{ pendingApproval.question ? 'The assistant has a question' : 'Your approval needed' }}
              </div>
              <div class="approval-text">{{ pendingApproval.question || pendingApproval.summary }}</div>
              <div class="approval-meta">
                <span><span class="dim">Tool</span> <span class="mono">{{ pendingApproval.tool }}</span></span>
                <span v-if="approvalArg"><span class="dim">Target</span> <span class="mono">{{ approvalArg }}</span></span>
                <span v-if="!pendingApproval.question" class="rule-line">
                  <span class="dim">“Always allow” saves</span>
                  <span class="mono">rules["{{ pendingApproval.tool }}"] = "allow"</span>
                </span>
              </div>
              <div v-if="approvalDiff" class="approval-diff">
                <div class="diff-head dim">Proposed edit — {{ approvalDiff.oldLines }} → {{ approvalDiff.newLines }} lines<span v-if="approvalDiff.truncated"> (truncated)</span></div>
                <pre class="diff-body"><span v-for="(l, i) in approvalDiff.lines" :key="i" class="diff-line" :class="`diff-${l.kind}`">{{ (l.kind === 'del' ? '− ' : l.kind === 'add' ? '+ ' : '  ') + l.text }}
</span></pre>
              </div>
              <pre v-else-if="approvalInput" class="tool-input approval-input">{{ approvalInput }}</pre>
              <div v-if="pendingApproval.question" class="w-row">
                <div class="w-field grow">
                  <UiInput
                    v-model="answerInput"
                    placeholder="Type your answer…"
                    aria-label="Approval answer"
                    @enter="answerApproval(true)"
                  />
                </div>
              </div>
              <div v-else class="w-row">
                <div class="w-field grow">
                  <UiInput
                    v-model="denyReason"
                    placeholder="Decline with feedback (optional — the assistant must follow it)…"
                    aria-label="Decline feedback"
                  />
                </div>
              </div>
              <div class="w-actions">
                <UiButton size="sm" variant="primary" @click="answerApproval(true)">Allow once</UiButton>
                <UiButton
                  size="sm"
                  title="Saves an explicit rule: rules[tool] = allow — visible in Controls"
                  @click="answerApproval(true, true)"
                >Always allow</UiButton>
                <UiButton size="sm" variant="danger" @click="answerApproval(false)">Decline</UiButton>
              </div>
              <p v-if="!pendingApproval.question" class="agent-note">Declining returns <span class="mono">denied: …</span> — the assistant works around it instead of retrying. Add feedback above to steer the next attempt.</p>
            </div>

            <div v-if="queue.length" class="queue" aria-label="Queued prompts">
              <UiBadge tone="info" size="sm" icon="solar:clock-circle-bold">Queued {{ queue.length }}</UiBadge>
              <span v-for="(q, qi) in queue" :key="qi" class="queue-item">
                <span class="queue-text">{{ q }}</span>
                <UiButton
                  size="xs"
                  icon="solar:close-bold"
                  icon-only
                  title="Remove from queue"
                  aria-label="Remove queued prompt"
                  @click="queue.splice(qi, 1)"
                />
              </span>
            </div>
          </template>

            <!-- Composer stays visible even with no session yet: sending
              auto-creates one (same as desktop). Hidden only on setup/controls tabs. -->
            <div class="composer" :class="{ focused: composerFocused }">
              <div v-if="slashHints.length && slashOpen" class="composer-slash" role="listbox" aria-label="Slash commands">
                <button
                  v-for="s in slashHints"
                  :key="s.cmd"
                  type="button"
                  role="option"
                  class="composer-slash-item"
                  @click="applySlash(s.insert)"
                >
                  <span class="mono">{{ s.cmd }}</span><span class="dim">{{ s.hint }}</span>
                </button>
              </div>
              <textarea
                v-model="promptInput"
                class="composer-box"
                :placeholder="!chatConfigId ? 'Pick an assistant in the sidebar, then ask anything…' : jobActive ? 'Working… type on, Enter queues your follow-up…' : viewing ? 'Ask anything… (Enter to send, / for commands)' : 'Ask anything — sending starts a new conversation… (Enter to send, / for commands)'"
                rows="2"
                aria-label="Message the assistant"
                @focus="composerFocused = true"
                @blur="composerFocused = false"
                @keydown="onComposerKey"
                @keydown.enter.exact.prevent="sendPrompt"
              />
              <div class="composer-bar">
                <span class="composer-hint dim">
                  {{ charCount ? `${charCount} chars` : 'Enter ↵ send' }} · / commands
                </span>
                <span class="composer-spacer" />
                <div v-if="voice.isSupported.value" class="voice-group">
                  <div class="voice-pop" role="group" aria-label="Dictation language">
                    <button
                      type="button"
                      class="voice-opt"
                      :class="{ on: voice.langPref.value === 'auto' }"
                      :aria-pressed="voice.langPref.value === 'auto'"
                      title="Auto-detect from your browser language"
                      @click="voice.setLang('auto')"
                    >Auto · {{ voice.lang.value === 'pt-BR' ? 'PT' : 'EN' }}</button>
                    <button
                      type="button"
                      class="voice-opt"
                      :class="{ on: voice.langPref.value === 'en-US' }"
                      :aria-pressed="voice.langPref.value === 'en-US'"
                      title="Dictate in English"
                      @click="voice.setLang('en-US')"
                    >EN</button>
                    <button
                      type="button"
                      class="voice-opt"
                      :class="{ on: voice.langPref.value === 'pt-BR' }"
                      :aria-pressed="voice.langPref.value === 'pt-BR'"
                      title="Dictate in Portuguese (PT-BR)"
                      @click="voice.setLang('pt-BR')"
                    >PT</button>
                  </div>
                  <UiButton size="sm" :variant="voice.listening.value ? 'danger' : 'ghost'" :title="voice.listening.value ? `Listening… ${voice.interim.value}` : voice.langPref.value === 'auto' ? `Dictate your message (auto: ${voice.lang.value === 'pt-BR' ? 'Portuguese' : 'English'} — hover to pick EN/PT)` : `Dictate your message (${voice.langPref.value === 'pt-BR' ? 'Portuguese' : 'English'} — hover to change)`" @click="toggleVoice">{{ voice.listening.value ? 'Stop' : 'Dictate' }}</UiButton>
                </div>
                <UiButton v-if="jobActive" size="sm" variant="danger" icon="solar:stop-bold" @click="abortJob">Stop</UiButton>
                <UiButton size="sm" variant="primary" icon="solar:arrow-right-bold" :disabled="!canSend" :title="!chatConfigId ? 'Select an assistant first' : ''" @click="sendPrompt">{{ jobActive ? 'Queue' : 'Send' }}</UiButton>
              </div>
            </div>
            <div v-if="jobLine" class="w-msg job-line"><AppIcon name="solar:clock-circle-bold" :size="11" /> {{ jobLine }}</div>
            <div v-if="jobError" class="w-msg err" :title="jobHint">{{ jobError }}</div>
            <div v-if="jobHint && jobError" class="w-msg"><AppIcon name="solar:info-circle-bold" :size="11" /> {{ jobHint }}</div>
            <div v-if="activeTab === 'chat' && !viewing && !chatConfigId && configs.length" class="w-msg">Select an assistant in the sidebar first — or create one in Setup.</div>
        </section>
      </main>
    </div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import UiBadge from '@/components/ui/UiBadge.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiCheckbox from '@/components/ui/UiCheckbox.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiSpinner from '@/components/ui/UiSpinner.vue'
import { computed, reactive, ref, watch, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { useAppStore } from '@/stores/app'
import { isStaticHost } from '@/composables/useTauri'
import {
  useAgent,
  runLocalAgent,
  listLocalConfigs,
  saveLocalConfig,
  deleteLocalConfig,
  getLocalKey,
  setLocalKey,
  forgetLocalKey,
  hasLocalKey,
  listLocalSessions,
  saveLocalSession,
  deleteLocalSession,
  abortLocalRun,
  recallBlockLocal,
  readChatConfigId,
  writeChatConfigId,
  CHAT_CONFIG_EVENT,
  CHAT_CONFIG_ID_KEY,
  LOCAL_CONFIGS_KEY,
  AGENT_CONFIGS_EVENT,
  AGENT_KEYS_EVENT,
  localKeyRevision,
  type LocalFailoverRoute,
} from '@/composables/useAgent'
import { agentPermissionPreset, agentErrorHint, defaultMcpServers } from '@/types'
import {
  applyBalancedToConfig,
  applyYoloToConfig,
  buildThread,
  contextWindowFor,
  estimateCost,
  estimateTranscriptTokens,
  isYoloConfig,
  permissionLabel,
  salientArg,
  toolMeta,
  toolPermissions,
} from '@/utils/agentUi'
import {
  capabilitySummary,
  copyText as copyToClipboard,
  quickPrompts,
  slashCommands,
} from '@/composables/useAgentHarness'
import { renderMarkdown } from '@/utils/markdown'
import { deploymentStorageEventKey } from '@/utils/deploymentScope'
import { useVoiceInput } from '@/composables/useVoiceInput'
import { useWindowUi } from '@/composables/useWindowUi'
import { diffBlocks, editBlocksOf } from '@/utils/agentDiff'
import type { AgentConfig, AgentJob, AgentSession, ProviderPreset } from '@/types'

const store = useAppStore()
const agent = useAgent()

const wasmMode = computed(() => {
  try {
    return isStaticHost()
  } catch {
    return false
  }
})

// ── data sources (server store vs browser-local) ──
const localPresets = ref<ProviderPreset[]>([])
const localConfigs = ref<AgentConfig[]>([])
const localSessions = ref<AgentSession[]>([])
const localViewing = ref<AgentSession | null>(null)
const localJob = ref<AgentJob | null>(null)
/** Provider keys live in memory only — never localStorage. */
const localKeys = ref<Record<string, string>>({})

const providers = computed(() => (wasmMode.value ? localPresets.value : store.agentProviders))
const configs = computed(() => (wasmMode.value ? localConfigs.value : store.agentConfigs))
const sessions = computed(() => (wasmMode.value ? localSessions.value : store.agentSessions))
// NOTE: serverViewing must be declared before `viewing` — the auto-compaction
// watcher below evaluates its getter eagerly during setup, and a forward
// reference here (or to contextPct/jobActive) throws
// `ReferenceError: Cannot access ... before initialization`, leaving the
// whole Agent window empty.
const serverViewing = ref<AgentSession | null>(null)
const viewing = computed(() => (wasmMode.value ? localViewing.value : serverViewing.value))
const activeJob = computed(() => (wasmMode.value ? localJob.value : store.activeAgentJob))

const busy = ref(false)
const keyBusy = ref(false)
const modelBusy = ref(false)
const setupMsg = ref('')
const keyInput = ref('')
const savedConfigId = ref('')
const models = ref<string[]>([])
const permPreset = ref<'strict' | 'balanced' | 'yolo'>('balanced')

const form = reactive({
  providerId: 'openrouter',
  name: '',
  model: '',
  baseUrlOverride: '',
  dialectOverride: 'openAi' as 'openAi' | 'anthropic',
  authSchemeOverride: 'bearer' as 'bearer' | 'header' | 'query' | 'none',
  authNameOverride: '',
  workingDir: '',
  agentKind: 'build' as 'build' | 'plan',
  shellMode: 'auto' as 'auto' | 'cybsh' | 'device',
  autoApprove: false,
  maxTurns: 25,
})

// Shared with CodeStudio via localStorage — picking an assistant here switches
// it there too (and vice versa), so the sealed key is never asked for twice.
const chatConfigId = ref(readChatConfigId())
const promptInput = ref('')

function adoptSharedChatConfig(id: string) {
  if (!id || chatConfigId.value === id) return
  const ids = wasmMode.value
    ? new Set(localConfigs.value.map(c => c.id))
    : new Set(configs.value.map(c => c.id))
  if (ids.size && !ids.has(id)) return
  chatConfigId.value = id
}
function onSharedChatConfig(e: Event) {
  adoptSharedChatConfig((e as CustomEvent<string>).detail ?? '')
}
function onSharedStorage(e: StorageEvent) {
  const key = deploymentStorageEventKey(e.key)
  if (key === CHAT_CONFIG_ID_KEY && e.newValue) adoptSharedChatConfig(e.newValue)
  if (key === LOCAL_CONFIGS_KEY) refreshLocal()
}
function onSharedConfigs() { refreshLocal() }
function onSharedKeys() { refreshLocal() }
if (typeof window !== 'undefined') {
  window.addEventListener(CHAT_CONFIG_EVENT, onSharedChatConfig)
  window.addEventListener('storage', onSharedStorage)
  window.addEventListener(AGENT_CONFIGS_EVENT, onSharedConfigs)
  window.addEventListener(AGENT_KEYS_EVENT, onSharedKeys)
}
watch(chatConfigId, id => {
  if (id && readChatConfigId() !== id) writeChatConfigId(id)
})

/* ── modern UX state (progressive disclosure, no behaviour change) ── */
const activeTab = ref<'chat' | 'setup' | 'controls'>('chat')
const agentWindowUi = useWindowUi()
const agentMobileQuery = typeof window !== 'undefined' && window.matchMedia
  ? window.matchMedia('(max-width: 768px)')
  : null
const agentMobileViewport = ref(agentMobileQuery?.matches ?? false)
const agentWindowNarrow = computed(() => agentMobileViewport.value || agentWindowUi.width.value < 760)
const sidebarOpen = ref(!agentWindowNarrow.value)
watch(agentWindowNarrow, narrow => { if (narrow) sidebarOpen.value = false })
function onAgentMobileChange(event: MediaQueryListEvent) {
  agentMobileViewport.value = event.matches
}
const sessionSearch = ref('')
/** Capability strip starts collapsed — one summary line, details on demand. */
const capsOpen = ref(false)
/** Setup provider picker search. */
const providerSearch = ref('')
/** Controls accordion: only one section open at a time. */
const controlsOpen = ref<'tools' | 'fallback' | 'perms'>('perms')
const composerFocused = ref(false)
const copiedKey = ref('')
const slashOpen = ref(false)
const caps = computed(() => capabilitySummary(chatConfig.value, viewing.value, wasmMode.value))
const friendlyTransport = computed(() =>
  wasmMode.value ? 'On-device · browser sandbox' : 'Desktop · full tools',
)
const filteredSessions = computed(() => {
  const q = sessionSearch.value.trim().toLowerCase()
  if (!q) return sessions.value
  return sessions.value.filter(s => s.title.toLowerCase().includes(q))
})
const slashHints = computed(() => {
  const cur = promptInput.value
  if (!cur.startsWith('/')) return []
  const q = cur.slice(1).toLowerCase()
  return slashCommands.filter(c => c.cmd.slice(1).startsWith(q)).slice(0, 5)
})
const charCount = computed(() => promptInput.value.length)
async function copyRow(text: string, key: string) {
  const ok = await copyToClipboard(text)
  if (ok) {
    copiedKey.value = key
    window.setTimeout(() => { if (copiedKey.value === key) copiedKey.value = '' }, 1400)
  }
}
function useQuick(prompt: string) {
  promptInput.value = prompt
  activeTab.value = 'chat'
}
function applySlash(insert: string) {
  promptInput.value = insert
  slashOpen.value = false
}
function onComposerKey(e: KeyboardEvent) {
  if (e.key === '/' && promptInput.value === '') slashOpen.value = true
  else if (e.key === 'Escape') slashOpen.value = false
}
function openSetup() {
  activeTab.value = 'setup'
}

/* ── voice dictation (prose mode: punctuation words → marks) ── */
const voice = useVoiceInput('prose')
let stopVoice: (() => void) | null = null
function toggleVoice() {
  if (voice.listening.value) {
    stopVoice?.()
    stopVoice = null
    return
  }
  stopVoice = voice.dictateInto(promptInput)
}
const answerInput = ref('')
const denyReason = ref('')
const importEl = ref<HTMLInputElement | null>(null)

const chatConfig = computed(() => configs.value.find(c => c.id === chatConfigId.value) ?? null)

// ─── capability surface: what the agent may do, before it does it ───
const capVisible = computed(() => !!chatConfig.value || !!viewing.value)

// One tool list for both transports (native parity: the same 11 tools;
// wasm-only limits surface per-tool via `unsupported`, not a second list).
const AGENT_TOOLS = ['read', 'write', 'edit', 'list', 'grep', 'glob', 'bash', 'task', 'question', 'memory_recall', 'memory_remember', 'self_research', 'skill_save', 'mcp_attach', 'repo_analyze', 'os_exec', 'ui_open_panel', 'ui_notify', 'secret_list', 'secret_get']

/** Per-tool default action, evaluated through the same `decide` as the loop. */
const toolPerms = computed(() => {
  const cfg = chatConfig.value
  if (!cfg) return []
  const base = toolPermissions(cfg.permission, cfg.agentKind, AGENT_TOOLS).map(p => ({
    ...p,
    unsupported: false,
  }))
  // Attached MCP servers are dynamic tools — surface each one so the model
  // permission for `mcp__server__tool` is visible next to the statics.
  for (const [name, server] of Object.entries(cfg.mcpServers ?? {})) {
    if (server.enabled === false) continue
    const probe = toolPermissions(cfg.permission, cfg.agentKind, [`mcp__${name}__probe`])
    const action = probe[0]?.action ?? 'ask'
    base.push({
      tool: `mcp__${name}__*`,
      action: action as 'allow' | 'ask' | 'deny',
      unsupported: (server.transport ?? 'http') !== 'http' && wasmMode.value,
    })
  }
  return base
})

function toolHelp(t: { tool: string; action: string; unsupported?: boolean }): string {
  if (t.unsupported) return `${t.tool} needs the dashboard on this transport — it answers unsupported:`
  if (t.action === 'deny') return `${t.tool} is denied by the ruleset`
  if (t.action === 'allow') return `${t.tool} runs without asking`
  return `${t.tool} opens the approval card before it runs`
}

// ─── YOLO toggle: one tap to allow everything on the current config ───
const yoloBusy = ref(false)
const yoloOn = computed(() => isYoloConfig(chatConfig.value))
const yoloTitle = computed(() =>
  yoloOn.value
    ? 'YOLO is ON — every tool runs without asking. Click to go back to balanced ask-by-default.'
    : 'YOLO — allow all tools, auto-approve asks, enable all MCP servers (plan becomes build). Protected standing-order writes still ask.',
)

async function toggleYolo() {
  const cfg = chatConfig.value
  if (!cfg || yoloBusy.value) return
  const enabling = !yoloOn.value
  const updated = enabling ? applyYoloToConfig(cfg) : applyBalancedToConfig(cfg)
  yoloBusy.value = true
  try {
    if (wasmMode.value) {
      saveLocalConfig(updated)
      refreshLocal()
    } else {
      const saved = await store.saveAgentConfig({ ...updated })
      if (!saved) return
    }
    // Keep the SETUP wizard in sync so a later SAVE there keeps YOLO.
    permPreset.value = enabling ? 'yolo' : 'balanced'
    form.autoApprove = updated.autoApprove
    form.agentKind = updated.agentKind
    const mcpCount = Object.keys(updated.mcpServers ?? {}).length
    const planNote = enabling && cfg.agentKind === 'plan' ? ' (plan → build)' : ''
    store.notifySuccess(
      enabling
        ? `YOLO ON — all tools allowed, auto-approve, ${mcpCount} MCP server(s) enabled${planNote}`
        : 'YOLO OFF — back to balanced ask-by-default',
    )
  } finally {
    yoloBusy.value = false
  }
}

const sessionConfigOptions = computed(() => [
  { label: 'SELECT CONFIG', value: '' },
  ...configs.value.map(c => ({ label: `${c.name} (${c.model})`, value: c.id })),
])

// ─── MCP attach/detach/discover (selected config) ───
const mcpTools = ref<Array<{ server: string; name: string; description: string }>>([])
const mcpBusy = ref(false)
const mcpMsg = ref('')
const mcpForm = reactive({ name: '', transport: 'stdio', command: '', args: '', url: '' })

const mcpEntries = computed(() => {
  const cfg = chatConfig.value
  if (!cfg) return []
  return Object.entries(cfg.mcpServers ?? {}).map(([name, server]) => ({ name, cfg: server }))
})

async function addMcp() {
  if (!chatConfigId.value || !mcpForm.name.trim()) return
  mcpBusy.value = true
  mcpMsg.value = ''
  try {
    const server = {
      transport: mcpForm.transport,
      command: mcpForm.transport === 'stdio' ? mcpForm.command.trim() || undefined : undefined,
      args: mcpForm.transport === 'stdio' ? mcpForm.args.split(/\s+/).filter(Boolean) : [],
      env: {},
      url: mcpForm.transport === 'http' ? mcpForm.url.trim() || undefined : undefined,
      headers: [],
      enabled: true,
    }
    const updated = await store.mcpAddServer(chatConfigId.value, mcpForm.name.trim(), server)
    if (updated) {
      mcpForm.name = ''
      mcpForm.command = ''
      mcpForm.args = ''
      mcpForm.url = ''
      mcpMsg.value = `Attached — ${updated.mcpServers ? Object.keys(updated.mcpServers).length : 0} server(s).`
    }
  } finally {
    mcpBusy.value = false
  }
}

async function removeMcp(name: string) {
  if (!chatConfigId.value) return
  mcpBusy.value = true
  try {
    await store.mcpRemoveServer(chatConfigId.value, name)
    mcpTools.value = mcpTools.value.filter(t => t.server !== name)
  } finally {
    mcpBusy.value = false
  }
}

async function refreshMcpTools() {
  if (!chatConfigId.value) return
  mcpBusy.value = true
  mcpMsg.value = ''
  try {
    const tools = await store.mcpListTools(chatConfigId.value)
    if (tools) {
      mcpTools.value = tools
      mcpMsg.value = tools.length ? `${tools.length} tools discovered.` : 'Connected — no tools exposed.'
    }
  } finally {
    mcpBusy.value = false
  }
}

async function compactThread() {
  if (!viewing.value || !viewing.value.messages.length || jobActive.value) return
  // Browser build: same one-no-tools-turn shape as the native
  // `compact_session` (summary → fresh "(compacted)" session, old kept for
  // revert, handoff stored to local memory best-effort).
  if (wasmMode.value) {
    await compactThreadLocal()
    return
  }
  const compacted = await store.compactAgentSession(viewing.value.configId, viewing.value.id)
  if (compacted) setViewing(compacted)
}

/** Local compaction: summarize via one provider turn, then start a fresh
 * session seeded with the handoff (native `compact_session_for` parity). */
async function compactThreadLocal() {
  const session = viewing.value
  if (!session || !session.messages.length || jobActive.value) return
  const cfg = localConfigs.value.find(c => c.id === session.configId)
  if (!cfg) {
    store.notifyError('No local config', 'save one in SETUP first')
    return
  }
  const preset = localPresetFor(cfg)
  const base = (cfg.baseUrlOverride || preset?.baseUrl || '').replace(/\/$/, '')
  if (!base) {
    store.notifyError('No endpoint', 'set an endpoint override or pick a preset with one')
    return
  }
  const key = localKeys.value[cfg.id] ?? getLocalKey(cfg.id) ?? ''
  if (!key && !(preset?.keyless ?? false)) {
    store.notifyError('No API key', 'paste the key in SETUP (kept in memory only)')
    return
  }
  const dialect = (cfg.dialectOverride ?? preset?.dialect ?? 'openAi') as 'openAi' | 'anthropic'
  const auth = (cfg.authSchemeOverride ?? preset?.auth ?? 'bearer') as 'bearer' | 'header' | 'query' | 'none'
  const headers: Array<[string, string]> = [...(preset?.extraHeaders ?? [])]
  if (auth === 'bearer' && key) headers.push(['Authorization', `Bearer ${key}`])
  else if (auth === 'header') headers.push([cfg.authNameOverride || preset?.authName || 'x-api-key', key])
  const url =
    auth === 'query'
      ? `${base}${dialect === 'anthropic' ? '/v1/messages' : '/chat/completions'}?${encodeURIComponent(cfg.authNameOverride || preset?.authName || 'key')}=${encodeURIComponent(key)}`
      : dialect === 'anthropic' ? `${base}/v1/messages` : `${base}/chat/completions`
  // Native handoff shape: newest-first, 1 KiB/msg, 80 KiB total cap.
  const handoff = [...session.messages]
    .reverse()
    .map(m => `${m.role}: ${(m.content ?? '').slice(0, 1024)}`)
    .join('\n\n')
    .slice(0, 80 * 1024)
  if (!handoff.trim()) return
  busy.value = true
  try {
    const { wasmAgentPrompt } = await import('@/composables/useWasmBackend')
    const res = await wasmAgentPrompt({
      url,
      dialect,
      model: cfg.model,
      headers,
      system: 'You compress session transcripts into actionable handoffs. Output the summary only.',
      messages: [{
        role: 'user',
        content: `Summarize this coding session into a handoff for a fresh agent: decisions made, files changed, errors seen, and the next concrete step. Be specific with file paths and tool results.\n\nTRANSCRIPT:\n${handoff}`,
      }],
      tools: false,
    })
    if (!res.ok || !res.turn?.content?.trim()) {
      store.notifyError('Compaction failed', res.error ?? 'integrity: compaction produced an empty summary')
      return
    }
    const summary = res.turn.content.trim()
    const now = localNow()
    const inTokens = res.turn.usage?.inputTokens ?? 0
    const outTokens = res.turn.usage?.outputTokens ?? 0
    const compacted: AgentSession = {
      id: newLocalId('ses'),
      title: `${session.title.slice(0, 48)} (compacted)`,
      configId: session.configId,
      providerId: session.providerId,
      model: session.model,
      agentKind: session.agentKind,
      workingDir: session.workingDir,
      messages: [{
        role: 'user',
        content: `Previous session summary (compacted at ${now}):\n${summary}`,
      }],
      usage: {
        inputTokens: session.usage.inputTokens + inTokens,
        outputTokens: session.usage.outputTokens + outTokens,
      },
      createdAt: now,
      updatedAt: now,
    }
    saveLocalSession(compacted)
    // Handoff doubles as long-term memory (best-effort, never breaks compact).
    try {
      const { localMemories } = await import('@/composables/useAgent')
      localMemories.remember(cfg.id, summary, 'compactHandoff')
    } catch {
      // Memory must never break compaction.
    }
    refreshLocal()
    setViewing(compacted)
    store.notifySuccess('Session compacted — old transcript kept for revert')
  } catch (e) {
    store.notifyError('Compaction failed', e)
  } finally {
    busy.value = false
  }
}

// ─── granular permission editor (P2.3): per-tool selects writing the
// same ruleset the loop enforces ───
const permBusy = ref(false)
const permMsg = ref('')
const permDraft = ref<Record<string, 'allow' | 'ask' | 'deny'>>({})

const permEditorTools = computed(() => {
  const cfg = chatConfig.value
  return AGENT_TOOLS.map(tool => {
    const draft = permDraft.value[tool]
    if (draft) return { tool, action: draft }
    const rule = cfg?.permission.rules[tool]
    const action = typeof rule === 'string' ? rule : (cfg?.permission.default ?? 'ask')
    return { tool, action: (action === 'allow' || action === 'deny' ? action : 'ask') as 'allow' | 'ask' | 'deny' }
  })
})

function setPermTool(tool: string, action: 'allow' | 'ask' | 'deny') {
  permDraft.value = { ...permDraft.value, [tool]: action }
}

function resetPermRules() {
  permDraft.value = {}
  permMsg.value = 'Draft cleared — SAVE RULES writes the balanced preset.'
}

async function savePermRules() {
  const cfg = chatConfig.value
  if (!cfg || permBusy.value) return
  const rules: Record<string, 'allow' | 'ask' | 'deny'> = {}
  for (const t of permEditorTools.value) rules[t.tool] = permDraft.value[t.tool] ?? t.action
  const updated: AgentConfig = {
    ...cfg,
    permission: { default: cfg.permission.default, rules },
    updatedAt: new Date().toISOString(),
  }
  permBusy.value = true
  try {
    if (wasmMode.value) {
      saveLocalConfig(updated)
      refreshLocal()
    } else {
      const saved = await store.saveAgentConfig(updated)
      if (!saved) return
    }
    permDraft.value = {}
    permMsg.value = `Saved — ${Object.values(rules).filter(a => a === 'deny').length} denied (stripped from the native schema).`
  } finally {
    permBusy.value = false
  }
}

// ─── fallback route editor: ordered assistant ids tried when the primary
// provider call dies (dead key, throttle, empty credits, offline) ───
const fallbackDraft = ref<string[] | null>(null)
const fallbackPick = ref('')
const fallbackBusy = ref(false)
const fallbackMsg = ref('')
const fallbackList = computed(() => fallbackDraft.value ?? chatConfig.value?.fallbackIds ?? [])
const fallbackOptions = computed(() => [
  { label: 'SELECT ASSISTANT', value: '' },
  ...configs.value
    .filter(c => c.id !== chatConfigId.value && !fallbackList.value.includes(c.id))
    .map(c => ({ label: `${c.name} (${c.providerId} · ${c.model})`, value: c.id })),
])

function fallbackName(id: string): string {
  const c = configs.value.find(c => c.id === id)
  return c ? `${c.name} (${c.providerId} · ${c.model})` : `${id.slice(0, 8)}… (deleted)`
}

function fallbackReady(id: string): boolean {
  const c = configs.value.find(c => c.id === id)
  return c ? (c.hasKey || isKeyless(c)) : false
}

function addFallback() {
  if (!fallbackPick.value) return
  if (fallbackList.value.length >= 4) {
    fallbackMsg.value = 'Max 4 fallbacks (5 routes total).'
    return
  }
  fallbackDraft.value = [...fallbackList.value, fallbackPick.value]
  fallbackPick.value = ''
  fallbackMsg.value = ''
}

function removeFallback(i: number) {
  fallbackDraft.value = fallbackList.value.filter((_, j) => j !== i)
}

function moveFallback(i: number, d: number) {
  const arr = [...fallbackList.value]
  const j = i + d
  if (j < 0 || j >= arr.length) return
  ;[arr[i], arr[j]] = [arr[j], arr[i]]
  fallbackDraft.value = arr
}

async function saveFallbacks() {
  const cfg = chatConfig.value
  if (!cfg || fallbackBusy.value) return
  const updated: AgentConfig = {
    ...cfg,
    fallbackIds: [...fallbackList.value],
    updatedAt: new Date().toISOString(),
  }
  fallbackBusy.value = true
  try {
    if (wasmMode.value) {
      saveLocalConfig(updated)
      refreshLocal()
    } else {
      const saved = await store.saveAgentConfig(updated)
      if (!saved) return
    }
    fallbackDraft.value = null
    fallbackMsg.value = (updated.fallbackIds ?? []).length
      ? `Saved — ${(updated.fallbackIds ?? []).length} fallback(s) in route.`
      : 'Saved — no fallbacks.'
  } finally {
    fallbackBusy.value = false
  }
}

watch(chatConfigId, () => {
  fallbackDraft.value = null
  fallbackPick.value = ''
  fallbackMsg.value = ''
})

// Server-mode configs live in Pinia: adopt the selection CodeStudio persisted
// and drop it when the config is deleted elsewhere.
watch(() => store.agentConfigs.map(c => c.id).join(','), () => {
  if (wasmMode.value) return
  const ids = new Set(store.agentConfigs.map(c => c.id))
  if (chatConfigId.value && !ids.has(chatConfigId.value)) {
    const shared = readChatConfigId()
    chatConfigId.value = shared && ids.has(shared) ? shared : (store.agentConfigs[0]?.id ?? '')
  }
  if (!chatConfigId.value) {
    const shared = readChatConfigId()
    if (shared && ids.has(shared)) chatConfigId.value = shared
    else if (store.agentConfigs.length) chatConfigId.value = store.agentConfigs[0].id
  }
})

// ─── auto-compaction (P3): meter ≥85 % with an idle thread compacts once
// per session instead of failing the next turn with `context:`.
// NOTE: the watcher lives next to contextPct/jobActive below — `watch`
// evaluates its getter eagerly during setup, so registering it here (before
// those consts are initialized) threw TDZ `Cannot access ... before
// initialization` and blanked the whole Agent window.
const autoCompactedFor = ref('')

/** Mode-aware viewer setter (server viewing lives in a ref, local in state). */
function setViewing(s: AgentSession | null) {
  if (wasmMode.value) localViewing.value = s
  else serverViewing.value = s
}

const preset = computed(() => providers.value.find(p => p.id === form.providerId) ?? null)
const presetBase = computed(() => preset.value?.baseUrl ?? 'https://…')
const isCustom = computed(() => form.providerId === 'custom')
const canSave = computed(() => form.name.trim() !== '' && form.model.trim() !== '' && form.providerId !== '')
/** Saving a key targets the sidebar selection first, else the just-saved wizard
 *  row — so a reload (savedConfigId reset) or picking an existing assistant
 *  still enables SAVE KEY, and switching assistants keys the right one. */
const effectiveKeyConfigId = computed(() => chatConfigId.value || savedConfigId.value || '')

// ─── browser-local mode (static host): same shapes, localStorage rows ──

function newLocalId(prefix: string): string {
  try {
    return `${prefix}-${crypto.randomUUID().slice(0, 8)}`
  } catch {
    return `${prefix}-${Date.now().toString(36)}`
  }
}

function localNow(): string {
  return new Date().toISOString()
}

function refreshLocal() {
  // Touch the revision so hasKey badges re-evaluate when CodeStudio seals a
  // key into the shared in-memory vault (a non-reactive Map).
  void localKeyRevision.value
  localConfigs.value = listLocalConfigs().map(c => ({
    ...c,
    hasKey: !!localKeys.value[c.id] || hasLocalKey(c.id) || c.hasKey,
  }))
  localSessions.value = listLocalSessions()
  if (wasmMode.value) {
    if (chatConfigId.value && !localConfigs.value.some(c => c.id === chatConfigId.value)) {
      const shared = readChatConfigId()
      chatConfigId.value = localConfigs.value.some(c => c.id === shared) ? shared : ''
    }
    if (!chatConfigId.value) {
      const shared = readChatConfigId()
      if (shared && localConfigs.value.some(c => c.id === shared)) chatConfigId.value = shared
      else if (localConfigs.value.length) chatConfigId.value = localConfigs.value[0].id
    }
  }
}

function localPresetFor(config: AgentConfig): ProviderPreset | null {
  return (
    localPresets.value.find(p => p.id === config.providerId) ??
    providers.value.find(p => p.id === config.providerId) ??
    null
  )
}

function localSystemPrompt(config: AgentConfig): string {
  const root = config.workingDir ? `/${config.workingDir}` : '/'
  return (
    `You are CyberManju, an AI coding agent running fully in the browser over a local file volume.\n` +
    `Working root: ${root}\n` +
    `Agent mode: ${config.agentKind} (plan = read-only, never edit).\n` +
    `SANDBOX: browser file volume — read/list/grep/glob/write/edit plus cybsh-subset bash, ` +
    `one bounded subagent (task) with the full file/shell toolset, and HTTP MCP servers (mcp__*). Device shell ` +
    `(curl/wget/git/python) and stdio MCP need the dashboard and answer unsupported: there.\n` +
    `TOOLS — paths: leading / = volume root, else working-dir-relative.\n` +
    `- read {path}: always read a file before editing it; the output ends with a ` +
    `\`[blake3:<hex>]\` line — pass it as expected_hash on edit, and never write it back ` +
    `(write/edit strip it automatically).\n` +
    `- list {path?}: one directory level; orient at / first.\n` +
    `- grep {pattern, path?, limit?}: regex over contents (invalid regex searches literally).\n` +
    `- glob {pattern, path?}: find files (* stays in one segment, ** crosses).\n` +
    `- edit {path, old_block, new_block, expected_hash?}: replace ONE exact block; missing → not_found:, ` +
    `ambiguous → conflict:, then re-read and send a larger block. expected_hash pins the file ` +
    `you read so a concurrent writer cannot slip through.\n` +
    `- write {path, content}: full-file create/overwrite; prefer edit for small changes.\n` +
    `- bash {command}: cybsh volume commands (ls/cat/cp/mv/rm/mkdir/ls/search/compute); device verbs answer unsupported:.\n` +
    `- task {goal, context?}: one bounded subagent with the full file/shell toolset (read/edit/write/bash, 5 turns).\n` +
    `- question {question}: ask the human when genuinely blocked — sparingly.\n` +
    `- self_research {query, path?, limit?}: introspect YOUR OWN source — sweep this repo for how something works, get file:line snippets back. Read-only, plan-safe. Call it before claiming how the agent works.\n` +
    `- skill_save {name, description, content}: persist a reusable skill into the \`.cybermanju\` container (.cybermanju/skills/<name>/SKILL.md) — survives restarts, syncs across devices.\n` +
    `- mcp_attach {name, url}: attach an HTTP MCP server to this assistant persistently (verified before saving; stdio refused here).\n` +
    `- repo_analyze {repo, branch?}: analyze any public GitHub repo WITHOUT cloning (metadata + tree + README over HTTPS; no git needed, works everywhere).\n` +
    `- os_exec {command}: run ONE cybsh line against the OS volume — same verbs as bash's cybsh subset, ` +
    `never a device shell (non-cybsh verbs answer unsupported:). One line per call; no pipes or chains.\n` +
    `- ui_open_panel {panel, tab?, path?}: open a UI panel for the user (files, terminal, agent, settings, …). ` +
    `Unknown ids answer not_found:; optional tab/path navigate inside the panel. Read-only and always allowed.\n` +
    `- ui_notify {level, message}: show the user a notification (info/success/warning/error). Always allowed; use sparingly.\n` +
    `- secret_list {}: list vault secret METADATA only (id/title/kind/username/url/tags/hasValue) — ` +
    `never values. Pair with secret_get only when the user explicitly asked you to use a stored credential.\n` +
    `- secret_get {id}: reveal ONE stored secret's plaintext. The result carries the plaintext to you — ` +
    `default permission is ASK (the approval card shows the title, never the value). Only call it when ` +
    `the user explicitly asked you to use this credential.\n` +
    `- mcp__server__tool: attached HTTP MCP servers only; stdio servers answer unsupported:.\n` +
    `- memory_recall {query, top_k?}: search long-term memory (past sessions, stored facts). ` +
    `Bounded and possibly stale — verify before acting.\n` +
    `- memory_remember {text}: store ONE durable fact for future sessions; one fact per call, ` +
    `never secrets or whole files.\n` +
    `INTERFACE: the Terminal panel runs cybsh — the same shell as your bash tool. Its ` +
    `\`theme\`/\`ui\` verbs customize the whole interface and persist to the volume mirror: ` +
    `\`ui theme <id>\`, \`ui accent <#hex|default> [--for <theme>]\`, \`ui density|glass|motion|glow\`, ` +
    `and inspect with \`ui get\`, \`ui vars [filter]\`, \`ui palette [theme]\`. When asked about ` +
    `the interface or cybsh verbs, call self_research first (topics: themes, cybsh verbs) ` +
    `and cite file:line — never invent verb names.\n` +
    `STANDING ORDERS: AGENTS.md, SKILL.md, .cybermanju/rules.md and persisted skills ` +
    `(.cybermanju/skills/*/SKILL.md) define your instructions, ` +
    `so writing one always asks for approval — AUTO APPROVE never covers them.\n` +
    `SELF-UNDERSTANDING: when asked how YOU work, call self_research first and cite the files it returns; ` +
    `extend yourself with skill_save (knowledge) and mcp_attach (HTTP tools).\n` +
    `WORKFLOW: orient (list/glob) → read → act → verify. Small verified steps; ` +
    `never invent file contents. Denials are information — work around them, never ` +
    `retry identically. Report errors with their machine prefix. Answer concisely; ` +
    `lead with what changed (file:line).`
  )
}

const customPreset: ProviderPreset = {
  id: 'custom',
  label: 'Custom endpoint',
  family: 'custom',
  baseUrl: '',
  defaultModel: '',
  dialect: 'openAi',
  auth: 'bearer',
  authName: null,
  keyEnv: '',
  keyless: false,
  extraHeaders: [],
}
const allPresets = computed(() => [...providers.value, ...(providers.value.some(p => p.id === 'custom') ? [] : [customPreset])])
const filteredPresets = computed(() => {
  const q = providerSearch.value.trim().toLowerCase()
  if (!q) return allPresets.value
  return allPresets.value.filter(
    p =>
      p.label.toLowerCase().includes(q) ||
      p.id.toLowerCase().includes(q) ||
      (p.defaultModel ?? '').toLowerCase().includes(q),
  )
})

function pickPreset(p: ProviderPreset) {
  form.providerId = p.id
  if (!form.model || !form.name) {
    form.model = p.defaultModel
    if (!form.name) form.name = p.label
  } else {
    form.model = p.defaultModel
  }
  form.baseUrlOverride = ''
  models.value = []
  setupMsg.value = p.keyless ? 'Keyless provider — save, no key needed.' : `Needs ${p.keyEnv || 'an API key'} — save config, then SEAL KEY.`
}

function isKeyless(cfg: AgentConfig) {
  return providers.value.find(p => p.id === cfg.providerId)?.keyless ?? false
}

function localConfigFromForm(): AgentConfig {
  const now = localNow()
  return {
    id: savedConfigId.value && wasmMode.value ? savedConfigId.value : newLocalId('cfg'),
    name: form.name.trim(),
    providerId: form.providerId,
    model: form.model.trim(),
    baseUrlOverride: form.baseUrlOverride.trim() || undefined,
    dialectOverride: isCustom.value ? form.dialectOverride : undefined,
    authSchemeOverride: isCustom.value ? form.authSchemeOverride : undefined,
    authNameOverride: isCustom.value && form.authNameOverride.trim() ? form.authNameOverride.trim() : undefined,
    workingDir: form.workingDir.trim(),
    agentKind: form.agentKind,
    shellMode: form.shellMode,
    permission: agentPermissionPreset(permPreset.value),
    autoApprove: form.autoApprove,
    maxTurns: Math.min(50, Math.max(1, form.maxTurns || 25)),
    hasKey: false,
    fallbackIds: [],
    createdAt: now,
    updatedAt: now,
    mcpServers: defaultMcpServers(),
  }
}

async function saveConfig() {
  busy.value = true
  setupMsg.value = ''
  try {
    if (wasmMode.value) {
      const cfg = localConfigFromForm()
      const prev = listLocalConfigs().find(c => c.id === cfg.id)
      // Re-saving the wizard row must not wipe the fallback route set in Controls.
      saveLocalConfig({ ...cfg, fallbackIds: prev?.fallbackIds ?? [], createdAt: prev?.createdAt ?? cfg.createdAt })
      refreshLocal()
      savedConfigId.value = cfg.id
      chatConfigId.value = cfg.id
      setupMsg.value = 'Saved locally — paste the key below (kept in memory only).'
      return
    }
    const saved = await store.saveAgentConfig({
      name: form.name.trim(),
      providerId: form.providerId,
      model: form.model.trim(),
      baseUrlOverride: form.baseUrlOverride.trim() || undefined,
      dialectOverride: isCustom.value ? form.dialectOverride : undefined,
      authSchemeOverride: isCustom.value ? form.authSchemeOverride : undefined,
      authNameOverride: isCustom.value && form.authNameOverride.trim() ? form.authNameOverride.trim() : undefined,
      workingDir: form.workingDir.trim(),
      agentKind: form.agentKind,
      shellMode: form.shellMode,
      permission: agentPermissionPreset(permPreset.value),
      autoApprove: form.autoApprove,
      maxTurns: Math.min(50, Math.max(1, form.maxTurns || 25)),
      fallbackIds: [],
      mcpServers: defaultMcpServers(),
    })
    if (saved) {
      savedConfigId.value = saved.id
      chatConfigId.value = saved.id
      setupMsg.value = isKeyless(saved)
        ? 'Saved — keyless provider, ready to chat.'
        : 'Saved — now SEAL KEY above, then chat.'
    }
  } finally {
    busy.value = false
  }
}

async function saveKey() {
  const targetId = effectiveKeyConfigId.value
  const key = keyInput.value.trim()
  if (!targetId || !key) return
  if (wasmMode.value) {
    localKeys.value[targetId] = key
    setLocalKey(targetId, key)
    keyInput.value = ''
    // Keep the wizard + sidebar pointing at the config that just got its key.
    savedConfigId.value = targetId
    chatConfigId.value = targetId
    refreshLocal()
    setupMsg.value = 'Key held in memory for this page only — never stored.'
    return
  }
  keyBusy.value = true
  try {
    if (await store.saveAgentKey(targetId, key)) {
      keyInput.value = ''
      savedConfigId.value = targetId
      setupMsg.value = 'Key sealed — never shown back.'
    }
  } finally {
    keyBusy.value = false
  }
}

async function removeCfg(id: string) {
  if (wasmMode.value) {
    deleteLocalConfig(id)
    delete localKeys.value[id]
    forgetLocalKey(id)
    refreshLocal()
  } else {
    await store.deleteAgentConfig(id)
  }
  if (chatConfigId.value === id) chatConfigId.value = ''
  if (readChatConfigId() === id) writeChatConfigId('')
}

async function refreshModels() {
  if (wasmMode.value) {
    const cfg = localConfigs.value.find(c => c.id === (savedConfigId.value || chatConfigId.value))
    const preset = cfg ? localPresetFor(cfg) : null
    const base = (cfg?.baseUrlOverride || preset?.baseUrl || '').replace(/\/$/, '')
    if (!base) {
      setupMsg.value = 'Save the config first, then refresh.'
      return
    }
    if ((preset?.dialect ?? 'openAi') === 'anthropic') {
      setupMsg.value = 'Anthropic has no list API — enter the model id manually.'
      return
    }
    modelBusy.value = true
    try {
      const headers: Record<string, string> = {}
      const key = localKeys.value[cfg?.id ?? ''] ?? getLocalKey(cfg?.id ?? '') ?? ''
      const auth = preset?.auth ?? 'bearer'
      if (auth === 'bearer' && key) headers['Authorization'] = `Bearer ${key}`
      else if (auth === 'header') headers[preset?.authName ?? 'x-api-key'] = key
      for (const [k, v] of preset?.extraHeaders ?? []) headers[k] = v
      const url = preset?.auth === 'query'
        ? `${base}/models?${encodeURIComponent(preset?.authName ?? 'key')}=${encodeURIComponent(key)}`
        : `${base}/models`
      const res = await fetch(url, { headers })
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const body = (await res.json()) as { data?: Array<{ id?: string }> }
      const list = (body.data ?? []).map(m => m.id ?? '').filter(Boolean)
      models.value = [...new Set(list)].sort()
      setupMsg.value = `${models.value.length} models listed.`
    } catch (e) {
      setupMsg.value = `Refresh failed: ${e instanceof Error ? e.message : String(e)}`
    } finally {
      modelBusy.value = false
    }
    return
  }
  if (!effectiveKeyConfigId.value) {
    setupMsg.value = 'Save the config first, then refresh.'
    return
  }
  modelBusy.value = true
  try {
    const list = await store.refreshAgentModels(effectiveKeyConfigId.value)
    if (list) {
      models.value = list
      setupMsg.value = `${list.length} models listed.`
    }
  } finally {
    modelBusy.value = false
  }
}

async function newSession() {
  if (!chatConfigId.value) return
  if (wasmMode.value) {
    const cfg = localConfigs.value.find(c => c.id === chatConfigId.value)
    if (!cfg) return
    const now = localNow()
    const s: AgentSession = {
      id: newLocalId('ses'),
      title: 'Untitled session',
      configId: cfg.id,
      providerId: cfg.providerId,
      model: cfg.model,
      agentKind: cfg.agentKind,
      workingDir: cfg.workingDir,
      messages: [],
      usage: { inputTokens: 0, outputTokens: 0 },
      createdAt: now,
      updatedAt: now,
    }
    saveLocalSession(s)
    refreshLocal()
    setViewing(s)
    return
  }
  const s = await (async () => {
    try {
      const { invoke } = await import('@/composables/useTauri')
      const created = await invoke<AgentSession>('create_agent_session', { configId: chatConfigId.value })
      await store.fetchAgentSessions()
      return created
    } catch (e) {
      store.notifyError('Failed to create session', e)
      return null
    }
  })()
  if (s) setViewing(s)
}

async function loadSession(id: string) {
  if (wasmMode.value) {
    setViewing(listLocalSessions().find(s => s.id === id) ?? null)
    return
  }
  setViewing(await store.loadAgentSession(id))
}

async function removeSession(id: string) {
  if (wasmMode.value) {
    deleteLocalSession(id)
    refreshLocal()
  } else {
    await store.deleteAgentSession(id)
  }
  if (viewing.value?.id === id) setViewing(null)
}

async function exportSession(id: string) {
  const s = wasmMode.value
    ? listLocalSessions().find(s => s.id === id) ?? null
    : await store.loadAgentSession(id)
  if (!s) return
  const blob = new Blob([JSON.stringify(s, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `agent-session-${id}.json`
  a.click()
  URL.revokeObjectURL(url)
}

function importClick() {
  importEl.value?.click()
}

async function importFile(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (!file) return
  try {
    const text = await file.text()
    const parsed = JSON.parse(text) as AgentSession
    if (!Array.isArray(parsed.messages)) throw new Error('not an agent session transcript')
    if (wasmMode.value) {
      const now = localNow()
      saveLocalSession({
        ...parsed,
        id: newLocalId('ses'),
        createdAt: parsed.createdAt || now,
        updatedAt: now,
      })
      refreshLocal()
    } else {
      const { invoke } = await import('@/composables/useTauri')
      await invoke('import_agent_session', { session: parsed })
      await store.fetchAgentSessions()
    }
  } catch (err) {
    store.notifyError('Import failed', err)
  } finally {
    (e.target as HTMLInputElement).value = ''
  }
}

const pendingApproval = computed(() => {
  if (wasmMode.value) {
    const p = agent.pendingApproval.value
    if (!p) return null
    return { tool: p.tool, input: p.input, summary: p.summary, question: p.question ?? null }
  }
  const job = activeJob.value
  if (!job || job.status !== 'waiting_approval' || !job.pending) return null
  if (viewing.value && job.sessionId !== viewing.value.id) return null
  return job.pending
})

const jobActive = computed(() => {
  if (wasmMode.value) return agent.running.value
  const s = activeJob.value?.status
  return s === 'running' || s === 'waiting_approval'
})

const jobLine = computed(() => {
  const job = activeJob.value
  if (!job) return ''
  const bits: string[] = [
    wasmMode.value ? 'LOCAL JOB' : `JOB ${job.jobId.slice(0, 8)}…`,
    job.status.toUpperCase(),
    `TURN ${job.turnsUsed}/${job.maxTurns}`,
  ]
  if (job.activity) bits.push(job.activity)
  if (elapsed.value) bits.push(elapsed.value)
  if (queue.value.length) bits.push(`${queue.value.length} QUEUED`)
  return bits.join(' · ')
})

const jobError = computed(() => {
  if (wasmMode.value) return localJob.value?.error ?? ''
  return activeJob.value?.error ?? ''
})
const jobHint = computed(() => {
  if (!jobError.value) return ''
  const d = agentErrorHint(jobError.value)
  return `${d.prefix}: ${d.hint}`
})

// ─── thread rendering: grouped tools, live state, context awareness ───
const messagesEl = ref<HTMLElement | null>(null)
const openRows = ref<Set<string>>(new Set())
const queue = ref<string[]>([])
const startedAt = ref(0)
const nowTick = ref(Date.now())
let tickTimer = 0
let liveTimer = 0

const threadRows = computed(() => buildThread(viewing.value?.messages ?? [], jobActive.value))

/** Keep the newest message in view: the transcript only grows downward,
 * so every content change pins the scroller to the bottom. */
function scrollToBottom() {
  void nextTick(() => {
    const el = messagesEl.value
    if (el) el.scrollTop = el.scrollHeight
  })
}

/** Row count + last-row content length: catches new messages AND a running
 * turn appending to the current row (tool output, approval text). */
const threadSignature = computed(() => {
  const rows = threadRows.value
  const last = rows[rows.length - 1]
  const lastLen = last
    ? last.kind === 'message'
      ? (last.message.content ?? '').length
      : JSON.stringify(last.rows ?? last).length
    : 0
  return [rows.length, lastLen, jobActive.value ? 1 : 0, viewing.value?.id ?? ''] as const
})

watch(threadSignature, () => scrollToBottom())

// Returning to the chat tab (or opening the panel) re-pins to the newest
// message — the thread only grows downward.
watch(activeTab, t => {
  if (t === 'chat') scrollToBottom()
})

const lastAssistantKey = computed(() => {
  const rows = threadRows.value
  for (let i = rows.length - 1; i >= 0; i--) {
    const r = rows[i]
    if (r.kind === 'message' && r.message.role === 'assistant' && r.message.content) return r.key
  }
  return ''
})

const contextTokens = computed(() => estimateTranscriptTokens(viewing.value?.messages ?? []))
const contextWindow = computed(() => contextWindowFor(chatConfig.value?.model || viewing.value?.model || ''))
const contextPct = computed(() => {
  const w = contextWindow.value || 1
  return Math.max(0, Math.min(100, Math.round((contextTokens.value / w) * 100)))
})
const ctxTone = computed(() => (contextPct.value >= 85 ? 'bad' : contextPct.value >= 60 ? 'warn' : 'ok'))

// Auto-compaction watcher: registered HERE (after contextPct + jobActive are
// initialized) because `watch` runs its getter once at registration —
// registering earlier threw a TDZ ReferenceError and blanked the panel.
watch(
  () => [contextPct.value, jobActive.value, viewing.value?.id] as const,
  ([pct, active, sessionId]) => {
    if (pct < 85 || active || !sessionId || !viewing.value?.messages.length) return
    if (autoCompactedFor.value === sessionId) return
    autoCompactedFor.value = sessionId
    store.notifySuccess('Context ≥85% — auto-compacting (old transcript kept)')
    void compactThread()
  },
)
const costUsd = computed(() => {
  const v = viewing.value
  return v ? estimateCost(v.model, v.usage) : null
})
const costLabel = computed(() => {
  const c = costUsd.value
  if (c == null) return ''
  return c < 0.01 ? c.toFixed(4) : c.toFixed(3)
})
const footerLine = computed(() => {
  const v = viewing.value
  if (!v) return ''
  const bits = [`AGENT · ${v.model}`, v.agentKind.toUpperCase(), `${v.messages.length} msgs`, `CTX ~${fmtTokens(contextTokens.value)}`]
  if (costUsd.value != null) bits.push(`~$${costLabel.value} EST`)
  return bits.join(' · ')
})

const elapsed = computed(() => {
  if (!startedAt.value) return ''
  const s = Math.max(0, Math.round((nowTick.value - startedAt.value) / 1000))
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, '0')}s`
})

function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`
  return String(n)
}

function isOpen(key: string): boolean {
  return openRows.value.has(key)
}
function toggleRow(key: string) {
  const next = new Set(openRows.value)
  if (next.has(key)) next.delete(key)
  else next.add(key)
  openRows.value = next
}

const approvalArg = computed(() => (pendingApproval.value ? salientArg(pendingApproval.value.input) : ''))
/** Pre/post diff for edit approvals (P2.1) — raw JSON stays for everything else. */
const approvalDiff = computed(() => {
  const p = pendingApproval.value
  if (!p || p.tool !== 'edit') return null
  const blocks = editBlocksOf(p.input)
  if (!blocks) return null
  return diffBlocks(blocks.oldBlock, blocks.newBlock)
})
const approvalInput = computed(() => {
  const p = pendingApproval.value
  if (!p) return ''
  try {
    const text = JSON.stringify(p.input ?? {}, null, 1)
    return text.length > 600 ? `${text.slice(0, 600)}…` : text
  } catch {
    return ''
  }
})

/** The prompt the user typed lands in the box even while a run is live. */
const canSend = computed(() => promptInput.value.trim() !== '' && chatConfigId.value !== '')

async function sendPrompt() {
  if (!canSend.value) return
  // A run is live: hold the prompt and drain it when the job settles.
  if (jobActive.value) {
    queue.value.push(promptInput.value.trim())
    promptInput.value = ''
    scrollToBottom()
    return
  }
  if (wasmMode.value) {
    await sendPromptLocal()
    return
  }
  const prompt = promptInput.value.trim()
  promptInput.value = ''
  scrollToBottom()
  // Ensure a session exists for this config; the job returns transcript via polling.
  let sessionId = viewing.value && viewing.value.configId === chatConfigId.value ? viewing.value.id : undefined
  if (!sessionId) {
    const { invoke } = await import('@/composables/useTauri')
    try {
      const created = await invoke<AgentSession>('create_agent_session', { configId: chatConfigId.value })
      await store.fetchAgentSessions()
      setViewing(created)
      sessionId = created.id
    } catch (e) {
      store.notifyError('Failed to create session', e)
      return
    }
  }
  await store.startAgentRun(chatConfigId.value, prompt, sessionId)
}

/** Browser-local run: same thread UI, loop in useAgent, volume tools. */
async function sendPromptLocal() {
  const prompt = promptInput.value.trim()
  if (!prompt || !chatConfigId.value) return
  const cfg = localConfigs.value.find(c => c.id === chatConfigId.value)
  if (!cfg) {
    store.notifyError('No local config selected', 'save one in SETUP first')
    return
  }
  const preset = localPresetFor(cfg)
  const base = (cfg.baseUrlOverride || preset?.baseUrl || '').replace(/\/$/, '')
  if (!base) {
    store.notifyError('No endpoint', 'set an endpoint override or pick a preset with one')
    return
  }
  const key = localKeys.value[cfg.id] ?? getLocalKey(cfg.id) ?? ''
  // Fallback routes: same transcript continues on the next usable assistant
  // (extra account/key or another provider) when this one's call dies.
  const fallbackRoutes: LocalFailoverRoute[] = []
  {
    const seen = new Set([cfg.id])
    for (const fid of cfg.fallbackIds ?? []) {
      if (seen.has(fid) || fallbackRoutes.length >= 4) continue
      seen.add(fid)
      const fc = localConfigs.value.find(c => c.id === fid)
      if (!fc) continue
      const fp = localPresetFor(fc)
      const fbase = (fc.baseUrlOverride || fp?.baseUrl || '').replace(/\/$/, '')
      if (!fbase) continue
      const fkey = localKeys.value[fc.id] ?? getLocalKey(fc.id) ?? ''
      if (!fkey && !(fp?.keyless ?? false)) continue
      const fdialect = (fc.dialectOverride ?? fp?.dialect ?? 'openAi') as 'openAi' | 'anthropic'
      const fauth = (fc.authSchemeOverride ?? fp?.auth ?? 'bearer') as 'bearer' | 'header' | 'query' | 'none'
      const fheaders: Array<[string, string]> = [...(fp?.extraHeaders ?? [])]
      if (fauth === 'bearer' && fkey) fheaders.push(['Authorization', `Bearer ${fkey}`])
      else if (fauth === 'header') fheaders.push([fc.authNameOverride || fp?.authName || 'x-api-key', fkey])
      fallbackRoutes.push({ label: `${fc.name} · ${fc.model}`, baseUrl: fbase, dialect: fdialect, model: fc.model, headers: fheaders })
    }
  }
  if (!key && !(preset?.keyless ?? false) && !fallbackRoutes.length) {
    store.notifyError('No API key', 'paste the key in SETUP (kept in memory only)')
    return
  }
  const dialect = (cfg.dialectOverride ?? preset?.dialect ?? 'openAi') as 'openAi' | 'anthropic'
  const auth = (cfg.authSchemeOverride ?? preset?.auth ?? 'bearer') as 'bearer' | 'header' | 'query' | 'none'
  const headers: Array<[string, string]> = [...(preset?.extraHeaders ?? [])]
  if (auth === 'bearer' && key) headers.push(['Authorization', `Bearer ${key}`])
  else if (auth === 'header') headers.push([cfg.authNameOverride || preset?.authName || 'x-api-key', key])
  const url =
    auth === 'query'
      ? `${base}${dialect === 'anthropic' ? '/v1/messages' : '/chat/completions'}?${encodeURIComponent(cfg.authNameOverride || preset?.authName || 'key')}=${encodeURIComponent(key)}`
      : dialect === 'anthropic'
        ? `${base}/v1/messages`
        : `${base}/chat/completions`

  let session = viewing.value && viewing.value.configId === cfg.id ? viewing.value : null
  if (!session) {
    const now = localNow()
    session = {
      id: newLocalId('ses'),
      title: prompt.split(/\s+/).slice(0, 8).join(' ') || 'Untitled session',
      configId: cfg.id,
      providerId: cfg.providerId,
      model: cfg.model,
      agentKind: cfg.agentKind,
      workingDir: cfg.workingDir,
      messages: [],
      usage: { inputTokens: 0, outputTokens: 0 },
      createdAt: now,
      updatedAt: now,
    }
    saveLocalSession(session)
    refreshLocal()
    setViewing(session)
  }
  promptInput.value = ''
  session.messages.push({ role: 'user', content: prompt })
  session.updatedAt = localNow()
  saveLocalSession(session)
  refreshLocal()
  setViewing({ ...session })
  scrollToBottom()

  localJob.value = {
    jobId: newLocalId('job'),
    sessionId: session.id,
    configId: cfg.id,
    status: 'running',
    turnsUsed: 0,
    maxTurns: cfg.maxTurns,
    usage: { ...session.usage },
  }
  let outcome: { stopped: 'done' | 'limit' | 'aborted' | 'error'; error?: string }
  try {
    outcome = await runLocalAgent(
      {
        baseUrl: base,
        dialect,
        model: cfg.model,
        headers,
        system: localSystemPrompt(cfg) + recallBlockLocal(cfg.id, prompt),
        maxTurns: cfg.maxTurns,
        permission: cfg.permission,
        autoApprove: cfg.autoApprove,
        agentKind: cfg.agentKind,
        configId: cfg.id,
        fallbacks: fallbackRoutes,
        // "Allow always" must survive the run, not just this turn.
        onRemember: (tool: string) => {
          const updated: AgentConfig = {
            ...cfg,
            permission: { default: cfg.permission.default, rules: { ...cfg.permission.rules, [tool]: 'allow' } },
            updatedAt: localNow(),
          }
          saveLocalConfig(updated)
          refreshLocal()
          store.notifySuccess(`Agent rule written: rules["${tool}"] = allow`)
        },
      },
      session.messages,
      session.usage,
      () => {
        session!.updatedAt = localNow()
        saveLocalSession(session!)
        if (localJob.value) {
          localJob.value = {
            ...localJob.value,
            status: 'running',
            usage: { ...session!.usage },
            activity: agent.localActivity.value || null,
          }
        }
        setViewing({ ...session! })
      },
    )
  } catch (e) {
    // runLocalAgent is not supposed to throw (turn errors return {stopped:'error'}),
    // but a transport-level throw (e.g. missing wasm bundle) must not become an
    // unhandled rejection — surface it as a failed job with a house prefix.
    const detail = e instanceof Error ? e.message : String(e)
    outcome = { stopped: 'error', error: detail.includes(':') ? detail : `network: ${detail}` }
  }
  const finished: AgentJob = {
    ...(localJob.value ?? {
      jobId: newLocalId('job'),
      sessionId: session.id,
      configId: cfg.id,
      maxTurns: cfg.maxTurns,
      usage: { ...session.usage },
    }),
    status: outcome.stopped === 'done' || outcome.stopped === 'limit' ? 'done' : outcome.stopped === 'aborted' ? 'cancelled' : 'error',
    usage: { ...session.usage },
    result:
      outcome.stopped === 'limit'
        ? `turn budget exhausted (${cfg.maxTurns} turns) — raise MAX TURNS in the config or continue in a new session; transcript saved`
        : undefined,
    error: outcome.error,
  }
  session.updatedAt = localNow()
  saveLocalSession(session)
  refreshLocal()
  setViewing({ ...session })
  localJob.value = finished
  // The browser loop has no job poller, so it announces its own terminal
  // states — the same one-shot toasts `announceAgentJob` gives native runs.
  if (outcome.stopped === 'done') store.notifySuccess('Agent finished')
  else if (outcome.stopped === 'limit') {
    store.notifySuccess(
      `Turn budget exhausted (${cfg.maxTurns} turns) — raise MAX TURNS or continue in a new session`,
    )
  } else if (outcome.stopped === 'error') {
    store.notifyError('Agent run failed', outcome.error ?? 'unknown error')
  } else if (outcome.stopped === 'aborted') store.notifySuccess('Agent run cancelled')
}

async function abortJob() {
  if (wasmMode.value) {
    abortLocalRun()
    if (localJob.value) localJob.value = { ...localJob.value, status: 'cancelled' }
    return
  }
  const job = activeJob.value
  if (job) await store.abortAgentJob(job.jobId)
}

async function answerApproval(approved: boolean, remember = false) {
  // DENY carries the feedback typed above — the loop hands it to the model
  // as `denied: … — user feedback: …`, which it must obey, not retry.
  const feedback = !approved ? denyReason.value.trim() || undefined : answerInput.value || undefined
  if (wasmMode.value) {
    const pending = agent.pendingApproval.value
    agent.pendingApproval.value = null
    pending?.resolve(
      approved,
      feedback,
      approved && remember,
    )
    answerInput.value = ''
    denyReason.value = ''
    return
  }
  const job = activeJob.value
  if (!job) return
  await store.approveAgentJob(job.jobId, approved, feedback, remember)
  answerInput.value = ''
  denyReason.value = ''
  if (remember && approved) void store.fetchAgentConfigs()
}

async function initRepo() {
  if (!chatConfigId.value || jobActive.value) return
  if (wasmMode.value) {
    // No detached worker in the browser sandbox — run the same analysis as an
    // attached browser-loop turn instead of refusing. It lists/reads via the
    // volume tools and proposes AGENTS.md through the normal approval card.
    promptInput.value = 'Analyze this repository (list the top-level layout, read key configs) and draft AGENTS.md standing orders: build/test commands, conventions, and what an agent must never do. Write it to AGENTS.md only after my approval.'
    await sendPromptLocal()
    return
  }
  await store.initAgentRun(chatConfigId.value)
}

function roleLabel(m: { role: string; toolName?: string | null }) {
  if (m.role === 'user') return 'YOU'
  if (m.role === 'tool') return `TOOL: ${m.toolName ?? ''}`
  if (m.role === 'assistant_tool') return 'AGENT TOOL'
  return 'AGENT'
}

function toolNameOf(m: { toolInput?: unknown }) {
  try {
    const arr = (m.toolInput ?? []) as Array<{ name?: string }>
    return arr.map(c => c.name ?? '?').join(', ')
  } catch {
    return ''
  }
}

function prettyInput(m: { toolName?: string | null; toolInput?: unknown }) {
  try {
    if (m.toolName) return JSON.stringify(m.toolInput ?? {}, null, 1).slice(0, 800)
    const arr = (m.toolInput ?? []) as Array<{ name?: string; input?: unknown }>
    return arr.map(c => `${c.name ?? '?'} ${JSON.stringify(c.input ?? {}).slice(0, 300)}`).join('\n')
  } catch {
    return ''
  }
}

/** Terminal status: pull the final transcript, then drain the prompt queue. */
watch(
  () => activeJob.value?.status,
  (status) => {
    const terminal = status === 'done' || status === 'error' || status === 'cancelled'
    if (terminal && !wasmMode.value && viewing.value) {
      void store.loadAgentSession(viewing.value.id).then(s => {
        if (s && activeJob.value?.sessionId === viewing.value?.id) setViewing(s)
      })
      void store.fetchAgentSessions()
    }
    if (!terminal || jobActive.value || !queue.value.length) return
    const next = queue.value.shift()!
    promptInput.value = next
    void nextTick(() => {
      void sendPrompt()
    })
  },
)

/**
 * Native runs push nothing over SSE, so while a job is live the thread is
 * re-read on the same 1.5s cadence as the job poller — that is what turns a
 * frozen transcript into a live one.
 */
async function refreshLiveThread() {
  if (wasmMode.value) return
  const job = activeJob.value
  const v = viewing.value
  if (!job || !v || job.sessionId !== v.id) return
  const fresh = await store.loadAgentSession(v.id)
  if (fresh && activeJob.value?.sessionId === v.id && viewing.value?.id === v.id) setViewing(fresh)
}

watch(jobActive, active => {
  if (active) {
    if (!startedAt.value) startedAt.value = Date.now()
    if (!tickTimer) tickTimer = window.setInterval(() => { if (!document.hidden) nowTick.value = Date.now() }, 1000)
    if (!liveTimer) liveTimer = window.setInterval(() => { if (!document.hidden) void refreshLiveThread() }, 1500)
    return
  }
  if (tickTimer) { window.clearInterval(tickTimer); tickTimer = 0 }
  if (liveTimer) { window.clearInterval(liveTimer); liveTimer = 0 }
  void refreshLiveThread()
})

onBeforeUnmount(() => {
  stopVoice?.()
  agentMobileQuery?.removeEventListener?.('change', onAgentMobileChange)
  if (tickTimer) window.clearInterval(tickTimer)
  if (liveTimer) window.clearInterval(liveTimer)
  if (typeof window !== 'undefined') {
    window.removeEventListener(CHAT_CONFIG_EVENT, onSharedChatConfig)
    window.removeEventListener('storage', onSharedStorage)
    window.removeEventListener(AGENT_CONFIGS_EVENT, onSharedConfigs)
    window.removeEventListener(AGENT_KEYS_EVENT, onSharedKeys)
  }
})

/** Offline fallback when the wasm bundle (or its catalog) is unavailable —
 *  mirrors `crates/agent/src/providers.rs` so Setup still offers real
 *  endpoints instead of Custom-only. The loop then reports the missing
 *  engine per-turn instead of failing the whole panel. */
const FALLBACK_PRESETS: ProviderPreset[] = [
  { id: 'openrouter', label: 'OpenRouter', family: 'openrouter', baseUrl: 'https://openrouter.ai/api/v1', defaultModel: 'anthropic/claude-sonnet-4-5', dialect: 'openAi', auth: 'bearer', authName: null, keyEnv: 'OPENROUTER_API_KEY', keyless: false, extraHeaders: [['HTTP-Referer', 'https://cybermanju.github.io/'], ['X-Title', 'CyberManju OS']] },
  { id: 'openai', label: 'OpenAI', family: 'gpt', baseUrl: 'https://api.openai.com/v1', defaultModel: 'gpt-5', dialect: 'openAi', auth: 'bearer', authName: null, keyEnv: 'OPENAI_API_KEY', keyless: false, extraHeaders: [] },
  { id: 'anthropic', label: 'Anthropic', family: 'claude', baseUrl: 'https://api.anthropic.com', defaultModel: 'claude-sonnet-4-5', dialect: 'anthropic', auth: 'header', authName: 'x-api-key', keyEnv: 'ANTHROPIC_API_KEY', keyless: false, extraHeaders: [['anthropic-version', '2023-06-01']] },
  { id: 'ollama', label: 'Ollama (local)', family: 'ollama', baseUrl: 'http://localhost:11434/v1', defaultModel: 'llama3.1:8b', dialect: 'openAi', auth: 'none', authName: null, keyEnv: '', keyless: true, extraHeaders: [] },
  { id: 'google', label: 'Google Gemini', family: 'gemini', baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai/', defaultModel: 'gemini-2.5-flash', dialect: 'openAi', auth: 'bearer', authName: null, keyEnv: 'GEMINI_API_KEY', keyless: false, extraHeaders: [] },
  { id: 'groq', label: 'Groq', family: 'groq', baseUrl: 'https://api.groq.com/openai/v1', defaultModel: 'llama-3.3-70b-versatile', dialect: 'openAi', auth: 'bearer', authName: null, keyEnv: 'GROQ_API_KEY', keyless: false, extraHeaders: [] },
  { id: 'mistral', label: 'Mistral', family: 'mistral', baseUrl: 'https://api.mistral.ai/v1', defaultModel: 'mistral-large-latest', dialect: 'openAi', auth: 'bearer', authName: null, keyEnv: 'MISTRAL_API_KEY', keyless: false, extraHeaders: [] },
  { id: 'deepseek', label: 'DeepSeek', family: 'deepseek', baseUrl: 'https://api.deepseek.com/v1', defaultModel: 'deepseek-chat', dialect: 'openAi', auth: 'bearer', authName: null, keyEnv: 'DEEPSEEK_API_KEY', keyless: false, extraHeaders: [] },
  { id: 'xai', label: 'xAI Grok', family: 'grok', baseUrl: 'https://api.x.ai/v1', defaultModel: 'grok-4', dialect: 'openAi', auth: 'bearer', authName: null, keyEnv: 'XAI_API_KEY', keyless: false, extraHeaders: [] },
  { id: 'cerebras', label: 'Cerebras', family: 'cerebras', baseUrl: 'https://api.cerebras.ai/v1', defaultModel: 'llama-3.3-70b', dialect: 'openAi', auth: 'bearer', authName: null, keyEnv: 'CEREBRAS_API_KEY', keyless: false, extraHeaders: [] },
]

onMounted(async () => {
  agentMobileQuery?.addEventListener?.('change', onAgentMobileChange)
  agentMobileViewport.value = agentMobileQuery?.matches ?? false
  if (wasmMode.value) {
    try {
      const presets = (await agent.wasmAgentCatalog()) as ProviderPreset[]
      localPresets.value = presets.length ? presets : [...FALLBACK_PRESETS]
      if (!presets.length) setupMsg.value = 'Browser engine unavailable — provider list is offline fallback.'
    } catch (e) {
      localPresets.value = [...FALLBACK_PRESETS]
      setupMsg.value = `Browser engine unavailable (${e instanceof Error ? e.message : String(e)}) — provider list is offline fallback.`
    }
    refreshLocal()
    if (!form.model) {
      const medium = localPresets.value.find(p => p.id === 'openrouter')
      if (medium) pickPreset(medium)
    }
    return
  }
  await Promise.allSettled([store.fetchAgentProviders(), store.fetchAgentConfigs(), store.fetchAgentSessions()])
  if (!chatConfigId.value) {
    const shared = readChatConfigId()
    if (shared && store.agentConfigs.some(c => c.id === shared)) chatConfigId.value = shared
    else if (store.agentConfigs.length) chatConfigId.value = store.agentConfigs[0].id
  }
  if (!form.model) {
    const medium = providers.value.find(p => p.id === 'openrouter')
    if (medium) pickPreset(medium)
  }
})
</script>

<style scoped>
/* CyberManju agent — modern chat UX.
   Layout: header / (sidebar + main). Sidebar collapses under 760px.
   Tone: sentence case, breathing room, sticky composer, avatars, meters. */

.agent {
  width: 100%;
  height: 100%;
  container-type: inline-size;
  container-name: agent;
  display: flex;
  flex-direction: column;
  background: var(--ui-surface);
  color: var(--ui-text);
  font-family: var(--ui-font);
  overflow: hidden;
}

/* ── header: single quiet toolbar row ── */
.agent-header {
  display: flex;
  align-items: center;
  min-width: 0;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--ui-separator);
  background: var(--ui-surface);
  flex-shrink: 0;
}
.agent-nav-toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  border-radius: var(--ui-radius-sm);
  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent);
  background: transparent;
  color: var(--ui-text-3);
  cursor: pointer;
}
.agent-nav-toggle:hover { background: var(--ui-accent-softer); color: var(--ui-accent); }

.agent-avatar {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background: var(--ui-accent-softer);
  color: var(--ui-accent);
  border: 1px solid color-mix(in srgb, var(--ui-accent) 35%, transparent);
  flex-shrink: 0;
}
.agent-avatar-pulse {
  position: absolute;
  inset: -3px;
  border-radius: 50%;
  border: 2px solid color-mix(in srgb, var(--ui-accent) 55%, transparent);
  animation: agent-pulse 1.6s ease-in-out infinite;
}
@keyframes agent-pulse { 0%,100% { opacity: .2; transform: scale(.94);} 50% { opacity: 1; transform: scale(1.04);} }
.agent-title { min-width: 0; flex: 1; }
.agent-title h2 { margin: 0; font-size: 13px; font-weight: 600; }
.agent-subtitle { margin: 0; font-size: 11px; color: var(--ui-text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.agent-subtitle-sep { margin: 0 4px; opacity: .5; }
.agent-header-meta { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
.agent-sidebar-backdrop,
.agent-mobile-nav { display: none; }
.agent-tabs { display: flex; gap: 2px; padding: 2px;  border: 1px solid color-mix(in srgb, var(--ui-text) 9%, transparent); border-radius: var(--ui-radius-lg); background: color-mix(in srgb, var(--ui-glass) 60%, transparent); }
.agent-tabs button {
  display: inline-flex; align-items: center; gap: 5px;
  border: 0; background: transparent; color: color-mix(in srgb, var(--ui-text) 60%, transparent);
  font: inherit; font-size: 11px; font-weight: 650;
  padding: 5px 9px; border-radius: var(--ui-radius-sm); cursor: pointer;
}
.agent-tabs button:hover { color: var(--ui-text); background: color-mix(in srgb, var(--ui-text) 7%, transparent); }
.agent-tabs button.on { color: var(--ui-accent); background: var(--ui-accent-softer); }

/* ── body ── */
.agent-body { flex: 1; display: flex; min-height: 0; min-width: 0; overflow: hidden; }
.agent-sidebar {
  width: 264px; flex-shrink: 0;
  border-right: 1px solid var(--ui-border);
  overflow-y: auto; overflow-x: hidden; padding: 12px;
  display: flex; flex-direction: column; gap: 14px;
  background: color-mix(in srgb, var(--ui-glass) 40%, transparent);
  min-width: 0;
}
.agent--sidebar-closed .agent-sidebar { display: none; }
.agent-side-section { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.agent-side-head { display: flex; align-items: center; justify-content: space-between; gap: 6px; }
.agent-side-head h3 { margin: 0; font-size: 11px; font-weight: 600; color: var(--ui-text-3); display: flex; align-items: center; gap: 6px; min-width: 0; overflow-wrap: anywhere; }
.agent-side-actions { display: flex; gap: 4px; flex-shrink: 0; }
.agent-side-list { display: flex; flex-direction: column; gap: 4px; max-height: 42%; overflow-y: auto; overflow-x: hidden; min-height: 0; min-width: 0; }
.agent-side-item {
  display: flex; align-items: flex-start; gap: 8px;
  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: var(--ui-radius-lg);  padding: 6px 8px; cursor: pointer; background: transparent;
  color: var(--ui-text); font: inherit; text-align: left; width: 100%;
  min-width: 0; overflow: hidden;
}
.agent-side-item:hover { background: color-mix(in srgb, var(--ui-text) 6%, transparent); }
.agent-side-item:hover .agent-side-item-btns { opacity: 1; }
.agent-side-item.on { background: var(--ui-accent-soft); }
.agent-side-item.on .agent-side-item-btns { opacity: 1; }
.agent-side-item-text { flex: 1 1 auto; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.agent-side-item-title { font-size: 12px; font-weight: 500; min-width: 0; white-space: normal; overflow-wrap: anywhere; word-break: break-word; line-height: 1.35; }
.agent-side-item-meta { font-size: 10px; color: var(--ui-text-3); min-width: 0; white-space: normal; overflow-wrap: anywhere; word-break: break-all; line-height: 1.35; }
.agent-side-item-badge { flex-shrink: 0; margin-top: 1px; }
.agent-side-item-btns { display: flex; gap: 2px; margin-left: auto; flex-shrink: 0; opacity: 0; transition: opacity var(--ui-dur-fast) var(--ui-ease-out); }
.agent-side-item-btns:focus-within { opacity: 1; }
.agent-side-row { display: flex; gap: 8px; }
.agent-side-row > * { flex: 1; min-width: 0; }

.agent-main { flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }

/* context line: one quiet row, details expand below */
.agent-caps {
  padding: 0 12px;  border-bottom: 1px solid color-mix(in srgb, var(--ui-text) 8%, transparent);
  background: color-mix(in srgb, var(--ui-glass) 70%, transparent);
  flex-shrink: 0;
}
.agent-caps.open { padding-bottom: 8px; }
.agent-caps-summary {
  display: flex; align-items: center; gap: 8px; width: 100%;
  border: 0; background: none; color: var(--ui-text-3);
  font: inherit; font-size: 11px; padding: 6px 0; cursor: pointer; text-align: left;
}
.agent-caps-summary:hover { color: var(--ui-text-2); }
.agent-caps-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--ui-success); flex-shrink: 0; }
.agent-caps-dot.warn { background: var(--ui-warning); }
.agent-caps-text { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.agent-caps-key { font-weight: 600; color: var(--ui-warning); }
.agent-caps-detail { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding-top: 2px; }
.agent-cap {
  display: inline-flex; align-items: center; gap: 5px;
  font-size: 11px; font-weight: 600;
  border: 1px solid color-mix(in srgb, var(--ui-text) 12%, transparent); border-radius: var(--ui-radius-xl);  padding: 3px 9px; background: color-mix(in srgb, var(--ui-glass) 60%, transparent);
}
.agent-cap.warn { border-color: color-mix(in srgb, var(--ui-warning) 55%, transparent); color: var(--ui-warning); }
.agent-cap-link { border: 0; background: none; color: var(--ui-accent); font: inherit; font-size: 11px; font-weight: 650; cursor: pointer; padding: 2px 4px; }
.agent-cap-tools { display: flex; flex-wrap: wrap; gap: 4px; flex-basis: 100%; }
.agent-cap-tool {
  display: inline-flex; align-items: center; gap: 4px;
  font-size: 10px; font-weight: 650;
  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: var(--ui-radius-lg);
  padding: 2px 6px; color: color-mix(in srgb, var(--ui-text) 75%, transparent);
}
.agent-cap-tool.act-allow { border-color: color-mix(in srgb, var(--ui-success) 45%, transparent); }
.agent-cap-tool.act-ask { border-color: color-mix(in srgb, var(--ui-warning) 50%, transparent); }
.agent-cap-tool.act-deny { border-color: color-mix(in srgb, var(--ui-danger) 45%, transparent); }

/* cards (setup / controls) */
.agent-card {
  margin: 10px 12px; border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: var(--ui-radius-xl); padding: 12px 14px; background: color-mix(in srgb, var(--ui-glass) 68%, transparent);}
.agent-card-title { font-size: 12px; font-weight: 600; margin: 12px 0 8px; display: flex; align-items: center; gap: 6px; color: var(--ui-text); }
.agent-card-title:first-child { margin-top: 0; }
/* accordion toggle for Controls sections */
.agent-card-toggle {
  display: flex; align-items: center; gap: 8px; width: 100%;
  border: 0; background: none; color: var(--ui-text);
  font: inherit; font-size: 12px; font-weight: 600;
  padding: 2px 0; cursor: pointer; text-align: left;
}
.agent-card-toggle:hover { color: var(--ui-accent); }
.agent-card-toggle > :last-child { margin-left: auto; color: var(--ui-text-3); }
.agent-card-fold { padding-top: 8px; }
.preset-search { margin-bottom: 8px; }
/* Setup tab is a direct child of the fixed-height main column: it must own
   its scroll instead of growing past the window (out-of-screen content). */
.agent-main > .agent-card {
  flex: 1;
  min-height: 0;
  min-width: 0;
  overflow-y: auto;
  overflow-x: hidden;
  overscroll-behavior: contain;
  touch-action: pan-x pan-y;
  scrollbar-gutter: stable;
}
.agent-controls {
  flex: 1;
  min-height: 0;
  min-width: 0;
  overflow-y: auto;
  overflow-x: hidden;
  overscroll-behavior: contain;
  touch-action: pan-x pan-y;
  scrollbar-gutter: stable;
  padding-bottom: 12px;
}
.agent-note { font-size: 11px; color: color-mix(in srgb, var(--ui-text) 55%, transparent);  margin: 8px 0 0; }

  .agent-link { border: 0; background: none; padding: 0; color: var(--ui-accent); font: inherit; font-weight: 650; cursor: pointer; }
.mono { font-family: var(--ui-font-mono);  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); padding: 0 4px; border-radius: var(--ui-radius-sm); font-size: .95em; }
.dim { color: color-mix(in srgb, var(--ui-text) 50%, transparent); }

/* legacy form helpers kept (script unchanged) */
.preset-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 6px; margin-bottom: 10px; max-height: 240px; overflow-y: auto; padding-right: 2px; }
.preset-card {
  background: color-mix(in srgb, var(--ui-glass) 60%, transparent);
  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: var(--ui-radius-lg);
  color: var(--ui-text); padding: 8px 10px; text-align: left; cursor: pointer; font-family: inherit;
  display: flex; flex-direction: column; gap: 2px;
}
.preset-card:hover { border-color: var(--ui-border-hover); background: var(--ui-glass-2); }
.preset-card.on { border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent); background: var(--ui-accent-softer); }
.preset-name { font-size: 12px; font-weight: 700; }
.preset-meta { font-size: 10px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.preset-free { font-size: 10px; color: var(--ui-accent); font-weight: 700; }
.w-field { display: flex; flex-direction: column; gap: 4px; margin-bottom: 8px; min-width: 0; flex: 1; }
.w-field.grow { flex: 1; min-width: 140px; }
.w-field-narrow { flex: 0 0 110px; }
.w-field.check-field { justify-content: flex-end; padding-bottom: 6px; flex: 0 0 auto; }
.w-label { font-size: 11px; font-weight: 650; color: color-mix(in srgb, var(--ui-text) 65%, transparent); }
.w-row { display: flex; gap: 8px; margin-bottom: 8px; flex-wrap: wrap; align-items: flex-end; }
.w-actions { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
.w-msg { font-size: 11px; margin-top: 6px; color: color-mix(in srgb, var(--ui-text) 70%, transparent); }
.w-msg.err { color: var(--ui-danger); }
.model-row { display: flex; gap: 6px; }
.config-list { display: flex; flex-direction: column; gap: 6px; margin-bottom: 8px; }
.config-card {  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: var(--ui-radius-xl); padding: 8px 10px; background: color-mix(in srgb, var(--ui-glass) 68%, transparent); }
.cfg-header { display: flex; align-items: center; gap: 8px; margin-bottom: 4px; flex-wrap: wrap; }
.cfg-name { font-size: 12px; font-weight: 700; flex: 1; }
.cfg-type { font-size: 10px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.cfg-meta { font-size: 10px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); margin-bottom: 4px; word-break: break-all; }
.cfg-actions { display: flex; gap: 6px; margin-top: 6px; }
.remote-list { margin-top: 6px; }
.remote-row { display: flex; justify-content: space-between; gap: 8px; font-size: 11px; border-bottom: 1px solid var(--ui-hairline); padding: 3px 0; }
.perm-grid { display: flex; flex-direction: column; gap: 4px; margin-bottom: 8px; }
.perm-row { display: flex; align-items: center; gap: 8px; font-size: 11px; }
.perm-row .ct-name { flex: 1; font-weight: 600; }

/* chat */
.agent-chat { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
.agent-welcome { margin: auto; max-width: 560px; width: 100%; box-sizing: border-box; max-height: 100%; min-height: 0; overflow-y: auto; overflow-x: hidden; padding: 24px; text-align: center; }
.agent-welcome-avatar {
  display: inline-flex; align-items: center; justify-content: center;
  width: 52px; height: 52px; border-radius: 50%;
  background: var(--ui-accent-softer); color: var(--ui-accent);
  border: 1px solid color-mix(in srgb, var(--ui-accent) 35%, transparent);
  margin-bottom: 10px;
}
.agent-welcome h3 { margin: 0 0 6px; font-size: 16px; }
.agent-welcome p { font-size: 12px; color: color-mix(in srgb, var(--ui-text) 65%, transparent); margin: 0 0 12px; }
.agent-quick { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 8px; text-align: left; }
.agent-quick-card {
  display: flex; flex-direction: column; gap: 4px; align-items: flex-start;
  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: var(--ui-radius-xl);
  background: color-mix(in srgb, var(--ui-glass) 55%, transparent);
  color: var(--ui-text); font: inherit; padding: 10px 12px; cursor: pointer;
}
.agent-quick-card:hover:not(:disabled) { border-color: color-mix(in srgb, var(--ui-accent) 45%, transparent); }
.agent-quick-card:disabled { opacity: .5; cursor: not-allowed; }
.agent-quick-label { font-size: 12px; font-weight: 700; }
.agent-quick-hint { font-size: 10px; color: color-mix(in srgb, var(--ui-text) 50%, transparent); }

.agent-thread-head { display: flex; align-items: center; gap: 8px; padding: 8px 12px 6px; flex-shrink: 0; min-width: 0; }
.agent-thread-title { display: flex; align-items: baseline; gap: 8px; min-width: 0; flex: 1; }
.agent-thread-title strong { font-size: 13px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.agent-thread-title .dim { font-size: 10px; white-space: nowrap; }
.meter { display: inline-flex; align-items: center; gap: 6px; flex-shrink: 0; }
.meter-label { font-size: 10px; color: var(--ui-text-3); font-variant-numeric: tabular-nums; }
.ctx-bar { display: inline-block; width: 72px; height: 5px; border-radius: 3px; background: color-mix(in srgb, var(--ui-text) 14%, transparent); overflow: hidden; }
.ctx-fill { display: block; height: 100%; background: var(--ui-accent); transition: width var(--ui-dur) var(--ui-ease-out); }
.ctx-fill.warn { background: var(--ui-warning); }
.ctx-fill.bad { background: var(--ui-danger); }

.agent-messages { flex: 1; min-height: 0; min-width: 0; overflow-y: auto; overflow-x: hidden; display: flex; flex-direction: column; gap: 10px; padding: 6px 14px 12px; overscroll-behavior: contain; touch-action: pan-x pan-y; scrollbar-gutter: stable; }
.agent-thread-empty { text-align: center; font-size: 12px; color: color-mix(in srgb, var(--ui-text) 55%, transparent); padding: 18px; }
.msg { display: flex; gap: 8px; }
.msg-avatar {
  display: inline-flex; align-items: center; justify-content: center;
  width: 26px; height: 26px; border-radius: 50%; flex-shrink: 0; margin-top: 2px;
  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent);
}
.avatar-user { background: color-mix(in srgb, var(--ui-accent) 14%, transparent); color: var(--ui-accent); }
.avatar-assistant { background: var(--ui-accent-softer); color: var(--ui-accent); }
.avatar-tool, .avatar-assistant_tool { background: color-mix(in srgb, var(--ui-text) 8%, transparent); color: color-mix(in srgb, var(--ui-text) 65%, transparent); }
.msg-main {
  flex: 1; min-width: 0;
  border: 1px solid var(--ui-border); border-radius: var(--ui-radius-lg);
  padding: 8px 10px; background: var(--ui-surface-2);
}
/* Flat chat: user = accent fill, assistant = solid surface-2. */
.role-user .msg-main { background: var(--ui-accent); border-color: transparent; color: var(--ui-on-accent); }
.role-user .msg-role, .role-user .msg-copy { color: var(--ui-on-accent); }

/* Plasma: user = accent 20% tint on surface-2 (assistant stays surface-2). */
[data-ui-shell='plasma'] .role-user .msg-main {
  background: color-mix(in srgb, var(--ui-accent) 20%, var(--ui-surface-2));
  border-color: color-mix(in srgb, var(--ui-accent) 40%, transparent);
  color: var(--ui-text);
}
[data-ui-shell='plasma'] .role-user .msg-role,
[data-ui-shell='plasma'] .role-user .msg-copy { color: var(--ui-text-2); }
.role-tool .msg-main { background: color-mix(in srgb, var(--ui-text) 5%, transparent); }
.msg-role { font-size: 11px; font-weight: 600; color: var(--ui-text-3); margin-bottom: 4px; display: flex; align-items: center; gap: 6px; }
.msg-copy { border: 0; background: none; color: inherit; opacity: .55; cursor: pointer; padding: 2px; display: inline-flex; }
.msg-copy { min-width: 44px; min-height: 44px; align-items: center; justify-content: center; margin: -10px -8px -10px -8px; }
.msg-copy:hover { opacity: 1; }
.msg-body { font-size: 12px; line-height: 1.55; white-space: pre-wrap; overflow-wrap: anywhere; word-break: break-word; min-width: 0; }
.msg-body.md { white-space: normal; }
.msg-body.md :deep(p) { margin: 0 0 6px; }
.msg-body.md :deep(p:last-child) { margin-bottom: 0; }
.msg-body.md :deep(h2), .msg-body.md :deep(h3), .msg-body.md :deep(h4) { font-size: 12px; font-weight: 800; margin: 8px 0 4px; }
.msg-body.md :deep(pre) {
  margin: 6px 0; padding: 8px; background: color-mix(in srgb, var(--ui-glass) 72%, transparent);
  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: var(--ui-radius-sm);
  overflow-x: hidden; white-space: pre-wrap; overflow-wrap: anywhere; word-break: break-word; font-size: 10px;
}
.msg-body.md :deep(code) { font-family: var(--ui-font-mono); font-size: .92em; }
.msg-body.md :deep(ul), .msg-body.md :deep(ol) { margin: 4px 0; padding-left: 16px; }
.msg-body.md :deep(blockquote) { margin: 4px 0; padding-left: 8px; border-left: 2px solid var(--ui-border); color: color-mix(in srgb, var(--ui-text) 65%, transparent); }
.msg-body.md :deep(a) { color: var(--ui-accent); }
.turn-footer { font-size: 10px; color: color-mix(in srgb, var(--ui-text) 50%, transparent); margin-top: 6px; padding-top: 5px; border-top: 1px dashed var(--ui-hairline); }
.tool-block { margin-top: 4px; }
.tool-name { font-size: 11px; font-weight: 700; }
.tool-input { font-size: 10px; color: color-mix(in srgb, var(--ui-text) 70%, transparent); white-space: pre-wrap; overflow-wrap: anywhere; word-break: break-word; margin: 4px 0 0; font-family: var(--ui-font-mono); min-width: 0; }

/* tool timeline */
.tool-group { display: flex; flex-direction: column; gap: 4px; margin-left: 34px; }
.tool-group .lead { padding: 6px 8px; }
.tool-row { border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: var(--ui-radius-lg); background: color-mix(in srgb, var(--ui-glass) 45%, transparent); overflow: hidden; }
.tool-row.is-running { border-color: color-mix(in srgb, var(--ui-accent) 50%, transparent); }
.tool-row.is-denied { border-color: color-mix(in srgb, var(--ui-danger) 50%, transparent); }
.tool-row.is-error { border-color: color-mix(in srgb, var(--ui-warning) 50%, transparent); }
.tool-head { width: 100%; min-height: 44px; display: flex; align-items: center; gap: 7px; padding: 6px 8px; background: none; border: 0; color: var(--ui-text); font-family: inherit; font-size: 11px; cursor: pointer; text-align: left; }
.tool-head:hover { background: color-mix(in srgb, var(--ui-text) 6%, transparent); }
.tool-title { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
.tool-chev { color: color-mix(in srgb, var(--ui-text) 45%, transparent); transform: rotate(90deg); transition: transform var(--ui-dur-fast) var(--ui-ease-out); }
.tool-row.open .tool-chev { transform: rotate(-90deg); }
.tool-detail { border-top: 1px solid var(--ui-hairline); padding: 6px 8px; }
.tool-result { font-size: 10px; white-space: pre-wrap; overflow-wrap: anywhere; word-break: break-word; margin: 6px 0 0; color: color-mix(in srgb, var(--ui-text) 75%, transparent); font-family: var(--ui-font-mono); max-height: 240px; overflow: auto; }
.tool-result.bad { color: var(--ui-danger); }
.tool-wait { font-size: 10px; margin-top: 6px; }

/* approval */
.approval {
  flex-shrink: 0; min-width: 0; max-height: 45%; overflow-y: auto; overflow-x: hidden;
  margin: 0 14px 10px; border: 1px solid color-mix(in srgb, var(--ui-warning) 45%, transparent);
  border-radius: var(--ui-radius-md); padding: 10px 12px;
  background: color-mix(in srgb, var(--ui-warning) 8%, transparent);
}
.approval-title { font-size: 12px; font-weight: 750; color: var(--ui-warning); margin-bottom: 4px; display: flex; align-items: center; gap: 6px; }
.approval-text { font-size: 12px; margin-bottom: 8px; word-break: break-word; }
.approval-meta { display: flex; flex-direction: column; gap: 3px; font-size: 11px; margin-bottom: 6px; }
.approval-meta > span { display: flex; flex-wrap: wrap; gap: 6px; align-items: baseline; }
.approval-input { max-height: 96px; overflow: auto; margin-bottom: 8px; }
.approval-diff { margin-bottom: 8px; }
.diff-head { font-size: 10px; font-weight: 700; margin-bottom: 4px; }
.diff-body { font-size: 10px; white-space: pre-wrap; word-break: break-word; margin: 0; max-height: 220px; overflow: auto; border: 1px solid color-mix(in srgb, var(--ui-text) 9%, transparent); border-radius: var(--ui-radius-lg); padding: 6px 8px; background: color-mix(in srgb, var(--ui-glass) 72%, transparent); }
.diff-line { display: block; }
.diff-del { color: var(--ui-danger); background: color-mix(in srgb, var(--ui-danger) 8%, transparent); }
.diff-add { color: var(--ui-success); background: color-mix(in srgb, var(--ui-success) 8%, transparent); }
.diff-ctx { opacity: .75; }
.approval.attention { box-shadow: var(--ui-shadow-2); }

/* queue */
.queue { display: flex; flex-direction: column; gap: 4px; margin: 0 14px 8px; flex-shrink: 0; min-width: 0; max-height: 25%; overflow-y: auto; overflow-x: hidden; }
.queue-item { display: flex; align-items: center; gap: 6px; font-size: 11px; border: 1px dashed var(--ui-border); border-radius: var(--ui-radius-sm); padding: 4px 8px; }
.queue-text { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.job-line { display: flex; align-items: center; gap: 5px; margin: 0 14px; flex-shrink: 0; min-width: 0; }

/* composer */
.composer {
  flex-shrink: 0; min-width: 0;
  margin: 0 14px calc(8px + env(safe-area-inset-bottom, 0px)); border: 1px solid color-mix(in srgb, var(--ui-border-strong));
  border-radius: var(--ui-radius-lg); background: color-mix(in srgb, var(--ui-glass) 70%, transparent);
  transition: border-color var(--ui-dur) var(--ui-ease-out), box-shadow var(--ui-dur) var(--ui-ease-out);
  position: relative;
}
.composer.focused { border-color: color-mix(in srgb, var(--ui-accent) 70%, transparent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--ui-accent) 15%, transparent); }
.composer-box {
  width: 100%; border: 0; background: transparent; color: var(--ui-text);
  font-family: var(--ui-font); font-size: 13px; line-height: 1.5;
  padding: 10px 12px 4px; outline: none; resize: vertical; min-height: 52px;
  max-height: min(40vh, 240px); overflow-y: auto; box-sizing: border-box; overflow-wrap: anywhere;
}
.composer-bar { display: flex; align-items: center; gap: 8px; padding: 4px 8px 8px; }
/* Dictation language: hidden picker that pops over the Dictate button on
   hover/focus. Default is Auto (neither EN nor PT picked) → browser-detected. */
.voice-group { position: relative; display: inline-flex; flex-shrink: 0; }
.voice-pop {
  position: absolute; bottom: calc(100% + 6px); right: 0;
  display: flex; gap: 4px; padding: 4px;
  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: var(--ui-radius-lg);
  background: color-mix(in srgb, var(--ui-glass) 92%, transparent);
  box-shadow: var(--ui-shadow-2); z-index: 6; white-space: nowrap;
  opacity: 0; visibility: hidden; transform: translateY(4px); pointer-events: none;
  transition: opacity var(--ui-dur-fast) var(--ui-ease-out), transform var(--ui-dur-fast) var(--ui-ease-out), visibility var(--ui-dur-fast);
}
.voice-group:hover .voice-pop, .voice-group:focus-within .voice-pop { opacity: 1; visibility: visible; transform: none; pointer-events: auto; }
.voice-opt {
  font: inherit; font-size: 11px; font-weight: 700; padding: 4px 8px; border-radius: 8px;
  border: 1px solid transparent; background: transparent; color: color-mix(in srgb, var(--ui-text) 70%, transparent);
  cursor: pointer; white-space: nowrap;
}
.voice-opt:hover { color: var(--ui-text); background: color-mix(in srgb, var(--ui-text) 7%, transparent); }
.voice-opt.on { color: var(--ui-accent); background: var(--ui-accent-softer); border-color: color-mix(in srgb, var(--ui-accent) 40%, transparent); }
.composer-hint { font-size: 10px; }
.composer-spacer { flex: 1; }
.composer-slash {
  position: absolute; bottom: 100%; left: 0; right: 0; margin-bottom: 6px;
  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: var(--ui-radius-xl); background: color-mix(in srgb, var(--ui-glass) 68%, transparent); box-shadow: var(--ui-shadow-2); overflow: hidden; z-index: 5;
}
.composer-slash-item {
  display: flex; gap: 10px; align-items: baseline; width: 100%;
  border: 0; background: none; color: var(--ui-text); font: inherit; font-size: 12px;
  padding: 8px 12px; cursor: pointer; text-align: left;
}
.composer-slash-item:hover { background: var(--ui-accent-softer); }

/* responsive: sidebar becomes overlay-friendly single column */
@media (max-width: 760px) {
  .agent-header { flex-wrap: nowrap; }
  .agent-header-meta { display: none; }
  .agent-sidebar { position: absolute; inset: 0 auto 0 0; z-index: 10; height: 100%; width: min(88vw, 360px); max-width: calc(100% - 20px); background: var(--ui-surface-2); box-shadow: var(--ui-shadow-2), inset 0 1px 0 var(--ui-glass-highlight); }
  .agent-body { position: relative; }
  .agent-main { position: relative; z-index: 1; }
  .agent-sidebar-backdrop { display: block; position: absolute; inset: 0; z-index: 8; width: 100%; height: 100%; padding: 0; border: 0; border-radius: 0; background: rgb(8 12 20 / 42%); }
  .agent-mobile-nav { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 6px; padding-bottom: 10px; border-bottom: 1px solid var(--ui-border); }
  .agent-mobile-nav button { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 4px; min-width: 0; min-height: 56px; padding: 6px 4px; border: 1px solid var(--ui-border); border-radius: 12px; background: transparent; color: var(--ui-text-2); font: inherit; font-size: 10px; font-weight: 650; text-align: center; }
  .agent-mobile-nav button.on { border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent); background: var(--ui-accent-softer); color: var(--ui-accent); }
  .tool-group { margin-left: 0; }
  .agent-side-item-btns { opacity: 1; }
}

/* narrow phones: tighter chrome, wrapping thread head, full-width composer */
@media (max-width: 560px) {
  .agent-header { padding: 6px 8px; gap: 6px; }
  .agent-title h2 { font-size: 12px; }
  .agent-subtitle { font-size: 10px; }
  .agent-caps { padding: 0 8px; }
  .agent-thread-head { flex-wrap: wrap; padding: 6px 8px 4px; }
  .agent-messages { padding: 4px 8px 8px; }
  .composer { margin: 0 8px 6px; }
  .composer-box { font-size: 16px; }
  .agent-card { margin: 8px; padding: 10px; }
  .preset-grid { grid-template-columns: repeat(auto-fill, minmax(120px, 1fr)); max-height: 200px; }
  .w-row { flex-direction: column; align-items: stretch; }
  .approval { margin: 0 8px 8px; max-height: 55%; }
  .queue { margin: 0 8px 6px; }
}

/* Mirror the phone layout when this app is placed in a narrow desktop window. */
@container agent (max-width: 760px) {
  .agent-header { flex-wrap: nowrap; }
  .agent-header-meta { display: none; }
  .agent-body { position: relative; }
  .agent-main { position: relative; z-index: 1; }
  .agent-sidebar { position: absolute; inset: 0 auto 0 0; z-index: 10; width: min(88cqi, 360px); max-width: calc(100% - 20px); background: var(--ui-surface-2); box-shadow: var(--ui-shadow-2); }
  .agent-sidebar-backdrop { display: block; position: absolute; inset: 0; z-index: 8; width: 100%; height: 100%; padding: 0; border: 0; border-radius: 0; background: rgb(8 12 20 / 42%); }
  .agent-mobile-nav { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 6px; padding-bottom: 10px; border-bottom: 1px solid var(--ui-border); }
  .agent-mobile-nav button { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 4px; min-width: 0; min-height: 56px; padding: 6px 4px; border: 1px solid var(--ui-border); border-radius: 12px; background: transparent; color: var(--ui-text-2); font: inherit; font-size: 10px; font-weight: 650; text-align: center; }
  .agent-mobile-nav button.on { border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent); background: var(--ui-accent-softer); color: var(--ui-accent); }
}

@media (prefers-reduced-motion: reduce) {
  .agent-avatar-pulse { animation: none; }
  .agent, .agent * { scroll-behavior: auto !important; transition-duration: 0.01ms !important; animation-duration: 0.01ms !important; animation-iteration-count: 1 !important; }
}

</style>
