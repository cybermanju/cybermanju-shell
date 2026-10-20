// CyberManju OS — disk & volume commands (AGENT-6)
//
// Thin wrappers over `cybermanju-disk`: every command takes the shared
// `AppState`, borrows the redb handle for the duration of the call and hands
// the crate's `Result` straight back to the frontend as `Result<T, String>`.
//
// Registered from `src-tauri/src/lib.rs` — this agent does not edit either.

use tauri::State;

use cybermanju_disk::{disk, volume, CheckReport, DiskRow, VolumeDf};

use crate::AppState;

/// Every `.cybermanju` disk this host knows about, attached or not.
#[tauri::command(async)]
pub fn list_disks(state: State<'_, AppState>) -> Result<Vec<DiskRow>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    disk::list(&db)
}

/// Create a disk of `size_bytes` bound to `config_id` and attach it.
///
/// `size_bytes` is the choosable size: `0`, sub-block sizes and sizes the
/// provider cannot actually back are refused here, not later.
#[tauri::command(async)]
pub fn create_disk(
    config_id: String,
    size_bytes: u64,
    passphrase: String,
    state: State<'_, AppState>,
) -> Result<DiskRow, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    disk::create(&db, &config_id, size_bytes, &passphrase)
}

/// Unlock a disk: verify its sealed container and start counting it in `df`.
#[tauri::command(async)]
pub fn attach_disk(
    id: String,
    passphrase: String,
    state: State<'_, AppState>,
) -> Result<DiskRow, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    disk::attach(&db, &id, &passphrase)
}

/// Seal a disk's container and take it out of `df`. Idempotent.
#[tauri::command(async)]
pub fn detach_disk(id: String, state: State<'_, AppState>) -> Result<DiskRow, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    disk::detach(&db, &id)
}

/// Grow (or shrink, when the data still fits) a disk's choosable capacity.
#[tauri::command(async)]
pub fn resize_disk(
    id: String,
    size_bytes: u64,
    state: State<'_, AppState>,
) -> Result<DiskRow, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    disk::resize(&db, &id, size_bytes)
}

/// `df(1)` for the merged volume — the "more providers, more space" number.
#[tauri::command(async)]
pub fn volume_df(state: State<'_, AppState>) -> Result<VolumeDf, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    volume::df(&db)
}

/// fsck: superblock checksum, catalog vs. sealed container, orphans, and
/// whether the disk is merely locked.
#[tauri::command(async)]
pub fn check_disk(id: String, state: State<'_, AppState>) -> Result<CheckReport, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    disk::check(&db, &id)
}

/// Designate the single key-holder disk (clears the flag on all others).
#[tauri::command(async)]
pub fn set_disk_key_holder(id: String, state: State<'_, AppState>) -> Result<DiskRow, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    disk::set_key_holder(&db, &id)
}
