<!-- CyberManju OS — FileManager (src/components/FileManager.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue, @vueuse. -->
<!-- Contracts: signals: engine. -->
<template>
  <div ref="rootRef" class="fm" :class="{ 'fm-drop': isOverDropZone }" :style="{ '--fm-zoom': zoom }" tabindex="0" @keydown="onKeydown">
    <div class="fm-aurora" aria-hidden="true" />
    <div class="fm-grain" aria-hidden="true" />

    <!-- ══ TOP COMMAND BAR ══ -->
    <header class="fm-top">
      <div class="fm-nav">
        <button class="fm-ibtn" title="Back" aria-label="Back" :disabled="!canBack" @click="goBack"><AppIcon name="solar:alt-arrow-left-bold" :size="15" /></button>
        <button class="fm-ibtn" title="Forward" aria-label="Forward" :disabled="!canFwd" @click="goFwd"><AppIcon name="solar:alt-arrow-right-bold" :size="15" /></button>
        <button class="fm-ibtn" title="Up one level" aria-label="Up" @click="goUp"><AppIcon name="solar:alt-arrow-up-bold" :size="15" /></button>
        <button class="fm-ibtn" title="Home" aria-label="Home" @click="goHome"><AppIcon name="solar:house-bold" :size="15" /></button>
        <button class="fm-ibtn" title="Refresh" aria-label="Refresh" @click="refresh"><AppIcon name="solar:refresh-bold" :size="15" /></button>
      </div>

      <nav class="fm-crumbs" aria-label="Breadcrumb" title="Click a crumb to jump · click the path to edit (Alt+L)">
        <template v-if="crumbEdit">
          <input
            ref="crumbInputRef"
            v-model="crumbDraft"
            class="fm-locedit mono"
            aria-label="Location (type a path, Enter to go)"
            spellcheck="false"
            @keydown.enter="commitCrumbEdit"
            @keydown.esc.stop="crumbEdit = false"
            @blur="crumbEdit = false"
          />
        </template>
        <template v-else>
          <button
            v-for="(c, i) in crumbs"
            :key="i"
            class="fm-crumb"
            :class="{ active: i === crumbs.length - 1 }"
            :title="c.path"
            @click="i === crumbs.length - 1 ? startCrumbEdit() : jumpCrumb(c.path)"
          >
            <AppIcon v-if="i === 0" name="solar:ssd-square-bold" :size="12" />
            <AppIcon v-else-if="i === crumbs.length - 1" name="solar:folder-open-bold" :size="12" />
            <AppIcon v-else name="solar:folder-bold" :size="12" />
            <span>{{ c.label }}</span>
            <span v-if="i < crumbs.length - 1" class="fm-crumb-sep">/</span>
          </button>
        </template>
      </nav>

      <div class="fm-search">
        <AppIcon name="solar:magnifier-bold" :size="13" />
        <input v-model="filterQuery" placeholder="Filter + ⏎ global search…" aria-label="Filter files" @keydown.enter="globalSearch" />
        <button v-if="filterQuery" class="fm-x" title="Clear" aria-label="Clear filter" @click="filterQuery = ''"><AppIcon name="solar:close-bold" :size="11" /></button>
      </div>

      <div v-if="!isMobileView" class="fm-viewswitch" role="tablist" aria-label="View mode">
        <button
          v-for="v in viewModes"
          :key="v.id"
          role="tab"
          :aria-selected="view === v.id"
          class="fm-vbtn"
          :class="{ active: view === v.id }"
          :title="v.hint"
          @click="setView(v.id)"
        >
          <AppIcon :name="v.icon" :size="14" />
        </button>
      </div>

      <div class="fm-topactions">
        <button v-if="isMobileView" class="fm-pill ghost fm-mobile-more" type="button" :aria-label="mobileToolsOpen ? 'Close tools and actions' : 'Open tools and actions'" :aria-expanded="mobileToolsOpen" @click="mobileToolsOpen = !mobileToolsOpen">
          <AppIcon name="solar:widget-5-bold" :size="15" /> <span>{{ mobileToolsOpen ? 'Close tools' : 'Tools & actions' }}</span>
        </button>
        <template v-else>
          <button class="fm-pill" title="New folder" @click="dlgFolder = true"><AppIcon name="solar:add-folder-bold" :size="13" /> <span>New</span></button>
          <button class="fm-pill" title="Upload / new file" @click="onUploadClick"><AppIcon name="solar:upload-bold" :size="13" /> <span>File</span></button>
          <button class="fm-pill ghost" :class="{ on: termOpen }" title="Toggle inline terminal (cybsh here)" @click="toggleTerm"><AppIcon name="solar:file-terminal-bold" :size="13" /> <span>Terminal</span></button>
          <button class="fm-pill ghost" :class="{ on: inspectorOpen }" title="Toggle inspector" @click="toggleInspectorPanel"><AppIcon name="solar:info-circle-bold" :size="13" /> <span>Info</span></button>
        </template>
      </div>

      <div v-if="isMobileView && mobileToolsOpen" class="fm-mobile-tools-menu" role="group" aria-label="File Manager tools" @keydown.esc.stop="mobileToolsOpen = false">
        <div class="fm-mobile-tools-head"><strong>Tools &amp; actions</strong><button class="fm-ibtn" type="button" aria-label="Close tools" @click="mobileToolsOpen = false"><AppIcon name="solar:close-bold" :size="16" /></button></div>
        <div class="fm-mobile-tools-group">
          <span class="fm-mobile-tools-label">Navigate</span>
          <button class="fm-mobile-tool" type="button" :disabled="!canBack" @click="goBack(); mobileToolsOpen = false"><AppIcon name="solar:alt-arrow-left-bold" :size="17" /> Back</button>
          <button class="fm-mobile-tool" type="button" :disabled="!canFwd" @click="goFwd(); mobileToolsOpen = false"><AppIcon name="solar:alt-arrow-right-bold" :size="17" /> Forward</button>
          <button class="fm-mobile-tool" type="button" @click="goUp(); mobileToolsOpen = false"><AppIcon name="solar:alt-arrow-up-bold" :size="17" /> Up one level</button>
          <button class="fm-mobile-tool" type="button" @click="goHome(); mobileToolsOpen = false"><AppIcon name="solar:house-bold" :size="17" /> Vault home</button>
          <button class="fm-mobile-tool" type="button" @click="refresh(); mobileToolsOpen = false"><AppIcon name="solar:refresh-bold" :size="17" /> Refresh files</button>
        </div>
        <div class="fm-mobile-tools-group">
          <span class="fm-mobile-tools-label">Create and browse</span>
          <button class="fm-mobile-tool" type="button" @click="dlgFolder = true; mobileToolsOpen = false"><AppIcon name="solar:add-folder-bold" :size="17" /> New folder</button>
          <button class="fm-mobile-tool" type="button" @click="onUploadClick(); mobileToolsOpen = false"><AppIcon name="solar:upload-bold" :size="17" /> Upload or new file</button>
          <button class="fm-mobile-tool" type="button" @click="toggleSidePanel(); mobileToolsOpen = false"><AppIcon name="solar:folder-bold" :size="17" /> Browse folders</button>
        </div>
        <div class="fm-mobile-tools-group">
          <span class="fm-mobile-tools-label">Workspace</span>
          <button class="fm-mobile-tool" type="button" :class="{ on: termOpen }" @click="toggleTerm(); mobileToolsOpen = false"><AppIcon name="solar:file-terminal-bold" :size="17" /> {{ termOpen ? 'Close terminal' : 'Open terminal' }}</button>
          <button class="fm-mobile-tool" type="button" :class="{ on: mobileInspectorOpen }" @click="toggleInspectorPanel(); mobileToolsOpen = false"><AppIcon name="solar:info-circle-bold" :size="17" /> Details and info</button>
        </div>
        <div class="fm-mobile-tools-group">
          <span class="fm-mobile-tools-label">Filters and selection</span>
          <button class="fm-mobile-tool" type="button" :class="{ on: showHidden }" @click="showHidden = !showHidden; mobileToolsOpen = false"><AppIcon name="solar:eye-bold" :size="17" /> {{ showHidden ? 'Hide hidden files' : 'Show hidden files' }}</button>
          <button class="fm-mobile-tool" type="button" :class="{ on: onlyMedia }" @click="onlyMedia = !onlyMedia; mobileToolsOpen = false"><AppIcon name="solar:gallery-bold" :size="17" /> {{ onlyMedia ? 'Show all files' : 'Filter media' }}</button>
          <button class="fm-mobile-tool" type="button" :class="{ on: density === 'compact' }" @click="density = density === 'compact' ? 'cozy' : 'compact'; mobileToolsOpen = false"><AppIcon name="solar:widget-2-bold" :size="17" /> {{ density === 'compact' ? 'Cozy spacing' : 'Compact spacing' }}</button>
          <button class="fm-mobile-tool" type="button" @click="selectAll(); mobileToolsOpen = false"><AppIcon name="solar:checklist-bold" :size="17" /> Select all files</button>
          <button class="fm-mobile-tool" type="button" @click="clearSel(); mobileToolsOpen = false"><AppIcon name="solar:close-bold" :size="17" /> Clear selection</button>
        </div>
        <div class="fm-mobile-tools-group">
          <span class="fm-mobile-tools-label">View</span>
          <div class="fm-mobile-view-grid" role="group" aria-label="Choose a file view">
            <button v-for="v in viewModes" :key="v.id" class="fm-mobile-tool" type="button" :class="{ on: view === v.id }" @click="setView(v.id); mobileToolsOpen = false"><AppIcon :name="v.icon" :size="17" /> {{ v.hint }}</button>
          </div>
        </div>
        <div class="fm-mobile-tools-group">
          <span class="fm-mobile-tools-label">Storage</span>
          <button class="fm-mobile-tool" type="button" @click="wm.open('sync'); mobileToolsOpen = false"><AppIcon name="solar:refresh-bold" :size="17" /> Sync settings</button>
          <button class="fm-mobile-tool" type="button" @click="wm.open('disks'); mobileToolsOpen = false"><AppIcon name="solar:ssd-square-bold" :size="17" /> Disks and vaults</button>
        </div>
      </div>
    </header>

    <!-- ══ SUB BAR: live volume / sync / selection ══ -->
    <div class="fm-sub">
      <!-- Scope tabs: Vault vs the aggregated Google Drive view.
           The Drive tab mounts every googleDrive config and lists all
           external Drive files (not vault yet) under /providers. -->
      <div class="fm-scopetabs" role="tablist" aria-label="File scope">
        <button
          role="tab"
          :aria-selected="scopeTab === 'vault'"
          class="fm-scopetab"
          :class="{ active: scopeTab === 'vault' }"
          title="Vault files (.cybermanju home — merged across disks)"
          @click="openVaultTab"
        >
          <AppIcon name="solar:ssd-square-bold" :size="12" />
          <span>Vault</span>
        </button>
        <button
          role="tab"
          :aria-selected="scopeTab === 'gdrive'"
          class="fm-scopetab"
          :class="{ active: scopeTab === 'gdrive' }"
          :title="gdriveConfigs.length ? `All Google Drive files (${gdriveConfigs.length} account${gdriveConfigs.length === 1 ? '' : 's'}) — not vault yet` : 'No Google Drive connected — add one in Accounts → Connections'"
          @click="openGdriveTab"
        >
          <AppIcon name="solar:cloud-storage-bold" :size="12" />
          <span>Google Drive</span>
          <span v-if="gdriveConfigs.length" class="fm-scopetab-n">{{ gdriveConfigs.length }}</span>
        </button>
      </div>
      <span class="fm-dot" />
      <span class="fm-stat"><b>{{ sortedFiles.length }}</b> items</span>
      <span class="fm-dot" />
      <span class="fm-stat"><b>{{ selCount }}</b> selected · <b>{{ humanBytes(selBytes) }}</b></span>
      <span class="fm-dot" />
      <span class="fm-stat hide-sm">{{ humanBytes(folderBytes) }} in view</span>
      <span class="fm-spacer" />
      <div v-if="df" class="fm-vol" :title="`${humanBytes(df.usedBytes)} of ${humanBytes(df.totalBytes)} — ${df.diskCount} disks`">
        <div class="fm-volbar"><i :style="{ width: volPct + '%' }" /></div>
        <span class="fm-stat mono">{{ volPct.toFixed(0) }}% vol</span>
      </div>
      <button
        v-for="p in providerChips"
        :key="p.id"
        class="fm-pchip"
        :class="{ off: !p.enabled }"
        :title="`${p.label} — ${p.enabled ? 'enabled' : 'disabled'}${p.detail ? ' · ' + p.detail : ''} — click: browse files`"
        @click="openProvider(p.id)"
      >
        <i :style="{ background: p.color }" />
        {{ p.short }}
      </button>
      <button class="fm-pill xs ghost" title="Open sync panel" @click="wm.open('sync')"><AppIcon name="solar:refresh-bold" :size="11" /> Sync</button>
      <button class="fm-pill xs ghost" title="Open disks & volume" @click="wm.open('disks')"><AppIcon name="solar:ssd-square-bold" :size="11" /> Disks</button>
    </div>

    <!-- ══ MAIN SPLIT ══ -->
    <div class="fm-main" :class="{
      'no-side': !sideOpen,
      'no-insp': !inspectorOpen,
      'fm-mobile-side-open': isMobileView && mobileSideOpen,
      'fm-mobile-insp-open': isMobileView && mobileInspectorOpen,
    }">
      <!-- SIDEBAR -->
      <aside v-if="sideOpen || isMobileView" class="fm-side">
        <div v-if="isMobileView" class="fm-mobile-panel-head">
          <b>Browse</b>
          <button class="fm-ibtn sm" type="button" aria-label="Close folders" @click="mobileSideOpen = false"><AppIcon name="solar:close-bold" :size="14" /></button>
        </div>
        <!-- Dolphin Places (plasma): Home, Recent, Trash, Devices. -->
        <div v-if="isPlasma" class="fm-side-sec">
          <div class="fm-side-h">Places</div>
          <button class="fm-srow" @click="goHome">
            <AppIcon name="solar:house-bold" :size="14" />
            <span class="t">Home</span>
          </button>
          <button class="fm-srow" @click="wm.open('recent')">
            <AppIcon name="solar:history-bold" :size="14" />
            <span class="t">Recent</span>
          </button>
          <button class="fm-srow" @click="openTrash">
            <AppIcon name="solar:trash-bin-trash-bold" :size="14" />
            <span class="t">Trash</span>
          </button>
          <button class="fm-srow" @click="wm.open('devices')">
            <AppIcon name="solar:plug-circle-bold" :size="14" />
            <span class="t">Devices</span>
          </button>
        </div>
        <div class="fm-side-sec">
          <div class="fm-side-h">VOLUME</div>
          <button class="fm-srow" :class="{ active: !scopeFolder && !diskScopeId }" @click="goHomeVault">
            <AppIcon name="solar:ssd-square-bold" :size="14" />
            <span class="t">Vault root</span>
            <span class="m mono">{{ humanBytes(df?.totalBytes) }}</span>
          </button>
          <button
            v-for="d in store.disks"
            :key="d.id"
            class="fm-srow"
            :class="{ active: diskScopeId === d.id }"
            :title="`${d.name} · ${d.provider} · ${humanBytes(d.usedBytes)} / ${humanBytes(d.capacityBytes)} · ${d.state} — click: files share · right-click: manage`"
            @click="openDiskFiles(d)"
            @contextmenu.prevent.stop="diskMenu($event, d)"
          >
            <i class="fm-pdot" :class="d.state" />
            <span class="t truncate">{{ d.name }}</span>
            <span class="m mono">{{ Math.round(diskPct(d.usedBytes, d.capacityBytes)) }}%</span>
          </button>
          <div v-if="!store.disks.length" class="fm-sempty">No disks — create one in Disks.</div>
        </div>
        <div class="fm-side-sec">
          <div class="fm-side-h">PROVIDERS · {{ store.syncConfigs.length }}</div>
          <!-- Pinned aggregate: every Google Drive account's external files in one tab. -->
          <button
            class="fm-srow"
            :class="{ active: scopeTab === 'gdrive' }"
            :title="gdriveConfigs.length ? `All Google Drive files (${gdriveConfigs.length} account${gdriveConfigs.length === 1 ? '' : 's'}) — browse every connected Drive here` : 'No Google Drive connected — add one in Accounts → Connections'"
            @click="openGdriveTab"
          >
            <AppIcon name="solar:cloud-storage-bold" :size="14" style="color: #34a853" />
            <span class="t">Google Drive</span>
            <span class="m mono">{{ gdriveConfigs.length || '—' }}</span>
          </button>
          <button
            v-for="c in store.syncConfigs"
            :key="c.id"
            class="fm-srow"
            :class="{ active: providerMountFor(c.id) && store.currentPath.startsWith(`/providers/${providerMountFor(c.id)}`) }"
            :title="`${backendLabel(c.backendType)} · ${c.repoName || c.basePath || c.name || c.id} — click: browse Google Drive / GitHub / GitLab files`"
            @click="openProvider(c.id)"
            @contextmenu.prevent.stop="providerMenu($event, c.id)"
          >
            <i class="fm-pdot" :style="{ background: backendColor(c.backendType) }" />
            <span class="t truncate">{{ providerShortName(c) }}</span>
            <span class="m mono">{{ c.enabled ? (providerMountFor(c.id) ? 'mounted' : 'mount…') : 'off' }}</span>
          </button>
          <div v-if="!store.syncConfigs.length" class="fm-sempty">No providers — add Google Drive / GitHub / GitLab in Accounts → Connections.</div>
          <button v-else-if="vfsMounts.length < store.syncConfigs.length" class="fm-srow" title="Mount every provider so all repos/files/folders show here" @click="mountAllProviders">
            <AppIcon name="solar:cloud-bold" :size="14" />
            <span class="t">Show all providers</span>
          </button>
        </div>
        <div v-if="hasDevice" class="fm-side-sec">
          <div class="fm-side-h">DEVICE</div>
          <button
            class="fm-srow"
            :class="{ active: isHostPathNow }"
            :title="isNative ? `Browse this device's files (sdcard on Android) — move anything between vault, providers and the device` : `Browse the added external folders — move anything between vault, providers and the device`"
            @click="openHostRoot"
          >
            <AppIcon name="solar:smartphone-bold" :size="14" />
            <span class="t">Device files</span>
            <span class="m mono">-os</span>
          </button>
          <button
            v-if="isNative"
            class="fm-srow"
            title="Browse from the filesystem root"
            @click="openHostFsRoot"
          >
            <AppIcon name="solar:folder-tree-bold" :size="14" />
            <span class="t">Filesystem root</span>
            <span class="m mono">/</span>
          </button>
          <button
            class="fm-srow"
            :title="isNative ? 'Pick any device folder to browse it here' : 'Add an external folder — pick a SUBfolder (Chrome blocks $HOME and system roots): create e.g. Documents/CyberManju and pick it'"
            @click="pickHostFolder"
          >
            <AppIcon name="solar:folder-open-bold" :size="14" />
            <span class="t">{{ isNative ? 'Pick folder…' : 'Add folder…' }}</span>
          </button>
          <button
            v-for="m in wasmMounts"
            :key="m.id"
            class="fm-srow"
            :class="{ active: store.currentPath === `/host/${m.label}` || store.currentPath.startsWith(`/host/${m.label}/`) }"
            :title="m.status === 'ready' ? `Browse ${m.label} — right-click: unmount` : `${m.label} (${m.status}) — right-click: unmount, or Re-allow below`"
            @click="openHostMount(m.label)"
            @contextmenu.prevent.stop="hostMountMenu($event, m)"
          >
            <AppIcon name="solar:folder-bold" :size="14" />
            <span class="t truncate">{{ m.label }}</span>
            <span class="m mono">{{ m.status === 'ready' ? 'ext' : m.status }}</span>
          </button>
          <button
            v-if="!isNative && hostWasmParked > 0"
            class="fm-srow"
            title="Re-allow the remembered folders (no re-pick)"
            @click="reallowHostWasm"
          >
            <AppIcon name="solar:key-bold" :size="14" />
            <span class="t">Re-allow ({{ hostWasmParked }})</span>
          </button>
        </div>
        <div v-else class="fm-side-sec">
          <div class="fm-side-h">DEVICE</div>
          <button
            class="fm-srow"
            title="This browser cannot open device folders — see native install options (Android, Linux, Windows, Docker)"
            @click="openInstall('host-unsupported')"
          >
            <AppIcon name="solar:download-bold" :size="14" />
            <span class="t">Get device access…</span>
          </button>
        </div>
        <div class="fm-side-sec">
          <div class="fm-side-h">FOLDERS</div>
          <button
            v-for="f in folders"
            :key="f.id"
            class="fm-srow"
            :class="{ active: store.selectedFileId === f.id }"
            @click="openFolder(f)"
            @dblclick="openFolder(f)"
            @contextmenu.prevent="fileMenu($event, f)"
          >
            <AppIcon name="solar:folder-bold" :size="14" />
            <span class="t truncate">{{ f.name }}</span>
          </button>
          <div v-if="!folders.length" class="fm-sempty">No subfolders here.</div>
        </div>
        <div class="fm-side-sec">
          <div class="fm-side-h">FAVORITES · {{ store.starredFiles.length }}</div>
          <button
            v-for="f in store.starredFiles.slice(0, 8)"
            :key="f.id"
            class="fm-srow"
            @click="revealFile(f)"
          >
            <AppIcon name="solar:star-bold" :size="13" />
            <span class="t truncate">{{ f.name }}</span>
          </button>
          <div v-if="!store.starredFiles.length" class="fm-sempty">Star anything to pin it.</div>
        </div>
        <div class="fm-side-sec">
          <div class="fm-side-h">TAGS</div>
          <div class="fm-tags">
            <button v-for="t in allTags.slice(0, 14)" :key="t" class="fm-tag" @click="filterQuery = t">#{{ t }}</button>
            <span v-if="!allTags.length" class="fm-sempty">No tags yet.</span>
          </div>
        </div>
      </aside>

      <!-- CENTER -->
      <section class="fm-center">
        <div class="fm-ctool">
          <button class="fm-ibtn sm" :title="isMobileView ? 'Browse folders' : sideOpen ? 'Hide sidebar' : 'Show sidebar'" :aria-label="isMobileView ? 'Browse folders' : sideOpen ? 'Hide sidebar' : 'Show sidebar'" @click="toggleSidePanel"><AppIcon name="solar:sidebar-bold" :size="13" /></button>
          <select v-model="sortField" class="fm-select" title="Sort by" aria-label="Sort by">
            <option value="name">Name</option>
            <option value="size">Size</option>
            <option value="date">Modified</option>
            <option value="type">Type</option>
          </select>
          <button class="fm-ibtn sm" :title="sortDir === 'asc' ? 'Ascending — flip to descending' : 'Descending — flip to ascending'" @click="sortDir = sortDir === 'asc' ? 'desc' : 'asc'">
            <AppIcon :name="sortDir === 'asc' ? 'solar:sort-from-bottom-to-top-bold' : 'solar:sort-from-top-to-bottom-bold'" :size="13" />
          </button>
          <button class="fm-chipbtn" :class="{ on: showHidden }" title="Show hidden files" @click="showHidden = !showHidden">hidden</button>
          <button class="fm-chipbtn" :class="{ on: onlyMedia }" title="Only media" @click="onlyMedia = !onlyMedia">media</button>
          <button class="fm-chipbtn" :class="{ on: density === 'compact' }" title="Toggle compact density" @click="density = density === 'compact' ? 'cozy' : 'compact'">compact</button>
          <span class="fm-spacer" />
          <button class="fm-pill xs" title="Select all (Ctrl+A)" @click="selectAll">All</button>
          <button class="fm-pill xs ghost" title="Clear selection (Esc)" @click="clearSel">None</button>
          <button class="fm-pill xs ghost" title="Open current folder in cybsh" @click="openFolderInTerm()"><AppIcon name="solar:file-terminal-bold" :size="11" /> cd here</button>
        </div>

        <!-- DISK SCOPE: per-disk share of the merged volume.
             Files are striped at the block level, so there is no per-file
             disk owner — this banner shows the disk's ratio (used/capacity
             + share of the volume) with a proportional file-size estimate. -->
        <div v-if="diskScope" class="fm-diskscope" role="status">
          <AppIcon name="solar:ssd-square-bold" :size="13" />
          <span class="fm-diskscope-t truncate" :title="diskScope.containerPath || diskScope.name">
            <b>{{ diskScope.name }}</b>
            <span class="dim">· {{ diskScope.provider }} · {{ diskScope.state }} · {{ Math.round(diskPct(diskScope.usedBytes, diskScope.capacityBytes)) }}% used</span>
          </span>
          <span class="fm-diskscope-bar" :title="`${humanBytes(diskScope.usedBytes)} / ${humanBytes(diskScope.capacityBytes)}`"><i :style="{ width: diskPct(diskScope.usedBytes, diskScope.capacityBytes) + '%' }" /></span>
          <span class="mono dim hide-sm" :title="`Share of merged volume by used bytes`">≈{{ (diskShare * 100).toFixed(1) }}% vol · ~{{ humanBytes(diskFileBytes) }} files</span>
          <span class="fm-spacer" />
          <button class="fm-pill xs" title="Open this disk in Disks & Volume" @click="diskScope && manageDisk(diskScope)">Manage disk</button>
          <button class="fm-pill xs ghost" title="Clear disk filter" @click="clearDiskScope()">✕</button>
        </div>

        <!-- PROVIDER SCOPE: Google Drive / GitHub / GitLab files live here.
             Every connected repo/folder is browsable; files move into the
             vault (.cybermanju) or straight between providers. -->
        <div v-if="isProviderPath" class="fm-diskscope provider" role="status">
          <AppIcon name="solar:cloud-bold" :size="13" />
          <span class="fm-diskscope-t truncate" :title="store.currentPath">
            <b>{{ providerScopeLabel }}</b>
            <span class="dim">· provider files — not vault yet</span>
          </span>
          <span class="fm-spacer" />
          <button class="fm-pill xs" title="Save selected provider files into the vault (.cybermanju)" :disabled="!selCount && !active" @click="saveProviderSelToVault">↓ Vault</button>
          <button class="fm-pill xs ghost" title="Move/copy a provider file to another provider" :disabled="!active && !selCount" @click="openProviderMove(active)">⇄ Move</button>
          <button class="fm-pill xs ghost" title="Back to vault root (.cybermanju home)" @click="goHomeVault">Vault ⌂</button>
        </div>

        <!-- DEVICE SCOPE: host filesystem via cybsh `-os` (sdcard on
             Android). Files here move to the vault or any provider and back. -->
        <div v-if="isHostPathNow" class="fm-diskscope provider" role="status">
          <AppIcon name="solar:smartphone-bold" :size="13" />
          <span class="fm-diskscope-t truncate" :title="hostAbsNow || hostWasmName || store.currentPath">
            <b>{{ hostAbsNow || hostWasmName || 'Device' }}</b>
            <span class="dim">· device files — not vault yet</span>
          </span>
          <span class="fm-spacer" />
          <button v-if="!isNative" class="fm-pill xs" title="Add an external folder — pick a SUBfolder (Chrome blocks $HOME and system roots)" @click="pickHostFolder">Add folder…</button>
          <button v-if="!isNative && hostMountLabel" class="fm-pill xs ghost" :title="`Remove '${hostMountLabel}' from Device files (files stay on disk)`" @click="unmountHostWasm(hostMountLabel, hostMountLabel)">Unmount</button>
          <button v-if="!isNative && hostWasmParked > 0" class="fm-pill xs ghost" title="Re-allow the remembered folders (no re-pick)" @click="reallowHostWasm">Re-allow ({{ hostWasmParked }})</button>
          <button class="fm-pill xs" title="Import selected device files into the vault (.cybermanju)" :disabled="!selCount && !active" @click="() => saveHostSelToVault()">↓ Vault</button>
          <button class="fm-pill xs" title="Save selected device files into the vault AND encrypt them there" :disabled="!selCount && !active" @click="() => vaultEncryptHostSel()">↓ Vault + Encrypt</button>
          <button class="fm-pill xs ghost" title="Move/copy to vault, provider or another device folder" :disabled="!active && !selCount" @click="openMoveTo(null)">⇄ Move</button>
          <button class="fm-pill xs ghost" title="Back to vault root (.cybermanju home)" @click="goHomeVault">Vault ⌂</button>
        </div>

        <div v-if="selCount > 0" class="fm-bulk" role="toolbar" aria-label="Bulk actions">
          <span class="fm-bulk-n">{{ selCount }} selected · {{ humanBytes(selBytes) }}</span>
          <button title="Encrypt" @click="bulkEncrypt"><AppIcon name="solar:lock-bold" :size="12" /></button>
          <button title="Compress (zstd)" @click="bulkCompress"><AppIcon name="solar:archive-bold" :size="12" /></button>
          <button title="Sync to provider…" @click="bulkSync"><AppIcon name="solar:cloud-bold" :size="12" /></button>
          <button title="Star" @click="bulkStar"><AppIcon name="solar:star-bold" :size="12" /></button>
          <button title="Copy" @click="copySel"><AppIcon name="solar:copy-bold" :size="12" /></button>
          <button title="Cut" @click="cutSel"><AppIcon name="solar:scissors-bold" :size="12" /></button>
          <button title="Paste here" :disabled="!clipboard.nodes.length" @click="pasteHere"><AppIcon name="solar:clipboard-paste-bold" :size="12" /></button>
          <button class="danger" title="Delete" @click="askDeleteSel"><AppIcon name="solar:trash-bin-trash-bold" :size="12" /></button>
          <button title="Clear" @click="clearSel"><AppIcon name="solar:close-bold" :size="12" /></button>
        </div>

        <!-- GRID -->
        <div
          v-if="view === 'grid' || view === 'masonry'"
          ref="scrollRef"
          class="fm-grid"
          :class="[view, density]"
          @scroll="onScroll"
          @contextmenu.prevent="bgMenu($event)"
        >
          <button
            v-for="f in renderFiles"
            :key="f.id"
            v-memo="[f.name, f.sizeBytes, f.modifiedAt, isSel(f.id), isBulk(f.id), f.encrypted, encLayer(f), f.isStarred]"
            class="fm-card"
            :class="{ sel: isSel(f.id), bulk: isBulk(f.id) }"
            :title="f.name"
            @click="clickFile(f, $event)"
            @dblclick="openFile(f)"
            @contextmenu.prevent.stop="fileMenu($event, f)"
            draggable="true"
            @dragstart="onDrag($event, f)"
          >
            <span class="fm-check" :class="{ on: isBulk(f.id) }" @click.stop="toggleBulk(f.id)">{{ isBulk(f.id) ? '✓' : '' }}</span>
            <span class="fm-thumb">
              <img v-if="f.thumbnailPath" :src="f.thumbnailPath" alt="" loading="lazy" decoding="async" @error="hideImg" />
              <AppIcon v-else :name="iconFor(f)" :size="26" />
            </span>
            <span class="fm-cname truncate">{{ f.name }}</span>
            <span class="fm-cmeta mono">{{ humanBytes(f.sizeBytes) }}</span>
            <span class="fm-flags">
              <AppIcon v-if="f.encrypted" name="solar:lock-bold" :size="10" title="Encrypted" />
              <AppIcon v-if="encLayer(f) !== 'none'" name="solar:archive-bold" :size="10" title="Compressed" />
              <AppIcon v-if="f.isStarred" name="solar:star-bold" :size="10" title="Starred" />
              <AppIcon v-if="f.gpsLat != null" name="solar:map-point-bold" :size="10" title="Geo-tagged" />
            </span>
          </button>
          <div v-if="!renderFiles.length && !store.isLoading" class="fm-empty">
            <AppIcon name="solar:folder-open-bold" :size="30" />
            <p>{{ filterQuery ? 'No files match' : (scopeTab === 'gdrive' && !gdriveConfigs.length ? 'No Google Drive connected — add one in Accounts → Connections' : (store.currentPath === '/' ? 'Vault is empty' : 'Empty folder')) }}</p>
            <span>{{ filterQuery ? 'Try a different filter, or ⏎ for global search.' : (store.currentPath === '/' ? 'This home shows everything merged across your disks — create your first folder to begin.' : 'Drop files, paste, upload — or create something new.') }}</span>
          </div>
          <div v-if="store.isLoading" class="fm-empty"><span class="fm-spin" /> Loading…</div>
          <button v-if="renderLimit < sortedFiles.length" class="fm-more" @click="renderLimit += 300">Show more ({{ sortedFiles.length - renderLimit }} left)</button>
        </div>

        <!-- LIST / DETAILS -->
        <div
          v-else-if="view === 'list' || view === 'details'"
          ref="scrollRef"
          class="fm-listwrap"
          @scroll="onScroll"
          @contextmenu.prevent="bgMenu($event)"
        >
          <div class="fm-lhead" :class="density">
            <span class="c0" @click="selectAll" title="Select all"><span class="fm-check sm" :class="{ on: allBulk }">{{ allBulk ? '✓' : '' }}</span></span>
            <span class="c1" @click="setSort('name')">Name {{ arrow('name') }}</span>
            <span class="c2" @click="setSort('size')">Size {{ arrow('size') }}</span>
            <span v-if="view === 'details'" class="c3">Algo / Layer</span>
            <span v-else class="c3" @click="setSort('type')">Type {{ arrow('type') }}</span>
            <span class="c4" @click="setSort('date')">Modified {{ arrow('date') }}</span>
            <span class="c5">Hash</span>
            <span class="c6">Flags</span>
          </div>
          <div class="fm-lbody">
            <div
              v-for="f in renderFiles"
              :key="f.id"
              v-memo="[f.name, f.sizeBytes, f.modifiedAt, isSel(f.id), isBulk(f.id), f.encrypted, encLayer(f), f.isStarred]"
              class="fm-lrow"
              :class="[{ sel: isSel(f.id), bulk: isBulk(f.id) }, density]"
              @click="clickFile(f, $event)"
              @dblclick="openFile(f)"
              @contextmenu.prevent.stop="fileMenu($event, f)"
              draggable="true"
              @dragstart="onDrag($event, f)"
            >
              <span class="c0"><span class="fm-check sm" :class="{ on: isBulk(f.id) }" @click.stop="toggleBulk(f.id)">{{ isBulk(f.id) ? '✓' : '' }}</span></span>
              <span class="c1">
                <span class="fm-ficon"><img v-if="f.thumbnailPath" :src="f.thumbnailPath" alt="" loading="lazy" decoding="async" @error="hideImg" /><AppIcon v-else :name="iconFor(f)" :size="15" /></span>
                <span class="truncate" :title="f.name">{{ f.name }}</span>
              </span>
              <span class="c2 mono">{{ humanBytes(f.sizeBytes) }}</span>
              <span v-if="view === 'details'" class="c3 mono">{{ cryptoLabel(f) }}</span>
              <span v-else class="c3 dim truncate">{{ shortType(f) }}</span>
              <span class="c4 mono dim">{{ fmtDate(f.modifiedAt) }}</span>
              <span class="c5 mono dim">{{ f.hashBlake3 ? f.hashBlake3.slice(0, 10) + '…' : '—' }}</span>
              <span class="c6">
                <AppIcon v-if="f.encrypted" name="solar:lock-bold" :size="11" title="Encrypted" />
                <AppIcon v-if="encLayer(f) !== 'none'" name="solar:archive-bold" :size="11" title="Compressed" />
                <AppIcon v-if="f.isStarred" name="solar:star-bold" :size="11" title="Starred" />
              </span>
            </div>
            <div v-if="!renderFiles.length && !store.isLoading" class="fm-empty"><p>{{ filterQuery ? 'No files match.' : (scopeTab === 'gdrive' && !gdriveConfigs.length ? 'No Google Drive connected — add one in Accounts → Connections.' : (store.currentPath === '/' ? 'Vault is empty — this home shows everything merged across your disks.' : 'Empty folder.')) }}</p></div>
            <button v-if="renderLimit < sortedFiles.length" class="fm-more" @click="renderLimit += 300">Show more ({{ sortedFiles.length - renderLimit }} left)</button>
          </div>
        </div>

        <!-- COLUMNS (macOS-style) -->
        <div v-else class="fm-cols" @contextmenu.prevent="bgMenu($event)">
          <div class="fm-col">
            <div class="fm-col-h">This folder</div>
            <button
              v-for="f in sortedFiles"
              :key="f.id"
              class="fm-colrow"
              :class="{ sel: isSel(f.id) || colPreview?.id === f.id }"
              @click="peekColumn(f)"
              @dblclick="openFile(f)"
              @contextmenu.prevent.stop="fileMenu($event, f)"
            >
              <AppIcon :name="iconFor(f)" :size="14" />
              <span class="truncate">{{ f.name }}</span>
              <AppIcon v-if="f.fileType === 'folder'" name="solar:alt-arrow-right-bold" :size="11" />
            </button>
          </div>
          <div class="fm-col preview">
            <div class="fm-col-h">{{ colPreview ? colPreview.name : 'Preview' }}</div>
            <div v-if="colPreview" class="fm-colprev">
              <AppIcon :name="iconFor(colPreview)" :size="40" />
              <div class="mono dim">{{ humanBytes(colPreview.sizeBytes) }} · {{ shortType(colPreview) }}</div>
              <div class="mono dim">{{ cryptoLabel(colPreview) }}</div>
              <div class="fm-colbtns">
                <button class="fm-pill xs" @click="openFile(colPreview!)">Open</button>
                <button class="fm-pill xs ghost" @click="openFolderInTerm(colPreview!)">Terminal</button>
              </div>
            </div>
            <div v-else class="fm-sempty">Select an item to preview.</div>
          </div>
        </div>

        <!-- STATUS BAR: path + selection + zoom + free space -->
        <footer class="fm-status">
          <span class="mono truncate" :title="store.currentPath">{{ store.currentPath }}</span>
          <span class="fm-dot" />
          <span v-if="active" class="truncate">▸ {{ active.name }} <span class="mono dim">{{ humanBytes(active.sizeBytes) }}</span></span>
          <span v-else class="dim">▸ nothing selected</span>
          <span class="fm-spacer" />
          <span v-if="active?.encrypted" class="fm-sbadge lock">🔒 {{ (active.encryptionAlgorithm || 'enc').toUpperCase() }}</span>
          <span v-if="active && encLayer(active) !== 'none'" class="fm-sbadge zip">🗜 {{ encLayer(active).toUpperCase() }}</span>
          <span v-if="clipboard.nodes.length" class="fm-sbadge clip">📋 {{ clipboard.nodes.length }} {{ clipboard.mode }}</span>
          <span v-if="df" class="mono dim hide-sm">{{ humanBytes((df.totalBytes || 0) - (df.usedBytes || 0)) }} free</span>
          <label v-if="view === 'grid' || view === 'masonry'" class="fm-zoom" title="Icon size">
            <AppIcon name="solar:gallery-bold" :size="12" />
            <input v-model.number="zoom" type="range" min="0.75" max="1.5" step="0.05" aria-label="Icon size" :style="{ '--p': (((zoom - 0.75) / 0.75) * 100).toFixed(1) + '%' }" />
          </label>
          <span class="mono dim hide-sm">{{ renderFiles.length }}/{{ sortedFiles.length }}</span>
        </footer>

        <!-- INLINE TERMINAL -->
        <div v-if="termOpen" class="fm-term">
          <div class="fm-term-h">
            <AppIcon name="solar:file-terminal-bold" :size="13" />
            <span class="mono">cybsh — {{ termCwd }}</span>
            <span v-if="store.shellBusy" class="fm-spin sm" />
            <span class="fm-spacer" />
            <button class="fm-ibtn sm" title="ls here" @click="runTerm('ls -la ' + sh(termCwd))">ls</button>
            <button class="fm-ibtn sm" title="Open full terminal window" @click="wm.open('terminal')">⧉</button>
            <button class="fm-ibtn sm" title="Close terminal" @click="termOpen = false"><AppIcon name="solar:close-bold" :size="11" /></button>
          </div>
          <div ref="termScrollRef" class="fm-term-body">
            <div v-for="l in termLines" :key="l.id" class="fm-term-l" :class="l.kind">{{ l.text }}</div>
          </div>
          <form class="fm-term-in" @submit.prevent="submitTerm">
            <span class="mono">❯</span>
            <input ref="termInputRef" v-model="termInput" class="mono" :placeholder="`run in ${termCwd}… (try: ls, stat <file>, sync status)`" aria-label="Terminal input" @keydown.up.prevent="termHist(-1)" @keydown.down.prevent="termHist(1)" />
          </form>
        </div>
      </section>

      <button
        v-if="isMobileView && (mobileSideOpen || mobileInspectorOpen)"
        class="fm-mobile-backdrop"
        type="button"
        aria-label="Close file panel"
        @click="closeMobilePanels"
      />

      <!-- INSPECTOR -->
      <aside v-if="inspectorOpen || isMobileView" class="fm-insp">
        <div v-if="isMobileView" class="fm-mobile-panel-head">
          <b>Details</b>
          <button class="fm-ibtn sm" type="button" aria-label="Close details" @click="mobileInspectorOpen = false"><AppIcon name="solar:close-bold" :size="14" /></button>
        </div>
        <div v-if="!active" class="fm-iempty">
          <AppIcon name="solar:file-search-bold" :size="28" />
          <p>Select a file</p>
          <span>Full metadata, crypto, distribution, versions & sharing live here.</span>
          <div v-if="df" class="fm-volcard">
            <div class="fm-side-h">VOLUME HEALTH</div>
            <div class="fm-volbar big"><i :style="{ width: volPct + '%' }" /></div>
            <div class="mono dim">{{ humanBytes(df.usedBytes) }} / {{ humanBytes(df.totalBytes) }} · {{ df.diskCount }} disks</div>
            <button class="fm-pill xs" @click="wm.open('disks')">Manage disks</button>
          </div>
        </div>
        <template v-else>
          <div class="fm-ihead">
            <span class="fm-ithumb"><AppIcon :name="iconFor(active)" :size="22" /></span>
            <div class="fm-ititle">
              <div class="truncate" :title="active.name"><b>{{ active.name }}</b></div>
              <div class="mono dim truncate">{{ active.path || store.currentPath }}</div>
            </div>
            <button class="fm-ibtn sm" title="Open in full terminal" @click="openFolderInTerm(active, true)"><AppIcon name="solar:file-terminal-bold" :size="13" /></button>
          </div>
          <div class="fm-tabs" role="tablist" aria-label="Inspector tabs">
            <button v-for="t in tabs" :key="t.id" role="tab" :aria-selected="inspTab === t.id" :class="{ active: inspTab === t.id }" @click="inspTab = t.id; onTab(t.id)">{{ t.label }}</button>
          </div>
          <div class="fm-ibody">
            <!-- INFO -->
            <div v-if="inspTab === 'info'" class="fm-isec">
              <div class="fm-kv"><span>Size</span><b class="mono">{{ humanBytes(active.sizeBytes) }}</b></div>
              <div class="fm-kv"><span>Type</span><b>{{ shortType(active) }}</b></div>
              <div class="fm-kv"><span>MIME</span><b class="mono">{{ active.mimeType || '—' }}</b></div>
              <div class="fm-kv"><span>Modified</span><b class="mono">{{ fmtDate(active.modifiedAt) }}</b></div>
              <div class="fm-kv"><span>Created</span><b class="mono">{{ fmtDate(active.createdAt) }}</b></div>
              <div class="fm-kv hash"><span>BLAKE3</span><b class="mono" :title="active.hashBlake3">{{ active.hashBlake3 || '—' }}</b></div>
              <div class="fm-kv"><span>Starred</span><button class="fm-pill xs" @click="store.toggleStar(active!.id)">{{ active.isStarred ? '★ yes' : '☆ star it' }}</button></div>
              <div v-if="active.tags?.length" class="fm-kv"><span>Tags</span><b>{{ active.tags.join(', ') }}</b></div>
              <div v-if="activeIsVault" class="fm-tagedit">
                <span class="fm-tagedit__label">TAGS — TEACH SEARCH</span>
                <div class="fm-tagedit__chips">
                  <span v-for="t in (active.tags || [])" :key="t" class="fm-tagedit__chip">
                    {{ t }}
                    <button type="button" class="fm-tagedit__x" :aria-label="`Remove tag ${t}`" title="Remove" @click="removeTag(t)">×</button>
                  </span>
                  <span v-if="!(active.tags || []).length" class="fm-sempty">No tags — add one to improve search.</span>
                </div>
                <form class="fm-tagedit__form" @submit.prevent="addTag()">
                  <input
                    v-model="tagDraft"
                    class="fm-tagedit__input"
                    type="text"
                    placeholder="Add tag… (Enter)"
                    aria-label="Add tag"
                    maxlength="48"
                  />
                  <button type="submit" class="fm-pill xs" :disabled="!tagDraft.trim() || tagSaving">Add</button>
                </form>
              </div>
              <div v-else class="fm-sempty">Tags live on vault files — save this {{ isHostId(active.id) ? 'device file' : 'provider file' }} to the vault to teach search about it.</div>
              <div v-if="active.gpsLat != null" class="fm-kv"><span>GPS</span><b class="mono">{{ active.gpsLat.toFixed(4) }}, {{ active.gpsLon?.toFixed(4) }}</b></div>
              <div v-if="active.faceGroupIds?.length" class="fm-kv"><span>Faces</span><b>{{ active.faceGroupIds.map(getFaceGroupName).join(', ') }}</b></div>
              <div v-if="store.parseResult?.symbols.length" class="fm-kv"><span>Symbols</span><b class="mono">{{ store.parseResult.symbols.length }} · {{ store.parseResult.language }}</b></div>
              <div class="fm-btnrow">
                <button v-if="isLocalTextProviderFile(active)" class="fm-pill xs" title="Edit and auto-save this text file in the selected phone folder" @click="openLocalProviderEditor(active)">Edit local text</button>
                <button v-if="isHostTextFile(active)" class="fm-pill xs" title="Edit this device text file in place" @click="openHostEditor(active)">Edit device text</button>
                <button class="fm-pill xs" @click="openFile(active)">Open</button>
                <button class="fm-pill xs ghost" @click="copyPath(active)">Copy path</button>
                <button class="fm-pill xs ghost" @click="beginRename(active)">Rename</button>
              </div>
              <div class="fm-btnrow">
                <button class="fm-pill xs ghost" @click="openFolderInTerm(active)">Terminal here</button>
                <button class="fm-pill xs ghost" @click="store.fetchFileVersions(active!.id); inspTab = 'vers'">Versions</button>
                <button class="fm-pill xs ghost danger" @click="askDelete(active)">Delete</button>
              </div>
            </div>
            <!-- CRYPTO -->
            <div v-else-if="inspTab === 'crypto'" class="fm-isec">
              <div class="fm-side-h">Encryption</div>
              <div v-if="!activeIsVault" class="fm-sempty">Shield engines act on vault files — save this file to the vault first.</div>
              <template v-else>
              <div class="fm-cryptocard" :class="{ on: active.encrypted }">
                <div class="fm-kv"><span>Status</span><b>{{ active.encrypted ? '🔒 ENCRYPTED' : '🔓 plain' }}</b></div>
                <div class="fm-kv"><span>Algorithm</span><b>{{ activeAlgoName }}</b></div>
                <div v-if="active.encrypted" class="fm-kv"><span>NIST level</span><b>L{{ activeNist }} {{ '●'.repeat(activeNist) }}{{ '○'.repeat(Math.max(0, 5 - activeNist)) }}</b></div>
                <div v-if="keyIdOf(active)" class="fm-kv"><span>Key ID</span><b class="mono">{{ keyIdOf(active) }}</b></div>
              </div>
              <div class="fm-algos">
                <button v-for="(info, algo) in ENCRYPTION_INFO" :key="algo" class="fm-algo" :class="{ cur: (active.encryptionAlgorithm || '').toLowerCase() === String(algo).toLowerCase() }" :title="info.description" @click="encryptWith(String(algo))">
                  <b>{{ info.name }}</b><span>L{{ info.nistLevel }}</span>
                </button>
              </div>
              <div class="fm-btnrow">
                <button v-if="!active.encrypted" class="fm-pill xs" @click="encryptWith('hybrid')">Encrypt · hybrid</button>
                <button v-else class="fm-pill xs" @click="store.decryptFile(active!.id)">Decrypt</button>
                <button class="fm-pill xs ghost" @click="wm.open('encryption', { tab: 'shield' })">Key manager</button>
              </div>
              <div class="fm-side-h">Compression</div>
              <div class="fm-cryptocard" :class="{ on: encLayer(active) !== 'none' }">
                <div class="fm-kv"><span>Layers</span><b>{{ (active.compressionLayers || []).join(' → ') || 'none' }}</b></div>
                <div v-if="store.compressionStats" class="fm-kv"><span>Last run</span><b class="mono">{{ humanBytes(store.compressionStats.originalSize) }} → {{ humanBytes(store.compressionStats.compressedSize) }} ({{ (store.compressionStats.ratio * 100).toFixed(1) }}%)</b></div>
                <div v-if="active.hashBlake3" class="fm-kv"><span>Integrity</span><b class="mono">blake3 ✓ pinned</b></div>
              </div>
              <div class="fm-algos">
                <button v-for="(info, t) in COMPRESSION_INFO" :key="t" class="fm-algo" :class="{ cur: encLayer(active) === t }" :title="info.description" @click="compressWith(String(t))">
                  <b>{{ info.name }}</b><span>{{ info.speed }}</span>
                </button>
              </div>
              <div class="fm-btnrow">
                <button class="fm-pill xs" @click="compressWith('zstd')">Compress · zstd</button>
                <button v-if="encLayer(active) !== 'none'" class="fm-pill xs ghost" @click="store.decompressFile(active!.id)">Decompress</button>
                <button class="fm-pill xs ghost" @click="wm.open('encryption', { tab: 'compress' })">Engine</button>
              </div>
              </template>
            </div>
            <!-- DISTRIBUTION -->
            <div v-else-if="inspTab === 'distro'" class="fm-isec">
              <div class="fm-side-h">Location</div>
              <div v-if="!activeIsVault" class="fm-kv"><span>This file</span><b class="mono">{{ isHostId(active.id) ? hostAbsFromVm(active.path || '') || 'device' : 'provider repo' }}</b></div>
              <div v-else class="fm-kv"><span>Vault file</span><b class="mono">.cybermanju</b></div>
              <div class="fm-kv"><span>Volume</span><b class="mono">{{ df ? `${humanBytes(df.usedBytes)} / ${humanBytes(df.totalBytes)}` : '—' }}</b></div>
              <div class="fm-distro">
                <div v-for="c in store.syncConfigs" :key="c.id" class="fm-drow" :title="c.basePath || c.repoName || c.backendType">
                  <i :style="{ background: backendColor(c.backendType) }" />
                  <span class="t truncate"><b>{{ backendLabel(c.backendType) }}</b> <span class="dim">{{ c.name || '' }}</span></span>
                  <span class="mono dim">{{ c.placement || 'whole' }}{{ c.parity ? '·p' + c.parity : '' }}</span>
                  <span class="fm-dstat" :class="{ on: c.enabled }">{{ c.enabled ? 'on' : 'off' }}</span>
                  <button v-if="activeIsVault" class="fm-pill xs" title="Sync this file now" @click="syncFileTo(c.id)">⇪</button>
                  <button v-if="activeIsVault" class="fm-pill xs ghost" title="Probe remote copy" @click="probeRemote(c.id)">locate</button>
                </div>
                <div v-if="!store.syncConfigs.length" class="fm-sempty">No providers configured — <button class="fm-link" @click="wm.open('accounts', { tab: 'connections' })">add one in Accounts</button>.</div>
              </div>
              <div v-if="remoteHits.length" class="fm-side-h">REMOTE COPIES FOUND · {{ remoteHits.length }}</div>
              <div v-for="r in remoteHits" :key="r.url + r.path" class="fm-kv small">
                <span class="truncate" :title="r.path">{{ r.name }}</span>
                <b class="mono">{{ humanBytes(r.sizeBytes) }}</b>
              </div>
              <div v-if="probing" class="dim mono">probing remotes…</div>
              <div class="fm-side-h">DISKS HOLDING THE VOLUME</div>
              <button
                v-for="d in store.disks"
                :key="d.id"
                class="fm-kv small as-row"
                :class="{ active: diskScopeId === d.id }"
                :title="`${d.name} — click: files share · right-click: manage`"
                @click="openDiskFiles(d)"
                @contextmenu.prevent.stop="diskMenu($event, d)"
              >
                <span class="truncate" :title="d.containerPath || d.name">{{ d.name }} · {{ d.provider }}</span>
                <b class="mono">{{ Math.round(diskPct(d.usedBytes, d.capacityBytes)) }}%</b>
              </button>
              <div class="fm-btnrow">
                <button class="fm-pill xs ghost" @click="wm.open('sync')">Sync panel</button>
                <button class="fm-pill xs ghost" @click="wm.open('disks')">Disks</button>
                <button v-if="activeIsVault" class="fm-pill xs ghost" @click="restoreToVault">Restore ↓</button>
              </div>
            </div>
            <!-- VERSIONS -->
            <div v-else-if="inspTab === 'vers'" class="fm-isec">
              <div class="fm-side-h">VERSION HISTORY</div>
              <div v-if="!activeIsVault" class="fm-sempty">Snapshots belong to vault files — save this file to the vault first.</div>
              <template v-else>
              <div v-for="v in store.fileVersions" :key="v.id" class="fm-kv small">
                <span>v{{ v.versionNumber }} · {{ fmtDate(v.createdAt) }}</span>
                <button class="fm-pill xs ghost" @click="store.revertToVersion(active!.id, v.id)">revert</button>
              </div>
              <div v-if="!store.fileVersions.length" class="fm-sempty">No snapshots yet.</div>
              <div class="fm-btnrow">
                <button class="fm-pill xs" @click="store.createVersion(active!.id)">Snapshot now</button>
                <button class="fm-pill xs ghost" @click="store.snapshotAllVersions()">Snapshot all</button>
              </div>
              <div class="fm-side-h">PERMISSIONS</div>
              <div class="fm-btnrow"><button class="fm-pill xs ghost" @click="wm.open('permissions')">Open ACL editor</button></div>
              </template>
            </div>
            <!-- SHARE -->
            <div v-else class="fm-isec">
              <div class="fm-side-h">SHARE LINK</div>
              <div v-if="!activeIsVault" class="fm-sempty">Share links address vault files — save this file to the vault first.</div>
              <template v-else>
              <div class="fm-btnrow">
                <button class="fm-pill xs" @click="makeShare(24)">24h link</button>
                <button class="fm-pill xs ghost" @click="makeShare(168)">7d link</button>
                <button class="fm-pill xs ghost" @click="store.fetchShareLinks()">Refresh</button>
              </div>
              <div v-if="lastShare" class="fm-sharebox mono">{{ lastShare }}</div>
              <div v-for="s in sharesForActive" :key="s.id" class="fm-kv small">
                <span class="mono truncate" :title="s.url">{{ s.url }}</span>
                <button class="fm-pill xs ghost" @click="copyText(s.url)">copy</button>
              </div>
              <div v-if="!sharesForActive.length" class="fm-sempty">No active links for this file.</div>
              <div class="fm-side-h">ADD TO COLLECTION</div>
              <div class="fm-btnrow">
                <button v-for="c in store.collections.slice(0, 4)" :key="c.id" class="fm-pill xs ghost" @click="store.addToCollection(c.id, active!.id)">{{ c.name }}</button>
              </div>
              </template>
            </div>
          </div>
        </template>
      </aside>
    </div>

    <!-- dialogs -->
    <Teleport to="body">
      <div v-if="dlgFolder" class="fm-ov" @click.self="dlgFolder = false">
        <div class="fm-modal">
          <h3>New folder</h3>
          <input ref="folderRef" v-model="dlgFolderName" placeholder="folder name" aria-label="Folder name" @keyup.enter="doMkdir" />
          <div class="fm-mrow"><button class="fm-pill ghost" @click="dlgFolder = false">Cancel</button><button class="fm-pill" @click="doMkdir">Create</button></div>
        </div>
      </div>
      <div v-if="dlgRename" class="fm-ov" @click.self="dlgRename = null">
        <div class="fm-modal">
          <h3>Rename</h3>
          <input ref="renameRef" v-model="dlgRenameName" aria-label="New name" @keyup.enter="doRename" />
          <div class="fm-mrow"><button class="fm-pill ghost" @click="dlgRename = null">Cancel</button><button class="fm-pill" @click="doRename">Rename</button></div>
        </div>
      </div>
      <div v-if="dlgDelete" class="fm-ov" @click.self="dlgDelete = null">
        <div class="fm-modal danger">
          <h3>Delete {{ dlgDeleteMany ? dlgDeleteMany + ' items' : '“' + (dlgDelete.name || '') + '”' }}?</h3>
          <p class="dim">{{ deleteIsHost ? 'Permanently deletes from this device — no trash there.' : 'Moves to Trash — restorable.' }}</p>
          <div class="fm-mrow"><button class="fm-pill ghost" @click="dlgDelete = null; dlgDeleteMany = 0">Cancel</button><button class="fm-pill danger" @click="doDelete">Delete</button></div>
        </div>
      </div>
      <div v-if="dlgSync" class="fm-ov" @click.self="dlgSync = null">
        <div class="fm-modal">
          <h3>Sync {{ (dlgSyncIds?.length || 1) }} file(s) to…</h3>
          <button v-for="c in store.syncConfigs.filter(c => c.enabled)" :key="c.id" class="fm-srow wide" @click="doSyncTo(c.id)">
            <i class="fm-pdot" :style="{ background: backendColor(c.backendType) }" />
            <span class="t">{{ backendLabel(c.backendType) }} {{ c.name ? '· ' + c.name : '' }}</span>
          </button>
          <div v-if="!store.syncConfigs.filter(c => c.enabled).length" class="fm-sempty">No enabled providers — open Sync first.</div>
          <div class="fm-mrow"><button class="fm-pill ghost" @click="dlgSync = null">Cancel</button><button class="fm-pill ghost" @click="wm.open('sync')">Sync panel</button></div>
        </div>
      </div>
      <div v-if="dlgProvMove" class="fm-ov" @click.self="dlgProvMove = null">
        <div class="fm-modal">
          <h3>{{ dlgProvMove.op === 'move' ? 'Move' : 'Copy' }} “{{ dlgProvMove.name }}” to…</h3>
          <label class="fm-dlg-label" for="fm-provdest">Destination provider</label>
          <select id="fm-provdest" v-model="dlgProvDestMount" class="fm-select wide" aria-label="Destination provider">
            <option v-for="m in vfsMounts" :key="m.id" :value="m.id">{{ m.name }} · {{ m.backendType }}</option>
          </select>
          <label class="fm-dlg-label" for="fm-provpath">Destination folder (optional)</label>
          <input id="fm-provpath" v-model="dlgProvDestDir" placeholder="folder/subfolder (blank = same folder)" aria-label="Destination folder" @keyup.enter="doProviderMove" />
          <div class="fm-mrow left">
            <button class="fm-chipbtn" :class="{ on: dlgProvOp === 'copy' }" @click="dlgProvOp = 'copy'">copy</button>
            <button class="fm-chipbtn" :class="{ on: dlgProvOp === 'move' }" @click="dlgProvOp = 'move'">move</button>
          </div>
          <div class="fm-mrow"><button class="fm-pill ghost" @click="dlgProvMove = null">Cancel</button><button class="fm-pill" :disabled="!dlgProvDestMount || provMoving" @click="doProviderMove">{{ provMoving ? 'Working…' : (dlgProvOp === 'move' ? 'Move' : 'Copy') }}</button></div>
        </div>
      </div>
      <div v-if="dlgMove" class="fm-ov" @click.self="dlgMove = null">
        <div class="fm-modal">
          <h3>{{ dlgMoveOp === 'move' ? 'Move' : 'Copy' }} {{ dlgMove.nodes.length }} item(s) to…</h3>
          <div class="fm-mrow left">
            <button class="fm-chipbtn" :class="{ on: dlgMoveKind === 'vault' }" @click="dlgMoveKind = 'vault'">vault</button>
            <button class="fm-chipbtn" :class="{ on: dlgMoveKind === 'provider' }" @click="dlgMoveKind = 'provider'">provider</button>
            <button v-if="hasDevice" class="fm-chipbtn" :class="{ on: dlgMoveKind === 'host' }" @click="dlgMoveKind = 'host'">device</button>
          </div>
          <template v-if="dlgMoveKind === 'vault'">
            <label class="fm-dlg-label" for="fm-movedest-vault">Vault folder</label>
            <input id="fm-movedest-vault" v-model="dlgMoveVault" placeholder="/photos" aria-label="Vault folder" @keyup.enter="doMoveTo" />
          </template>
          <template v-else-if="dlgMoveKind === 'provider'">
            <label class="fm-dlg-label" for="fm-movedest-prov">Destination provider</label>
            <select id="fm-movedest-prov" v-model="dlgMoveProvMount" class="fm-select wide" aria-label="Destination provider">
              <option v-for="m in vfsMounts" :key="m.id" :value="m.id">{{ m.name }} · {{ m.backendType }}</option>
            </select>
            <label class="fm-dlg-label" for="fm-movedest-provpath">Destination folder (optional)</label>
            <input id="fm-movedest-provpath" v-model="dlgMoveProvDir" placeholder="folder/subfolder (blank = root)" aria-label="Destination folder" @keyup.enter="doMoveTo" />
          </template>
          <template v-else>
            <label class="fm-dlg-label" for="fm-movedest-host">Device folder</label>
            <input id="fm-movedest-host" v-model="dlgMoveHostDir" :placeholder="isNative ? '/sdcard/Download (blank = here)' : '/Photos (blank = here, inside the picked home folder)'" aria-label="Device folder" @keyup.enter="doMoveTo" />
            <div v-if="isNative" class="fm-mrow left">
              <button class="fm-pill ghost" title="Pick a device folder" @click="pickMoveHostDir">Pick folder…</button>
            </div>
          </template>
          <div class="fm-mrow left">
            <button class="fm-chipbtn" :class="{ on: dlgMoveOp === 'copy' }" @click="dlgMoveOp = 'copy'">copy</button>
            <button class="fm-chipbtn" :class="{ on: dlgMoveOp === 'move' }" @click="dlgMoveOp = 'move'">move</button>
          </div>
          <div class="fm-mrow"><button class="fm-pill ghost" @click="dlgMove = null">Cancel</button><button class="fm-pill" :disabled="moveWorking" @click="doMoveTo">{{ moveWorking ? 'Working…' : (dlgMoveOp === 'move' ? 'Move' : 'Copy') }}</button></div>
        </div>
      </div>
      <div v-if="providerEditor" class="fm-ov fm-local-editor-ov" @click.self="closeLocalProviderEditor">
        <section class="fm-modal fm-local-editor" role="dialog" aria-modal="true" aria-labelledby="fm-local-editor-title">
          <header class="fm-local-editor__head">
            <div class="fm-local-editor__title">
              <span class="fm-local-editor__eyebrow">LOCAL FOLDER · ANDROID SAF</span>
              <h3 id="fm-local-editor-title" :title="providerEditor.name">{{ providerEditor.name }}</h3>
              <span class="fm-local-editor__path" :title="providerEditor.remotePath">{{ providerEditor.remotePath }}</span>
            </div>
            <button class="fm-ibtn" aria-label="Close editor" title="Save and close" :disabled="providerEditor.saving" @click="closeLocalProviderEditor"><AppIcon name="solar:close-bold" :size="16" /></button>
          </header>
          <textarea ref="localEditorTextarea" v-model="providerEditor.text" class="fm-local-editor__text" aria-label="File contents" spellcheck="false" autocapitalize="off" autocomplete="off" @input="queueLocalProviderSave" />
          <footer class="fm-local-editor__foot">
            <span class="fm-local-editor__status" :class="{ error: !!providerEditor.error }" role="status" aria-live="polite">
              {{ providerEditor.error || (providerEditor.saving ? 'Saving to selected folder…' : providerEditor.text !== providerEditor.savedText ? 'Unsaved changes · auto-save in a moment' : 'Saved directly to selected folder') }}
            </span>
            <div class="fm-mrow">
              <button class="fm-pill ghost" :disabled="providerEditor.saving" @click="closeLocalProviderEditor">Close</button>
              <button class="fm-pill" :disabled="providerEditor.saving || providerEditor.text === providerEditor.savedText" @click="saveLocalProviderEditor(true)">{{ providerEditor.saving ? 'Saving…' : 'Save now' }}</button>
            </div>
          </footer>
        </section>
      </div>
      <div v-if="hostEditor" class="fm-ov fm-local-editor-ov" @click.self="closeHostEditor">
        <section class="fm-modal fm-local-editor" role="dialog" aria-modal="true" aria-labelledby="fm-host-editor-title">
          <header class="fm-local-editor__head">
            <div class="fm-local-editor__title">
              <span class="fm-local-editor__eyebrow">DEVICE FILE · SAVES IN PLACE</span>
              <h3 id="fm-host-editor-title" :title="hostEditor.name">{{ hostEditor.name }}</h3>
              <span class="fm-local-editor__path" :title="hostEditor.absPath">{{ hostEditor.absPath }}</span>
            </div>
            <button class="fm-ibtn" aria-label="Close editor" title="Save and close" :disabled="hostEditor.saving" @click="closeHostEditor"><AppIcon name="solar:close-bold" :size="16" /></button>
          </header>
          <textarea ref="hostEditorTextarea" v-model="hostEditor.text" class="fm-local-editor__text" aria-label="File contents" spellcheck="false" autocapitalize="off" autocomplete="off" />
          <footer class="fm-local-editor__foot">
            <span class="fm-local-editor__status" :class="{ error: !!hostEditor.error }" role="status" aria-live="polite">
              {{ hostEditor.error || (hostEditor.saving ? 'Saving to device…' : hostEditor.text !== hostEditor.savedText ? 'Unsaved changes' : 'Saved on this device') }}
            </span>
            <div class="fm-mrow">
              <button class="fm-pill ghost" :disabled="hostEditor.saving" @click="closeHostEditor">Close</button>
              <button class="fm-pill" :disabled="hostEditor.saving || hostEditor.text === hostEditor.savedText" @click="saveHostEditor">{{ hostEditor.saving ? 'Saving…' : 'Save now' }}</button>
            </div>
          </footer>
        </section>
      </div>
      <input ref="filePickRef" type="file" multiple hidden @change="onFilesPicked" />
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { ref, computed, watch, onMounted, onBeforeUnmount, nextTick } from 'vue'
import { useDropZone } from '@vueuse/core'
import { useAppStore } from '@/stores/app'
import { useWindowManager } from '@/composables/useWindowManager'
import { useTheme } from '@/composables/useTheme'
import { useContextMenu } from '@/composables/useContextMenu'
import { useWindowUi } from '@/composables/useWindowUi'
import { invoke, isTauri } from '@/composables/useTauri'
import { destFromVmPath, hostAbsFromVm, isHostId, isHostPath, sourceFromNode } from '@/utils/hostBrowse'
import { hostWasmSupported } from '@/utils/hostWasm'
import { requestInstallWindow } from '@/utils/installLinks'
import { humanBytes, diskPct } from '@/utils/format'
import { trackShellCwd } from '@/utils/shellCwd'
import { decodeProviderText, isTextEditableProviderName, MAX_PROVIDER_TEXT_BYTES } from '@/utils/providerText'
import { copyToClipboard } from '@/utils/clipboard'
import { openLocalFile } from '@/utils/openExternal'
import { ENCRYPTION_INFO, COMPRESSION_INFO, SYNC_BACKEND_INFO } from '@/types'
import type { DiskRow, FileNode, SyncBackendType } from '@/types'

