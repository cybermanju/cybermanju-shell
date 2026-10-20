<!-- CyberManju OS — CodeStudio (src/components/CodeStudio.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div ref="rootRef" class="cs" tabindex="0" @keydown="onGlobalKey">
    <div class="cs-aurora" aria-hidden="true" />

    <!-- ══ TOP: window-ish toolbar ══ -->
    <header class="cs-top">
      <div class="cs-logo"><AppIcon name="solar:file-code-bold" :size="14" /><b>STUDIO</b></div>
      <div class="cs-mobile-file" :title="focusTab?.path || focusTab?.label || 'No file open'">
        <b>{{ focusTab?.label || 'Choose a file' }}</b>
        <span v-if="focusTab?.dirty" aria-label="Unsaved changes">●</span>
      </div>
      <span class="cs-transport">{{ transportLabel }}</span>
      <span v-if="focusTab?.parse" class="cs-engine" :class="{ ts: focusTab.parse.engine === 'tree-sitter' }" :title="focusTab.parse.engine === 'tree-sitter' ? 'Real grammar parse' : 'Heuristic fallback'">
        {{ (focusTab.parse.engine || 'heuristic').toUpperCase() }}
      </span>
      <span class="cs-spacer" />
      <button class="cs-btn" :disabled="!focusTab || saving" title="Save (Ctrl+S)" @click="saveTab(focusTab)"><AppIcon name="solar:diskette-bold" :size="13" /> <span class="cs-btn-label">{{ saving ? 'Saving…' : 'Save' }}</span></button>
      <button class="cs-btn" :disabled="!focusTab" title="Split editor right" @click="splitRight"><AppIcon name="solar:columns-3-bold" :size="13" /> Split</button>
      <div v-if="voice.isSupported.value" class="cs-voice-group">
        <div class="cs-voice-pop" role="group" aria-label="Voice-code language">
          <button type="button" class="cs-voice-opt" :class="{ on: voice.langPref.value === 'auto' }" :aria-pressed="voice.langPref.value === 'auto'" title="Auto-detect from your browser language" @click="voice.setLang('auto')">Auto · {{ voice.lang.value === 'pt-BR' ? 'PT' : 'EN' }}</button>
          <button type="button" class="cs-voice-opt" :class="{ on: voice.langPref.value === 'en-US' }" :aria-pressed="voice.langPref.value === 'en-US'" title="Dictate code in English" @click="voice.setLang('en-US')">EN</button>
          <button type="button" class="cs-voice-opt" :class="{ on: voice.langPref.value === 'pt-BR' }" :aria-pressed="voice.langPref.value === 'pt-BR'" title="Dictate code in Portuguese (PT-BR)" @click="voice.setLang('pt-BR')">PT</button>
        </div>
        <button class="cs-btn" :class="{ on: voiceCoding }" :disabled="!focusTab" type="button" :title="voiceCoding ? `Listening… ${voice.interim.value || 'speak code'}` : 'Voice-code: dictate code at the cursor (“open paren”, “camel case …”, “nova linha”) — parsed live. Hover for EN/PT.'" @click="toggleVoiceCode"><AppIcon name="solar:microphone-bold" :size="13" /> {{ voiceCoding ? 'Stop' : 'Voice' }}</button>
      </div>
      <button class="cs-btn" :class="{ on: aiOpen }" title="Toggle AI panel (Ctrl+G)" @click="isMobileLayout ? toggleMobileAgent() : aiOpen = !aiOpen"><AppIcon name="solar:bot-bold" :size="13" /> <span class="cs-btn-label">AI</span></button>
      <button class="cs-btn" :class="{ on: bottomOpen }" title="Toggle panel (Ctrl+`)" @click="bottomOpen = !bottomOpen"><AppIcon name="solar:file-terminal-bold" :size="13" /> Panel</button>
    </header>

    <nav v-if="isMobileLayout" class="cs-mobilebar" aria-label="Editor navigation">
      <button v-for="a in activities" :key="a.id" type="button" :class="{ active: mobileExplorerOpen && sideView === a.id }" :aria-label="a.label" @click="openMobileSideView(a.id)">
        <AppIcon :name="a.icon" :size="17" /><span>{{ a.id }}</span>
      </button>
      <button type="button" :class="{ active: aiOpen }" aria-label="AI agent" @click="toggleMobileAgent"><AppIcon name="solar:bot-bold" :size="17" /><span>Agent</span></button>
      <button type="button" :class="{ active: bottomOpen }" aria-label="Terminal" @click="toggleMobileTerminal"><AppIcon name="solar:file-terminal-bold" :size="17" /><span>Terminal</span></button>
    </nav>

    <div class="cs-mid">
      <!-- ══ ACTIVITY BAR ══ -->
      <nav class="cs-activity" aria-label="Views">
        <button v-for="a in activities" :key="a.id" :class="{ active: sideView === a.id }" :title="a.label" @click="sideView = sideView === a.id ? '' : a.id">
          <AppIcon :name="a.icon" :size="17" />
        </button>
        <span class="cs-spacer" />
        <button :class="{ active: false }" title="Full agent panel" @click="wm.open('agent')"><AppIcon name="solar:chat-square-bold" :size="17" /></button>
        <button title="Settings" @click="wm.open('settings')"><AppIcon name="solar:settings-bold" :size="17" /></button>
      </nav>

      <!-- ══ SIDEBAR ══ -->
      <aside v-if="sideView" class="cs-side" :class="{ 'is-mobile-drawer': isMobileLayout, 'is-mobile-open': mobileExplorerOpen }" :aria-label="`${sideView} panel`" :aria-modal="isMobileLayout && mobileExplorerOpen ? 'true' : undefined" :role="isMobileLayout && mobileExplorerOpen ? 'dialog' : 'complementary'">
        <!-- FILES -->
        <template v-if="sideView === 'files'">
          <div class="cs-side-h">EXPLORER
            <span class="cs-spacer" />
            <button class="cs-ibtn" title="Refresh" @click="refreshExplorer"><AppIcon name="solar:refresh-bold" :size="12" /></button>
            <button v-if="isWasm" class="cs-ibtn" title="New file here" @click="wasmNewShow = !wasmNewShow"><AppIcon name="solar:add-bold" :size="12" /></button>
            <button class="cs-ibtn cs-drawer-close" title="Close panel" aria-label="Close panel" @click="sideView = ''"><AppIcon name="solar:close-bold" :size="12" /></button>
          </div>
          <input v-model="explorerFilter" class="cs-input" placeholder="Filter files…" spellcheck="false" />
          <div v-if="isWasm" class="cs-pathrow">
            <button class="cs-ibtn" :disabled="wasmCwd === '/'" title="Up" @click="wasmUp"><AppIcon name="solar:arrow-up-bold" :size="12" /></button>
            <span class="mono dim truncate">{{ wasmCwd }}</span>
          </div>
          <div v-if="isWasm && wasmNewShow" class="cs-pathrow">
            <input v-model="newFileName" class="cs-input" placeholder="new-file.txt" spellcheck="false" @keyup.enter="createWasmFile" />
            <button class="cs-btn xs" @click="createWasmFile">Create</button>
          </div>
          <div class="cs-tree">
            <template v-if="!isWasm">
              <TreeRow v-for="n in treeRoots" :key="n.id" :node="n" :depth="0" :active-key="focusTab?.key" @open="openNode" />
              <p v-if="!treeRoots.length" class="dim cs-empty">No editable text files (≤1 MiB, unencrypted).</p>
            </template>
            <template v-else>
              <button v-for="e in wasmEntriesFiltered" :key="e.key" class="cs-trow" :class="{ active: e.key === focusTab?.key, dir: e.isDir }" @click="e.isDir ? navigateWasm(e.path) : openWasmPath(e.path, e.label)" @dblclick="e.isDir ? navigateWasm(e.path) : openWasmPath(e.path, e.label)">
                <AppIcon :name="e.isDir ? 'solar:folder-bold' : 'solar:file-bold'" :size="13" />
                <span class="truncate">{{ e.label }}</span>
              </button>
              <p v-if="!wasmEntriesFiltered.length" class="dim cs-empty">Empty directory.</p>
            </template>
          </div>
        </template>

        <!-- SEARCH (shared surface — same component as the Search window) -->
        <template v-else-if="sideView === 'search'">
          <div class="cs-side-h">SEARCH <span class="cs-spacer" /><button class="cs-ibtn cs-drawer-close" title="Close panel" aria-label="Close panel" @click="sideView = ''"><AppIcon name="solar:close-bold" :size="12" /></button></div>
          <div class="cs-searchwrap">
            <SearchPanel @open="openSearchResult" />
          </div>
        </template>

        <!-- OUTLINE -->
        <template v-else-if="sideView === 'outline'">
          <div class="cs-side-h">OUTLINE · {{ outlineSymbols.length }} <span class="cs-spacer" /><button class="cs-ibtn cs-drawer-close" title="Close panel" aria-label="Close panel" @click="sideView = ''"><AppIcon name="solar:close-bold" :size="12" /></button></div>
          <input v-model="outlineFilter" class="cs-input" placeholder="Filter symbols…" spellcheck="false" />
          <div class="cs-tree">
            <button v-for="s in outlineSymbols" :key="s.name + s.startLine" class="cs-trow" @click="jumpToLine(s.startLine)">
              <span class="cs-kind">{{ s.kind }}</span>
              <span class="truncate">{{ s.name }}</span>
              <span class="dim mono">:{{ s.startLine }}</span>
            </button>
            <p v-if="!outlineSymbols.length" class="dim cs-empty">{{ focusTab?.parse ? 'No symbols.' : 'Open a file to parse.' }}</p>
          </div>
        </template>

        <!-- INTEL (merged tree-sitter panel: active file / paste / path) -->
        <template v-else-if="sideView === 'intel'">
          <div class="cs-side-h">CODE INTEL · {{ intelSymbols.length }} <span class="cs-spacer" /><button class="cs-ibtn cs-drawer-close" title="Close panel" aria-label="Close panel" @click="sideView = ''"><AppIcon name="solar:close-bold" :size="12" /></button></div>
          <div class="cs-intel-src" role="tablist" aria-label="Intel source">
            <button :class="{ on: intelSource === 'file' }" role="tab" @click="intelSource = 'file'">FILE</button>
            <button :class="{ on: intelSource === 'paste' }" role="tab" @click="intelSource = 'paste'">PASTE</button>
            <button v-if="isDesktop" :class="{ on: intelSource === 'path' }" role="tab" @click="intelSource = 'path'">PATH</button>
          </div>
          <template v-if="intelSource === 'paste'">
            <input v-model="intelFileName" class="cs-input" aria-label="Snippet file name" placeholder="snippet.rs (drives language)…" spellcheck="false" autocapitalize="off" autocorrect="off" autocomplete="off" />
            <textarea v-model="intelPaste" class="cs-input cs-intel-paste" aria-label="Code snippet" rows="5" placeholder="fn main() { … }" spellcheck="false" autocapitalize="off" autocorrect="off" autocomplete="off" />
            <div class="cs-pathrow">
              <span class="mono dim">{{ intelPaste.length }} CHARS / 1 MiB</span>
              <span class="cs-spacer" />
              <button class="cs-btn xs primary" :disabled="!intelPaste.trim() || intelBusy" @click="runIntelPaste">{{ intelBusy ? 'Parsing…' : 'Parse' }}</button>
            </div>
          </template>
          <template v-else-if="intelSource === 'path'">
            <div class="cs-pathrow">
              <input v-model="intelPath" class="cs-input" aria-label="Local file path" placeholder="/home/user/main.rs" spellcheck="false" autocapitalize="off" autocorrect="off" autocomplete="off" @keyup.enter="runIntelPath" />
            </div>
            <div class="cs-pathrow">
              <span class="dim">DESKTOP ONLY — reads from disk via Tauri.</span>
              <span class="cs-spacer" />
              <button class="cs-btn xs primary" :disabled="!intelPath.trim() || intelBusy" @click="runIntelPath">{{ intelBusy ? 'Parsing…' : 'Parse' }}</button>
            </div>
          </template>
          <p v-if="intelError" class="cs-err">{{ intelError }}</p>
          <div v-if="intelMeta" class="cs-intel-meta mono dim">
            <span>{{ intelMetaTitle }}</span>
            <span>{{ intelMeta.language }} · {{ (intelMeta.engine || 'heuristic').toUpperCase() }} · {{ intelMeta.totalLines }} LINES · {{ intelMeta.symbols.length }} SYMS · {{ intelMeta.parseTimeMs }}ms</span>
          </div>
          <p v-else-if="intelSource === 'file'" class="dim cs-empty">{{ focusTab ? 'Parsing…' : 'Open a file to parse.' }}</p>
          <input v-model="intelFilter" class="cs-input" placeholder="Filter symbols…" spellcheck="false" />
          <div v-if="intelKindCounts.length" class="cs-chips">
            <button :class="{ on: !intelKind }" @click="intelKind = ''">ALL</button>
            <button v-for="kc in intelKindCounts" :key="kc.kind" :class="{ on: intelKind === kc.kind }" @click="intelKind = intelKind === kc.kind ? '' : kc.kind">{{ kc.kind }} ({{ kc.count }})</button>
          </div>
          <div class="cs-tree">
            <button v-for="s in intelSymbols" :key="s.name + s.startLine" class="cs-trow" @click="intelJump(s)" :title="s.detail || s.name">
              <span class="cs-kind">{{ s.kind }}</span>
              <span class="truncate">{{ s.name }}</span>
              <span class="dim mono">:{{ s.startLine }}{{ s.endLine !== s.startLine ? '–' + s.endLine : '' }}</span>
            </button>
            <p v-if="intelMeta && !intelSymbols.length" class="dim cs-empty">{{ intelMeta.symbols.length ? 'No symbols match the filter.' : 'No symbols found — try another file.' }}</p>
          </div>
          <div v-if="intelSource === 'paste' && intelPasteLines.length" ref="intelPreviewRef" class="cs-intel-preview">
            <div
              v-for="(line, li) in intelPasteLines"
              :key="li"
              class="cs-intel-pline"
              :class="{ hit: li + 1 === intelPreviewLine }"
              :data-ln="li + 1"
            >
              <span class="cs-intel-pln">{{ li + 1 }}</span>
              <span class="cs-intel-pcode" v-html="highlightPasteLine(line)"></span>
            </div>
          </div>
        </template>

        <!-- SESSIONS -->
        <template v-else-if="sideView === 'sessions'">
          <div class="cs-side-h">AI SESSIONS · {{ ai.sessions.value.length }} <span class="cs-spacer" /><button class="cs-ibtn cs-drawer-close" title="Close panel" aria-label="Close panel" @click="sideView = ''"><AppIcon name="solar:close-bold" :size="12" /></button></div>
          <div class="cs-tree">
            <button v-for="s in ai.sessions.value" :key="s.id" class="cs-trow col" :class="{ active: ai.viewing.value?.id === s.id }" @click="ai.loadSession(s.id)">
              <span class="truncate"><b>{{ s.title }}</b></span>
              <span class="dim">{{ s.model }} · {{ s.messages.length }} msgs</span>
            </button>
            <p v-if="!ai.sessions.value.length" class="dim cs-empty">No sessions yet — ask the AI.</p>
          </div>
        </template>
      </aside>

      <button v-if="isMobileLayout && sideView && mobileExplorerOpen" class="cs-drawer-backdrop" aria-label="Close panel" @click="mobileExplorerOpen = false" />

      <!-- ══ EDITOR COLUMN ══ -->
      <section class="cs-edcol">
        <div class="cs-groups" :class="{ split: groupBKey }">
          <!-- GROUP A -->
          <div class="cs-group">
            <div class="cs-tabs">
              <div v-for="t in tabs" :key="'a' + t.key" class="cs-tab" :class="{ active: t.key === groupAKey, dirty: t.dirty }" @click="activate(t.key, 'a')">
                <span v-if="t.dirty" class="cs-dot">●</span>
                <span class="truncate">{{ t.label }}</span>
                <button class="cs-x" title="Close" @click.stop="closeTab(t.key)"><AppIcon name="solar:close-bold" :size="10" /></button>
              </div>
            </div>
            <EditorPane v-if="tabA" :key="'a:' + tabA.key" :tab="tabA" group="a" @cursor="onCursor" @edit="onEdit(tabA)" @jump="(ln: number) => onGutterJump(ln, 'a')" @selline="(ln: number) => onGutterSelect(ln, 'a')" />
            <div v-else class="cs-welcome">
              <AppIcon name="solar:file-code-bold" :size="34" />
              <p>Open a file from the Explorer</p>
              <span class="dim">Ctrl+S saves (versions snapshot) · Ctrl+F finds · Ctrl+G splits AI in</span>
              <button v-if="isMobileLayout" class="cs-btn primary" type="button" @click="openMobileSideView('files')"><AppIcon name="solar:folder-bold" :size="14" /> Browse files</button>
            </div>
          </div>
          <!-- GROUP B -->
          <div v-if="groupBKey" class="cs-group">
            <div class="cs-tabs">
              <div class="cs-tab active">
                <span v-if="tabB?.dirty" class="cs-dot">●</span>
                <span class="truncate">{{ tabB?.label }}</span>
                <span class="cs-spacer" />
                <button class="cs-x" title="Close split" @click="groupBKey = null"><AppIcon name="solar:close-bold" :size="10" /></button>
              </div>
            </div>
            <EditorPane v-if="tabB" :key="'b:' + tabB.key" :tab="tabB" group="b" @cursor="onCursor" @edit="onEdit(tabB!)" @jump="(ln: number) => onGutterJump(ln, 'b')" @selline="(ln: number) => onGutterSelect(ln, 'b')" />
          </div>
        </div>

        <!-- FIND BAR -->
        <div v-if="findOpen" class="cs-find">
          <input ref="findRef" v-model="findQuery" class="cs-input" placeholder="Find…" spellcheck="false" @keyup.enter="findNext(1)" />
          <input v-model="replaceQuery" class="cs-input" placeholder="Replace…" spellcheck="false" @keyup.enter="replaceOne" />
          <span class="dim mono">{{ findCount ? `${findIndex + 1}/${findCount}` : '' }}</span>
          <button class="cs-btn xs" @click="findNext(1)">Next</button>
          <button class="cs-btn xs" @click="findNext(-1)">Prev</button>
          <button class="cs-btn xs" @click="replaceOne">Replace</button>
          <button class="cs-btn xs" @click="replaceAll">All</button>
          <button class="cs-ibtn" title="Go to line" @click="gotoOpen = !gotoOpen">:ln</button>
          <input v-if="gotoOpen" v-model="gotoLine" class="cs-input sm" placeholder="line…" @keyup.enter="jumpToLine(Number(gotoLine))" />
          <button class="cs-ibtn" aria-label="Close find bar" @click="findOpen = false"><AppIcon name="solar:close-bold" :size="12" /></button>
        </div>

        <!-- BOTTOM PANEL -->
        <div v-if="bottomOpen" class="cs-bottom">
          <div class="cs-btabs">
            <button :class="{ active: bottomTab === 'term' }" @click="bottomTab = 'term'">TERMINAL</button>
            <button :class="{ active: bottomTab === 'problems' }" @click="bottomTab = 'problems'">PROBLEMS · {{ problems.length }}</button>
            <span class="cs-spacer" />
            <button class="cs-ibtn" aria-label="Close bottom panel" @click="bottomOpen = false"><AppIcon name="solar:close-bold" :size="12" /></button>
          </div>
          <div v-if="bottomTab === 'term'" class="cs-termwrap">
            <div ref="termScrollRef" class="cs-term">
              <div v-for="l in termLines" :key="l.id" class="cs-terml" :class="l.kind">{{ l.text }}</div>
            </div>
            <form class="cs-termin" @submit.prevent="submitTerm">
              <span class="mono">❯</span>
              <input v-model="termInput" class="mono" :placeholder="`cybsh in ${termCwd}… (TAB completes)`" aria-label="Terminal input" @keydown.tab.prevent="completeTerm" />
            </form>
          </div>
          <div v-else class="cs-problems">
            <div v-for="p in problems" :key="p.key" class="cs-prow" @click="p.tab && activate(p.tab, 'a'); p.line && jumpToLine(p.line)">
              <span class="cs-pkind" :class="p.sev">{{ p.sev }}</span>
              <span class="truncate">{{ p.text }}</span>
              <span class="dim mono">{{ p.where }}</span>
            </div>
            <p v-if="!problems.length" class="dim cs-empty">No problems — parses clean, all tabs saved.</p>
          </div>
        </div>

        <!-- STATUS BAR -->
        <footer class="cs-status">
          <span>Ln {{ cursorLine }}, Col {{ cursorCol }}</span>
          <span class="dim">{{ focusTab?.language || '—' }}</span>
          <span v-if="crumbPath" class="dim truncate hide-mid" :title="crumbPath">{{ crumbPath }}</span>
          <span class="cs-spacer" />
          <span v-if="focusTab" :class="focusTab.dirty ? 'cs-warn' : 'dim'">{{ focusTab.dirty ? '● unsaved' : 'saved ✓' }}</span>
          <span class="dim">{{ focusTab?.content.length ?? 0 }} B</span>
          <span v-if="voiceCoding" class="cs-voicestat" :title="voice.interim.value || 'Listening — speak code'"><AppIcon name="solar:microphone-bold" :size="11" /> {{ voice.interim.value || 'listening…' }}</span>
          <span class="cs-aistat" :class="{ run: ai.jobActive.value }" :title="ai.jobLine.value || 'AI idle'">
            <AppIcon name="solar:bot-bold" :size="11" /> {{ ai.jobActive.value ? ai.jobLine.value : 'AI ready' }}
          </span>
        </footer>
      </section>

      <!-- ══ AI PANEL ══ -->
      <aside v-if="aiOpen" class="cs-ai">
        <div class="cs-ai-h">
          <AppIcon name="solar:bot-bold" :size="14" /><b>AI · {{ ai.chatConfig.value?.name || 'no config' }}</b>
          <span class="cs-spacer" />
          <button class="cs-ibtn" title="Open full agent panel" @click="wm.open('agent')"><AppIcon name="solar:chat-square-bold" :size="12" /></button>
          <button class="cs-ibtn" title="Close AI" @click="aiOpen = false"><AppIcon name="solar:close-bold" :size="12" /></button>
        </div>
        <div class="cs-ai-cfg">
          <select v-model="ai.chatConfigId.value" class="cs-select" aria-label="Agent config">
            <option value="">— pick agent config —</option>
            <option v-for="c in ai.configs.value" :key="c.id" :value="c.id">{{ c.name }} · {{ c.model }}</option>
          </select>
          <button class="cs-btn xs" title="New session" @click="ai.newSession">New</button>
        </div>
        <div v-if="!ai.configs.value.length" class="cs-ai-note">
          No agent configs. <button class="cs-link" @click="wm.open('agent')">Set one up in Agent →</button>
        </div>
        <div v-if="ai.wasmMode.value && ai.chatConfig.value && !hasLocalKey" class="cs-ai-note">
          Browser loop needs the key (memory only):
          <form @submit.prevent="sealKey"><input v-model="keyInput" type="password" class="cs-input" placeholder="paste API key…" /><button class="cs-btn xs" type="submit">Seal</button></form>
        </div>
        <div v-if="ai.viewing.value" class="cs-ai-meta mono dim">
          {{ ai.viewing.value.title }} · ctx ~{{ fmtTok(ai.contextTokens.value) }}/{{ fmtTok(ai.contextWindow.value) }}
          <span v-if="ai.costUsd.value != null"> · ~${{ ai.costUsd.value.toFixed(2) }}</span>
          <div class="cs-ctxbar"><i :style="{ width: ai.contextPct.value + '%' }" :class="ai.contextPct.value >= 85 ? 'bad' : ai.contextPct.value >= 60 ? 'warn' : ''" /></div>
        </div>

        <div ref="threadRef" class="cs-thread">
          <template v-for="row in ai.threadRows.value" :key="row.key">
            <div v-if="row.kind === 'message'" class="cs-msg" :class="'r-' + row.message.role">
              <div class="cs-role dim">{{ roleLabel(row.message) }}</div>
              <div v-if="row.message.role === 'assistant' && row.message.content" class="cs-md" v-html="renderMarkdown(row.message.content)" />
              <div v-else-if="row.message.content" class="cs-body">{{ row.message.content }}</div>
              <div v-else class="cs-toolblock">{{ toolNameOf(row.message) }}</div>
            </div>
            <div v-else class="cs-tgroup">
              <div v-if="row.lead" class="cs-md" v-html="renderMarkdown(row.lead)" />
              <div v-for="t in row.rows" :key="t.key" class="cs-trow2" :class="['is-' + t.state, { open: openRows.has(t.key) }]">
                <button class="cs-thead" @click="toggleRow(t.key)">
                  <AppIcon :name="toolMeta(t.name).icon" :size="11" />
                  <span class="truncate">{{ t.title }}</span>
                  <span class="cs-tstate">{{ t.state }}</span>
                </button>
                <div v-if="openRows.has(t.key) || t.state === 'running'" class="cs-tdetail">
                  <pre>{{ prettyTool(t) }}</pre>
                  <pre v-if="t.result" class="res">{{ t.result.slice(0, 1200) }}</pre>
                  <span v-else-if="t.state === 'running'" class="dim mono">waiting for result…</span>
                </div>
              </div>
            </div>
          </template>
          <p v-if="!ai.threadRows.value.length" class="dim cs-empty">Ask about this file — attach it with @file.</p>
        </div>

        <div v-if="ai.pendingApproval.value" class="cs-approval">
          <div class="cs-apph"><AppIcon name="solar:shield-check-bold" :size="12" /> {{ ai.pendingApproval.value.question ? 'NEEDS YOUR ANSWER' : 'APPROVAL' }}</div>
          <div class="cs-apptext">{{ ai.pendingApproval.value.question || ai.pendingApproval.value.summary }}</div>
          <pre v-if="!ai.pendingApproval.value.question" class="cs-appinput">{{ approvalInput }}</pre>
          <input v-if="ai.pendingApproval.value.question" v-model="ai.answerInput.value" class="cs-input" placeholder="Type answer…" @keyup.enter="ai.answer(true)" />
          <div class="cs-approw">
            <button class="cs-btn xs primary" @click="ai.answer(true)">Allow</button>
            <button v-if="!ai.pendingApproval.value.question" class="cs-btn xs" @click="ai.answer(true, true)">Always</button>
            <button class="cs-btn xs danger" @click="ai.answer(false)">Deny</button>
          </div>
        </div>

        <div v-if="ai.queue.value.length" class="cs-queue">
          <span v-for="(q, qi) in ai.queue.value" :key="qi" class="cs-qitem" :title="q">
            <span class="truncate">{{ q.slice(0, 90) }}</span>
            <button class="cs-ibtn" title="Drop queued prompt" @click="ai.dropQueued(qi)"><AppIcon name="solar:close-bold" :size="10" /></button>
          </span>
        </div>
        <div class="cs-quick">
          <button :class="{ on: attachFile }" title="Attach active file to next prompt" @click="attachFile = !attachFile">@file{{ focusTab ? ':' + shortName(focusTab.label) : '' }}</button>
          <button :class="{ on: attachSel }" :disabled="!selection" title="Attach current selection" @click="attachSel = !attachSel">@sel{{ selection ? ` (${selection.length}B)` : '' }}</button>
          <button title="Explain this file" @click="quick('explain')">Explain</button>
          <button title="Review for bugs" @click="quick('review')">Review</button>
          <button title="Apply last code block to file" @click="applyLastBlock">Apply ⤵</button>
        </div>
        <form class="cs-prompt" @submit.prevent="sendPrompt">
          <textarea ref="promptEl" v-model="promptInput" rows="2" :placeholder="ai.jobActive.value ? 'Running — Ctrl+Enter queues next…' : 'Ask the AI… (Ctrl+Enter sends)'" @keydown.ctrl.enter.exact.prevent="sendPrompt" @keydown.meta.enter.exact.prevent="sendPrompt" />
          <div v-if="voice.isSupported.value" class="cs-voice-group">
            <div class="cs-voice-pop" role="group" aria-label="Dictation language">
              <button type="button" class="cs-voice-opt" :class="{ on: voice.langPref.value === 'auto' }" :aria-pressed="voice.langPref.value === 'auto'" title="Auto-detect from your browser language" @click="voice.setLang('auto')">Auto · {{ voice.lang.value === 'pt-BR' ? 'PT' : 'EN' }}</button>
              <button type="button" class="cs-voice-opt" :class="{ on: voice.langPref.value === 'en-US' }" :aria-pressed="voice.langPref.value === 'en-US'" title="Dictate in English" @click="voice.setLang('en-US')">EN</button>
              <button type="button" class="cs-voice-opt" :class="{ on: voice.langPref.value === 'pt-BR' }" :aria-pressed="voice.langPref.value === 'pt-BR'" title="Dictate in Portuguese (PT-BR)" @click="voice.setLang('pt-BR')">PT</button>
            </div>
            <button class="cs-btn xs" :class="{ on: voice.listening.value }" type="button" :title="voice.listening.value ? `Listening… ${voice.interim.value}` : voice.langPref.value === 'auto' ? `Dictate — auto (${voice.lang.value === 'pt-BR' ? 'Portuguese' : 'English'}), hover to pick EN/PT` : 'Dictate (code mode: say “open paren”, “dot”, “camel case …” — hover to change language)'" @click="toggleVoice">{{ voice.listening.value ? 'Stop' : 'Mic' }}</button>
          </div>
          <button class="cs-btn xs primary" type="submit" :disabled="!canSend">{{ ai.jobActive.value ? 'Queue' : 'Send' }}</button>
          <button v-if="ai.jobActive.value" class="cs-btn xs danger" type="button" @click="ai.abort()">Stop</button>
        </form>
        <div v-if="ai.jobLine.value" class="mono dim cs-jobline">{{ ai.jobLine.value }}</div>
        <div v-if="ai.jobError.value" class="cs-err" :title="ai.jobHint.value">{{ ai.jobError.value.slice(0, 200) }}</div>
      </aside>
    </div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import SearchPanel from '@/components/SearchPanel.vue'
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount, h, defineComponent } from 'vue'
import type { PropType } from 'vue'
import { useAppStore } from '@/stores/app'
import { useWindowManager } from '@/composables/useWindowManager'
import { useStudioAgent, type FileCtx } from '@/composables/useStudioAgent'
import { hasLocalKey as hasSharedKey, localKeyRevision } from '@/composables/useAgent'
import { invoke } from '@/composables/useTauri'
import { useTransport } from '@/composables/useTransport'
import { escapeHtml, renderMarkdown } from '@/utils/markdown'
import { checkBrackets, detectLanguage, highlightSyntax } from '@/utils/codeDetect'
import { correctShellLine } from '@/utils/speechCorrect'
import { useVoiceInput } from '@/composables/useVoiceInput'
import { trackShellCwd } from '@/utils/shellCwd'
import { toolMeta } from '@/utils/agentUi'
import type { CodeSymbol, FileNode, ParseResult } from '@/types'

