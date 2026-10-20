// CyberManju OS — Index (src/types/index.ts).
// Role: module (see exports below).
// Exports: ViewMode, PanelType, SidebarSection, ModuleInfo, EncryptionAlgo, CompressionType, AccountType, CollectionType.
// Integrates: standalone (no imports).
// Contracts: signals: AGENT-, integrity, conflict.
export type ViewMode = 'grid' | 'list' | 'masonry' | 'columns' | 'details'
export type PanelType = 'landing' | 'files' | 'preview' | 'encryption' | 'compression' | 'collections' | 'faces' | 'map' | 'code' | 'editor' | 'agent' | 'search' | 'style' | 'accounts' | 'loose-groups' | 'sync' | 'transfer' | 'webdash' | 'users' | 'dashboard' | 'settings' | 'trash' | 'activity' | 'favorites' | 'recent' | 'storage' | 'terminal' | 'processes' | 'disks' | 'devices' | 'permissions' | 'cron' | 'automation' | 'schedules' | 'secrets' | 'passwords' | 'credentials' | 'install' | 'archive'
export type SidebarSection = 'tree' | 'locations' | 'collections' | 'people' | 'styles' | 'loose' | 'users' | 'sync' | 'dashboard' | 'landing' | 'tools'

export interface ModuleInfo {
  id: PanelType
  label: string
  icon: string
  color: string
  gradient: string
  description: string
  requiresAuth: boolean
}
export type EncryptionAlgo = 'kyber1024' | 'dilithium5' | 'frodokem1344' | 'hybrid' | 'aes256'
export type CompressionType = 'none' | 'lz4' | 'brotli' | 'zstd' | 'triple'
export type AccountType = 'local' | 'cloud' | 'network'
export type CollectionType = 'highlights' | 'best_moments' | 'custom'

export interface FileNode {
  id: string
  name: string
  fileType: string
  parentId?: string
  sizeBytes: number
  mimeType?: string
  hashBlake3?: string
  encrypted: boolean
  encryptionAlgorithm?: string
  compressionLayers: string[]
  thumbnailPath?: string
  contextData?: Record<string, unknown>
  tags?: string[]
  collectionIds?: string[]
  faceGroupIds?: string[]
  looseGroupIds?: string[]
  gpsLat?: number
  gpsLon?: number
  createdAt: string
  modifiedAt: string
  isStarred?: boolean
  isHidden?: boolean
  path?: string
  children?: FileNode[]
  accountId?: string
  locationId?: string
  contentText?: string
  treeSitterAst?: string
  permissions?: FilePermission[]
}

export interface FilePermission {
  userId: string
  username: string
  access: 'read' | 'write' | 'admin'
}

export interface Account {
  id: string
  name: string
  accountType: string
  path?: string
  color: string
  isActive: boolean
  createdAt: string
  updatedAt: string
}

export interface CloudAccount {
  id: string
  name: string
  backendType: SyncBackendType
  token?: string
  config: Record<string, unknown>
  createdAt: string
  updatedAt: string
}

export interface Collection {
  id: string
  name: string
  collectionType: CollectionType
  color: string
  description?: string
  itemIds: string[]
  createdAt: string
  updatedAt: string
}

export interface FaceGroup {
  id: string
  name: string
  color?: string
  icon?: string
  fileIds: string[]
  centroidEmbedding?: number[]
  binaryHash?: number
  cohesion?: number
  embeddingCount: number
  algorithm?: string
  /** Which detector filled the group (`onnx` / `heuristic-v2` / `mixed`); absent = written before engine reporting. */
  detectionEngine?: string
  createdAt: string
}

export interface EncryptionKeyInfo {
  id: string
  algorithm: string
  algorithmDisplay: string
  nistLevel: number
  color: string
  publicKeyPreview: string
  hasPrivateKey: boolean
  createdAt: string
}

export interface EncryptionStatus {
  isEncrypted: boolean
  algorithm?: string
  nistLevel?: number
  keyId?: string
  encryptedAt?: string
}

export interface LooseGroup {
  id: string
  name: string
  color: string
  icon?: string
  fileIds: string[]
  createdAt: string
}

export interface SearchResult {
  fileId: string
  fileName: string
  score: number
  snippet?: string
  matchType?: string
}

export interface GeoMarker {
  fileId: string
  fileName: string
  lat: number
  lng: number
  address?: string
  thumbnail?: string
}

export interface CompressionStats {
  originalSize: number
  compressedSize: number
  ratio: number
  layer: string
  layerDetails: LayerDetail[]
  blake3Hash: string
  durationMs: number
}

export interface LayerDetail {
  name: string
  algorithm: string
  inputSize: number
  outputSize: number
  ratio: number
  color: string
}

export interface CodeSymbol {
  name: string
  kind: string
  startLine: number
  endLine: number
  detail?: string
  children: CodeSymbol[]
}

export interface ParseResult {
  filePath: string
  language: string
  /** Which engine ran: real grammars (desktop) or heuristic fallback. */
  engine?: 'tree-sitter' | 'heuristic' | string
  symbols: CodeSymbol[]
  totalLines: number
  parseTimeMs: number
}

/** Text content of a managed file for the code editor. */
export interface FileContent {
  fileId: string
  name: string
  content: string
  sizeBytes: number
  truncated: boolean
  hashBlake3?: string | null
}

/** Summary returned after saving edited content. */
export interface SavedContent {
  fileId: string
  name: string
  sizeBytes: number
  hashBlake3?: string | null
  modifiedAt: string
}