const store = useAppStore()
const wm = useWindowManager()
const ctx = useContextMenu()
const isPlasma = computed(() => useTheme().shellStyle.value === 'plasma')

/* Dolphin extras: editable location bar (Alt+L), icon zoom, Places. */
const crumbEdit = ref(false)
const crumbDraft = ref('')
const crumbInputRef = ref<HTMLInputElement | null>(null)
const zoom = ref(1)
function startCrumbEdit() {
  crumbDraft.value = store.currentPath || '/'
  crumbEdit.value = true
  nextTick(() => crumbInputRef.value?.select())
}
function commitCrumbEdit() {
  crumbEdit.value = false
  const p = crumbDraft.value.trim() || '/'
  jumpCrumb(p)
}
function openTrash() {
  wm.open('trash')
  store.fetchTrashItems()
}

type ViewId = 'grid' | 'list' | 'columns' | 'details' | 'masonry'
const viewModes: { id: ViewId; icon: string; hint: string }[] = [
  { id: 'grid', icon: 'solar:grid-3x3-bold', hint: 'Grid (Ctrl+G)' },
  { id: 'list', icon: 'solar:list-bold', hint: 'List (Ctrl+L)' },
  { id: 'details', icon: 'solar:checklist-bold', hint: 'Details — crypto + hash columns' },
  { id: 'columns', icon: 'solar:columns-3-bold', hint: 'Columns (macOS style)' },
  { id: 'masonry', icon: 'solar:widget-2-bold', hint: 'Masonry' },
]
const view = ref<ViewId>((['grid', 'list', 'masonry'].includes(store.viewMode) ? store.viewMode : 'grid') as ViewId)
function setView(v: ViewId) {
  view.value = v
  if (v === 'grid' || v === 'list' || v === 'masonry') store.viewMode = v
  else store.viewMode = v === 'details' ? 'list' : 'grid'
}

