// CyberManju OS — Sync (crates/types/src/sync.rs).
// Role: shared schemas: files, sync, agent, schedules.
// Exports: SyncBackendType, SyncStatus, ConflictPolicy, PlacementMode, CloudAccount, OAuthCredentials, is_expired, SyncFile.
// Integrates: serde.
// Contracts: signals: AGENT-, conflict, not_found.
use serde::{Deserialize, Serialize};

/// Sync destination kind: local disk image or a cloud provider backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum SyncBackendType {
    Local,
    GitHub,
    GitLab,
    GoogleDrive,
}

impl std::fmt::Display for SyncBackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::GitHub => write!(f, "github"),
            Self::GitLab => write!(f, "gitlab"),
            Self::GoogleDrive => write!(f, "googleDrive"),
        }
    }
}

// <<< AGENT-2 STATUS COLLAPSE: one generation of terminal/active states.
// The old enum carried two generations at once — `Done` *and* `Completed`
// (same meaning), `Syncing` *and* the phase statuses (a blip that was always
// overwritten by `Scanning` microseconds later). Canonical set:
//   idle → scanning → {compressing|uploading|linking|cleaning} → completed
//        | error | cancelled
// Legacy payloads written by older builds ("done", "syncing") still
// deserialize via serde aliases; serialization only ever emits the
// canonical camelCase names. >>>
/// Async sync-job lifecycle: idle → scanning → {compressing|uploading|
/// linking|cleaning} → completed, or error / cancelled. Polled via
/// `GET /api/sync/jobs/{jobId}` after the 202 start.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum SyncStatus {
    Idle,
    #[serde(alias = "syncing")]
    Scanning,
    Compressing,
    Uploading,
    Linking,
    Cleaning,
    Error,
    #[serde(alias = "done")]
    Completed,
    Cancelled,
}

impl std::fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Scanning => write!(f, "scanning"),
            Self::Compressing => write!(f, "compressing"),
            Self::Uploading => write!(f, "uploading"),
            Self::Linking => write!(f, "linking"),
            Self::Cleaning => write!(f, "cleaning"),
            Self::Error => write!(f, "error"),
            Self::Completed => write!(f, "completed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// What to do when a re-sync finds the remote locator already populated
/// with content we did not just write. `skip` keeps remote bytes safe by
/// default; the pipeline reports every applied conflict through
/// `SyncResult.errors` with a `conflict:` prefix.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConflictPolicy {
    #[default]
    Skip,
    Overwrite,
    KeepBoth,
}

/// Where a file's bytes are placed. `whole` uploads one artifact to one
/// provider (today's behaviour); `striped` splits the file into BLAKE3-
/// addressed chunks round-robined across ≥2 enabled configs
/// (`crates/sync/src/manifest.rs`).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PlacementMode {
    #[default]
    Whole,
    Striped,
}

/// Cloud provider account; tokens never serialize (AGENT-3 secrets rule).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudAccount {
    pub id: String,
    pub name: String,
    pub backend_type: SyncBackendType,
    // <<< AGENT-3 SECRETS: never serialize a provider token to a client >>>
    #[serde(skip_serializing)]
    pub token: Option<String>,
    pub oauth_credentials: Option<OAuthCredentials>,
    pub config: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

// <<< AGENT-3 SECRETS: `access_token`, `refresh_token` and `client_secret`
// are omitted from every serialized form (GET /api/sync/configs and the
// whole-object `SyncConfig`/`CloudAccount` responses). `default` keeps the
// struct literal round-trip working when the key is absent. >>>
/// OAuth token set; secrets individually `skip_serializing`, never leave the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthCredentials {
    #[serde(skip_serializing, default)]
    pub access_token: String,
    #[serde(skip_serializing)]
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub client_id: String,
    #[serde(skip_serializing)]
    pub client_secret: Option<String>,
}

impl OAuthCredentials {
    /// Whether the access token expired, or expires within `buffer_seconds`.
    pub fn is_expired(&self, buffer_seconds: u64) -> bool {
        match self.expires_at {
            Some(expires) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                now + buffer_seconds >= expires
            }
            // No expiry information published — assume it is still valid.
            None => false,
        }
    }
}