/* ═══════════════ tabs / editor core ═══════════════ */

interface Tab {
  key: string
  label: string
  kind: 'managed' | 'wasm'
  fileId: string
  path: string
  language: string
  content: string
  savedContent: string
  dirty: boolean
  parse: ParseResult | null
  pinned?: boolean
}

const store = useAppStore()
const wm = useWindowManager()
const ai = useStudioAgent()

const transportBackend = computed(() => useTransport().backend)
const isDesktop = computed(() => transportBackend.value === 'tauri')
const isWasm = computed(() => transportBackend.value === 'wasm')
const isMobileLayout = ref(false)
const mobileExplorerOpen = ref(false)
const mobileMedia = typeof window !== 'undefined' ? window.matchMedia('(max-width: 860px)') : null
const syncMobileLayout = () => {
  isMobileLayout.value = mobileMedia?.matches ?? false
  if (!isMobileLayout.value) mobileExplorerOpen.value = false
}
const transportLabel = computed(() => (isDesktop.value ? 'DESKTOP' : isWasm.value ? 'WASM LOCAL' : 'WEB / REST'))

const tabs = ref<Tab[]>([])
const groupAKey = ref('')
const groupBKey = ref<string | null>(null)
const activeGroup = ref<'a' | 'b'>('a')
const tabA = computed(() => tabs.value.find(t => t.key === groupAKey.value) ?? null)
const tabB = computed(() => (groupBKey.value ? tabs.value.find(t => t.key === groupBKey.value) ?? null : null))
const focusTab = computed(() => (activeGroup.value === 'b' ? tabB.value ?? tabA.value : tabA.value))

