// CyberManju OS — Sync configuration + run lifecycle (shared by Tauri IPC and REST)
//
// <<< AGENT-2 JOBS: `POST /api/sync/start` no longer runs the pipeline on a
// request thread. `start_job` registers a run and returns `202 {jobId}`
// immediately; the pipeline executes on a worker thread, its progress is
// polled through `job`/`latest_progress`, and terminal results are mirrored
// into the `sync_runs` history table. The blocking `start` remains for the
// Tauri transport, which already owns its own command thread — it registers
// the same run, so both transports share ids, progress and cancel. >>>

use std::fs;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::{Arc, RwLock};

use cybermanju_compression::TripleCompressor;
use cybermanju_crypto::keystore;
use cybermanju_db::Database;
use cybermanju_sync::manifest;
use cybermanju_sync::pipeline::CYBE_MAGIC;
use cybermanju_sync::state::{RunRegistry, SyncRun};
use cybermanju_sync::{
    create_backend, quota_usage, QuotaUsage, SyncPipeline, SyncState, SyncStatus,
};
use cybermanju_types::sync::{
    RemoteFile, SyncConfig, SyncFile, SyncProgress, SyncResult, SyncRunRecord,
};
use log::error;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

// ─── Request wire types ──────────────────────────────────────────────

/// `POST /api/sync/start` body.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRequest {
    pub config_id: String,
    pub file_ids: Vec<String>,
}

/// Body carrying a full `SyncConfig` (create / test / remote listing).
#[derive(Debug, Deserialize)]
pub struct ConfigRequest {
    pub config: SyncConfig,
}

/// `POST /api/sync/remote-files` body.
#[derive(Debug, Deserialize)]
pub struct RemoteFilesRequest {
    pub config: SyncConfig,
    #[serde(default)]
    pub prefix: String,
}

/// `POST /api/sync/cancel` body — every field optional so an empty body
/// stays a valid "cancel whatever is running".
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelRequest {
    #[serde(default)]
    pub job_id: Option<String>,
}

/// `POST /api/sync/restore` body: `fileId | remotePath` selects the copy,
/// `destPath` defaults to the recorded original path.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreRequest {
    pub config_id: String,
    #[serde(default)]
    pub file_id: Option<String>,
    #[serde(default)]
    pub remote_path: Option<String>,
    #[serde(default)]
    pub dest_path: Option<String>,
}

/// `DELETE /api/sync/remote` body.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDeleteRequest {
    pub config_id: String,
    pub remote_path: String,
}

/// `POST /api/sync/move` body: relocate one file's single copy from
/// provider A to provider B (unified-disk move, not copy+orphan).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRequest {
    pub file_id: String,
    pub from_config_id: String,
    pub to_config_id: String,
}

/// Successful `POST /api/sync/move` response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveOutcome {
    pub file_id: String,
    pub from_config_id: String,
    pub to_config_id: String,
    pub remote_path: String,
    pub bytes: u64,
    pub noop: bool,
}

/// `202` body of `POST /api/sync/start` and payload of `GET /api/sync/jobs/{id}`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncJob {
    pub job_id: String,
    pub config_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: SyncStatus,
    pub progress: SyncProgress,
    pub result: Option<SyncResult>,
}

/// Successful `POST /api/sync/restore` response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOutcome {
    pub path: String,
    pub bytes: u64,
    /// Restored bytes matched the stored plaintext BLAKE3.
    pub verified: bool,
}

// ─── Config CRUD ─────────────────────────────────────────────────────

/// List all saved sync configurations.
///
/// Provider tokens live in the `sync_secrets` side table (the config row
/// never serializes them — AGENT-3's `skip_serializing`), and are merged
/// back here so the backends still see credentials.
pub fn list_configs(db: &Database) -> Result<Vec<SyncConfig>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_sync_configs_table())
        .map_err(|e| e.to_string())?;

    let mut configs = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let mut config: SyncConfig =
            serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        if config.token.is_none() {
            config.token = db.get_sync_secret(&config.id).map_err(|e| e.to_string())?;
        }
        configs.push(config);
    }

    Ok(configs)
}

