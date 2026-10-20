// CyberManju OS — Op-log API (local-first sync exchange, §2C/P3-2).
//
// Devices exchange mutation rows (`OpRow`) using the version vectors the
// `leases` module already models. Push is idempotent (`merge_ops` skips
// known `op_id`s); pull pages by `seq`; conflicts renders the fork pairs
// the UI lets the user resolve; resolve appends a `resolve` op so history
// is never deleted — both forks stay readable under the resolution.
//
// Contract:
//   - GET /api/oplog/since?since=N&limit=M → `{ head, ops }`; `since` is an
//     inclusive next-sequence cursor and `head` is the next cursor for paging.
//   - POST /api/oplog/push { deviceId, ops, signature, algorithm } → verified merge
//   - GET /api/oplog/conflicts → `{ conflicts: [{a, b}] }`
//   - POST /api/oplog/resolve { opA, opB, choice, deviceId? } → `{ resolved, op }`
// Every operation is owner-scoped; pushes require a signature from the
// registered device key, and unsigned legacy pushes fail closed.

use cybermanju_db::Database;
use cybermanju_types::schema::OpRow;

fn respond<T: serde::Serialize>(result: Result<T, String>, origin: Option<&str>) -> String {
    match result {
        Ok(value) => crate::json_ok(&value, origin),
        Err(message) => {
            let status = if message.starts_with("not_found:") {
                404
            } else if message.starts_with("auth:") {
                403
            } else if message.starts_with("too_large:") {
                413
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

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushRequest {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub ops: Vec<OpRow>,
    #[serde(default)]
    pub signature: String,
    #[serde(default)]
    pub algorithm: String,
}

/// Merge foreign ops idempotently; absurd pages refused before they merge.
pub fn push(db: &Database, req: &PushRequest, owner_id: &str) -> Result<serde_json::Value, String> {
    if req.ops.len() > 10_000 {
        return Err(format!(
            "too_large: push holds {} ops (cap 10000 per call — page it)",
            req.ops.len()
        ));
    }
    if owner_id.trim().is_empty() || req.device_id.trim().is_empty() {
        return Err("auth: an authenticated owner and device id are required".to_string());
    }
    let device = db
        .get_device(&req.device_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("not_found: no device '{}'", req.device_id))?;
    if device.revoked || device.owner_id.is_empty() || device.owner_id != owner_id {
        return Err(
            "auth: device is not active and owned by the authenticated account".to_string(),
        );
    }
    for op in &req.ops {
        if op.kind.trim().is_empty() {
            return Err("invalid: every pushed op needs a kind".to_string());
        }
        if op.device_id.trim().is_empty() {
            return Err("invalid: every pushed op needs a device_id".to_string());
        }
        if op.device_id != req.device_id {
            return Err("auth: every pushed op must belong to the attested device".to_string());
        }
    }
    let verified = cybermanju_sync::auth::verify_envelope(
        &req.ops,
        &req.signature,
        &req.algorithm,
        &device.public_key,
        &device.key_algorithm,
    )?;
    let (appended, skipped) = db.merge_ops(&req.ops).map_err(|e| e.to_string())?;
    let head = db.oplog_head().map_err(|e| e.to_string())?;
    Ok(
        serde_json::json!({ "appended": appended, "skipped": skipped, "head": head, "verified": verified }),
    )
}

/// Resolution request — `choice` is `keep-a` | `keep-b` | `merge`.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveRequest {
    #[serde(default)]
    pub op_a: String,
    #[serde(default)]
    pub op_b: String,
    #[serde(default)]
    pub choice: String,
    #[serde(default)]
    pub device_id: String,
}

/// Resolve one fork pair by appending a `resolve` op. Both forks stay in the
/// log — resolution is a verdict, not a deletion. Unknown op ids are
/// `not_found:`, unknown choices are `invalid:` (the UI only sends the three
/// documented verbs; the cybsh layer normalizes to `merge` first).
pub fn resolve_op(
    db: &Database,
    req: &ResolveRequest,
    owner_id: &str,
) -> Result<serde_json::Value, String> {
    if req.op_a.trim().is_empty() || req.op_b.trim().is_empty() {
        return Err("invalid: resolve needs both op ids (opA, opB)".to_string());
    }
    if !["keep-a", "keep-b", "merge"].contains(&req.choice.as_str()) {
        return Err("invalid: choice must be keep-a, keep-b or merge".to_string());
    }
    let a = db
        .get_op_by_id(&req.op_a)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("not_found: no op '{}'", req.op_a))?;
    let b = db
        .get_op_by_id(&req.op_b)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("not_found: no op '{}'", req.op_b))?;
    let owner_devices: std::collections::HashSet<String> = db
        .list_devices()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|d| !d.revoked && d.owner_id == owner_id)
        .map(|d| d.id)
        .collect();
    if owner_id.trim().is_empty()
        || !owner_devices.contains(&a.device_id)
        || !owner_devices.contains(&b.device_id)
    {
        return Err(
            "auth: both conflict operations must belong to the authenticated account".to_string(),
        );
    }
    let device_id = if req.device_id.trim().is_empty() {
        Database::local_device_id()
    } else {
        req.device_id.clone()
    };
    if !owner_devices.contains(&device_id) {
        return Err(
            "auth: resolution device is not owned by the authenticated account".to_string(),
        );
    }
    let file_id = a.file_id.clone().or(b.file_id.clone());
    let row = OpRow {
        seq: 0,
        op_id: String::new(),
        device_id,
        created_at: chrono::Utc::now().to_rfc3339(),
        kind: "resolve".to_string(),
        file_id,
        payload: serde_json::json!({
            "opA": a.op_id,
            "opB": b.op_id,
            "choice": req.choice,
        }),
        bases: Default::default(),
    };
    let stored = db.append_op(row).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "resolved": true, "op": stored }))
}

