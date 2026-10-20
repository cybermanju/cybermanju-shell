// CyberManju OS — Devices (src-tauri/src/commands/devices.rs).
// Role: Tauri IPC commands for device identity (§2B.1) + LAN peers (§2B.2).
// Exports: list_devices, register_device, register_this_device,
// revoke_device, remove_device, list_lan_peers, sign_peer_ops_for_device,
// device_seed_key_id.
// Integrates: shared web API (devices), PQC engine (ML-DSA attestation),
// sync lan scanner; State<AppState> locks mirror the cron module.
// Contracts: trusted-local transport (same posture as the rest of IPC);
// revocation is still admin-gated on the REST path.
use tauri::State;

use crate::AppState;
use cybermanju_types::schema::DeviceRow;

// ---------------------------------------------------------------------------
// Device identity — thin wrappers over the shared web API
// ---------------------------------------------------------------------------

/// All registered devices (revoked rows included, flagged).
#[tauri::command(async)]
pub fn list_devices(state: State<'_, AppState>) -> Result<Vec<DeviceRow>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    db.list_devices().map_err(|e| e.to_string())
}

/// Register (or re-attest) a device row.
#[tauri::command(async)]
pub fn register_device(
    name: String,
    public_key: Option<String>,
    key_algorithm: Option<String>,
    state: State<'_, AppState>,
) -> Result<DeviceRow, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_web::api::devices::register(
        &db,
        &cybermanju_web::api::devices::RegisterRequest {
            id: None,
            name,
            public_key,
            key_algorithm,
        },
        "local",
        true,
    )
}

/// Keystore row id holding one device's sealed ML-DSA seed.
pub fn device_seed_key_id(device_id: &str) -> String {
    format!("device:{device_id}")
}

/// Enroll THIS device: fresh ML-DSA-65 keypair, public half registered, seed
/// half sealed into the encryption-keys table (same at-rest posture as every
/// other keypair — `seal_private_key` refuses raw persistence). The seed
/// never crosses IPC: attested sync signs server-side via
/// `sign_peer_ops_for_device`. If the vault is locked or persistence fails,
/// enrollment fails closed and removes the just-created device row.
#[tauri::command(async)]
pub fn register_this_device(name: String, state: State<'_, AppState>) -> Result<DeviceRow, String> {
    use crate::crypto::pqc::{algorithm_from_str, PqcEngine};
    let algo = algorithm_from_str("ml_dsa65")
        .ok_or_else(|| "integrity: ml_dsa65 unknown to the PQC engine".to_string())?;
    let mut engine = PqcEngine::new();
    let keypair = engine
        .generate_keypair(algo)
        .map_err(|e| format!("Key generation failed: {e}"))?;
    let public_key_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &keypair.public_key,
    );
    let sealed = super::encryption::seal_private_key(&keypair.private_key)
        .map_err(|e| format!("auth: cannot seal device identity seed: {e}"))?;
    let row = {
        let db = state.db.write().map_err(|e| e.to_string())?;
        cybermanju_web::api::devices::register(
            &db,
            &cybermanju_web::api::devices::RegisterRequest {
                // The op-log writer uses this same stable local identity;
                // registering a random UUID here would make every local op
                // fail the signer/device binding check during peer sync.
                id: Some(cybermanju_db::Database::local_device_id()),
                name,
                public_key: Some(public_key_b64.clone()),
                key_algorithm: Some("ml-dsa-65".to_string()),
            },
            "local",
            true,
        )?
    };
    // Persist the seed for later attestation. Do not report success if either
    // the keystore write or commit fails; remove the row to avoid an
    // apparently enrolled but unusable identity.
    let stored = crate::db::schema::EncryptionKey {
        id: device_seed_key_id(&row.id),
        algorithm: "ml-dsa-65".to_string(),
        public_key: public_key_b64,
        private_key: sealed,
        created_at: chrono::Utc::now().to_rfc3339(),
        label: Some(format!("device identity seed for {}", row.name)),
        x25519_public_key: None,
        x25519_private_key: None,
    };
    let persist_result = (|| -> Result<(), String> {
        let db = state.db.write().map_err(|e| e.to_string())?;
        let serialized = serde_json::to_string(&stored).map_err(|e| e.to_string())?;
        let tx = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = tx
                .open_table(crate::db::Database::get_encryption_keys_table())
                .map_err(|e| e.to_string())?;
            table
                .insert(stored.id.as_str(), serialized.as_str())
                .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(err) = persist_result {
        if let Ok(db) = state.db.write() {
            let _ = db.remove_device(&row.id);
        }
        return Err(format!(
            "integrity: device identity seed could not be saved: {err}"
        ));
    }
    Ok(row)
}

/// Sign op-log rows as THIS device without ever exposing its seed: loads the
/// sealed `device:<id>` seed (fails closed when the vault is locked or the
/// device was enrolled unattested) and returns `{ signature, algorithm }`
/// for `SignedSyncBody`. This is the desktop signing path for attested peer
/// sync; Pages signs in-browser via wasm `ml_dsa65_sign`.
#[tauri::command(async)]
pub fn sign_peer_ops_for_device(
    device_id: String,
    ops_json: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let seed_b64 = {
        let db = state.db.read().map_err(|e| e.to_string())?;
        let device = db
            .get_device(device_id.trim())
            .map_err(|e| e.to_string())?
            .ok_or_else(|| {
                format!(
                    "not_found: no device '{}'; enroll this device first",
                    device_id.trim()
                )
            })?;
        if device.revoked || device.owner_id != "local" {
            return Err(
                "auth: signing requires an active device owned by this local vault".to_string(),
            );
        }
        let tx = db.begin_read().map_err(|e| e.to_string())?;
        let table = tx
            .open_table(crate::db::Database::get_encryption_keys_table())
            .map_err(|e| e.to_string())?;
        let key_id = device_seed_key_id(device_id.trim());
        let stored: crate::db::schema::EncryptionKey = match table
            .get(key_id.as_str())
            .map_err(|e| e.to_string())?
        {
            Some(value) => serde_json::from_str(value.value()).map_err(|e| e.to_string())?,
            None => {
                return Err(format!(
                    "not_found: no sealed seed for device '{}' — enroll this device first (register_this_device seals it)",
                    device_id.trim()
                ))
            }
        };
        if stored.public_key != device.public_key || stored.algorithm != device.key_algorithm {
            return Err(
                "integrity: sealed signing key does not match the device registry".to_string(),
            );
        }
        let seed = super::encryption::open_private_key(&stored.private_key).map_err(|e| {
            format!("auth: vault is locked or the seed is unreadable — unlock to sign ({e})")
        })?;
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &seed)
    };
    let ops: Vec<cybermanju_types::schema::OpRow> =
        serde_json::from_str(&ops_json).map_err(|e| format!("invalid: ops are not JSON: {e}"))?;
    if ops.iter().any(|op| op.device_id != device_id) {
        return Err("auth: every signed op must belong to the requested device".to_string());
    }
    let signature = cybermanju_sync::auth::sign_envelope(&seed_b64, "ml-dsa-65", &ops)?;
    Ok(serde_json::json!({ "signature": signature, "algorithm": "ml-dsa-65" }))
}