/// Create (or overwrite) a sync configuration. Generates an ID when absent.
pub fn save_config(db: &Database, config: SyncConfig) -> Result<SyncConfig, String> {
    let config_id = if config.id.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        config.id.clone()
    };
    // AGENT-3 request 6: validate caller-supplied identifiers before they
    // become database keys.
    crate::security::validate_id(&config_id)?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut config = config;
    config.id = config_id.clone();
    if config.created_at.is_none() {
        config.created_at = Some(now.clone());
    }
    config.updated_at = Some(now);

    let incoming_token = config.token.clone();
    let serialized = serde_json::to_string(&config).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_sync_configs_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(config_id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    // The token never reaches the JSON row; it goes to the side table.
    // An absent token on update leaves any stored secret untouched.
    if let Some(token) = incoming_token {
        db.put_sync_secret(&config_id, &token)
            .map_err(|e| e.to_string())?;
    }

    // Single key-holder invariant: designating this provider clears the flag
    // on every other provider (best-effort, never fails the save itself).
    if config.key_holder {
        if let Ok(all) = list_configs(db) {
            for other in all
                .into_iter()
                .filter(|c| c.id != config_id && c.key_holder)
            {
                let mut cleared = other.clone();
                cleared.key_holder = false;
                cleared.updated_at = Some(chrono::Utc::now().to_rfc3339());
                if let Ok(serialized) = serde_json::to_string(&cleared) {
                    if let Ok(tx) = db.begin_write() {
                        let done = (|| {
                            {
                                let mut table = tx
                                    .open_table(Database::get_sync_configs_table())
                                    .map_err(|e| e.to_string())?;
                                table
                                    .insert(cleared.id.as_str(), serialized.as_str())
                                    .map_err(|e| e.to_string())?;
                            }
                            tx.commit().map_err(|e| e.to_string())
                        })();
                        let _ = done;
                    }
                }
            }
        }
    }

    Ok(config)
}

/// Delete a sync configuration by ID (row + its stored secret).
pub fn delete_config(db: &Database, config_id: &str) -> Result<bool, String> {
    crate::security::validate_id(config_id)?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_sync_configs_table())
            .map_err(|e| e.to_string())?;
        let removed = table
            .remove(config_id)
            .map_err(|e| e.to_string())?
            .is_some();
        if !removed {
            return Err(format!("not_found: sync config '{}' not found", config_id));
        }
        let mut secrets = tx
            .open_table(Database::get_sync_secrets_table())
            .map_err(|e| e.to_string())?;
        let _ = secrets.remove(config_id).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(true)
}

/// Load a single sync configuration by ID.
pub fn get_config(db: &Database, config_id: &str) -> Result<SyncConfig, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_sync_configs_table())
        .map_err(|e| e.to_string())?;
    let value = table
        .get(config_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("not_found: sync config '{}' not found", config_id))?;
    let mut config: SyncConfig = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
    if config.token.is_none() {
        config.token = db.get_sync_secret(config_id).map_err(|e| e.to_string())?;
    }
    Ok(config)
}

/// Test a configuration's remote connectivity.
pub fn test_connection(config: &SyncConfig) -> Result<bool, String> {
    let backend = create_backend(config)?;
    backend.test_connection()
}

/// List files stored on the remote backend for a configuration.
pub fn list_remote_files(config: &SyncConfig, prefix: &str) -> Result<Vec<RemoteFile>, String> {
    let backend = create_backend(config)?;
    backend.list_files(prefix)
}

// ─── Private vault repo provisioning (GitHub + GitLab) ─────────────

/// `POST /api/sync/create-repo` body. When `configId` is set the stored
/// provider token is used unless `token` is pasted explicitly; otherwise
/// `token` is required. `name` accepts `my-vault` or `owner/my-vault`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRepoRequest {
    pub backend_type: String,
    #[serde(default)]
    pub config_id: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    pub name: String,
    #[serde(default = "default_private")]
    pub private: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub base_path: Option<String>,
}

fn default_private() -> bool {
    true
}

/// Create a new private vault repository on GitHub/GitLab and return what
/// the frontend stores in `SyncConfig.repo_name` (+ branch + URL).
pub fn create_repo(
    db: &RwLock<Database>,
    req: CreateRepoRequest,
) -> Result<cybermanju_sync::CreatedRepo, String> {
    if req.name.trim().is_empty() {
        return Err("unsupported: repo name must not be empty".to_string());
    }
    // Resolve the token: explicit paste wins, then the stored secret of an
    // existing provider config (so OAuth-connected providers need no paste).
    let mut token = req.token.clone().unwrap_or_default();
    if token.trim().is_empty() {
        if let Some(config_id) = req.config_id.as_deref().filter(|s| !s.trim().is_empty()) {
            crate::security::validate_id(config_id)?;
            let db = db.read().map_err(|e| e.to_string())?;
            let config = get_config(&db, config_id)?;
            token = cybermanju_sync::oauth::resolve_token(&config).unwrap_or_default();
            if token.trim().is_empty() {
                let secret = db
                    .get_sync_secret(config_id)
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
                token = secret;
            }
        }
    }
    if token.trim().is_empty() {
        return Err(
            "auth: repo creation needs a token — paste a PAT or connect with OAuth first"
                .to_string(),
        );
    }
    cybermanju_sync::create_repository(&cybermanju_sync::CreateRepoInput {
        backend: req.backend_type,
        token,
        name: req.name,
        private: req.private,
        description: req.description.unwrap_or_default(),
        branch: req.branch.unwrap_or_else(|| "main".to_string()),
        base_url: req.base_path,
    })
}