const MAX_EDIT_BYTES = 1024 * 1024
const CODE_EXTS = new Set(['rs','ts','tsx','js','jsx','py','go','c','h','cpp','hpp','cc','java','rb','swift','kt','html','htm','css','scss','less','json','toml','yaml','yml','md','mdx','sql','sh','bash','zsh','lua','zig','ex','vue','svelte','dart','txt','log','ini','cfg'])
const extOf = (n: string) => { const p = n.toLowerCase().split('.'); return p.length > 1 ? p[p.length - 1] : '' }
const shortName = (n: string) => (n.length > 18 ? '…' + n.slice(-17) : n)
function isEditableNode(n: FileNode): boolean {
  if (n.fileType !== 'file' || n.encrypted) return false
  if (n.sizeBytes > MAX_EDIT_BYTES) return false
  const m = n.mimeType ?? ''
  if (m.startsWith('text/') || m === 'application/json') return true
  return CODE_EXTS.has(extOf(n.name))
}

/* ═══════════════ explorer tree (managed) ═══════════════ */

const sideView = ref('files')
const activities = [
  { id: 'files', label: 'Explorer', icon: 'solar:folder-bold' },
  { id: 'search', label: 'Search', icon: 'solar:magnifier-bold' },
  { id: 'outline', label: 'Outline', icon: 'solar:folder-tree-bold' },
  { id: 'intel', label: 'Code intel (tree-sitter)', icon: 'solar:code-bold' },
  { id: 'sessions', label: 'AI sessions', icon: 'solar:chat-square-bold' },
]
const explorerFilter = ref('')
const editableFiles = computed(() => {
  const q = explorerFilter.value.trim().toLowerCase()
  return store.files.filter(n => isEditableNode(n)).filter(n => !q || n.name.toLowerCase().includes(q))
})
interface TreeNode extends FileNode { kids: TreeNode[] }
const treeRoots = computed<TreeNode[]>(() => {
  const map = new Map<string, TreeNode>()
  for (const f of editableFiles.value) map.set(f.id, { ...f, kids: [] })
  const roots: TreeNode[] = []
  for (const n of map.values()) {
    const p = n.parentId && map.get(n.parentId)
    if (p) p.kids.push(n)
    else if (n.fileType === 'folder' || !n.parentId) roots.push(n)
    else {
      // parent folder itself isn't editable (or hidden) — still show the file at root
      roots.push(n)
    }
  }
  const sort = (xs: TreeNode[]) => xs.sort((a, b) => ((b.fileType === 'folder') as unknown as number) - ((a.fileType === 'folder') as unknown as number) || a.name.localeCompare(b.name)).forEach(n => sort(n.kids))
  sort(roots)
  return roots
})

// Explicit annotation: the component references itself for folder recursion,
// and without it vue-tsc infers a circular `any` (TS7022/TS7024). The stable
// identity also keeps per-folder collapse state across explorer re-renders.
const TreeRow: ReturnType<typeof defineComponent> = defineComponent({
  name: 'TreeRow',
  props: {
    node: { type: Object as PropType<TreeNode>, required: true },
    depth: { type: Number, default: 0 },
    activeKey: { type: String, default: '' },
  },
  emits: ['open'],
  setup(props, { emit }) {
    const collapsed = ref(props.depth > 1)
    const isDir = computed(() => props.node.fileType === 'folder')
    const isActive = computed(() => props.node.id === (props.activeKey ?? '').replace(/^m:/, ''))
    const onRow = () => {
      if (isDir.value) collapsed.value = !collapsed.value
      else emit('open', props.node)
    }
    return () =>
      h('div', { class: 'cs-tbranch' }, [
        h('button', {
          class: ['cs-trow', { dir: isDir.value, active: isActive.value }],
          style: { paddingLeft: `${6 + props.depth * 12}px` },
          onClick: onRow,
          onDblclick: onRow,
        }, [
          h('span', { class: 'cs-caret' }, isDir.value ? (collapsed.value ? '▸' : '▾') : ''),
          h('span', { class: 'cs-ticon' }, isDir.value ? '📁' : '📄'),
          h('span', { class: 'truncate' }, props.node.name),
        ]),
        isDir.value && !collapsed.value
          ? props.node.kids.map(k =>
              h(TreeRow, { node: k, depth: props.depth + 1, activeKey: props.activeKey, onOpen: (n: FileNode) => emit('open', n) }),
            )
          : null,
      ])
  },
})

async function openNode(n: FileNode) {
  if (n.fileType === 'folder') return
  const key = `m:${n.id}`
  const existing = tabs.value.find(t => t.key === key)
  if (existing) { mobileExplorerOpen.value = false; activate(key, activeGroup.value); return }
  const res = await store.readManagedContent(n.id)
  if (!res) return
  mobileExplorerOpen.value = false
  tabs.value.push({ key, label: n.name, kind: 'managed', fileId: n.id, path: n.id, language: detectLanguage(n.name, res.content).language, content: res.content, savedContent: res.content, dirty: false, parse: null })
  activate(key, activeGroup.value)
  void reparse(tabs.value[tabs.value.length - 1])
}

function activate(key: string, group: 'a' | 'b') {
  activeGroup.value = group
  if (group === 'a') groupAKey.value = key
  else groupBKey.value = key
  nextTick(() => updateCursorFromDom())
}
function closeTab(key: string) {
  const i = tabs.value.findIndex(t => t.key === key)
  if (i === -1) return
  tabs.value.splice(i, 1)
  if (groupBKey.value === key) groupBKey.value = null
  if (groupAKey.value === key) {
    // Prefer the neighbour, but never mirror the split group.
    const cands = [tabs.value[i - 1], tabs.value[i], ...tabs.value].filter((t): t is Tab => !!t)
    groupAKey.value = cands.find(t => t.key !== groupBKey.value)?.key ?? ''
  }
}
function splitRight() {
  if (!focusTab.value) return
  groupBKey.value = focusTab.value.key
  activeGroup.value = 'b'
}