const sortField = ref<'name' | 'size' | 'date' | 'type'>('name')
const sortDir = ref<'asc' | 'desc'>('asc')
const filterQuery = ref('')
const showHidden = ref(false)
const onlyMedia = ref(false)
const density = ref<'cozy' | 'compact'>('cozy')
const renderLimit = ref(240)
const sideOpen = ref(true)
const inspectorOpen = ref(true)
const windowUi = useWindowUi()
const mobileViewportMatches = ref(false)
const isMobileView = computed(() => mobileViewportMatches.value || windowUi.breakpoint.value !== 'lg')
const mobileSideOpen = ref(false)
const mobileInspectorOpen = ref(false)
const mobileToolsOpen = ref(false)
let mobileViewport: MediaQueryList | null = null
function onMobileViewportChange(event: MediaQueryListEvent) {
  mobileViewportMatches.value = event.matches
  if (!event.matches && windowUi.breakpoint.value === 'lg') closeMobilePanels()
}
function toggleSidePanel() {
  if (isMobileView.value) {
    mobileInspectorOpen.value = false
    mobileSideOpen.value = !mobileSideOpen.value
  } else {
    sideOpen.value = !sideOpen.value
  }
}
function toggleInspectorPanel() {
  if (isMobileView.value) {
    mobileSideOpen.value = false
    mobileInspectorOpen.value = !mobileInspectorOpen.value
  } else {
    inspectorOpen.value = !inspectorOpen.value
  }
}
function closeMobilePanels() {
  mobileSideOpen.value = false
  mobileInspectorOpen.value = false
  mobileToolsOpen.value = false
}
function handleMobileBack(event: Event) {
  if (!isMobileView.value) return
  // Only the sheet on top claims the press — background sheets stay mounted.
  if ((event as CustomEvent<{ panel?: string }>).detail?.panel !== 'files') return
  if (providerEditor.value) void closeLocalProviderEditor()
  else if (hostEditor.value) void closeHostEditor()
  else if (mobileToolsOpen.value) mobileToolsOpen.value = false
  else if (mobileInspectorOpen.value) mobileInspectorOpen.value = false
  else if (mobileSideOpen.value) mobileSideOpen.value = false
  else if (termOpen.value) termOpen.value = false
  else return
  event.preventDefault()
}
const inspTab = ref<'info' | 'crypto' | 'distro' | 'vers' | 'share'>('info')
const tabs = [
  { id: 'info', label: 'Info' },
  { id: 'crypto', label: 'Crypto' },
  { id: 'distro', label: 'Distro' },
  { id: 'vers', label: 'Vers' },
  { id: 'share', label: 'Share' },
] as const

const rootRef = ref<HTMLElement | null>(null)
const scrollRef = ref<HTMLElement | null>(null)
const filePickRef = ref<HTMLInputElement | null>(null)
const folderRef = ref<HTMLInputElement | null>(null)
const renameRef = ref<HTMLInputElement | null>(null)

// ── nav history ──
const hist = ref<string[]>([store.currentPath || '/'])
const histIdx = ref(0)
const canBack = computed(() => histIdx.value > 0)
const canFwd = computed(() => histIdx.value < hist.value.length - 1)
function pushHist(p: string) {
  if (hist.value[histIdx.value] === p) return
  hist.value = hist.value.slice(0, histIdx.value + 1)
  hist.value.push(p)
  histIdx.value = hist.value.length - 1
}
async function navTo(path: string) {
  if (isMobileView.value) closeMobilePanels()
  store.currentPath = path
  pushHist(path)
  renderLimit.value = 240
  colPreview.value = null
  await store.fetchFiles(path)
  if (String(path || '').replace(/\\/g, '/').startsWith('/host')) void refreshHostWasmMeta()
}
function goBack() { if (canBack.value) { histIdx.value--; store.currentPath = hist.value[histIdx.value]; void store.fetchFiles(store.currentPath) } }
function goFwd() { if (canFwd.value) { histIdx.value++; store.currentPath = hist.value[histIdx.value]; void store.fetchFiles(store.currentPath) } }
function goUp() {
  const p = store.currentPath.replace(/\/+$/, '') || '/'
  if (p === '/' || p === '') return navTo('/')
  navTo(p.split('/').slice(0, -1).join('/') || '/')
}
function goHome() { store.selectFile(null); void navTo('/') }
function goHomeVault() { clearDiskScope(true); goHome() }
function refresh() { void Promise.all([store.fetchFiles(store.currentPath), store.fetchOsDf(), store.fetchSyncConfigs(), store.fetchDisks()]) }
function jumpCrumb(p: string) { store.selectFile(null); void navTo(p) }
const crumbs = computed(() => {
  const p = store.currentPath || '/'
  const parts = p.split('/').filter(Boolean)
  const norm = p.replace(/\\/g, '/')
  const home = norm === '/host' || norm.startsWith('/host/') ? 'device' : 'vault'
  const out = [{ label: home, path: norm.startsWith('/host') ? '/host' : '/' }]
  let acc = ''
  for (const part of parts) { acc += '/' + part; out.push({ label: part, path: acc }) }
  return out
})

