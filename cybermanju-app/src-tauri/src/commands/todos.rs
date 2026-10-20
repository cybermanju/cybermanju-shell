// CyberManju OS — Todos (src-tauri/src/commands/todos.rs).
// Role: Tauri IPC commands for server-side todos (§4.9 agent bridge).
// Exports: list_todos, create_todo, toggle_todo, delete_todo.
// Integrates: shared web API (todos); State<AppState> locks mirror cron.
// Contracts: trusted-local transport — no owner filter here (the desktop is
// single-user-local); the REST path owner-scopes non-admins.
use tauri::State;

use crate::AppState;
use cybermanju_types::schema::TodoRow;

/// All server todos (the agent's list too — same rows, owner-tagged).
#[tauri::command(async)]
pub fn list_todos(state: State<'_, AppState>) -> Result<Vec<TodoRow>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    db.list_todos(None).map_err(|e| e.to_string())
}

/// Create a local-source todo.
#[tauri::command(async)]
pub fn create_todo(title: String, state: State<'_, AppState>) -> Result<TodoRow, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_web::api::todos::create(
        &db,
        "local",
        &cybermanju_web::api::todos::CreateRequest { title },
    )
}

/// Complete or reopen a todo by exact id.
#[tauri::command(async)]
pub fn toggle_todo(id: String, done: bool, state: State<'_, AppState>) -> Result<TodoRow, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    let mut row = db
        .list_todos(None)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| format!("not_found: no todo `{id}`"))?;
    row.done = done;
    row.done_at = if done {
        Some(chrono::Utc::now().to_rfc3339())
    } else {
        None
    };
    db.save_todo(&row).map_err(|e| e.to_string())?;
    Ok(row)
}

/// Delete a todo by exact id.
#[tauri::command(async)]
pub fn delete_todo(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    db.remove_todo(&id).map_err(|e| e.to_string())
}