export interface User {
  id: string
  username: string
  passwordHash?: string
  displayName?: string
  role: 'admin' | 'user' | 'viewer'
  isActive: boolean
  createdAt: string
  updatedAt: string
}

export interface UserFilePermission {
  id: string
  userId: string
  fileId: string
  access: 'read' | 'write' | 'admin'
  grantedBy: string
  grantedAt: string
}

export interface AuthResult {
  userId: string
  username: string
  role: string
  displayName?: string
  token: string
}

export interface DashboardStatus {
  running: boolean
  port: number
  url: string
  activeConnections: number
}

export interface ApiEndpoint {
  method: string
  path: string
  description: string
}

export interface TrashItem {
  id: string
  originalFile: FileNode
  deletedAt: string
  deletedBy?: string
  restorePath?: string
}

export interface AuditEntry {
  id: string
  action: string
  entityType: string
  entityId: string
  userId?: string
  details?: Record<string, unknown>
  timestamp: string
}

export interface FileVersion {
  id: string
  fileId: string
  versionNumber: number
  hashBlake3?: string
  sizeBytes: number
  snapshotData?: string
  createdAt: string
}

export interface ShareLink {
  id: string
  fileId: string
  token: string
  expiresAt: string
  url: string
}

export const CYBER = {
  bgDeep: '#000000',
  bgPanel: '#000000',
  bgCard: '#FFFFFF',
  bgHover: '#FFFFFF',
  borderHeavy: '#FFFFFF',
  borderNeon: '#FFFFFF',
  borderGold: '#FFFFFF',
  saffronGold: '#FFFFFF',
  lotusPink: '#FFFFFF',
  templeOrange: '#FFFFFF',
  prayerBlue: '#FFFFFF',
  prayerWhite: '#FFFFFF',
  prayerRed: '#FFFFFF',
  prayerGreen: '#FFFFFF',
  prayerYellow: '#FFFFFF',
  matrixGreen: '#FFFFFF',
  matrixDarkGreen: '#000000',
  cyberBlue: '#FFFFFF',
  cyberPurple: '#FFFFFF',
  neonPink: '#FFFFFF',
  neonYellow: '#FFFFFF',
  textPrimary: '#FFFFFF',
  textSecondary: '#FFFFFF',
  textMuted: '#FFFFFF',
  textNeon: '#FFFFFF',
} as const

export const PRAYER_FLAGS = ['#FFFFFF', '#000000', '#FFFFFF', '#000000', '#FFFFFF'] as const

export const ENCRYPTION_INFO: Record<EncryptionAlgo, { name: string; nistLevel: number; description: string; color: string }> = {
  kyber1024: {
    name: 'ML-KEM (Kyber-1024)',
    nistLevel: 5,
    description: 'NIST FIPS 203 - Lattice-based key encapsulation. Resistant to Shor\'s algorithm and all known quantum attacks.',
    color: '#FFFFFF',
  },
  dilithium5: {
    name: 'ML-DSA (Dilithium-5)',
    nistLevel: 5,
    description: 'NIST FIPS 204 - Lattice-based digital signature. Maximum security level, quantum-resistant signing.',
    color: '#FFFFFF',
  },
  frodokem1344: {
    name: 'FrodoKEM-1344',
    nistLevel: 3,
    description: 'Learning-with-errors based. Conservative security estimates with classical ring structure.',
    color: '#FFFFFF',
  },
  hybrid: {
    name: 'Hybrid PQ+Classical',
    // P0-15: ML-KEM-768 is NIST Level 3 — never claim 5.
    nistLevel: 3,
    description: 'Combines ML-KEM-768 (Level 3) with X25519 for defense-in-depth transitional security.',
    color: '#FFFFFF',
  },
  aes256: {
    name: 'AES-256-GCM',
    nistLevel: 0,
    description: 'Classical symmetric encryption. Not quantum-resistant - recommended only in hybrid mode.',
    color: '#FFFFFF',
  },
}

export const COMPRESSION_INFO: Record<CompressionType, { name: string; description: string; color: string; speed: string }> = {
  none: { name: 'None', description: 'Uncompressed raw data', color: '#FFFFFF', speed: 'Instant' },
  lz4: { name: 'LZ4 (lz4_flex)', description: 'Ultra-fast pure Rust compression (~400 MB/s). Real-time previews and streaming.', color: '#FFFFFF', speed: 'Ultra-Fast' },
  brotli: { name: 'Brotli-11', description: 'Google\'s format, quality 11 — runs natively in the browser wasm pack.', color: '#FFFFFF', speed: 'Medium' },
  zstd: { name: 'Zstandard (zstd)', description: 'Facebook\'s algorithm. Excellent ratio/speed balance, configurable levels 1-22.', color: '#FFFFFF', speed: 'Fast' },
  triple: { name: 'Triple-Layer', description: 'LZ4 -> ZSTD-15 -> Brotli-11 cascading. Maximum compression for archival.', color: '#FFFFFF', speed: 'Slow' },
}

export type SyncBackendType = 'local' | 'github' | 'gitlab' | 'googleDrive'
export type SyncStatusType =
  | 'idle'
  | 'scanning'
  | 'compressing'
  | 'uploading'
  | 'linking'
  | 'cleaning'
  | 'error'
  | 'done'
  | 'syncing'
  | 'completed'
  | 'cancelled'