// ── file scope: children of selected folder, else current listing ──
const scopeFolder = computed(() => {
  const s = store.selectedFile
  return s && s.fileType === 'folder' ? s : null
})
const baseFiles = computed(() => {
  let src = store.files
  if (scopeFolder.value) {
    const folderId = scopeFolder.value.id
    const kids = store.files.filter(f => f.parentId === folderId)
    if (kids.length) src = kids
  }
  // Google Drive tab at the /providers root: only googleDrive mounts and
  // their pending placeholders (GitHub/GitLab stay reachable via sidebar).
  if (gdriveOnly.value && store.currentPath.replace(/\\/g, '/') === '/providers') {
    src = src.filter(f => rowBackend(f) === 'googleDrive')
  }
  return src
})
const folders = computed(() => baseFiles.value.filter(f => f.fileType === 'folder').sort((a, b) => a.name.localeCompare(b.name)))
const sortedFiles = computed(() => {
  let fs = [...baseFiles.value]
  if (!showHidden.value) fs = fs.filter(f => !f.isHidden && !f.name.startsWith('.'))
  if (onlyMedia.value) fs = fs.filter(f => /^(image|video|audio)\//.test(f.mimeType || ''))
  const q = filterQuery.value.trim().toLowerCase()
  if (q) fs = fs.filter(f => f.name.toLowerCase().includes(q) || (f.mimeType || '').toLowerCase().includes(q))
  const m = sortDir.value === 'asc' ? 1 : -1
  fs.sort((a, b) => {
    if (sortField.value === 'name') return a.name.localeCompare(b.name) * m
    if (sortField.value === 'size') return (a.sizeBytes - b.sizeBytes) * m
    if (sortField.value === 'date') return (new Date(a.modifiedAt).getTime() - new Date(b.modifiedAt).getTime()) * m
    return ((a.mimeType || a.fileType) > (b.mimeType || b.fileType) ? 1 : -1) * m
  })
  // folders first on name sort
  if (sortField.value === 'name') fs.sort((a, b) => ((b.fileType === 'folder') as unknown as number) - ((a.fileType === 'folder') as unknown as number))
  return fs
})
const renderFiles = computed(() => sortedFiles.value.slice(0, renderLimit.value))
function onScroll(e: Event) {
  const el = e.target as HTMLElement
  if (el.scrollHeight - el.scrollTop - el.clientHeight < 600 && renderLimit.value < sortedFiles.value.length) renderLimit.value += 300
}
watch([filterQuery, sortField, sortDir, showHidden, onlyMedia], () => { renderLimit.value = 240 })
// A scoped disk deleted/detached away must not leave a stale banner.
watch(() => store.disks.map(d => d.id).join(','), () => {
  if (diskScopeId.value && !store.disks.some(d => d.id === diskScopeId.value)) diskScopeId.value = null
})

const folderBytes = computed(() => sortedFiles.value.reduce((s, f) => s + (f.sizeBytes || 0), 0))
const selCount = computed(() => store.selectedFileIds.length)
const selBytes = computed(() => store.selectedFileIds.reduce((s, id) => s + (store.files.find(f => f.id === id)?.sizeBytes || 0), 0))
const allBulk = computed(() => sortedFiles.value.length > 0 && sortedFiles.value.every(f => store.selectedFileIds.includes(f.id)))
const active = computed(() => store.selectedFile)
const isSel = (id: string) => store.selectedFileId === id
const isBulk = (id: string) => store.selectedFileIds.includes(id)

const df = computed(() => store.osDf)
const volPct = computed(() => { const d = df.value; return !d || !d.totalBytes ? 0 : Math.min(100, (d.usedBytes / d.totalBytes) * 100) })
const allTags = computed(() => [...new Set(store.files.flatMap(f => f.tags || []))].sort())

function backendLabel(b: SyncBackendType | string) { return (SYNC_BACKEND_INFO as Record<string, { name: string }>)[b]?.name ?? b }
function backendColor(b: SyncBackendType | string) { return (SYNC_BACKEND_INFO as Record<string, { color: string }>)[b]?.color ?? '#8b93a7' }
const providerChips = computed(() => store.syncConfigs.map(c => ({
  id: c.id,
  label: backendLabel(c.backendType),
  short: backendLabel(c.backendType).split(' ')[0],
  color: backendColor(c.backendType),
  enabled: c.enabled,
  detail: c.repoName || c.basePath || '',
})))

// ── Google Drive scope tab ────────────────────────────────────
// Aggregates every googleDrive sync config's external files in one
// tab: mounts all Drive configs, lists them under /providers (Drive
// mounts only), and reuses the provider banner (↓ Vault / ⇄ Move).
const gdriveOnly = ref(false)
const gdriveConfigs = computed(() => store.syncConfigs.filter(c => c.backendType === 'googleDrive'))
/** Backend of a `/providers/<mountId>` root row (mount or pending placeholder). */
function rowBackend(f: FileNode): string | null {
  const segs = String(f.path || f.id || '').replace(/\\/g, '/').split('/').filter(Boolean)
  if (segs[0] !== 'providers' || segs.length < 2) return null
  const mountId = segs[1]
  let cfgId = ''
  if (mountId.startsWith('pending-')) cfgId = mountId.slice('pending-'.length)
  else cfgId = vfsMounts.value.find(m => m.id === mountId)?.configId || ''
  return cfgId ? (store.syncConfigs.find(c => c.id === cfgId)?.backendType ?? null) : null
}
/** Active scope tab: 'gdrive' inside the Drive view, 'vault' on vault paths, null elsewhere. */
const scopeTab = computed<'vault' | 'gdrive' | null>(() => {
  const p = store.currentPath.replace(/\\/g, '/')
  if (!p.startsWith('/providers')) return 'vault'
  if (p === '/providers') return gdriveOnly.value ? 'gdrive' : null
  const segs = p.split('/').filter(Boolean)
  return rowBackend({ id: '', path: `/providers/${segs[1] || ''}` } as FileNode) === 'googleDrive' ? 'gdrive' : null
})
async function openGdriveTab() {
  clearDiskScope(true)
  gdriveOnly.value = true
  for (const c of store.syncConfigs.filter(c => c.enabled && c.backendType === 'googleDrive')) {
    try { await ensureMountForConfig(c.id) } catch (e) { store.notifyError('Mount failed: Google Drive', e) }
  }
  store.selectFile(null)
  await navTo('/providers')
}
function openVaultTab() { gdriveOnly.value = false; goHomeVault() }

// ── providers in the file manager ─────────────────────────────
// Every connected Google Drive / GitHub / GitLab repo is browsable
// here (not only in Sync/Transfer). Mounts are the canal's VFS rows
// (`providers/<mountId>/…`, carried inside `.cybermanju`); one mount
// per sync config is ensured on demand so all repos/files/folders show.
interface ProvMount { id: string; configId: string; name: string; backendType: string; basePath?: string }
const vfsMounts = ref<ProvMount[]>([])
interface LocalProviderEditorState {
  mountId: string
  remotePath: string
  name: string
  text: string
  savedText: string
  saving: boolean
  error: string
}
const providerEditor = ref<LocalProviderEditorState | null>(null)
const localEditorTextarea = ref<HTMLTextAreaElement | null>(null)
let localEditorTimer: ReturnType<typeof setTimeout> | null = null
async function refreshVfsMounts() {
  try {
    const { listVfsMounts } = await import('@/composables/useProviderCanal')
    vfsMounts.value = (await listVfsMounts()) as ProvMount[]
  } catch { /* canal unavailable — sidebar still lists sync configs */ }
}
function providerMountFor(configId: string): string | null {
  return vfsMounts.value.find(m => m.configId === configId)?.id ?? null
}
function providerShortName(c: { backendType: string; name?: string; repoName?: string; basePath?: string }): string {
  const repo = (c.repoName || '').split('/').pop() || c.basePath || c.name || backendLabel(c.backendType)
  return `${backendLabel(c.backendType).split(' ')[0]} · ${String(repo).slice(0, 22)}`
}
async function ensureMountForConfig(configId: string): Promise<string> {
  const hit = providerMountFor(configId)
  if (hit) return hit
  const cfg = store.syncConfigs.find(c => c.id === configId)
  if (!cfg) throw new Error('not_found: provider was deleted in Accounts — pick another one')
  const { saveVfsMount } = await import('@/composables/useProviderCanal')
  const mount = await saveVfsMount({
    configId: cfg.id,
    name: `${backendLabel(cfg.backendType)} · ${(cfg.repoName || cfg.basePath || cfg.name || cfg.id).slice(0, 32)}`,
    backendType: cfg.backendType,
    basePath: cfg.basePath || '',
  })
  await refreshVfsMounts()
  return mount.id
}
async function mountAllProviders() {
  for (const c of store.syncConfigs) {
    try { await ensureMountForConfig(c.id) } catch (e) { store.notifyError(`Mount failed: ${backendLabel(c.backendType)}`, e) }
  }
  await refreshVfsMounts()
  store.notifySuccess(`Providers mounted (${vfsMounts.value.length}/${store.syncConfigs.length})`)
}
/** Browse a provider's files/folders straight from the file manager. */
async function openProvider(configId: string) {
  clearDiskScope(true)
  try {
    const mountId = await ensureMountForConfig(configId)
    store.selectFile(null)
    await navTo(`/providers/${mountId}`)
  } catch (e) { store.notifyError('Cannot open provider', e) }
}
function openMount(mountId: string) {
  clearDiskScope(true)
  store.selectFile(null)
  void navTo(`/providers/${mountId}`)
}
function providerMenu(e: MouseEvent, configId: string) {
  ctx.replaceEntries('fm_provider_row', [
    { id: 'open', label: 'BROWSE FILES', icon: 'solar:folder-open-bold', action: () => void openProvider(configId) },
    { id: 'sync', label: 'SYNC PANEL', icon: 'solar:refresh-bold', action: () => wm.open('sync') },
    { id: 'conn', label: 'CONNECTIONS', icon: 'solar:cloud-bold', action: () => wm.open('accounts', { tab: 'connections' }) },
  ])
  ctx.open(e, 'fm_provider_row')
}
/** Local `providers/<mount>/<remote…>` split (no import needed). */
function splitProviderId(fileId: string): { mountId: string; remotePath: string } | null {
  const segs = String(fileId || '').replace(/\\/g, '/').replace(/^\/+/, '').split('/').filter(Boolean)
  if (segs.length < 2 || segs[0] !== 'providers') return null
  return { mountId: segs[1], remotePath: segs.slice(2).join('/') }
}
function splitProviderPath(path: string): { mountId: string; remotePath: string } | null {
  return splitProviderId(path)
}
const isProviderPath = computed(() => store.currentPath.replace(/\\/g, '/').startsWith('/providers'))
/** Vault rows are the only ones with crypto/versions/shares/tags/sync state. */
function isVaultRow(f: FileNode | null | undefined): boolean {
  if (!f || !f.id) return false
  return !isHostId(f.id) && !splitProviderId(f.id)
}
const activeIsVault = computed(() => isVaultRow(active.value ?? null))
const isProviderFile = (f: FileNode) => String(f.id || '').replace(/\\/g, '/').startsWith('providers/') || splitProviderPath(store.currentPath) !== null
function isLocalScopedProviderFile(f: FileNode): boolean {
  const split = splitProviderId(f.id)
  return !!split?.remotePath && vfsMounts.value.some(m => m.id === split.mountId && m.backendType === 'scopedStorage')
}
function isLocalTextProviderFile(f: FileNode): boolean {
  return isLocalScopedProviderFile(f) && f.fileType === 'file' && isTextEditableProviderName(f.name, f.mimeType || '')
}
function clearLocalEditorTimer() {
  if (localEditorTimer) clearTimeout(localEditorTimer)
  localEditorTimer = null
}
async function openLocalProviderEditor(f: FileNode) {
  const split = splitProviderId(f.id)
  if (!split?.remotePath || !isLocalTextProviderFile(f)) return
  if (f.sizeBytes > MAX_PROVIDER_TEXT_BYTES) {
    store.notifyError('File is too large to edit here', `Text editing is limited to ${MAX_PROVIDER_TEXT_BYTES} bytes.`)
    return
  }
  try {
    const { readVfsFile } = await import('@/composables/useProviderCanal')
    const got = await readVfsFile(split.mountId, split.remotePath, { locator: locatorOf(f) })
    const text = decodeProviderText(got.bytes)
    closeMobilePanels()
    providerEditor.value = { mountId: split.mountId, remotePath: split.remotePath, name: f.name, text, savedText: text, saving: false, error: '' }
    await nextTick()
    localEditorTextarea.value?.focus()
  } catch (e) {
    store.notifyError('Cannot edit this local file as text', e)
  }
}
function queueLocalProviderSave() {
  const state = providerEditor.value
  if (!state || state.text === state.savedText) return
  clearLocalEditorTimer()
  localEditorTimer = setTimeout(() => {
    if (providerEditor.value === state) void saveLocalProviderEditor(false)
  }, 700)
}
async function saveLocalProviderEditor(manual: boolean) {
  const state = providerEditor.value
  if (!state || state.saving || state.text === state.savedText) return
  clearLocalEditorTimer()
  state.saving = true
  state.error = ''
  const snapshot = state.text
  let wrote = false
  try {
    const { writeVfsFile } = await import('@/composables/useProviderCanal')
    await writeVfsFile(state.mountId, state.remotePath, new TextEncoder().encode(snapshot), { locator: state.remotePath })
    if (providerEditor.value !== state) return
    state.savedText = snapshot
    state.error = ''
    wrote = true
    await store.fetchFiles(store.currentPath)
    if (manual) store.notifySuccess(`Saved '${state.name}' to the selected phone folder`)
  } catch (e) {
    if (providerEditor.value === state) {
      state.error = e instanceof Error ? e.message : String(e)
      store.notifyError('Local file save failed', e)
    }
  } finally {
    if (providerEditor.value === state) {
      state.saving = false
      if (wrote && state.text !== state.savedText) queueLocalProviderSave()
    }
  }
}
async function closeLocalProviderEditor() {
  const state = providerEditor.value
  if (!state || state.saving) return
  clearLocalEditorTimer()
  if (state.text !== state.savedText) {
    await saveLocalProviderEditor(false)
    if (providerEditor.value !== state || state.saving || state.text !== state.savedText || state.error) return
  }
  providerEditor.value = null
}
const providerScopeLabel = computed(() => {  const split = splitProviderPath(store.currentPath)
  if (!split) return 'Providers'
  const mount = vfsMounts.value.find(m => m.id === split.mountId)
  const cfg = mount ? store.syncConfigs.find(c => c.id === mount.configId) : undefined
  const who = mount?.name || (cfg ? `${backendLabel(cfg.backendType)} · ${(cfg.repoName || cfg.basePath || '').slice(0, 28)}` : split.mountId)
  return split.remotePath ? `${who} / ${split.remotePath}` : who
})

// ── device files: host filesystem browsed through cybsh `-os` ──────
// Same tree as vault/providers (`/host…`), listed over the os_exec bridge
// so desktop (IPC/REST) and Android (IPC) share every verb; bytes ride
// plugin-fs. On WASM/static builds the same namespace is served by ADDED
// external folders (File System Access API dialog — `hostWasm`): Chromium
// blocks the whole `$HOME` and the Documents/Downloads/Desktop roots
// ("contains system files"), so users add SUBfolders (`/host` lists the
// mounts, `/host/<Label>/…` is one picked tree). Every verb below keeps
// working with zero per-callsite branching.
const isNative = computed(() => {
  try {
    return isTauri()
  } catch {
    return false
  }
})
// Visible when a host exists: native IPC, or the dialog browser API.
const hasDevice = computed(() => {
  if (isNative.value) return true
  try {
    return hostWasmSupported()
  } catch {
    return false
  }
})
// Added external folders on web (mount registry state for sidebar/banner).
const hostWasmState = ref('')
const hostWasmName = ref('')
const hostWasmParked = ref(0)
const wasmMounts = ref<{ id: string; label: string; status: string }[]>([])
async function refreshHostWasmMeta() {
  if (isNative.value) {
    hostWasmState.value = ''
    hostWasmName.value = ''
    hostWasmParked.value = 0
    wasmMounts.value = []
    return
  }
  try {
    const { listHostWasmMounts, hostWasmStatus } = await import('@/utils/hostWasm')
    const mounts = await listHostWasmMounts().catch(() => [])
    wasmMounts.value = mounts
    const st = await hostWasmStatus().catch(() => null)
    hostWasmState.value = st?.status ?? (mounts.length ? 'ready' : 'missing')
    hostWasmParked.value = st?.parked ?? mounts.filter((m) => m.status !== 'ready').length
    hostWasmName.value = mounts.length ? `${mounts.length} folder(s)` : ''
  } catch {
    hostWasmState.value = ''
    hostWasmName.value = ''
    hostWasmParked.value = 0
    wasmMounts.value = []
  }
}
const isHostPathNow = computed(() => isHostPath(store.currentPath))
const hostAbsNow = computed(() => hostAbsFromVm(store.currentPath))
/** Mount label behind the current listing (`/host/Docs/a` → `Docs`). */
const hostMountLabel = computed(() => {
  if (isNative.value) return ''
  const m = /^\/([^/]+)/.exec(hostAbsNow.value || '')
  return m ? m[1] : ''
})
async function openHostRoot() {
  if (!isNative.value) {
    // Web: added external folders are the host. When none is added yet,
    // pick one now (still the same click gesture the picker requires) —
    // guiding to a SUBfolder, since Chrome blocks `$HOME`/system roots.
    try {
      const { listHostWasmMounts, pickHostWasmDir, isPickerCancel } = await import('@/utils/hostWasm')
      const mounts = await listHostWasmMounts().catch(() => [])
      if (!mounts.length) {
        store.notifySuccess('Pick a SUBfolder (e.g. create CyberManju inside Documents) — Chrome blocks the whole $HOME and system folders')
        try {
          const picked = await pickHostWasmDir()
          clearDiskScope(true)
          store.selectFile(null)
          await navTo(`/host/${picked.label}`)
          await refreshHostWasmMeta()
          store.notifySuccess(`'${picked.name}' added — browse it under Device files`)
          return
        } catch (e) {
          if (isPickerCancel(e)) return // dismissed — stay silent
          store.notifyError('Cannot add folder', e)
          return
        }
      }
    } catch (e) {
      store.notifyError('Cannot open device files', e)
      return
    }
    clearDiskScope(true)
    store.selectFile(null)
    await navTo('/host')
    await refreshHostWasmMeta()
    return
  }
  clearDiskScope(true)
  store.selectFile(null)
  await navTo('/host')
}
function openHostFsRoot() {
  if (!isNative.value) {
    // No filesystem root in a browser tab — the folders list is the root.
    void openHostRoot()
    return
  }
  clearDiskScope(true)
  store.selectFile(null)
  void navTo('/host/')
}
/** Browse one added folder. */
function openHostMount(label: string) {
  clearDiskScope(true)
  store.selectFile(null)
  void navTo(`/host/${label}`)
}
function hostMountMenu(e: MouseEvent, m: { id: string; label: string }) {
  ctx.replaceEntries('fm_host_mount', [
    { id: 'open', label: 'OPEN FOLDER', icon: 'solar:folder-open-bold', action: () => openHostMount(m.label) },
    { id: 'unmount', label: 'UNMOUNT (KEEP FILES)', icon: 'solar:minus-circle-bold', action: () => void unmountHostWasm(m.id, m.label) },
  ])
  ctx.open(e, 'fm_host_mount')
}
/** Forget one added folder (bytes stay on disk — unmount, never delete). */
async function unmountHostWasm(idOrLabel: string, label: string) {
  if (!window.confirm(`Remove '${label}' from Device files? The files stay on disk.`)) return
  try {
    const { removeHostWasmMount } = await import('@/utils/hostWasm')
    await removeHostWasmMount(idOrLabel)
    await refreshHostWasmMeta()
    if (store.currentPath === `/host/${label}` || store.currentPath.startsWith(`/host/${label}/`)) {
      await navTo('/host')
    } else {
      await store.fetchFiles(store.currentPath)
    }
    store.notifySuccess(`'${label}' removed — files stay on disk`)
  } catch (e) {
    store.notifyError('Cannot remove folder', e)
  }
}
async function pickHostFolder() {
  if (!isNative.value) {
    // Dialog browser API: one external SUBfolder per pick (Chrome blocks
    // `$HOME`/system roots) — each lands as `/host/<Label>`.
    const { pickHostWasmDir, isPickerCancel } = await import('@/utils/hostWasm')
    try {
      const picked = await pickHostWasmDir()
      clearDiskScope(true)
      store.selectFile(null)
      await navTo(`/host/${picked.label}`)
      await refreshHostWasmMeta()
      store.notifySuccess(`'${picked.name}' added — browse it under Device files`)
    } catch (e) {
      if (isPickerCancel(e)) return // dismissed — stay silent
      // The central notifyError net opens the Install window when the
      // failure is a missing folder picker; anything else stays a toast.
      store.notifyError('Cannot add folder', e)
    }
    return
  }
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const sel = await open({ directory: true, multiple: false })
    if (typeof sel === 'string' && sel) {
      const { vmForHost } = await import('@/utils/hostBrowse')
      clearDiskScope(true)
      store.selectFile(null)
      await navTo(vmForHost(sel))
    }
  } catch (e) {
    store.notifyError('Cannot pick a device folder', e)
  }
}
/** Open the Install window (native builds per platform, App × CLI). */
function openInstall(reason = 'host-unsupported') {
  requestInstallWindow(reason)
}
/** User-gesture re-grant for parked folders, then re-list. */
async function reallowHostWasm() {
  try {
    const { hostWasmReallow } = await import('@/utils/hostWasm')
    const ok = await hostWasmReallow()
    await refreshHostWasmMeta()
    if (ok) {
      store.notifySuccess('Folders re-allowed')
      await store.fetchFiles(store.currentPath)
    } else {
      store.notifyError('Folders stay parked', 'grant access to browse them')
    }
  } catch (e) {
    store.notifyError('Cannot re-allow the folders', e)
  }
}
/** Absolute host dir behind the current listing (`/host` → `pwd -os`). */
async function currentHostDir(): Promise<string> {
  const { resolveHostDir } = await import('@/utils/hostBrowse')
  return resolveHostDir(store.currentPath)
}
async function openHostWithSystem(f: FileNode) {
  const abs = isHostId(f.id) ? f.id.slice('host:'.length) : hostAbsFromVm(f.path || '')
  if (!abs) {
    store.notifyError('Nothing to open', 'select a device file first')
    return
  }
  if (!isNative.value) {
    // No system opener in a browser tab — download the bytes instead.
    try {
      const { readHostBytes } = await import('@/utils/hostBrowse')
      const bytes = await readHostBytes(abs)
      const url = URL.createObjectURL(new Blob([bytes.buffer as ArrayBuffer], { type: 'application/octet-stream' }))
      const a = document.createElement('a')
      a.href = url
      a.download = f.name || 'file'
      document.body.appendChild(a)
      a.click()
      a.remove()
      setTimeout(() => URL.revokeObjectURL(url), 4000)
    } catch (e) {
      store.notifyError('Cannot download this device file', e)
    }
    return
  }
  try {
    // Platform opener: Android hands the file to a viewer through the
    // Intent/FileProvider bridge — `plugin-shell open` has no `xdg-open` on
    // Android (`Scoped shell IO error: No such file or directory (os error 2)`).
    await openLocalFile(abs)
  } catch (e) {
    store.notifyError('Cannot open with system', e)
  }
}

// ── device text editing (the `-os` counterpart of the SAF text editor) ──
// Host text files open in a modal backed by plugin-fs bytes: strict UTF-8
// decode refuses binary honestly, saves are explicit, and closing with
// unsaved changes saves first (like the phone-folder editor).
const MAX_HOST_TEXT_BYTES = 512 * 1024
interface HostEditorState {
  absPath: string
  name: string
  text: string
  savedText: string
  saving: boolean
  error: string
}
const hostEditor = ref<HostEditorState | null>(null)
const hostEditorTextarea = ref<HTMLTextAreaElement | null>(null)
function isHostTextFile(f: FileNode): boolean {
  return isHostId(f.id)
    && f.fileType === 'file'
    && (f.sizeBytes || 0) <= MAX_HOST_TEXT_BYTES
    && isTextEditableProviderName(f.name, f.mimeType || '')
}
async function openHostEditor(f: FileNode) {
  const abs = isHostId(f.id) ? f.id.slice('host:'.length) : ''
  if (!abs || f.fileType === 'folder') return
  if ((f.sizeBytes || 0) > MAX_HOST_TEXT_BYTES) {
    store.notifyError('File is too large to edit here', `Device text editing is limited to ${MAX_HOST_TEXT_BYTES} bytes.`)
    return
  }
  try {
    const { readHostBytes } = await import('@/utils/hostBrowse')
    const bytes = await readHostBytes(abs)
    let text: string
    try {
      text = new TextDecoder('utf-8', { fatal: true }).decode(bytes)
    } catch {
      store.notifyError('Binary files need a system app', 'open it with OPEN WITH SYSTEM instead')
      return
    }
    if (text.indexOf('\0') >= 0) {
      store.notifyError('Binary files need a system app', 'open it with OPEN WITH SYSTEM instead')
      return
    }
    closeMobilePanels()
    hostEditor.value = { absPath: abs, name: f.name, text, savedText: text, saving: false, error: '' }
    await nextTick()
    hostEditorTextarea.value?.focus()
  } catch (e) {
    store.notifyError('Cannot edit this device file as text', e)
  }
}
async function saveHostEditor() {
  const state = hostEditor.value
  if (!state || state.saving || state.text === state.savedText) return
  state.saving = true
  state.error = ''
  try {
    const { writeHostBytes } = await import('@/utils/hostBrowse')
    await writeHostBytes(state.absPath, new TextEncoder().encode(state.text))
    if (hostEditor.value !== state) return
    state.savedText = state.text
    await store.fetchFiles(store.currentPath)
    store.notifySuccess(`Saved '${state.name}' on this device`)
  } catch (e) {
    if (hostEditor.value === state) {
      state.error = e instanceof Error ? e.message : String(e)
      store.notifyError('Device file save failed', e)
    }
  } finally {
    if (hostEditor.value === state) state.saving = false
  }
}
async function closeHostEditor() {
  const state = hostEditor.value
  if (!state || state.saving) return
  if (state.text !== state.savedText) {
    await saveHostEditor()
    if (hostEditor.value !== state || state.saving || state.text !== state.savedText || state.error) return
  }
  hostEditor.value = null
}

// ── disk scope: click a disk → its ratio + proportional files share ──
// Disks hold block ranges of the merged volume (no per-file owner), so the
// scope shows the disk's used/capacity ratio and its share of the volume
// with a clearly-labelled proportional file-size estimate.
const diskScopeId = ref<string | null>(null)
const diskScope = computed(() => store.disks.find(d => d.id === diskScopeId.value) ?? null)
const diskTotalUsed = computed(() => store.disks.reduce((s, d) => s + (d.usedBytes || 0), 0))
/** This disk's share of the merged volume (by used bytes, capacity fallback). */
const diskShare = computed(() => {
  const d = diskScope.value
  if (!d) return 0
  if (diskTotalUsed.value > 0) return Math.min(1, (d.usedBytes || 0) / diskTotalUsed.value)
  const cap = store.disks.reduce((s, x) => s + (x.capacityBytes || 0), 0)
  if (cap > 0) return Math.min(1, (d.capacityBytes || 0) / cap)
  return 0
})
/** Proportional file-size estimate for the banner (never a placement claim). */
const diskFileBytes = computed(() => Math.round(folderBytes.value * diskShare.value))
function openDiskFiles(d: DiskRow) {
  diskScopeId.value = d.id
  store.selectFile(null)
  colPreview.value = null
  renderLimit.value = 240
  inspectorOpen.value = true
  inspTab.value = 'distro'
  onTab('distro')
}
function clearDiskScope(silent = false) {
  diskScopeId.value = null
  if (!silent) renderLimit.value = 240
}
function manageDisk(d: DiskRow) {
  wm.open('disks', { tab: 'disks', diskId: d.id, highlightId: d.id })
}
function diskMenu(e: MouseEvent, d: DiskRow) {
  const attached = d.state === 'attached'
  ctx.replaceEntries('fm_disk_row', [
    { id: 'open', label: 'OPEN FILES SHARE', icon: 'solar:folder-open-bold', action: () => openDiskFiles(d) },
    { id: 'manage', label: 'MANAGE DISK', icon: 'solar:ssd-square-bold', action: () => manageDisk(d) },
    { id: 'div0', label: '', divider: true },
    attached
      ? { id: 'detach', label: 'DETACH DISK', icon: 'solar:minus-circle-bold', action: () => store.detachDisk(d.id) }
      : { id: 'attach', label: 'ATTACH DISK', icon: 'solar:plug-circle-bold', action: () => store.attachDisk(d.id, '') },
    { id: 'check', label: 'CHECK DISK', icon: 'solar:shield-check-bold', action: () => store.checkDisk(d.id) },
    { id: 'div1', label: '', divider: true },
    { id: 'copy', label: 'COPY DISK ID', icon: 'solar:copy-bold', action: () => copyText(d.id) },
    ...(diskScopeId.value === d.id
      ? [{ id: 'clear', label: 'CLEAR FILTER', icon: 'solar:close-bold', action: () => clearDiskScope() }]
      : []),
  ])
  ctx.open(e, 'fm_disk_row')
}