/// One text/binary seed file for a fresh vault repo.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedFile {
    pub path: String,
    pub content_base64: String,
}

/// `POST /api/sync/seed-repo` body — writes small bootstrap files
/// (README, `cybermanju.json` manifest, the exported `vault.cybermanju`
/// bytes as base64) into an existing provider repo via its sync backend.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedRepoRequest {
    pub config: SyncConfig,
    pub files: Vec<SeedFile>,
}

/// Seed a vault repo with bootstrap files. Each file is base64-decoded to a
/// temp file and uploaded through the provider backend (Contents API on
/// GitHub, files API on GitLab). Caps: 8 files, 5 MiB each — the vault
/// container itself is seeded the same way by the frontend export step.
pub fn seed_repo(req: SeedRepoRequest) -> Result<Vec<String>, String> {
    if req.files.is_empty() {
        return Err("unsupported: seed needs at least one file".to_string());
    }
    if req.files.len() > 8 {
        return Err("too_large: seed holds at most 8 files".to_string());
    }
    let backend = create_backend(&req.config)?;
    let dir = std::env::temp_dir().join(format!("cyb-seed-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).map_err(|e| format!("network: cannot stage seed files: {}", e))?;
    let mut urls = Vec::with_capacity(req.files.len());
    for file in &req.files {
        let clean = file.path.trim().trim_matches('/').to_string();
        if clean.is_empty() || clean.split('/').any(|s| s == "..") || clean.len() > 256 {
            let _ = fs::remove_dir_all(&dir);
            return Err(format!("unsupported: seed path '{}' is invalid", file.path));
        }
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            file.content_base64.trim(),
        )
        .map_err(|e| format!("unsupported: seed file '{}' is not base64: {}", clean, e))?;
        if bytes.len() > 5 * 1024 * 1024 {
            let _ = fs::remove_dir_all(&dir);
            return Err(format!("too_large: seed file '{}' exceeds 5 MiB", clean));
        }
        let stage = dir.join(format!("seed-{}", urls.len()));
        fs::write(&stage, &bytes)
            .map_err(|e| format!("network: cannot stage seed file '{}': {}", clean, e))?;
        match backend.upload_file(stage.to_string_lossy().as_ref(), &clean) {
            Ok(url) => urls.push(url),
            Err(e) => {
                let _ = fs::remove_dir_all(&dir);
                return Err(e);
            }
        }
    }
    let _ = fs::remove_dir_all(&dir);
    Ok(urls)
}

/// `POST /api/sync/upload` body — one file's bytes (base64) written to a
/// provider remote path through its sync backend. This is the VFS
/// write-through: provider mounts are no longer read-only.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadRequest {
    pub config: SyncConfig,
    pub remote_path: String,
    pub content_base64: String,
}

