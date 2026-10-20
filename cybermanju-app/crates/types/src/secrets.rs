// CyberManju OS — secrets keystore rows (Phase 3, password manager).
//
// `SecretRow` is the metadata + sealed-value shape stored in the `secrets`
// table. The sealed value NEVER serializes in list/search responses — the
// `has_value` flag (the `hasKey` contract from agent configs) tells the UI
// whether a reveal is worth offering. Values are sealed with the existing
// `keystore::seal` (`seal:v1`: Argon2id + ChaCha20Poly1305) under the
// per-transport vault passphrase.

use serde::{Deserialize, Serialize};

/// Kinds of stored secrets — drives the icon + generator defaults in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretKind {
    /// Login (username + password).
    Login,
    /// Payment card (number + CVC — generator not offered).
    Card,
    /// Free-form secure note.
    Note,
    /// API token / key (generator offered, no username).
    ApiKey,
}

impl SecretKind {
    /// Wire spelling (`login`, `card`, `note`, `apiKey`) for a stored kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            SecretKind::Login => "login",
            SecretKind::Card => "card",
            SecretKind::Note => "note",
            SecretKind::ApiKey => "apiKey",
        }
    }

    /// Parse a user/API spelling into a kind; unknown input is `invalid:`.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "login" | "password" => Ok(SecretKind::Login),
            "card" | "payment" => Ok(SecretKind::Card),
            "note" | "securenote" => Ok(SecretKind::Note),
            "api" | "apikey" | "api_key" | "token" | "key" => Ok(SecretKind::ApiKey),
            other => Err(format!(
                "invalid: unknown secret kind '{other}' — login | card | note | apiKey"
            )),
        }
    }
}

/// One stored secret. `value_sealed` is the base64 `seal:v1` blob — it is
/// `#[serde(skip)]` on the public list/search responses (see
/// [`SecretMeta`]) and only opened by the explicit `reveal` path, which
/// writes an audit row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretRow {
    pub id: String,
    pub kind: SecretKind,
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
    pub favorite: bool,
    pub created_at: String,
    pub updated_at: String,
    /// Base64 `seal:v1` blob — NEVER serialized in list responses.
    #[serde(default)]
    pub value_sealed: String,
    /// Whether a value is stored (the `hasKey` contract). Always serialized.
    pub has_value: bool,
}

/// Metadata-only projection returned by list/search — structurally cannot
/// carry `value_sealed`, so a leak would be a compile error, not a review
/// miss.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretMeta {
    pub id: String,
    pub kind: SecretKind,
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
    pub favorite: bool,
    pub created_at: String,
    pub updated_at: String,
    pub has_value: bool,
}

impl From<&SecretRow> for SecretMeta {
    fn from(row: &SecretRow) -> Self {
        SecretMeta {
            id: row.id.clone(),
            kind: row.kind,
            title: row.title.clone(),
            username: row.username.clone(),
            url: row.url.clone(),
            category: row.category.clone(),
            tags: row.tags.clone(),
            notes: row.notes.clone(),
            favorite: row.favorite,
            created_at: row.created_at.clone(),
            updated_at: row.updated_at.clone(),
            has_value: row.has_value,
        }
    }
}

impl SecretRow {
    /// Project to the never-leaks metadata shape.
    pub fn meta(&self) -> SecretMeta {
        SecretMeta::from(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_never_carries_the_sealed_value() {
        let row = SecretRow {
            id: "s1".into(),
            kind: SecretKind::Login,
            title: "GitHub".into(),
            username: Some("user".into()),
            url: Some("https://github.com".into()),
            category: Some("work".into()),
            tags: vec!["dev".into()],
            notes: None,
            favorite: true,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            value_sealed: "seal:v1:secret".into(),
            has_value: true,
        };
        let meta = row.meta();
        let json = serde_json::to_string(&meta).expect("serialize meta");
        assert!(!json.contains("value_sealed"), "meta leaked value_sealed");
        assert!(!json.contains("seal:v1"), "meta leaked sealed bytes");
        assert!(json.contains("\"hasValue\":true"));
        assert_eq!(meta.kind, SecretKind::Login);
    }

    #[test]
    fn kind_parses_the_wire_spellings() {
        assert_eq!(SecretKind::parse("login").unwrap(), SecretKind::Login);
        assert_eq!(SecretKind::parse("apiKey").unwrap(), SecretKind::ApiKey);
        assert_eq!(SecretKind::parse("token").unwrap(), SecretKind::ApiKey);
        assert_eq!(SecretKind::parse("Note").unwrap(), SecretKind::Note);
        assert!(SecretKind::parse("card").is_ok());
        assert!(SecretKind::parse("bogus").is_err());
    }
}