export interface SyncConfig {
  id: string
  backendType: SyncBackendType
  enabled: boolean
  accountId?: string
  name?: string
  basePath?: string
  repoName?: string
  branch?: string
  token?: string
  folderId?: string
  autoSync: boolean
  compressBeforeUpload: boolean
  createPreviews: boolean
  deleteRawAfterSync: boolean
  maxConcurrentUploads: number
  encryptBeforeUpload?: boolean
  conflictPolicy?: 'skip' | 'overwrite' | 'keepBoth'
  placement?: 'whole' | 'striped'
  parity?: number
  /** Refuse upload when no master passphrase exists (default false = warn + upload). */
  requireEncryption?: boolean
  /** Hash basenames into remote locators so providers never see real names. */
  obfuscateNames?: boolean
  /**
   * Duplicate-everywhere opt-in. Default false = single-copy home: the file
   * lives on its `homeConfigId` provider only; `true` restores the legacy
   * batch-copy behaviour for this provider (mirror target).
   */
  mirror?: boolean
  /** This provider's `.cybermanju` holds the keys that unlock other disks (user-chosen). */
  keyHolder?: boolean
  createdAt?: string
  updatedAt?: string
}

export interface SyncFile {
  id: string
  originalPath: string
  compressedPath?: string
  previewPath?: string
  remoteUrl?: string
  /** Which provider currently holds the single copy (unified-disk home). */
  configId?: string
  /** Preferred home provider chosen by the user (`sync_configs.id`). */
  homeConfigId?: string
  remotePath?: string
  sizeBytes: number
  compressedSizeBytes?: number
  hashBlake3?: string
  backendType: SyncBackendType
  syncedAt?: string
  status: SyncStatusType
  errorMessage?: string
}

export interface SyncProgress {
  totalFiles: number
  processedFiles: number
  currentFile?: string
  status: SyncStatusType
  bytesUploaded: number
  errors: string[]
  startedAt?: string
  estimatedRemainingSeconds?: number
}

export interface SyncResult {
  filesSynced: number
  bytesUploaded: number
  bytesSavedByCompression: number
  errors: string[]
  durationMs: number
}

export interface RemoteFile {
  name: string
  path: string
  sizeBytes: number
  modifiedAt: string
  url: string
}

// ── Sync jobs / runs / restore / quota (AGENT-2 REST contract) ──

export interface SyncJob {
  jobId: string
  configId: string
  startedAt: string
  finishedAt?: string | null
  status: SyncStatusType
  progress: SyncProgress
  result?: SyncResult | null
}

export interface SyncRunRecord {
  runId: string
  configId: string
  startedAt: string
  finishedAt?: string | null
  status: SyncStatusType
  filesSynced: number
  bytesUploaded: number
  errors: string[]
  progress?: SyncProgress | null
  result?: SyncResult | null
}

export interface RestoreOutcome {
  path: string
  bytes: number
  verified: boolean
}

export interface QuotaUsage {
  backendType?: string
  totalBytes?: number | null
  usedBytes?: number | null
  remainingRequests?: number | null
  requestLimit?: number | null
  resetAt?: string | null
  detail?: string
  [key: string]: unknown
}

export interface ChunkManifestEntry {
  index: number
  hashBlake3: string
  sizeBytes: number
  providers: string[]
}

export interface ChunkManifest {
  version: number
  fileId: string
  chunkSize: number
  totalSize: number
  chunks: ChunkManifestEntry[]
}

// ── Durability (AGENT-7): scrub / repair / gc / leases ──

export interface ScrubRun {
  runId: string
  status: string
  checked: number
  ok: number
  corrupt: number
  missing: number
  durationMs: number
  findings?: unknown[]
  [key: string]: unknown
}

export interface RepairStatus {
  queuedFindings: number
  repairs: unknown[]
  repaired: number
  unrecoverable: number
  skipped: number
  tasks: unknown[]
  [key: string]: unknown
}

export interface RepairTask {
  taskId: string
  kind: string
  detail?: unknown
  [key: string]: unknown
}

export interface GcReport {
  checked: number
  kept: number
  deleted: number
  bytesFreed: number
  dryRun?: boolean
  warnings: string[]
  [key: string]: unknown
}

export interface LeaseInfo {
  scope: string
  holder: string
  acquiredAt: string
  expiresAt: string
  stolenFrom?: string | null
  [key: string]: unknown
}

/** Map AGENT-1 error prefixes to user-facing hints. Never throws.
 *
 * Covers the full AGENT-1 contract shared by sync + agent transports:
 * `auth:/rate_limited:/not_found:/unsupported:/too_large:/integrity:/
 * network:/disk_full:/conflict:` plus `invalid:/context:/limit:`.
 * Sync call sites use the storage-flavoured hints below; agent call sites
 * prefer {@link agentErrorHint} for LLM-run-flavoured wording of the same
 * prefixes. Both functions recognise every prefix — the split is hint
 * wording (scrub/resize vs COMPACT/retry), not coverage. */