/// Revoke a device (row stays for history, fails closed everywhere).
#[tauri::command(async)]
pub fn revoke_device(id: String, state: State<'_, AppState>) -> Result<DeviceRow, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    let mut row = db
        .get_device(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("not_found: no device `{id}`"))?;
    row.revoked = true;
    row.last_seen_at = chrono::Utc::now().to_rfc3339();
    db.save_device(&row).map_err(|e| e.to_string())?;
    Ok(row)
}

/// Delete a device row entirely.
#[tauri::command(async)]
pub fn remove_device(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    db.remove_device(&id).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// LAN peers (§2B.2) — active mDNS scan, 2 s cap
// ---------------------------------------------------------------------------

/// Probe a peer's `/api/readyz` with a bounded timeout. `Ok(true)` on HTTP
/// 2xx, else an AGENT-1 `network:` error — never a hang, never a panic.
/// Works for manual `device@host:port` peers on mobile/docker where mDNS
/// multicast is blocked.
#[tauri::command(async)]
pub fn probe_peer(base_url: String, timeout_secs: Option<u64>) -> Result<bool, String> {
    cybermanju_sync::iroh::http_probe(&base_url, timeout_secs.unwrap_or(15))
}

/// Per-platform P2P reality check for the Devices → P2P matrix: every target
/// names its working fallback (manual peers, host networking, fetch …).
#[tauri::command(async)]
pub fn peer_platform_notes() -> Vec<serde_json::Value> {
    cybermanju_sync::peers::platform_notes()
        .into_iter()
        .map(|(target, note)| serde_json::json!({ "target": target, "note": note }))
        .collect()
}

/// Devices answering `_cybermanju._tcp.local` on the LAN right now.
#[tauri::command(async)]
pub fn list_lan_peers() -> Result<Vec<serde_json::Value>, String> {
    let peers = cybermanju_sync::lan::discover(
        std::net::SocketAddr::V4(std::net::SocketAddrV4::new(
            cybermanju_sync::lan::MDNS_ADDR,
            cybermanju_sync::lan::MDNS_PORT,
        )),
        std::time::Duration::from_secs(2),
    )?;
    Ok(peers
        .iter()
        .map(|p| {
            serde_json::json!({
                "deviceId": p.device_id,
                "name": p.name,
                "host": p.host,
                "port": p.port,
                "lastSeenSecs": p.last_seen_secs,
            })
        })
        .collect())
}