/* ═══════════════ wasm explorer ═══════════════ */

const wasmCwd = ref('/')
const wasmEntries = ref<string[]>([])
const wasmNewShow = ref(false)
const newFileName = ref('')
const wasmEntriesFiltered = computed(() => {
  const q = explorerFilter.value.trim().toLowerCase()
  return wasmEntries.value.filter(n => !q || n.toLowerCase().includes(q)).map(name => {
    const isDir = name.endsWith('/')
    const clean = isDir ? name.slice(0, -1) : name
    return { key: `w:${wasmCwd.value === '/' ? '/' + clean : wasmCwd.value + '/' + clean}`, label: clean, path: wasmCwd.value === '/' ? `/${clean}` : `${wasmCwd.value}/${clean}`, isDir }
  })
})
async function refreshExplorer() {
  if (isWasm.value) wasmEntries.value = await store.listWasmDir(wasmCwd.value)
  else await store.fetchFiles()
}
function wasmUp() {
  if (wasmCwd.value === '/') return
  const p = wasmCwd.value.split('/').filter(Boolean)
  p.pop()
  wasmCwd.value = p.length ? '/' + p.join('/') : '/'
  void refreshExplorer()
}
function navigateWasm(path: string) { wasmCwd.value = path; void refreshExplorer() }
async function createWasmFile() {
  const name = newFileName.value.trim().replace(/\//g, '_')
  if (!name) return
  const full = wasmCwd.value === '/' ? `/${name}` : `${wasmCwd.value}/${name}`
  newFileName.value = ''
  wasmNewShow.value = false
  if (await store.saveWasmFile(full, '')) {
    await refreshExplorer()
    await openWasmPath(full, name)
  }
}
async function openWasmPath(path: string, label: string) {
  const key = `w:${path}`
  const existing = tabs.value.find(t => t.key === key)
  if (existing) { mobileExplorerOpen.value = false; activate(key, activeGroup.value); return }
  const content = await store.readWasmFile(path)
  if (content === null) return
  mobileExplorerOpen.value = false
  tabs.value.push({ key, label, kind: 'wasm', fileId: '', path, language: detectLanguage(label, content).language, content, savedContent: content, dirty: false, parse: null })
  activate(key, activeGroup.value)
  void reparse(tabs.value[tabs.value.length - 1])
}

/* ═══════════════ live sync: explorer + open tabs follow agent writes ═══════════════
   The agent edits files server-side (or in the WASM volume) while the run is
   active. The explorer used to go stale until a manual refresh, and open tabs
   kept dead bytes. While a job is active we re-list every 2.5s and refresh
   clean tabs; on terminal status we do one final pass. Dirty tabs (user has
   unsaved edits) keep their content — only the label follows a rename. */

let explorerSyncTimer = 0
let syncingAgentFiles = false
async function syncOpenTabsWithDisk() {
  for (const tab of tabs.value) {
    try {
      if (tab.kind === 'managed') {
        const node = store.files.find(f => f.id === tab.fileId)
        if (node && node.name !== tab.label) tab.label = node.name
        if (!node || tab.dirty) continue
        const res = await invoke<{ content: string }>('read_file_content', { fileId: tab.fileId })
        const latest = typeof res?.content === 'string' ? res.content : null
        if (latest !== null && latest !== tab.savedContent) {
          tab.content = latest
          tab.savedContent = latest
          tab.dirty = false
          void reparse(tab)
        }
      } else {
        if (tab.dirty) continue
        const res = await invoke<{ ok: boolean; output: string }>('os_exec', { line: `cat "${tab.path.replace(/"/g, '\\"')}"` }).catch(() => null)
        if (!res || !res.ok) continue
        if (res.output !== tab.savedContent) {
          tab.content = res.output
          tab.savedContent = res.output
          tab.dirty = false
          void reparse(tab)
        }
      }
    } catch { /* best-effort per tab — next poll heals */ }
  }
}
async function syncExplorerAndTabs() {
  if (syncingAgentFiles) return
  syncingAgentFiles = true
  try {
    await refreshExplorer()
    await syncOpenTabsWithDisk()
  } finally {
    syncingAgentFiles = false
  }
}
watch(() => ai.jobActive.value, (active, was) => {
  if (active && !explorerSyncTimer) {
    explorerSyncTimer = window.setInterval(() => { if (!document.hidden) void syncExplorerAndTabs() }, 2500)
    void syncExplorerAndTabs()
  } else if (!active && was) {
    if (explorerSyncTimer) { window.clearInterval(explorerSyncTimer); explorerSyncTimer = 0 }
    void syncExplorerAndTabs()
  }
})
// A new tool row usually means a write/edit/rename just landed — sync between polls.
watch(() => ai.threadRows.value.length, (n, prev) => {
  if (n !== prev && ai.jobActive.value) void syncExplorerAndTabs()
})

/* ═══════════════ search ═══════════════ */

/* ═══════════════ search (shared SearchPanel) ═══════════════ */

async function openSearchResult(fileId: string) {
  const node = store.files.find(f => f.id === fileId)
  if (node && isEditableNode(node)) return openNode(node)
  const got = await store.getFile(fileId)
  if (got && isEditableNode(got)) return openNode(got)
  store.notifySuccess('Found — but not an editable text file')
}

/* ═══════════════ outline / parse ═══════════════ */

const outlineFilter = ref('')
const parsingOutline = ref(false)
const outlineSymbols = computed<CodeSymbol[]>(() => {
  const q = outlineFilter.value.trim().toLowerCase()
  const syms = focusTab.value?.parse?.symbols ?? []
  if (!q) return syms
  return syms.filter(s => s.name.toLowerCase().includes(q) || (s.detail ?? '').toLowerCase().includes(q))
})
async function reparse(tab: Tab | null) {
  if (!tab || tab.content.length > 256 * 1024) return
  parsingOutline.value = true
  try {
    const res = await invoke<ParseResult>('parse_text', { fileName: tab.label, content: tab.content })
    const t = tabs.value.find(t => t.key === tab.key)
    if (t) t.parse = res
  } catch { /* best-effort */ } finally { parsingOutline.value = false }
}
let reparseTimer = 0
function scheduleReparse() {
  window.clearTimeout(reparseTimer)
  const tab = focusTab.value
  if (!tab || tab.content.length > 256 * 1024) return
  reparseTimer = window.setTimeout(() => void reparse(tab), 1200)
}
const cursorLine = ref(1)
const cursorCol = ref(1)
const selection = ref('')
function onCursor(line: number, col: number, sel: string) {
  cursorLine.value = line
  cursorCol.value = col
  selection.value = sel
}
function updateCursorFromDom() {
  const el = document.querySelector<HTMLTextAreaElement>(`.cs-pane[data-g="${activeGroup.value}"] textarea`)
  if (!el || !focusTab.value) return
  const pos = el.selectionStart ?? 0
  const before = focusTab.value.content.slice(0, pos)
  cursorLine.value = before.split('\n').length
  cursorCol.value = pos - before.lastIndexOf('\n')
  selection.value = el.value.slice(el.selectionStart ?? 0, el.selectionEnd ?? 0)
}
const crumbPath = computed(() => {
  const tab = focusTab.value
  if (!tab?.parse) return ''
  const chain: string[] = []
  const walk = (syms: CodeSymbol[]) => {
    for (const s of syms) {
      if (s.startLine <= cursorLine.value && cursorLine.value <= s.endLine) { chain.push(s.name); walk(s.children); break }
    }
  }
  walk(tab.parse.symbols)
  return chain.join(' › ')
})
function jumpToLine(line: number, group: 'a' | 'b' = activeGroup.value) {
  const el = document.querySelector<HTMLTextAreaElement>(`.cs-pane[data-g="${group}"] textarea`)
  const tab = group === 'b' ? tabB.value ?? tabA.value : tabA.value
  if (!el || !tab || !line) return
  const lines = tab.content.split('\n')
  const idx = Math.max(0, Math.min(line - 1, lines.length - 1))
  const pos = lines.slice(0, idx).join('\n').length + (idx > 0 ? 1 : 0)
  el.focus()
  el.selectionStart = el.selectionEnd = pos
  // Keep the highlight/gutter layers aligned when jumping from a distance.
  el.scrollTop = Math.max(0, (idx - 2) * 19.2)
  updateCursorFromDom()
}
/** Select a whole line (1-based) — gutter double-click. */
function selectLineAt(line: number, group: 'a' | 'b' = activeGroup.value) {
  const el = document.querySelector<HTMLTextAreaElement>(`.cs-pane[data-g="${group}"] textarea`)
  const tab = group === 'b' ? tabB.value ?? tabA.value : tabA.value
  if (!el || !tab || !line) return
  const lines = tab.content.split('\n')
  const idx = Math.max(0, Math.min(line - 1, lines.length - 1))
  const start = lines.slice(0, idx).join('\n').length + (idx > 0 ? 1 : 0)
  el.focus()
  el.selectionStart = start
  el.selectionEnd = start + lines[idx].length
  el.scrollTop = Math.max(0, (idx - 2) * 19.2)
  updateCursorFromDom()
}
/** Gutter click: focus that pane's tab and jump the cursor to the line. */
function onGutterJump(line: number, group: 'a' | 'b') {
  activate(group === 'b' ? groupBKey.value ?? groupAKey.value : groupAKey.value, group)
  nextTick(() => jumpToLine(line, group))
}
/** Gutter double-click: select the whole line for immediate editing. */
function onGutterSelect(line: number, group: 'a' | 'b') {
  activate(group === 'b' ? groupBKey.value ?? groupAKey.value : groupAKey.value, group)
  nextTick(() => selectLineAt(line, group))
}

/* ═══════════════ code intel (merged tree-sitter panel) ═══════════════
   The old standalone Code Intelligence window lives here now: one studio,
   three sources — the active FILE tab (live reparse), PASTE (ad-hoc text)
   and PATH (desktop disk read). Paste/path parses stay local so they never
   clobber the shared store result that the inspector preview renders. */

const intelSource = ref<'file' | 'paste' | 'path'>('file')
const intelKind = ref('')
const intelFilter = ref('')
const intelFileName = ref('snippet.rs')
const intelPaste = ref('')
const intelPasteResult = ref<ParseResult | null>(null)
const intelPath = ref('')
const intelPathResult = ref<ParseResult | null>(null)
const intelBusy = ref(false)
const intelError = ref('')
const intelPreviewRef = ref<HTMLElement | null>(null)
const intelPreviewLine = ref(0)

const intelMeta = computed<ParseResult | null>(() =>
  intelSource.value === 'file' ? focusTab.value?.parse ?? null
  : intelSource.value === 'paste' ? intelPasteResult.value
  : intelPathResult.value,
)
const intelMetaTitle = computed(() => {
  if (intelSource.value === 'file') return focusTab.value?.label ?? ''
  if (intelSource.value === 'paste') return intelFileName.value.trim() || 'snippet.txt'
  return intelMeta.value?.filePath ?? intelPath.value.trim()
})
const intelKindCounts = computed(() => {
  const counts = new Map<string, number>()
  for (const s of intelMeta.value?.symbols ?? []) counts.set(s.kind, (counts.get(s.kind) ?? 0) + 1)
  return [...counts.entries()]
    .map(([kind, count]) => ({ kind, count }))
    .sort((a, b) => b.count - a.count || a.kind.localeCompare(b.kind))
})
const intelSymbols = computed<CodeSymbol[]>(() => {
  const q = intelFilter.value.trim().toLowerCase()
  return (intelMeta.value?.symbols ?? []).filter(s => {
    if (intelKind.value && s.kind !== intelKind.value) return false
    if (!q) return true
    return s.name.toLowerCase().includes(q) || (s.detail ?? '').toLowerCase().includes(q)
  })
})
/** Paste preview is capped so a megabyte dump can't freeze the sidebar. */
const intelPasteLines = computed(() => (intelSource.value === 'paste' && intelPasteResult.value ? intelPaste.value.split('\n').slice(0, 300) : []))
function highlightPasteLine(line: string): string {
  return highlightSyntax(line, intelPasteResult.value?.language ?? '')
}
async function runIntelPaste() {
  const content = intelPaste.value
  if (!content.trim() || content.length > MAX_EDIT_BYTES) {
    intelError.value = content.length > MAX_EDIT_BYTES ? 'Paste exceeds the 1 MiB parse limit.' : ''
    return
  }
  intelBusy.value = true
  intelError.value = ''
  try {
    intelPasteResult.value = await invoke<ParseResult>('parse_text', { fileName: intelFileName.value.trim() || 'snippet.txt', content })
    intelKind.value = ''
    intelPreviewLine.value = 0
  } catch (e) {
    intelError.value = e instanceof Error ? e.message : 'Parse failed.'
  } finally {
    intelBusy.value = false
  }
}
async function runIntelPath() {
  const p = intelPath.value.trim()
  if (!p) return
  intelBusy.value = true
  intelError.value = ''
  try {
    intelPathResult.value = await invoke<ParseResult>('parse_file', { filePath: p })
    intelKind.value = ''
  } catch (e) {
    intelError.value = e instanceof Error ? e.message : 'Parse failed.'
  } finally {
    intelBusy.value = false
  }
}
function intelJump(s: CodeSymbol) {
  if (intelSource.value === 'file') {
    jumpToLine(s.startLine)
    return
  }
  if (intelSource.value === 'paste') {
    intelPreviewLine.value = s.startLine
    nextTick(() => {
      intelPreviewRef.value?.querySelector(`[data-ln="${s.startLine}"]`)?.scrollIntoView({ block: 'center' })
    })
  }
}

/* ═══════════════ find / replace ═══════════════ */

const findOpen = ref(false)
const findQuery = ref('')
const replaceQuery = ref('')
const findIndex = ref(-1)
const gotoOpen = ref(false)
const gotoLine = ref('')
const findRef = ref<HTMLInputElement | null>(null)
const findMatches = computed(() => {
  const tab = focusTab.value
  const q = findQuery.value
  if (!tab || !q) return []
  const out: number[] = []
  let i = tab.content.indexOf(q)
  while (i !== -1 && out.length < 500) { out.push(i); i = tab.content.indexOf(q, i + q.length) }
  return out
})
const findCount = computed(() => findMatches.value.length)
watch(findQuery, () => { findIndex.value = -1 })
function focusMatch() {
  const tab = focusTab.value
  const el = document.querySelector<HTMLTextAreaElement>(`.cs-pane[data-g="${activeGroup.value}"] textarea`)
  if (!el || !tab || !findMatches.value.length) return
  const pos = findMatches.value[findIndex.value]
  el.focus()
  el.selectionStart = pos
  el.selectionEnd = pos + findQuery.value.length
  updateCursorFromDom()
}
function findNext(dir: 1 | -1) {
  if (!findMatches.value.length) { findIndex.value = -1; return }
  findIndex.value = (findIndex.value + dir + findMatches.value.length) % findMatches.value.length
  focusMatch()
}
function replaceOne() {
  const tab = focusTab.value
  if (!tab || !findQuery.value || findIndex.value < 0) return
  const pos = findMatches.value[findIndex.value]
  tab.content = tab.content.slice(0, pos) + replaceQuery.value + tab.content.slice(pos + findQuery.value.length)
  onEdit(tab)
}
function replaceAll() {
  const tab = focusTab.value
  if (!tab || !findQuery.value) return
  tab.content = tab.content.split(findQuery.value).join(replaceQuery.value)
  onEdit(tab)
}

/* ═══════════════ save / edit ═══════════════ */

const saving = ref(false)
function onEdit(tab: Tab) {
  tab.dirty = tab.content !== tab.savedContent
  scheduleReparse()
}
async function saveTab(tab: Tab | null) {
  if (!tab || saving.value) return
  saving.value = true
  try {
    if (tab.kind === 'managed') {
      const saved = await store.saveManagedContent(tab.fileId, tab.content)
      if (saved) { tab.savedContent = tab.content; tab.dirty = false }
    } else {
      if (await store.saveWasmFile(tab.path, tab.content)) { tab.savedContent = tab.content; tab.dirty = false }
    }
    if (!tab.dirty) void reparse(tab)
  } finally { saving.value = false }
}
function onGlobalKey(e: KeyboardEvent) {
  const t = e.target as HTMLElement
  const tag = t.tagName
  const inInput = tag === 'INPUT' || tag === 'SELECT'
  const inPrompt = t === promptEl.value
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') { e.preventDefault(); void saveTab(focusTab.value); return }
  // Find works from the editor textarea too (VSCode-like); plain inputs keep theirs.
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f' && !inInput && !inPrompt) { e.preventDefault(); findOpen.value = true; nextTick(() => findRef.value?.focus()); return }
  // Ctrl+G summons the AI from anywhere except its own prompt (there it blurs back).
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'g') {
    e.preventDefault()
    if (inPrompt) { promptEl.value?.blur(); return }
    aiOpen.value = true
    nextTick(() => promptEl.value?.focus())
    return
  }
  if (e.ctrlKey && e.key === '`' && !inPrompt) { e.preventDefault(); bottomOpen.value = !bottomOpen.value; return }
  if (e.key === 'Escape' && sideView.value && !inPrompt) { sideView.value = ''; return }
  if (e.key === 'Escape' && findOpen.value && !inPrompt) { findOpen.value = false; return }
}