export function describeSyncError(err: string): { prefix: string; hint: string } {
  const prefix = (err.split(':')[0] || '').trim()
  switch (prefix) {
    case 'auth':
      return { prefix, hint: 'Check the provider token / OAuth connection, then retry.' }
    case 'rate_limited':
      return { prefix, hint: 'Provider throttled the request. Wait and retry with backoff.' }
    case 'not_found':
      return { prefix, hint: 'Remote object is gone. Re-run sync or restore from another provider.' }
    case 'unsupported':
      return { prefix, hint: 'This provider cannot do that operation. See docs for per-backend limits.' }
    case 'too_large':
      return { prefix, hint: 'File exceeds the provider limit. Enable striped placement or pick another provider.' }
    case 'integrity':
      return { prefix, hint: 'Checksum mismatch — bytes were not trusted. Run scrub + repair.' }
    case 'network':
      return { prefix, hint: 'Network failure. Retry; the operation is idempotent.' }
    case 'disk_full':
    case 'disk full':
      return { prefix: 'disk_full', hint: 'Volume is full. Attach or resize a disk, then retry.' }
    case 'conflict':
      return { prefix, hint: 'Another writer holds the volume. Resolve the lease or change conflict policy.' }
    case 'invalid':
      return { prefix, hint: 'Request rejected before it ran — fix the field named in the message.' }
    case 'context':
      return { prefix, hint: 'Working set too large for this pass — narrow the scope, then retry.' }
    case 'limit':
      return { prefix, hint: 'Budget or page limit hit — raise the limit or continue from the cursor.' }
    default:
      return { prefix: 'unknown', hint: 'See logs for detail.' }
  }
}

/** Hints for *agent* errors — the sync hints above talk about scrub/resize,
 *  which is nonsense for an LLM run. `context:` is agent-specific: retrying
 *  an overflowed transcript can never work, compacting it can. */
export function agentErrorHint(err: string): { prefix: string; hint: string } {
  const prefix = (err.split(':')[0] || '').trim()
  switch (prefix) {
    case 'auth':
      return { prefix, hint: 'Provider rejected the key — reseal it in SETUP, then retry.' }
    case 'rate_limited':
      return { prefix, hint: 'Provider throttled the call. The harness backs off automatically.' }
    case 'context':
      return { prefix, hint: 'Transcript too large for this model — run COMPACT, then continue.' }
    case 'limit':
      return { prefix, hint: 'Budget hit — turn budget exhausted (raise MAX TURNS) or provider credits empty (top up, or add a fallback assistant).' }
    case 'not_found':
      return { prefix, hint: 'Path or config is gone — re-check WORKING DIR and the file tree.' }
    case 'unsupported':
      return { prefix, hint: 'That tool needs a different transport (device shell / stdio MCP need desktop or dashboard).' }
    case 'too_large':
      return { prefix, hint: 'Input exceeds the tool size cap — narrow the scope or split the file.' }
    case 'integrity':
      return { prefix, hint: 'The file changed under the agent — it should re-read and retry.' }
    case 'conflict':
      return { prefix, hint: 'Ambiguous edit anchor — the agent must resend a larger block.' }
    case 'invalid':
      return { prefix, hint: 'Request rejected before the run — check the model id, that the model supports tool calls, and the field named in the message.' }
    case 'network':
      return { prefix, hint: 'Transport failure. Retry once; auth errors are never retried.' }
    case 'error':
      return { prefix, hint: 'The provider or a tool failed mid-run — see the transcript detail, fix, then retry.' }
    case 'timeout':
      return { prefix, hint: 'The call timed out — narrow the scope or retry; the harness never retries auth failures.' }
    case 'denied':
      return { prefix, hint: 'The run was stopped by a rule or repeated refusal — approve it or change course, never retry identically.' }
    case 'cancelled':
      return { prefix, hint: 'The run was cancelled — resume with a new prompt when ready.' }
    case 'disk_full':
    case 'disk full':
      return { prefix: 'disk_full', hint: 'Volume is full — attach or resize a disk, then retry.' }
    default:
      return { prefix: prefix || 'unknown', hint: 'See the transcript for the machine-prefixed error.' }
  }
}

