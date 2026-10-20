// CyberManju OS — Audit (src-tauri/src/commands/audit.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: get_audit_log.
// Integrates: cybermanju_web, tauri.
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — Audit log commands (thin wrappers over the shared API)

use cybermanju_web::api;
use tauri::State;

use crate::db::schema::AuditEntry;
use crate::AppState;

/// Fetch audit log entries with optional limit and entity filter.
#[tauri::command(async)]
pub fn get_audit_log(
    limit: Option<u32>,
    entity_type: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<AuditEntry>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    api::audit::list(&db, limit, entity_type.as_deref())
}
