// CyberManju OS — Cron (src-tauri/src/commands/cron.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: cron_list, cron_save, cron_delete, cron_run, cron_history, cron_set_enabled, cron_ensure_started.
// Integrates: tauri, cybermanju_types.
// Contracts: none special (pure logic; errors bubble to caller).
use std::sync::Arc;
use tauri::State;

use crate::AppState;
use cybermanju_types::schedule::{ScheduleRow, ScheduleRun};

// ---------------------------------------------------------------------------
// Scheduler (cron) Tauri commands — thin wrappers over the shared web API
// ---------------------------------------------------------------------------

/// List all schedules.
#[tauri::command(async)]
pub fn cron_list(state: State<'_, AppState>) -> Result<Vec<ScheduleRow>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    cybermanju_web::api::cron_api::list(&db)
}

/// Create a new schedule.
#[tauri::command(async)]
pub fn cron_save(row: ScheduleRow, state: State<'_, AppState>) -> Result<ScheduleRow, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_web::api::cron_api::save(&db, row)
}

/// Delete a schedule by id.
#[tauri::command(async)]
pub fn cron_delete(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_web::api::cron_api::remove(&db, &id)
}

/// Fire a schedule now (synchronous — the script runs to completion).
#[tauri::command(async)]
pub fn cron_run(id: String, state: State<'_, AppState>) -> Result<ScheduleRun, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    cybermanju_os::scheduler::run_now(&db, &id)
}

/// Run history for one schedule (newest first, capped).
#[tauri::command(async)]
pub fn cron_history(
    id: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<ScheduleRun>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    cybermanju_web::api::cron_api::history(&db, &id, limit.unwrap_or(20))
}

/// Enable or disable a schedule.
#[tauri::command(async)]
pub fn cron_set_enabled(
    id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<ScheduleRow, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_web::api::cron_api::set_enabled(&db, &id, enabled)
}

/// Arm the background daemon (idempotent — called from the first cron poll).
#[tauri::command(async)]
pub fn cron_ensure_started(state: State<'_, AppState>) -> Result<(), String> {
    cybermanju_os::scheduler::ensure_started(Arc::clone(&state.db));
    Ok(())
}
