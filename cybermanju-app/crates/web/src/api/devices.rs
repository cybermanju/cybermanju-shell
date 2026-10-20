// CyberManju OS — Devices API (device identity registry, §2B.1).
//
// One ML-DSA keypair per device + a `devices` registry table. The private
// half never leaves the device (sealed in its keystore via the existing
// `generate_keypair` flow); this API carries the public half + display name
// so ops, leases, bundles and LAN peers attribute to a device id instead of
// a bare hostname (which PID-reuse and DHCP make unreliable).
//
// Contract:
//   - GET /api/devices → only the caller's `DeviceRow[]` (admins may audit all)
//   - POST /api/devices { id?, name, publicKey?, keyAlgorithm? } → row
//     (id generated when absent; re-POST updates name/key + heartbeat)
//   - DELETE /api/devices/{id} → `{ removed: true }` or `not_found:`
//   - POST /api/devices/{id}/revoke → row with `revoked: true` (Admin;
//     revocation fails closed, the row stays for history)
// Device identity is owner-scoped even for relay/peer routes; DELETE/revoke
// are Admin (see `security::required_role`).

use cybermanju_db::Database;
use cybermanju_types::schema::DeviceRow;

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn respond<T: serde::Serialize>(result: Result<T, String>, origin: Option<&str>) -> String {
    match result {
        Ok(value) => crate::json_ok(&value, origin),
        Err(message) => {
            let status = if message.starts_with("not_found:") {
                404
            } else if message.starts_with("auth:") {
                403
            } else if message.starts_with("unsupported:") {
                501
            } else if message.starts_with("integrity:") {
                409
            } else {
                400
            };
            crate::json_error(status, &message, origin)
        }
    }
}

fn parse<T: for<'de> serde::Deserialize<'de>>(body: &str) -> Result<T, String> {
    serde_json::from_str(body).map_err(|e| format!("invalid: bad JSON body: {e}"))
}

/// Enrollment request — id optional (generated when absent).
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub public_key: Option<String>,
    #[serde(default)]
    pub key_algorithm: Option<String>,
}

/// Register (or re-attest) a device. Re-POST with the same id refreshes the
/// name/key and heartbeats — enrollment is idempotent, never duplicating.
pub fn register(
    db: &Database,
    req: &RegisterRequest,
    owner_id: &str,
    is_admin: bool,
) -> Result<DeviceRow, String> {
    if owner_id.trim().is_empty() {
        return Err("auth: device enrollment requires an authenticated owner".to_string());
    }
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err("invalid: device name is required".to_string());
    }
    if name.len() > 120 {
        return Err("invalid: device name exceeds 120 chars".to_string());
    }
    let id = match req.id.as_deref().map(str::trim) {
        Some(id) if !id.is_empty() => {
            crate::security::validate_id(id)?;
            id.to_string()
        }
        _ => format!("dev-{}", uuid::Uuid::new_v4()),
    };
    let now = now_rfc3339();
    let mut row = match db.get_device(&id).map_err(|e| e.to_string())? {
        Some(row) => {
            if row.owner_id != owner_id {
                if row.owner_id.is_empty() && is_admin {
                    // Admin may explicitly claim a legacy ownerless row.
                } else {
                    return Err("auth: device id is already owned by another account".to_string());
                }
            }
            row
        }
        None => DeviceRow {
            id: id.clone(),
            name: name.clone(),
            owner_id: owner_id.to_string(),
            public_key: String::new(),
            key_algorithm: "ml-dsa-65".to_string(),
            created_at: now.clone(),
            last_seen_at: now.clone(),
            revoked: false,
        },
    };
    row.owner_id = owner_id.to_string();
    row.name = name;
    if let Some(key) = req.public_key.as_deref() {
        let key = key.trim();
        if key.len() > 16_384 {
            return Err("invalid: public key exceeds 16 KiB".to_string());
        }
        row.public_key = key.to_string();
    }
    if let Some(algo) = req.key_algorithm.as_deref() {
        let algo = algo.trim();
        if !algo.is_empty() {
            row.key_algorithm = algo.to_string();
        }
    }
    row.last_seen_at = now;
    db.save_device(&row).map_err(|e| e.to_string())?;
    Ok(row)
}