/// One synced copy of a local file, persisted in the `sync_files` table.
///
/// Key: `{file_id}/{config_id}` — the same file may live on several
/// providers. `remote_path` is the durable locator the pipeline uploaded
/// to (item 2: without it restore and remote delete are impossible);
/// `remote_url` is what `upload_file` returned (a provider locator such as
/// a Drive file URL) which some backends need for downloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFile {
    pub id: String,
    pub config_id: Option<String>,
    /// Preferred home provider for single-copy placement (`sync_configs.id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_config_id: Option<String>,
    pub original_path: String,
    pub compressed_path: Option<String>,
    pub preview_path: Option<String>,
    pub remote_url: Option<String>,
    pub remote_path: Option<String>,
    pub size_bytes: u64,
    pub compressed_size_bytes: Option<u64>,
    /// BLAKE3 of the original plaintext — the restore-time verify baseline.
    pub hash_blake3: Option<String>,
    /// BLAKE3 of the exact bytes that were uploaded (ciphertext when
    /// encryption was on) — the download-back verify baseline.
    pub artifact_hash: Option<String>,
    /// Chunk manifest reference for `placement: striped` (item 10).
    pub manifest_ref: Option<String>,
    /// Set when a download-back verification succeeded.
    pub last_verified_at: Option<String>,
    /// Keystore key handle used for `encrypted` artifacts (never the key).
    pub key_handle: Option<String>,
    pub compressed: Option<bool>,
    pub encrypted: Option<bool>,
    pub backend_type: SyncBackendType,
    pub synced_at: Option<String>,
    pub status: SyncStatus,
    pub error_message: Option<String>,
}

/// One finished sync run, persisted in the `sync_runs` table so the UI can
/// show history (item 14) and `GET /api/sync/jobs/{id}` survives a restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRunRecord {
    pub run_id: String,
    pub config_id: String,
    /// JWT `user_id` that started the run (`None` = legacy row written
    /// before per-user jobs; those stay visible to any authenticated
    /// caller, new rows are owner-checked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    pub started_at: String,
    pub finished_at: String,
    pub status: SyncStatus,
    pub files_synced: u32,
    pub bytes_uploaded: u64,
    pub errors: Vec<String>,
    /// Final progress snapshot at terminal state.
    pub progress: SyncProgress,
    pub result: Option<SyncResult>,
}

/// Provider sync profile: placement, encryption, conflict policy and OAuth.
/// One config per provider; files pick a home config (single-copy default).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfig {
    pub id: String,
    pub backend_type: SyncBackendType,
    pub enabled: bool,
    pub account_id: Option<String>,
    pub name: Option<String>,
    pub base_path: Option<String>,
    pub repo_name: Option<String>,
    pub branch: Option<String>,
    // <<< AGENT-3 SECRETS: provider tokens must never reach a client.
    // Deserialization is unaffected, so `POST /api/sync/configs` still
    // accepts a raw token in the body. >>>
    #[serde(skip_serializing)]
    pub token: Option<String>,
    pub folder_id: Option<String>,
    pub auto_sync: bool,
    pub compress_before_upload: bool,
    pub create_previews: bool,
    pub delete_raw_after_sync: bool,
    pub max_concurrent_uploads: u32,
    // <<< AGENT-2 CONTRACT FIELDS >>>
    // All three carry serde defaults so configs written before these fields
    // existed keep parsing. Encryption defaults **on**: a local artifact is
    // cheap to protect, and plaintext uploads to third parties are the
    // failure mode this field exists to end.
    #[serde(default = "default_encrypt_before_upload")]
    pub encrypt_before_upload: bool,
    #[serde(default)]
    pub conflict_policy: ConflictPolicy,
    #[serde(default)]
    pub placement: PlacementMode,
    // <<< AGENT-2 item 10: replicas per chunk in `striped` mode (not
    // Reed–Solomon — simple replication: chunk i is uploaded to the next
    // `min(parity, n-1)` configs in the plan). Defaults to 1 so opting into
    // striping without a knob still tolerates one provider loss per chunk;
    // set 0 for pure round-robin (no redundancy). Ignored in `whole` mode. >>>
    #[serde(default = "default_parity")]
    pub parity: u8,
    // <<< FULL-IMPROVE: encrypt-or-fail + basename obfuscation >>>
    // `require_encryption`: refuse the upload when no master passphrase is
    // available instead of uploading WITHOUT encryption with a warning.
    // Defaults off (warning path preserved); turn it on for vault sets.
    #[serde(default)]
    pub require_encryption: bool,
    // `obfuscate_names`: hash file basenames into the remote locator so
    // providers see `cybermanju_sync/<dir8>/<hash16>` instead of real names.
    // Deterministic (idempotent re-syncs converge); the original name stays
    // in the local `sync_files` record for restore. Defaults off.
    #[serde(default)]
    pub obfuscate_names: bool,
    // Unified-disk placement: `mirror` opts a provider back into the legacy
    // duplicate-everywhere behaviour. Default false = single-copy home: a
    // file lives on its home provider only, moves are explicit relocations.
    #[serde(default)]
    pub mirror: bool,
    // User-chosen key holder: this provider's `.cybermanju` unwraps the keys
    // for the other disks. Display + policy only — crypto still verifies.
    #[serde(default)]
    pub key_holder: bool,
    // <<< AGENT-3 CONTRACT: filled/refreshed by the OAuth flow. Secrets
    // inside `OAuthCredentials` are individually `skip_serializing`. >>>
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_credentials: Option<OAuthCredentials>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

