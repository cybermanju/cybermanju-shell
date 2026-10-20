// CyberManju OS — Core Library
// Orchestrates redb, ML-KEM PQC (pqcrypto-mlkem), Tantivy, Tree-sitter, triple compression, face clustering

pub mod commands;
pub mod compression;
pub mod crypto;
pub mod db;
pub mod faces; // ML module: detect_faces_in_file, embedding_distance, cluster_embeddings
pub mod preview;
pub mod search;
pub mod sync;
pub mod tree_sitter; // parse_file, get_symbols (tauri commands)
pub mod web_dashboard;

use commands::faces as face_cmd;
use commands::sync as sync_cmd;
use commands::{
    accounts, audit, batch, collections, dashboard, devices, encryption, files,
    import as import_cmd, map, search as search_cmd, share, todos, trash, users, versions,
};
use db::Database;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{ipc::Invoke, Manager, Runtime};

/// Shared desktop state: one redb handle + one Tantivy index for IPC and REST.
pub struct AppState {
    /// Shared with the web dashboard so the redb file is only opened once.
    pub db: Arc<RwLock<Database>>,
    /// Shared with the web dashboard so REST search sees live index updates.
    pub tantivy_index: Arc<RwLock<search::SearchIndex>>,
    pub compression: compression::TripleCompressor,
    pub hmac_secret: [u8; 32],
}

// WebDashboard now handles its own shutdown (Drop impl with signal channel + thread join).

/// Fail startup with a loud message. On desktop this exits the process;
/// on mobile (`cfg(mobile)`, i.e. the Android build) it panics so the crash
/// lands in logcat with a backtrace instead of a silent process death.
fn fatal(msg: &str) -> ! {
    // Marker-prefixed AND single-line: the reason must survive a
    // `grep "CyberManju OS FATAL"` triage window (2026-10-08: a startup
    // refusal's message landed on following lines and was filtered out,
    // leaving only the bare `lib.rs:43` breadcrumb).
    tracing::error!("CyberManju OS FATAL: {}", msg);
    #[cfg(mobile)]
    {
        panic!("{}", msg);
    }
    #[cfg(not(mobile))]
    {
        std::process::exit(1);
    }
}

/// Android app-private files dir, multi-user safe.
///
/// `Context.getFilesDir()` is per-user (`/data/user/<id>/<pkg>/files`) but
/// native code has no Context, so a hardcoded `/data/data/<pkg>/files`
/// (a symlink to `/data/user/0/...`) breaks under work profiles / secondary
/// users with EACCES at first launch — an instant close before first paint.
/// Every candidate below is proven writable (mkdir + probe file) before it
/// is returned, so a bad guess is skipped instead of bricking startup.
#[cfg(target_os = "android")]
fn probe_android_files_dir() -> std::path::PathBuf {
    const PKG: &str = "com.cybermanju.os";
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(d) = std::env::var("CYBERMANJU_ANDROID_FILES_DIR") {
        if !d.trim().is_empty() {
            candidates.push(std::path::PathBuf::from(d));
        }
    }
    if let Some(dir) = cybermanju_web::security::default_secret_dir() {
        if !candidates.contains(&dir) {
            candidates.push(dir);
        }
    }
    // Per-user real dirs (secondary users / work profiles). Listing
    // /data/user may be denied — then this just contributes nothing.
    if let Ok(entries) = std::fs::read_dir("/data/user") {
        let mut ids: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.is_empty() && n.as_bytes().iter().all(u8::is_ascii_digit))
            .collect();
        ids.sort();
        for id in ids {
            let p = std::path::PathBuf::from(format!("/data/user/{id}/{PKG}/files"));
            if !candidates.contains(&p) {
                candidates.push(p);
            }
        }
    }
    for c in &candidates {
        if dir_is_writable(c) {
            return c.clone();
        }
        tracing::warn!(
            "Android files-dir candidate not writable, skipping: {}",
            c.display()
        );
    }
    tracing::error!(
        "no writable Android files dir found; falling back to the canonical path (startup may fail)"
    );
    std::path::PathBuf::from(format!("/data/data/{PKG}/files"))
}