/* ═══════════════ problems ═══════════════ */

const bottomOpen = ref(false)
const bottomTab = ref<'term' | 'problems'>('term')
const problems = computed(() => {
  const out: { key: string; sev: string; text: string; where: string; tab: string | null; line: number }[] = []
  for (const t of tabs.value) {
    if (t.dirty) out.push({ key: t.key + ':dirty', sev: 'info', text: `Unsaved changes`, where: t.label, tab: t.key, line: 0 })
    if (t.parse && !t.parse.symbols.length && t.content.length > 0) out.push({ key: t.key + ':nosym', sev: 'warn', text: 'No symbols parsed', where: `${t.label} · ${t.parse.engine}`, tab: t.key, line: 0 })
    if (t.content.length > 0 && t.content.length <= 256 * 1024) {
      for (const b of checkBrackets(t.content, t.language)) {
        out.push({ key: `${t.key}:br:${b.line}:${b.message}`, sev: 'warn', text: b.message, where: t.label, tab: t.key, line: b.line })
      }
    }
  }
  return out
})

/* ═══════════════ bottom terminal (cybsh mirror — server keeps the real cwd) ═══════════════ */

// Writable mirror: the old `computed(() => '/')` was readonly, so every
// `cd` assignment failed typecheck (TS2540) and the label never moved.
const termCwd = ref('/')
const termInput = ref('')
const termLines = ref<{ id: number; kind: string; text: string }[]>([])
const termScrollRef = ref<HTMLElement | null>(null)
let termId = 0
function termPush(kind: string, text: string) {
  for (const line of text.split('\n')) {
    if (line === '' && kind !== 'in') continue
    termLines.value.push({ id: ++termId, kind, text: line || ' ' })
  }
  if (termLines.value.length > 400) termLines.value.splice(0, termLines.value.length - 400)
  nextTick(() => { const el = termScrollRef.value; if (el) el.scrollTop = el.scrollHeight })
}
async function runTerm(line: string) {
  const full = line.trim()
  if (!full) return
  const fix = correctShellLine(full)
  if (fix.fixed) termPush('out', `auto-fix: ${fix.fixes.join(', ')}`)
  const effective = fix.line || full
  termPush('in', '❯ ' + effective)
  termCwd.value = trackShellCwd(termCwd.value, effective)
  const res = await store.execShellLine(effective)
  termPush(res.ok ? 'out' : 'err', res.output || (res.ok ? 'ok' : 'failed'))
}
function submitTerm() { const v = termInput.value; termInput.value = ''; void runTerm(v) }

/** Tab completion for the mirror terminal (same live table as CYBSH). */
async function completeTerm() {
  const prefix = termInput.value
  if (!prefix.trim()) {
    termPush('out', 'Tab completes commands · subcommands (disk …) — type to narrow')
    return
  }
  let hits: string[] = []
  try {
    hits = await store.completeShellLine(prefix)
  } catch {
    hits = []
  }
  if (hits.length === 1) {
    termInput.value = hits[0].includes(' ') ? hits[0] : `${hits[0]} `
  } else if (hits.length > 1) {
    termPush('out', hits.join('   '))
    termPush('out', `${hits.length} candidates — keep typing`)
  } else {
    termPush('out', 'no candidates')
  }
}

/* ═══════════════ AI panel ═══════════════ */

const aiOpen = ref(!(mobileMedia?.matches ?? false))
function openMobileSideView(view: string) {
  aiOpen.value = false
  bottomOpen.value = false
  if (sideView.value === view && mobileExplorerOpen.value) mobileExplorerOpen.value = false
  else { sideView.value = view; mobileExplorerOpen.value = true }
}
function toggleMobileAgent() {
  aiOpen.value = !aiOpen.value
  mobileExplorerOpen.value = false
  if (aiOpen.value) bottomOpen.value = false
}
function toggleMobileTerminal() {
  bottomTab.value = 'term'
  bottomOpen.value = !bottomOpen.value
  mobileExplorerOpen.value = false
  if (bottomOpen.value) aiOpen.value = false
}
const promptInput = ref('')
const promptEl = ref<HTMLTextAreaElement | null>(null)
const threadRef = ref<HTMLElement | null>(null)
const openRows = ref<Set<string>>(new Set())
const attachFile = ref(true)
const attachSel = ref(false)
const keyInput = ref('')
const hasLocalKey = computed(() => {
  // localKeyRevision makes this re-evaluate when the Agent panel seals a key
  // into the shared in-memory vault (a non-reactive Map).
  void localKeyRevision.value
  if (!ai.chatConfigId.value) return false
  if (ai.localKeys.value[ai.chatConfigId.value]) return true
  return hasSharedKey(ai.chatConfigId.value)
})

function scrollThreadToBottom() {
  void nextTick(() => {
    const el = threadRef.value
    if (el) el.scrollTop = el.scrollHeight
  })
}

/** Row count + last-row content length: catches new messages AND a running
 * turn appending to the current row, so the newest message stays in view. */
const aiThreadSignature = computed(() => {
  const rows = ai.threadRows.value
  const last = rows[rows.length - 1]
  const lastLen = last
    ? last.kind === 'message'
      ? (last.message.content ?? '').length
      : JSON.stringify(last).length
    : 0
  return [rows.length, lastLen, ai.jobActive.value ? 1 : 0] as const
})

watch(aiThreadSignature, () => scrollThreadToBottom())
watch(() => ai.viewing.value?.id, () => scrollThreadToBottom())
watch(() => ai.configs.value.length, n => {
  if (n && !ai.chatConfigId.value) ai.chatConfigId.value = ai.configs.value[0].id
})

const canSend = computed(() => promptInput.value.trim() !== '' && ai.chatConfigId.value !== '')

/* ── voice dictation (code mode: symbols + camel/snake phrases) ── */
const voice = useVoiceInput('code')
let stopVoice: (() => void) | null = null
/** Which surface owns the live mic: the AI prompt or the editor buffer. */
const voiceTarget = ref<'prompt' | 'code' | null>(null)
const voiceCoding = computed(() => voice.listening.value && voiceTarget.value === 'code')
function stopVoiceAll() {
  stopVoice?.()
  stopVoice = null
  voiceTarget.value = null
}
function toggleVoice() {
  if (voice.listening.value) {
    stopVoiceAll()
    return
  }
  voiceTarget.value = 'prompt'
  stopVoice = voice.dictateInto(promptInput)
}
/**
 * Voice-code: dictate code syntax straight into the open file at the
 * cursor. Spoken symbols ("open paren", "camel case …", "nova linha")
 * run through the code-mode tables, the buffer updates + re-highlights,
 * and tree-sitter reparses (outline / intel / problems follow).
 */