/// Write raw bytes to `remote_path` on the provider. Same 5 MiB cap and
/// path validation as the seeder; the bytes travel as-is (callers pass
/// already-encrypted artifacts when the config demands it).
pub fn upload_bytes(req: UploadRequest) -> Result<String, String> {
    let clean = req.remote_path.trim().trim_matches('/').to_string();
    if clean.is_empty() || clean.split('/').any(|s| s == "..") || clean.len() > 256 {
        return Err(format!(
            "unsupported: upload path '{}' is invalid",
            req.remote_path
        ));
    }
    let bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        req.content_base64.trim(),
    )
    .map_err(|e| format!("unsupported: upload body is not base64: {}", e))?;
    if bytes.len() > 5 * 1024 * 1024 {
        return Err(format!("too_large: upload of '{}' exceeds 5 MiB", clean));
    }
    let backend = create_backend(&req.config)?;
    let dir = std::env::temp_dir().join(format!("cyb-upload-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).map_err(|e| format!("network: cannot stage upload: {}", e))?;
    let stage = dir.join("upload.bin");
    fs::write(&stage, &bytes).map_err(|e| format!("network: cannot stage upload: {}", e))?;
    let out = backend.upload_file(stage.to_string_lossy().as_ref(), &clean);
    let _ = fs::remove_dir_all(&dir);
    out
}

// ─── Progress & cancel ───────────────────────────────────────────────

/// Current sync progress snapshot (lockless).
pub fn progress(sync_state: &Arc<SyncState>) -> SyncProgress {
    sync_state.snapshot()
}

/// Progress of the most recent registered run; falls back to the shared
/// state when nothing has run yet. This is what `GET /api/sync/progress`
/// serves so legacy pollers keep working across runs.
/// Trusted-local entry point (no owner check).
pub fn latest_progress(fallback: &Arc<SyncState>) -> SyncProgress {
    match RunRegistry::global().latest() {
        Some(run) => run.state.snapshot(),
        None => fallback.snapshot(),
    }
}

/// Owner-bound variant of [`latest_progress`] (P0-10): non-admins read
/// their own latest run's progress, never another user's `current_file`.
pub fn latest_progress_for(
    fallback: &Arc<SyncState>,
    requester: &str,
    is_admin: bool,
) -> SyncProgress {
    if requester.is_empty() || is_admin {
        return latest_progress(fallback);
    }
    match RunRegistry::global().latest_for(requester, false) {
        Some(run) => run.state.snapshot(),
        None => fallback.snapshot(),
    }
}

/// Request cancellation of the current sync (lockless) — Tauri transport.
pub fn cancel(sync_state: &Arc<SyncState>) -> bool {
    // Sets the flag the pipeline polls on every file, and flips the reported
    // status to `cancelled`.
    sync_state.cancel();
    true
}

/// Cancel a specific run by id, or the latest run when `job_id` is `None`.
/// Unknown ids answer `false`; "nothing running" answers `true` (idempotent).
/// Trusted-local entry point (no owner check).
pub fn cancel_job(job_id: Option<&str>) -> bool {
    RunRegistry::global().cancel(job_id)
}

/// Owner-bound cancel for the REST transport (P0-10): explicit ids cancel
/// only the caller's own run (admins: any); `None` cancels the caller's
/// latest own run instead of the global latest.
pub fn cancel_job_as(job_id: Option<&str>, requester: &str, is_admin: bool) -> bool {
    RunRegistry::global().cancel_as(job_id, requester, is_admin)
}

// ─── Run lifecycle ───────────────────────────────────────────────────

/// Load a config and make sure a run against it may start.
fn load_enabled_config(db: &RwLock<Database>, config_id: &str) -> Result<SyncConfig, String> {
    crate::security::validate_id(config_id)?;
    let db = db.read().map_err(|e| e.to_string())?;
    let config = get_config(&db, config_id)?;
    if !config.enabled {
        return Err(format!("Sync config '{}' is not enabled", config_id));
    }
    Ok(config)
}

fn validate_file_ids(file_ids: &[String]) -> Result<(), String> {
    for id in file_ids {
        crate::security::validate_id(id)?;
    }
    Ok(())
}

/// Expand folder ids into their descendant file ids (breadth-first through
/// the parent index), so "sync this folder to provider X" just works.
/// Unknown ids pass through untouched (the pipeline reports them honestly);
/// cycles are impossible via the `seen` set. Caps at 5000 files — beyond
/// that the caller should sync in batches (`too_large:`).
fn expand_sync_ids(db: &Database, file_ids: &[String]) -> Result<Vec<String>, String> {
    const MAX_EXPANDED: usize = 5000;
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stack: Vec<String> = file_ids.to_vec();
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let is_folder = db
            .get_file_node(&id)
            .ok()
            .flatten()
            .is_some_and(|node| node.file_type == "folder");
        if is_folder {
            let children = db.list_by_parent(&id).map_err(|e| e.to_string())?;
            stack.extend(children);
        } else {
            out.push(id);
        }
        if out.len() + stack.len() > MAX_EXPANDED {
            return Err(format!(
                "too_large: folder expansion exceeds {} files — sync in smaller batches",
                MAX_EXPANDED
            ));
        }
    }
    Ok(out)
}

fn job_snapshot(run: &SyncRun) -> SyncJob {
    let progress = run.state.snapshot();
    let outcome = run.outcome();
    SyncJob {
        job_id: run.run_id.clone(),
        config_id: run.config_id.clone(),
        started_at: run.started_at.clone(),
        finished_at: outcome.as_ref().map(|o| o.finished_at.clone()),
        status: progress.status.clone(),
        progress,
        result: outcome.and_then(|o| o.result),
    }
}