/// Authenticated owner/admin context supplied by the HTTP auth gate.
#[derive(Clone, Copy)]
pub struct OplogAuth<'a> {
    pub owner_id: &'a str,
    pub is_admin: bool,
}

/// Dispatch `/api/oplog/*`.
pub fn route(
    db: &Database,
    method: &str,
    path_segments: &[&str],
    query: &str,
    body: &str,
    auth: OplogAuth<'_>,
    origin: Option<&str>,
) -> Option<String> {
    let response = match path_segments {
        ["api", "oplog", "since"] if method == "GET" => {
            let since = crate::parse_query_param(query, "since")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            let limit = crate::parse_query_param(query, "limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(500);
            let limit = limit.clamp(1, 5000);
            let result = if auth.is_admin {
                db.list_ops_since(since, limit).map(|ops| {
                    let head = ops
                        .last()
                        .map(|op| op.seq.saturating_add(1))
                        .unwrap_or(since);
                    (ops, head)
                })
            } else if auth.owner_id.trim().is_empty() {
                Err(anyhow::anyhow!("auth: authenticated owner is required"))
            } else {
                db.list_devices().and_then(|rows| {
                    let ids: std::collections::HashSet<String> = rows
                        .into_iter()
                        .filter(|d| !d.revoked && d.owner_id == auth.owner_id)
                        .map(|d| d.id)
                        .collect();
                    db.list_ops_since_for_devices(since, limit, &ids)
                        .map(|ops| {
                            let head = ops
                                .last()
                                .map(|op| op.seq.saturating_add(1))
                                .unwrap_or(since);
                            (ops, head)
                        })
                })
            }
            .map_err(|e| e.to_string());
            match result {
                Ok((ops, head)) => {
                    respond(Ok(serde_json::json!({ "head": head, "ops": ops })), origin)
                }
                Err(e) => respond::<serde_json::Value>(Err(e), origin),
            }
        }

        ["api", "oplog", "push"] if method == "POST" => match parse::<PushRequest>(body) {
            Ok(req) => respond(push(db, &req, auth.owner_id), origin),
            Err(e) => respond::<serde_json::Value>(Err(e), origin),
        },

        ["api", "oplog", "conflicts"] if method == "GET" => {
            let result = if auth.is_admin {
                db.op_conflicts()
            } else if auth.owner_id.trim().is_empty() {
                Err(anyhow::anyhow!("auth: authenticated owner is required"))
            } else {
                db.list_devices().and_then(|rows| {
                    let ids: std::collections::HashSet<String> = rows
                        .into_iter()
                        .filter(|d| !d.revoked && d.owner_id == auth.owner_id)
                        .map(|d| d.id)
                        .collect();
                    db.op_conflicts_for_devices(&ids)
                })
            }
            .map_err(|e| e.to_string());
            match result {
                Ok(pairs) => respond(
                    Ok(serde_json::json!({
                        "conflicts": pairs.iter().map(|(a, b)| serde_json::json!({ "a": a, "b": b })).collect::<Vec<_>>()
                    })),
                    origin,
                ),
                Err(e) => respond::<serde_json::Value>(Err(e), origin),
            }
        }

        ["api", "oplog", "resolve"] if method == "POST" => match parse::<ResolveRequest>(body) {
            Ok(req) => respond(resolve_op(db, &req, auth.owner_id), origin),
            Err(e) => respond::<serde_json::Value>(Err(e), origin),
        },

        _ => return None,
    };
    Some(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cybermanju_types::schema::DeviceRow;

    fn test_db() -> (Database, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("cybermanju-oplog-api-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(path.to_str().unwrap()).unwrap();
        (db, path)
    }

    fn device(id: &str, owner: &str) -> DeviceRow {
        DeviceRow {
            id: id.to_string(),
            name: id.to_string(),
            owner_id: owner.to_string(),
            public_key: String::new(),
            key_algorithm: "ml-dsa-65".to_string(),
            created_at: "2026-10-12T00:00:00Z".to_string(),
            last_seen_at: "2026-10-12T00:00:00Z".to_string(),
            revoked: false,
        }
    }

    fn op(device_id: &str, payload: &str) -> OpRow {
        OpRow {
            seq: 0,
            op_id: String::new(),
            device_id: device_id.to_string(),
            created_at: "2026-10-12T00:00:00Z".to_string(),
            kind: "rename".to_string(),
            file_id: Some("shared-file-id".to_string()),
            payload: serde_json::json!({ "value": payload }),
            bases: Default::default(),
        }
    }

    #[test]
    fn pull_and_conflicts_never_return_another_owners_ops() {
        let (db, path) = test_db();
        db.save_device(&device("device-a", "owner-a")).unwrap();
        db.save_device(&device("device-b", "owner-b")).unwrap();
        db.append_op(op("device-a", "private-a")).unwrap();
        db.append_op(op("device-b", "private-b")).unwrap();

        let pull = route(
            &db,
            "GET",
            &["api", "oplog", "since"],
            "since=0&limit=50",
            "",
            OplogAuth {
                owner_id: "owner-a",
                is_admin: false,
            },
            None,
        )
        .unwrap();
        assert!(pull.contains("private-a"));
        assert!(!pull.contains("private-b"));

        let conflicts = route(
            &db,
            "GET",
            &["api", "oplog", "conflicts"],
            "",
            "",
            OplogAuth {
                owner_id: "owner-a",
                is_admin: false,
            },
            None,
        )
        .unwrap();
        assert!(!conflicts.contains("private-b"));
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unsigned_push_and_cross_owner_resolution_fail_closed() {
        let (db, path) = test_db();
        db.save_device(&device("device-a", "owner-a")).unwrap();
        db.save_device(&device("device-b", "owner-b")).unwrap();
        let a = db.append_op(op("device-a", "a")).unwrap();
        let b = db.append_op(op("device-b", "b")).unwrap();

        let error = push(
            &db,
            &PushRequest {
                device_id: "device-a".to_string(),
                ops: vec![a.clone()],
                signature: String::new(),
                algorithm: "ml-dsa-65".to_string(),
            },
            "owner-a",
        )
        .unwrap_err();
        assert!(error.starts_with("auth:"));
        assert!(resolve_op(
            &db,
            &ResolveRequest {
                op_a: a.op_id,
                op_b: b.op_id,
                choice: "merge".to_string(),
                device_id: "device-a".to_string(),
            },
            "owner-a",
        )
        .unwrap_err()
        .starts_with("auth:"));
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