function toggleVoiceCode() {
  if (voice.listening.value) {
    const wasCoding = voiceTarget.value === 'code'
    const tab = focusTab.value
    stopVoiceAll()
    // Stopping the editor mic parses immediately so intel/problems settle.
    if (wasCoding && tab) void reparse(tab)
    return
  }
  const tab = focusTab.value
  if (!tab) {
    store.notifyError('No file open', 'open a file first, then dictate')
    return
  }
  voiceTarget.value = 'code'
  const el = document.querySelector<HTMLTextAreaElement>(`.cs-pane[data-g="${activeGroup.value}"] textarea`)
  el?.focus()
  stopVoice = voice.dictateWith((ins) => insertVoiceCode(tab, ins.text))
}
/** Splice a voice transcript into `tab` at the live cursor/selection. */
function insertVoiceCode(tab: Tab, text: string) {
  const el = document.querySelector<HTMLTextAreaElement>(`.cs-pane[data-g="${activeGroup.value}"] textarea`)
  const s = el?.selectionStart ?? tab.content.length
  const en = el?.selectionEnd ?? s
  const chunk = (s > 0 && !/\s$/.test(tab.content.slice(0, s)) ? ' ' : '') + text
  tab.content = tab.content.slice(0, s) + chunk + tab.content.slice(en)
  const pos = s + chunk.length
  onEdit(tab)
  nextTick(() => {
    if (!el) return
    el.focus()
    el.selectionStart = el.selectionEnd = pos
    updateCursorFromDom()
  })
}
function activeFileCtx(): FileCtx[] {
  const tab = focusTab.value
  if (!tab) return []
  const out: FileCtx[] = []
  if (attachFile.value) out.push({ label: tab.label, path: tab.kind === 'wasm' ? tab.path : tab.label, language: tab.language, content: tab.content.slice(0, 6000), selection: attachSel.value && selection.value ? selection.value.slice(0, 2000) : undefined })
  return out
}
async function sendPrompt() {
  if (!canSend.value) return
  const v = promptInput.value
  promptInput.value = ''
  scrollThreadToBottom()
  await ai.send(v, activeFileCtx())
}
function quick(kind: 'explain' | 'review') {
  if (!focusTab.value) return
  const p = kind === 'explain'
    ? `Explain what this file does, section by section. Reference file:line. Be concise.`
    : `Review this code for bugs, edge cases and security issues. List findings with file:line, then propose minimal fixes.`
  void ai.send(p, activeFileCtx())
}
function lastCodeBlock(): string | null {
  const msgs = ai.viewing.value?.messages ?? []
  for (let i = msgs.length - 1; i >= 0; i--) {
    const c = msgs[i].content || ''
    // Closed fence first; fall back to a trailing unclosed fence (the model
    // sometimes stops before emitting the closer).
    const m = /```(?:\w+)?\n([\s\S]*?)```/.exec(c) ?? /```(?:\w+)?\n([\s\S]*)$/.exec(c)
    if (m) return m[1]
  }
  return null
}
function applyLastBlock() {
  const tab = focusTab.value
  const block = lastCodeBlock()
  if (!tab) { store.notifyError('No file open', 'open a file first'); return }
  if (!block) { store.notifyError('No code block', 'the agent has not emitted a fenced block yet'); return }
  // Insert at the cursor/selection — never overwrite the whole file.
  const clean = block.replace(/\n$/, '')
  const el = document.querySelector<HTMLTextAreaElement>(`.cs-pane[data-g="${activeGroup.value}"] textarea`)
  if (el) {
    const s = el.selectionStart ?? tab.content.length
    const en = el.selectionEnd ?? s
    tab.content = tab.content.slice(0, s) + clean + tab.content.slice(en)
    const pos = s + clean.length
    nextTick(() => { el.focus(); el.selectionStart = el.selectionEnd = pos })
  } else {
    tab.content = tab.content + (tab.content.endsWith('\n') || !tab.content ? '' : '\n') + clean + '\n'
  }
  onEdit(tab)
  store.notifySuccess('Applied agent code block — review then save')
}
function sealKey() {
  if (ai.chatConfigId.value && keyInput.value) {
    ai.sealLocalKey(ai.chatConfigId.value, keyInput.value)
    keyInput.value = ''
    store.notifySuccess('Key held in memory for this page only')
  }
}
function roleLabel(m: { role: string; toolName?: string | null }) {
  if (m.role === 'user') return 'YOU'
  if (m.role === 'tool') return `TOOL · ${m.toolName ?? ''}`
  if (m.role === 'assistant_tool') return 'AGENT · TOOLS'
  return 'AGENT'
}
function toolNameOf(m: { toolInput?: unknown }) {
  try {
    const arr = (m.toolInput ?? []) as Array<{ name?: string }>
    return arr.map(c => c.name ?? '?').join(', ')
  } catch { return '' }
}
function prettyTool(t: { name: string; input: unknown }) {
  try { return `${t.name} ${JSON.stringify(t.input ?? {}).slice(0, 500)}` } catch { return t.name }
}
const approvalInput = computed(() => {
  const p = ai.pendingApproval.value
  if (!p) return ''
  try {
    const text = JSON.stringify(p.input ?? {}, null, 1)
    return text.length > 600 ? text.slice(0, 600) + '…' : text
  } catch { return '' }
})
function toggleRow(key: string) {
  const next = new Set(openRows.value)
  if (next.has(key)) next.delete(key)
  else next.add(key)
  openRows.value = next
}
function fmtTok(n: number) {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`
  return String(n)
}

/* ═══════════════ editor pane (highlight overlay) ═══════════════ */

const EditorPane = defineComponent({
  name: 'EditorPane',
  props: {
    tab: { type: Object as PropType<Tab>, required: true },
    group: { type: String, required: true },
  },
  emits: ['cursor', 'edit', 'jump', 'selline'],
  setup(props, { emit }) {
    const lines = computed(() => props.tab.content.split('\n').length)
    // Highlight is O(n) regex over the whole file per keystroke — past this
    // size render plain escaped text so big logs don't freeze the pane.
    // `:key="tab.key"` on each EditorPane instance guarantees this view state
    // (scroll, textarea) remounts per file instead of leaking across tabs.
    const MAX_HL_BYTES = 64 * 1024
    const html = computed(() => (props.tab.content.length > MAX_HL_BYTES ? escapeHtml(props.tab.content) : highlightSyntax(props.tab.content, props.tab.language)))
    const taRef = ref<HTMLTextAreaElement | null>(null)
    const hlRef = ref<HTMLElement | null>(null)
    const guRef = ref<HTMLElement | null>(null)
    const onScroll = () => {
      const ta = taRef.value
      if (!ta) return
      if (hlRef.value) { hlRef.value.scrollTop = ta.scrollTop; hlRef.value.scrollLeft = ta.scrollLeft }
      if (guRef.value) guRef.value.scrollTop = ta.scrollTop
    }
    const cursor = (el?: HTMLTextAreaElement | null) => {
      const ta = el ?? taRef.value
      if (!ta) return
      const pos = ta.selectionStart ?? 0
      const before = props.tab.content.slice(0, pos)
      emit('cursor', before.split('\n').length, pos - before.lastIndexOf('\n'), ta.value.slice(ta.selectionStart ?? 0, ta.selectionEnd ?? 0))
    }
    const onKey = (e: KeyboardEvent) => {
      const ta = e.target as HTMLTextAreaElement
      if (e.key === 'Tab') {
        e.preventDefault()
        const s = ta.selectionStart ?? 0
        const en = ta.selectionEnd ?? 0
        props.tab.content = props.tab.content.slice(0, s) + '  ' + props.tab.content.slice(en)
        nextTick(() => { ta.selectionStart = ta.selectionEnd = s + 2; cursor(ta) })
        emit('edit')
      }
    }
    const onInput = (e: Event) => {
      const ta = e.target as HTMLTextAreaElement
      props.tab.content = ta.value
      emit('edit')
      cursor(ta)
    }
    // Wheel over the line numbers scrolls the code: the gutter itself is
    // overflow:hidden (no native scroll), so forward the delta to the
    // textarea and mirror it back — otherwise that 46px strip feels dead.
    const onGutterWheel = (e: WheelEvent) => {
      const ta = taRef.value
      if (!ta) return
      e.preventDefault()
      ta.scrollTop += e.deltaY
      ta.scrollLeft += e.deltaX
      onScroll()
    }
    // One div per line guarantees a 1:1 line-number mapping even when the
    // highlight layer wraps or the font metrics shift — a single
    // newline-joined text node collapses/misaligns in those cases.
    const gutterRows = computed(() => Array.from({ length: lines.value }, (_, i) => i + 1))
    return () =>
      h('div', { class: 'cs-pane', 'data-g': props.group }, [
        h('div', {
          class: 'cs-gutter',
          ref: guRef,
          title: 'Scroll to move · click a line to jump · double-click to select it',
          onWheel: onGutterWheel,
        }, gutterRows.value.map(n => h('div', {
          class: 'cs-gln',
          key: n,
          title: `Line ${n} — click to jump, double-click to select`,
          onClick: () => emit('jump', n),
          onDblclick: () => emit('selline', n),
        }, String(n)))),
        h('div', { class: 'cs-code' }, [
          h('pre', { class: 'cs-hl', ref: hlRef, 'aria-hidden': 'true' }, [h('code', { innerHTML: html.value })]),
          h('textarea', {
            ref: taRef,
            class: 'cs-input2',
            spellcheck: false,
            wrap: 'off',
            value: props.tab.content,
            onInput,
            onScroll,
            onKeydown: onKey,
            onKeyup: (e: KeyboardEvent) => cursor(e.target as HTMLTextAreaElement),
            onClick: (e: MouseEvent) => cursor(e.target as HTMLTextAreaElement),
            // Double-click word-selects natively; re-emit so the status bar
            // and @sel attach track the selected word immediately.
            onDblclick: (e: MouseEvent) => {
              const ta = e.target as HTMLTextAreaElement
              ta.focus()
              nextTick(() => cursor(ta))
            },
          }),
        ]),
      ])
  },
})

const rootRef = ref<HTMLElement | null>(null)

onMounted(() => {
  syncMobileLayout()
  mobileMedia?.addEventListener('change', syncMobileLayout)
  void refreshExplorer()
  void ai.ensureCatalog()
})
onBeforeUnmount(() => { mobileMedia?.removeEventListener('change', syncMobileLayout); window.clearTimeout(reparseTimer); if (explorerSyncTimer) window.clearInterval(explorerSyncTimer); stopVoice?.() })
</script>

<style scoped>
.cs { position: relative; display: flex; flex-direction: column; height: 100%; overflow: hidden; outline: none;
  background: var(--ui-surface);
  color: var(--ui-text); font-size: 12px; }
.cs-aurora { display: none; }
.cs-mobile-file, .cs-mobilebar { display: none; }

/* top */
.cs-top { display: flex; align-items: center; gap: 8px; padding: 7px 10px; background: var(--ui-glass);
  border-bottom: 1px solid var(--ui-hairline); backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate)); }
.cs-logo { display: flex; align-items: center; gap: 6px; color: var(--ui-text-2); font-size: 12px; font-weight: 600; }
.cs-transport, .cs-engine { font-size: 10px; font-weight: 500; color: var(--ui-text-3);
  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); padding: 2px 7px; border-radius: 99px; }
.cs-spacer { flex: 1; }
.cs-btn { display: inline-flex; align-items: center; gap: 5px; padding: 5px 11px; border-radius: 9px; cursor: pointer;
  font-size: 11px; font-weight: 700; color: var(--ui-text-2); background: color-mix(in srgb, var(--ui-text) 5%, transparent);
  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); white-space: nowrap; }
.cs-btn:hover:not(:disabled) { color: var(--ui-text); border-color: var(--ui-border-strong); }
.cs-btn:disabled { opacity: .4; } .cs-btn.on { color: var(--ui-text); border-color: var(--ui-border-strong); background: color-mix(in srgb, var(--ui-text) 8%, transparent); }
.cs-btn.xs { padding: 3px 9px; font-size: 10px; }
.cs-btn.primary { background: var(--ui-accent); color: var(--ui-on-accent); border-color: transparent; }
.cs-btn.danger { color: var(--ui-danger); border-color: color-mix(in srgb, var(--ui-danger) 45%, transparent); }
.cs-ibtn { display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 24px; border-radius: 7px;
  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); background: transparent; color: var(--ui-text-3); cursor: pointer; flex-shrink: 0; }
.cs-ibtn:hover:not(:disabled) { color: var(--ui-text); background: color-mix(in srgb, var(--ui-text) 7%, transparent); }
.cs-ibtn:disabled { opacity: .3; }
.cs-input, .cs-select { background: color-mix(in srgb, var(--ui-text) 4%, transparent); border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent);
  color: var(--ui-text); font-size: 11.5px; padding: 5px 8px; border-radius: 8px; outline: none; min-width: 0; }
.cs-input:focus, .cs-select:focus { border-color: var(--ui-accent); box-shadow: var(--ui-focus-ring); }
.cs-input.sm { width: 64px; }
.cs-select.xs { padding: 3px 6px; font-size: 10px; font-weight: 700; }
.cs-voice-group { position: relative; display: inline-flex; flex-shrink: 0; }
.cs-voice-pop {
  position: absolute; bottom: calc(100% + 6px); right: 0;
  display: flex; gap: 4px; padding: 4px;
  border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: 9px;
  background: color-mix(in srgb, var(--ui-glass) 92%, transparent);
  box-shadow: var(--ui-shadow-2); z-index: 6; white-space: nowrap;
  opacity: 0; visibility: hidden; transform: translateY(4px); pointer-events: none;
  transition: opacity var(--ui-dur-fast) var(--ui-ease-out), transform var(--ui-dur-fast) var(--ui-ease-out), visibility var(--ui-dur-fast);
}
.cs-voice-group:hover .cs-voice-pop, .cs-voice-group:focus-within .cs-voice-pop { opacity: 1; visibility: visible; transform: none; pointer-events: auto; }
.cs-voice-opt {
  font: inherit; font-size: 10px; font-weight: 700; padding: 3px 8px; border-radius: 7px;
  border: 1px solid transparent; background: transparent; color: color-mix(in srgb, var(--ui-text) 70%, transparent);
  cursor: pointer; white-space: nowrap;
}
.cs-voice-opt:hover { color: var(--ui-text); background: color-mix(in srgb, var(--ui-text) 7%, transparent); }
.cs-voice-opt.on { color: var(--ui-accent); background: var(--ui-accent-softer); border-color: color-mix(in srgb, var(--ui-accent) 40%, transparent); }
.cs-link { background: none; border: none; color: var(--ui-accent); cursor: pointer; font-weight: 700; }