/// Dispatch `/api/devices/*`.
pub fn route(
    db: &Database,
    method: &str,
    path_segments: &[&str],
    body: &str,
    owner_id: &str,
    is_admin: bool,
    origin: Option<&str>,
) -> Option<String> {
    let response = match path_segments {
        ["api", "devices"] if method == "GET" => {
            let rows = db
                .list_devices()
                .map(|rows| {
                    rows.into_iter()
                        .filter(|row| is_admin || row.owner_id == owner_id)
                        .collect::<Vec<_>>()
                })
                .map_err(|e| e.to_string());
            respond(rows, origin)
        }

        ["api", "devices"] if method == "POST" => match parse::<RegisterRequest>(body) {
            Ok(req) => respond(register(db, &req, owner_id, is_admin), origin),
            Err(e) => respond::<serde_json::Value>(Err(e), origin),
        },

        ["api", "devices", id] if method == "DELETE" => match db.remove_device(id) {
            Ok(true) => respond(Ok(serde_json::json!({ "removed": true })), origin),
            Ok(false) => {
                respond::<serde_json::Value>(Err(format!("not_found: no device `{id}`")), origin)
            }
            Err(e) => respond::<serde_json::Value>(Err(e.to_string()), origin),
        },

        ["api", "devices", id, "revoke"] if method == "POST" => {
            match db.get_device(id).map_err(|e| e.to_string()) {
                Ok(Some(mut row)) => {
                    row.revoked = true;
                    row.last_seen_at = now_rfc3339();
                    match db.save_device(&row).map_err(|e| e.to_string()) {
                        Ok(()) => respond(Ok(row), origin),
                        Err(e) => respond::<DeviceRow>(Err(e), origin),
                    }
                }
                Ok(None) => {
                    respond::<DeviceRow>(Err(format!("not_found: no device `{id}`")), origin)
                }
                Err(e) => respond::<DeviceRow>(Err(e), origin),
            }
        }

        _ => return None,
    };
    Some(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> (Database, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "cybermanju-devices-api-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(path.to_str().unwrap()).unwrap();
        (db, path)
    }

    fn request(id: &str) -> RegisterRequest {
        RegisterRequest {
            id: Some(id.to_string()),
            name: "Phone".to_string(),
            public_key: None,
            key_algorithm: None,
        }
    }

    #[test]
    fn owner_cannot_take_over_or_list_another_accounts_device() {
        let (db, path) = test_db();
        register(&db, &request("device-a"), "owner-a", false).unwrap();
        let takeover = register(&db, &request("device-a"), "owner-b", false).unwrap_err();
        assert!(takeover.starts_with("auth:"));

        let hidden = route(&db, "GET", &["api", "devices"], "", "owner-b", false, None).unwrap();
        assert!(!hidden.contains("device-a"));
        let visible = route(&db, "GET", &["api", "devices"], "", "owner-a", false, None).unwrap();
        assert!(visible.contains("device-a"));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn only_admin_can_claim_a_legacy_ownerless_device() {
        let (db, path) = test_db();
        db.save_device(&DeviceRow {
            id: "legacy-device".to_string(),
            name: "Old phone".to_string(),
            owner_id: String::new(),
            public_key: String::new(),
            key_algorithm: "ml-dsa-65".to_string(),
            created_at: now_rfc3339(),
            last_seen_at: now_rfc3339(),
            revoked: false,
        })
        .unwrap();
        assert!(register(&db, &request("legacy-device"), "owner-a", false)
            .unwrap_err()
            .starts_with("auth:"));
        let claimed = register(&db, &request("legacy-device"), "owner-a", true).unwrap();
        assert_eq!(claimed.owner_id, "owner-a");
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