// ── icons / labels ──
function iconFor(f: FileNode): string {
  if (f.fileType === 'folder') return 'solar:folder-bold'
  if (f.encrypted) return 'solar:lock-bold'
  if (encLayer(f) !== 'none') return 'solar:archive-bold'
  const m = f.mimeType || ''
  if (m.startsWith('image/')) return 'solar:gallery-bold'
  if (m.startsWith('video/')) return 'solar:videocamera-bold'
  if (m.startsWith('audio/')) return 'solar:music-note-bold'
  if (/pdf/.test(m)) return 'solar:file-favourite-bold'
  if (/zip|tar|gz|rar|7z|archive/.test(m)) return 'solar:archive-bold'
  if (/text|json|xml|javascript|code/.test(m)) return 'solar:file-text-bold'
  return 'solar:file-bold'
}
function encLayer(f: FileNode): string { return (f.compressionLayers && f.compressionLayers[0]) || 'none' }
function shortType(f: FileNode): string {
  if (f.fileType === 'folder') return 'Folder'
  const m = f.mimeType || ''
  if (!m) return 'File'
  if (m.startsWith('image/')) return 'Image'
  if (m.startsWith('video/')) return 'Video'
  if (m.startsWith('audio/')) return 'Audio'
  if (m.includes('pdf')) return 'PDF'
  if (/zip|tar|gz|rar|7z/.test(m)) return 'Archive'
  if (/text|json|xml/.test(m)) return 'Text'
  return m.split('/')[1]?.toUpperCase() || 'File'
}
function cryptoLabel(f: FileNode): string {
  const e = f.encrypted ? (f.encryptionAlgorithm || 'ENC').toUpperCase() : 'plain'
  const c = encLayer(f)
  return c === 'none' ? e : `${e} · ${c.toUpperCase()}`
}
function fmtDate(s: string): string {
  if (!s) return '—'
  const d = new Date(s)
  return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })
}
function arrow(f: string) { return sortField.value === f ? (sortDir.value === 'asc' ? '↑' : '↓') : '' }
function setSort(f: 'name' | 'size' | 'date' | 'type') {
  if (sortField.value === f) sortDir.value = sortDir.value === 'asc' ? 'desc' : 'asc'
  else { sortField.value = f; sortDir.value = 'asc' }
}
function hideImg(e: Event) { (e.target as HTMLImageElement).style.display = 'none' }

// ── selection / open ──
function clickFile(f: FileNode, e: MouseEvent) {
  if (isMobileView.value) {
    if (f.fileType === 'folder') { openFolder(f); return }
    store.selectFile(f.id)
    inspectorOpen.value = true
    inspTab.value = 'info'
    mobileSideOpen.value = false
    mobileInspectorOpen.value = true
    return
  }
  if (e.metaKey || e.ctrlKey) { toggleBulk(f.id); return }
  if (e.shiftKey && store.selectedFileId) {
    const ids = sortedFiles.value.map(x => x.id)
    const a = ids.indexOf(store.selectedFileId)
    const b = ids.indexOf(f.id)
    if (a >= 0 && b >= 0) {
      store.selectedFileIds = ids.slice(Math.min(a, b), Math.max(a, b) + 1)
      store.isMultiSelect = true
      return
    }
  }
  store.selectFile(f.id)
}
function toggleBulk(id: string) {
  const i = store.selectedFileIds.indexOf(id)
  if (i >= 0) store.selectedFileIds.splice(i, 1)
  else store.selectedFileIds.push(id)
  store.isMultiSelect = store.selectedFileIds.length > 0
}
function selectAll() { store.selectedFileIds = sortedFiles.value.map(f => f.id); store.isMultiSelect = true }
function clearSel() { store.selectedFileIds = []; store.isMultiSelect = false }
function openFolder(f: FileNode) {
  // Unmounted provider placeholder (`pending-<configId>`): mount first so
  // every connected repo/folder actually opens instead of showing empty.
  const pend = /pending-([^/]+)/.exec(String(f.id || f.path || ''))
  if (pend) { void openProvider(pend[1]); return }
  if (isMobileView.value) closeMobilePanels()
  store.selectFile(f.id)
  const p = f.path || (store.currentPath.replace(/\/+$/, '') + '/' + f.name)
  void navTo(p)
}
function openFile(f: FileNode) {
  if (f.fileType === 'folder') return openFolder(f)
  store.selectFile(f.id)
  // Merged quick-look: the floating preview window is gone — details live
  // in the inspector, so opening a file raises it on the info tab.
  inspectorOpen.value = true
  inspTab.value = 'info'
  if (isMobileView.value) {
    mobileSideOpen.value = false
    mobileInspectorOpen.value = true
  }
  wm.open('files', { inspector: true, inspTab: 'info' })
}
function revealFile(f: FileNode) {
  store.selectFile(f.id)
  if (f.path) void navTo(f.path.split('/').slice(0, -1).join('/') || '/')
}
const colPreview = ref<FileNode | null>(null)
function peekColumn(f: FileNode) { store.selectFile(f.id); colPreview.value = f }
function globalSearch() {
  const q = filterQuery.value.trim()
  if (!q) return
  store.searchQuery = q
  wm.open('search')
  void store.searchFiles(q)
}
function onDrag(e: DragEvent, f: FileNode) { e.dataTransfer?.setData('text/plain', f.id); if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copy' }

// ── context menus (reuse registered contexts) ──
function fileMenu(e: MouseEvent, f: FileNode) {
  if (isHostId(f.id) || (f.path && isHostPath(f.path))) {
    ctx.replaceEntries('file_grid_item', [
      { id: 'open', label: 'OPEN', icon: 'solar:folder-open-bold', action: () => openFile(f) },
      { id: 'system', label: 'OPEN WITH SYSTEM', icon: 'solar:share-bold', action: () => void openHostWithSystem(f) },
      ...(isHostTextFile(f) ? [{ id: 'edit-host', label: 'EDIT TEXT · SAVE TO DEVICE', icon: 'solar:pen-bold', action: () => void openHostEditor(f) }] : []),
      { id: 'move', label: 'MOVE / COPY TO…', icon: 'solar:transfer-horizontal-bold', action: () => openMoveTo(f) },
      { id: 'save', label: 'SAVE TO VAULT ↓', icon: 'solar:download-bold', action: () => void saveHostSelToVault([f]) },
      ...(f.fileType === 'file' ? [{ id: 'vaultenc', label: 'SAVE TO VAULT + ENCRYPT', icon: 'solar:lock-bold', action: () => void vaultEncryptHostSel([f]) }] : []),
      { id: 'div0', label: '', divider: true },
      { id: 'star', label: f.isStarred ? 'UNSTAR' : 'STAR', icon: 'solar:star-bold', action: () => store.toggleStar(f.id) },
      { id: 'rename', label: 'RENAME', icon: 'solar:pen-bold', action: () => beginRename(f) },
      { id: 'copyd', label: 'COPY PATH', icon: 'solar:copy-bold', action: () => copyPath(f) },
      { id: 'div1', label: '', divider: true },
      { id: 'delete', label: 'DELETE FROM DEVICE', icon: 'solar:trash-bin-trash-bold', action: () => askDelete(f) },
    ])
    ctx.open(e, 'file_grid_item')
    return
  }
  if (splitProviderId(f.id)) {
    ctx.replaceEntries('file_grid_item', [
      { id: 'open', label: 'OPEN', icon: 'solar:folder-open-bold', action: () => openFile(f) },
      ...(isLocalTextProviderFile(f) ? [{ id: 'edit-local', label: 'EDIT TEXT · SAVE TO PHONE', icon: 'solar:pen-bold', action: () => void openLocalProviderEditor(f) }] : []),
      { id: 'save', label: 'SAVE TO VAULT ↓', icon: 'solar:download-bold', action: () => void saveProviderFileToVault(f) },
      { id: 'move', label: 'MOVE / COPY TO PROVIDER ⇄', icon: 'solar:transfer-horizontal-bold', action: () => openProviderMove(f) },
      { id: 'div0', label: '', divider: true },
      { id: 'star', label: f.isStarred ? 'UNSTAR' : 'STAR', icon: 'solar:star-bold', action: () => store.toggleStar(f.id) },
      { id: 'copyd', label: 'COPY PATH', icon: 'solar:copy-bold', action: () => copyPath(f) },
      { id: 'div1', label: '', divider: true },
      { id: 'delete', label: 'DELETE FROM PROVIDER', icon: 'solar:trash-bin-trash-bold', action: () => void deleteProviderFile(f) },
    ])
    ctx.open(e, 'file_grid_item')
    return
  }
  ctx.replaceEntries('file_grid_item', [
    { id: 'open', label: 'OPEN', icon: 'solar:folder-open-bold', action: () => openFile(f) },
    { id: 'terminal', label: 'OPEN IN TERMINAL', icon: 'solar:file-terminal-bold', action: () => openFolderInTerm(f) },
    { id: 'preview', label: 'QUICK LOOK', icon: 'solar:eye-bold', action: () => { store.selectFile(f.id); inspectorOpen.value = true; inspTab.value = 'info'; wm.open('files', { inspector: true, inspTab: 'info' }) } },
    { id: 'div0', label: '', divider: true },
    { id: 'compress', label: 'COMPRESS · ZSTD', icon: 'solar:archive-bold', action: () => store.compressFile(f.id, 'zstd') },
    { id: 'encrypt', label: 'ENCRYPT · HYBRID', icon: 'solar:lock-bold', action: () => store.encryptFile(f.id, 'hybrid') },
    { id: 'decrypt', label: 'DECRYPT', icon: 'solar:lock-unlocked-bold', action: () => store.decryptFile(f.id) },
    { id: 'decompress', label: 'DECOMPRESS', icon: 'solar:archive-up-bold', action: () => store.decompressFile(f.id) },
    { id: 'div1', label: '', divider: true },
    { id: 'sync', label: 'SYNC TO PROVIDER…', icon: 'solar:cloud-bold', action: () => { dlgSyncIds.value = [f.id]; dlgSync.value = true } },
    { id: 'share', label: 'SHARE LINK', icon: 'solar:share-bold', action: () => { store.selectFile(f.id); inspTab.value = 'share'; inspectorOpen.value = true; void makeShare(24) } },
    { id: 'star', label: f.isStarred ? 'UNSTAR' : 'STAR', icon: 'solar:star-bold', action: () => store.toggleStar(f.id) },
    { id: 'rename', label: 'RENAME', icon: 'solar:pen-bold', action: () => beginRename(f) },
    { id: 'duplicate', label: 'DUPLICATE', icon: 'solar:copy-add-bold', action: () => store.duplicateFileContext(f.id) },
    { id: 'div2', label: '', divider: true },
    { id: 'versions', label: 'VERSIONS', icon: 'solar:history-bold', action: () => { store.selectFile(f.id); inspTab.value = 'vers'; inspectorOpen.value = true; void store.fetchFileVersions(f.id) } },
    { id: 'perms', label: 'PERMISSIONS', icon: 'solar:key-bold', action: () => { store.selectFile(f.id); wm.open('permissions') } },
    { id: 'delete', label: 'DELETE', icon: 'solar:trash-bin-trash-bold', action: () => askDelete(f) },
  ])
  ctx.open(e, 'file_grid_item')
}
function bgMenu(e: MouseEvent) { ctx.open(e, 'file_grid_bg') }

// ── clipboard copy/cut/paste (any namespace → any folder) ──
// Snapshots are FileNodes, not ids: pasting into another namespace must
// still know each source's kind (vault id, provider locator, host path)
// after the listing has moved on.
const clipboard = ref<{ nodes: FileNode[]; mode: 'copy' | 'cut' }>({ nodes: [], mode: 'copy' })
function copySel() {
  clipboard.value = { nodes: store.files.filter(f => store.selectedFileIds.includes(f.id)), mode: 'copy' }
  store.notifySuccess(`Copied ${clipboard.value.nodes.length} item(s)`)
}
function cutSel() {
  clipboard.value = { nodes: store.files.filter(f => store.selectedFileIds.includes(f.id)), mode: 'cut' }
  store.notifySuccess(`Cut ${clipboard.value.nodes.length} item(s) — paste to move`)
}
async function pasteHere() {
  const { nodes, mode } = clipboard.value
  if (!nodes.length) return
  const op = mode === 'cut' ? 'move' : 'copy'
  const { destFromVmPath, liveMoveDeps, moveAnywhere, resolveHostDir } = await import('@/utils/hostBrowse')
  let dest = destFromVmPath(store.currentPath)
  if (!dest && isHostPath(store.currentPath)) {
    // Native `/host` carries no absolute — resolve the `-os` default dir;
    // the web folders list (`/host` with no mount) refuses honestly here.
    try {
      dest = { kind: 'host', absDir: await resolveHostDir(store.currentPath) }
    } catch (e) {
      store.notifyError('Cannot resolve device folder', e)
      return
    }
  }
  if (!dest) {
    store.notifyError('Cannot paste here', 'this folder is not a move destination')
    return
  }
  const deps = await liveMoveDeps()
  let ok = 0
  for (const f of nodes) {
    const src = sourceFromNode(f, store.currentPath)
    if (!src) continue
    if (op === 'move' && isMountRootMove(src)) continue
    try {
      await moveAnywhere(src, dest, op, deps)
      ok++
    } catch (e) {
      store.notifyError(`Paste failed: ${f.name}`, e)
    }
  }
  if (mode === 'cut') clipboard.value = { nodes: [], mode: 'copy' }
  clearSel()
  await store.fetchFiles(store.currentPath)
  if (ok) store.notifySuccess(op === 'move' ? `Moved ${ok} item(s) here (verified)` : `Copied ${ok} item(s) here`)
}

// ── bulk ──
// Shield engines act on vault rows only: provider/device rows are skipped
// with an honest count instead of one error toast per file.
function vaultSel(): FileNode[] {
  return store.files.filter(f => store.selectedFileIds.includes(f.id) && isVaultRow(f))
}
async function bulkEncrypt() {
  const rows = vaultSel()
  const skipped = store.selectedFileIds.length - rows.length
  let failed = 0
  for (const f of rows) { try { await store.encryptFile(f.id, 'hybrid') } catch { failed++ } }
  if (skipped) store.notifyError(`Bulk encrypt skipped ${skipped} non-vault item(s)`, 'save them to the vault first')
  if (failed) store.notifyError(`Bulk encrypt: ${failed} file(s) failed`, 'see per-file messages')
  clearSel()
}
async function bulkCompress() {
  const rows = vaultSel()
  const skipped = store.selectedFileIds.length - rows.length
  let failed = 0
  for (const f of rows) { try { await store.compressFile(f.id, 'zstd') } catch { failed++ } }
  if (skipped) store.notifyError(`Bulk compress skipped ${skipped} non-vault item(s)`, 'save them to the vault first')
  if (failed) store.notifyError(`Bulk compress: ${failed} file(s) failed`, 'see per-file messages')
  clearSel()
}
function bulkSync() {
  const rows = vaultSel()
  if (!rows.length) { store.notifyError('Nothing vault-side to sync', 'select vault files first (providers/device files sync after a vault save)'); return }
  dlgSyncIds.value = rows.map(f => f.id)
  dlgSync.value = true
}
async function bulkStar() { for (const id of [...store.selectedFileIds]) { try { await store.toggleStar(id) } catch { /* per-file toast already shown */ } } clearSel() }
function askDeleteSel() { dlgDelete.value = null; dlgDeleteMany.value = store.selectedFileIds.length }

// ── dialogs ──
const dlgFolder = ref(false)
const dlgFolderName = ref('')
const dlgRename = ref<FileNode | null>(null)
const dlgRenameName = ref('')
const dlgDelete = ref<FileNode | null>(null)
const dlgDeleteMany = ref(0)
const dlgSync = ref<boolean | null>(null)
const dlgSyncIds = ref<string[]>([])
watch(dlgFolder, v => { if (v) nextTick(() => folderRef.value?.focus()) })
async function doMkdir() {
  if (!dlgFolderName.value.trim()) return
  const name = dlgFolderName.value.trim().replace(/[/\\]+/g, '_')
  if (isHostPathNow.value) {
    // Web folders list (`/host` with no mount behind it) takes no folders —
    // mounts arrive through the picker, subfolders inside a mount.
    if (!isNative.value && !hostAbsNow.value) {
      store.notifyError('Pick a folder first', 'use Add folder… to add an external folder, then create folders inside it')
      dlgFolderName.value = ''
      dlgFolder.value = false
      return
    }
    try {
      const base = hostAbsNow.value || await currentHostDir()
      const { mkdirHost } = await import('@/utils/hostBrowse')
      await mkdirHost(`${base.replace(/\/+$/, '')}/${name}`)
      store.notifySuccess(`Device folder '${name}' created`)
    } catch (e) {
      store.notifyError('Cannot create device folder', e)
    }
    dlgFolderName.value = ''
    dlgFolder.value = false
    await store.fetchFiles(store.currentPath)
    return
  }
  await store.createFolder(name, scopeFolder.value?.id || '')
  dlgFolderName.value = ''
  dlgFolder.value = false
}
function beginRename(f: FileNode) { dlgRename.value = f; dlgRenameName.value = f.name; nextTick(() => renameRef.value?.focus()) }
async function doRename() {
  if (dlgRename.value && dlgRenameName.value.trim()) {
    const target = dlgRenameName.value.trim().replace(/[/\\]+/g, '_')
    const f = dlgRename.value
    if (isHostId(f.id)) {
      // Device rename is a same-folder `mv -os`.
      const abs = f.id.slice('host:'.length)
      const parent = abs.replace(/\\/g, '/').split('/').slice(0, -1).join('/') || '/'
      try {
        const { mvHost } = await import('@/utils/hostBrowse')
        await mvHost(abs, `${parent}/${target}`)
        store.notifySuccess(`Renamed to '${target}' on this device`)
      } catch (e) {
        store.notifyError('Device rename failed', e)
      }
      dlgRename.value = null
      await store.fetchFiles(store.currentPath)
      return
    }
    if (splitProviderId(f.id)) {
      store.notifyError('Provider renames ride on move', 'use MOVE / COPY TO… for provider files')
      dlgRename.value = null
      return
    }
    await store.renameFile(f.id, target)
  }
  dlgRename.value = null
}
function askDelete(f: FileNode) { dlgDeleteMany.value = 0; dlgDelete.value = f }
const deleteIsHost = computed(() => {
  if (dlgDelete.value && isHostId(dlgDelete.value.id)) return true
  if (dlgDeleteMany.value > 0) {
    return store.files
      .filter(f => store.selectedFileIds.includes(f.id))
      .every(f => isHostId(f.id))
  }
  return false
})
// ── user tags (teach search/parse + scene matching on any file) ──
const tagDraft = ref('')
const tagSaving = ref(false)
async function addTag() {
  const file = active.value
  const tag = tagDraft.value.trim()
  if (!file || !tag || tagSaving.value) return
  if (!isVaultRow(file)) { store.notifyError('Tags live on vault files', 'save this file to the vault first'); return }
  if ((file.tags || []).some(t => t.toLowerCase() === tag.toLowerCase())) {
    tagDraft.value = ''
    return
  }
  tagSaving.value = true
  try {
    await store.setFileTags(file.id, [...(file.tags || []), tag])
    tagDraft.value = ''
  } finally {
    tagSaving.value = false
  }
}
async function removeTag(tag: string) {
  const file = active.value
  if (!file || tagSaving.value) return
  if (!isVaultRow(file)) { store.notifyError('Tags live on vault files', 'save this file to the vault first'); return }
  tagSaving.value = true
  try {
    await store.setFileTags(file.id, (file.tags || []).filter(t => t !== tag))
  } finally {
    tagSaving.value = false
  }
}
async function doDelete() {
  // Device deletes are permanent (`rm -os -r` — there is no trash on the
  // host); provider deletes go through the canal; the vault keeps trash.
  const targets: FileNode[] = dlgDeleteMany.value > 0
    ? store.files.filter(f => store.selectedFileIds.includes(f.id))
    : (dlgDelete.value ? [dlgDelete.value] : [])
  const hostTargets = targets.filter(f => isHostId(f.id))
  const rest = targets.filter(f => !isHostId(f.id))
  for (const f of hostTargets) {
    try {
      const { rmHost } = await import('@/utils/hostBrowse')
      await rmHost(f.id.slice('host:'.length), f.fileType === 'folder')
    } catch (e) {
      store.notifyError(`Device delete failed: ${f.name}`, e)
    }
  }
  for (const f of rest.filter(f => splitProviderId(f.id))) {
    try {
      await removeProviderFile(f)
    } catch (e) {
      store.notifyError(`Provider delete failed: ${f.name}`, e)
    }
  }
  const vaultRest = rest.filter(f => !splitProviderId(f.id))
  if (dlgDeleteMany.value > 0) {
    if (vaultRest.length) await store.batchDeleteFiles(vaultRest.map(f => f.id))
  } else if (vaultRest.length && dlgDelete.value) {
    await store.deleteFile(dlgDelete.value.id)
  }
  if (hostTargets.length) {
    clearSel()
    await store.fetchFiles(store.currentPath)
    if (hostTargets.length && !rest.length) store.notifySuccess(`Deleted ${hostTargets.length} item(s) from this device`)
  }
  dlgDelete.value = null
  dlgDeleteMany.value = 0
}
async function doSyncTo(configId: string) {
  const picked = dlgSyncIds.value.length ? [...dlgSyncIds.value] : [...store.selectedFileIds]
  dlgSync.value = null
  dlgSyncIds.value = []
  // The sync engine addresses vault rows by id — provider/device rows sync
  // after a vault save, never directly.
  const ids = picked.filter(id => {
    const f = store.files.find(x => x.id === id)
    return f ? isVaultRow(f) : true
  })
  if (picked.length - ids.length > 0) {
    store.notifyError(`Sync skipped ${picked.length - ids.length} non-vault item(s)`, 'save them to the vault first')
  }
  if (!ids.length) { store.notifyError('Nothing to sync', 'select a vault file first'); return }
  await store.startSync(configId, ids)
  clearSel()
}

// ── upload / new file ──
function onUploadClick() { filePickRef.value?.click() }
// In a provider folder uploads go straight to that Google Drive /
// GitHub / GitLab path via the canal; on the device they land in the
// host folder (plugin-fs bytes); in the vault they use upload_file.
async function uploadDroppedFiles(files: File[] | null) {
  if (!files?.length) return
  const prov = splitProviderPath(store.currentPath)
  if (prov && prov.mountId && !prov.mountId.startsWith('pending-')) {
    const { writeVfsFile } = await import('@/composables/useProviderCanal')
    for (const f of files) {
      try {
        const buf = new Uint8Array(await f.arrayBuffer())
        const dest = prov.remotePath ? `${prov.remotePath.replace(/\/+$/, '')}/${f.name}` : f.name
        await writeVfsFile(prov.mountId, dest, buf)
      } catch (err) { store.notifyError(`Upload failed: ${f.name}`, err) }
    }
    await store.fetchFiles(store.currentPath)
    store.notifySuccess(`Upload complete (${files.length})`)
    return
  }
  if (isHostPathNow.value) {
    // Web folders list takes no files — mounts arrive through the picker.
    if (!isNative.value && !hostAbsNow.value) {
      store.notifyError('Pick a folder first', 'use Add folder… to add an external folder, then drop files inside it')
      return
    }
    try {
      const base = hostAbsNow.value || await currentHostDir()
      const { writeHostBytes } = await import('@/utils/hostBrowse')
      for (const f of files) {
        try {
          const buf = new Uint8Array(await f.arrayBuffer())
          await writeHostBytes(`${base.replace(/\/+$/, '')}/${f.name}`, buf)
        } catch (err) { store.notifyError(`Upload failed: ${f.name}`, err) }
      }
    } catch (err) {
      store.notifyError('Cannot resolve device folder', err)
      return
    }
    await store.fetchFiles(store.currentPath)
    store.notifySuccess(`Upload complete (${files.length})`)
    return
  }
  for (const f of files) {
    try {
      const buf = new Uint8Array(await f.arrayBuffer())
      await invoke('upload_file', { fileName: f.name, fileData: Array.from(buf), parentPath: store.currentPath })
    } catch (err) { store.notifyError(`Upload failed: ${f.name}`, err) }
  }
  await store.fetchFiles(store.currentPath)
  store.notifySuccess(`Upload complete (${files.length})`)
}
const { isOverDropZone } = useDropZone(rootRef, { onDrop: (files) => void uploadDroppedFiles(files ?? []) })
async function onFilesPicked(e: Event) {
  const input = e.target as HTMLInputElement
  const list = input.files
  if (!list?.length) return
  // Same routing as drag-drop: provider folders go to that Google Drive /
  // GitHub / GitLab path via the canal, device folders take bytes, vault
  // folders use upload_file.
  const prov = splitProviderPath(store.currentPath)
  if (prov && prov.mountId && !prov.mountId.startsWith('pending-')) {
    const { writeVfsFile } = await import('@/composables/useProviderCanal')
    for (const f of Array.from(list)) {
      try {
        const buf = new Uint8Array(await f.arrayBuffer())
        const dest = prov.remotePath ? `${prov.remotePath.replace(/\/+$/, '')}/${f.name}` : f.name
        await writeVfsFile(prov.mountId, dest, buf)
      } catch (err) { store.notifyError(`Upload failed: ${f.name}`, err) }
    }
  } else if (isHostPathNow.value) {
    if (!isNative.value && !hostAbsNow.value) {
      store.notifyError('Pick a folder first', 'use Add folder… to add an external folder, then upload inside it')
      input.value = ''
      return
    }
    try {
      const base = hostAbsNow.value || await currentHostDir()
      const { writeHostBytes } = await import('@/utils/hostBrowse')
      for (const f of Array.from(list)) {
        try {
          const buf = new Uint8Array(await f.arrayBuffer())
          await writeHostBytes(`${base.replace(/\/+$/, '')}/${f.name}`, buf)
        } catch (err) { store.notifyError(`Upload failed: ${f.name}`, err) }
      }
    } catch (err) {
      store.notifyError('Cannot resolve device folder', err)
      input.value = ''
      return
    }
  } else {
    for (const f of Array.from(list)) {
      try {
        const buf = new Uint8Array(await f.arrayBuffer())
        await invoke('upload_file', { fileName: f.name, fileData: Array.from(buf), parentPath: store.currentPath })
      } catch (err) { store.notifyError(`Upload failed: ${f.name}`, err) }
    }
  }
  input.value = ''
  await store.fetchFiles(store.currentPath)
  store.notifySuccess('Upload complete')
}
async function copyPath(f: FileNode) { await copyToClipboard(f.path || f.name) }
async function copyText(t: string) { await copyToClipboard(t) }

// ── crypto quick actions ──
const activeAlgoName = computed(() => {
  const a = (active.value?.encryptionAlgorithm || '').toLowerCase()
  const hit = Object.entries(ENCRYPTION_INFO).find(([k]) => k.toLowerCase() === a)
  return hit ? hit[1].name : (active.value?.encrypted ? (active.value?.encryptionAlgorithm || 'ENCRYPTED').toUpperCase() : '—')
})
const activeNist = computed(() => {
  const a = (active.value?.encryptionAlgorithm || '').toLowerCase()
  const hit = Object.entries(ENCRYPTION_INFO).find(([k]) => k.toLowerCase() === a)
  return hit ? hit[1].nistLevel : 0
})
function keyIdOf(f: FileNode): string {
  const k = (f.contextData as Record<string, unknown> | undefined)?.keyId
  return k ? String(k).slice(0, 18) + '…' : ''
}
async function encryptWith(algo: string) { if (active.value) await store.encryptFile(active.value.id, algo) }
async function compressWith(layer: string) { if (active.value) await store.compressFile(active.value.id, layer) }

// ── distribution ──
const probing = ref(false)
const remoteHits = ref<{ name: string; path: string; sizeBytes: number; url: string }[]>([])
async function probeRemote(configId: string) {
  const cfg = store.syncConfigs.find(c => c.id === configId)
  const file = active.value
  if (!cfg || !file) return
  if (!isVaultRow(file)) { store.notifyError('Probing starts in the vault', 'save this file to the vault first'); return }
  const name = file.name
  probing.value = true
  try {
    const rem = await store.listRemoteFiles(cfg, '')
    remoteHits.value = rem.filter(r => r.name.toLowerCase().includes(name.toLowerCase().split('.')[0])).slice(0, 12)
    if (!remoteHits.value.length) store.notifySuccess('Probe done — no name match on this provider')
  } finally { probing.value = false }
}
async function syncFileTo(configId: string) {
  if (!active.value) return
  if (!isVaultRow(active.value)) { store.notifyError('Sync starts in the vault', 'save this file to the vault first'); return }
  await store.startSync(configId, [active.value.id])
}
async function restoreToVault() {
  if (!active.value) return
  if (!isVaultRow(active.value)) { store.notifyError('Restore targets the vault', 'pick a vault file to restore into'); return }
  const cfg = store.syncConfigs.find(c => c.enabled)
  if (!cfg) { wm.open('sync'); return }
  await store.restoreSyncFile(cfg.id, active.value.id)
}

// ── provider → vault (.cybermanju) + provider ⇄ provider ──────
// Provider rows have no vault file id — moves go through the canal:
// read bytes → write destination → verify → delete source on move.
const provMoving = ref(false)
const dlgProvMove = ref<{ mountId: string; remotePath: string; locator: string; name: string; op: 'copy' | 'move'; isDir: boolean } | null>(null)
const dlgProvDestMount = ref('')
const dlgProvDestDir = ref('')
const dlgProvOp = ref<'copy' | 'move'>('move')
function locatorOf(f: FileNode): string {
  const split = splitProviderId(f.id)
  const rel = split?.remotePath || f.path?.split('/').slice(3).join('/') || f.name
  return rel
}
// Vault write: `upload_file` on Tauri (byte-exact); `os_write` text
// fallback on web/static where the Tauri-only upload has no REST twin.
// Binary bytes cannot ride a text volume write — those refuse honestly.
async function vaultWriteBytes(fileName: string, bytes: Uint8Array, parentPath = '/') {
  const parent = parentPath || '/'
  try {
    await invoke('upload_file', { fileName, fileData: Array.from(bytes), parentPath: parent })
    return
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    if (!/Web Mode|WASM Mode|not supported|dashboard/i.test(msg)) throw e
  }
  let text: string | null = null
  try {
    const decoded = new TextDecoder('utf-8', { fatal: true }).decode(bytes)
    text = decoded.indexOf(String.fromCharCode(0)) >= 0 ? null : decoded
  } catch { text = null }
  if (text === null) {
    throw new Error('[Web Mode] binary files need the desktop app or dashboard server to enter the vault (.cybermanju) — text files work here via os_write')
  }
  const safe = String(fileName || 'provider-file').replace(/[\\/]+/g, '_')
  await invoke('os_write', { path: `${parent === '/' ? '' : parent}/${safe}`, content: text })
}
/** Ensure a vault folder chain exists (`a/b/c` → rows for each level). */
async function ensureVaultDirs(rel: string): Promise<string> {
  const segs = String(rel || '').split('/').filter(Boolean)
  let acc = ''
  for (const seg of segs) {
    const parent = acc || '/'
    try {
      await invoke('create_folder', { name: seg, parentId: parent })
    } catch {
      // Already there — vault names merge, which is exactly right.
    }
    acc = parent === '/' ? `/${seg}` : `${parent}/${seg}`
  }
  return acc || '/'
}
async function saveProviderFileToVault(f: FileNode) {
  const split = splitProviderId(f.id) ?? splitProviderPath(store.currentPath + '/' + f.name)
  if (!split || !split.remotePath) { store.notifyError('Nothing to save', 'open a provider file first'); return }
  if (f.fileType === 'folder') {
    // Folders are prefixes on blob stores — import the whole tree,
    // preserving its shape under `/<folder>/…` instead of flattening every
    // basename into the root (which collided and overwrote same-named files
    // from different subfolders).
    const top = split.remotePath.split('/').filter(Boolean).pop() || f.name
    const { listVfsDir, readVfsFile } = await import('@/composables/useProviderCanal')
    await ensureVaultDirs(top)
    const stack = [split.remotePath]
    let n = 0
    while (stack.length) {
      const dir = stack.pop() as string
      const kids = await listVfsDir(split.mountId, dir).catch((e) => { throw e })
      for (const k of kids) {
        const src = [dir, k.name].filter(Boolean).join('/').replace(/^\/+/, '')
        if (k.isDir) { stack.push(src); continue }
        // Relative path under the saved top folder → same shape in vault.
        const rel = src.slice(split.remotePath.length).replace(/^\/+/, '')
        const relDir = rel.split('/').slice(0, -1).join('/')
        const parent = relDir ? await ensureVaultDirs(`${top}/${relDir}`) : `/${top}`
        const got = await readVfsFile(split.mountId, src, { locator: k.locator || src })
        await vaultWriteBytes(k.name, got.bytes, parent)
        n++
      }
    }
    await store.fetchFiles(isProviderPath.value ? store.currentPath : '/')
    store.notifySuccess(`Saved ${n} file(s) to vault /${top}`)
    return
  }
  const { readVfsFile } = await import('@/composables/useProviderCanal')
  const got = await readVfsFile(split.mountId, split.remotePath, { locator: locatorOf(f) })
  await vaultWriteBytes(f.name, got.bytes)
  await store.fetchFiles(isProviderPath.value ? store.currentPath : '/')
  store.notifySuccess(`Saved '${f.name}' to vault (.cybermanju)`)
}
async function saveProviderSelToVault() {
  const ids = store.selectedFileIds.length ? [...store.selectedFileIds] : (active.value ? [active.value.id] : [])
  if (!ids.length) { store.notifyError('Nothing to save', 'select a provider file first'); return }
  let n = 0
  for (const id of ids) {
    const f = store.files.find(x => x.id === id)
    if (!f) continue
    try { await saveProviderFileToVault(f); n++ } catch (e) { store.notifyError(`Save failed: ${f.name}`, e) }
  }
  clearSel()
  if (n) void navTo('/')
}
function openProviderMove(f: FileNode | null) {
  const file = f ?? active.value
  if (!file) { store.notifyError('Nothing to move', 'select a provider file first'); return }
  const split = splitProviderId(file.id)
  if (!split || !split.remotePath) { store.notifyError('Vault files move differently', 'use Sync to provider, or open a provider file to move it'); return }
  dlgProvOp.value = 'move'
  dlgProvDestDir.value = ''
  dlgProvDestMount.value = vfsMounts.value.find(m => m.id !== split.mountId)?.id ?? ''
  dlgProvMove.value = { mountId: split.mountId, remotePath: split.remotePath, locator: locatorOf(file), name: file.name, op: 'move', isDir: file.fileType === 'folder' }
}
async function doProviderMove() {
  const src = dlgProvMove.value
  if (!src || !dlgProvDestMount.value || provMoving.value) return
  const destDir = dlgProvDestDir.value.trim().replace(/^\/+|\/+$/g, '')
  // Same-object guard (files and folders alike): the destination object
  // path must differ from the source locator.
  const destObj = destDir ? `${destDir}/${src.name}` : src.name
  if (dlgProvDestMount.value === src.mountId && destObj === src.remotePath) {
    store.notifyError('Same place', 'pick another provider or a different folder')
    return
  }
  provMoving.value = true
  try {
    // Folders cross as verified trees (one file at a time); files take the
    // same verified copy path — the transfer board's rule, one code path.
    const { liveMoveDeps, moveAnywhere } = await import('@/utils/hostBrowse')
    const deps = await liveMoveDeps()
    const msg = await moveAnywhere(
      { kind: 'provider', mountId: src.mountId, remotePath: src.remotePath, locator: src.locator, name: src.name, isDir: !!src.isDir },
      { kind: 'provider', mountId: dlgProvDestMount.value, remoteDir: destDir },
      dlgProvOp.value,
      deps,
    )
    dlgProvMove.value = null
    await store.fetchFiles(store.currentPath)
    store.notifySuccess(msg)
  } catch (e) {
    store.notifyError(dlgProvOp.value === 'move' ? 'Move failed' : 'Copy failed', e)
  } finally {
    provMoving.value = false
  }
}
async function removeProviderFile(f: FileNode) {
  const split = splitProviderId(f.id)
  if (!split || !split.remotePath) return
  const { deleteVfsFile } = await import('@/composables/useProviderCanal')
  await deleteVfsFile(split.mountId, split.remotePath, { locator: locatorOf(f) })
  await store.fetchFiles(store.currentPath)
  store.notifySuccess(`Deleted '${f.name}' from provider`)
}
async function deleteProviderFile(f: FileNode) {
  const split = splitProviderId(f.id)
  if (!split || !split.remotePath) return
  if (!window.confirm(`Delete '${f.name}' from this provider?`)) return
  try {
    await removeProviderFile(f)
  } catch (e) { store.notifyError('Remote delete failed', e) }
}

// ── move anything → anywhere (vault ⇄ provider ⇄ device) ─────────
// The dialog collects a destination in any namespace; `moveAnywhere` does
// the verified crossing (files and trees, copy or move).
const dlgMove = ref<{ nodes: FileNode[] } | null>(null)
const dlgMoveOp = ref<'copy' | 'move'>('move')
const dlgMoveKind = ref<'vault' | 'provider' | 'host'>('vault')
const dlgMoveVault = ref('/')
const dlgMoveProvMount = ref('')
const dlgMoveProvDir = ref('')
const dlgMoveHostDir = ref('')
const moveWorking = ref(false)
function openMoveTo(f: FileNode | null) {
  const nodes = f
    ? [f]
    : store.files.filter(x => store.selectedFileIds.includes(x.id))
  const list = nodes.length ? nodes : (active.value ? [active.value] : [])
  if (!list.length) {
    store.notifyError('Nothing to move', 'select a file first')
    return
  }
  dlgMoveOp.value = 'move'
  // Default destination: the other side of where we stand.
  if (isHostPathNow.value) {
    dlgMoveKind.value = 'vault'
    dlgMoveVault.value = '/'
  } else if (isProviderPath.value) {
    dlgMoveKind.value = 'vault'
    dlgMoveVault.value = '/'
  } else {
    dlgMoveKind.value = hasDevice.value ? 'host' : 'provider'
    dlgMoveHostDir.value = hostAbsNow.value || ''
    dlgMoveVault.value = store.currentPath || '/'
  }
  if (!dlgMoveProvMount.value) dlgMoveProvMount.value = vfsMounts.value[0]?.id ?? ''
  dlgMove.value = { nodes: list }
}
async function pickMoveHostDir() {
  if (!isNative.value) {
    // The WASM host is the picked tree — destinations are typed paths
    // inside it (`/Photos`), not a second OS picker (which would replace
    // the `$HOME` handle instead of choosing a destination).
    store.notifyError('Type the device folder', 'destinations inside the picked home folder are typed, e.g. /Photos')
    return
  }
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const sel = await open({ directory: true, multiple: false })
    if (typeof sel === 'string' && sel) dlgMoveHostDir.value = sel
  } catch (e) {
    store.notifyError('Cannot pick a device folder', e)
  }
}
async function doMoveTo() {
  const dlg = dlgMove.value
  if (!dlg || moveWorking.value) return
  const { destFromVmPath, liveMoveDeps, moveAnywhere, resolveHostDir, sourceFromNode } = await import('@/utils/hostBrowse')
  moveWorking.value = true
  try {
    let dest = null as null | { kind: 'vault'; vaultPath: string } | { kind: 'provider'; mountId: string; remoteDir: string } | { kind: 'host'; absDir: string }
    if (dlgMoveKind.value === 'vault') {
      const v = dlgMoveVault.value.trim() || '/'
      dest = { kind: 'vault', vaultPath: v.startsWith('/') ? v : `/${v}` }
    } else if (dlgMoveKind.value === 'provider') {
      if (!dlgMoveProvMount.value) {
        store.notifyError('Pick a provider', 'no destination mount selected')
        return
      }
      dest = { kind: 'provider', mountId: dlgMoveProvMount.value, remoteDir: dlgMoveProvDir.value.trim().replace(/^\/+|\/+$/g, '') }
    } else {
      let dir = dlgMoveHostDir.value.trim()
      if (!dir && isHostPathNow.value) dir = hostAbsNow.value || await currentHostDir()
      if (!dir) dir = await resolveHostDir('/host')
      dest = { kind: 'host', absDir: dir }
    }
    // Same-folder no-op guard for the fast paths.
    const deps = await liveMoveDeps()
    let ok = 0
    for (const f of dlg.nodes) {
      const src = sourceFromNode(f, store.currentPath)
      if (!src) continue
      if (dlgMoveOp.value === 'move' && isMountRootMove(src)) continue
      try {
        await moveAnywhere(src, dest, dlgMoveOp.value, deps)
        ok++
      } catch (e) {
        store.notifyError(`${dlgMoveOp.value === 'move' ? 'Move' : 'Copy'} failed: ${f.name}`, e)
      }
    }
    dlgMove.value = null
    clearSel()
    await store.fetchFiles(store.currentPath)
    if (ok) store.notifySuccess(dlgMoveOp.value === 'move' ? `Moved ${ok} item(s) (verified)` : `Copied ${ok} item(s)`)
  } catch (e) {
    store.notifyError('Cannot move here', e)
  } finally {
    moveWorking.value = false
  }
}
/**
 * Mounted web folders (`host:/Label`) copy fine but never MOVE (the
 * cleanup would wipe a whole picked tree — mounts unmount instead).
 * True when the move must be refused up-front (with its own toast).
 */