/// Run the pipeline for an already-registered run, then record the outcome
/// in the registry and the `sync_runs` history.
///
/// A history failure is logged, never returned — the run itself succeeded
/// and pollers already have its result.
fn execute_run(
    db: &RwLock<Database>,
    compression: &TripleCompressor,
    run: &Arc<SyncRun>,
    config: SyncConfig,
    file_ids: Vec<String>,
) -> Result<SyncResult, String> {
    let pipeline = SyncPipeline::new(config, Arc::clone(&run.state));
    let outcome = pipeline.sync_all(file_ids, db, compression);

    let (result, error) = match outcome {
        Ok(result) => {
            if run.state.is_cancelled() {
                run.state.set_status(SyncStatus::Cancelled);
            } else {
                run.state.set_status(SyncStatus::Completed);
                run.state.set_current(None);
                let total = run.state.snapshot().total_files;
                run.state.set_processed(total);
            }
            (Some(result), None)
        }
        Err(e) => {
            run.state.set_status(SyncStatus::Error);
            run.state.add_error(e.clone());
            (None, Some(e))
        }
    };

    // Registry first — pollers must see the terminal state immediately.
    RunRegistry::global().finish(run, result.clone(), error.clone());

    // Then durable history (item 14).
    let progress = run.state.snapshot();
    let record = SyncRunRecord {
        run_id: run.run_id.clone(),
        config_id: run.config_id.clone(),
        owner_id: Some(run.owner_id.clone()),
        started_at: run.started_at.clone(),
        finished_at: run
            .outcome()
            .map(|o| o.finished_at)
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        status: progress.status.clone(),
        files_synced: result.as_ref().map(|r| r.files_synced).unwrap_or(0),
        bytes_uploaded: result.as_ref().map(|r| r.bytes_uploaded).unwrap_or(0),
        errors: progress.errors.clone(),
        progress,
        result: result.clone(),
    };
    {
        let db = db.read().map_err(|e| e.to_string())?;
        if let Err(e) = db.save_sync_run(&record) {
            error!("could not persist sync run {}: {}", run.run_id, e);
        }
    }

    match result {
        Some(result) => Ok(result),
        None => Err(error.unwrap_or_else(|| "sync run failed".to_string())),
    }
}

/// Start a sync run on a worker thread; returns as soon as the run is
/// registered (the REST `202 {jobId}` path).
///
/// Trusted-local entry point: the run is owned by `"local"`. REST callers
/// must use [`start_job_for`] with the JWT `user_id` (P0-10).
pub fn start_job(
    db: &Arc<RwLock<Database>>,
    config_id: &str,
    file_ids: Vec<String>,
) -> Result<SyncJob, String> {
    start_job_for(db, config_id, file_ids, "local")
}

/// Owner-bound variant of [`start_job`] for the REST transport.
pub fn start_job_for(
    db: &Arc<RwLock<Database>>,
    config_id: &str,
    file_ids: Vec<String>,
    owner_id: &str,
) -> Result<SyncJob, String> {
    // Validate everything while the caller is still on the request thread:
    // a bad config must be a 4xx, not a job that fails in the dark.
    let config = load_enabled_config(db, config_id)?;
    validate_file_ids(&file_ids)?;
    // Folders expand to their descendant files (short read lock, released
    // before the worker thread starts).
    let file_ids = {
        let guard = db.read().map_err(|e| e.to_string())?;
        expand_sync_ids(&guard, &file_ids)?
    };

    let run = RunRegistry::global().begin_owned(
        config_id,
        Arc::new(SyncState::new()),
        file_ids.len() as u32,
        owner_id,
    )?;

    let db2 = Arc::clone(db);
    let run2 = Arc::clone(&run);
    let spawned = std::thread::Builder::new()
        .name(format!("sync-{}", run.run_id))
        .spawn(move || {
            let compression = TripleCompressor::new();
            let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
                execute_run(&db2, &compression, &run2, config, file_ids)
            }));
            if let Err(_panic) = outcome {
                // A panicking worker must not leave pollers stuck on a
                // status that never becomes terminal.
                run2.state.set_status(SyncStatus::Error);
                run2.state.add_error("sync worker panicked".to_string());
                RunRegistry::global().finish(&run2, None, Some("sync worker panicked".to_string()));
            }
        })
        .map_err(|e| {
            // Registration already happened — close the run so it can never
            // sit as "active" with nothing driving it.
            RunRegistry::global().finish(
                &run,
                None,
                Some(format!("could not start sync worker: {}", e)),
            );
            format!("could not start sync worker: {}", e)
        })?;
    drop(spawned);

    Ok(job_snapshot(&run))
}

/// Start a sync run for `config_id` **on the calling thread** (Tauri path).
///
/// The run is registered like any REST job — same ids, same history, same
/// scoped cancel — it just executes inline. The database lock is released
/// before the pipeline runs: the pipeline acquires its own lock per file,
/// so holding one here would deadlock.
pub fn start(
    db: &RwLock<Database>,
    compression: &TripleCompressor,
    sync_state: &Arc<SyncState>,
    config_id: &str,
    file_ids: Vec<String>,
) -> Result<SyncResult, String> {
    let config = load_enabled_config(db, config_id)?;
    validate_file_ids(&file_ids)?;
    // Same folder → files expansion as the REST path (short read lock;
    // the pipeline takes its own per-file locks afterwards).
    let file_ids = {
        let guard = db.read().map_err(|e| e.to_string())?;
        expand_sync_ids(&guard, &file_ids)?
    };

    let run =
        RunRegistry::global().begin(config_id, Arc::clone(sync_state), file_ids.len() as u32)?;
    // `begin` armed the state (fresh cancel flag, reset progress, status
    // `scanning`) — no explicit reset here, that used to clear cancels.
    execute_run(db, compression, &run, config, file_ids)
}