export const MODULE_METADATA: Record<PanelType, ModuleInfo> = {
  landing: { id: 'landing', label: 'HOME', icon: 'solar:house-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0a0a0a 50%, #000000 100%)', description: 'Quantum-resistant encrypted file manager', requiresAuth: false },
  files: { id: 'files', label: 'FILES', icon: 'solar:folder-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0a0a1a 50%, #000000 100%)', description: 'Browse and manage your encrypted files', requiresAuth: true },
  search: { id: 'search', label: 'SEARCH', icon: 'solar:magnifier-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0a0a0a 50%, #000000 100%)', description: 'Unified search — apps, collections, people, providers, files', requiresAuth: true },
  collections: { id: 'collections', label: 'SEARCH', icon: 'solar:library-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0d0800 50%, #000000 100%)', description: 'Organize (alias of unified search)', requiresAuth: true },
  faces: { id: 'faces', label: 'SEARCH', icon: 'solar:face-scan-circle-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0d0008 50%, #000000 100%)', description: 'People (alias of unified search)', requiresAuth: true },
  map: { id: 'map', label: 'MAP', icon: 'solar:map-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000a0d 50%, #000000 100%)', description: 'GPS-tagged files on MapLibre GL', requiresAuth: true },
  code: { id: 'code', label: 'CODE STUDIO', icon: 'solar:code-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d08 50%, #000000 100%)', description: 'Code Studio — editor + tree-sitter intel (alias of editor)', requiresAuth: true },
  editor: { id: 'editor', label: 'CODE STUDIO', icon: 'solar:file-code-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d08 50%, #000000 100%)', description: 'VSCode-like editor with tree-sitter outline, AI sidecar and terminal', requiresAuth: true },
  sync: { id: 'sync', label: 'SYNC', icon: 'solar:refresh-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #00080d 50%, #000000 100%)', description: 'Multi-backend cloud sync', requiresAuth: true },
  transfer: { id: 'transfer', label: 'TRANSFER', icon: 'solar:share-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d08 50%, #000000 100%)', description: 'Visual file transfers between providers', requiresAuth: true },
  accounts: { id: 'accounts', label: 'ACCOUNTS', icon: 'solar:user-circle-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0a0a0d 50%, #000000 100%)', description: 'Connections, vault file and local users', requiresAuth: true },
  'loose-groups': { id: 'loose-groups', label: 'SEARCH', icon: 'solar:users-group-two-rounded-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0d0d00 50%, #000000 100%)', description: 'Loose groups (alias of unified search)', requiresAuth: true },
  style: { id: 'style', label: 'SEARCH', icon: 'solar:tag-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0a000d 50%, #000000 100%)', description: 'Style tags (alias of unified search)', requiresAuth: true },
  users: { id: 'users', label: 'ACCOUNTS', icon: 'solar:users-group-rounded-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0d0000 50%, #000000 100%)', description: 'Local users (alias of accounts)', requiresAuth: true },
  dashboard: { id: 'dashboard', label: 'REMOTE', icon: 'solar:monitor-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #00080d 50%, #000000 100%)', description: 'Web dashboard and API status', requiresAuth: true },
  webdash: { id: 'webdash', label: 'REMOTE', icon: 'solar:kanban-square-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #05050a 50%, #000000 100%)', description: 'Remote dashboard (alias of dashboard)', requiresAuth: true },
  preview: { id: 'preview', label: 'FILES', icon: 'solar:eye-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0a0a0a 50%, #000000 100%)', description: 'Quick look (alias of files inspector)', requiresAuth: true },
  encryption: { id: 'encryption', label: 'SHIELD', icon: 'solar:lock-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0d0000 50%, #000000 100%)', description: 'File shield — keys, encryption and compression', requiresAuth: true },
  compression: { id: 'compression', label: 'SHIELD', icon: 'solar:archive-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d00 50%, #000000 100%)', description: 'Compression engine (alias of shield)', requiresAuth: true },
  settings: { id: 'settings', label: 'SETTINGS', icon: 'solar:settings-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0a0a0a 50%, #000000 100%)', description: 'Application settings and preferences', requiresAuth: true },
  trash: { id: 'trash', label: 'TRASH', icon: 'solar:trash-bin-trash-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0d0000 50%, #000000 100%)', description: 'Deleted files', requiresAuth: true },
  activity: { id: 'activity', label: 'ACTIVITY', icon: 'solar:pulse-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #00080d 50%, #000000 100%)', description: 'File activity log', requiresAuth: true },
  favorites: { id: 'favorites', label: 'SEARCH', icon: 'solar:star-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0d0800 50%, #000000 100%)', description: 'Favorites (alias of unified search)', requiresAuth: true },
  recent: { id: 'recent', label: 'RECENT', icon: 'solar:history-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #080808 50%, #000000 100%)', description: 'Recently modified files', requiresAuth: true },
  storage: { id: 'storage', label: 'DISKS', icon: 'solar:database-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000a00 50%, #000000 100%)', description: 'Storage overview (alias of disks)', requiresAuth: true },
  terminal: { id: 'terminal', label: 'CYBSH', icon: 'solar:file-terminal-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d08 50%, #000000 100%)', description: 'System terminal — cybsh', requiresAuth: true },
  agent: { id: 'agent', label: 'AGENT', icon: 'solar:bot-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d08 50%, #000000 100%)', description: 'Native AI coding agent', requiresAuth: true },
  processes: { id: 'processes', label: 'TASKS', icon: 'solar:cpu-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d0d 50%, #000000 100%)', description: 'Process table, top and task control', requiresAuth: true },
  cron: { id: 'cron', label: 'SCHEDULES', icon: 'solar:alarm-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d0d 50%, #000000 100%)', description: 'Scheduled scripts (alias of tasks)', requiresAuth: true },
  secrets: { id: 'secrets', label: 'VAULT', icon: 'solar:wallet-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d0d 50%, #000000 100%)', description: 'Passwords, cards, notes and API keys — sealed at rest', requiresAuth: true },
  passwords: { id: 'passwords', label: 'VAULT', icon: 'solar:wallet-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d0d 50%, #000000 100%)', description: 'Passwords (alias of vault)', requiresAuth: true },
  credentials: { id: 'credentials', label: 'VAULT', icon: 'solar:wallet-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d0d 50%, #000000 100%)', description: 'Credentials (alias of vault)', requiresAuth: true },
  automation: { id: 'automation', label: 'SCHEDULES', icon: 'solar:clock-circle-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d0d 50%, #000000 100%)', description: 'Automation schedules (alias of tasks)', requiresAuth: true },
  schedules: { id: 'schedules', label: 'SCHEDULES', icon: 'solar:calendar-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d0d 50%, #000000 100%)', description: 'Cron schedules (alias of tasks)', requiresAuth: true },
  disks: { id: 'disks', label: 'DISKS', icon: 'solar:ssd-square-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000a00 50%, #000000 100%)', description: 'Storage overview, per-provider disks and the merged volume', requiresAuth: true },
  devices: { id: 'devices', label: 'DEVICES', icon: 'solar:plug-circle-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000a0d 50%, #000000 100%)', description: 'Plugged hardware, sensors and browser capabilities', requiresAuth: true },
  permissions: { id: 'permissions', label: 'PERMS', icon: 'solar:key-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #0d0000 50%, #000000 100%)', description: 'Per-file access control', requiresAuth: true },
  install: { id: 'install', label: 'INSTALL', icon: 'solar:download-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d08 50%, #000000 100%)', description: 'Native app + CLI downloads for every platform', requiresAuth: false },
  archive: { id: 'archive', label: 'ARCHIVE', icon: 'solar:notebook-bold', color: '#FFFFFF', gradient: 'linear-gradient(180deg, #000000 0%, #000d08 50%, #000000 100%)', description: 'Life archive: notes, timeline, people, tasks, bookmarks, datasets', requiresAuth: false },
}

