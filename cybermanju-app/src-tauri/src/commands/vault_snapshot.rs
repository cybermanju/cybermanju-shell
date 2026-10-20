// CyberManju OS — Vault snapshot (src-tauri/src/commands/vault_snapshot.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: snapshot_native_database, backup_export, vault_kv_get, vault_kv_set, vault_kv_delete, vault_kv_list.
// Integrates: serde_json, tauri.
// Contracts: none special (pure logic; errors bubble to caller).
use serde_json::{json, Value};
use tauri::State;

use crate::AppState;

const MAX_KV_KEY_BYTES: usize = 512;
const MAX_KV_VALUE_BYTES: usize = 5 * 1024 * 1024;

/// Return a point-in-time image of the native redb database.
///
/// The frontend wraps these bytes with the shared CYBMJ01 web container codec
/// before writing a user-selected Android/iOS SAF file.
#[tauri::command(async)]
pub fn snapshot_native_database(state: State<'_, AppState>) -> Result<Vec<u8>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    db.snapshot_bytes()
        .map_err(|e| format!("native database snapshot failed: {e:#}"))
}

/// Vault backup export — the shared REST shape (`api::backup::export_db`)
/// over IPC: raw image base64'd, sealed into CYBMJ01 by the frontend with a
/// passphrase that never leaves the device.
#[tauri::command(async)]
pub fn backup_export(
    state: State<'_, AppState>,
) -> Result<cybermanju_web::api::backup::ExportReport, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    cybermanju_web::api::backup::export_db(&db)
}

/// Native mobile equivalent of the WASM `kv.get` operation.
#[tauri::command(async)]
pub fn vault_kv_get(key: String, state: State<'_, AppState>) -> Result<Value, String> {
    if key.len() > MAX_KV_KEY_BYTES {
        return Err(format!("invalid: key exceeds {MAX_KV_KEY_BYTES} bytes"));
    }
    let db = state.db.read().map_err(|e| e.to_string())?;
    match db
        .kv_get(&key)
        .map_err(|e| format!("vault kv get failed: {e:#}"))?
    {
        Some(value) => Ok(json!({ "key": key, "bytes": value.len(), "value": value })),
        None => Ok(Value::Null),
    }
}

/// Native mobile equivalent of the WASM `kv.set` operation.
#[tauri::command(async)]
pub fn vault_kv_set(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    if key.is_empty() || key.len() > MAX_KV_KEY_BYTES {
        return Err(format!("invalid: key must be 1–{MAX_KV_KEY_BYTES} bytes"));
    }
    if value.len() > MAX_KV_VALUE_BYTES {
        return Err(format!("invalid: value exceeds {MAX_KV_VALUE_BYTES} bytes"));
    }
    let bytes = value.len();
    let db = state.db.read().map_err(|e| e.to_string())?;
    db.kv_set(&key, &value)
        .map_err(|e| format!("vault kv set failed: {e:#}"))?;
    Ok(json!({ "key": key, "bytes": bytes }))
}

/// Native mobile equivalent of the WASM `kv.delete` operation.
#[tauri::command(async)]
pub fn vault_kv_delete(key: String, state: State<'_, AppState>) -> Result<bool, String> {
    if key.len() > MAX_KV_KEY_BYTES {
        return Err(format!("invalid: key exceeds {MAX_KV_KEY_BYTES} bytes"));
    }
    let db = state.db.read().map_err(|e| e.to_string())?;
    db.kv_delete(&key)
        .map_err(|e| format!("vault kv delete failed: {e:#}"))
}

/// Native mobile equivalent of the WASM `kv.list` operation (keys and sizes only).
#[tauri::command(async)]
pub fn vault_kv_list(
    prefix: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<Value>, String> {
    let prefix = prefix.unwrap_or_default();
    let db = state.db.read().map_err(|e| e.to_string())?;
    let rows = db
        .kv_list(&prefix)
        .map_err(|e| format!("vault kv list failed: {e:#}"))?;
    Ok(rows
        .into_iter()
        .map(|(key, bytes)| json!({ "key": key, "bytes": bytes }))
        .collect())
}