/// mkdir + write/delete a probe file. Pure check, no state kept.
#[cfg(target_os = "android")]
fn dir_is_writable(dir: &std::path::Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".cyb_write_probe");
    if std::fs::write(&probe, b"ok").is_err() {
        return false;
    }
    let _ = std::fs::remove_file(&probe);
    true
}

/// Resolve the desktop database path without depending on the process CWD.
///
/// Order: `DB_PATH` env (Docker/server convention) → shared platform data
/// dir (`CYBERMANJU_DATA_DIR` → XDG/`~/.local/share/cybermanju-os`, see
/// `cybermanju_web::security::default_secret_dir`) → process CWD fallback.
/// AppImage/Flatpak launches must never write into the read-only bundle
/// mount, so a relative path is only the last resort.
///
/// Android: there is no CWD and scoped storage limits writes to the
/// app-private files dir. The dir is probed for writability
/// (`probe_android_files_dir`, multi-user safe) instead of trusting one
/// hardcoded path; `DB_PATH` still wins when set.
fn resolve_desktop_db_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("DB_PATH") {
        if !p.trim().is_empty() {
            return std::path::PathBuf::from(p);
        }
    }
    #[cfg(target_os = "android")]
    {
        return probe_android_files_dir().join("cybermanju.db");
    }
    #[cfg(not(target_os = "android"))]
    {
        let dir = cybermanju_web::security::default_secret_dir().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        });
        dir.join("cybermanju.db")
    }
}

/// The Tantivy index lives next to the database so one backup covers both
/// (same convention as `docker/server`: `SEARCH_INDEX_PATH` env or
/// `<db parent>/tantivy_index`).
fn resolve_desktop_index_path(db_path: &std::path::Path) -> std::path::PathBuf {
    if let Ok(p) = std::env::var("SEARCH_INDEX_PATH") {
        if !p.trim().is_empty() {
            return std::path::PathBuf::from(p);
        }
    }
    db_path
        .parent()
        .map(|p| p.join("tantivy_index"))
        .unwrap_or_else(|| std::path::PathBuf::from("tantivy_index"))
}

/// Open the redb database, quarantining a corrupt file instead of bricking.
///
/// A half-written redb file (killed mid-commit, disk full, bad restore)
/// fails to open on EVERY launch. The old code turned that into `fatal()`
/// (panic on mobile) before the window ever showed — the permanent
/// "installs but instantly closes" brick. Now the corrupt file is renamed
/// to `cybermanju.corrupt-<unix_ts>.bak` beside the DB and a fresh vault
/// is created; the backup stays on disk for manual recovery.
fn open_database(db_path: &std::path::Path) -> Database {
    let db_path_str = db_path.to_string_lossy();
    match Database::new(&db_path_str) {
        Ok(d) => d,
        Err(first) => {
            tracing::error!(
                "database open failed at {}: {first} — quarantining the file and recreating",
                db_path.display()
            );
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup = db_path.with_file_name(format!("cybermanju.corrupt-{stamp}.bak"));
            if let Err(e) = std::fs::rename(db_path, &backup) {
                fatal(&format!(
                    "Failed to initialize redb database at {} ({first}); quarantine of the corrupt file also failed: {e}",
                    db_path.display()
                ));
            }
            tracing::warn!("moved corrupt database to {}", backup.display());
            match Database::new(&db_path_str) {
                Ok(d) => d,
                Err(second) => fatal(&format!(
                    "Failed to initialize redb database at {} even after quarantining the corrupt file: {second}",
                    db_path.display()
                )),
            }
        }
    }
}