// ── OS layer (AGENT-8): cybsh, task table, compute fan-out ───────────────

/** `POST /api/os/exec` — one `cybsh` line in, one rendered answer out. */
export interface ShellResult {
  ok: boolean
  line: string
  output: string
  error?: string
  prompt: string
}

/** One row of the process table (`compute_tasks`). */
export interface OsTask {
  id: number
  /** `sync` · `scrub` · `repair` · `gc` · `compute` · `shell` · `index`. */
  kind: string
  name: string
  state: 'pending' | 'running' | 'done' | 'failed' | 'killed'
  /** 0 … 1. */
  progress: number
  startedAt: string
  endedAt?: string
  bytes: number
  provider: string
  error?: string
}

export interface OsTaskCounts {
  pending: number
  running: number
  done: number
  failed: number
  killed: number
  total: number
}

/** `GET /api/os/ps`. */
export interface OsPs {
  tasks: OsTask[]
  counts: OsTaskCounts
  pid: number
  uptimeMs: number
}

/** `GET /api/os/top` — live system stats over the same table. */
export interface OsTop {
  uptimeMs: number
  load: { load1: number; load5: number; load15: number; source: string }
  mem: { rssBytes: number; sharedBytes: number; totalBytes: number; availableBytes: number }
  cpuPercent: number
  counts: OsTaskCounts
  tasks: OsTask[]
}

/** One provider's contribution to the worker pool. */
export interface OsProviderWorker {
  provider: string
  slots: number
}

/** `GET /api/os/workers` — local rayon slots + provider slots. */
export interface OsWorkers {
  localThreads: number
  providerSlots: number
  total: number
  providers: OsProviderWorker[]
}

/** `GET /api/os/jobs` — the compute catalogue. */
export interface OsJob {
  name: string
  description: string
  takesPath: boolean
}

/** One `.cybermanju` disk as the OS layer sees it (`GET /api/os/df`). */
export interface OsDisk {
  id: string
  name: string
  provider: string
  capacityBytes: number
  state: string
  health: string
  createdAt: string
  usedBytes: number
  compute: number
}

/** `GET /api/os/df` — the merged volume. */
export interface OsVolumeDf {
  totalBytes: number
  usedBytes: number
  freeBytes: number
  root: string
  diskCount: number
  attachedBytes: number
  scratchBytes: number
  disks: OsDisk[]
}

/** One recurring schedule — twin of `cybermanju_types::ScheduleRow`
 * (`GET /api/cron`, `cron_list`). Optional fields are `skip_serializing`
 * on the Rust side, so a row may simply omit them. */
export interface ScheduleRow {
  id: string
  /** Volume path of the `.cybsh` script. */
  path: string
  /** Cron expression (`"30 2 * * *"`) or interval (`"every 10m"`). */
  expr: string
  enabled: boolean
  description?: string | null
  createdAt: string
  lastFiredAt?: string | null
  nextFireAt?: string | null
  lastRunId?: string | null
  /** Fire once on daemon start even if the first cron fire is later. */
  runOnBoot?: boolean
}

/** One fire of a schedule — twin of `cybermanju_types::ScheduleRun`. */
export interface ScheduleRun {
  runId: string
  scheduleId: string
  startedAt: string
  finishedAt: string
  /** `ok` | `error` | `not_found` — same family as sync statuses. */
  status: string
  /** Script output tail, or the AGENT-1-prefixed error. */
  outputTail?: string | null
}

/** Vault secret kind — twin of `cybermanju_types::secrets::SecretKind`. */
export type SecretKind = 'login' | 'card' | 'note' | 'apiKey'

/** Secret metadata — twin of `SecretMeta`. NEVER carries `valueSealed`:
 *  list/get strip it server-side (the `hasKey` contract). Reveal is a
 *  separate call that returns the plaintext once, audit-logged. */
export interface SecretMeta {
  id: string
  kind: SecretKind
  title: string
  username?: string | null
  url?: string | null
  category?: string | null
  tags: string[]
  notes?: string | null
  favorite: boolean
  createdAt: string
  updatedAt: string
  hasValue: boolean
}

/** Create/update payload — `value` is the plaintext, sealed on write and
 *  never echoed back. */
export interface SecretInput {
  id?: string
  kind?: SecretKind
  title: string
  username?: string | null
  url?: string | null
  category?: string | null
  tags?: string[]
  notes?: string | null
  favorite?: boolean
  value?: string | null
}

/** Device identity row — twin of `DeviceRow` (one ML-DSA keypair per
 *  device; the private half never leaves the device). */
export interface DeviceInfo {
  id: string
  name: string
  ownerId?: string
  publicKey?: string
  keyAlgorithm?: string
  createdAt: string
  lastSeenAt: string
  revoked?: boolean
}

/** Server-side todo — twin of `TodoRow` (agent + Archive share one list). */
export interface ServerTodo {
  id: string
  title: string
  done: boolean
  createdAt: string
  doneAt?: string
  ownerId: string
  source: string
}

/** One mDNS-discovered LAN peer (`GET /api/peers`, `list_lan_peers`). */
export interface LanPeer {
  deviceId: string
  name: string
  host: string
  port: number
  lastSeenSecs: number
}