/* mid */
.cs-mid { flex: 1; display: flex; min-height: 0; position: relative; }
.cs-activity { width: 44px; flex-shrink: 0; display: flex; flex-direction: column; align-items: center; gap: 4px; padding: 8px 0;
  border-right: 1px solid var(--ui-hairline); background: color-mix(in srgb, var(--ui-surface) 60%, transparent); }
.cs-activity button { display: flex; width: 34px; height: 34px; align-items: center; justify-content: center; border-radius: 10px;
  background: transparent; border: none; border-left: 2px solid transparent; color: var(--ui-text-3); cursor: pointer; }
.cs-activity button:hover { color: var(--ui-text); background: color-mix(in srgb, var(--ui-text) 7%, transparent); }
.cs-activity button.active { color: var(--ui-text); border-left-color: var(--ui-accent); background: color-mix(in srgb, var(--ui-text) 9%, transparent); }

/* sidebar */
.cs-side { width: 232px; flex-shrink: 0; display: flex; flex-direction: column; gap: 6px; padding: 8px;
  border-right: 1px solid var(--ui-hairline); background: color-mix(in srgb, var(--ui-surface) 55%, transparent); min-height: 0; }
.cs-drawer-backdrop { display: none; }
.cs-side-h { display: flex; align-items: center; font-size: 10px; font-weight: 600; color: var(--ui-text-3); padding: 2px 4px; }
.cs-tree { flex: 1; overflow-y: auto; min-height: 0; overscroll-behavior: contain; touch-action: pan-x pan-y; }
.cs-searchwrap { flex: 1; overflow-y: auto; min-height: 0; overscroll-behavior: contain; touch-action: pan-x pan-y; }
.cs-searchwrap :deep(.panel-search) { padding: 4px 0; }
.cs-trow { display: flex; align-items: center; gap: 6px; width: 100%; padding: 5px 6px; border-radius: 8px; cursor: pointer;
  background: transparent; border: none; color: var(--ui-text-2); font-size: 12px; text-align: left; }
.cs-trow:hover { background: color-mix(in srgb, var(--ui-text) 6%, transparent); color: var(--ui-text); }
.cs-trow.active { background: var(--ui-accent); color: var(--ui-on-accent); }
.blurred .cs-trow.active { background: color-mix(in srgb, var(--ui-text) 14%, transparent); color: var(--ui-text); }
.cs-trow.dir { color: var(--ui-text-2); font-weight: 600; }
.cs-trow.col { flex-direction: column; align-items: flex-start; gap: 1px; }
.cs-tbranch { display: flex; flex-direction: column; }
.cs-caret { width: 10px; color: var(--ui-text-3); font-size: 9px; flex-shrink: 0; }
.cs-ticon { flex-shrink: 0; }
.cs-kind { font-size: 10px; font-weight: 500; color: var(--ui-text-3); min-width: 52px; }
.cs-snip { font-size: 10px; }
.cs-empty { padding: 10px; font-size: 11px; text-align: center; }
.cs-pathrow { display: flex; align-items: center; gap: 6px; }
.cs-intel-src { display: flex; gap: 4px; }
.cs-intel-src button { flex: 1; padding: 4px 0; font-size: 11px; font-weight: 500;
  background: transparent; border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); color: var(--ui-text-3); border-radius: 8px; cursor: pointer; }
.cs-intel-src button.on { color: var(--ui-text); border-color: var(--ui-border-strong); background: color-mix(in srgb, var(--ui-text) 8%, transparent); }
.cs-intel-meta { display: flex; flex-direction: column; gap: 2px; font-size: 9.5px; padding: 2px 4px; }
.cs-chips { display: flex; flex-wrap: wrap; gap: 4px; }
.cs-chips button { font-size: 10px; font-weight: 500; padding: 2px 8px; border-radius: 99px; cursor: pointer;
  border: 1px solid color-mix(in srgb, var(--ui-text) 12%, transparent); background: transparent; color: var(--ui-text-3); }
.cs-chips button.on { color: var(--ui-on-accent); border-color: transparent; background: var(--ui-accent); }
.cs-intel-paste { resize: vertical; min-height: 90px; font-family: var(--ui-font-mono); font-size: 11px; }
.cs-intel-preview { border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: var(--ui-radius-lg); max-height: 220px; overflow-y: auto;
  overscroll-behavior: contain; font-family: var(--ui-font-mono); font-size: 10.5px; line-height: 1.5; }
.cs-intel-pline { display: flex; gap: 8px; padding: 0 8px; }
.cs-intel-pline.hit { background: var(--ui-accent-softer); box-shadow: inset 2px 0 0 var(--ui-accent); }
.cs-intel-pln { width: 30px; flex-shrink: 0; text-align: right; color: color-mix(in srgb, var(--ui-text) 30%, transparent); user-select: none; }
.cs-intel-pcode { flex: 1; white-space: pre; overflow-x: auto; }
.mono { font-family: var(--ui-font-mono); font-size: 10.5px; } .dim { color: var(--ui-text-3); }
.truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

/* editor column */
.cs-edcol { flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; }
.cs-groups { flex: 1; display: flex; min-height: 0; }
.cs-groups.split .cs-group { width: 50%; }
.cs-group { flex: 1; display: flex; flex-direction: column; min-width: 0; min-height: 0; }
.cs-group + .cs-group { border-left: 1px solid var(--ui-hairline); }
.cs-tabs { display: flex; overflow-x: auto; overscroll-behavior: contain; touch-action: pan-x pan-y;
  border-bottom: 1px solid var(--ui-hairline); background: color-mix(in srgb, var(--ui-surface) 65%, transparent); flex-shrink: 0; }
.cs-tab { display: flex; align-items: center; gap: 6px; padding: 7px 8px 7px 12px; font-size: 11.5px; cursor: pointer; white-space: nowrap;
  color: var(--ui-text-3); border-right: 1px solid var(--ui-hairline); max-width: 190px; }
.cs-tab.active { color: var(--ui-text); background: var(--ui-surface-2); box-shadow: inset 0 2px 0 var(--ui-accent); }
.cs-dot { color: var(--ui-warning); font-size: 8px; }
.cs-x { display: inline-flex; background: none; border: none; color: inherit; opacity: .5; cursor: pointer; padding: 2px; }
.cs-x:hover { opacity: 1; color: var(--ui-danger); }
.cs-welcome { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; color: var(--ui-text-3); }
.cs-welcome p { font-weight: 800; color: var(--ui-text-2); margin: 0; }
.cs-pane { flex: 1; display: flex; min-height: 0; }
.cs-gutter { width: 46px; flex-shrink: 0; padding: 10px 6px 10px 0; text-align: right; color: color-mix(in srgb, var(--ui-text) 30%, transparent);
  font-family: var(--ui-font-mono); font-size: 12px; line-height: 1.6; overflow: hidden; user-select: none; cursor: default; }
.cs-gln { height: calc(12px * 1.6); line-height: 1.6; white-space: nowrap; border-radius: 4px; padding-right: 2px; cursor: pointer; }
.cs-gln:hover { color: var(--ui-accent); background: color-mix(in srgb, var(--ui-accent) 12%, transparent); }
.cs-code { flex: 1; position: relative; min-width: 0; }
.cs-hl, .cs-input2 { margin: 0; padding: 10px 12px; font-family: var(--ui-font-mono); font-size: 12px; line-height: 1.6; white-space: pre; tab-size: 2; }
.cs-hl { position: absolute; inset: 0; overflow: hidden; pointer-events: none; color: var(--ui-text); }
.cs-input2 { position: absolute; inset: 0; width: 100%; height: 100%; background: transparent; border: none; outline: none; resize: none;
  color: transparent; caret-color: var(--ui-accent); overflow: auto; }
/* The text itself is transparent (the highlight layer shows through), so the
   native selection needs translucency — otherwise a double-clicked word
   becomes an unreadable solid block instead of visibly selected code. */
.cs-input2::selection { background: color-mix(in srgb, var(--ui-accent) 32%, transparent); color: transparent; }
.cs-input2:focus { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--ui-accent) 35%, transparent); }

/* find */
.cs-find { display: flex; align-items: center; gap: 6px; padding: 6px 10px; border-top: 1px solid color-mix(in srgb, var(--ui-text) 8%, transparent);
  background: var(--ui-surface); flex-shrink: 0; flex-wrap: wrap; }
.cs-find .cs-input { width: 150px; }

/* bottom */
.cs-bottom { height: 190px; flex-shrink: 0; display: flex; flex-direction: column; border-top: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); background: var(--ui-surface); }
.cs-btabs { display: flex; gap: 2px; padding: 4px 8px 0; }
.cs-btabs button { padding: 5px 12px; font-size: 11px; font-weight: 500; background: transparent; border: none;
  border-bottom: 2px solid transparent; color: var(--ui-text-3); cursor: pointer; }
.cs-btabs button.active { color: var(--ui-text); border-bottom-color: var(--ui-accent); }
.cs-termwrap { flex: 1; display: flex; flex-direction: column; min-height: 0; }
.cs-term { flex: 1; min-height: 0; overflow-y: auto; overscroll-behavior: contain; touch-action: pan-x pan-y;
  scrollbar-gutter: stable; padding: 6px 10px; font-family: var(--ui-font-mono); font-size: 10.5px; }
.cs-term::-webkit-scrollbar, .cs-thread::-webkit-scrollbar, .cs-tree::-webkit-scrollbar,
.cs-problems::-webkit-scrollbar, .cs-tabs::-webkit-scrollbar, .cs-input2::-webkit-scrollbar { height: 8px; width: 10px; }
.cs-term::-webkit-scrollbar-thumb, .cs-thread::-webkit-scrollbar-thumb, .cs-tree::-webkit-scrollbar-thumb,
.cs-problems::-webkit-scrollbar-thumb, .cs-tabs::-webkit-scrollbar-thumb, .cs-input2::-webkit-scrollbar-thumb {
  background: color-mix(in srgb, var(--ui-text) 28%, transparent);
  border-radius: var(--ui-radius-full); border: 2px solid transparent; background-clip: content-box; }
.cs-term::-webkit-scrollbar-thumb:hover, .cs-thread::-webkit-scrollbar-thumb:hover, .cs-tree::-webkit-scrollbar-thumb:hover,
.cs-problems::-webkit-scrollbar-thumb:hover, .cs-tabs::-webkit-scrollbar-thumb:hover, .cs-input2::-webkit-scrollbar-thumb:hover {
  background: color-mix(in srgb, var(--ui-text) 42%, transparent); background-clip: content-box; border: 2px solid transparent; }
.cs-terml.in { color: var(--ui-accent); font-weight: 700; } .cs-terml.out { color: var(--ui-text-2); white-space: pre-wrap; } .cs-terml.err { color: var(--ui-danger); white-space: pre-wrap; }
.cs-termin { display: flex; gap: 7px; padding: 6px 10px; border-top: 1px solid var(--ui-hairline); color: var(--ui-accent); }
.cs-termin input { flex: 1; background: transparent; border: none; outline: none; color: var(--ui-text); font-size: 11.5px; }
.cs-problems { flex: 1; min-height: 0; overflow-y: auto; overscroll-behavior: contain; touch-action: pan-x pan-y; padding: 4px 8px; }
.cs-prow { display: flex; gap: 8px; align-items: center; padding: 5px 8px; border-radius: 8px; cursor: pointer; font-size: 11.5px; }
.cs-prow:hover { background: var(--ui-accent-softer); }
.cs-pkind { font-size: 8.5px; font-weight: 800; padding: 1px 7px; border-radius: 99px; background: color-mix(in srgb, var(--ui-text) 8%, transparent); }
.cs-pkind.warn { color: var(--ui-warning); } .cs-pkind.info { color: var(--ui-info); }

/* status */
.cs-status { display: flex; align-items: center; gap: 12px; padding: 4px 10px; font-size: 10px; font-family: var(--ui-font-mono);
  border-top: 1px solid var(--ui-hairline); background: color-mix(in srgb, var(--ui-surface) 80%, transparent); flex-shrink: 0; overflow: hidden; white-space: nowrap; }