/// Open the Tantivy index, rebuilding a corrupt directory instead of bricking.
///
/// Same story as the database: a torn index (killed mid-commit) fails to
/// open on every launch. The index is fully rebuildable from the DB
/// (`rebuild_search_index`), so wipe + recreate is safe; the old code
/// `fatal()`-panicked here before first paint on mobile.
fn open_search_index(index_path: &std::path::Path) -> search::SearchIndex {
    let index_path_str = index_path.to_string_lossy();
    match search::SearchIndex::new(&index_path_str) {
        Ok(i) => i,
        Err(first) => {
            tracing::error!(
                "search index open failed at {}: {first} — wiping and recreating (rebuildable via rebuild_search_index)",
                index_path.display()
            );
            if let Err(e) = std::fs::remove_dir_all(index_path) {
                fatal(&format!(
                    "Failed to initialize Tantivy at {} ({first}); wiping the corrupt index also failed: {e}",
                    index_path.display()
                ));
            }
            match search::SearchIndex::new(&index_path_str) {
                Ok(i) => i,
                Err(second) => fatal(&format!(
                    "Failed to initialize Tantivy at {} even after wiping the corrupt index: {second}",
                    index_path.display()
                )),
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Desktop entry point: build state, start the dashboard bridge, run Tauri.
pub fn run() {
    // Breadcrumb: on mobile a panic before first paint is otherwise a silent
    // "auto-close". The payload goes to stderr (visible in `adb logcat`)
    // before the default hook runs.
    #[cfg(mobile)]
    {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Flattened to one line so the location + payload survive the
            // same `grep "CyberManju OS FATAL"` triage window as above.
            eprintln!(
                "CyberManju OS FATAL (panic): {}",
                info.to_string().replace('\n', " | ")
            );
            default(info);
        }));
    }

    // Logging must never panic the process: a second run() (Android activity
    // recreate) or an invalid RUST_LOG value falls back to a sane default.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    if tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .is_err()
    {
        eprintln!("CyberManju OS: global tracing subscriber already set; continuing");
    }
    tracing::info!("CyberManju OS starting...");

    // Storage + dashboard init lives in the `.setup()` hook below, NOT here.
    // The OS delivers a cybermanju:// OAuth return by spawning a SECOND
    // process: single-instance (first plugin) hands its argv to the running
    // app and exits that process during plugin setup. Everything above this
    // point is side-effect free (stderr logging only); if the database were
    // opened here, the second process would hit the first instance's
    // exclusive redb lock and quarantine the LIVE vault before exiting.
    // The dashboard Arc is ferried out via a holder for the clean shutdown
    // after `.run()` returns.
    // Android: publish ONE probed app-private files dir to every data-dir
    // consumer (keystore master passphrase, sync OAuth sealing, disk, OS
    // volume root, JWT secret). `HOME` is unset in Android app processes,
    // so without this they all resolve None and fail closed on device —
    // sync OAuth refuses with "No master passphrase available to encrypt
    // OAuth credentials". The probe writes only its own probe file (never
    // the DB), so the second-process invariant above still holds. No-op when
    // an operator already exported CYBERMANJU_DATA_DIR.
    #[cfg(target_os = "android")]
    {
        let already = std::env::var("CYBERMANJU_DATA_DIR").unwrap_or_default();
        if already.trim().is_empty() {
            let dir = probe_android_files_dir();
            tracing::info!("Android data dir: {}", dir.display());
            std::env::set_var("CYBERMANJU_DATA_DIR", &dir);
        }
    }
    let dashboard_holder: Arc<Mutex<Option<Arc<web_dashboard::WebDashboard>>>> =
        Arc::new(Mutex::new(None));
    let dashboard_holder_setup = Arc::clone(&dashboard_holder);

    // NOTE (2026-10-08): tauri-plugin-log was removed. It calls
    // `log::set_logger` at run(), which collides with our own
    // `tracing_subscriber::fmt().try_init()` above AND with a second run()
    // in the same process (Android activity recreate) — both surfaced as
    // `PluginInitialization("log", "attempted to set a logger after the
    // logging system was already initialized")` + `.expect()` panic at the
    // old lib.rs:527 before first paint. The frontend never imported
    // `@tauri-apps/plugin-log`; Rust logs go to stderr (logcat) via
    // tracing, so the plugin bought nothing.
    // OAuth deep links on Windows/Linux arrive as a second-process CLI
    // argument — single-instance (with the `deep-link` feature) forwards them
    // to the running app so the frontend `onOpenUrl` listener fires instead
    // of booting a stray second window that would exchange PKCE in the wrong
    // process. Must be first for the same reason.
    let mut builder = tauri::Builder::default();
    #[cfg(not(mobile))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {}));
    }
    builder
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_scoped_storage::init())
        .plugin(tauri_plugin_deep_link::init())
        .invoke_handler(native_command_handler())
        .setup(move |app| {
            // Storage + dashboard init runs HERE — after every plugin's
            // setup, so single-instance (registered first) has already
            // detected a running first instance and exited a second process
            // (OAuth deep-link return) before we touch the redb exclusive
            // lock, the index, or the dashboard port. See the note above
            // `dashboard_holder`.
            let db_path = resolve_desktop_db_path();
            if let Some(parent) = db_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    fatal(&format!(
                        "Failed to create database directory {}: {}",
                        parent.display(),
                        e
                    ));
                }
            }
            tracing::info!("Using database at {}", db_path.display());
            let db = Arc::new(RwLock::new(open_database(&db_path)));
            tracing::info!("redb database initialized");

            // Tantivy full-text search index.
            // Mobile note: the index is mmap'd next to the database and grows
            // with the corpus — on Android this lives in the app-private files
            // dir (no scoped-storage permission needed) but counts against the
            // app quota; large restores should run on Wi-Fi/charger
            // (see docs/ANDROID.md §ABI).
            let index_path = resolve_desktop_index_path(&db_path);
            tracing::info!("Using search index at {}", index_path.display());
            let tantivy_index = open_search_index(&index_path);
            tracing::info!("Tantivy search index ready");

            // Triple-layer compressor.
            let compressor = compression::TripleCompressor::new();

            // HMAC secret for secure session tokens (P0-5): stable across
            // restarts, not OsRng-fresh per boot — desktop sessions used to
            // die on every restart while JWTs lived. Load-or-create at
            // `<data dir>/ipc_hmac_secret` (0600); when no data dir resolves
            // (P0-17 fail-closed), fall back to a fresh secret and log it —
            // sessions become restart-scoped, honestly.
            let mut hmac_secret = [0u8; 32];
            {
                use rand_core::{OsRng, RngCore};
                let stable = cybermanju_crypto::keystore::data_dir().and_then(|dir| {
                    let path = dir.join("ipc_hmac_secret");
                    if let Ok(raw) = std::fs::read(&path) {
                        if raw.len() == 32 {
                            let mut s = [0u8; 32];
                            s.copy_from_slice(&raw);
                            return Some(s);
                        }
                    }
                    let mut fresh = [0u8; 32];
                    OsRng.fill_bytes(&mut fresh);
                    let stored = std::fs::create_dir_all(&dir)
                        .and_then(|_| std::fs::write(&path, fresh))
                        .map(|_| {
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let _ = std::fs::set_permissions(
                                    &path,
                                    std::fs::Permissions::from_mode(0o600),
                                );
                            }
                        })
                        .is_ok()
                        && path.exists();
                    if stored {
                        return Some(fresh);
                    }
                    None
                });
                match stable {
                    Some(s) => hmac_secret = s,
                    None => {
                        tracing::warn!(
                            "no data dir for the IPC HMAC secret — sessions are restart-scoped"
                        );
                        OsRng.fill_bytes(&mut hmac_secret);
                    }
                }
            }

            let state = AppState {
                db: Arc::clone(&db),
                tantivy_index: Arc::new(RwLock::new(tantivy_index)),
                compression: compressor,
                hmac_secret,
            };

            // Dashboard state for connection tracking and lifecycle.
            let dashboard_state = Arc::new(dashboard::DashboardState::new());

            // Sync state for progress tracking and cancellation — shared with
            // the web dashboard so REST and Tauri callers observe the same
            // progress.
            let sync_state = Arc::new(sync_cmd::SyncState::new());

            // ─── Web Dashboard (localhost-only, JWT-authenticated) ───
            // Desktop/server only. On mobile the dashboard is NOT started:
            // binding a localhost HTTP server on a phone wastes battery, trips
            // Play policy review, and no LAN peer expects it — the Android app
            // is a local vault (Tauri IPC), not a server. Dashboard commands
            // stay registered so the frontend's transport switch keeps
            // working; they report "unavailable".
            #[cfg(mobile)]
            let dashboard = {
                let mut dashboard = web_dashboard::WebDashboard::new_shared(
                    web_dashboard::DEFAULT_PORT,
                    Arc::clone(&db),
                );
                dashboard.sync_state = Arc::clone(&sync_state);
                dashboard.set_search_index(Arc::clone(&state.tantivy_index));
                tracing::info!("Web Dashboard disabled on mobile (local-vault mode)");
                Arc::new(dashboard)
            };
            // It borrows the application's database handle instead of opening
            // the redb file a second time (which would fail on the exclusive
            // file lock).
            #[cfg(not(mobile))]
            let dashboard = {
                let mut dashboard = web_dashboard::WebDashboard::new_shared(
                    web_dashboard::DEFAULT_PORT,
                    Arc::clone(&db),
                );
                dashboard.sync_state = Arc::clone(&sync_state);
                dashboard.set_search_index(Arc::clone(&state.tantivy_index));
                let dashboard = Arc::new(dashboard);
                match dashboard.start() {
                    Ok(()) => tracing::info!(
                        "Web Dashboard started on port {} (localhost only, JWT auth)",
                        web_dashboard::DEFAULT_PORT
                    ),
                    Err(e) => tracing::error!("Failed to start Web Dashboard: {}", e),
                }
                dashboard
            };

            app.manage(state);
            app.manage(dashboard_state);
            app.manage(sync_state);
            app.manage(Arc::clone(&dashboard));
            *dashboard_holder_setup
                .lock()
                .expect("dashboard holder poisoned") = Some(dashboard);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Fatal error while running CyberManju OS — see logs above");

    // ─── Clean shutdown: stop the dashboard before dropping the Arc ──
    // Mobile never started it (the Option stays Some but stop() is a no-op
    // there — same Drop path). dashboard.stop() runs from the MAIN thread
    // (not from the accept thread's own Drop, which would self-deadlock).
    // The underscore name keeps mobile builds warning-free (deny warnings).
    let _dashboard = dashboard_holder
        .lock()
        .expect("dashboard holder poisoned")
        .take();
    #[cfg(not(mobile))]
    if let Some(dashboard) = _dashboard {
        dashboard.stop();
    }
    #[cfg(not(mobile))]
    log::info!("Web Dashboard shut down cleanly");
}

