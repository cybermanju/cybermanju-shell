// CyberManju OS — Sync (src-tauri/src/commands/sync.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: list_sync_configs, create_sync_config, delete_sync_config, start_sync, get_sync_progress, get_sync_status, test_sync_connection, get_sync_usage.
// Integrates: tauri.
// Contracts: signals: AGENT-, unsupported.
use std::sync::Arc;
use tauri::State;

pub use cybermanju_sync::SyncState;

use crate::sync::models::*;
use crate::AppState;

// ---------------------------------------------------------------------------
// Tauri commands (thin wrappers over the shared sync API)
// ---------------------------------------------------------------------------

/// List all saved sync configurations.
#[tauri::command(async)]
pub fn list_sync_configs(state: State<'_, AppState>) -> Result<Vec<SyncConfig>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    cybermanju_web::api::sync_api::list_configs(&db)
}

/// Create (or save) a sync configuration.
#[tauri::command(async)]
pub fn create_sync_config(
    config: SyncConfig,
    state: State<'_, AppState>,
) -> Result<SyncConfig, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_web::api::sync_api::save_config(&db, config)
}

/// Delete a sync configuration by ID.
#[tauri::command(async)]
pub fn delete_sync_config(config_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_web::api::sync_api::delete_config(&db, &config_id)
}

/// Start a sync operation for the given config and file IDs.
///
/// The shared implementation releases the database lock before the pipeline
/// runs: the pipeline acquires its own locks per file, so holding one here
/// would deadlock.
#[tauri::command(async)]
pub fn start_sync(
    config_id: String,
    file_ids: Vec<String>,
    state: State<'_, AppState>,
    sync_state: State<'_, Arc<SyncState>>,
) -> Result<SyncResult, String> {
    cybermanju_web::api::sync_api::start(
        &state.db,
        &state.compression,
        &sync_state,
        &config_id,
        file_ids,
    )
}

/// Get the current sync progress.
///
/// Also arms the auto-sync scheduler (item 11) — the desktop polls this
/// command, so the first poll starts the background scan exactly like the
/// REST routes do.
#[tauri::command(async)]
pub fn get_sync_progress(
    sync_state: State<'_, Arc<SyncState>>,
    state: State<'_, AppState>,
) -> Result<SyncProgress, String> {
    cybermanju_sync::scheduler::ensure_started(Arc::clone(&state.db));
    cybermanju_os::scheduler::ensure_started(Arc::clone(&state.db));
    Ok(cybermanju_web::api::sync_api::progress(&sync_state))
}

/// Get the latest sync status summary (the same shape as GET /api/sync/status).
#[tauri::command(async)]
pub fn get_sync_status(
    sync_state: State<'_, Arc<SyncState>>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    cybermanju_sync::scheduler::ensure_started(Arc::clone(&state.db));
    cybermanju_os::scheduler::ensure_started(Arc::clone(&state.db));
    let progress = cybermanju_web::api::sync_api::latest_progress(&sync_state);
    let provider = cybermanju_sync::state::RunRegistry::global()
        .latest()
        .map(|run| run.config_id.clone());
    Ok(serde_json::json!({
        "syncEnabled": true,
        "status": progress.status,
        "lastSync": progress.started_at,
        "provider": provider,
    }))
}

/// Test the connection for a sync configuration.
#[tauri::command(async)]
pub fn test_sync_connection(config: SyncConfig) -> Result<bool, String> {
    cybermanju_web::api::sync_api::test_connection(&config)
}

/// Provider quota/usage for a config (the IPC twin of
/// `GET /api/sync/usage/:id`). Pure function of the saved config — no
/// dashboard needed, so Android gets real quota instead of a localhost
/// connection refusal. (`oauth_start` stays REST-only: it mints dashboard
/// redirect URLs and needs the server's OAuth client.)
#[tauri::command(async)]
pub fn get_sync_usage(
    config_id: String,
    state: State<'_, AppState>,
) -> Result<cybermanju_sync::QuotaUsage, String> {
    let config = {
        let db = state.db.read().map_err(|e| e.to_string())?;
        cybermanju_web::api::sync_api::get_config(&db, &config_id)?
    };
    cybermanju_sync::quota_usage(&config)
}

/// Cancel the current sync operation.
#[tauri::command(async)]
pub fn cancel_sync(sync_state: State<'_, Arc<SyncState>>) -> Result<bool, String> {
    Ok(cybermanju_web::api::sync_api::cancel(&sync_state))
}

