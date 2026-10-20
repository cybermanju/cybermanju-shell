// CyberManju OS — Audit (crates/web/src/api/audit.rs).
// Role: REST dashboard (:3456): routes, auth, security headers.
// Exports: list.
// Integrates: cybermanju_db, cybermanju_types, redb.
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — Audit log (shared by Tauri IPC and REST)

use cybermanju_db::Database;
use cybermanju_types::schema::AuditEntry;
use redb::ReadableTable;

/// Fetch audit log entries with optional limit and entity filter.
pub fn list(
    db: &Database,
    limit: Option<u32>,
    entity_type: Option<&str>,
) -> Result<Vec<AuditEntry>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_audit_log_table())
        .map_err(|e| e.to_string())?;

    let mut entries: Vec<AuditEntry> = table
        .iter()
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            let (_, value) = entry.ok()?;
            serde_json::from_str::<AuditEntry>(value.value()).ok()
        })
        .filter(|e| match entity_type {
            Some(et) => e.entity_type == et,
            None => true,
        })
        .collect();

    // Sort by timestamp descending (most recent first)
    entries.sort_by_key(|a| std::cmp::Reverse(a.timestamp.clone()));

    let limit = limit.unwrap_or(100) as usize;
    entries.truncate(limit);
    Ok(entries)
}
