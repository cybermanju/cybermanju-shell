// CyberManju OS — App (src/stores/app.ts).
// Role: Pinia app store.
// Exports: useAppStore.
// Integrates: pinia, vue, @/….
// Contracts: signals: AGENT-, engine, auth.
// CyberManju OS — Pinia Store
import { defineStore } from 'pinia'
import { ref, computed, watch } from 'vue'
import { invoke } from '@/composables/useTauri'
import { useNotifications } from '@/composables/useNotifications'
import type {
  FileNode, Account, Collection, FaceGroup, LooseGroup,
  SearchResult, EncryptionStatus, EncryptionKeyInfo,
  CompressionStats, ParseResult, GeoMarker,
  FileContent, SavedContent,
  ProviderPreset, AgentConfig, AgentSession, AgentJob, McpServerConfig,
  AgentMemory, MemoryHit,
  ViewMode, PanelType, SidebarSection,
  SyncConfig, SyncProgress, SyncResult, RemoteFile,
  SyncJob, SyncRunRecord, RestoreOutcome, QuotaUsage,
  ScrubRun, RepairStatus, RepairTask, GcReport, LeaseInfo,
  AuthResult, ModuleInfo, TrashItem, AuditEntry, FileVersion, User,
  DashboardStatus,
  ShellResult, OsPs, OsTop, OsWorkers, OsJob, OsVolumeDf, DiskRow,
  ScheduleRow, ScheduleRun,
  SecretMeta, SecretInput,
  BackupEntry, BackupReport, BackupVerify,
  DeviceInfo, ServerTodo, LanPeer, PresencePeer,
  SyncBackendType,
} from '@/types'
import { MODULE_METADATA, oauthSlugForBackend, describeSyncError, agentErrorHint } from '@/types'
import { isSyncSuccess, isSyncTerminal, firstSyncError, refreshAfterSync as fanOutSyncRefresh, syncSummaryLine } from '@/utils/syncRefresh'
import { parseStarIds, serializeStarIds } from '@/utils/stars'
import { parseSchedule, scheduleNextAfter } from '@/utils/schedule'
import { MANAGED_SCRIPT_PATH } from '@/utils/backup'
import { setAuthToken, getAuthToken, isWebMode, isStaticHost } from '@/composables/useTauri'
import { isInstallNeededError, installReasonFor, requestInstallWindow } from '@/utils/installLinks'
import { createUiRequestDedupe } from '@/utils/uiRequests'
import { normalizeConflicts, type OpEntry } from '@/utils/oplog'
import type { RelayMessage } from '@/utils/p2pPresence'
import { useWindowManager } from '@/composables/useWindowManager'

