// CyberManju OS — Secrets (src-tauri/src/commands/secrets.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: secret_list, secret_save, secret_update, secret_get, secret_reveal, secret_delete.
// Integrates: tauri, cybermanju_types.
// Contracts: none special (pure logic; errors bubble to caller).
use tauri::State;

use crate::AppState;
use cybermanju_types::secrets::SecretMeta;

// ---------------------------------------------------------------------------
// Secrets keystore Tauri commands — thin wrappers over the shared web API
// ---------------------------------------------------------------------------
//
// Values are sealed at rest (`seal:v1` under the master passphrase); list
// returns `SecretMeta[]` (hasValue flag only) and never the sealed blob.

/// List secret metadata (never values).
#[tauri::command(async)]
pub fn secret_list(state: State<'_, AppState>) -> Result<Vec<SecretMeta>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    cybermanju_web::api::secrets::list(&db)
}

/// Create or update a secret (plaintext `value` is sealed on write and
/// never echoed back).
#[tauri::command(async)]
pub fn secret_save(
    request: cybermanju_web::api::secrets::CreateRequest,
    state: State<'_, AppState>,
) -> Result<SecretMeta, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_web::api::secrets::upsert(&db, &request)
}

/// Partial-update a secret by id.
#[tauri::command(async)]
pub fn secret_update(
    id: String,
    request: cybermanju_web::api::secrets::UpdateRequest,
    state: State<'_, AppState>,
) -> Result<SecretMeta, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_web::api::secrets::update(&db, &id, &request)
}

/// Single secret metadata row.
#[tauri::command(async)]
pub fn secret_get(id: String, state: State<'_, AppState>) -> Result<SecretMeta, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    cybermanju_web::api::secrets::get_meta(&db, &id)
}

/// Reveal the plaintext value (audited server-side; never logged here).
#[tauri::command(async)]
pub fn secret_reveal(id: String, state: State<'_, AppState>) -> Result<String, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    cybermanju_web::api::secrets::reveal(&db, &id)
}

/// Delete a secret by id.
#[tauri::command(async)]
pub fn secret_delete(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_web::api::secrets::remove(&db, &id)
}