.cs-warn { color: var(--ui-warning); font-weight: 700; }
.cs-voicestat { display: inline-flex; align-items: center; gap: 4px; color: var(--ui-danger); font-weight: 700; max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.cs-aistat { display: inline-flex; align-items: center; gap: 4px; color: var(--ui-text-3); max-width: 40%; overflow: hidden; text-overflow: ellipsis; }
.cs-aistat.run { color: var(--ui-accent); font-weight: 700; }

/* AI panel */
.cs-ai { width: 320px; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0;
  border-left: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); background: var(--ui-glass);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate)); }
.cs-ai-h { display: flex; align-items: center; gap: 7px; padding: 9px 10px; border-bottom: 1px solid color-mix(in srgb, var(--ui-text) 8%, transparent); font-size: 12px; color: var(--ui-accent); }
.cs-ai-cfg { display: flex; gap: 6px; padding: 8px 10px 4px; }
.cs-ai-cfg .cs-select { flex: 1; }
.cs-ai-note { margin: 6px 10px 0; padding: 8px 10px; font-size: 11px; border-radius: 10px; color: var(--ui-text-2);
  border: 1px dashed var(--ui-border-strong); background: color-mix(in srgb, var(--ui-text) 3%, transparent); }
.cs-ai-note form { display: flex; gap: 6px; margin-top: 6px; }
.cs-ai-note .cs-input { flex: 1; }
.cs-ai-meta { padding: 6px 10px 0; font-size: 10px; }
.cs-ctxbar { height: 4px; border-radius: 99px; background: color-mix(in srgb, var(--ui-text) 10%, transparent); margin-top: 4px; overflow: hidden; }
.cs-ctxbar i { display: block; height: 100%; background: var(--ui-success); border-radius: 99px; }
.cs-ctxbar i.warn { background: var(--ui-warning); } .cs-ctxbar i.bad { background: var(--ui-danger); }
.cs-thread { flex: 1; overflow-y: auto; overscroll-behavior: contain; touch-action: pan-x pan-y;
  padding: 8px 10px; display: flex; flex-direction: column; gap: 8px; min-height: 0; }
.cs-msg { padding: 7px 9px; border-radius: 11px; border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); background: color-mix(in srgb, var(--ui-text) 2.5%, transparent); }
.cs-msg.r-user { background: var(--ui-accent-softer); border-color: color-mix(in srgb, var(--ui-accent) 30%, transparent); }
.cs-role { font-size: 10px; font-weight: 600; color: var(--ui-text-3); margin-bottom: 3px; }
.cs-body { font-size: 11.5px; white-space: pre-wrap; word-break: break-word; }
.cs-md { font-size: 11.5px; word-break: break-word; }
.cs-md :deep(pre) { background: color-mix(in srgb, var(--ui-text) 5%, transparent); border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: var(--ui-radius-lg); padding: 7px; overflow-x: auto; font-size: 10.5px; }
.cs-md :deep(code) { font-family: var(--ui-font-mono); font-size: 10.5px; }
.cs-md :deep(p) { margin: 4px 0; } .cs-md :deep(ul) { margin: 4px 0; padding-left: 16px; }
.cs-toolblock { font-family: var(--ui-font-mono); font-size: 10px; color: var(--ui-text-3); }
.cs-tgroup { display: flex; flex-direction: column; gap: 4px; }
.cs-trow2 { border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent); border-radius: var(--ui-radius-lg); overflow: hidden; }
.cs-thead { display: flex; align-items: center; gap: 6px; width: 100%; padding: 5px 8px; background: transparent; border: none;
  color: var(--ui-text-2); cursor: pointer; font-size: 11px; text-align: left; }
.cs-thead:hover { background: color-mix(in srgb, var(--ui-text) 6%, transparent); }
.cs-tstate { margin-left: auto; font-size: 10px; font-weight: 500; color: var(--ui-text-3); }
.is-running .cs-tstate { color: var(--ui-accent); } .is-error .cs-tstate, .is-denied .cs-tstate { color: var(--ui-danger); }
.cs-tdetail { padding: 4px 8px 8px; } .cs-tdetail pre { font-size: 10px; white-space: pre-wrap; word-break: break-word; margin: 4px 0; color: var(--ui-text-2); }
.cs-tdetail pre.res { color: var(--ui-text-3); max-height: 160px; overflow-y: auto; }
.cs-approval { margin: 0 10px; padding: 9px 10px; border-radius: 12px; border: 1px solid color-mix(in srgb, var(--ui-warning) 35%, transparent);
  background: color-mix(in srgb, var(--ui-warning) 7%, transparent); display: flex; flex-direction: column; gap: 6px; }
.cs-apph { font-size: 11px; font-weight: 600; display: flex; gap: 5px; align-items: center; color: var(--ui-text); }
.cs-apptext { font-size: 11px; } .cs-appinput { font-size: 10px; max-height: 90px; overflow-y: auto; margin: 0; color: var(--ui-text-2); }
.cs-approw { display: flex; gap: 6px; }
.cs-quick { display: flex; gap: 4px; padding: 6px 10px 0; flex-wrap: wrap; }
.cs-quick button { font-size: 10px; font-weight: 500; padding: 3px 9px; border-radius: 99px; cursor: pointer;
  border: 1px solid color-mix(in srgb, var(--ui-text) 12%, transparent); background: transparent; color: var(--ui-text-3); }
.cs-quick button.on { color: var(--ui-on-accent); border-color: transparent; background: var(--ui-accent); }
.cs-quick button:hover:not(:disabled) { color: var(--ui-text); } .cs-quick button:disabled { opacity: .4; }
.cs-queue { display: flex; flex-direction: column; gap: 4px; padding: 6px 10px 0; }
.cs-qitem { display: flex; align-items: center; gap: 6px; font-size: 10px; color: var(--ui-text-3);
  border: 1px dashed var(--ui-border-strong); border-radius: 8px; padding: 3px 4px 3px 9px; }
.cs-qitem .truncate { flex: 1; min-width: 0; }
.cs-prompt { display: flex; gap: 6px; padding: 8px 10px; align-items: flex-end; }
.cs-prompt textarea { flex: 1; background: color-mix(in srgb, var(--ui-text) 4%, transparent); border: 1px solid color-mix(in srgb, var(--ui-text) 10%, transparent);
  border-radius: 10px; color: var(--ui-text); font-size: 11.5px; padding: 7px 9px; outline: none; resize: none; }
.cs-prompt textarea:focus { border-color: var(--ui-accent); box-shadow: var(--ui-focus-ring); }
.cs-jobline { padding: 0 10px 4px; font-size: 10px; color: var(--ui-text-3); }
.cs-err { margin: 0 10px 8px; font-size: 10px; color: var(--ui-danger); }

@media (max-width: 1100px) { .cs-ai { width: 270px; } .cs-side { width: 190px; } }
@media (max-width: 860px) {
  .cs { min-height: 0; }
  .cs-top { display: flex; flex-wrap: nowrap; gap: 8px; padding: 7px 9px; }
  .cs-top .cs-logo b { display: none; }
  .cs-mobile-file { display: flex; align-items: center; gap: 6px; flex: 1; min-width: 0; color: var(--ui-text); font-size: 12px; }
  .cs-mobile-file b { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cs-mobile-file span { flex: 0 0 auto; color: var(--ui-warning); }
  .cs-transport, .cs-engine { display: none; }
  .cs-top .cs-btn[title="Split editor right"], .cs-top > .cs-voice-group,
  .cs-top .cs-btn[title="Toggle panel (Ctrl+`)"] { display: none; }
  .cs-top .cs-btn { min-height: 40px; padding: 7px 9px; }
  .cs-top .cs-btn-label { display: none; }
  .cs-top .cs-btn[title="Toggle AI panel (Ctrl+G)"] { min-width: 42px; justify-content: center; color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 36%, transparent); }
  .cs-mobilebar { display: flex; align-items: stretch; gap: 3px; flex: 0 0 auto; overflow-x: auto; padding: 4px 6px; border-bottom: 1px solid var(--ui-hairline); background: var(--ui-surface); scrollbar-width: none; }
  .cs-mobilebar::-webkit-scrollbar { display: none; }
  .cs-mobilebar button { display: flex; flex: 1 0 48px; min-width: 48px; min-height: 46px; flex-direction: column; align-items: center; justify-content: center; gap: 2px; padding: 3px 4px; border: 1px solid transparent; border-radius: 11px; background: transparent; color: var(--ui-text-3); font: inherit; font-size: 9px; text-transform: capitalize; white-space: nowrap; }
  .cs-mobilebar button.active { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 28%, transparent); background: var(--ui-accent-softer); }
  .cs-mid { flex: 1; min-height: 0; overflow: hidden; }
  .cs-activity { display: none; }
  .cs-side { display: none; position: absolute; z-index: 31; inset: 0 12px 0 0; width: auto; max-width: none; padding: 10px 10px max(10px, env(safe-area-inset-bottom)); border: 1px solid var(--ui-border); border-radius: 0 18px 18px 0; background: var(--ui-surface-2); box-shadow: var(--ui-shadow-3); overflow: hidden; }
  .cs-side.is-mobile-open { display: flex; }
  .cs-drawer-backdrop { display: block; position: absolute; z-index: 30; inset: 0; width: 100%; height: 100%; border: 0; padding: 0; background: color-mix(in srgb, #000 32%, transparent); cursor: pointer; }
  .cs-drawer-close, .cs-side .cs-ibtn { min-width: 44px; min-height: 44px; }
  .cs-side-h { min-height: 44px; }
  .cs-input, .cs-select { min-height: 44px; font-size: 16px; }
  .cs-trow { min-height: 44px; padding: 8px 7px; }
  .cs-tabs { min-height: 44px; }
  .cs-tab { min-height: 44px; padding: 8px 8px 8px 12px; }
  .cs-x { width: 44px; height: 44px; margin: -8px -8px -8px 0; justify-content: center; }
  .cs-edcol { position: relative; flex: 1; min-width: 0; min-height: 0; }
  .cs-groups.split { flex-direction: column; }
  .cs-groups.split .cs-group { width: 100%; min-height: 0; }
  .cs-input2, .cs-hl { padding-bottom: calc(10px + env(safe-area-inset-bottom)); font-size: 13px !important; line-height: 1.6; }
  .cs-status { gap: 6px; padding: 5px 8px; font-size: 9px; }
  .cs-aistat { max-width: 28%; }
  .cs-find { padding: 8px; gap: 6px; }
  .cs-find .cs-input { flex: 1 1 120px; width: auto; min-width: 0; }
  .cs-bottom { position: absolute; z-index: 15; inset: auto 0 31px; height: min(34dvh, 280px); border: 1px solid var(--ui-border); border-radius: 18px 18px 0 0; box-shadow: var(--ui-shadow-3); }
  .cs-termin input { min-height: 44px; font-size: 16px; }
  .cs-btabs button { min-height: 44px; }
  .cs-ai { display: flex; position: absolute; z-index: 25; inset: 6px; width: auto; max-width: none; border: 1px solid var(--ui-border); border-radius: 20px; background: var(--ui-surface-2); box-shadow: var(--ui-shadow-3); backdrop-filter: none; }
  .cs-ai-h { min-height: 48px; padding: 8px 10px; }
  .cs-ai-cfg { align-items: center; padding: 7px 10px 4px; }
  .cs-ai-cfg .cs-select { min-width: 0; }
  .cs-ai-note { font-size: 12px; }
  .cs-thread { padding: 10px; }
  .cs-msg { padding: 9px 10px; }
  .cs-body, .cs-md { font-size: 13px; }
  .cs-md :deep(pre), .cs-md :deep(code) { font-size: 12px; }
  .cs-approval { max-height: 28dvh; overflow-y: auto; }
  .cs-quick { flex-wrap: nowrap; overflow-x: auto; padding: 6px 10px 0; }
  .cs-quick button { flex: 0 0 auto; min-height: 40px; }
  .cs-prompt { display: flex; flex-wrap: wrap; gap: 7px; padding: 8px 10px max(8px, env(safe-area-inset-bottom)); }
  .cs-prompt textarea { flex: 1 0 100%; width: 100%; min-height: 72px; max-height: 120px; box-sizing: border-box; font-size: 16px !important; }
  .cs-prompt > .cs-voice-group { margin-right: auto; }
  .cs-prompt > .cs-btn { min-height: 44px; }
  .cs-prompt > .cs-btn.primary { margin-left: auto; }
  .cs-prompt > .cs-btn.danger { margin-left: 0; }
  .hide-mid { display: none; }
}
@media (prefers-reduced-motion: reduce) {
  .cs-voice-pop { transition: none; }
}
</style>

<style>
.sx-k { color: var(--ui-accent); font-weight: 700; }
.sx-s { color: color-mix(in srgb, var(--ui-info) 80%, var(--ui-text)); }
.sx-c { color: color-mix(in srgb, var(--ui-text) 35%, transparent); font-style: italic; }
.sx-n { color: color-mix(in srgb, var(--ui-warning) 80%, var(--ui-text)); }
</style>