export const useAppStore = defineStore('cybermanju', () => {
  // ── Navigation State ──────────────────────────────────────
  const currentPath = ref('/')
  const currentPanel = ref<PanelType>('landing')
  const viewMode = ref<ViewMode>('grid')
  const selectedFileId = ref<string | null>(null)
  const sidebarSection = ref<SidebarSection>('tree')
  const sidebarCollapsed = ref(false)

  // ── Data State ────────────────────────────────────────────
  const files = ref<FileNode[]>([])
  const accounts = ref<Account[]>([])
  const activeAccountId = ref<string | null>(null)
  const collections = ref<Collection[]>([])
  const faceGroups = ref<FaceGroup[]>([])
  const looseGroups = ref<LooseGroup[]>([])
  const searchResults = ref<SearchResult[]>([])
  const searchPage = ref(0)
  const searchTotalResults = ref(0)
  const searchPageSize = 20
  const geoMarkers = ref<GeoMarker[]>([])

  // ── Encryption State ──────────────────────────────────────
  const encryptionStatus = ref<EncryptionStatus>({ isEncrypted: false })
  const encryptionKeys = ref<EncryptionKeyInfo[]>([])

  // ── Compression State ─────────────────────────────────────
  const compressionStats = ref<CompressionStats | null>(null)

  // ── Trash State ────────────────────────────────────────────
  const trashItems = ref<TrashItem[]>([])
  const showTrashPanel = ref(false)

  // ── Audit State ────────────────────────────────────────────
  const auditLog = ref<AuditEntry[]>([])

  // ── Versions State ─────────────────────────────────────────
  const fileVersions = ref<FileVersion[]>([])

  // ── Dashboard State ─────────────────────────────────────────
  const dashboardStatus = ref<DashboardStatus>({
    running: false,
    port: 3456,
    url: 'http://localhost:3456',
    activeConnections: 0,
  })

  // ── Sync State ────────────────────────────────────────────
  const syncConfigs = ref<SyncConfig[]>([])
  const syncProgress = ref<SyncProgress | null>(null)
  const syncJobs = ref<SyncJob[]>([])
  const syncRuns = ref<SyncRunRecord[]>([])
  const syncStatus = ref<{ syncEnabled: boolean; status: string; lastSync?: string | null; provider?: string | null } | null>(null)
  /** Last terminal sync-run timestamp — file manager + terminal watch this to auto-update. */
  const lastSyncAt = ref(0)
  /** Summary of the last finished run (toast text + terminal `sys` notice). */
  const lastSyncSummary = ref('')

  // ── Durability State (AGENT-7) ──────────────────────────────
  const repairStatus = ref<RepairStatus | null>(null)
  const scrubRuns = ref<ScrubRun[]>([])
  const leaseInfo = ref<LeaseInfo | null>(null)
  const lastGc = ref<GcReport | null>(null)

  // ── OS layer (AGENT-8): cybsh, tasks, compute, volume ───
  const osPs = ref<OsPs | null>(null)
  const osTop = ref<OsTop | null>(null)
  const osWorkers = ref<OsWorkers | null>(null)
  const osJobs = ref<OsJob[]>([])
  const osDf = ref<OsVolumeDf | null>(null)
  const disks = ref<DiskRow[]>([])
  /** True while a `cybsh` line is in flight — the status bar shows it. */
  const shellBusy = ref(false)

  // ── Scheduler (cron): recurring `.cybsh` triggers ─────────
  const schedules = ref<ScheduleRow[]>([])

  // ── Secrets keystore (Phase 3): sealed password-manager rows ──
  // Metadata only — `valueSealed` never reaches this store; reveal is a
  // one-shot call whose plaintext the panel copies and forgets.
  const secrets = ref<SecretMeta[]>([])

  // ── Code Intelligence State ───────────────────────────────
  const parseResult = ref<ParseResult | null>(null)

  // ── Auth State ────────────────────────────────────────────
  // Two separate systems (do not conflate):
  // - Dashboard sign-in: username + password → JWT (`authenticate_user`),
  //   required for EVERY /api call in Docker/web mode. One session per
  //   browser (token in localStorage); each new device signs in again.
  // - Provider OAuth (Google/GitHub/GitLab): per-provider sync tokens in the
  //   Connections tab, unlimited accounts incl. same-provider repeats.
  const AUTH_USER_KEY = 'cybermanju.authUser'
  const currentUser = ref<AuthResult | null>(null)
  const authToken = ref(getAuthToken())
  const isAuthenticated = computed(() => !!currentUser.value)
  /** True while the Docker/web login gate must be shown instead of data. */
  const needsAuth = ref(false)
  /** True when no dashboard account exists yet (public register is open). */
  const needsSetup = ref(false)
  const authChecked = ref(false)
  const authBusy = ref(false)
  const authError = ref<string | null>(null)

  function readStoredUser(): AuthResult | null {
    try {
      const raw = typeof localStorage !== 'undefined' ? localStorage.getItem(AUTH_USER_KEY) : null
      if (!raw) return null
      const parsed = JSON.parse(raw) as Partial<AuthResult>
      if (typeof parsed?.username === 'string' && typeof parsed?.userId === 'string') {
        return {
          userId: parsed.userId,
          username: parsed.username,
          role: typeof parsed.role === 'string' ? parsed.role : 'user',
          displayName: typeof parsed.displayName === 'string' ? parsed.displayName : undefined,
          token: '',
        }
      }
    } catch {
      // Corrupt cache — treated as signed out.
    }
    return null
  }

  function persistUser(user: AuthResult | null) {
    try {
      if (user) {
        const { token: _t, ...rest } = user
        localStorage.setItem(AUTH_USER_KEY, JSON.stringify(rest))
      } else {
        localStorage.removeItem(AUTH_USER_KEY)
      }
    } catch {
      // Private mode — session still works for this page load.
    }
  }

  /** Persist the JWT (store + localStorage) so REST calls stay authenticated. */
  function setSessionToken(token: string) {
    authToken.value = token
    setAuthToken(token)
  }

  /** True when this page talks to a dashboard that demands a JWT. */
  function requiresServerAuth(): boolean {
    try {
      return isWebMode() && !isStaticHost()
    } catch {
      return false
    }
  }

  /**
   * Probe the server without credentials: are we in first-run setup?
   * Public endpoint, safe on every device.
   */
  async function checkAuthStatus(): Promise<void> {
    if (!requiresServerAuth()) {
      needsAuth.value = false
      authChecked.value = true
      return
    }
    try {
      const status = await invoke<{ registrationOpen?: boolean }>('auth_status')
      needsSetup.value = status?.registrationOpen === true
    } catch {
      // Server unreachable — leave the previous guess; the probe inside
      // initialize() will surface the network error once.
    }
    if (getAuthToken()) {
      try {
        await invoke('dashboard_status')
        needsAuth.value = false
        if (!currentUser.value) {
          const stored = readStoredUser()
          if (stored) currentUser.value = { ...stored, token: getAuthToken() }
        }
      } catch {
        // Token invalid/expired/revoked (or rotated secret) — drop it and
        // show the gate instead of spamming 401s on every fetch.
        setSessionToken('')
        persistUser(null)
        currentUser.value = null
        needsAuth.value = true
      }
    } else {
      if (!currentUser.value) {
        const stored = readStoredUser()
        if (stored && getAuthToken()) currentUser.value = { ...stored, token: getAuthToken() }
      }
      needsAuth.value = !currentUser.value
    }
    authChecked.value = true
  }

  /** Sign in with a dashboard username + password (any device). */
  async function login(username: string, password: string): Promise<boolean> {
    authBusy.value = true
    authError.value = null
    try {
      const res = await invoke<AuthResult>('authenticate_user', { username, password })
      setSessionToken(res.token)
      currentUser.value = res
      persistUser(res)
      needsAuth.value = false
      needsSetup.value = false
      notifySuccess(`Signed in — ${res.username}`)
      await initialize()
      return true
    } catch (e) {
      authError.value = e instanceof Error ? e.message : String(e)
      notifyError('Sign-in failed', e)
      return false
    } finally {
      authBusy.value = false
    }
  }

  /**
   * Create the FIRST dashboard account (bootstrap-only: the server closes
   * public registration once a user exists). Further accounts are created
   * by an admin in Accounts → Users — that path accepts any number of
   * accounts. Provider OAuth accounts are separate and unlimited (including
   * several on the same provider).
   */
  async function register(username: string, password: string, displayName?: string): Promise<boolean> {
    authBusy.value = true
    authError.value = null
    try {
      await invoke('register_user', { username, password, displayName, sessionToken: getAuthToken() })
      // Bootstrap register returns the user, not a token — sign in after.
      return await login(username, password)
    } catch (e) {
      authError.value = e instanceof Error ? e.message : String(e)
      notifyError('Registration failed', e)
      return false
    } finally {
      authBusy.value = false
    }
  }

  // Web mode: a 401 means the dashboard JWT is missing or expired. Offer the
  // dashboard sign-in gate — NOT provider OAuth (those tokens open provider
  // sync, they never authenticate /api calls).
  if (typeof window !== 'undefined') {
    window.addEventListener('cybermanju:unauthorized', () => {
      if (requiresServerAuth()) {
        needsAuth.value = true
        currentUser.value = null
        notifyError(
          'Session expired',
          'Sign in again with your dashboard username + password'
        )
        window.dispatchEvent(new CustomEvent('cybermanju:open-login'))
      }
    })
  }

  // ── User Management State ──────────────────────────────────
  const users = ref<User[]>([])

  // ── Transition State ──────────────────────────────────────
  const previousPanel = ref<PanelType | null>(null)
  const isTransitioning = ref(false)

  // ── UI State ──────────────────────────────────────────────
  const searchQuery = ref('')
  const isSearching = ref(false)
  const isLoading = ref(false)
  let loadingOperations = 0
  function beginLoading() {
    loadingOperations += 1
    isLoading.value = true
  }
  function endLoading() {
    loadingOperations = Math.max(0, loadingOperations - 1)
    isLoading.value = loadingOperations > 0
  }
  const lastError = ref<string | null>(null)
  // Wallpaper effects (MatrixRain canvas) are opt-in — the default
  // wallpaper is a static gradient (see DesktopShell).
  const matrixRainEnabled = ref(false)
  const commandPaletteOpen = ref(false)
  const showShortcutsHelp = ref(false)
  const createFolderPromptOpen = ref(false)
  const sortBy = ref<'name' | 'date' | 'size' | 'type'>('name')
  const autoRefreshInterval = ref(0)
  let autoRefreshTimer: ReturnType<typeof setInterval> | null = null
  const selectedFileIds = ref<string[]>([])
  const isMultiSelect = ref(false)

  // ── Module Helpers ─────────────────────────────────────────
  const currentModule = computed<ModuleInfo>(() =>
    MODULE_METADATA[currentPanel.value as PanelType] || MODULE_METADATA.files
  )

  // ── Computed ──────────────────────────────────────────────
  const selectedFile = computed(() =>
    files.value.find(f => f.id === selectedFileId.value) || null
  )

  const activeAccount = computed(() =>
    accounts.value.find(a => a.id === activeAccountId.value) || accounts.value[0] || null
  )

  const encryptedFiles = computed(() =>
    files.value.filter(f => f.encrypted)
  )

  const compressedFiles = computed(() =>
    files.value.filter(f => f.compressionLayers && f.compressionLayers.length > 0 && !f.compressionLayers.includes('none'))
  )

  const starredFiles = computed(() =>
    files.value.filter(f => f.isStarred)
  )

  const folders = computed(() =>
    files.value.filter(f => f.fileType === 'folder')
  )

  const currentFolderFiles = computed(() =>
    files.value.filter(f => f.parentId === selectedFileId.value || (selectedFileId.value === null && !f.parentId))
  )

  // ── Favorites (persisted stars) ─────────────────────────
  // The Rust `FileNode` schema has no star column, so stars live beside
  // the file table: localStorage everywhere (instant, offline) mirrored
  // into the worker kv (`stars` key) on static hosts so they ride inside
  // the `.cybermanju` container alongside third-party provider data.
  const STAR_LS_KEY = 'cybermanju.stars.v1'
  const STAR_KV_KEY = 'stars'
  const starredIds = ref<Set<string>>(new Set())

  function applyStars() {
    const ids = starredIds.value
    for (const f of files.value) {
      f.isStarred = ids.has(f.id)
    }
  }

  // Provider fallback row synthesis lives in `utils/providerBrowse`
  // (pure + unit-tested); this store only orchestrates mounts + canal.

  function readStarCache(): string[] {
    try {
      const raw = localStorage.getItem(STAR_LS_KEY)
      if (!raw) return []
      return parseStarIds(raw)
    } catch {
      return []
    }
  }

  async function loadStars() {
    const cached = readStarCache()
    // Static host: the container kv wins when present (shared/portable).
    if (isStaticHost()) {
      try {
        const { wasmDbDispatch } = await import('@/composables/useWasmBackend')
        const row = (await wasmDbDispatch('kv.get', { key: STAR_KV_KEY }).catch(() => null)) as {
          value?: unknown
        } | null
        const ids = parseStarIds(row?.value)
        if (ids.length > 0) {
          starredIds.value = new Set(ids)
          try { localStorage.setItem(STAR_LS_KEY, serializeStarIds(ids)) } catch { /* private mode */ }
          applyStars()
          return
        }
      } catch {
        // Worker unavailable — fall through to the localStorage cache.
      }
    }
    if (cached.length > 0) {
      starredIds.value = new Set(cached)
      applyStars()
    }
  }

  async function persistStars() {
    const raw = serializeStarIds(starredIds.value)
    try { localStorage.setItem(STAR_LS_KEY, raw) } catch { /* private mode */ }
    if (isStaticHost()) {
      try {
        const { wasmDbDispatch } = await import('@/composables/useWasmBackend')
        await wasmDbDispatch('kv.set', { key: STAR_KV_KEY, value: raw }).catch(() => null)
      } catch {
        // Best-effort mirror — localStorage already holds the truth.
      }
    }
  }

  const { notify } = useNotifications()
  /** Count of suppressed `[WASM Mode]` parity gaps — surfaced in Settings/logs. */
  const wasmGapCount = ref(0)

  function notifyError(msg: string, error: unknown) {
    const detail = error instanceof Error ? error.message : String(error)
    // On the static WASM pack the absence of the dashboard is expected —
    // don't spam a toast per failed fetch on startup, but log + count so
    // parity gaps stay visible in the console.
    if (detail.startsWith('[WASM Mode]')) {
      wasmGapCount.value += 1
      console.error(`[WASM parity gap #${wasmGapCount.value}] ${msg}: ${detail}`)
      lastError.value = null
      return
    }
    // Centralized AGENT-1 hint: sync wording first, agent wording when the
    // prefix is agent-specific (invalid:/context:/limit:) or sync has no hint.
    let hint = describeSyncError(detail).hint
    if (hint === 'See logs for detail.') {
      const agent = agentErrorHint(detail)
      if (!agent.hint.startsWith('See the transcript')) hint = agent.hint
    }
    lastError.value = hint !== 'See logs for detail.' ? `${msg}: ${detail} — ${hint}` : `${msg}: ${detail}`
    notify('error', lastError.value)
    console.error(lastError.value)
    // Central Install-window net: every toast that means "this needs a
    // native build" opens the per-platform installer — present call sites
    // and future ones alike, so no unsupported state can strand the user.
    // The predicate is deliberately narrow (picker / desktop-app /
    // dashboard wordings only); provider-shaped `unsupported:` (bad repo
    // name, bad path, unknown layer) stays a toast. Headless-safe:
    // `requestInstallWindow` no-ops where no window exists. `wm.open`
    // re-steers the live Install window, so bursts refocus instead of
    // stacking.
    if (isInstallNeededError(`${msg}: ${detail}`)) {
      requestInstallWindow(installReasonFor(`${msg}: ${detail}`))
    }
  }

  function notifySuccess(msg: string) {
    notify('success', msg)
  }

  function clearError() {
    lastError.value = null
  }

  // ── Actions: Init ─────────────────────────────────────────
  async function initialize() {
    // Docker/web transport without a JWT: every /api call would 401, so show
    // the login gate instead of firing the whole fetch fan-out (the console
    // spam in the bug report). Each device signs in once; the token persists
    // per browser after that.
    if (requiresServerAuth() && !getAuthToken()) {
      await checkAuthStatus()
      needsAuth.value = true
      return
    }
    beginLoading()
    clearError()
    try {
      await loadStars()
      await Promise.allSettled([
        fetchFiles(),
        fetchAccounts(),
        fetchCollections(),
        fetchFaceGroups(),
        fetchLooseGroups(),
        fetchEncryptionStatus(),
        listKeys(),
        fetchSyncConfigs(),
        fetchSchedules(),
        fetchSecrets(),
      ])
      startAutoRefresh()
      startScheduleTick()
    } finally {
      endLoading()
    }
  }

  function startAutoRefresh() {
    if (autoRefreshTimer) clearInterval(autoRefreshTimer)
    if (autoRefreshInterval.value > 0) {
      autoRefreshTimer = setInterval(async () => {
        // Never poll behind the login gate — each tick would 401.
        if (needsAuth.value) return
        // Same for a hidden window: no reader, no reason to run seven
        // backend fetches; the first visible tick refetches everything.
        if (typeof document !== 'undefined' && document.hidden) return
        await Promise.allSettled([
          fetchFiles(),
          fetchAccounts(),
          fetchCollections(),
          fetchFaceGroups(),
          fetchEncryptionStatus(),
          fetchSyncConfigs(),
          fetchTrashItems(),
        ])
      }, autoRefreshInterval.value * 1000)
    }
  }

  watch(autoRefreshInterval, () => startAutoRefresh())

  // ── Actions: Selection ────────────────────────────────────
  function selectFile(fileId: string | null) {
    selectedFileId.value = fileId
  }

  async function toggleStar(fileId: string) {
    const file = files.value.find(f => f.id === fileId)
    const next = !(file?.isStarred ?? starredIds.value.has(fileId))
    if (next) starredIds.value.add(fileId)
    else starredIds.value.delete(fileId)
    if (file) file.isStarred = next
    // Persist beside the DB (localStorage + container kv mirror); the
    // file-table schema itself carries no star column on any backend.
    await persistStars()
  }

  // ── Actions: Files ────────────────────────────────────────
  async function fetchFiles(parentPath?: string) {
    beginLoading()
    clearError()
    try {
      const path = parentPath || currentPath.value
      // Device files (`/host…`) live on the host filesystem, not in the
      // vault: list them through cybsh `-os` verbs over the os_exec bridge
      // (Tauri IPC/REST on desktop, IPC on Android) or — on WASM/static
      // builds — through the picked `$HOME` folder (File System Access API,
      // same `/host` namespace and `host:<abs>` ids).
      if (String(path || '') === '/host' || String(path || '').replace(/\\/g, '/').startsWith('/host/')) {
        const { isTauri } = await import('@/composables/useTauri')
        if (!isTauri()) {
          try {
            const { hostWasmSupported } = await import('@/utils/hostWasm')
            if (hostWasmSupported()) {
              const { listHostDir } = await import('@/utils/hostBrowse')
              files.value = await listHostDir(String(path))
              applyStars()
              return
            }
          } catch (e) {
            files.value = []
            notifyError('Device files need a home folder', e)
            return
          }
          // No folder picker in this browser: the central notifyError net
          // opens the Install window (native app per platform, App × CLI).
          files.value = []
          notifyError('Device files need the app', 'this browser has no folder picker — install options are opening')
          return
        }
        const { listHostDir } = await import('@/utils/hostBrowse')
        files.value = await listHostDir(String(path))
        applyStars()
        return
      }
      // CONTROL 5.7 — read-only provider browse: `/providers/<id>/…` never
      // touches `list_files`; mounts are lower layers served by the canal.
      if (String(path || '').replace(/\\/g, '/').startsWith('/providers')) {
        const canal = await import('@/composables/useProviderCanal')
        const parsed = canal.parseProviderPath(String(path))
        if (!parsed) {
          // Root shows EVERY mount plus every enabled-but-unmounted config,
          // so all Google Drive / GitHub / GitLab repos are visible even
          // before their VFS mount row exists (clicking mounts on demand).
          let mounts: Awaited<ReturnType<typeof canal.listVfsMounts>> = []
          try {
            mounts = await canal.listVfsMounts()
          } catch {
            mounts = []
          }
          const now = new Date().toISOString()
          const rows = mounts.map(m => ({
            id: `providers/${m.id}`,
            name: m.name,
            fileType: 'folder',
            parentId: '/providers',
            path: `/providers/${m.id}`,
            sizeBytes: 0,
            encrypted: false,
            compressionLayers: [],
            createdAt: m.createdAt || now,
            modifiedAt: m.updatedAt || now,
          }) as FileNode)
          try {
            const { pendingMountId } = await import('@/utils/providerBrowse')
            const mounted = new Set(mounts.map(m => m.configId))
            for (const c of syncConfigs.value.filter(c => c.enabled && !mounted.has(c.id))) {
              const label = `${c.backendType} · ${(c.repoName || c.basePath || c.name || c.id).slice(0, 32)} (mount…)`
              const pend = pendingMountId(c.id)
              rows.push({
                id: `providers/${pend}`,
                name: label,
                fileType: 'folder',
                parentId: '/providers',
                path: `/providers/${pend}`,
                sizeBytes: 0,
                encrypted: false,
                compressionLayers: [],
                createdAt: now,
                modifiedAt: now,
              } as FileNode)
            }
          } catch {
            // Config list unavailable — mounts alone still render.
          }
          files.value = rows
        } else if (parsed.mountId.startsWith('pending-')) {
          // Placeholder row: no bytes to list until the mount exists.
          files.value = []
        } else {
          // Canal first (folders + files, one level); REST fallback when
          // the wasm bundle is unavailable (desktop without wasm build).
          // The fallback synthesizes folder rows from recursive paths so
          // GitHub/GitLab/Drive folders stay navigable on every transport.
          try {
            const entries = await canal.listVfsDir(parsed.mountId, parsed.remotePath)
            files.value = entries.map(e => ({
              id: `providers/${parsed.mountId}/${e.locator || e.path}`,
              name: e.name,
              fileType: e.isDir ? 'folder' : 'file',
              parentId: String(path),
              path: canal.providerPathFor(parsed.mountId, e.path),
              sizeBytes: Number(e.sizeBytes ?? 0),
              encrypted: false,
              compressionLayers: [],
              createdAt: String(e.modifiedAt || new Date().toISOString()),
              modifiedAt: String(e.modifiedAt || new Date().toISOString()),
            }) as FileNode)
          } catch (canalErr) {
            const mounts = await canal.listVfsMounts().catch(() => [])
            const mount = mounts.find(m => m.id === parsed.mountId)
            const cfg = mount && syncConfigs.value.find(c => c.id === mount.configId)
            if (!cfg) throw canalErr
            const remote = await invoke<RemoteFile[]>('list_remote_files', { config: await withLiveToken(cfg), prefix: parsed.remotePath })
            const { remoteFilesToProviderNodes } = await import('@/utils/providerBrowse')
            files.value = remoteFilesToProviderNodes(remote, parsed.mountId, String(path), canal.providerPathFor)
          }
        }
        applyStars()
        return
      }
      const result = await invoke<FileNode[]>('list_files', { parentPath: path })
      files.value = result
      // Vault root (`/`) is the merged home of every top-level entry across
      // all disks. Folders made by older builds were indexed under `''`
      // instead of `/`, so no root listing would ever show them — sweep the
      // legacy key and merge those orphans back into the home view.
      // Transport-uniform: Tauri answers the `''` key with exactly those
      // rows; REST/WASM answer with the full table, so keep only true
      // orphans (`parentId` unset). Trash can never leak in — trashed rows
      // leave the files table entirely.
      if ((path || '/') === '/') {
        try {
          const legacy = await invoke<FileNode[]>('list_files', { parentPath: '' })
          const known = new Set(files.value.map(f => f.id))
          for (const o of legacy) {
            if (!o || known.has(o.id)) continue
            if (!o.parentId) {
              files.value.push(o)
              known.add(o.id)
            }
          }
        } catch {
          // Best-effort heal — the primary root listing already loaded.
        }
      }
      // Re-apply persisted stars — no backend ships a star column.
      applyStars()
    } catch (e) {
      notifyError('Failed to fetch files', e)
    } finally {
      endLoading()
    }
  }

  async function getFile(fileId: string) {
    try {
      return await invoke<FileNode>('get_file', { fileId })
    } catch (e) {
      notifyError('Failed to get file', e)
      return null
    }
  }

  // The parent index is keyed by VAULT PATH (the same key `fetchFiles`
  // lists by: `/`, `/photos`, …) — never by node id. Callers historically
  // passed a selected folder's id or `''`, which indexed the new folder
  // under a key no listing ever queries, so it silently never showed.
  // Normalize here so every entry point lands where the user is looking.
  async function createFolder(name: string, parentId: string) {
    try {
      const clean = (name || '').trim()
      if (!clean) {
        notifyError('Folder name is required', 'type a name first')
        return
      }
      let key = (parentId || '').trim()
      if (!key.startsWith('/')) key = currentPath.value || '/'
      key = key.replace(/\/+$/, '') || '/'
      // `/providers/…` is a read-only canal browse of remote mounts —
      // writing a vault node there would vanish from that view.
      if (key.startsWith('/providers')) {
        notifyError('Providers are read-only', 'create the folder in the vault instead')
        return
      }
      await invoke('create_folder', { name: clean, parentId: key })
      await fetchFiles()
      notifySuccess(`Folder '${clean}' created`)
    } catch (e) {
      notifyError('Failed to create folder', e)
    }
  }

  async function deleteFile(fileId: string) {
    try {
      await invoke('delete_file', { fileId })
      files.value = files.value.filter(f => f.id !== fileId)
      if (selectedFileId.value === fileId) selectedFileId.value = null
      selectedFileIds.value = selectedFileIds.value.filter(id => id !== fileId)
      notifySuccess(`File deleted`)
    } catch (e) {
      notifyError('Failed to delete file', e)
    }
  }

  async function renameFile(fileId: string, newName: string) {
    try {
      await invoke('rename_file', { fileId, newName })
      await fetchFiles()
      notifySuccess('File renamed')
    } catch (e) {
      notifyError('Failed to rename file', e)
    }
  }

  async function duplicateFileContext(fileId: string) {
    try {
      await invoke('duplicate_file_context', { fileId })
      await fetchFiles()
      notifySuccess('File context duplicated')
    } catch (e) {
      notifyError('Failed to duplicate file', e)
    }
  }

  /**
   * Replace a file's user tags (any image/doc/folder/file). Tags feed the
   * Tantivy index (filename+content+tags BM25) and the scene matcher on
   * every transport — this is how users teach search what things are.
   */
  async function setFileTags(fileId: string, tags: string[]) {
    try {
      await invoke('set_file_tags', { fileId, tags })
      await fetchFiles()
      notifySuccess('Tags saved')
      return true
    } catch (e) {
      notifyError('Failed to save tags', e)
      return false
    }
  }

  // ── Actions: Search ───────────────────────────────────────
  async function searchFiles(query: string, page?: number) {
    if (!query.trim()) { searchResults.value = []; searchTotalResults.value = 0; return }
    isSearching.value = true
    clearError()
    try {
      const pg = page ?? 0
      const limit = searchPageSize
      const result = await invoke<{ results: SearchResult[]; total: number }>('search_files_paginated', { query, limit, offset: pg * limit })
      if (pg === 0) {
        searchResults.value = result.results
      } else {
        searchResults.value = [...searchResults.value, ...result.results]
      }
      searchTotalResults.value = result.total
      searchPage.value = pg
    } catch {
      try {
        const result = await invoke<SearchResult[]>('search_files', { query, limit: 50 })
        if (page && page > 0) {
          searchResults.value = [...searchResults.value, ...result]
        } else {
          searchResults.value = result
        }
        searchTotalResults.value = result.length
        searchPage.value = page ?? 0
      } catch (e) {
        notifyError('Search failed', e)
      }
    } finally {
      isSearching.value = false
    }
  }

  function loadMoreSearchResults() {
    if (searchQuery.value.trim()) {
      searchFiles(searchQuery.value, searchPage.value + 1)
    }
  }

  // ── Actions: Encryption ───────────────────────────────────
  async function fetchEncryptionStatus() {
    try {
      encryptionStatus.value = await invoke<EncryptionStatus>('get_encryption_status', {
        fileId: selectedFileId.value || undefined,
      })
    } catch (e) {
      notifyError('Failed to get encryption status', e)
    }
  }

  async function generateKeypair(algorithm: string) {
    try {
      await invoke('generate_keypair', { algorithm })
      await listKeys()
      await fetchEncryptionStatus()
    } catch (e) {
      notifyError('Failed to generate keypair', e)
    }
  }

  async function listKeys() {
    try {
      encryptionKeys.value = await invoke<EncryptionKeyInfo[]>('list_keys')
    } catch (e) {
      notifyError('Failed to list keys', e)
    }
  }

  async function encryptFile(fileId: string, algorithm: string) {
    try {
      await invoke('encrypt_file', { fileId, algorithm })
      await fetchFiles()
      await fetchEncryptionStatus()
    } catch (e) {
      notifyError('Encryption failed', e)
    }
  }

  async function decryptFile(fileId: string) {
    try {
      await invoke('decrypt_file', { fileId })
      await fetchFiles()
      await fetchEncryptionStatus()
    } catch (e) {
      notifyError('Decryption failed', e)
    }
  }

  // ── Actions: Compression ──────────────────────────────────
  async function compressFile(fileId: string, layer: string) {
    try {
      const stats = await invoke<CompressionStats>('compress_file', { fileId, layer })
      compressionStats.value = stats
      await fetchFiles()
    } catch (e) {
      notifyError('Compression failed', e)
    }
  }

  async function decompressFile(fileId: string) {
    try {
      const stats = await invoke<CompressionStats>('decompress_file', { fileId })
      compressionStats.value = stats
      await fetchFiles()
    } catch (e) {
      notifyError('Decompression failed', e)
    }
  }

  // ── Actions: Collections ──────────────────────────────────
  async function fetchCollections() {
    try {
      collections.value = await invoke<Collection[]>('list_collections')
    } catch (e) {
      notifyError('Failed to fetch collections', e)
    }
  }

  async function createCollection(name: string, type: string, color: string, description?: string) {
    try {
      await invoke('create_collection', { name, collectionType: type, color, description })
      await fetchCollections()
    } catch (e) {
      notifyError('Failed to create collection', e)
    }
  }

  async function addToCollection(collectionId: string, fileId: string, note?: string) {
    try {
      await invoke('add_to_collection', { collectionId, fileId, note })
      await fetchCollections()
    } catch (e) {
      notifyError('Failed to add to collection', e)
    }
  }

  async function removeFromCollection(collectionId: string, fileId: string) {
    try {
      await invoke('remove_from_collection', { collectionId, fileId })
      await fetchCollections()
    } catch (e) {
      notifyError('Failed to remove from collection', e)
    }
  }

  // ── Actions: Face Groups ──────────────────────────────────
  async function fetchFaceGroups() {
    try {
      faceGroups.value = await invoke<FaceGroup[]>('list_face_groups')
    } catch (e) {
      notifyError('Failed to fetch face groups', e)
    }
  }

  async function detectFaces(fileId: string) {
    try {
      await invoke('detect_faces', { fileId })
      await fetchFaceGroups()
    } catch (e) {
      notifyError('Face detection failed', e)
    }
  }

  async function detectFacesBatch() {
    try {
      const result = await invoke<{ clustersCreated: number; totalFaces: number; noiseFaces: number; avgCohesion: number; strategyUsed: string; detectionEngines?: string[] }>('detect_faces_batch_cmd')
      await fetchFaceGroups()
      return result
    } catch (e) {
      notifyError('Batch face detection failed', e)
      return null
    }
  }

  async function reclusterFaces(strategy?: string) {
    try {
      const result = await invoke<{ clustersCreated: number; totalFaces: number; noiseFaces: number; avgCohesion: number; strategyUsed: string; detectionEngines?: string[] }>('recluster_faces', { strategy })
      await fetchFaceGroups()
      return result
    } catch (e) {
      notifyError('Re-clustering failed', e)
      return null
    }
  }

  async function renameFaceGroup(groupId: string, newName: string) {
    try {
      await invoke('rename_face_group', { groupId, newName })
      await fetchFaceGroups()
    } catch (e) {
      notifyError('Failed to rename face group', e)
    }
  }

  async function mergeFaceGroups(sourceGroupId: string, targetGroupId: string) {
    try {
      await invoke('merge_face_groups', { sourceGroupId, targetGroupId })
      await fetchFaceGroups()
    } catch (e) {
      notifyError('Failed to merge face groups', e)
    }
  }

  async function deleteFaceGroup(groupId: string) {
    try {
      await invoke('delete_face_group', { groupId })
      await fetchFaceGroups()
    } catch (e) {
      notifyError('Failed to delete face group', e)
    }
  }

  async function findSimilarFaces(groupId: string, threshold?: number) {
    try {
      return await invoke<FaceGroup[]>('find_similar_faces', { groupId, threshold })
    } catch (e) {
      notifyError('Failed to find similar faces', e)
      return []
    }
  }

  // ── Actions: Accounts ─────────────────────────────────────
  async function fetchAccounts() {
    try {
      accounts.value = await invoke<Account[]>('list_accounts')
      const active = accounts.value.find(a => a.isActive)
      if (active) activeAccountId.value = active.id
    } catch (e) {
      notifyError('Failed to fetch accounts', e)
    }
  }

  async function createAccount(name: string, type: string, path?: string, color?: string) {
    try {
      await invoke('create_account', { name, accountType: type, path, color })
      await fetchAccounts()
    } catch (e) {
      notifyError('Failed to create account', e)
    }
  }

  async function switchAccount(accountId: string) {
    try {
      await invoke('switch_account', { accountId })
      activeAccountId.value = accountId
      await fetchAccounts()
    } catch (e) {
      notifyError('Failed to switch account', e)
    }
  }

  async function deleteAccount(accountId: string) {
    try {
      await invoke('delete_account', { accountId })
      await fetchAccounts()
      notifySuccess('Account deleted')
    } catch (e) {
      notifyError('Failed to delete account (active accounts cannot be deleted)', e)
    }
  }

  // ── Actions: Map ──────────────────────────────────────────
  async function fetchGeoFiles() {
    try {
      const geoFiles = await invoke<Array<{ id: string; name: string; gpsLat?: number; gpsLon?: number }>>('get_geo_files')
      geoMarkers.value = geoFiles
        .filter(f => f.gpsLat != null && f.gpsLon != null)
        .map(f => ({
          fileId: f.id,
          fileName: f.name,
          lat: f.gpsLat!,
          lng: f.gpsLon!,
        }))
    } catch (e) {
      notifyError('Failed to fetch geo files', e)
    }
  }

  // ── Actions: Tree-sitter ──────────────────────────────────
  async function parseFileCode(filePath: string) {
    try {
      parseResult.value = await invoke<ParseResult>('parse_file', { filePath })
    } catch (e) {
      notifyError('Parse failed', e)
    }
  }

  /**
   * Parse source text on every transport: Tauri runs the real grammars,
   * REST runs the shared heuristic core (same shape, honest `engine`).
   * Returns the result for callers that render from it directly.
   */
  async function parseCodeText(fileName: string, content: string) {
    try {
      const result = await invoke<ParseResult>('parse_text', { fileName, content })
      parseResult.value = result
      return result
    } catch (e) {
      notifyError('Parse failed', e)
      return null
    }
  }

  // ── Actions: Editor file content (transport-aware) ───────────
  /** Managed file bytes (desktop Tauri command, REST on web). */
  async function readManagedContent(fileId: string) {
    try {
      return await invoke<FileContent>('read_file_content', { fileId })
    } catch (e) {
      notifyError('Failed to read file content', e)
      return null
    }
  }

  /** Save managed file bytes (snapshots a version first, server-side). */
  async function saveManagedContent(fileId: string, content: string) {
    try {
      const saved = await invoke<SavedContent>('write_file_content', { fileId, content })
      notifySuccess(`Saved ${saved.name} (${saved.sizeBytes} bytes — version snapshotted)`)
      await fetchFiles()
      return saved
    } catch (e) {
      notifyError('Save failed', e)
      return null
    }
  }

  /** WASM-volume text (static host only): read via the shell. */
  async function readWasmFile(path: string) {
    try {
      const res = await invoke<ShellResult>('os_exec', { line: `cat "${path}"` })
      if (!res.ok) {
        notifyError('Failed to read file', res.output)
        return null
      }
      return res.output
    } catch (e) {
      notifyError('Failed to read file', e)
      return null
    }
  }

  /** WASM-volume save (static host only, 1 MiB cap enforced Rust-side). */
  async function saveWasmFile(path: string, content: string) {
    try {
      const res = await invoke<{ ok: boolean; output: string }>('os_write', { path, content })
      if (res && (res as { ok?: boolean }).ok === false) {
        notifyError('Save failed', (res as { output?: string }).output ?? '')
        return false
      }
      notifySuccess(`Saved ${path}`)
      return true
    } catch (e) {
      notifyError('Save failed', e)
      return false
    }
  }

  /** WASM-volume directory listing (static host only). */
  async function listWasmDir(path: string) {
    try {
      const res = await invoke<{ ok: boolean; output: string } | string[]>('os_ls', { path })
      if (Array.isArray(res)) return res as string[]
      const out = (res as { output?: string }).output ?? ''
      return out.split('\n').map(s => s.trim()).filter(Boolean)
    } catch (e) {
      notifyError('Failed to list directory', e)
      return []
    }
  }

  // ── Actions: Loose Groups ─────────────────────────────────
  async function fetchLooseGroups() {
    try {
      looseGroups.value = await invoke<LooseGroup[]>('list_loose_groups')
    } catch (e) {
      notifyError('Failed to fetch loose groups', e)
    }
  }

  async function createLooseGroup(name: string, color = '#FFFFFF') {
    const clean = name.trim()
    if (!clean) {
      notifyError('Invalid group name', 'Give the loose group a name first')
      return null
    }
    try {
      const group = await invoke<LooseGroup>('create_loose_group', { name: clean, color })
      await fetchLooseGroups()
      notifySuccess(`Loose group "${clean}" created`)
      return group
    } catch (e) {
      notifyError('Failed to create loose group', e)
      return null
    }
  }

  async function addFileToLooseGroup(groupId: string, fileId: string) {
    try {
      const group = await invoke<LooseGroup>('add_to_loose_group', { groupId, fileId })
      await Promise.allSettled([fetchLooseGroups(), fetchFiles()])
      notifySuccess('File added to loose group')
      return group
    } catch (e) {
      notifyError('Failed to add file to loose group', e)
      return null
    }
  }

  // ── Actions: Sync ──────────────────────────────────────
  async function fetchSyncConfigs() {
    try {
      syncConfigs.value = await invoke<SyncConfig[]>('list_sync_configs')
    } catch (e) {
      notifyError('Failed to fetch sync configs', e)
    }
  }

  async function createSyncConfig(config: Omit<SyncConfig, 'id' | 'createdAt' | 'updatedAt'>) {
    try {
      await invoke('create_sync_config', { config })
      await fetchSyncConfigs()
    } catch (e) {
      notifyError('Failed to create sync config', e)
    }
  }

  async function deleteSyncConfig(configId: string) {
    try {
      await invoke('delete_sync_config', { configId })
      await fetchSyncConfigs()
    } catch (e) {
      notifyError('Failed to delete sync config', e)
    }
  }

  /**
   * Upsert a full sync config (create when `id` is empty, overwrite when set —
   * `save_config` on the backend). An absent `token` leaves the stored secret
   * untouched, so toggling `enabled` never wipes credentials. Returns the
   * saved row, or null on failure (toast already shown).
   */
  async function saveSyncConfig(config: SyncConfig): Promise<SyncConfig | null> {
    try {
      const saved = await invoke<SyncConfig>('create_sync_config', { config })
      await fetchSyncConfigs()
      return saved ?? null
    } catch (e) {
      notifyError('Failed to save provider', e)
      return null
    }
  }

  /**
   * Merge the live Supabase OAuth session token into a config row that has
   * none saved. Desktop Rust backends resolve credentials as (credential
   * file → `config.token`) and never see browser-session tokens otherwise —
   * so Drive list/probe/upload fails with `auth: no token` on AppImage while
   * the Pages build (direct Drive fetch with the session token) works.
   * Returns the row unchanged when no session matches.
   */
  async function withLiveToken(config: SyncConfig): Promise<SyncConfig> {
    try {
      if (typeof config.token === 'string' && config.token.trim()) return config
      const backend = String(config.backendType ?? '')
      if (backend !== 'googleDrive' && backend !== 'github' && backend !== 'gitlab') return config
      const { supabaseSession, supabaseSessionProvider, supabaseProviderFor } = await import('@/composables/useSupabase')
      const session = await supabaseSession().catch(() => null)
      const prov = session ? supabaseSessionProvider(session) : null
      if (prov && session?.provider_token && supabaseProviderFor(backend) === prov) {
        return { ...config, token: session.provider_token }
      }
    } catch {
      // Session unreadable — the backend error explains itself.
    }
    return config
  }

  /**
   * Quiet connectivity probe — same `test_sync_connection` call the Sync
   * panel uses, but without toast spam so the Account Manager can poll it
   * while an OAuth browser flow completes.
   */
  async function probeSyncConnection(config: SyncConfig): Promise<{ ok: boolean; detail: string }> {
    try {
      const ok = await invoke<boolean>('test_sync_connection', { config: await withLiveToken(config) })
      return ok
        ? { ok: true, detail: 'provider answered' }
        : { ok: false, detail: 'provider answered false — check token / OAuth, then retry' }
    } catch (e) {
      return { ok: false, detail: e instanceof Error ? e.message : String(e) }
    }
  }

  async function startSync(configId: string, fileIds: string[]) {
    try {
      await invoke('start_sync', { configId, fileIds })
      await pollSyncProgress()
    } catch (e) {
      notifyError('Failed to start sync', e)
    }
  }

  async function pollSyncProgress() {
    try {
      syncProgress.value = await invoke<SyncProgress>('get_sync_progress')
      const status = syncProgress.value?.status
      if (isSyncTerminal(status)) {
        // Terminal state: the provider data landed (or the run died), so
        // every view that renders provider data refreshes now — file
        // manager, volume `df`, sync status/runs — instead of going stale
        // until a manual refresh. `lastSyncAt` is what the terminal panel
        // watches to print its `sys` notice.
        if (isSyncSuccess(status)) {
          await refreshAfterSync(syncSummaryLine(syncProgress.value))
        } else if (status === 'error') {
          notifyError('Sync failed', firstSyncError(syncProgress.value) || 'see the sync panel for details')
        }
        return
      }
      setTimeout(() => pollSyncProgress(), 1000)
    } catch (e) {
      notifyError('Failed to get sync progress', e)
    }
  }

  /**
   * Re-fetch everything a finished provider run can change (file manager
   * listing, volume `df`, sync status/runs) and tell sibling tabs via the
   * `cybermanju-os` bus. Records `lastSyncAt`/`lastSyncSummary` for the
   * terminal's auto-update notice.
   */
  async function refreshAfterSync(summary: string) {
    await fanOutSyncRefresh({
      fetchFiles: () => fetchFiles(),
      fetchOsDf: () => fetchOsDf(),
      fetchSyncStatus: () => fetchSyncStatus(),
      fetchSyncRuns: () => fetchSyncRuns(),
      postBroadcast: (message) => {
        new BroadcastChannel('cybermanju-os').postMessage(message)
      },
    })
    lastSyncSummary.value = summary
    lastSyncAt.value = Date.now()
    if (summary) notifySuccess(summary)
  }

  async function getSyncProgress() {
    try {
      syncProgress.value = await invoke<SyncProgress>('get_sync_progress')
    } catch (e) {
      notifyError('Failed to get sync progress', e)
    }
  }

  async function testSyncConnection(config: SyncConfig) {
    try {
      return await invoke<boolean>('test_sync_connection', { config: await withLiveToken(config) })
    } catch (e) {
      notifyError('Sync connection test failed', e)
      return false
    }
  }

  async function cancelSync() {
    try {
      await invoke('cancel_sync')
      syncProgress.value = null
    } catch (e) {
      notifyError('Failed to cancel sync', e)
    }
  }

  async function listRemoteFiles(config: SyncConfig, prefix: string) {
    try {
      return await invoke<RemoteFile[]>('list_remote_files', { config: await withLiveToken(config), prefix })
    } catch (e) {
      notifyError('Failed to list remote files', e)
      return []
    }
  }

  async function getSyncJob(jobId: string) {
    try {
      const job = await invoke<SyncJob>('get_sync_job', { jobId })
      const i = syncJobs.value.findIndex(j => j.jobId === jobId)
      if (i >= 0) syncJobs.value[i] = job
      else syncJobs.value.push(job)
      return job
    } catch (e) {
      notifyError('Failed to get sync job', e)
      return null
    }
  }

  async function fetchSyncRuns() {
    try {
      syncRuns.value = await invoke<SyncRunRecord[]>('list_sync_runs')
    } catch (e) {
      notifyError('Failed to fetch sync runs', e)
    }
  }

  async function fetchSyncStatus() {
    try {
      syncStatus.value = await invoke<{ syncEnabled: boolean; status: string; lastSync?: string | null; provider?: string | null }>('get_sync_status')
    } catch (e) {
      notifyError('Failed to fetch sync status', e)
    }
  }

  async function restoreSyncFile(configId: string, fileId?: string, remotePath?: string, destPath?: string) {
    try {
      const out = await invoke<RestoreOutcome>('restore_sync_file', { configId, fileId, remotePath, destPath })
      notifySuccess(`Restored ${out.bytes} bytes to ${out.path}${out.verified ? ' (verified)' : ''}`)
      // A restore writes local files from the provider — same auto-update
      // as a finished run so the file manager + terminal show it at once.
      await refreshAfterSync(`restored ${out.path} · views refreshed`)
      return out
    } catch (e) {
      notifyError('Restore failed', e)
      return null
    }
  }

  async function deleteRemoteFile(configId: string, remotePath: string) {
    try {
      await invoke<boolean>('delete_remote_file', { configId, remotePath })
      notifySuccess('Remote file deleted')
      return true
    } catch (e) {
      notifyError('Remote delete failed', e)
      return false
    }
  }

  /** Unified-disk move: relocate one file's single copy A→B (verified, no recompress). */
  async function moveSyncFile(fileId: string, fromConfigId: string, toConfigId: string) {
    try {
      const out = await invoke<{ remotePath: string; bytes: number; noop: boolean }>('move_sync_file', {
        fileId,
        fromConfigId,
        toConfigId,
      })
      if (out?.noop) notifySuccess('Already home — nothing moved')
      else notifySuccess(`Moved to ${toConfigId} (${out?.bytes ?? 0} bytes, verified)`)
      await Promise.allSettled([fetchSyncRuns(), fetchSyncConfigs(), fetchDisks(), fetchOsDf()])
      return out
    } catch (e) {
      notifyError('Move failed', e)
      return null
    }
  }

  async function fetchSyncUsage(configId: string) {
    try {
      return await invoke<QuotaUsage>('get_sync_usage', { configId })
    } catch (e) {
      notifyError('Failed to fetch provider quota', e)
      return null
    }
  }

  async function oauthStart(provider: string, configId: string) {
    try {
      // The backend route only knows google|github|gitlab slugs —
      // Google Drive uses the `google` OAuth client.
      const slug = oauthSlugForBackend(provider as SyncBackendType) ?? provider
      const res = await invoke<{ authorizeUrl: string; state: string }>('oauth_start', { provider: slug, configId })
      if (res?.authorizeUrl && typeof window !== 'undefined') window.open(res.authorizeUrl, '_blank')
      return res
    } catch (e) {
      notifyError('OAuth start failed (fall back to manual token paste)', e)
      return null
    }
  }

  /**
   * Create a new private vault repo on GitHub/GitLab (PAT or stored OAuth
   * token). Desktop/Docker go through the Rust backend; the static Pages
   * build posts directly to the provider API (CORS-OK).
   */
  async function createProviderRepo(args: {
    backendType: string
    configId?: string
    token?: string
    name: string
    private?: boolean
    description?: string
    branch?: string
    basePath?: string
  }) {
    try {
      const repo = await invoke<{
        backend: string
        repoName: string
        fullName: string
        branch: string
        url: string
        projectId?: string | null
      }>('create_provider_repo', {
        backendType: args.backendType,
        configId: args.configId,
        token: args.token,
        name: args.name,
        private: args.private ?? true,
        description: args.description,
        branch: args.branch,
        basePath: args.basePath,
      })
      return repo ?? null
    } catch (e) {
      notifyError('Repo creation failed', e)
      return null
    }
  }

  /** Seed a fresh vault repo with README / manifest / vault bytes. */
  async function seedRepoFiles(
    config: SyncConfig,
    files: Array<{ path: string; contentBase64: string }>,
  ) {
    try {
      return await invoke<string[]>('seed_repo_files', { config: await withLiveToken(config), files })
    } catch (e) {
      notifyError('Repo seed failed', e)
      return null
    }
  }

  /** Write one file to a provider remote path (Drive folders are created server-side). */
  async function uploadRemoteFile(config: SyncConfig, remotePath: string, contentBase64: string) {
    try {
      return await invoke<string>('upload_remote_file', { config: await withLiveToken(config), remotePath, contentBase64 })
    } catch (e) {
      notifyError('Remote upload failed', e)
      return null
    }
  }

  /**
   * Create a disk AND land its `.cybermanju` files on the provider: Drive
   * gets a `cybermanju-disks/<disk>` folder with manifest + vault file,
   * GitHub/GitLab gets a private repo (created when missing) with the same
   * seed. The disk row always survives a remote failure — it comes back as
   * `remoteWarning` instead of a throw.
   */
  async function createDiskWithRemote(
    config: SyncConfig,
    opts: { sizeMb?: number; passphrase?: string; diskName?: string; token?: string; vaultBytes?: Uint8Array | null },
  ) {
    const { provisionDiskWithRemote } = await import('@/utils/diskProvision')
    let token = opts.token?.trim() || (typeof config.token === 'string' ? config.token.trim() : '')
    if (!token) {
      // Callers that only pass the saved row (no draft/session lookup) still
      // get the live OAuth grant — otherwise the Drive seed 401s on desktop
      // while static direct writes succeed.
      token = (await withLiveToken(config)).token?.trim() || ''
    }
    if (!opts.passphrase) {
      notifyError('Failed to create disk', 'auth: a disk passphrase must not be empty — type one on the disk card first')
      throw new Error('auth: a disk passphrase must not be empty — type one on the disk card first')
    }
    const out = await provisionDiskWithRemote({
      config,
      sizeMb: opts.sizeMb ?? 512,
      passphrase: opts.passphrase ?? '',
      diskName: opts.diskName,
      token,
      vaultBytes: opts.vaultBytes,
      useDirectSeed: isStaticHost(),
      createDisk: async (configId, sizeBytes, pass) => {
        try {
          if (!pass) {
            notifyError('Failed to create disk', 'auth: a disk passphrase must not be empty — type one on the disk card first')
            return null
          }
          const row = await invoke<DiskRow>('create_disk', { configId, sizeBytes, passphrase: pass })
          return row ? { id: (row as DiskRow).id, name: (row as DiskRow).name } : null
        } catch (e) {
          notifyError('Failed to create disk', e)
          return null
        }
      },
      attachDisk: async (diskId, pass) => {
        try {
          await invoke('attach_disk', { id: diskId, passphrase: pass })
        } catch {
          // Attach is best-effort here; the disk card offers Attach.
        }
      },
      provisionDeps: {
        // Desktop/Docker Rust backends read the token OUT of the config —
        // the OAuth session token never reaches them otherwise. Merge the
        // resolved token in when the saved row has none (Drive remote seed
        // 401s without this while static-host direct writes succeed).
        uploadFile: async (cfg, remotePath, contentBase64) =>
          invoke<string>('upload_remote_file', {
            config: token && !String((cfg as SyncConfig).token ?? '').trim()
              ? { ...cfg, token }
              : cfg,
            remotePath,
            contentBase64,
          }),
        seedFiles: async (cfg, files) =>
          invoke<string[]>('seed_repo_files', {
            config: token && !String((cfg as SyncConfig).token ?? '').trim()
              ? { ...cfg, token }
              : cfg,
            files,
          }),
        createRepo: async (input) => createProviderRepo({
          backendType: input.backendType,
          token: input.token || undefined,
          name: input.name,
          private: input.privateRepo,
          description: input.description,
          branch: input.branch,
          basePath: input.instanceUrl,
        }),
        saveConfig: async (c) => saveSyncConfig(c),
        driveWriteDirect: async (input) => {
          const { driveWriteDirect } = await import('@/utils/gitProvision')
          return driveWriteDirect(input)
        },
      },
    })
    await Promise.allSettled([fetchDisks(), fetchOsDf(), fetchSyncConfigs()])
    if (out.remote) {
      const where = config.backendType === 'googleDrive'
        ? `Drive folder \`${out.remote.remoteDir}\``
        : `private repo \`${out.remote.config.repoName}\``
      notifySuccess(`Disk created — ${where} holds its .cybermanju files`)
    } else if (out.remoteWarning) {
      notifyError('Disk created, but the remote seed failed', out.remoteWarning)
    }
    return out
  }

  async function logout() {
    try {
      if (getAuthToken()) await invoke('logout_user')
    } catch {
      // Revocation is best-effort — expiry revokes the rest.
    }
    currentUser.value = null
    persistUser(null)
    setSessionToken('')
    if (requiresServerAuth()) needsAuth.value = true
    notifySuccess('Logged out')
  }

  // ── Actions: Durability (AGENT-7) ─────────────────────────
  async function fetchRepairStatus() {
    try {
      repairStatus.value = await invoke<RepairStatus>('repair_status')
    } catch (e) {
      notifyError('Failed to fetch repair status', e)
    }
  }

  async function fetchRepairTasks() {
    try {
      return await invoke<RepairTask[]>('repair_tasks')
    } catch (e) {
      notifyError('Failed to fetch repair tasks', e)
      return []
    }
  }

  async function fetchRepairHealth() {
    try {
      return await invoke<Record<string, unknown>>('repair_health')
    } catch (e) {
      notifyError('Failed to fetch provider health', e)
      return null
    }
  }

  async function runRepair(findings?: unknown[]) {
    try {
      const task = await invoke<RepairTask>('repair_run', { findings: findings ?? [] })
      notifySuccess(`Repair started (${task.taskId})`)
      await fetchRepairStatus()
      return task
    } catch (e) {
      notifyError('Repair failed to start', e)
      return null
    }
  }

  async function runRebuild() {
    try {
      const task = await invoke<RepairTask>('repair_rebuild')
      notifySuccess(`Catalog rebuild started (${task.taskId})`)
      return task
    } catch (e) {
      notifyError('Rebuild failed to start', e)
      return null
    }
  }

  async function runGc(dryRun = true) {
    try {
      const report = await invoke<GcReport>('repair_gc', { dryRun })
      lastGc.value = report
      notifySuccess(dryRun ? `GC dry run: ${report.deleted} would be freed` : `GC freed ${report.bytesFreed} bytes`)
      return report
    } catch (e) {
      notifyError('GC failed', e)
      return null
    }
  }

  async function runScrub() {
    try {
      const task = await invoke<RepairTask>('scrub_run')
      notifySuccess(`Scrub started (${task.taskId})`)
      return task
    } catch (e) {
      notifyError('Scrub failed to start', e)
      return null
    }
  }

  async function fetchScrubRuns() {
    try {
      scrubRuns.value = await invoke<ScrubRun[]>('scrub_runs')
    } catch (e) {
      notifyError('Failed to fetch scrub runs', e)
    }
  }

  async function acquireLease(holder: string, scope = 'volume', ttlSecs = 60) {
    try {
      leaseInfo.value = await invoke<LeaseInfo>('lease_acquire', { holder, scope, ttlSecs })
      return leaseInfo.value
    } catch (e) {
      notifyError('Lease acquire failed', e)
      return null
    }
  }

  async function releaseLease(holder: string, scope = 'volume') {
    try {
      await invoke<boolean>('lease_release', { holder, scope })
      leaseInfo.value = null
      return true
    } catch (e) {
      notifyError('Lease release failed', e)
      return false
    }
  }

  async function fetchLeaseStatus(scope?: string) {
    try {
      leaseInfo.value = await invoke<LeaseInfo>('lease_status', { scope })
    } catch (e) {
      notifyError('Failed to fetch lease status', e)
    }
  }

  // ── Actions: Trash ─────────────────────────────────────────
  async function fetchTrashItems() {
    try {
      trashItems.value = await invoke<TrashItem[]>('list_trash')
    } catch (e) {
      notifyError('Failed to fetch trash', e)
    }
  }

  async function restoreTrashItem(fileId: string) {
    try {
      await invoke('restore_from_trash', { fileId })
      await fetchTrashItems()
      await fetchFiles()
      notifySuccess('File restored from trash')
    } catch (e) {
      notifyError('Failed to restore file', e)
    }
  }

  async function emptyTrash() {
    try {
      const count = await invoke<number>('empty_trash')
      trashItems.value = []
      notifySuccess(`Permanently deleted ${count} items`)
    } catch (e) {
      notifyError('Failed to empty trash', e)
    }
  }

  async function deleteFromTrash(fileId: string) {
    try {
      await invoke('delete_from_trash', { fileId })
      await fetchTrashItems()
      notifySuccess('File permanently deleted')
    } catch (e) {
      notifyError('Failed to delete from trash', e)
    }
  }

  // ── Actions: Audit Log ─────────────────────────────────────
  async function fetchAuditLog(limit?: number, entityType?: string) {
    try {
      auditLog.value = await invoke<AuditEntry[]>('get_audit_log', { limit, entityType })
    } catch (e) {
      notifyError('Failed to fetch audit log', e)
    }
  }

  // ── Actions: File Versions ─────────────────────────────────
  async function fetchFileVersions(fileId: string) {
    try {
      fileVersions.value = await invoke<FileVersion[]>('list_file_versions', { fileId })
    } catch (e) {
      notifyError('Failed to fetch file versions', e)
    }
  }

  async function createVersion(fileId: string) {
    try {
      await invoke('create_file_version', { fileId })
      await fetchFileVersions(fileId)
      notifySuccess('Version snapshot created')
    } catch (e) {
      notifyError('Failed to create version', e)
    }
  }

  async function revertToVersion(fileId: string, versionId: string) {
    try {
      await invoke('revert_file_version', { fileId, versionId })
      await fetchFileVersions(fileId)
      await fetchFiles()
      notifySuccess('File reverted to version')
    } catch (e) {
      notifyError('Failed to revert file version', e)
    }
  }

  async function snapshotAllVersions() {
    try {
      const count = await invoke<number>('snapshot_all_versions')
      notifySuccess(`Snapshotted ${count} files`)
    } catch (e) {
      notifyError('Failed to snapshot all versions', e)
    }
  }

  // ── Actions: Dashboard Status ───────────────────────────────
  // Static hosts (Pages/WASM) have no dashboard behind the page: the invoke
  // layer answers with a local `{running:false}` stub, so this never throws
  // and never poisons `dashboardStatus` with a shapeless payload (a prior
  // Pages crash was `Cannot read properties of undefined (reading
  // 'running')` from templates reading `dashboardStatus.running` after a
  // bad assignment).
  const DASHBOARD_OFFLINE: DashboardStatus = {
    running: false,
    port: 3456,
    url: 'http://localhost:3456',
    activeConnections: 0,
  }
  function isDashboardStatus(v: unknown): v is DashboardStatus {
    const o = v as Partial<DashboardStatus> | null | undefined
    return !!o && typeof o.running === 'boolean'
  }
  async function fetchDashboardStatus() {
    try {
      const res = await invoke<DashboardStatus>('dashboard_status')
      if (!isDashboardStatus(res)) return
      dashboardStatus.value = res
    } catch (e) {
      notifyError('Failed to fetch dashboard status', e)
    }
  }

  async function startDashboard() {
    try {
      const result = await invoke<DashboardStatus>('start_dashboard')
      if (!isDashboardStatus(result)) return
      dashboardStatus.value = result
      notifySuccess('Dashboard started')
    } catch (e) {
      notifyError('Failed to start dashboard', e)
    }
  }

  async function stopDashboard() {
    try {
      await invoke<boolean>('stop_dashboard')
      dashboardStatus.value = { running: false, port: 3456, url: 'http://localhost:3456', activeConnections: 0 }
      notifySuccess('Dashboard stopped')
    } catch (e) {
      notifyError('Failed to stop dashboard', e)
    }
  }

  // ── Actions: User Management ───────────────────────────────
  async function fetchUsers() {
    try {
      users.value = await invoke<User[]>('list_users', { sessionToken: getAuthToken() })
    } catch (e) {
      notifyError('Failed to fetch users', e)
    }
  }

  async function createUser(username: string, password: string, role: string) {
    try {
      await invoke('create_user', { username, password, role, sessionToken: getAuthToken() })
      await fetchUsers()
      notifySuccess(`User '${username}' created`)
    } catch (e) {
      notifyError('Failed to create user', e)
    }
  }

  async function deleteUser(userId: string) {
    try {
      await invoke('delete_user', { userId, sessionToken: getAuthToken() })
      await fetchUsers()
      notifySuccess('User deleted')
    } catch (e) {
      notifyError('Failed to delete user', e)
    }
  }

  async function updateUserRole(userId: string, role: string) {
    try {
      await invoke('update_user_role', { userId, role, sessionToken: getAuthToken() })
      await fetchUsers()
      notifySuccess('User role updated')
    } catch (e) {
      notifyError('Failed to update user role', e)
    }
  }

  // ── Actions: Batch Operations ──────────────────────────────
  async function batchDeleteFiles(fileIds: string[]) {
    try {
      const count = await invoke<number>('batch_delete', { fileIds })
      await fetchFiles()
      // Mirror deleteFile: drop the deleted ids from every selection so the
      // inspector and bulk bar never act on ghosts.
      const gone = new Set(fileIds)
      if (selectedFileId.value && gone.has(selectedFileId.value)) selectedFileId.value = null
      selectedFileIds.value = selectedFileIds.value.filter(id => !gone.has(id))
      if (selectedFileIds.value.length === 0) isMultiSelect.value = false
      notifySuccess(`Batch deleted ${count} files`)
    } catch (e) {
      notifyError('Batch delete failed', e)
    }
  }

  async function batchEncryptFiles(fileIds: string[], algorithm: string) {
    try {
      const count = await invoke<number>('batch_encrypt', { fileIds, algorithm })
      await fetchFiles()
      notifySuccess(`Batch encrypted ${count} files`)
    } catch (e) {
      notifyError('Batch encrypt failed', e)
    }
  }

  async function batchCompressFiles(fileIds: string[], layer: string) {
    try {
      const count = await invoke<number>('batch_compress', { fileIds, layer })
      await fetchFiles()
      notifySuccess(`Batch compressed ${count} files`)
    } catch (e) {
      notifyError('Batch compress failed', e)
    }
  }

  // ── Actions: Share Links ────────────────────────────────────
  const shareLinks = ref<import('@/types').ShareLink[]>([])

  async function generateShareLink(fileId: string, expiresInHours?: number) {
    try {
      const result = await invoke<import('@/types').ShareLink>('generate_share_link', { fileId, expiresInHours })
      await fetchShareLinks()
      notifySuccess('Share link generated')
      return result
    } catch (e) {
      notifyError('Failed to generate share link', e)
      return null
    }
  }

  async function fetchShareLinks() {
    try {
      shareLinks.value = await invoke<import('@/types').ShareLink[]>('list_share_links')
    } catch (e) {
      notifyError('Failed to fetch share links', e)
    }
  }

  // ── Actions: URL Import ─────────────────────────────────────
  async function importFromUrl(url: string, parentPath: string) {
    try {
      const result = await invoke<FileNode>('import_from_url', { url, parentPath })
      await fetchFiles()
      notifySuccess('File imported from URL')
      return result
    } catch (e) {
      notifyError('Failed to import from URL', e)
      return null
    }
  }

  // ── Actions: Parent Index Rebuild ──────────────────────────
  async function rebuildParentIndex() {
    try {
      const count = await invoke<number>('rebuild_parent_index')
      await fetchFiles()
      notifySuccess(`Rebuilt parent index for ${count} files`)
    } catch (e) {
      notifyError('Failed to rebuild parent index', e)
    }
  }

  // ── Actions: OS layer (AGENT-8) ───────────────────────────
  /**
   * Run one `cybsh` line. A command that ran and reported a failure comes
   * back as `{ ok: false }` with the message in `output` — only a transport
   * failure lands in the catch, and that is reported as a shell error too so
   * the terminal never silently drops a line.
   */
  async function execShellLine(line: string): Promise<ShellResult> {
    shellBusy.value = true
    try {
      return await invoke<ShellResult>('os_exec', { line })
    } catch (e) {
      const detail = e instanceof Error ? e.message : String(e)
      return { ok: false, line, output: detail, error: detail, prompt: 'cybsh> ' }
    } finally {
      shellBusy.value = false
    }
  }

  /** Tab completion off the live command table; never throws. */
  async function completeShellLine(prefix: string): Promise<string[]> {
    try {
      return await invoke<string[]>('os_complete', { prefix })
    } catch {
      return []
    }
  }

  /** Polled from the status bar, so failures stay quiet — the task panel
   *  shows `…` until the API is reachable instead of spamming toasts.
   *  Shapeless payloads (e.g. an un-unwrapped wasm envelope) are dropped
   *  with a console warning instead of poisoning `osPs.counts`. */
  async function fetchOsPs() {
    try {
      const res = await invoke<OsPs>('os_ps')
      if (!res || !Array.isArray((res as OsPs).tasks) || !(res as OsPs).counts) {
        osPs.value = null
        return
      }
      osPs.value = res
    } catch {
      osPs.value = null
    }
  }

  async function fetchOsTop() {
    try {
      osTop.value = await invoke<OsTop>('os_top')
    } catch {
      // System stats are optional background telemetry. A static/web session
      // may have no OS endpoint; keep the panel in its n/a state without a toast.
      osTop.value = null
    }
  }

  async function fetchOsWorkers() {
    try {
      osWorkers.value = await invoke<OsWorkers>('os_workers')
    } catch (e) {
      notifyError('Failed to fetch worker pool', e)
    }
  }

  async function fetchOsJobs() {
    try {
      osJobs.value = await invoke<OsJob[]>('os_jobs')
    } catch (e) {
      notifyError('Failed to fetch compute jobs', e)
    }
  }

  async function fetchOsDf() {
    try {
      osDf.value = await invoke<OsVolumeDf>('os_df')
    } catch (e) {
      notifyError('Failed to fetch volume usage', e)
    }
  }

  /** `kill <id>` through the same syscall boundary the terminal uses. */
  async function killOsTask(taskId: number) {
    const result = await execShellLine(`kill ${taskId}`)
    if (!result.ok) {
      notifyError('Failed to kill task', result.output)
      return
    }
    await fetchOsPs()
    notifySuccess(`Task ${taskId} killed`)
  }

  /** `compute run <job> <path>` — starts a fan-out job in the background. */
  async function runComputeJob(job: string, path: string) {
    const result = await execShellLine(`compute run ${job} ${path}`)
    if (!result.ok) notifyError('Failed to start compute job', result.output)
    else notifySuccess(`${job} started on ${osWorkers.value?.total ?? '?'} workers`)
    await fetchOsPs()
    return result
  }

  // ── Actions: Scheduler (cron) ─────────────────────────────
  let cronArmed = false
  let scheduleTickTimer: ReturnType<typeof setInterval> | null = null
  let scheduleVisibilityHooked = false

  async function fetchSchedules() {
    try {
      schedules.value = await invoke<ScheduleRow[]>('cron_list')
      if (!cronArmed) {
        cronArmed = true
        // Boots the daemon thread over IPC/REST. On Pages this is a no-op —
        // the browser tick below *is* the daemon there.
        await invoke('cron_ensure_started').catch(() => true)
      }
    } catch (e) {
      notifyError('Failed to fetch schedules', e)
    }
  }

  /** Stamp `nextFireAt` with the TS twin so every transport stores the same
   * row shape (the server recomputes its own on write anyway; the browser
   * has no server, so this is the only computation Pages ever gets). */
  function scheduleRowWithFire(row: Partial<ScheduleRow>): ScheduleRow {
    const expr = String(row.expr ?? '').trim()
    const spec = parseSchedule(expr) // throws `invalid: …` (AGENT-1 family)
    const next = scheduleNextAfter(spec, new Date())
    return {
      id: row.id ?? '',
      path: String(row.path ?? ''),
      expr,
      enabled: row.enabled ?? true,
      description: row.description ?? null,
      createdAt: row.createdAt ?? new Date().toISOString(),
      lastFiredAt: row.lastFiredAt ?? null,
      nextFireAt: next ? next.toISOString() : null,
      lastRunId: row.lastRunId ?? null,
      runOnBoot: row.runOnBoot ?? false,
    }
  }

  async function cronSave(row: Partial<ScheduleRow>): Promise<ScheduleRow | null> {
    try {
      const saved = await invoke<ScheduleRow>('cron_save', { row: scheduleRowWithFire(row) })
      await fetchSchedules()
      return saved
    } catch (e) {
      notifyError('Failed to save schedule', e)
      return null
    }
  }

  async function cronDelete(id: string) {
    try {
      await invoke('cron_delete', { id })
      await fetchSchedules()
    } catch (e) {
      notifyError('Failed to delete schedule', e)
    }
  }

  /** Run one schedule now. Returns the recorded run (or `null` on error). */
  async function cronRun(id: string): Promise<ScheduleRun | null> {
    try {
      const run = await invoke<ScheduleRun>('cron_run', { id })
      await fetchSchedules()
      return run
    } catch (e) {
      notifyError('Failed to run schedule', e)
      return null
    }
  }

  async function cronSetEnabled(id: string, enabled: boolean) {
    try {
      await invoke('cron_set_enabled', { id, enabled })
      await fetchSchedules()
    } catch (e) {
      notifyError('Failed to update schedule', e)
    }
  }

  async function cronHistory(id: string): Promise<ScheduleRun[]> {
    try {
      return await invoke<ScheduleRun[]>('cron_history', { id })
    } catch (e) {
      notifyError('Failed to fetch schedule history', e)
      return []
    }
  }

  /**
   * Pages has no daemon thread — this *is* the daemon: fire every due row
   * through `cron_run`, which lands on the same execution path as
   * `cron run <id>`. Gated to static hosts (desktop/server own the real
   * thread) and to visible tabs so a backgrounded tab never runs scripts.
   */
  async function tickSchedules() {
    if (!isStaticHost()) return
    if (typeof document !== 'undefined' && document.hidden) return
    await fetchSchedules()
    const due = schedules.value.filter(s => {
      if (!s.enabled) return false
      if (!s.lastFiredAt && s.runOnBoot) return true
      if (!s.nextFireAt) return false
      return new Date(s.nextFireAt).getTime() <= Date.now()
    })
    for (const row of due) await cronRun(row.id)
  }

  /** Arm the browser-side scheduler (static hosts only, 30 s tick + focus). */
  function startScheduleTick() {
    if (!isStaticHost()) return
    if (scheduleTickTimer) clearInterval(scheduleTickTimer)
    scheduleTickTimer = setInterval(() => {
      // Hidden window: nobody can act on a due schedule row, and the
      // visibilitychange hook above refires it the moment we come back.
      if (typeof document !== 'undefined' && document.hidden) return
      void tickSchedules()
    }, 30_000)
    if (!scheduleVisibilityHooked && typeof document !== 'undefined') {
      scheduleVisibilityHooked = true
      document.addEventListener('visibilitychange', () => {
        if (!document.hidden) void tickSchedules()
      })
    }
  }

  // ── Actions: Secrets keystore (Phase 3) ───────────────────
  async function fetchSecrets() {
    try {
      secrets.value = await invoke<SecretMeta[]>('secret_list')
    } catch (e) {
      notifyError('Failed to fetch vault secrets', e)
    }
  }

  /** Create or update a secret. `value` is the plaintext — sealed on
   *  write and never echoed back (the store only sees metadata). */
  async function createSecret(input: SecretInput): Promise<SecretMeta | null> {
    try {
      const saved = await invoke<SecretMeta>('secret_save', { request: input })
      await fetchSecrets()
      return saved
    } catch (e) {
      notifyError('Failed to save secret', e)
      return null
    }
  }

  async function updateSecret(id: string, patch: Partial<SecretInput>): Promise<SecretMeta | null> {
    try {
      const saved = await invoke<SecretMeta>('secret_update', { id, request: patch })
      await fetchSecrets()
      return saved
    } catch (e) {
      notifyError('Failed to update secret', e)
      return null
    }
  }

  async function deleteSecret(id: string) {
    try {
      await invoke('secret_delete', { id })
      await fetchSecrets()
    } catch (e) {
      notifyError('Failed to delete secret', e)
    }
  }

  /** Reveal the plaintext once — the caller copies it and forgets it.
   *  The server audits the reveal; this never logs the value. */
  async function revealSecret(id: string): Promise<string | null> {
    try {
      const raw = await invoke<string | { value?: string }>('secret_reveal', { id })
      // REST answers `{ value }`; Tauri answers the bare string.
      if (typeof raw === 'string') return raw
      return typeof raw?.value === 'string' ? raw.value : null
    } catch (e) {
      notifyError('Failed to reveal secret', e)
      return null
    }
  }

  // ── Actions: Device identity (§2B.1) + LAN peers (§2B.2) ──
  const devices = ref<DeviceInfo[]>([])
  const lanPeers = ref<LanPeer[]>([])

  async function fetchDevices() {
    try {
      devices.value = await invoke<DeviceInfo[]>('list_devices')
    } catch (e) {
      notifyError('Failed to fetch devices', e)
    }
  }

  async function registerDevice(name: string) {
    try {
      const row = await invoke<DeviceInfo>('register_device', { name })
      await fetchDevices()
      notifySuccess(`Device '${row.name}' enrolled`)
      return row
    } catch (e) {
      notifyError('Failed to register device', e)
      return null
    }
  }

  async function registerThisDevice(name: string) {
    try {
      const row = await invoke<DeviceInfo>('register_this_device', { name })
      await fetchDevices()
      notifySuccess(`This device enrolled with a fresh ML-DSA key`)
      return row
    } catch (e) {
      notifyError('Failed to enroll this device', e)
      return null
    }
  }

  async function revokeDevice(id: string) {
    try {
      await invoke('revoke_device', { id })
      await fetchDevices()
      notifySuccess('Device revoked')
    } catch (e) {
      notifyError('Failed to revoke device', e)
    }
  }

  async function removeDevice(id: string) {
    try {
      await invoke('remove_device', { id })
      await fetchDevices()
    } catch (e) {
      notifyError('Failed to remove device', e)
    }
  }

  async function fetchLanPeers() {
    try {
      lanPeers.value = await invoke<LanPeer[]>('list_lan_peers')
    } catch (e) {
      notifyError('LAN scan failed', e)
    }
  }

  // ── Actions: Op-log conflicts + direct LAN sync (§2B.2/§2C close-out) ──
  const opConflicts = ref<{ a: OpEntry; b: OpEntry }[]>([])
  const lanSyncMsg = ref('')

  async function fetchOpConflicts() {
    try {
      const res = await invoke<{ conflicts?: { a: OpEntry; b: OpEntry }[] }>('oplog_conflicts')
      opConflicts.value = normalizeConflicts(res)
    } catch (e) {
      notifyError('Failed to load conflicts', e)
    }
  }

  async function resolveOpConflict(opA: string, opB: string, choice: 'keep-a' | 'keep-b' | 'merge') {
    try {
      const deviceId = devices.value[0]?.id ?? ''
      await invoke('oplog_resolve', { opA, opB, choice, deviceId })
      await fetchOpConflicts()
      notifySuccess(`Conflict resolved (${choice}) — history kept, resolution appended`)
    } catch (e) {
      notifyError('Failed to resolve conflict', e)
    }
  }

  async function syncWithPeer(deviceId: string, ops: unknown[] = [], attestation?: { signature: string; algorithm: string }) {
    try {
      if (
        typeof attestation?.signature !== 'string' ||
        !attestation.signature.trim() ||
        typeof attestation.algorithm !== 'string' ||
        !attestation.algorithm.trim()
      ) {
        throw new Error('auth: peer sync requires an ML-DSA attestation from an enrolled device; unsigned sync is disabled')
      }
      lanSyncMsg.value = `Syncing with ${deviceId}… (pull → push → merge)`
      const res = await invoke<{ appended?: number; skipped?: number; head?: number; verified?: boolean }>(
        'peer_sync',
        { id: deviceId, ops, ...attestation },
      )
      if (res.verified !== true) throw new Error('auth: peer did not verify the signed sync envelope')
      await fetchOpConflicts()
      const seal = ' · attested ✓'
      lanSyncMsg.value = `Synced with ${deviceId} (appended ${res.appended ?? 0}, skipped ${res.skipped ?? 0}${seal})`
      return res
    } catch (e) {
      notifyError('LAN sync failed', e)
      lanSyncMsg.value = ''
      return null
    }
  }

  // ── Actions: server-assisted presence (§2B.2 PEX — mobile/docker answer) ──
  const knownPeers = ref<PresencePeer[]>([])
  const presenceMsg = ref('')

  async function fetchKnownPeers() {
    try {
      knownPeers.value = await invoke<PresencePeer[]>('peers_known')
      return knownPeers.value
    } catch (e) {
      notifyError('Presence read failed', e)
      return []
    }
  }

  async function announcePresence(deviceId: string, host: string, port: number) {
    try {
      const entry = await invoke<PresencePeer>('peers_announce', { deviceId, host, port })
      presenceMsg.value = `Announced as ${entry.deviceId} @ ${entry.host}:${entry.port} (5 min TTL)`
      await fetchKnownPeers()
      return entry
    } catch (e) {
      notifyError('Presence announce failed', e)
      presenceMsg.value = ''
      return null
    }
  }

  // ── Actions: relay mailbox + pairing + swarm compute (§2B bonds) ──
  const relayMsg = ref('')
  const pairingMsg = ref('')

  async function relayDropMsg(to: string, kind: string, payload: unknown, from = '') {
    try {
      const res = await invoke<{ accepted?: boolean; id?: string }>('relay_drop', {
        from: from || devices.value[0]?.id || 'here',
        to,
        kind,
        payload: payload ?? null,
      })
      return res
    } catch (e) {
      notifyError('Relay drop failed', e)
      return null
    }
  }

  async function relayPollMsg(forDevice: string) {
    try {
      const res = await invoke<{ messages?: RelayMessage[] }>('relay_poll', { for: forDevice })
      return Array.isArray(res.messages) ? res.messages : []
    } catch (e) {
      notifyError('Relay poll failed', e)
      return []
    }
  }

  async function relayAckMsg(forDevice: string, ids: string[]) {
    try {
      const res = await invoke<{ cleared?: number }>('relay_ack', { for: forDevice, ids })
      return Number(res.cleared ?? 0)
    } catch (e) {
      notifyError('Relay ack failed', e)
      return 0
    }
  }

  async function makePairingCode() {
    try {
      const code = await invoke<string>('pairing_code')
      pairingMsg.value = `Code: ${code} — read it aloud; the other side claims pair:${code}`
      return code
    } catch (e) {
      notifyError('Pairing code failed', e)
      return null
    }
  }

  async function claimPairingCode(code: string) {
    try {
      const { pairingTarget } = await import('@/utils/pairing')
      const target = pairingTarget(code)
      const messages = await relayPollMsg(target)
      const ids = messages.map((m) => m.id).filter((id) => id.trim())
      if (ids.length > 0) await relayAckMsg(target, ids)
      pairingMsg.value =
        messages.length > 0
          ? `Claimed ${messages.length} envelope(s) from pair:${code}`
          : `Nothing waiting at pair:${code} yet — sender drops first`
      return messages
    } catch (e) {
      notifyError('Pairing claim failed', e)
      return []
    }
  }

  /** Pure-compute swarm/fountain coding (native IPC twins; Pages uses wasm exports directly). */
  async function swarmEncode(payloadBase64: string, data = 0, parity = 0) {
    try {
      // Tauri binds by exact param name — send both cases, the twin picks its own.
      return await invoke<unknown[]>('swarm_encode', {
        bytesBase64: payloadBase64, bytes_base64: payloadBase64, data, parity,
      })
    } catch (e) {
      notifyError('Swarm encode failed', e)
      return null
    }
  }

  async function swarmDecode(symbols: unknown) {
    try {
      const json = JSON.stringify(symbols)
      return await invoke<string>('swarm_decode', { symbolsJson: json, symbols_json: json })
    } catch (e) {
      notifyError('Swarm decode failed', e)
      return null
    }
  }

  async function fountainEncode(payloadBase64: string, mtu = 0, repair = 0) {
    try {
      return await invoke<unknown[]>('fountain_encode', {
        bytesBase64: payloadBase64, bytes_base64: payloadBase64, mtu, repair,
      })
    } catch (e) {
      notifyError('Fountain encode failed', e)
      return null
    }
  }

  async function fountainDecode(symbols: unknown) {
    try {
      const json = JSON.stringify(symbols)
      return await invoke<string>('fountain_decode', { symbolsJson: json, symbols_json: json })
    } catch (e) {
      notifyError('Fountain decode failed', e)
      return null
    }
  }

  // ── Actions: Server todos (§4.9 agent bridge — same rows the agent uses) ──
  const serverTodos = ref<ServerTodo[] | null>(null)

  async function fetchServerTodos() {
    try {
      serverTodos.value = await invoke<ServerTodo[]>('list_todos')
    } catch {
      // Static hosts have no dashboard — Archive TASKS falls back to local.
      serverTodos.value = null
    }
  }

  async function createServerTodo(title: string) {
    try {
      const row = await invoke<ServerTodo>('create_todo', { title })
      await fetchServerTodos()
      return row
    } catch (e) {
      notifyError('Failed to create todo', e)
      return null
    }
  }

  async function toggleServerTodo(id: string, done: boolean) {
    try {
      await invoke('toggle_todo', { id, done })
      await fetchServerTodos()
    } catch (e) {
      notifyError('Failed to update todo', e)
    }
  }

  async function deleteServerTodo(id: string) {
    try {
      await invoke('delete_todo', { id })
      await fetchServerTodos()
    } catch (e) {
      notifyError('Failed to delete todo', e)
    }
  }

  // ── Actions: Vault backup (scheduled + on-demand) ─────────────
  /** Run one `backup …` line everywhere (`os_exec` twin on each transport). */
  async function backupShell(line: string): Promise<string> {
    const res = await invoke<ShellResult>('os_exec', { line })
    if (!res.ok) throw new Error(res.output || res.error || 'backup command failed')
    return res.output
  }

  function parseBackupJson<T>(output: string): T {
    return JSON.parse(output) as T
  }

  async function fetchBackupList(dest: string): Promise<BackupEntry[]> {
    try {
      const output = await backupShell(`backup list ${dest} --json`)
      const rows = parseBackupJson<BackupEntry[]>(output)
      return Array.isArray(rows) ? rows : []
    } catch (e) {
      notifyError('Failed to list backups', e)
      return []
    }
  }

  async function exportBackupToProvider(
    dest: string,
    keep = 7,
    plain = false,
  ): Promise<BackupReport | null> {
    try {
      const output = await backupShell(
        `backup export --to ${dest} --keep ${keep}${plain ? ' --plain' : ''} --json`,
      )
      const report = parseBackupJson<BackupReport>(output)
      notifySuccess(
        `Backup exported (${report.bytes} bytes${report.encrypted ? ', sealed' : ', PLAINTEXT'})` +
          (report.pruned.length ? ` — pruned ${report.pruned.length} old` : ''),
      )
      return report
    } catch (e) {
      notifyError('Backup export failed', e)
      return null
    }
  }

  async function verifyBackupPath(path: string, deep = false): Promise<BackupVerify | null> {
    try {
      const output = await backupShell(`backup verify ${path}${deep ? ' --deep' : ''} --json`)
      return parseBackupJson<BackupVerify>(output)
    } catch (e) {
      notifyError('Backup verify failed', e)
      return null
    }
  }

  function base64ToBytes(b64: string): Uint8Array {
    const bin = atob(b64)
    const out = new Uint8Array(bin.length)
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
    return out
  }

  /**
   * Download a full vault snapshot now. Tauri/REST return the RAW image
   * (sealed here, so the passphrase never leaves the device); Pages
   * returns the already-sealed container. Either way the trial-decrypt
   * below proves the file before it lands in Downloads.
   */
  async function downloadBackup(passphrase: string): Promise<string | null> {
    try {
      const raw = (await invoke<Record<string, unknown>>('backup_export', { passphrase })) as {
        name?: unknown
        bytes?: unknown
        sealed?: unknown
        imageB64?: unknown
        imageBytes?: unknown
        blake3?: unknown
      }
      const { isContainer, decodeContainer, encodeContainer, containerPrimitives } =
        await import('@/utils/container')
      let sealed: Uint8Array
      let name = typeof raw.name === 'string' && raw.name ? raw.name : 'cybermanju.cybermanju'
      if (raw.bytes instanceof Uint8Array || Array.isArray(raw.bytes)) {
        // Pages/static shape: already a container.
        sealed = raw.bytes instanceof Uint8Array ? raw.bytes : Uint8Array.from(raw.bytes as number[])
      } else if (typeof raw.imageB64 === 'string') {
        // Tauri/REST shape: raw image — seal it on-device.
        const image = base64ToBytes(raw.imageB64)
        const primitives = await containerPrimitives().catch(() => null)
        if (!primitives) throw new Error('unsupported: database sealing needs the app build on this browser')
        sealed = await encodeContainer(image, passphrase, primitives)
        const { buildBackupName } = await import('@/utils/backup')
        if (!name || name === 'cybermanju.cybermanju') name = buildBackupName()
      } else {
        throw new Error('unsupported: database export failed on this build')
      }
      if (!isContainer(sealed)) throw new Error('integrity: export is not a container')
      // Trial-decrypt proves passphrase + integrity (AEAD) before download.
      await decodeContainer(sealed, passphrase)
      const blob = new Blob([sealed.buffer as ArrayBuffer], { type: 'application/octet-stream' })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = name
      document.body.appendChild(a)
      a.click()
      a.remove()
      setTimeout(() => URL.revokeObjectURL(url), 30_000)
      notifySuccess(`Backup downloaded (${sealed.length} bytes — store it away from this device)`)
      return name
    } catch (e) {
      notifyError('Backup download failed', e)
      return null
    }
  }

  /**
   * Verify picked backup bytes (upload / provider file): header always,
   * trial-decrypt when sealed. Wrong passphrases surface as `auth:`.
   */
  async function verifyBackupBytes(
    bytes: Uint8Array,
    passphrase: string,
  ): Promise<{ encrypted: boolean; imageBytes: number; legacy: boolean } | null> {
    try {
      const { verifyContainerHeader } = await import('@/utils/backup')
      const { decodeContainer } = await import('@/utils/container')
      const header = verifyContainerHeader(bytes)
      if (header.legacy) {
        return { encrypted: false, imageBytes: header.bodyBytes, legacy: true }
      }
      if (header.encrypted && !passphrase) {
        throw new Error('auth: this backup is passphrase-sealed — enter it to verify')
      }
      const decoded = await decodeContainer(bytes, passphrase)
      return { encrypted: header.encrypted, imageBytes: decoded.image.length, legacy: false }
    } catch (e) {
      const detail = e instanceof Error ? e.message : String(e)
      if (/wrong passphrase|did not decrypt|refused the passphrase/i.test(detail)) {
        notifyError('Backup verify failed', 'auth: container refused the passphrase (wrong key or damaged body)')
      } else {
        notifyError('Backup verify failed', e)
      }
      return null
    }
  }

  /**
   * Apply a verified image. Pages replaces the OPFS database + reloads;
   * every other transport needs the app stopped first, so this reports
   * back and the wizard shows the guided steps instead.
   */
  async function restoreBackupImage(image: Uint8Array): Promise<'reloaded' | 'manual'> {
    if (isStaticHost()) {
      await invoke('backup_restore', { image })
      notifySuccess('Backup restored — reloading onto the restored vault')
      setTimeout(() => window.location.reload(), 800)
      return 'reloaded'
    }
    return 'manual'
  }

  function findBackupSchedule(): ScheduleRow | undefined {
    return schedules.value.find((s) => s.path === MANAGED_SCRIPT_PATH)
  }

  /**
   * Schedule unattended snapshots: write the managed script (frontmatter
   * owns the expr) via `os_write` on every transport, then `cron add`.
   */
  async function scheduleBackup(
    expr: string,
    dest: string,
    keep = 7,
    plain = false,
  ): Promise<ScheduleRow | null> {
    try {
      const { managedBackupScript, MANAGED_SCRIPT_PATH: scriptPath } = await import('@/utils/backup')
      parseSchedule(expr.trim()) // throws `invalid:` before anything is written
      const existing = findBackupSchedule()
      if (existing && existing.expr !== expr.trim()) {
        await invoke('cron_delete', { id: existing.id })
      }
      const script = managedBackupScript(expr.trim(), dest, { keep, plain })
      await invoke('os_exec', { line: 'mkdir -p "/.cybermanju"' }).catch(() => null)
      await invoke('os_write', { path: scriptPath, content: script })
      const saved = await cronSave({
        id: existing?.id ?? '',
        path: scriptPath,
        expr: expr.trim(),
        description: 'Nightly vault snapshot (Backup panel)',
        enabled: true,
      })
      if (saved) notifySuccess(`Backup scheduled (${expr.trim()} → ${dest}, next ${saved.nextFireAt ?? 'soon'})`)
      return saved
    } catch (e) {
      notifyError('Failed to schedule backup', e)
      return null
    }
  }

  async function listBackupMounts(): Promise<{ id: string; name: string; backendType: string; configId: string }[]> {
    try {
      const canal = await import('@/composables/useProviderCanal')
      return await canal.listVfsMounts()
    } catch {
      return []
    }
  }

  /**
   * Pull one backup file's bytes from a provider destination
   * (`/providers/<mount>[/sub]` + file name) for verify/restore.
   */
  async function fetchBackupBytes(dest: string, name: string): Promise<Uint8Array | null> {
    try {
      const head = dest.replace(/^\/providers\//, '').split('/').filter(Boolean)
      const key = head.shift() ?? ''
      const sub = head.join('/')
      const mounts = await listBackupMounts()
      const mount =
        mounts.find((m) => m.id === key) ??
        mounts.find((m) => m.configId === key) ??
        mounts.find((m) => m.name.toLowerCase() === key.toLowerCase())
      if (!mount) throw new Error(`not_found: no provider mount \`${key}\``)
      const canal = await import('@/composables/useProviderCanal')
      const file = await canal.readVfsFile(mount.id, sub ? `${sub}/${name}` : name)
      return file.bytes
    } catch (e) {
      notifyError('Failed to read backup file', e)
      return null
    }
  }

  // ── Actions: AI agent ─────────────────────────────────────
  const agentProviders = ref<ProviderPreset[]>([])
  const agentConfigs = ref<AgentConfig[]>([])
  const agentSessions = ref<AgentSession[]>([])
  const agentJobs = ref<AgentJob[]>([])
  const activeAgentJob = ref<AgentJob | null>(null)

  async function fetchAgentProviders() {
    try {
      agentProviders.value = await invoke<ProviderPreset[]>('list_agent_providers')
    } catch (e) {
      notifyError('Failed to fetch providers', e)
    }
  }

  async function fetchAgentConfigs() {
    try {
      agentConfigs.value = await invoke<AgentConfig[]>('list_agent_configs')
    } catch (e) {
      notifyError('Failed to fetch agent configs', e)
    }
  }

  async function saveAgentConfig(config: Omit<AgentConfig, 'id' | 'hasKey' | 'createdAt' | 'updatedAt'> & { id?: string }) {
    try {
      const saved = await invoke<AgentConfig>('save_agent_config', {
        config: { ...config, id: config.id ?? '', hasKey: false, createdAt: '', updatedAt: '' },
      })
      await fetchAgentConfigs()
      notifySuccess(`Agent config '${saved.name}' saved`)
      return saved
    } catch (e) {
      notifyError('Failed to save agent config', e)
      return null
    }
  }

  async function deleteAgentConfig(configId: string) {
    try {
      await invoke('delete_agent_config', { configId })
      await fetchAgentConfigs()
      notifySuccess('Agent config deleted')
    } catch (e) {
      notifyError('Failed to delete agent config', e)
    }
  }

  async function saveAgentKey(configId: string, apiKey: string) {
    try {
      await invoke('save_agent_key', { configId, apiKey })
      await fetchAgentConfigs()
      notifySuccess('API key sealed')
      return true
    } catch (e) {
      notifyError('Failed to save API key', e)
      return false
    }
  }

  async function refreshAgentModels(configId: string) {
    try {
      return await invoke<string[]>('list_agent_models', { configId })
    } catch (e) {
      notifyError('Model refresh failed', e)
      return null
    }
  }

  async function fetchAgentSessions() {
    try {
      agentSessions.value = await invoke<AgentSession[]>('list_agent_sessions')
    } catch (e) {
      notifyError('Failed to fetch agent sessions', e)
    }
  }

  async function loadAgentSession(sessionId: string) {
    try {
      return await invoke<AgentSession>('get_agent_session', { sessionId })
    } catch (e) {
      notifyError('Failed to load agent session', e)
      return null
    }
  }

  async function deleteAgentSession(sessionId: string) {
    try {
      await invoke('delete_agent_session', { sessionId })
      await fetchAgentSessions()
    } catch (e) {
      notifyError('Failed to delete agent session', e)
    }
  }

  async function startAgentRun(configId: string, prompt: string, sessionId?: string) {
    try {
      const job = await invoke<AgentJob>('start_agent_run', { configId, sessionId, prompt })
      activeAgentJob.value = job
      await subscribeAgentJob(job.jobId)
      return job
    } catch (e) {
      notifyError('Failed to start agent run', e)
      return null
    }
  }

  /**
   * REST dashboard job tail: `GET /api/agent/jobs/:id/events` streams
   * snapshots over a fetch reader. Native IPC and static WASM builds without
   * a configured server have no local `/events` endpoint, so they poll via
   * `invoke()` directly instead of probing localhost. Any REST stream failure
   * also degrades to the 1.5 s `pollAgentJob` loop.
   */
  async function subscribeAgentJob(jobId: string) {
    const applySnapshot = (job: AgentJob) => {
      activeAgentJob.value = job
      const i = agentJobs.value.findIndex(j => j.jobId === jobId)
      if (i >= 0) agentJobs.value[i] = job
      else agentJobs.value.unshift(job)
      announceAgentJob(jobId, job)
      applyAgentUiRequest(job)
      return job.status === 'done' || job.status === 'error' || job.status === 'cancelled'
    }
    try {
      const { getServerUrl, getAuthToken, isStaticHost, isTauri } = await import('@/composables/useTauri')
      const { parseAgentStreamChunk, isStreamDone, resolveAgentEventsBase } = await import('@/utils/agentStream')
      const base = resolveAgentEventsBase({
        serverUrl: getServerUrl(),
        isTauri: isTauri(),
        isStaticHost: isStaticHost(),
        origin: typeof window !== 'undefined' ? window.location.origin : 'http://localhost:3456',
      })
      if (!base) {
        await pollAgentJob(jobId)
        return
      }
      const headers: Record<string, string> = { Accept: 'text/event-stream' }
      const token = getAuthToken()
      if (token) headers['Authorization'] = `Bearer ${token}`
      const res = await fetch(`${base}/api/agent/jobs/${encodeURIComponent(jobId)}/events`, { headers })
      if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`)
      const reader = res.body.getReader()
      const decoder = new TextDecoder()
      let buf = ''
      for (;;) {
        const { done, value } = await reader.read()
        if (done) break
        buf += decoder.decode(value, { stream: true })
        const frames = buf.split('\n\n')
        buf = frames.pop() ?? ''
        for (const frame of frames) {
          for (const ev of parseAgentStreamChunk(`${frame}\n\n`)) {
            if (isStreamDone(ev.data)) {
              await pollAgentJob(jobId)
              return
            }
            if (ev.event !== 'job') continue
            try {
              if (applySnapshot(JSON.parse(ev.data) as AgentJob)) {
                try { reader.cancel() } catch { /* already closed */ }
                return
              }
            } catch {
              // Malformed frame — the next poll heals the view.
            }
          }
        }
      }
      // Stream ended (cap, idle close) with the job still live.
      const cur = activeAgentJob.value
      if (cur && cur.jobId === jobId && (cur.status === 'running' || cur.status === 'waiting_approval')) {
        await pollAgentJob(jobId)
      }
    } catch {
      await pollAgentJob(jobId)
    }
  }

  async function pollAgentJob(jobId: string) {
    try {
      const job = await invoke<AgentJob>('agent_job_status', { jobId })
      activeAgentJob.value = job
      const i = agentJobs.value.findIndex(j => j.jobId === jobId)
      if (i >= 0) agentJobs.value[i] = job
      else agentJobs.value.unshift(job)
      announceAgentJob(jobId, job)
      applyAgentUiRequest(job)
      if (job.status === 'running' || job.status === 'waiting_approval') {
        setTimeout(() => pollAgentJob(jobId), 1500)
      }
    } catch (e) {
      notifyError('Failed to poll agent job', e)
    }
  }

  /**
   * Apply a one-shot `ui_request` (from `ui_open_panel`/`ui_notify`) exactly
   * once per `uiSeq`. Deduped across the SSE subscribe and the 1.5 s poll
   * fallback — a replayed snapshot never re-opens a panel or re-fires a toast.
   */
  const agentUiDedupe = createUiRequestDedupe()
  function applyAgentUiRequest(job: AgentJob) {
    if (!job.uiRequest) return
    agentUiDedupe.apply(job.uiRequest, {
      open: (panel, props) => {
        useWindowManager().open(panel, props)
      },
      notify: (level, msg) => {
        notify(level, msg)
      },
    })
  }

  /**
   * Announce the two things a user must not miss — the agent is blocked on
   * them, and the run is over — exactly once per job and status.
   */
  const agentJobSeen: Record<string, string> = {}
  function announceAgentJob(jobId: string, job: AgentJob) {
    if (agentJobSeen[jobId] === job.status) return
    agentJobSeen[jobId] = job.status
    if (job.status === 'waiting_approval') {
      const tool = job.pending?.tool ?? 'a tool'
      notifySuccess(`Agent waiting for approval — ${tool}`)
      return
    }
    if (job.status === 'done') {
      const out = (job.result ?? '').trim().split('\n')[0]?.slice(0, 120)
      notifySuccess(out ? `Agent finished — ${out}` : 'Agent finished')
      return
    }
    if (job.status === 'error') notifyError('Agent run failed', job.error ?? 'unknown error')
    if (job.status === 'cancelled') notifySuccess('Agent run cancelled')
  }

  async function abortAgentJob(jobId: string) {
    try {
      await invoke('abort_agent_job', { jobId })
      await pollAgentJob(jobId)
    } catch (e) {
      notifyError('Failed to abort agent job', e)
    }
  }

  async function approveAgentJob(jobId: string, approved: boolean, answer?: string, remember?: boolean) {
    try {
      await invoke('approve_agent_job', { jobId, approved, answer, remember: remember ?? false })
      await pollAgentJob(jobId)
    } catch (e) {
      notifyError('Failed to answer approval', e)
      return null
    }
  }

  async function initAgentRun(configId: string) {
    try {
      const job = await invoke<AgentJob>('init_agent_run', { configId })
      activeAgentJob.value = job
      await subscribeAgentJob(job.jobId)
      notifySuccess('Repo-init started — the agent will write AGENTS.md')
      return job
    } catch (e) {
      notifyError('Failed to start repo-init', e)
      return null
    }
  }

  async function compactAgentSession(configId: string, sessionId: string) {
    try {
      const compacted = await invoke<AgentSession>('compact_agent_session', { configId, sessionId })
      await fetchAgentSessions()
      notifySuccess('Session compacted — old transcript kept for revert')
      return compacted
    } catch (e) {
      notifyError('Compaction failed', e)
      return null
    }
  }

  // ── Semantic memory (redb `agent_memories`: text + vectors) ──
  const agentMemories = ref<AgentMemory[]>([])

  async function fetchAgentMemories(configId?: string) {
    try {
      agentMemories.value = await invoke<AgentMemory[]>('list_agent_memories', { configId })
    } catch (e) {
      notifyError('Failed to fetch memories', e)
    }
  }

  async function storeAgentMemory(configId: string, text: string, sessionId?: string) {
    try {
      const saved = await invoke<AgentMemory>('store_agent_memory', { configId, sessionId, text })
      await fetchAgentMemories(configId)
      notifySuccess('Remembered for future sessions')
      return saved
    } catch (e) {
      notifyError('Failed to store memory', e)
      return null
    }
  }

  async function recallAgentMemories(query: string, configId?: string, topK = 3) {
    try {
      return await invoke<MemoryHit[]>('recall_agent_memories', { configId, query, topK })
    } catch (e) {
      notifyError('Memory recall failed', e)
      return null
    }
  }

  async function deleteAgentMemory(memoryId: string, configId?: string) {
    try {
      await invoke('delete_agent_memory', { memoryId })
      await fetchAgentMemories(configId)
      notifySuccess('Memory deleted')
    } catch (e) {
      notifyError('Failed to delete memory', e)
    }
  }

  async function mcpAddServer(configId: string, name: string, server: McpServerConfig) {
    try {
      const updated = await invoke<AgentConfig>('mcp_add_server', { configId, name, server })
      await fetchAgentConfigs()
      notifySuccess(`MCP server '${name}' attached`)
      return updated
    } catch (e) {
      notifyError('Failed to attach MCP server', e)
      return null
    }
  }

  async function mcpRemoveServer(configId: string, name: string) {
    try {
      await invoke('mcp_remove_server', { configId, name })
      await fetchAgentConfigs()
      notifySuccess(`MCP server '${name}' detached`)
    } catch (e) {
      notifyError('Failed to detach MCP server', e)
    }
  }

  async function mcpListTools(configId: string) {
    try {
      return await invoke<Array<{ server: string; name: string; description: string }>>('mcp_list_tools', { configId })
    } catch (e) {
      notifyError('MCP discovery failed', e)
      return null
    }
  }

  async function fetchDisks() {
    try {
      disks.value = await invoke<DiskRow[]>('list_disks')
    } catch (e) {
      notifyError('Failed to fetch disks', e)
    }
  }

  async function createDisk(configId: string, sizeBytes: number, passphrase: string) {
    try {
      if (!passphrase) {
        notifyError('Failed to create disk', 'auth: a disk passphrase must not be empty — type one on the disk card first')
        return null
      }
      const disk = await invoke<DiskRow>('create_disk', { configId, sizeBytes, passphrase })
      await fetchDisks()
      await fetchOsDf()
      notifySuccess(`Disk created — ${disk?.id ?? 'ok'}`)
      return disk
    } catch (e) {
      notifyError('Failed to create disk', e)
      return null
    }
  }

  async function attachDisk(diskId: string, passphrase: string) {
    try {
      await invoke('attach_disk', { id: diskId, passphrase })
      await fetchDisks()
      await fetchOsDf()
      notifySuccess('Disk attached — volume grew')
    } catch (e) {
      notifyError('Failed to attach disk', e)
    }
  }

  async function detachDisk(diskId: string) {
    try {
      await invoke('detach_disk', { id: diskId })
      await fetchDisks()
      await fetchOsDf()
      notifySuccess('Disk detached')
    } catch (e) {
      notifyError('Failed to detach disk', e)
    }
  }

  async function resizeDisk(diskId: string, sizeBytes: number) {
    try {
      await invoke('resize_disk', { id: diskId, sizeBytes })
      await fetchDisks()
      await fetchOsDf()
      notifySuccess('Disk resized')
    } catch (e) {
      notifyError('Failed to resize disk', e)
    }
  }

  async function checkDisk(diskId: string) {
    try {
      const report = await invoke<{ ok: boolean }>('check_disk', { id: diskId })
      await fetchDisks()
      if (report?.ok) notifySuccess('Disk check passed')
      else notifyError('Disk check found problems', 'see the disk card for details')
      return report
    } catch (e) {
      notifyError('Failed to check disk', e)
      return null
    }
  }

  async function setDiskKeyHolder(diskId: string) {
    try {
      await invoke('set_disk_key_holder', { id: diskId })
      await fetchDisks()
      notifySuccess('Key holder set — this disk unwraps the others')
      return true
    } catch (e) {
      notifyError('Failed to set key holder', e)
      return false
    }
  }

  return {
    // State
    currentPath, currentPanel, viewMode, selectedFileId, sidebarSection, sidebarCollapsed,
    files, accounts, activeAccountId, collections, faceGroups, looseGroups,
    searchResults, geoMarkers, encryptionStatus, encryptionKeys,
    compressionStats, parseResult, syncConfigs, syncProgress,
    syncJobs, syncRuns, syncStatus, repairStatus, scrubRuns, leaseInfo, lastGc,
    osPs, osTop, osWorkers, osJobs, osDf, disks, shellBusy,
    schedules,
    secrets,
    trashItems, showTrashPanel, auditLog, fileVersions, dashboardStatus, shareLinks,
    searchQuery, searchTotalResults, isSearching, isLoading, lastError, wasmGapCount, matrixRainEnabled,
    commandPaletteOpen, lastSyncAt, lastSyncSummary,
    showShortcutsHelp, createFolderPromptOpen,
    selectedFileIds, isMultiSelect, users, autoRefreshInterval, sortBy,
    // Computed
    currentUser, selectedFile, activeAccount, encryptedFiles, compressedFiles,
    starredFiles, folders, currentFolderFiles,
    // Actions
    initialize, selectFile, toggleStar, clearError,
    fetchFiles, getFile, createFolder, deleteFile, renameFile, duplicateFileContext, setFileTags,
    searchFiles, loadMoreSearchResults, fetchEncryptionStatus, generateKeypair, listKeys, encryptFile, decryptFile,
    compressFile, decompressFile, fetchCollections, createCollection, addToCollection, removeFromCollection,
    fetchFaceGroups, detectFaces, detectFacesBatch, reclusterFaces,
    renameFaceGroup, mergeFaceGroups, deleteFaceGroup, findSimilarFaces,
    fetchAccounts, createAccount, switchAccount, deleteAccount, fetchGeoFiles,
    parseFileCode, parseCodeText, fetchLooseGroups, createLooseGroup, addFileToLooseGroup,
    readManagedContent, saveManagedContent, readWasmFile, saveWasmFile, listWasmDir,
    fetchSyncConfigs, createSyncConfig, saveSyncConfig, probeSyncConnection, deleteSyncConfig, startSync,
    getSyncProgress, testSyncConnection, cancelSync, listRemoteFiles, refreshAfterSync,
    getSyncJob, fetchSyncRuns, fetchSyncStatus, restoreSyncFile, deleteRemoteFile, moveSyncFile,
    fetchSyncUsage, oauthStart, createProviderRepo, seedRepoFiles, uploadRemoteFile,
    fetchRepairStatus, runRepair, runRebuild, runGc, runScrub, fetchScrubRuns,
    fetchRepairTasks, fetchRepairHealth,
    acquireLease, releaseLease, fetchLeaseStatus,
    // OS layer (cybsh, tasks, compute, disks, volume)
    execShellLine, completeShellLine,
    fetchOsPs, fetchOsTop, fetchOsWorkers, fetchOsJobs, fetchOsDf,
    killOsTask, runComputeJob,
    // Scheduler (cron)
    fetchSchedules, cronSave, cronDelete, cronRun, cronSetEnabled, cronHistory,
    fetchSecrets, createSecret, updateSecret, deleteSecret, revealSecret,
    devices, lanPeers, fetchDevices, registerDevice, registerThisDevice,
    revokeDevice, removeDevice, fetchLanPeers,
    knownPeers, presenceMsg, fetchKnownPeers, announcePresence,
    relayMsg, pairingMsg, relayDropMsg, relayPollMsg, relayAckMsg,
    makePairingCode, claimPairingCode,
    swarmEncode, swarmDecode, fountainEncode, fountainDecode,
    opConflicts, lanSyncMsg, fetchOpConflicts, resolveOpConflict, syncWithPeer,
    serverTodos, fetchServerTodos, createServerTodo, toggleServerTodo, deleteServerTodo,
    backupShell, fetchBackupList, exportBackupToProvider, verifyBackupPath,
    downloadBackup, verifyBackupBytes, restoreBackupImage, scheduleBackup,
    findBackupSchedule, listBackupMounts, fetchBackupBytes,
    tickSchedules, startScheduleTick,
    agentProviders, agentConfigs, agentSessions, agentJobs, activeAgentJob,
    fetchAgentProviders, fetchAgentConfigs, saveAgentConfig, deleteAgentConfig,
    saveAgentKey, refreshAgentModels, fetchAgentSessions, loadAgentSession, deleteAgentSession,
    startAgentRun, pollAgentJob, subscribeAgentJob, abortAgentJob, approveAgentJob, initAgentRun,
    compactAgentSession, mcpAddServer, mcpRemoveServer, mcpListTools,
    agentMemories, fetchAgentMemories, storeAgentMemory, recallAgentMemories, deleteAgentMemory,
    fetchDisks, createDisk, createDiskWithRemote, attachDisk, detachDisk, resizeDisk, checkDisk, setDiskKeyHolder,
    // User Management
    fetchUsers, createUser, deleteUser, updateUserRole,
    // Trash
    fetchTrashItems, restoreTrashItem, emptyTrash, deleteFromTrash,
    // Audit
    fetchAuditLog,
    // Versions
    fetchFileVersions, createVersion, revertToVersion, snapshotAllVersions,
    // Batch
    batchDeleteFiles, batchEncryptFiles, batchCompressFiles,
    // Dashboard
    fetchDashboardStatus, startDashboard, stopDashboard,
    // Share Links
    generateShareLink, fetchShareLinks,
    // URL Import
    importFromUrl,
    // Auth
    setSessionToken, logout, login, register, checkAuthStatus, requiresServerAuth,
    needsAuth, needsSetup, authChecked, authBusy, authError, authToken, isAuthenticated,
    // Utility
    rebuildParentIndex,
    notifySuccess,
    notifyError,
  }
})
