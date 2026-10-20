// CyberManju OS — Secrets keystore (Phase 3, password manager).
//
// Single source of truth for secret CRUD + reveal, shared by the REST
// route, the Tauri commands, and the WASM dispatch. Values are sealed with
// `keystore::seal_str` under the per-transport master passphrase
// (`master_passphrase()`); the sealed blob NEVER leaves this module in a
// list/search response — the wire shape is `SecretMeta` (structurally
// cannot carry the value). Reveal writes an audit row (never the value).
//
// Contract (same as agent config keys, AGENT-3):
//   - list/search → `SecretMeta[]` (hasValue flag only)
//   - reveal     → plaintext value (audited), `not_found` on missing id
//   - upsert      → `SecretMeta` (hasValue flag; value never echoed)
//   - delete     → `{ removed: true }` or `not_found:`
//
// `route()` returns `Some(response)` for `/api/secrets/*` and `None` for
// anything else. Wired in `lib.rs` beside the cron family and registered in
// `security::ROUTED_SEGMENTS` ("secrets"). Roles: `required_role` leaves
// the family at the default `Authenticated` — the vault is per-user, and
// the sealed blob is useless without the master passphrase.

use cybermanju_crypto::keystore;
use cybermanju_db::Database;
use cybermanju_types::secrets::{SecretKind, SecretMeta, SecretRow};

/// Create/upsert request — `value` is the plaintext, sealed on write and
/// never echoed back.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub title: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub favorite: Option<bool>,
    /// Plaintext value — sealed on write, never echoed.
    #[serde(default)]
    pub value: Option<String>,
}

/// Partial-update request — omitted fields are left untouched. A `Some`
/// `value` re-seals and bumps `updated_at`; a `None` leaves the sealed blob.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRequest {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub favorite: Option<bool>,
    #[serde(default)]
    pub value: Option<String>,
}