function isMountRootMove(src: { kind: string; absPath?: string; name?: string }): boolean {
  if (isNative.value || src.kind !== 'host') return false
  const abs = String(src.absPath ?? '')
  if (!/^\/[^/]+$/.test(abs)) return false
  store.notifyError(
    `Move failed: ${src.name || abs}`,
    'mounted folders copy — move the files inside instead (or unmount it)',
  )
  return true
}
/** Device selection → vault root (the ⇄ shortcut beside every host row). */
async function saveHostSelToVault(nodes?: FileNode[]) {
  const list = nodes?.length
    ? nodes
    : store.files.filter(x => store.selectedFileIds.includes(x.id))
  const picked = list.length ? list : (active.value ? [active.value] : [])
  if (!picked.length) {
    store.notifyError('Nothing to save', 'select a device file first')
    return
  }
  const { liveMoveDeps, moveAnywhere, sourceFromNode } = await import('@/utils/hostBrowse')
  const deps = await liveMoveDeps()
  let n = 0
  for (const f of picked) {
    const src = sourceFromNode(f, store.currentPath)
    if (!src || src.kind !== 'host') continue
    try {
      await moveAnywhere(src, { kind: 'vault', vaultPath: '/' }, 'copy', deps)
      n++
    } catch (e) {
      store.notifyError(`Save failed: ${f.name}`, e)
    }
  }
  clearSel()
  if (n) {
    await navTo('/')
    store.notifySuccess(`Saved ${n} file(s) to the vault`)
  }
}
/**
 * Upload bytes and return the vault node on every transport. Tauri answers
 * `UploadResult.file_node`, the WASM pack answers the node itself; REST has
 * no byte-upload endpoint, so text falls back through `os_write` and the
 * row is located by name (newest first) — binary refuses honestly there.
 */
async function uploadBytesNode(fileName: string, bytes: Uint8Array, parentPath = '/'): Promise<FileNode> {
  const clean = String(fileName || 'file').replace(/[\\/]+/g, '_')
  const parent = parentPath || '/'
  try {
    const res = await invoke<unknown>('upload_file', { fileName: clean, fileData: Array.from(bytes), parentPath: parent })
    const node = (res && typeof res === 'object' && 'file_node' in (res as Record<string, unknown>)
      ? (res as Record<string, unknown>).file_node
      : res) as FileNode | undefined
    if (node && node.id) return node
    throw new Error('integrity: upload returned no node — retry')
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e)
    if (!/Web Mode|WASM Mode|not supported|dashboard|network:/i.test(msg)) throw e
    await vaultWriteBytes(clean, bytes, parent)
    const rows = await invoke<FileNode[]>('list_files', { parentPath: parent })
    const hits = rows
      .filter((r) => r.name === clean)
      .sort((a, b) => new Date(b.modifiedAt).getTime() - new Date(a.modifiedAt).getTime())
    if (!hits[0]) throw new Error('integrity: saved but the vault row is not visible — encrypt it from the vault')
    return hits[0]
  }
}
/**
 * Device files → vault AND encrypted there (the crypto pipeline is
 * vault-row based: keys, versions and shares only exist in the vault).
 * Files only — folders save plain first, then encrypt file-by-file inside.
 */
async function vaultEncryptHostSel(nodes?: FileNode[]) {
  const list = nodes?.length
    ? nodes
    : store.files.filter((x) => store.selectedFileIds.includes(x.id))
  const picked = list.length ? list : (active.value ? [active.value] : [])
  if (!picked.length) {
    store.notifyError('Nothing to encrypt', 'select a device file first')
    return
  }
  const dirs = picked.filter((f) => f.fileType === 'folder').length
  if (dirs) {
    store.notifyError(
      `Skipped ${dirs} folder(s)`,
      'folders save plain first (↓ Vault) — then encrypt their files inside the vault',
    )
  }
  let ok = 0
  for (const f of picked) {
    if (!isHostId(f.id) || f.fileType === 'folder') continue
    try {
      const { readHostBytes } = await import('@/utils/hostBrowse')
      const bytes = await readHostBytes(f.id.slice('host:'.length))
      const node = await uploadBytesNode(f.name, bytes, '/')
      await store.encryptFile(node.id, 'hybrid')
      ok++
    } catch (e) {
      store.notifyError(`Encrypt failed: ${f.name}`, e)
    }
  }
  clearSel()
  if (ok) {
    await navTo('/')
    store.notifySuccess(`Saved + encrypted ${ok} file(s) in the vault`)
  }
}
function onTab(t: string) {
  if (!active.value) return
  // Device/provider rows carry no vault state (versions, shares, crypto
  // pipeline) — their tabs explain that instead of firing doomed fetches.
  if (!activeIsVault.value && (t === 'vers' || t === 'share' || t === 'crypto')) return
  if (t === 'vers') void store.fetchFileVersions(active.value.id)
  if (t === 'share') void store.fetchShareLinks()
  if (t === 'crypto') void store.fetchEncryptionStatus()
  if (t === 'distro') void Promise.all([store.fetchSyncConfigs(), store.fetchOsDf(), store.fetchDisks()])
}
watch(() => store.selectedFileId, id => {
  if (!id) return
  if (!activeIsVault.value) return
  if (inspTab.value === 'vers') void store.fetchFileVersions(id)
  if (inspTab.value === 'crypto') void store.fetchEncryptionStatus()
})

// Merged quick-look: `preview` opens Files with the inspector raised.
const winProps = defineProps<{ inspector?: boolean; inspTab?: string }>()
function applyWindowProps() {
  if (winProps.inspector) inspectorOpen.value = true
  const t = winProps.inspTab
  if (t === 'info' || t === 'crypto' || t === 'distro' || t === 'vers' || t === 'share') {
    inspTab.value = t
    onTab(t)
  }
}
watch(() => [winProps.inspector, winProps.inspTab], applyWindowProps)

function getFaceGroupName(groupId: string): string {
  return store.faceGroups.find(fg => fg.id === groupId)?.name || groupId
}

// ── share ──
const lastShare = ref('')
const sharesForActive = computed(() => active.value ? store.shareLinks.filter(s => s.fileId === active.value?.id) : [])
async function makeShare(hours: number) {
  if (!active.value) return
  if (!isVaultRow(active.value)) { store.notifyError('Sharing starts in the vault', 'save this file to the vault first'); return }
  const s = await store.generateShareLink(active.value.id, hours)
  if (s) { lastShare.value = s.url; await copyText(s.url) }
}

// ── inline terminal (cybsh, scoped to current folder) ──
const termOpen = ref(false)
const termCwd = ref(store.currentPath || '/')
const termInput = ref('')
const termLines = ref<{ id: number; kind: string; text: string }[]>([])
const termHistArr = ref<string[]>([])
const termHistIdx = ref(-1)
const termScrollRef = ref<HTMLElement | null>(null)
const termInputRef = ref<HTMLInputElement | null>(null)
let termId = 0
function termPush(kind: string, text: string) {
  for (const line of text.split('\n')) {
    if (line === '' && kind !== 'in') continue
    termLines.value.push({ id: ++termId, kind, text: line || ' ' })
  }
  if (termLines.value.length > 500) termLines.value.splice(0, termLines.value.length - 500)
  nextTick(() => { const el = termScrollRef.value; if (el) el.scrollTop = el.scrollHeight })
}
function sh(p: string) { return `"${p.replace(/"/g, '\\"')}"` }
async function runTerm(line: string) {
  const full = line.trim()
  if (!full) return
  termPush('in', '❯ ' + full)
  // Same mirror as CodeStudio: the server shell is stateful, so track `cd`
  // (plain, relative, quoted, chained) instead of letting the label go stale.
  termCwd.value = trackShellCwd(termCwd.value, full)
  const res = await store.execShellLine(full)
  termPush(res.ok ? 'out' : 'err', res.output || (res.ok ? 'ok' : 'failed'))
}
function submitTerm() {
  const v = termInput.value
  termInput.value = ''
  if (v.trim()) { termHistArr.value.unshift(v); termHistIdx.value = -1 }
  void runTerm(v)
}
function termHist(d: -1 | 1) {
  const n = termHistIdx.value + d
  if (n >= -1 && n < termHistArr.value.length) {
    termHistIdx.value = n
    termInput.value = n === -1 ? '' : termHistArr.value[n]
  }
}
function toggleTerm() {
  termOpen.value = !termOpen.value
  if (termOpen.value) {
    termCwd.value = active.value?.fileType === 'folder' ? (active.value.path || store.currentPath) : store.currentPath
    nextTick(() => termInputRef.value?.focus())
    if (!termLines.value.length) void runTerm(`ls -la ${sh(termCwd.value)}`)
  }
}
function openFolderInTerm(f?: FileNode, full = false) {
  const hostAbs = f && isHostId(f.id)
    ? f.id.slice('host:'.length)
    : (f && f.fileType === 'folder' && f.path && isHostPath(f.path) ? hostAbsFromVm(f.path) : '')
  const dirAbs = hostAbs || (isHostPathNow.value ? (hostAbsNow.value || '') : '')
  if (dirAbs) {
    // Device folder: the same shell with `-os` (host cwd is separate).
    const line = `cd -os ${sh(dirAbs)} && ls -la -os`
    if (full) {
      wm.open('terminal')
      void store.execShellLine(line)
    } else {
      termOpen.value = true
      termCwd.value = dirAbs
      nextTick(() => termInputRef.value?.focus())
      void runTerm(line)
    }
    return
  }
  const p = f ? (f.fileType === 'folder' ? (f.path || store.currentPath + '/' + f.name) : store.currentPath) : store.currentPath
  if (full) { wm.open('terminal'); void store.execShellLine(`cd ${sh(p)}`) }
  else {
    termOpen.value = true
    termCwd.value = p
    nextTick(() => termInputRef.value?.focus())
    void runTerm(`cd ${sh(p)} && ls -la`)
  }
}