/// Run status/progress/result by id — memory first, then `sync_runs`
/// history (so a job id survives both eviction and restarts).
/// Trusted-local entry point (no owner check).
pub fn job(db: &RwLock<Database>, job_id: &str) -> Result<SyncJob, String> {
    job_for(db, job_id, "", true)
}

/// Owner-bound poll for the REST transport (P0-10): live runs are checked
/// with [`RunRegistry::get_for`], history rows with an owner are checked
/// against the caller (admins bypass both). Legacy history rows without an
/// owner stay visible to any authenticated caller. Unknown ids and
/// other-users' runs are indistinguishable (`not_found:`).
pub fn job_for(
    db: &RwLock<Database>,
    job_id: &str,
    requester: &str,
    is_admin: bool,
) -> Result<SyncJob, String> {
    crate::security::validate_id(job_id)?;
    if let Some(run) = RunRegistry::global().get_for(job_id, requester, is_admin) {
        return Ok(job_snapshot(&run));
    }
    // No live run visible to this caller — fall through to history only
    // when the id is not claimed by someone else's live run (avoid a
    // history oracle for live foreign ids).
    if !requester.is_empty() && !is_admin && RunRegistry::global().get(job_id).is_some() {
        return Err(format!("not_found: sync job '{job_id}' not found"));
    }
    let db = db.read().map_err(|e| e.to_string())?;
    let record = db
        .get_sync_run(job_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            // P0-21: evicted (or history-pruned) ids answer `expired:`, never
            // `not_found:` — the job existed, its poll window passed. Unknown
            // ids stay `not_found:`.
            if RunRegistry::global().was_evicted(job_id) {
                format!(
                    "expired: sync job '{job_id}' left the live registry — \
                     poll the latest run or start a new one"
                )
            } else {
                format!("not_found: sync job '{job_id}' not found")
            }
        })?;
    if !requester.is_empty() && !is_admin {
        if let Some(owner) = record.owner_id.as_deref() {
            if owner != requester {
                return Err(format!("not_found: sync job '{job_id}' not found"));
            }
        }
    }
    Ok(SyncJob {
        job_id: record.run_id,
        config_id: record.config_id,
        started_at: record.started_at,
        finished_at: Some(record.finished_at),
        status: record.status,
        progress: record.progress,
        result: record.result,
    })
}

/// Run history, newest first (item 14). Trusted-local (Tauri): all rows.
pub fn runs(db: &RwLock<Database>, limit: usize) -> Result<Vec<SyncRunRecord>, String> {
    let db = db.read().map_err(|e| e.to_string())?;
    db.list_sync_runs(limit.clamp(1, 50))
        .map_err(|e| e.to_string())
}

/// Owner-bound history for the REST transport (P0-10): admins see all
/// rows, everyone else sees only their own runs plus legacy rows without
/// an owner. Empty requester (trusted local) sees everything.
pub fn runs_for(
    db: &RwLock<Database>,
    limit: usize,
    requester: &str,
    is_admin: bool,
) -> Result<Vec<SyncRunRecord>, String> {
    let rows = runs(db, limit)?;
    if requester.is_empty() || is_admin {
        return Ok(rows);
    }
    Ok(rows
        .into_iter()
        .filter(|r| {
            r.owner_id
                .as_deref()
                .map(|o| o == requester)
                .unwrap_or(true)
        })
        .collect())
}

// ─── Restore & remote delete (item 7) ────────────────────────────────