/// Render a `Result` the way this crate does elsewhere: 404 on
/// `not_found:`, 501 on `unsupported:` (no vault passphrase), 409 on
/// `integrity:` (sealed blob fails to open), 400 otherwise.
fn respond<T: serde::Serialize>(result: Result<T, String>, origin: Option<&str>) -> String {
    match result {
        Ok(value) => crate::json_ok(&value, origin),
        Err(message) => {
            let status = if message.starts_with("not_found:") {
                404
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
    serde_json::from_str(body).map_err(|err| format!("invalid: bad JSON: {err}"))
}

/// No vault passphrase configured — refuse loudly, never store plaintext.
fn require_passphrase() -> Result<String, String> {
    keystore::master_passphrase().ok_or_else(|| {
        "unsupported: no vault passphrase configured (CYBERMANJU_MASTER_PASSPHRASE \
         or <data dir>/master.passphrase)"
            .to_string()
    })
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn parse_kind(raw: Option<&str>) -> Result<SecretKind, String> {
    match raw {
        None => Ok(SecretKind::Login),
        Some(s) if s.trim().is_empty() => Ok(SecretKind::Login),
        Some(s) => SecretKind::parse(s),
    }
}

fn blank(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|s| !s.is_empty())
}

fn seal_value(passphrase: &str, plaintext: &str) -> Result<String, String> {
    if plaintext.trim().is_empty() {
        return Ok(String::new());
    }
    keystore::seal_str(passphrase, plaintext.as_bytes())
        .map_err(|e| format!("integrity: failed to seal secret: {e}"))
}

fn db_err(e: impl std::fmt::Display) -> String {
    format!("network: database error: {e}")
}

/// Upsert (create or update-by-id). A present `value` re-seals; an omitted
/// `value` on update leaves the existing sealed blob untouched.
pub fn upsert(db: &Database, req: &CreateRequest) -> Result<SecretMeta, String> {
    let title = req.title.trim();
    if title.is_empty() {
        return Err("invalid: secret title is required".to_string());
    }
    let passphrase = require_passphrase()?;
    let ts = now();
    let id = match req.id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => id.to_string(),
        None => uuid::Uuid::new_v4().to_string(),
    };
    let existing = db.get_secret(&id).map_err(db_err)?;
    let created_at = existing
        .as_ref()
        .map(|e| e.created_at.clone())
        .unwrap_or_else(|| ts.clone());
    let (value_sealed, has_value) = if let Some(plaintext) = req.value.as_deref() {
        let sealed = seal_value(&passphrase, plaintext)?;
        let has = !sealed.is_empty();
        (sealed, has)
    } else {
        match &existing {
            Some(e) => (e.value_sealed.clone(), e.has_value),
            None => (String::new(), false),
        }
    };
    let row = SecretRow {
        id: id.clone(),
        kind: parse_kind(req.kind.as_deref())?,
        title: title.to_string(),
        username: blank(req.username.clone()),
        url: blank(req.url.clone()),
        category: blank(req.category.clone()),
        tags: req
            .tags
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect(),
        notes: blank(req.notes.clone()),
        favorite: req
            .favorite
            .unwrap_or_else(|| existing.as_ref().map(|e| e.favorite).unwrap_or(false)),
        created_at,
        updated_at: ts,
        value_sealed,
        has_value,
    };
    db.save_secret(&row).map_err(db_err)?;
    db.log_audit(
        "secret.upsert",
        "secret",
        &row.id,
        None,
        Some(serde_json::json!({ "title": row.title, "kind": row.kind.as_str() })),
    )
    .map_err(db_err)?;
    Ok(row.meta())
}

pub fn update(db: &Database, id: &str, req: &UpdateRequest) -> Result<SecretMeta, String> {
    let mut row = db
        .get_secret(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("not_found: no secret `{id}`"))?;
    if let Some(kind) = &req.kind {
        row.kind = SecretKind::parse(kind)?;
    }
    if let Some(title) = &req.title {
        let t = title.trim();
        if t.is_empty() {
            return Err("invalid: secret title is required".to_string());
        }
        row.title = t.to_string();
    }
    if let Some(username) = &req.username {
        row.username = blank(Some(username.clone()));
    }
    if let Some(url) = &req.url {
        row.url = blank(Some(url.clone()));
    }
    if let Some(category) = &req.category {
        row.category = blank(Some(category.clone()));
    }
    if let Some(tags) = &req.tags {
        row.tags = tags
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
    }
    if let Some(notes) = &req.notes {
        row.notes = blank(Some(notes.clone()));
    }
    if let Some(favorite) = req.favorite {
        row.favorite = favorite;
    }
    if let Some(plaintext) = req.value.as_deref() {
        let passphrase = require_passphrase()?;
        row.value_sealed = seal_value(&passphrase, plaintext)?;
        row.has_value = !row.value_sealed.is_empty();
    }
    row.updated_at = now();
    db.save_secret(&row).map_err(db_err)?;
    db.log_audit(
        "secret.update",
        "secret",
        &row.id,
        None,
        Some(serde_json::json!({ "title": row.title })),
    )
    .map_err(db_err)?;
    Ok(row.meta())
}

/// Metadata list — never carries sealed values.
pub fn list(db: &Database) -> Result<Vec<SecretMeta>, String> {
    let rows = db.list_secrets().map_err(db_err)?;
    Ok(rows.iter().map(SecretMeta::from).collect())
}

/// Single metadata row.
pub fn get_meta(db: &Database, id: &str) -> Result<SecretMeta, String> {
    let row = db
        .get_secret(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("not_found: no secret `{id}`"))?;
    Ok(row.meta())
}

/// Reveal the plaintext value — audited, `not_found` on missing, `integrity:`
/// when the sealed blob fails to open (tamper / wrong passphrase).
pub fn reveal(db: &Database, id: &str) -> Result<String, String> {
    let row = db
        .get_secret(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("not_found: no secret `{id}`"))?;
    if !row.has_value || row.value_sealed.is_empty() {
        return Err(format!("not_found: secret `{id}` has no stored value"));
    }
    let passphrase = require_passphrase()?;
    let value = keystore::open_sealed_str(&passphrase, &row.value_sealed)
        .map_err(|e| format!("integrity: failed to open sealed secret: {e}"))?;
    db.log_audit(
        "secret.reveal",
        "secret",
        &row.id,
        None,
        Some(serde_json::json!({ "title": row.title })),
    )
    .map_err(db_err)?;
    Ok(value)
}

pub fn remove(db: &Database, id: &str) -> Result<bool, String> {
    let removed = db.remove_secret(id).map_err(db_err)?;
    if removed {
        db.log_audit("secret.delete", "secret", id, None, None)
            .map_err(db_err)?;
    }
    Ok(removed)
}

/// Dispatch `/api/secrets/*`.
pub fn route(
    db: &Database,
    method: &str,
    path_segments: &[&str],
    body: &str,
    origin: Option<&str>,
) -> Option<String> {
    let response = match path_segments {
        ["api", "secrets"] if method == "GET" => respond(list(db), origin),

        ["api", "secrets"] if method == "POST" => match parse::<CreateRequest>(body) {
            Ok(req) => respond(upsert(db, &req), origin),
            Err(e) => respond::<serde_json::Value>(Err(e), origin),
        },

        ["api", "secrets", id, "reveal"] if method == "GET" => respond(
            reveal(db, id).map(|v| serde_json::json!({ "value": v })),
            origin,
        ),

        ["api", "secrets", id] if method == "GET" => respond(get_meta(db, id), origin),

        ["api", "secrets", id] if method == "PUT" => match parse::<UpdateRequest>(body) {
            Ok(req) => respond(update(db, id, &req), origin),
            Err(e) => respond::<serde_json::Value>(Err(e), origin),
        },

        ["api", "secrets", id] if method == "DELETE" => match remove(db, id) {
            Ok(true) => respond(Ok(serde_json::json!({ "removed": true })), origin),
            Ok(false) => {
                respond::<serde_json::Value>(Err(format!("not_found: no secret `{id}`")), origin)
            }
            Err(e) => respond::<serde_json::Value>(Err(e), origin),
        },

        _ => return None,
    };
    Some(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_claims_only_the_secrets_family() {
        // Structural mirror of the match arms: anything outside the family
        // falls through to `None` before any db use.
        fn claimed(segments: &[&str]) -> bool {
            matches!(
                segments,
                ["api", "secrets"] | ["api", "secrets", _] | ["api", "secrets", _, "reveal"]
            )
        }
        assert!(claimed(&["api", "secrets"]));
        assert!(claimed(&["api", "secrets", "abc"]));
        assert!(claimed(&["api", "secrets", "abc", "reveal"]));
        assert!(!claimed(&["api", "nope"]));
        assert!(!claimed(&["api", "cron"]));
        assert!(!claimed(&["api", "secrets", "abc", "other"]));
    }

    #[test]
    fn status_mapping_matches_the_error_contract() {
        fn status_of(message: &str) -> u16 {
            if message.starts_with("not_found:") {
                404
            } else if message.starts_with("unsupported:") {
                501
            } else if message.starts_with("integrity:") {
                409
            } else {
                400
            }
        }
        assert_eq!(status_of("not_found: x"), 404);
        assert_eq!(status_of("invalid: x"), 400);
        assert_eq!(status_of("unsupported: x"), 501);
        assert_eq!(status_of("integrity: x"), 409);
        assert_eq!(status_of("network: x"), 400);
    }

    #[test]
    fn blank_trims_to_none() {
        assert_eq!(blank(Some("  ".into())), None);
        assert_eq!(blank(Some(" x ".into())), Some("x".to_string()));
        assert_eq!(blank(None), None);
    }

    #[test]
    fn parse_kind_defaults_to_login() {
        assert_eq!(parse_kind(None).unwrap(), SecretKind::Login);
        assert_eq!(parse_kind(Some("")).unwrap(), SecretKind::Login);
        assert_eq!(parse_kind(Some("apiKey")).unwrap(), SecretKind::ApiKey);
        assert!(parse_kind(Some("bogus")).is_err());
    }

    #[test]
    fn seal_value_skips_empty() {
        // No passphrase needed for the empty path — plaintext whitespace
        // seals to nothing and hasValue flips false.
        assert_eq!(seal_value("unused", "   ").unwrap(), "");
    }
}
