// CyberManju OS — Os shell (src-tauri/src/commands/os_shell.rs).
//! Native IPC for the cybsh OS layer (mobile + desktop fallback).
//!
//! The Web Dashboard serves these over REST (`POST /api/os/exec`, …) but the
//! mobile build never starts the dashboard — the WebView must reach the same
//! kernel over Tauri IPC. Every command below mirrors its REST twin's shape
//! (`ShellResult` envelope for exec, raw structs otherwise) so the frontend
//! transport switch stays invisible.

use serde::Serialize;
use tauri::State;

use crate::AppState;

/// `POST /api/os/exec` result — one `cybsh` line in, one rendered answer out.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellResult {
    pub ok: bool,
    pub line: String,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub prompt: &'static str,
}

/// `du` summary for one volume path (bytes + file count).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuResult {
    pub path: String,
    pub bytes: u64,
    pub files: u64,
}

/// Run one `cybsh` line against the volume kernel + task table.
#[tauri::command(async)]
pub fn os_exec(line: String, state: State<'_, AppState>) -> Result<ShellResult, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    let owned = line.clone();
    match cybermanju_os::execute(&owned, Some(&*db)) {
        Ok(output) => Ok(ShellResult {
            ok: true,
            line: owned,
            output,
            error: None,
            prompt: cybermanju_os::PROMPT,
        }),
        Err(message) => Ok(ShellResult {
            ok: false,
            line: owned,
            output: message,
            error: None,
            prompt: cybermanju_os::PROMPT,
        }),
    }
}

/// Tab completion off the live command table.
#[tauri::command(async)]
pub fn os_complete(prefix: String) -> Result<Vec<String>, String> {
    let trimmed = prefix.strip_prefix('/').unwrap_or(&prefix);
    Ok(cybermanju_os::completions(trimmed))
}

/// `stat(2)` for one volume path.
#[tauri::command(async)]
pub fn os_stat(path: String) -> Result<cybermanju_os::Stat, String> {
    cybermanju_os::Kernel::global().stat(&path)
}

/// `readdir(3)` for one volume path.
#[tauri::command(async)]
pub fn os_ls(path: String) -> Result<Vec<cybermanju_os::DirEntry>, String> {
    cybermanju_os::Kernel::global().readdir(&path)
}

/// `du(1)` for one volume path.
#[tauri::command(async)]
pub fn os_du(path: String) -> Result<DuResult, String> {
    match cybermanju_os::Kernel::global().du(&path) {
        Ok((bytes, files)) => Ok(DuResult { path, bytes, files }),
        Err(message) => Err(message),
    }
}

const MAX_WRITE_BYTES: usize = 1024 * 1024;

/// Editor save path — same 1 MiB cap + envelope as `PUT /api/os/write`.
#[tauri::command(async)]
pub fn os_write(path: String, content: String) -> Result<serde_json::Value, String> {
    if path.trim().is_empty() {
        return Err("invalid: path is required".to_string());
    }
    if content.len() > MAX_WRITE_BYTES {
        return Err(format!(
            "too_large: content is {} bytes, volume write limit is {}",
            content.len(),
            MAX_WRITE_BYTES
        ));
    }
    let kernel = cybermanju_os::Kernel::global();
    let fd = kernel.open(&path, cybermanju_os::OpenFlags::create())?;
    let bytes = content.as_bytes();
    let mut written = 0usize;
    let result = (|| {
        while written < bytes.len() {
            let n = kernel.write(fd, &bytes[written..])?;
            if n == 0 {
                return Err("io error: write returned 0 bytes".to_string());
            }
            written += n;
        }
        Ok(written)
    })();
    let _ = kernel.close(fd);
    let bytes = result?;
    Ok(serde_json::json!({
        "ok": true,
        "output": format!("wrote {} ({} bytes)", path, bytes),
        "path": path,
        "bytes": bytes,
    }))
}

/// Single disk lookup (frontend `get_disk { id }`).
#[tauri::command(async)]
pub fn get_disk(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<cybermanju_disk::DiskRow>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    cybermanju_disk::disk::get(&db, &id)
}

/// Destroy a disk row (frontend `destroy_disk { id }`).
#[tauri::command(async)]
pub fn destroy_disk(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_disk::disk::destroy(&db, &id)
}