fn default_encrypt_before_upload() -> bool {
    true
}

fn default_parity() -> u8 {
    1
}

/// Live counters for a running sync job; polled until a terminal [`SyncStatus`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    pub total_files: u32,
    pub processed_files: u32,
    pub current_file: Option<String>,
    pub status: SyncStatus,
    pub bytes_uploaded: u64,
    pub errors: Vec<String>,
    pub started_at: Option<String>,
    pub estimated_remaining_seconds: Option<f64>,
}

/// Terminal outcome of a sync run; per-file failures carry `conflict:` etc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub files_synced: u32,
    pub bytes_uploaded: u64,
    pub bytes_saved_by_compression: u64,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

/// One remote listing row; `path` is provider-relative for pipeline reuse.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFile {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified_at: String,
    pub url: String,
}

// <<< AGENT-1 CAPABILITIES: static per-provider limits/feature flags.
// `Default` is deliberately permissive (no fixed size cap, everything
// supported except chunking) so an unimplemented backend reports *unknown*
// rather than lying about a restriction it does not have. >>>
/// Static per-provider limits and feature flags; `Default` is permissive.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// Largest single file the backend accepts; `u64::MAX` = no fixed cap.
    pub max_size_bytes: u64,
    pub supports_delete: bool,
    /// A direct (token-free) download URL is available from `get_file_url`.
    pub supports_direct_download: bool,
    /// `list_files` returns every descendant under the prefix, not only
    /// the immediate children.
    pub recursive_list: bool,
    /// `upload_file_chunked` is implemented (resumable/multipart upload).
    pub chunked: bool,
    // <<< AGENT-8 COMPUTE: concurrent compute slots this provider offers.
    // "More providers → more processing" (MISSING.md E2) is only
    // representable if the model can score compute, so the scheduler sums
    // this across the attached pool and adds it to the local rayon slots.
    // `#[serde(default)]` keeps stored/older JSON deserializing. >>>
    /// Concurrent compute slots the provider offers to the fan-out
    /// scheduler (0 = storage only, no compute contribution).
    #[serde(default)]
    pub compute: u32,
    // <<< /AGENT-8 COMPUTE >>>
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            max_size_bytes: u64::MAX,
            supports_delete: true,
            supports_direct_download: true,
            recursive_list: true,
            chunked: false,
            // <<< AGENT-8 COMPUTE: a backend that has not reported a value
            // still contributes one slot — attaching it must never *reduce*
            // parallelism. >>>
            compute: 1,
            // <<< /AGENT-8 COMPUTE >>>
        }
    }
}

/// Object-safe provider surface: upload/download/delete/list plus defaulted
/// capability, stat, chunked-upload and dir-listing extensions.
pub trait StorageBackend: Send + Sync {
    fn name(&self) -> &str;
    fn backend_type(&self) -> SyncBackendType;
    fn upload_file(&self, local_path: &str, remote_path: &str) -> Result<String, String>;
    fn download_file(&self, remote_path: &str, local_path: &str) -> Result<(), String>;
    fn delete_file(&self, remote_path: &str) -> Result<(), String>;
    fn list_files(&self, prefix: &str) -> Result<Vec<RemoteFile>, String>;
    fn get_file_url(&self, remote_path: &str) -> Result<String, String>;
    fn test_connection(&self) -> Result<bool, String>;

    // <<< AGENT-1 TRAIT EXTENSION: defaulted so the 8 existing methods and
    // every current implementor (AGENT-2's pipeline, AGENT-5's UI) keep
    // compiling untouched. >>>

    /// Static capability report for this provider.
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    /// Best-effort metadata for one remote path.
    /// `Ok(None)` means "unknown" — the caller treats it as "no conflict
    /// information", never as "file missing". A path that is confirmed
    /// missing must be `Err("not_found: …")`.
    fn stat(&self, _remote_path: &str) -> Result<Option<RemoteFile>, String> {
        Ok(None)
    }

    /// Resumable / chunked upload for backends that support it.
    fn upload_file_chunked(&self, _local_path: &str, _remote_path: &str) -> Result<String, String> {
        Err("unsupported: chunked upload is not implemented for this backend".to_string())
    }

    /// Immediate subdirectories of `prefix` (one level, no recursion).
    /// Returned rows carry the human `name` plus the provider-relative
    /// `path` a sibling `list_files` entry would use. The default is empty:
    /// git-backed listings synthesize folders from recursive blob paths, so
    /// only backends whose file listing hides folders (Google Drive)
    /// override this.
    fn list_dirs(&self, _prefix: &str) -> Result<Vec<RemoteFile>, String> {
        Ok(Vec::new())
    }
}
