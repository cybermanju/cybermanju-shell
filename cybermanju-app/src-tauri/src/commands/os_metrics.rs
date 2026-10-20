// CyberManju OS — Os metrics (src-tauri/src/commands/os_metrics.rs).
//! Native IPC equivalents for the read-only OS telemetry routes.
//!
//! Mobile builds do not start the local REST dashboard, so these readings must
//! be callable directly from the Tauri WebView. The underlying procfs reads are
//! best-effort and return unavailable/zero values when Android withholds them;
//! they do not require a runtime permission prompt.
use crate::AppState;
use tauri::State;

#[tauri::command(async)]
/// Process snapshot for the task manager (best-effort on mobile).
pub fn os_ps(state: State<'_, AppState>) -> Result<cybermanju_os::PsSnapshot, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    Ok(cybermanju_os::ps(Some(&*db)))
}

#[tauri::command(async)]
/// Top-like resource ranking for the task manager (best-effort on mobile).
pub fn os_top(state: State<'_, AppState>) -> Result<cybermanju_os::TopSnapshot, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    Ok(cybermanju_os::top(Some(&*db)))
}

#[tauri::command(async)]
/// Background worker pool state for the OS panel.
pub fn os_workers(state: State<'_, AppState>) -> Result<cybermanju_os::Workers, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    Ok(cybermanju_os::workers(Some(&*db)))
}

#[tauri::command(async)]
/// Compute jobs currently available from attached providers + local slots.
pub fn os_jobs() -> Result<Vec<cybermanju_os::compute::JobInfo>, String> {
    Ok(cybermanju_os::available_jobs())
}

/// Statistics for CyberManju's virtual volume, not the phone's entire storage.
#[tauri::command(async)]
pub fn os_df(state: State<'_, AppState>) -> Result<cybermanju_os::VolumeDf, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    Ok(cybermanju_os::Kernel::global().df(Some(&*db)))
}