// ── keyboard ──
function onKeydown(e: KeyboardEvent) {
  const t = e.target as HTMLElement
  if (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT') return
  if (e.key === 'Delete' || e.key === 'Backspace') { if (store.selectedFileIds.length) askDeleteSel(); else if (active.value) askDelete(active.value) }
  else if (e.key === 'F2' && active.value) beginRename(active.value)
  else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') { e.preventDefault(); selectAll() }
  else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c') copySel()
  else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'x') cutSel()
  else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'v') void pasteHere()
  else if (e.key === 'Enter' && active.value) openFile(active.value)
  else if (e.key === 'F11') { e.preventDefault(); inspectorOpen.value = !inspectorOpen.value }
  else if (e.altKey && e.key.toLowerCase() === 'l') { e.preventDefault(); startCrumbEdit() }
  else if (e.key === 'Escape') { if (providerEditor.value) void closeLocalProviderEditor(); else if (hostEditor.value) void closeHostEditor(); else { clearSel(); termOpen.value = false } }
  else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    const ids = sortedFiles.value.map(f => f.id)
    let i = active.value ? ids.indexOf(active.value.id) : -1
    i = e.key === 'ArrowDown' ? Math.min(ids.length - 1, i + 1) : Math.max(0, i - 1)
    if (ids[i]) store.selectFile(ids[i])
  }
}

onMounted(() => {
  mobileViewport = window.matchMedia?.('(max-width: 900px)') ?? null
  if (mobileViewport) {
    mobileViewportMatches.value = mobileViewport.matches
    mobileViewport.addEventListener?.('change', onMobileViewportChange)
  }
  window.addEventListener('cybermanju:mobile-back', handleMobileBack)
  void Promise.all([store.fetchOsDf(), store.fetchSyncConfigs(), store.fetchDisks(), store.fetchCollections()])
    .then(async () => {
      // Every connected Google Drive / GitHub / GitLab repo is browsable
      // here: refresh mounts, then mount any enabled-but-unmounted config
      // so "all providers" is the default view, not a manual step.
      await refreshVfsMounts().catch(() => {})
      for (const c of store.syncConfigs.filter(c => c.enabled && !providerMountFor(c.id))) {
        try { await ensureMountForConfig(c.id) } catch { /* per-config toast already shown on open */ }
      }
      await refreshVfsMounts().catch(() => {})
    })
    .catch(() => {})
  void store.fetchFiles(store.currentPath)
  void refreshHostWasmMeta()
  applyWindowProps()
})
onBeforeUnmount(() => {
  clearLocalEditorTimer()
  mobileViewport?.removeEventListener?.('change', onMobileViewportChange)
  window.removeEventListener('cybermanju:mobile-back', handleMobileBack)
})
</script>

<style scoped>
.fm { position: relative; display: flex; flex-direction: column; height: 100%; overflow: hidden; outline: none; container-type: size; container-name: file-manager;
  background: var(--ui-surface);
  color: var(--ui-text); font-size: 12px; }
.fm-aurora { display: none; }
.fm-grain { display: none; }
.fm > *:not(.fm-aurora):not(.fm-grain) { position: relative; z-index: 1; }
/* OS drop target (VueUse useDropZone): host-OS files land straight in the vault. */
.fm-drop { outline: 2px dashed color-mix(in srgb, var(--ui-accent) 70%, transparent); outline-offset: -6px; }

/* top bar */
.fm-top { display: flex; align-items: center; gap: 8px; padding: 8px 10px;
  background: var(--ui-glass); border-bottom: 1px solid var(--ui-hairline);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate)); -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate)); }
.fm-nav { display: flex; gap: 2px; }
.fm-ibtn { display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 28px; border-radius: 9px;
  border: 1px solid transparent; background: transparent; color: var(--ui-text-2); cursor: pointer;
  transition: all var(--ui-dur-fast) var(--ui-ease-out); }
.fm-ibtn:hover:not(:disabled) { background: color-mix(in srgb, var(--ui-text) 7%, transparent); color: var(--ui-text); }
.fm-ibtn:disabled { opacity: .3; cursor: default; }
.fm-ibtn.sm { width: 24px; height: 24px; border-radius: 7px; }
.fm-crumbs { display: flex; align-items: center; gap: 2px; flex: 1; min-width: 0; overflow-x: auto; padding: 3px 8px;
  background: color-mix(in srgb, var(--ui-text) 4%, transparent); border: 1px solid var(--ui-hairline); border-radius: 10px; }
.fm-crumb { display: inline-flex; align-items: center; gap: 5px; padding: 3px 7px; border-radius: 7px; white-space: nowrap;
  color: var(--ui-text-3); background: transparent; border: none; cursor: pointer; font-size: 11.5px; font-weight: 600; }
.fm-crumb:hover { color: var(--ui-text); background: color-mix(in srgb, var(--ui-text) 7%, transparent); }
.fm-crumb.active { color: var(--ui-text); }
.fm-crumb-sep { opacity: .4; margin-left: 5px; }
.fm-search { display: flex; align-items: center; gap: 6px; min-width: 170px; max-width: 250px; padding: 0 9px; height: 30px;
  border-radius: 10px; border: 1px solid var(--ui-hairline); background: color-mix(in srgb, var(--ui-text) 4%, transparent); color: var(--ui-text-3); }
.fm-search:focus-within { border-color: var(--ui-accent); box-shadow: var(--ui-focus-ring); color: var(--ui-text); }
.fm-search input { flex: 1; min-width: 0; background: transparent; border: none; outline: none; color: var(--ui-text); font-size: 12px; }
.fm-x { background: none; border: none; color: var(--ui-text-3); cursor: pointer; display: flex; }
.fm-viewswitch { display: flex; gap: 2px; padding: 2px; border-radius: 10px; border: 1px solid var(--ui-hairline); background: color-mix(in srgb, var(--ui-text) 3%, transparent); }
.fm-vbtn { display: flex; padding: 5px 8px; border-radius: 7px; border: none; background: transparent; color: var(--ui-text-3); cursor: pointer; }
.fm-vbtn.active { background: color-mix(in srgb, var(--ui-text) 10%, transparent); color: var(--ui-text); }
.fm-topactions { display: flex; gap: 6px; }
.fm-pill { display: inline-flex; align-items: center; gap: 5px; padding: 5px 11px; border-radius: 999px; cursor: pointer;
  font-size: 11px; font-weight: 600; color: var(--ui-text); background: color-mix(in srgb, var(--ui-text) 6%, transparent);
  border: 1px solid var(--ui-border); transition: background-color var(--ui-dur-fast) var(--ui-ease-out); white-space: nowrap; }
.fm-pill:hover { background: color-mix(in srgb, var(--ui-text) 10%, transparent); }
.fm-pill.ghost { background: transparent; border-color: var(--ui-hairline); color: var(--ui-text-2); }
.fm-pill.ghost.on { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 45%, transparent); }
.fm-pill.xs { padding: 3px 9px; font-size: 10px; }
.fm-pill.danger { background: color-mix(in srgb, var(--ui-danger) 14%, transparent); border-color: color-mix(in srgb, var(--ui-danger) 50%, transparent); color: var(--ui-danger); }

/* sub bar */
.fm-sub { display: flex; align-items: center; gap: 8px; padding: 5px 10px; border-bottom: 1px solid var(--ui-hairline);
  background: color-mix(in srgb, var(--ui-surface) 70%, transparent); font-size: 10.5px; overflow-x: auto; }
.fm-stat { color: var(--ui-text-3); white-space: nowrap; } .fm-stat b { color: var(--ui-text); }
.fm-dot { width: 3px; height: 3px; border-radius: 50%; background: var(--ui-text-faint); flex-shrink: 0; }
/* scope tabs (Vault | Google Drive) */
.fm-scopetabs { display: flex; align-items: center; gap: 2px; flex-shrink: 0; padding: 2px; border-radius: 8px;
  background: color-mix(in srgb, var(--ui-text) 6%, transparent); }
.fm-scopetab { display: inline-flex; align-items: center; gap: 5px; padding: 3px 9px; border: 0; border-radius: 6px;
  background: transparent; color: var(--ui-text-3); font-size: 10.5px; font-weight: 700; cursor: pointer; white-space: nowrap; }
.fm-scopetab:hover { color: var(--ui-text); background: color-mix(in srgb, var(--ui-text) 6%, transparent); }
.fm-scopetab.active { background: var(--ui-surface); color: var(--ui-text); box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25); }
.fm-scopetab-n { font-size: 9px; font-weight: 800; padding: 0 4px; border-radius: 99px; min-width: 14px; text-align: center;
  background: color-mix(in srgb, var(--ui-text) 12%, transparent); color: var(--ui-text-2); }
.fm-scopetab.active .fm-scopetab-n { background: color-mix(in srgb, #34a853 25%, transparent); color: var(--ui-text); }
.fm-spacer { flex: 1; }
.fm-vol { display: flex; align-items: center; gap: 6px; }
.fm-volbar { width: 90px; height: 6px; border-radius: 99px; background: color-mix(in srgb, var(--ui-text) 10%, transparent); overflow: hidden; }
.fm-volbar.big { width: 100%; height: 8px; }
.fm-volbar i { display: block; height: 100%; border-radius: 99px; background: var(--ui-accent); transition: width .5s var(--ui-ease-out); }
.fm-pchip { display: inline-flex; align-items: center; gap: 5px; padding: 2px 8px 2px 6px; border-radius: 99px; cursor: pointer;
  border: 1px solid var(--ui-hairline); background: transparent; color: var(--ui-text-3); font-size: 10px; font-weight: 500; white-space: nowrap; }
.fm-pchip i { width: 7px; height: 7px; border-radius: 50%; }
.fm-pchip.off { opacity: .45; }
.fm-pchip:hover { border-color: var(--ui-border-strong); color: var(--ui-text); }

/* main split */
.fm-main { position: relative; flex: 1; display: grid; grid-template-columns: 212px 1fr 300px; min-height: 0; }
.fm-mobile-panel-head, .fm-mobile-backdrop { display: none; }
.fm-main.no-side { grid-template-columns: 0 1fr 300px; }
.fm-main.no-insp { grid-template-columns: 212px 1fr 0; }
.fm-main.no-side.no-insp { grid-template-columns: 0 1fr 0; }
.fm-side { border-right: 1px solid var(--ui-separator); overflow-y: auto; padding: 8px;
  background: var(--ui-sidebar); backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate)); }
.fm-main.no-side .fm-side { display: none; }
.fm-side-sec { margin-bottom: 12px; }
.fm-side-h { font-size: 11px; font-weight: 600; letter-spacing: 0; color: var(--ui-text-2); padding: 4px 6px;
  text-transform: lowercase; }
.fm-side-h::first-letter { text-transform: uppercase; }
.fm-srow { display: flex; align-items: center; gap: 7px; width: 100%; min-height: 28px; padding: 0 8px; border-radius: var(--ui-radius-sm); cursor: pointer;
  background: transparent; border: none; color: var(--ui-text-2); font-size: 13px; font-weight: 400; text-align: left;
  transition: background-color var(--ui-dur-fast) ease-out; }
.fm-srow:hover { background: color-mix(in srgb, var(--ui-text) 6%, transparent); color: var(--ui-text); }
/* Selected = accent fill + white text (focused window), grey fill when blurred. */
.fm-srow.active { background: var(--ui-accent); color: var(--ui-on-accent); font-weight: 500; }
.blurred .fm-srow.active { background: color-mix(in srgb, var(--ui-text) 14%, transparent); color: var(--ui-text); }
.fm-srow.wide { border: 1px solid var(--ui-hairline); margin-bottom: 6px; }
.fm-srow .t { flex: 1; min-width: 0; font-weight: 600; } .fm-srow .m { font-size: 9.5px; color: var(--ui-text-3); }
.fm-sempty { font-size: 10.5px; color: var(--ui-text-3); padding: 6px 8px; }
.fm-pdot { width: 8px; height: 8px; border-radius: 50%; background: var(--ui-success); flex-shrink: 0; }
.fm-pdot.detached, .fm-pdot.error { background: var(--ui-danger); }
.fm-tags { display: flex; flex-wrap: wrap; gap: 4px; padding: 2px 4px; }
.fm-tag { font-size: 10px; font-weight: 700; padding: 2px 8px; border-radius: 99px; cursor: pointer;
  border: 1px solid var(--ui-hairline); background: var(--ui-glass); color: var(--ui-text-2); }
.fm-tag:hover { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 45%, transparent); }
.fm-link { background: none; border: none; color: var(--ui-accent); cursor: pointer; font-weight: 700; }

/* center */
.fm-center { position: relative; display: flex; flex-direction: column; min-width: 0; min-height: 0; }
.fm-ctool { display: flex; align-items: center; gap: 6px; padding: 6px 10px; border-bottom: 1px solid var(--ui-hairline); }
.fm-select { background: color-mix(in srgb, var(--ui-text) 5%, transparent); border: 1px solid var(--ui-hairline); color: var(--ui-text);
  font-size: 10.5px; font-weight: 700; padding: 4px 6px; border-radius: 8px; cursor: pointer; }
.fm-chipbtn { font-size: 9.5px; font-weight: 800; letter-spacing: .06em; padding: 3px 9px; border-radius: 99px; cursor: pointer;
  border: 1px solid var(--ui-hairline); background: transparent; color: var(--ui-text-3); }
.fm-chipbtn.on { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 50%, transparent); background: var(--ui-accent-softer); }
.fm-bulk { display: flex; align-items: center; gap: 4px; padding: 5px 10px; background: color-mix(in srgb, var(--ui-accent) 10%, transparent);
  border-bottom: 1px solid color-mix(in srgb, var(--ui-accent) 30%, transparent); }
/* disk scope banner: per-disk ratio + volume share (block-level, estimate labelled) */
.fm-diskscope { display: flex; align-items: center; gap: 8px; padding: 5px 10px; font-size: 10.5px;
  background: color-mix(in srgb, var(--ui-accent) 8%, transparent);
  border-bottom: 1px solid color-mix(in srgb, var(--ui-accent) 25%, transparent); color: var(--ui-text-2); }
.fm-diskscope-t { display: inline-flex; align-items: baseline; gap: 5px; min-width: 0; max-width: 40%; }
.fm-diskscope-t b { color: var(--ui-text); }
.fm-diskscope-bar { width: 110px; height: 6px; border-radius: 99px; flex-shrink: 0;
  background: color-mix(in srgb, var(--ui-text) 10%, transparent); overflow: hidden; }
.fm-diskscope-bar i { display: block; height: 100%; border-radius: 99px; background: var(--ui-accent); }
.fm-kv.as-row { width: 100%; cursor: pointer; background: transparent; border: 1px solid transparent; border-radius: 8px; padding: 4px 6px; color: inherit; font: inherit; text-align: left; }
.fm-kv.as-row:hover { background: color-mix(in srgb, var(--ui-text) 5%, transparent); }
.fm-kv.as-row.active { border-color: color-mix(in srgb, var(--ui-accent) 50%, transparent); background: var(--ui-accent-softer); }
.fm-bulk-n { font-size: 10.5px; font-weight: 800; color: var(--ui-accent); margin-right: 6px; }
.fm-bulk button { display: inline-flex; padding: 5px 8px; border-radius: 8px; border: 1px solid var(--ui-hairline);
  background: var(--ui-glass); color: var(--ui-text-2); cursor: pointer; }
.fm-bulk button:hover:not(:disabled) { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 40%, transparent); }
.fm-bulk button:disabled { opacity: .35; } .fm-bulk button.danger { color: var(--ui-danger); }

/* grid — perf: containment + content-visibility */
.fm-grid { flex: 1; min-height: 0; overflow-y: auto; overscroll-behavior: contain; touch-action: pan-x pan-y;
  scrollbar-gutter: stable; padding: 12px; display: grid; gap: 10px; align-content: start;
  grid-template-columns: repeat(auto-fill, minmax(calc(128px * var(--fm-zoom, 1)), 1fr)); }
.fm-locedit { flex: 1; min-width: 0; background: transparent; border: none; outline: none; color: var(--ui-text);
  font-size: 12px; padding: 3px 7px; }
.fm-zoom { display: inline-flex; align-items: center; gap: 6px; color: var(--ui-text-3); }
.fm-zoom input[type='range'] { width: 90px; }
.fm-grid::-webkit-scrollbar { width: 8px; }
.fm-grid::-webkit-scrollbar-thumb { background: color-mix(in srgb, var(--ui-text) 28%, transparent);
  border-radius: var(--ui-radius-full); border: 2px solid transparent; background-clip: content-box; }
.fm-grid::-webkit-scrollbar-thumb:hover { background: color-mix(in srgb, var(--ui-text) 42%, transparent); background-clip: content-box; border: 2px solid transparent; }
.fm-grid.masonry { grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); }
.fm-grid.compact { gap: 6px; grid-template-columns: repeat(auto-fill, minmax(104px, 1fr)); }
.fm-card { position: relative; display: flex; flex-direction: column; align-items: center; gap: 3px; padding: 14px 8px 10px;
  border-radius: var(--ui-radius-md); cursor: pointer; content-visibility: auto; contain-intrinsic-size: auto 132px;
  background: transparent; border: 1px solid transparent;
  transition: background-color var(--ui-dur-fast) ease-out; }
.fm-card:hover { background: color-mix(in srgb, var(--ui-text) 6%, transparent); }
/* Icon-view selection = accent fill with white text. */
.fm-card.sel { background: var(--ui-accent); border-color: transparent; }
.fm-card.sel .fm-cname, .fm-card.sel .fm-cmeta, .fm-card.sel .fm-thumb, .fm-card.sel .fm-flags { color: var(--ui-on-accent); }
.blurred .fm-card.sel { background: color-mix(in srgb, var(--ui-text) 14%, transparent); }
.blurred .fm-card.sel .fm-cname, .blurred .fm-card.sel .fm-cmeta, .blurred .fm-card.sel .fm-thumb, .blurred .fm-card.sel .fm-flags { color: var(--ui-text); }
.fm-card.bulk { border-style: dashed; }
.fm-check { position: absolute; top: 7px; left: 7px; width: 16px; height: 16px; border-radius: 6px; display: flex; align-items: center; justify-content: center;
  font-size: 10px; font-weight: 800; border: 1px solid var(--ui-border-strong); color: transparent; background: transparent; cursor: pointer; }
.fm-check.on, .fm-card:hover .fm-check { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent); }
.fm-check.on { background: var(--ui-accent-softer); }
.fm-check.sm { position: static; width: 14px; height: 14px; }
.fm-thumb { height: calc(60px * var(--fm-zoom, 1)); display: flex; align-items: center; justify-content: center; color: var(--ui-text-2); }
.fm-thumb img { max-width: calc(64px * var(--fm-zoom, 1)); max-height: calc(60px * var(--fm-zoom, 1)); border-radius: 8px; object-fit: cover; }
.fm-cname { font-size: 11px; font-weight: 650; width: 100%; text-align: center; }
.fm-cmeta { font-size: 9.5px; color: var(--ui-text-3); }
.fm-flags { position: absolute; top: 8px; right: 8px; display: flex; gap: 3px; color: var(--ui-text-3); }
.fm-empty { grid-column: 1 / -1; display: flex; flex-direction: column; align-items: center; gap: 6px; padding: 48px 20px; color: var(--ui-text-3); }
.fm-empty p { font-weight: 800; color: var(--ui-text-2); margin: 0; } .fm-empty span { font-size: 11px; }
.fm-more { grid-column: 1 / -1; padding: 8px; border-radius: 10px; border: 1px dashed var(--ui-border-strong);
  background: transparent; color: var(--ui-text-2); cursor: pointer; font-weight: 700; }
.fm-spin { width: 14px; height: 14px; border-radius: 50%; border: 2px solid color-mix(in srgb, var(--ui-accent) 30%, transparent);
  border-top-color: var(--ui-accent); animation: fm-rot .7s linear infinite; display: inline-block; }
.fm-spin.sm { width: 11px; height: 11px; }
@keyframes fm-rot { to { transform: rotate(360deg); } }

@media (prefers-reduced-motion: reduce) {
  .fm-spin { animation: none; }
}

/* list */
.fm-listwrap { flex: 1; display: flex; flex-direction: column; min-height: 0; }
.fm-lhead, .fm-lrow { display: grid; grid-template-columns: 30px minmax(0, 3fr) 90px 130px 120px 110px 60px; align-items: center; gap: 6px; padding: 0 10px; }
.fm-lhead { height: 30px; flex-shrink: 0; font-size: 11px; font-weight: 600; letter-spacing: 0; color: var(--ui-text-2);
  border-bottom: 1px solid var(--ui-separator); background: var(--ui-surface); }
.fm-lhead.compact { height: 26px; }
.fm-lhead span { cursor: pointer; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.fm-lhead span:hover { color: var(--ui-text); }
.fm-lbody { flex: 1; min-height: 0; overflow-y: auto; overscroll-behavior: contain; touch-action: pan-x pan-y; }
.fm-lbody::-webkit-scrollbar { width: 8px; }
.fm-lbody::-webkit-scrollbar-thumb { background: color-mix(in srgb, var(--ui-text) 28%, transparent);
  border-radius: var(--ui-radius-full); border: 2px solid transparent; background-clip: content-box; }
.fm-lbody::-webkit-scrollbar-thumb:hover { background: color-mix(in srgb, var(--ui-text) 42%, transparent); background-clip: content-box; border: 2px solid transparent; }
.fm-lrow { height: 34px; border-bottom: 1px solid var(--ui-hairline); cursor: pointer; content-visibility: auto; contain-intrinsic-size: auto 34px; font-size: 12px; }
.fm-lrow.compact { height: 28px; }
/* Zebra rows. */
.fm-lbody .fm-lrow:nth-child(even):not(.sel) { background: color-mix(in srgb, var(--ui-text) 3%, transparent); }
.fm-lrow:hover:not(.sel) { background: color-mix(in srgb, var(--ui-text) 6%, transparent); }
/* List selection = full accent fill, white text; grey when blurred. */
.fm-lrow.sel { background: var(--ui-accent); color: var(--ui-on-accent); }
.fm-lrow.sel .dim, .fm-lrow.sel .fm-ficon { color: var(--ui-on-accent); }
.blurred .fm-lrow.sel { background: color-mix(in srgb, var(--ui-text) 14%, transparent); color: var(--ui-text); }
.blurred .fm-lrow.sel .dim, .blurred .fm-lrow.sel .fm-ficon { color: var(--ui-text-2); }
.fm-lrow.bulk:not(.sel) { background: color-mix(in srgb, var(--ui-accent) 7%, transparent); }
.fm-lrow .c1 { display: flex; align-items: center; gap: 7px; min-width: 0; font-weight: 600; }
.fm-ficon { width: 22px; height: 22px; display: flex; align-items: center; justify-content: center; color: var(--ui-text-2); flex-shrink: 0; }
.fm-ficon img { max-width: 22px; max-height: 22px; border-radius: 5px; }
.mono { font-family: var(--ui-font-mono); font-size: 10.5px; }
.dim { color: var(--ui-text-3); }
.c0 { display: flex; justify-content: center; } .c2 { text-align: right; } .c6 { display: flex; gap: 4px; color: var(--ui-text-3); }

/* columns */
.fm-cols { flex: 1; display: grid; grid-template-columns: 1fr 240px; min-height: 0; }
.fm-col { overflow-y: auto; border-right: 1px solid var(--ui-hairline); }
.fm-col-h { position: sticky; top: 0; padding: 7px 10px; font-size: 11px; font-weight: 600; letter-spacing: 0; color: var(--ui-text-2);
  background: var(--ui-surface); border-bottom: 1px solid var(--ui-separator); }
.fm-colrow { display: flex; align-items: center; gap: 8px; width: 100%; padding: 7px 10px; background: transparent; border: none;
  border-bottom: 1px solid var(--ui-hairline); color: var(--ui-text-2); cursor: pointer; font-size: 12px; text-align: left; }
.fm-colrow:hover { background: var(--ui-accent-softer); }
.fm-colrow.sel { background: var(--ui-accent-softer); color: var(--ui-text); box-shadow: inset 2px 0 0 var(--ui-accent); }
.fm-colprev { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 22px 14px; color: var(--ui-text-2); text-align: center; }
.fm-colbtns { display: flex; gap: 6px; }

/* status + terminal */
.fm-status { display: flex; align-items: center; gap: 8px; padding: 5px 10px; font-size: 10.5px;
  border-top: 1px solid var(--ui-hairline); background: color-mix(in srgb, var(--ui-surface) 75%, transparent); overflow: hidden; white-space: nowrap; }
.fm-sbadge { font-size: 9.5px; font-weight: 800; padding: 2px 8px; border-radius: 99px; border: 1px solid var(--ui-hairline); }
.fm-sbadge.lock { color: var(--ui-warning); border-color: color-mix(in srgb, var(--ui-warning) 45%, transparent); }
.fm-sbadge.zip { color: var(--ui-info); border-color: color-mix(in srgb, var(--ui-info) 45%, transparent); }
.fm-sbadge.clip { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 45%, transparent); }
.fm-term { border-top: 1px solid color-mix(in srgb, var(--ui-accent) 30%, transparent); background: rgba(0,0,0,.55);
  backdrop-filter: blur(12px); display: flex; flex-direction: column; height: 190px; flex-shrink: 0; }