/** One presence heartbeat (`GET /api/peers/known`, `peers_known`) — the
 * server-assisted peer list for targets without mDNS (mobile/docker). */
export interface PresencePeer {
  deviceId: string
  host: string
  port: number
  seenAtSecs: number
}

/** One snapshot file (`backup list` — every transport, `--json`). */
export interface BackupEntry {
  name: string
  bytes: number
}

/** Finished snapshot export (`backup export --json`). */
export interface BackupReport {
  path: string
  bytes: number
  blake3: string
  encrypted: boolean
  pruned: string[]
}

/** Snapshot verification (`backup verify --json`, header + optional deep). */
export interface BackupVerify {
  path: string
  legacy: boolean
  version: number
  encrypted: boolean
  compressed: boolean
  bodyBytes: number
  totalBytes: number
  blake3: string
  imageBytes: number | null
}

/** AGENT-6's disk catalog row (`GET /api/disk/list`). */
export interface DiskRow {
  id: string
  name: string
  provider: string
  configId: string
  volumeUuid: string
  capacityBytes: number
  blockSize: number
  usedBytes: number
  placementOrder: number
  state: string
  health: string
  containerPath: string
  blocksWrittenSinceCheckpoint: number
  /** User-chosen key holder: this disk's passphrase unwraps other disks. */
  holdsKeys?: boolean
  createdAt: string
  updatedAt: string
}

export const SYNC_BACKEND_INFO: Record<SyncBackendType, { name: string; description: string; color: string; icon: string }> = {
  local: {
    name: 'Local Storage',
    description: 'Sync files to a local directory on this machine. Fast, no network required.',
    color: '#FFFFFF',
    icon: 'solar:ssd-square-bold',
  },
  github: {
    name: 'GitHub',
    description: 'Sync files to a GitHub repository using the Contents API. Supports releases for large files.',
    color: '#FFFFFF',
    icon: 'solar:code-square-bold',
  },
  gitlab: {
    name: 'GitLab',
    description: 'Sync files to a GitLab project repository. Full CRUD via GitLab API v4.',
    color: '#FFFFFF',
    icon: 'solar:code-circle-bold',
  },
  googleDrive: {
    name: 'Google Drive',
    description: 'Sync files to Google Drive folders. Full CRUD via Drive API v3.',
    color: '#FFFFFF',
    icon: 'solar:folder-sync-bold',
  },
}

/**
 * Map a frontend `SyncBackendType` to the OAuth route slug the backend
 * understands (`crates/sync/src/oauth.rs::provider_endpoints`).
 * Returns `null` for backends with no OAuth flow (local dir).
 * Google Drive uses the `google` OAuth client.
 */
export function oauthSlugForBackend(backend: SyncBackendType | string): string | null {
  switch (backend) {
    case 'googleDrive':
    case 'google':
      return 'google'
    case 'github':
      return 'github'
    case 'gitlab':
      return 'gitlab'
    default:
      return null
  }
}

/** Backends that can show an "OAuth connect" button. */
export function isOauthCapable(backend: SyncBackendType | string): boolean {
  return oauthSlugForBackend(backend) !== null
}

/* ── AI agent (native core + sidecar-compatible shapes) ─────────────── */

export type LlmDialect = 'openAi' | 'anthropic'
export type AuthScheme = 'bearer' | 'header' | 'query' | 'none'
export type PermissionAction = 'allow' | 'ask' | 'deny'
export type AgentKind = 'build' | 'plan'
/** Which shell the agent's `bash` tool runs (native transports only). */
export type ShellMode = 'auto' | 'cybsh' | 'device'

export interface ProviderPreset {
  id: string
  label: string
  family: string
  baseUrl: string
  defaultModel: string
  dialect: LlmDialect
  auth: AuthScheme
  authName?: string | null
  keyEnv: string
  keyless: boolean
  extraHeaders: Array<[string, string]>
}

export type PermissionRule = PermissionAction | Array<[string, PermissionAction]>

export interface PermissionRuleset {
  default: PermissionAction
  rules: Record<string, PermissionRule>
}

export interface AgentConfig {
  id: string
  name: string
  providerId: string
  model: string
  baseUrlOverride?: string | null
  dialectOverride?: LlmDialect | null
  authSchemeOverride?: AuthScheme | null
  authNameOverride?: string | null
  workingDir: string
  agentKind: AgentKind
  /** Which shell `bash` runs (native only). Missing = auto. */
  shellMode?: ShellMode | null
  permission: PermissionRuleset
  autoApprove: boolean
  maxTurns: number
  hasKey: boolean
  /** Failover route: config ids tried in order when this config's provider call fails. Same-provider ids = extra accounts/keys. */
  fallbackIds?: string[]
  /** Embedding model for semantic memory (`{base}/embeddings`); unset = provider default. */
  embeddingModel?: string | null
  mcpServers?: Record<string, McpServerConfig>
  createdAt: string
  updatedAt: string
}

export interface McpServerConfig {
  transport: string
  command?: string | null
  args: string[]
  env: Record<string, string>
  url?: string | null
  headers: Array<[string, string]>
  enabled: boolean
}

export interface ChatMessage {
  role: string
  content: string
  toolCallId?: string | null
  toolName?: string | null
  toolInput?: unknown
}

export interface ToolCall {
  id: string
  name: string
  input: Record<string, unknown>
}

export interface TokenUsage {
  inputTokens: number
  outputTokens: number
}