/// Download a remote copy, verify it against the stored plaintext hash,
/// decrypt/decompress as recorded, and write it locally.
pub fn restore(db: &RwLock<Database>, req: RestoreRequest) -> Result<RestoreOutcome, String> {
    crate::security::validate_id(&req.config_id)?;
    if let Some(file_id) = &req.file_id {
        crate::security::validate_id(file_id)?;
    }

    let config = {
        let db = db.read().map_err(|e| e.to_string())?;
        get_config(&db, &req.config_id)?
    };

    // Resolve locator candidates + the default destination.
    let (candidates, record, default_dest) = {
        let db = db.read().map_err(|e| e.to_string())?;
        if let Some(file_id) = &req.file_id {
            let record: SyncFile = db
                .get_sync_file(file_id, &req.config_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("not_found: sync record not found for file {}", file_id))?;
            let mut candidates: Vec<String> = Vec::new();
            // Stored path first (works for Local/GitHub/Drive/GitLab), then
            // the provider locator upload returned (e.g. a Drive file URL)
            // — tried in order until one downloads.
            if let Some(path) = &record.remote_path {
                candidates.push(path.clone());
            }
            if let Some(url) = &record.remote_url {
                if !candidates.contains(url) {
                    candidates.push(url.clone());
                }
            }
            if candidates.is_empty() && record.manifest_ref.is_none() {
                // Striped records legitimately have no single locator — the
                // manifest branch below reassembles them instead.
                return Err("not_found: sync record has no remote locator".to_string());
            }
            let dest = record.original_path.clone();
            (candidates, Some(record), Some(dest))
        } else {
            let remote = req
                .remote_path
                .clone()
                .filter(|p| !p.trim().is_empty())
                .ok_or_else(|| "unsupported: remotePath or fileId is required".to_string())?;
            (vec![remote], None, None)
        }
    };

    let dest = match (&req.dest_path, &default_dest) {
        (Some(dest), _) => dest.clone(),
        (None, Some(dest)) => dest.clone(),
        (None, None) => return Err("destPath is required when restoring by remotePath".to_string()),
    };
    // Destination hygiene: NUL bytes never reach the filesystem, and `..`
    // segments must not escape the destination's own parent directory
    // (recorded original paths are server-truth and unaffected; explicit
    // `destPath` values are caller-controlled on multi-user dashboards).
    if dest.contains('\0') {
        return Err("unsupported: restore destination contains a NUL byte".to_string());
    }
    if dest.split('/').any(|segment| segment == "..")
        || dest.split('\\').any(|segment| segment == "..")
    {
        return Err(format!(
            "unsupported: restore destination '{}' escapes its directory",
            dest
        ));
    }

    // Striped placement (item 10): the manifest — not a single locator —
    // says where the pieces live. Reassembly downloads, verifies every
    // chunk's plaintext hash, verifies the whole file, and only then
    // publishes `dest` (never a half-written destination).
    if let Some(manifest_json) = record.as_ref().and_then(|r| r.manifest_ref.clone()) {
        let manifest: manifest::ChunkManifest = serde_json::from_str(&manifest_json)
            .map_err(|e| format!("integrity: stored manifest is malformed: {}", e))?;
        let participants = {
            let db = db.read().map_err(|e| e.to_string())?;
            db.list_sync_configs().map_err(|e| e.to_string())?
        };
        let bytes = manifest::restore(&manifest, &participants, &dest)?;
        if let Some(mut record) = record {
            record.last_verified_at = Some(chrono::Utc::now().to_rfc3339());
            record.status = SyncStatus::Completed;
            let db = db.write().map_err(|e| e.to_string())?;
            db.upsert_sync_file(&record).map_err(|e| e.to_string())?;
        }
        return Ok(RestoreOutcome {
            path: dest,
            bytes,
            verified: true,
        });
    }

    let backend = create_backend(&config)?;

    // Download to a sidecar file; only a fully transformed, verified
    // payload touches the destination path.
    let part = format!("{}.restore.part", dest);
    let mut last_err: Option<String> = None;
    let mut downloaded = false;
    for candidate in &candidates {
        match backend.download_file(candidate, &part) {
            Ok(()) => {
                downloaded = true;
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }
    if !downloaded {
        let _ = fs::remove_file(&part);
        return Err(
            last_err.unwrap_or_else(|| "not_found: no remote copy could be downloaded".to_string())
        );
    }

    let mut bytes =
        fs::read(&part).map_err(|e| format!("integrity: restore read failed: {}", e))?;
    let _ = fs::remove_file(&part);

    // Decrypt first (magic prefix), then decompress.
    if bytes.starts_with(CYBE_MAGIC) {
        let passphrase = keystore::master_passphrase().ok_or_else(|| {
            "integrity: artifact is encrypted but no master passphrase is available".to_string()
        })?;
        bytes = keystore::open_sealed(&passphrase, &bytes[CYBE_MAGIC.len()..])
            .map_err(|e| format!("integrity: restore could not decrypt: {}", e))?;
    }

    let expected = record.as_ref().and_then(|r| r.hash_blake3.clone());
    let compressor = TripleCompressor::new();
    let mut verified = false;
    match expected {
        Some(expected) => {
            if cybermanju_sync::blake3_hex(&bytes) == expected {
                verified = true;
            } else {
                // Must be a compressed artifact — decompress, then hold the
                // plaintext against the stored baseline.
                let (plain, _size) = compressor.decompress_triple(&bytes).map_err(|e| {
                    format!(
                        "integrity: restored bytes do not match the stored hash and \
                             are not a compressed artifact: {}",
                        e
                    )
                })?;
                if cybermanju_sync::blake3_hex(&plain) != expected {
                    return Err(
                        "integrity: restored plaintext does not match the stored hash".to_string(),
                    );
                }
                bytes = plain;
                verified = true;
            }
        }
        None => {
            // No local baseline (restore by remotePath) — best effort: keep
            // the bytes as they are unless they are a compressed artifact.
            if let Ok((plain, _size)) = compressor.decompress_triple(&bytes) {
                bytes = plain;
            }
        }
    }

    if let Some(parent) = Path::new(&dest).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "integrity: restore could not create '{}': {}",
                    parent.display(),
                    e
                )
            })?;
        }
    }
    fs::write(&dest, &bytes).map_err(|e| format!("integrity: restore write failed: {}", e))?;

    // Stamp the locator record so the next verification starts fresh.
    if let Some(mut record) = record {
        if verified {
            record.last_verified_at = Some(chrono::Utc::now().to_rfc3339());
            record.status = SyncStatus::Completed;
        }
        let db = db.write().map_err(|e| e.to_string())?;
        db.upsert_sync_file(&record).map_err(|e| e.to_string())?;
    }

    Ok(RestoreOutcome {
        path: dest,
        bytes: bytes.len() as u64,
        verified,
    })
}