/// List files on the remote backend.
#[tauri::command(async)]
pub fn list_remote_files(config: SyncConfig, prefix: String) -> Result<Vec<RemoteFile>, String> {
    cybermanju_web::api::sync_api::list_remote_files(&config, &prefix)
}

// ---------------------------------------------------------------------------
// <<< AGENT-2 RESTORE / JOBS: the desktop half of the item-7 contract. >>>
// ---------------------------------------------------------------------------

/// Restore a synced copy: download → verify → decrypt/decompress → write.
#[tauri::command(async)]
pub fn restore_sync_file(
    config_id: String,
    file_id: Option<String>,
    remote_path: Option<String>,
    dest_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<cybermanju_web::api::sync_api::RestoreOutcome, String> {
    cybermanju_web::api::sync_api::restore(
        &state.db,
        cybermanju_web::api::sync_api::RestoreRequest {
            config_id,
            file_id,
            remote_path,
            dest_path,
        },
    )
}

/// Delete a remote object (`unsupported:` providers surface as an error,
/// never a fake success).
#[tauri::command(async)]
pub fn delete_remote_file(
    config_id: String,
    remote_path: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    cybermanju_web::api::sync_api::delete_remote(
        &state.db,
        cybermanju_web::api::sync_api::RemoteDeleteRequest {
            config_id,
            remote_path,
        },
    )
}

/// Status/progress/result of one run by id (memory, then `sync_runs`).
#[tauri::command(async)]
pub fn get_sync_job(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<cybermanju_web::api::sync_api::SyncJob, String> {
    cybermanju_web::api::sync_api::job(&state.db, &job_id)
}

/// Run history (newest first) for the sync history UI.
#[tauri::command(async)]
pub fn list_sync_runs(state: State<'_, AppState>) -> Result<Vec<SyncRunRecord>, String> {
    cybermanju_web::api::sync_api::runs(&state.db, 20)
}

/// Create a new private vault repository on GitHub/GitLab.
///
/// Resolves the provider token from the explicit paste or from the stored
/// secret of `configId`, then calls the provider "create repo" endpoint.
/// Returns what the frontend stores in `SyncConfig.repo_name`.
// Flat arity is the Tauri IPC + REST contract (store + `useTauri` pass one
// JSON object per arg); the body funnels everything into `CreateRepoRequest`.
#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub fn create_provider_repo(
    backend_type: String,
    config_id: Option<String>,
    token: Option<String>,
    name: String,
    private: Option<bool>,
    description: Option<String>,
    branch: Option<String>,
    base_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<cybermanju_sync::CreatedRepo, String> {
    cybermanju_web::api::sync_api::create_repo(
        &state.db,
        cybermanju_web::api::sync_api::CreateRepoRequest {
            backend_type,
            config_id,
            token,
            name,
            private: private.unwrap_or(true),
            description,
            branch,
            base_path,
        },
    )
}

/// Seed a vault repo with bootstrap files (README, manifest, exported
/// `.cybermanju` bytes as base64) through the provider sync backend.
#[tauri::command(async)]
pub fn seed_repo_files(
    config: SyncConfig,
    files: Vec<cybermanju_web::api::sync_api::SeedFile>,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let _ = state;
    cybermanju_web::api::sync_api::seed_repo(cybermanju_web::api::sync_api::SeedRepoRequest {
        config,
        files,
    })
}

/// Relocate one file's single copy from provider A to provider B:
/// download A → upload B → verify B → delete A → retarget record.
#[tauri::command(async)]
pub fn move_sync_file(
    file_id: String,
    from_config_id: String,
    to_config_id: String,
    state: State<'_, AppState>,
) -> Result<cybermanju_web::api::sync_api::MoveOutcome, String> {
    cybermanju_web::api::sync_api::move_file(
        &state.db,
        cybermanju_web::api::sync_api::MoveRequest {
            file_id,
            from_config_id,
            to_config_id,
        },
    )
}

/// Write one file's bytes (base64) to a provider remote path — the VFS
/// write-through, so provider mounts are read-write on desktop too.
#[tauri::command(async)]
pub fn upload_remote_file(
    config: SyncConfig,
    remote_path: String,
    content_base64: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let _ = state;
    cybermanju_web::api::sync_api::upload_bytes(cybermanju_web::api::sync_api::UploadRequest {
        config,
        remote_path,
        content_base64,
    })
}