/// One command registry for production (including Android) and host IPC tests.
fn native_command_handler<R: Runtime>() -> impl Fn(Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        // File operations
        files::list_files,
        files::get_file,
        files::create_folder,
        files::delete_file,
        files::rename_file,
        files::set_file_tags,
        files::duplicate_file_context,
        files::move_file,
        files::get_preview,
        files::read_file_content,
        files::write_file_content,
        // Read-only OS telemetry and virtual-volume stats. The REST dashboard
        // is intentionally not started on mobile, so the WebView uses IPC.
        commands::os_metrics::os_ps,
        commands::os_metrics::os_top,
        commands::os_metrics::os_workers,
        commands::os_metrics::os_jobs,
        commands::os_metrics::os_df,
        // cybsh OS layer over IPC (mobile has no dashboard; desktop falls
        // back here when :3456 is down). Mirrors POST /api/os/* shapes.
        commands::os_shell::os_exec,
        commands::os_shell::os_complete,
        commands::os_shell::os_stat,
        commands::os_shell::os_ls,
        commands::os_shell::os_du,
        commands::os_shell::os_write,
        commands::os_shell::get_disk,
        commands::os_shell::destroy_disk,
        // Android launcher bridge — Gaveta de Apps (device-local IPC;
        // off-Android the commands refuse with `unsupported:`, never mocks).
        commands::launcher::launcher_list_apps,
        commands::launcher::launcher_open_app,
        commands::launcher::launcher_uninstall_app,
        commands::launcher::launcher_list_icon_packs,
        commands::launcher::launcher_pack_icon,
        commands::launcher::launcher_list_social_apps,
        commands::launcher::launcher_list_messages,
        commands::launcher::launcher_clear_messages,
        commands::launcher::launcher_notification_state,
        commands::launcher::launcher_open_notification_settings,
        commands::launcher::launcher_open_url,
        commands::launcher::launcher_open_file,
        // Point-in-time redb image used by the web-compatible mobile vault mirror
        commands::vault_snapshot::snapshot_native_database,
        commands::vault_snapshot::backup_export,
        commands::vault_snapshot::vault_kv_get,
        commands::vault_snapshot::vault_kv_set,
        commands::vault_snapshot::vault_kv_delete,
        commands::vault_snapshot::vault_kv_list,
        // Search
        search_cmd::search_files,
        search_cmd::search_files_paginated,
        search_cmd::search_files_federated,
        search_cmd::suggest,
        // Encryption
        encryption::encrypt_file,
        encryption::decrypt_file,
        encryption::get_encryption_status,
        encryption::generate_keypair,
        encryption::list_keys,
        // Compression
        commands::compression::compress_file,
        commands::compression::decompress_file,
        commands::compression::get_compression_stats,
        // Collections
        collections::list_collections,
        collections::create_collection,
        collections::add_to_collection,
        collections::remove_from_collection,
        // Face grouping (commands layer — delegates to crate::faces for ML)
        face_cmd::detect_faces,
        face_cmd::detect_faces_batch_cmd,
        face_cmd::recluster_faces,
        face_cmd::rename_face_group,
        face_cmd::merge_face_groups,
        face_cmd::delete_face_group,
        face_cmd::find_similar_faces,
        face_cmd::list_face_groups,
        face_cmd::get_group_files,
        // Map / GPS
        map::get_geo_files,
        map::extract_exif_gps,
        // Accounts
        accounts::list_accounts,
        accounts::create_account,
        accounts::switch_account,
        accounts::delete_account,
        // Tree-sitter code intelligence
        tree_sitter::parse_file,
        tree_sitter::get_symbols,
        tree_sitter::parse_text,
        // Loose groups
        files::create_loose_group,
        files::add_to_loose_group,
        files::list_loose_groups,
        // User management & permissions
        users::register_user,
        users::create_user,
        users::delete_user,
        users::update_user_role,
        users::authenticate_user,
        users::list_users,
        users::set_file_permission,
        users::grant_file_permission,
        users::revoke_file_permission,
        users::verify_file_access,
        users::get_file_permissions,
        users::verify_token,
        // Dashboard
        dashboard::dashboard_status,
        dashboard::start_dashboard,
        dashboard::stop_dashboard,
        // Sync
        sync_cmd::list_sync_configs,
        sync_cmd::create_sync_config,
        sync_cmd::delete_sync_config,
        sync_cmd::start_sync,
        sync_cmd::get_sync_progress,
        sync_cmd::get_sync_status,
        sync_cmd::test_sync_connection,
        sync_cmd::get_sync_usage,
        sync_cmd::cancel_sync,
        sync_cmd::list_remote_files,
        // <<< AGENT-2 SYNC JOBS/RESTORE >>>
        sync_cmd::restore_sync_file,
        sync_cmd::delete_remote_file,
        sync_cmd::get_sync_job,
        sync_cmd::list_sync_runs,
        sync_cmd::create_provider_repo,
        sync_cmd::seed_repo_files,
        sync_cmd::upload_remote_file,
        sync_cmd::move_sync_file,
        // Scheduler (cron) — recurring .cybsh triggers
        commands::cron::cron_list,
        commands::cron::cron_save,
        commands::cron::cron_delete,
        commands::cron::cron_run,
        commands::cron::cron_history,
        commands::cron::cron_set_enabled,
        commands::cron::cron_ensure_started,
        // Device identity (§2B.1) + LAN peers (§2B.2).
        devices::list_devices,
        devices::register_device,
        devices::register_this_device,
        devices::revoke_device,
        devices::remove_device,
        devices::list_lan_peers,
        devices::probe_peer,
        devices::peer_platform_notes,
        devices::sign_peer_ops_for_device,
        // P2P compute twins (§2B swarming + pairing + fountain codes —
        // pure math, no sockets).
        commands::p2p::swarm_encode,
        commands::p2p::swarm_decode,
        commands::p2p::fountain_encode,
        commands::p2p::fountain_decode,
        commands::p2p::pairing_code,
        // Server-side todos (§4.9 agent bridge — same rows the agent tools use).
        todos::list_todos,
        todos::create_todo,
        todos::toggle_todo,
        todos::delete_todo,
        // Secrets keystore (Phase 3) — sealed password-manager rows
        commands::secrets::secret_list,
        commands::secrets::secret_save,
        commands::secrets::secret_update,
        commands::secrets::secret_get,
        commands::secrets::secret_reveal,
        commands::secrets::secret_delete,
        // Native AI agent (configs/sessions/detached jobs)
        commands::agent::list_agent_providers,
        commands::agent::list_agent_configs,
        commands::agent::save_agent_config,
        commands::agent::delete_agent_config,
        commands::agent::save_agent_key,
        commands::agent::list_agent_models,
        commands::agent::list_agent_sessions,
        commands::agent::get_agent_session,
        commands::agent::create_agent_session,
        commands::agent::import_agent_session,
        commands::agent::delete_agent_session,
        commands::agent::start_agent_run,
        commands::agent::agent_job_status,
        commands::agent::list_agent_jobs,
        commands::agent::abort_agent_job,
        commands::agent::approve_agent_job,
        commands::agent::init_agent_run,
        commands::agent::compact_agent_session,
        commands::agent::mcp_add_server,
        commands::agent::mcp_remove_server,
        commands::agent::mcp_list_tools,
        commands::agent::list_agent_memories,
        commands::agent::store_agent_memory,
        commands::agent::recall_agent_memories,
        commands::agent::delete_agent_memory,
        // File import / upload
        import_cmd::import_file,
        import_cmd::import_from_url,
        import_cmd::scan_directory,
        import_cmd::upload_file,
        import_cmd::rebuild_search_index,
        // Share links
        share::generate_share_link,
        share::get_shared_file,
        share::list_share_links,
        // Trash / recycle bin
        trash::list_trash,
        trash::restore_from_trash,
        trash::empty_trash,
        trash::delete_from_trash,
        // Audit log
        audit::get_audit_log,
        // Batch operations
        batch::batch_delete,
        batch::batch_encrypt,
        batch::batch_decrypt,
        batch::batch_compress,
        // File versioning
        versions::list_file_versions,
        versions::create_file_version,
        versions::revert_file_version,
        versions::snapshot_all_versions,
        // Parent index rebuild
        files::rebuild_parent_index,
        // Disks & volumes (AGENT-6)
        commands::disk::create_disk,
        commands::disk::attach_disk,
        commands::disk::detach_disk,
        commands::disk::resize_disk,
        commands::disk::list_disks,
        commands::disk::volume_df,
        commands::disk::check_disk,
        commands::disk::set_disk_key_holder,
    ]
}

#[cfg(test)]
mod native_crypto_tests;