/// Delete a remote object through AGENT-1's backend.
///
/// Honest 501s: a backend that answers `unsupported: …` is surfaced as-is
/// and mapped to HTTP 501 by the route. Any locator record pointing at the
/// deleted path is dropped so restore can never chase a ghost.
pub fn delete_remote(db: &RwLock<Database>, req: RemoteDeleteRequest) -> Result<bool, String> {
    crate::security::validate_id(&req.config_id)?;
    if req.remote_path.trim().is_empty() {
        return Err("remotePath is required".to_string());
    }

    let config = {
        let db = db.read().map_err(|e| e.to_string())?;
        get_config(&db, &req.config_id)?
    };
    let backend = create_backend(&config)?;
    backend.delete_file(&req.remote_path)?;

    let db = db.write().map_err(|e| e.to_string())?;
    if let Some(record) = db
        .find_sync_file_by_remote(&req.config_id, &req.remote_path)
        .map_err(|e| e.to_string())?
    {
        if let Some(config_id) = &record.config_id {
            db.remove_sync_file(&record.id, config_id)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(true)
}

// ─── Unified-disk move A→B (single-copy relocation) ────────────────

/// Move one file's single copy from provider A to provider B.
///
/// Bytes travel as-is (the sealed `.cyb3` artifact is backend-agnostic —
/// no recompress, no re-encrypt), verified by artifact BLAKE3 on the way
/// back. Order: download A → upload B → verify B → delete A → retarget
/// the `sync_files` record (`config_id` + `home_config_id`). A failed
/// upload leaves A untouched; a failed record update compensates by
/// removing the orphaned B copy.
pub fn move_file(db: &RwLock<Database>, req: MoveRequest) -> Result<MoveOutcome, String> {
    // Clone the handle under a short read, then run lock-free: the verified
    // transfer does network I/O and must never hold the request lock.
    let owned = db.read().map_err(|e| e.to_string())?.clone();
    move_file_on_db(&owned, req)
}

/// `&Database` core so `cybsh sync move` (which holds no request lock)
/// shares the exact verified relocation as the REST/Tauri path.
pub fn move_file_on_db(db: &Database, req: MoveRequest) -> Result<MoveOutcome, String> {
    // Validate before touching the shared core (same `validate_id` contract).
    crate::security::validate_id(&req.file_id)?;
    crate::security::validate_id(&req.from_config_id)?;
    crate::security::validate_id(&req.to_config_id)?;
    if req.from_config_id == req.to_config_id {
        let remote = {
            db.get_sync_file(&req.file_id, &req.from_config_id)
                .map_err(|e| e.to_string())?
                .and_then(|r| r.remote_path.clone())
                .unwrap_or_default()
        };
        return Ok(MoveOutcome {
            file_id: req.file_id,
            from_config_id: req.from_config_id,
            to_config_id: req.to_config_id,
            remote_path: remote,
            bytes: 0,
            noop: true,
        });
    }
    // Delegate bytes to the shared verified core; map its outcome back.
    relocate_into_move_outcome(db, req)
}

/// Shared verified relocation (`cybermanju_sync::relocate`) mapped onto the
/// REST wire shape. The duplicate noop above keeps the id contract local.
fn relocate_into_move_outcome(db: &Database, req: MoveRequest) -> Result<MoveOutcome, String> {
    let out = cybermanju_sync::relocate(db, &req.file_id, &req.from_config_id, &req.to_config_id)?;
    Ok(MoveOutcome {
        file_id: out.file_id,
        from_config_id: out.from_config_id,
        to_config_id: out.to_config_id,
        remote_path: out.remote_path,
        bytes: out.bytes,
        noop: out.noop,
    })
}

/// Provider quota/usage for a config (wraps AGENT-1's `quota::usage`).
pub fn usage(db: &RwLock<Database>, config_id: &str) -> Result<QuotaUsage, String> {
    crate::security::validate_id(config_id)?;
    let config = {
        let db = db.read().map_err(|e| e.to_string())?;
        get_config(&db, config_id)?
    };
    quota_usage(&config)
}