.fm-term-h { display: flex; align-items: center; gap: 7px; padding: 5px 10px; color: var(--ui-text-2); border-bottom: 1px solid var(--ui-hairline); font-size: 10.5px; }
.fm-term-body { flex: 1; min-height: 0; overflow-y: auto; overscroll-behavior: contain; touch-action: pan-x pan-y;
  scrollbar-gutter: stable; padding: 6px 10px; font-family: var(--ui-font-mono); font-size: 10.5px; }
.fm-term-body::-webkit-scrollbar { width: 8px; }
.fm-term-body::-webkit-scrollbar-thumb { background: color-mix(in srgb, var(--ui-text) 28%, transparent);
  border-radius: var(--ui-radius-full); border: 2px solid transparent; background-clip: content-box; }
.fm-term-body::-webkit-scrollbar-thumb:hover { background: color-mix(in srgb, var(--ui-text) 42%, transparent); background-clip: content-box; border: 2px solid transparent; }
.fm-term-l.in { color: var(--ui-accent); font-weight: 700; }
.fm-term-l.out { color: var(--ui-text-2); white-space: pre-wrap; word-break: break-word; }
.fm-term-l.err { color: var(--ui-danger); white-space: pre-wrap; }
.fm-term-in { display: flex; align-items: center; gap: 7px; padding: 6px 10px; border-top: 1px solid var(--ui-hairline); color: var(--ui-accent); }
.fm-term-in input { flex: 1; background: transparent; border: none; outline: none; color: var(--ui-text); font-size: 11.5px; }

/* inspector */
.fm-insp { border-left: 1px solid var(--ui-hairline); display: flex; flex-direction: column; min-height: 0; min-width: 0;
  background: var(--ui-glass); backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate)); -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate)); }
.fm-main.no-insp .fm-insp { display: none; }
.fm-iempty { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 34px 18px; color: var(--ui-text-3); text-align: center; }
.fm-iempty p { font-weight: 800; color: var(--ui-text-2); margin: 0; } .fm-iempty span { font-size: 11px; }
.fm-volcard { width: 100%; margin-top: 12px; padding: 10px; border-radius: 12px; border: 1px solid var(--ui-hairline); background: color-mix(in srgb, var(--ui-text) 3%, transparent); display: flex; flex-direction: column; gap: 7px; }
.fm-ihead { display: flex; align-items: center; gap: 8px; padding: 10px; border-bottom: 1px solid var(--ui-hairline); }
.fm-ithumb { width: 38px; height: 38px; border-radius: 11px; display: flex; align-items: center; justify-content: center; flex-shrink: 0;
  background: var(--ui-accent-softer); color: var(--ui-accent); border: 1px solid color-mix(in srgb, var(--ui-accent) 35%, transparent); }
.fm-ititle { flex: 1; min-width: 0; font-size: 12px; } .fm-ititle .mono { font-size: 9.5px; }
.fm-tabs { display: flex; gap: 2px; padding: 6px 8px 0; border-bottom: 1px solid var(--ui-hairline); }
.fm-tabs button { flex: 1; padding: 6px 2px; font-size: 9.5px; font-weight: 800; letter-spacing: .06em; cursor: pointer;
  background: transparent; border: none; border-bottom: 2px solid transparent; color: var(--ui-text-3); }
.fm-tabs button.active { color: var(--ui-accent); border-bottom-color: var(--ui-accent); }
.fm-ibody { flex: 1; overflow-y: auto; padding: 10px; }
.fm-isec { display: flex; flex-direction: column; gap: 8px; }
.fm-kv { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: 11px; }
.fm-kv span { color: var(--ui-text-3); flex-shrink: 0; } .fm-kv b { font-weight: 650; text-align: right; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
.fm-kv.small { font-size: 10.5px; } .fm-kv.hash b { font-size: 9px; word-break: break-all; }
.fm-btnrow { display: flex; flex-wrap: wrap; gap: 5px; }
.fm-tagedit { display: flex; flex-direction: column; gap: 6px; margin-top: 4px; padding-top: 8px; border-top: 1px solid var(--ui-hairline); }
.fm-tagedit__label { font-size: 9px; font-weight: 700; letter-spacing: 0.8px; color: var(--ui-text-3); }
.fm-tagedit__chips { display: flex; flex-wrap: wrap; gap: 4px; }
.fm-tagedit__chip { display: inline-flex; align-items: center; gap: 4px; font-size: 10.5px; font-weight: 600; padding: 2px 4px 2px 8px; border-radius: var(--ui-radius-full); border: 1px solid color-mix(in srgb, var(--ui-accent) 35%, transparent); background: var(--ui-accent-softer); color: var(--ui-text); }
.fm-tagedit__x { display: inline-flex; align-items: center; justify-content: center; width: 16px; height: 16px; border-radius: 50%; border: none; background: transparent; color: var(--ui-text-3); cursor: pointer; font-size: 12px; line-height: 1; }
.fm-tagedit__x:hover { color: var(--ui-danger); }
.fm-tagedit__form { display: flex; gap: 6px; }
.fm-tagedit__input { flex: 1; min-width: 0; background: color-mix(in srgb, var(--ui-surface) 80%, transparent); border: 1px solid var(--ui-border); border-radius: var(--ui-radius-sm); color: var(--ui-text); font-family: var(--ui-font); font-size: 11px; padding: 4px 8px; outline: none; }
.fm-tagedit__input:focus { border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent); }
.fm-cryptocard { padding: 9px 10px; border-radius: 12px; border: 1px solid var(--ui-hairline); background: color-mix(in srgb, var(--ui-text) 3%, transparent); display: flex; flex-direction: column; gap: 5px; }
.fm-cryptocard.on { border-color: color-mix(in srgb, var(--ui-warning) 45%, transparent); box-shadow: 0 0 18px color-mix(in srgb, var(--ui-warning) 12%, transparent); }
.fm-algos { display: grid; grid-template-columns: 1fr 1fr; gap: 5px; }
.fm-algo { display: flex; flex-direction: column; gap: 1px; padding: 6px 8px; border-radius: 10px; cursor: pointer; text-align: left;
  border: 1px solid var(--ui-hairline); background: transparent; color: var(--ui-text-2); }
.fm-algo b { font-size: 10px; } .fm-algo span { font-size: 9px; color: var(--ui-text-3); font-weight: 800; }
.fm-algo:hover { border-color: color-mix(in srgb, var(--ui-accent) 45%, transparent); }
.fm-algo.cur { border-color: color-mix(in srgb, var(--ui-accent) 60%, transparent); background: var(--ui-accent-softer); }
.fm-distro { display: flex; flex-direction: column; gap: 5px; }
.fm-drow { display: flex; align-items: center; gap: 7px; padding: 6px 8px; border-radius: 10px; border: 1px solid var(--ui-hairline); background: color-mix(in srgb, var(--ui-text) 2%, transparent); }
.fm-drow i { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
.fm-drow .t { flex: 1; min-width: 0; font-size: 10.5px; }
.fm-dstat { font-size: 8.5px; font-weight: 800; padding: 1px 7px; border-radius: 99px; background: color-mix(in srgb, var(--ui-text) 8%, transparent); color: var(--ui-text-3); }
.fm-dstat.on { background: color-mix(in srgb, var(--ui-success) 18%, transparent); color: var(--ui-success); }
.fm-sharebox { padding: 8px; border-radius: 9px; border: 1px dashed color-mix(in srgb, var(--ui-accent) 50%, transparent);
  background: var(--ui-accent-softer); color: var(--ui-accent); font-size: 10px; word-break: break-all; }

/* dialogs */
.fm-ov { position: fixed; inset: 0; z-index: 10000; display: flex; align-items: center; justify-content: center;
  background: color-mix(in srgb, var(--ui-bg-deep) 60%, transparent); backdrop-filter: blur(6px); }
.fm-modal { width: 330px; max-width: 92vw; padding: 18px; border-radius: 18px; background: var(--ui-glass-2);
  border: 1px solid var(--ui-border-strong); box-shadow: var(--ui-shadow-3); backdrop-filter: blur(var(--ui-blur-strong)) saturate(var(--ui-saturate)); }
.fm-modal h3 { margin: 0 0 10px; font-size: 13px; font-weight: 800; overflow: hidden; text-overflow: ellipsis; }
.fm-modal p { margin: 0 0 10px; }
.fm-modal input { width: 100%; box-sizing: border-box; padding: 8px 10px; border-radius: 10px; margin-bottom: 12px;
  background: color-mix(in srgb, var(--ui-text) 5%, transparent); border: 1px solid var(--ui-border); color: var(--ui-text); outline: none; }
.fm-modal input:focus { border-color: color-mix(in srgb, var(--ui-accent) 60%, transparent); }
.fm-local-editor-ov { padding: 16px; box-sizing: border-box; }
.fm-local-editor { display: flex; flex-direction: column; gap: 12px; width: min(760px, 96vw); max-width: 96vw; max-height: min(88dvh, 900px); box-sizing: border-box; }
.fm-local-editor__head { display: flex; align-items: flex-start; gap: 12px; min-width: 0; }
.fm-local-editor__title { min-width: 0; flex: 1; }
.fm-local-editor__eyebrow { display: block; margin-bottom: 5px; color: var(--ui-accent); font-size: 9px; font-weight: 800; letter-spacing: .1em; }
.fm-local-editor__head h3 { margin: 0 0 4px; font-size: 15px; }
.fm-local-editor__path { display: block; overflow: hidden; color: var(--ui-text-3); font-family: var(--ui-font-mono, monospace); font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
.fm-local-editor__text { width: 100%; min-height: min(58dvh, 620px); flex: 1; box-sizing: border-box; resize: vertical; padding: 14px; border: 1px solid var(--ui-border); border-radius: 14px; outline: none; background: color-mix(in srgb, var(--ui-bg-deep) 72%, transparent); color: var(--ui-text); font: 13px/1.55 var(--ui-font-mono, ui-monospace, monospace); tab-size: 2; }
.fm-local-editor__text:focus { border-color: color-mix(in srgb, var(--ui-accent) 58%, transparent); }
.fm-local-editor__foot { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.fm-local-editor__status { min-width: 0; color: var(--ui-text-2); font-size: 11px; overflow-wrap: anywhere; }
.fm-local-editor__status.error { color: var(--ui-danger); }
.fm-mrow { display: flex; justify-content: flex-end; gap: 8px; }
.truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.hide-sm { }
@media (max-width: 1100px) { .fm-main { grid-template-columns: 180px 1fr 260px; } .fm-main.no-side { grid-template-columns: 0 1fr 260px; } }
@media (max-width: 900px) {
  .fm-main, .fm-main.no-side, .fm-main.no-insp { display: flex; grid-template-columns: 1fr; min-width: 0; }
  .fm-center { flex: 1; width: 100%; }
  .fm-side, .fm-insp { display: none; }
  .fm-main.fm-mobile-side-open .fm-side {
    position: absolute; inset: 0 auto 0 0; z-index: 51; display: flex; flex-direction: column;
    width: min(86vw, 340px); box-sizing: border-box; overflow-y: auto; padding: 12px;
    border: 1px solid var(--ui-border); border-radius: 0 20px 20px 0;
    background: var(--ui-glass-2); box-shadow: var(--ui-shadow-3);
  }
  .fm-main.fm-mobile-insp-open .fm-insp {
    position: absolute; inset: auto 0 0; z-index: 51; display: flex; flex-direction: column;
    height: min(82dvh, 720px); max-height: 100%;
    box-sizing: border-box; overflow: hidden; border: 1px solid var(--ui-border);
    border-bottom: 0; border-radius: 24px 24px 0 0;
    background: var(--ui-glass-2); box-shadow: var(--ui-shadow-3);
  }
  .fm-mobile-backdrop {
    position: absolute; inset: 0; z-index: 50; display: block; width: 100%; height: 100%;
    padding: 0; border: 0; border-radius: 0; background: rgba(8, 12, 20, .42);
    backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px);
  }
  .fm-mobile-panel-head {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    flex: 0 0 auto; min-height: 48px; padding: 4px 8px 10px;
    border-bottom: 1px solid var(--ui-hairline); color: var(--ui-text); font-size: 14px;
  }
  .fm-mobile-panel-head .fm-ibtn { width: 40px; height: 40px; }
  .hide-sm { display: none; }
  .fm-top { flex-wrap: wrap; }
  .fm-crumbs { order: 5; flex-basis: 100%; }
}
/* Phone layout: compact navigation, scrollable tool rows, touch-first file cards. */
@media (max-width: 560px) {
  .fm-local-editor-ov { align-items: flex-end; padding: 0; }
  .fm-local-editor { width: 100%; max-width: 100%; max-height: 94dvh; padding: 18px 14px max(14px, env(safe-area-inset-bottom)); border-radius: 24px 24px 0 0; }
  .fm-local-editor__text { min-height: 48dvh; max-height: 58dvh; resize: none; padding: 13px; font-size: 16px; }
  .fm-local-editor__foot { align-items: stretch; flex-direction: column; }
  .fm-local-editor__foot .fm-mrow { display: grid; grid-template-columns: 1fr 1fr; }
  .fm-local-editor__foot .fm-pill { min-height: 44px; justify-content: center; }
  .fm-top {
    display: grid; grid-template-columns: 44px minmax(0, 1fr) 44px;
    grid-template-areas: 'nav crumbs tools' 'search search search';
    gap: 6px; padding: 7px 8px; position: relative; z-index: 20;
  }
  .fm-nav { grid-area: nav; min-width: 0; overflow-x: auto; }
  .fm-nav .fm-ibtn { width: 44px; min-width: 44px; height: 44px; }
  .fm-nav .fm-ibtn:not(:first-child) { display: none; }
  .fm-crumbs { grid-area: crumbs; order: 0; flex-basis: auto; min-width: 0; min-height: 44px; padding-inline: 4px; }
  .fm-crumb { gap: 4px; padding-inline: 5px; font-size: 10.5px; min-height: 40px; }
  .fm-search { grid-area: search; width: 100%; max-width: none; min-width: 0; height: 44px; box-sizing: border-box; }
  .fm-search input { font-size: 16px; }
  .fm-topactions { grid-area: tools; min-width: 0; max-width: 100%; justify-content: flex-end; }
  .fm-topactions .fm-mobile-more { width: 44px; min-width: 44px; min-height: 44px; justify-content: center; padding: 0; }
  .fm-mobile-more span { display: none; }
  .fm-sub { gap: 6px; padding: 5px 8px; font-size: 10px; }
  .fm-sub .fm-dot, .fm-sub .hide-sm, .fm-sub .fm-vol, .fm-sub .fm-pchip, .fm-sub .fm-pill { display: none; }
  .fm-sub .fm-stat { font-size: 10px; }
  .fm-pchip { font-size: 9px; }
  .fm-ctool { min-height: 48px; flex-shrink: 0; gap: 7px; overflow-x: auto; padding: 5px 8px; touch-action: pan-x; }
  .fm-ctool .fm-spacer { display: none; }
  .fm-ctool > :nth-child(n + 4) { display: none; }
  .fm-ctool .fm-ibtn { min-width: 44px; min-height: 44px; }
  .fm-select, .fm-chipbtn, .fm-ctool .fm-pill { min-height: 38px; flex-shrink: 0; }
  .fm-bulk { min-height: 48px; flex-shrink: 0; flex-wrap: nowrap; overflow-x: auto; }
  .fm-bulk button { flex: 0 0 44px; min-height: 44px; justify-content: center; }
  .fm-bulk-n { flex: 0 0 auto; white-space: nowrap; }
  .fm-diskscope { flex-wrap: wrap; }
  .fm-diskscope-t { max-width: calc(100% - 30px); }
  .fm-grid { padding: 10px; gap: 8px; grid-template-columns: repeat(auto-fill, minmax(112px, 1fr)); }
  .fm-grid.compact { grid-template-columns: repeat(auto-fill, minmax(104px, 1fr)); }
  .fm-grid.masonry { grid-template-columns: repeat(auto-fill, minmax(112px, 1fr)); }
  .fm-card { min-height: 132px; padding: 20px 7px 10px; border-radius: 16px; }
  .fm-thumb { height: calc(58px * var(--fm-zoom, 1)); }
  .fm-cname { font-size: 12px; }
  .fm-cmeta { font-size: 10.5px; }
  .fm-check { top: 6px; left: 6px; width: 32px; height: 32px; border-radius: 50%; background: color-mix(in srgb, var(--ui-surface) 84%, transparent); }
  .fm-lhead { display: none; }
  .fm-lrow, .fm-lrow.compact {
    grid-template-columns: 36px minmax(0, 1fr) auto; gap: 7px; height: auto; min-height: 62px;
    box-sizing: border-box; padding: 8px 10px;
  }
  .fm-lrow .c0 { grid-column: 1; grid-row: 1; }
  .fm-lrow .c1 { grid-column: 2; grid-row: 1; font-size: 12px; }
  .fm-lrow .c2 { grid-column: 3; grid-row: 1; max-width: 72px; font-size: 10px; }
  .fm-lrow .c3, .fm-lrow .c4, .fm-lrow .c5, .fm-lrow .c6 { display: none; }
  .fm-ficon { width: 30px; height: 30px; }
  .fm-status { min-height: 40px; gap: 7px; overflow-x: auto; padding: 6px 8px; }
  .fm-status > :first-child { max-width: 42vw; }
  .fm-term { position: absolute; inset: auto 0 0; z-index: 30; height: min(48dvh, 420px); max-height: min(48dvh, calc(100% - 180px)); border-radius: 18px 18px 0 0; box-shadow: var(--ui-shadow-3); }
  .fm-term-h button { min-width: 44px; min-height: 44px; }
  .fm-term-in { min-height: 52px; }
  .fm-term-in input { min-height: 44px; font-size: 16px; }
  .fm-ihead { padding: 10px 12px; }
  .fm-tabs { overflow-x: auto; flex-shrink: 0; padding-inline: 10px; }
  .fm-tabs button { flex: 0 0 auto; min-width: 66px; min-height: 44px; font-size: 10px; }
  .fm-ibody { padding: 14px; overscroll-behavior: contain; }
  .fm-isec { gap: 12px; }
  .fm-kv { align-items: flex-start; gap: 12px; font-size: 12px; }
  .fm-kv b { overflow-wrap: anywhere; }
  .fm-btnrow { gap: 8px; }
  .fm-btnrow .fm-pill { min-height: 40px; }
  .fm-empty { padding: 32px 18px; }
}

/* Per-window twin: floating/tiled windows can be narrow on a wide desktop. */
@container file-manager (max-width: 900px) {
  .fm-main, .fm-main.no-side, .fm-main.no-insp { display: flex; grid-template-columns: 1fr; min-width: 0; }
  .fm-center { flex: 1; width: 100%; min-width: 0; }
  .fm-side, .fm-insp { display: none; }
  .fm-main.fm-mobile-side-open .fm-side { position: absolute; inset: 0 auto 0 0; z-index: 51; display: flex; flex-direction: column; width: min(86cqi, 340px); box-sizing: border-box; overflow-y: auto; padding: 12px; border: 1px solid var(--ui-border); border-radius: 0 20px 20px 0; background: var(--ui-glass-2); box-shadow: var(--ui-shadow-3); }
  .fm-main.fm-mobile-insp-open .fm-insp { position: absolute; inset: auto 0 0; z-index: 51; display: flex; flex-direction: column; height: min(82dvh, 720px); max-height: 100%; box-sizing: border-box; overflow: hidden; border: 1px solid var(--ui-border); border-bottom: 0; border-radius: 24px 24px 0 0; background: var(--ui-glass-2); box-shadow: var(--ui-shadow-3); }
  .fm-mobile-backdrop { position: absolute; inset: 0; z-index: 50; display: block; width: 100%; height: 100%; padding: 0; border: 0; border-radius: 0; background: rgba(8, 12, 20, .42); }
  .fm-mobile-panel-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex: 0 0 auto; min-height: 48px; padding: 4px 8px 10px; border-bottom: 1px solid var(--ui-hairline); color: var(--ui-text); font-size: 14px; }
  .fm-mobile-panel-head .fm-ibtn { width: 40px; height: 40px; }
  .hide-sm { display: none; }
  .fm-top { flex-wrap: wrap; }
  .fm-crumbs { order: 5; flex-basis: 100%; }
}

@container file-manager (max-width: 560px) {
  .fm-top { display: grid; grid-template-columns: 44px minmax(0, 1fr) 44px; grid-template-areas: 'nav crumbs tools' 'search search search'; gap: 6px; padding: 7px 8px; position: relative; z-index: 20; }
  .fm-nav { grid-area: nav; min-width: 0; overflow-x: auto; }
  .fm-nav .fm-ibtn { width: 44px; min-width: 44px; height: 44px; }
  .fm-nav .fm-ibtn:not(:first-child) { display: none; }
  .fm-crumbs { grid-area: crumbs; order: 0; flex-basis: auto; min-width: 0; min-height: 44px; padding-inline: 4px; }
  .fm-crumb { gap: 4px; padding-inline: 5px; font-size: 10.5px; min-height: 40px; }
  .fm-search { grid-area: search; width: 100%; max-width: none; min-width: 0; height: 44px; box-sizing: border-box; }
  .fm-search input { font-size: 16px; }
  .fm-topactions { grid-area: tools; min-width: 0; max-width: 100%; justify-content: flex-end; }
  .fm-topactions .fm-mobile-more { width: 44px; min-width: 44px; min-height: 44px; justify-content: center; padding: 0; }
  .fm-mobile-more span { display: none; }
  .fm-sub { gap: 6px; padding: 5px 8px; font-size: 10px; }
  .fm-sub .fm-dot, .fm-sub .hide-sm, .fm-sub .fm-vol, .fm-sub .fm-pchip, .fm-sub .fm-pill { display: none; }
  .fm-sub .fm-stat { font-size: 10px; }
  .fm-ctool { min-height: 48px; flex-shrink: 0; gap: 7px; overflow-x: auto; padding: 5px 8px; touch-action: pan-x; }
  .fm-ctool .fm-spacer { display: none; }
  .fm-ctool > :nth-child(n + 4) { display: none; }
  .fm-ctool .fm-ibtn { min-width: 44px; min-height: 44px; }
  .fm-select, .fm-chipbtn, .fm-ctool .fm-pill { min-height: 38px; flex-shrink: 0; }
  .fm-mobile-tools-menu { position: fixed; inset: 8px; z-index: 80; max-height: none; overflow-y: auto; overscroll-behavior: contain; padding: 10px 10px max(12px, env(safe-area-inset-bottom)); border: 1px solid var(--ui-border-strong); border-radius: 20px; background: var(--ui-surface-2); color: var(--ui-text); box-shadow: var(--ui-shadow-3); }
}

.fm-mobile-tools-menu { display: flex; flex-direction: column; gap: 8px; }
.fm-mobile-tools-head { display: flex; align-items: center; justify-content: space-between; min-height: 44px; padding: 0 4px 6px; border-bottom: 1px solid var(--ui-hairline); }
.fm-mobile-tools-head strong { font-size: 13px; }
.fm-mobile-tools-group { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 6px; }
.fm-mobile-tools-label { grid-column: 1 / -1; padding: 3px 4px; color: var(--ui-text-3); font-size: 10px; font-weight: 750; letter-spacing: .06em; text-transform: uppercase; }
.fm-mobile-tool { display: flex; align-items: center; gap: 8px; min-width: 0; min-height: 44px; padding: 8px 10px; border: 1px solid var(--ui-border); border-radius: 11px; background: color-mix(in srgb, var(--ui-text) 3%, transparent); color: var(--ui-text); font: inherit; font-size: 11px; text-align: left; }
.fm-mobile-tool:disabled { opacity: .4; cursor: default; }
.fm-mobile-tool.on { border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent); background: var(--ui-accent-softer); color: var(--ui-accent); }
.fm-mobile-view-grid { grid-column: 1 / -1; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 6px; }

/* On phones the navigation/search toolbar sits below the file area. Keeping it
   in normal flex flow reserves space instead of covering files or the status. */
@media (max-width: 560px) {
  .fm-top { order: 3; flex: 0 0 auto; box-sizing: border-box; padding-bottom: max(8px, env(safe-area-inset-bottom)); border-top: 1px solid var(--ui-hairline); border-bottom: 0; box-shadow: 0 -6px 18px color-mix(in srgb, var(--ui-bg-deep) 22%, transparent); }
  .fm-sub { order: 1; }
  .fm-main { order: 2; }
  .fm-ctool { display: none; }
  .fm-mobile-tools-menu { position: absolute; inset: auto 8px calc(100% + 8px); z-index: 80; max-height: min(72dvh, 620px); overflow-y: auto; overscroll-behavior: contain; padding: 10px; }
}

@container file-manager (max-width: 560px) {
  .fm-top { order: 3; flex: 0 0 auto; box-sizing: border-box; padding-bottom: max(8px, env(safe-area-inset-bottom)); border-top: 1px solid var(--ui-hairline); border-bottom: 0; box-shadow: 0 -6px 18px color-mix(in srgb, var(--ui-bg-deep) 22%, transparent); }
  .fm-sub { order: 1; }
  .fm-main { order: 2; }
  .fm-ctool { display: none; }
  .fm-mobile-tools-menu { position: absolute; inset: auto 8px calc(100% + 8px); z-index: 80; max-height: min(72dvh, 620px); overflow-y: auto; overscroll-behavior: contain; padding: 10px; }
}
</style>