export interface AgentSession {
  id: string
  /** Transcript owner (JWT user_id, "local" on desktop); absent on legacy rows. */
  ownerId?: string
  title: string
  configId: string
  providerId: string
  model: string
  agentKind: AgentKind
  workingDir: string
  messages: ChatMessage[]
  usage: TokenUsage
  createdAt: string
  updatedAt: string
}

export interface PendingApproval {
  tool: string
  input: Record<string, unknown>
  summary: string
  question?: string | null
}

export interface AgentJob {
  jobId: string
  sessionId: string
  configId: string
  status: string
  turnsUsed: number
  maxTurns: number
  usage: TokenUsage
  result?: string | null
  error?: string | null
  pending?: PendingApproval | null
  /** Live worker status (`thinking · gpt-5`, `read src/lib.rs`). */
  activity?: string | null
  /** Terminal-state nudge: long run, nothing stored — UI offers REMEMBER. */
  memoryHint?: string | null
  /** One-shot UI request from `ui_open_panel`/`ui_notify`
   *  (consumed-once, deduped by `uiSeq`). Mirrors the Rust
   *  `JobState.ui_request` wire shape; `utils/uiRequests.ts` applies it. */
  uiRequest?: {
    op: 'open' | 'notify'
    panel?: string
    tab?: string
    path?: string
    level?: string
    msg?: string
    seq: number
  } | null
  /** Monotonic seq for `uiRequest` dedupe. */
  uiSeq?: number
}

/* ── Semantic memory (redb `agent_memories`: curated text + vectors) ─── */

export type MemoryOrigin = 'remember' | 'compactHandoff' | 'import'

/** One long-term memory. `embedding` is empty for keyword-only rows. */
export interface AgentMemory {
  id: string
  configId: string
  /** Owner user id (`''` on rows predating ownership — readable as legacy). */
  ownerId?: string
  text: string
  embedding: number[]
  dims: number
  origin: MemoryOrigin
  sessionId?: string | null
  uses: number
  createdAt: string
  updatedAt: string
}

/** One ranked recall hit (text + score, never vectors). */
export interface MemoryHit {
  id: string
  text: string
  score: number
  origin: MemoryOrigin
  sessionId?: string | null
  updatedAt: string
}

/** Built-in permission presets offered by the Agent panel wizard.
 * `yolo` is a true allow-all: every tool (edits, shell, subagents, MCP)
 * runs without asking and `stripDeniedTools` keeps the whole schema.
 * Rails that survive YOLO by design: plan-kind persona denies, protected
 * standing-order writes downgrade to ask, and explicit `deny` rules
 * (none here) always beat auto-approve. */
export const DEFAULT_EXA_MCP_NAME = 'exa'
export const DEFAULT_EXA_MCP_URL = 'https://mcp.exa.ai/mcp'

/** Keyless Exa web-search MCP attached to every new config. */
export function defaultMcpServers(): Record<string, McpServerConfig> {
  return {
    [DEFAULT_EXA_MCP_NAME]: {
      transport: 'http',
      args: [],
      env: {},
      url: DEFAULT_EXA_MCP_URL,
      headers: [],
      enabled: true,
    },
  }
}
export function agentPermissionPreset(name: 'strict' | 'balanced' | 'yolo'): PermissionRuleset {
  if (name === 'strict') {
    return { default: 'ask', rules: {} }
  }
  if (name === 'yolo') {
    return { default: 'allow', rules: {} }
  }
  return {
    default: 'ask',
    rules: {
      read: 'allow',
      list: 'allow',
      grep: 'allow',
      self_research: 'allow',
      repo_analyze: 'allow',
      ui_open_panel: 'allow',
      ui_notify: 'allow',
      secret_list: 'allow',
      bash: [['*', 'ask'], ['git *', 'allow'], ['curl *', 'allow'], ['wget *', 'allow'], ['rm *', 'deny']],
      mcp__exa__web_search_exa: 'allow',
      mcp__exa__web_fetch_exa: 'allow',
    },
  }
}

// ── Android launcher bridge (Gaveta de Apps) ──────────────────────
// Mirrors `crates/launcher` JSON shapes (camelCase). Overrides live in the
// launcher store keyed by `android:<package>` or `os:<panel>`.

export interface AndroidApp {
  packageName: string
  label: string
  activityClass: string
  systemApp: boolean
  /** True for mail/chat/social packages (WhatsApp, Gmail, Instagram, …). */
  socialApp: boolean
  /** `data:image/png;base64,…` 96px raster; "" = render a letter tile. */
  iconBase64: string
}

export interface IconPack {
  packageName: string
  label: string
}

export interface LauncherOverride {
  /** Display alias (rename). Empty = system label. */
  alias: string
  /** Explicit order index; -1 = unordered (label sort). */
  order: number
  /** Gallery/pack PNG data URL replacing the system icon. */
  customIcon: string
  /** Preferred icon-pack package for pack-icon resolution. */
  iconPack: string
  hidden: boolean
}

export function defaultLauncherOverride(): LauncherOverride {
  return { alias: '', order: -1, customIcon: '', iconPack: '', hidden: false }
}

// ── Messages hub (stored notification mirror) ───────────────────
// Mirrors `crates/launcher` JSON shapes (camelCase).

export interface StoredMessage {
  key: string
  packageName: string
  appLabel: string
  title: string
  text: string
  /** Post time, unix millis. */
  timestamp: number
  category: string
}

export interface NotificationState {
  enabled: boolean
  detail: string
}
