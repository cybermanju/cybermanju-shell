// CyberManju OS — Schema (crates/types/src/schema.rs).
// Role: shared schemas: files, sync, agent, schedules.
// Exports: FileNode, Account, Collection, CollectionItem, FaceGroup, EncryptionKey, LooseGroup, User.
// Integrates: serde.
// Contracts: signals: engine.
use serde::{Deserialize, Serialize};

/// Vault file row: identity, crypto/compression state, tags, groups and GPS.
/// One `FileNode` per file; the Tantivy BM25 index and scene matcher read
/// `name` + `tags`, never the encrypted bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileNode {
    pub id: String,
    pub name: String,
    pub file_type: String,
    pub parent_id: Option<String>,
    pub size_bytes: u64,
    pub mime_type: Option<String>,
    pub hash_blake3: Option<String>,
    pub encrypted: bool,
    pub encryption_algorithm: Option<String>,
    pub compression_layers: Vec<String>,
    pub thumbnail_path: Option<String>,
    pub context_data: Option<serde_json::Value>,
    pub tags: Vec<String>,
    pub collection_ids: Vec<String>,
    pub face_group_ids: Vec<String>,
    pub loose_group_ids: Vec<String>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub created_at: String,
    pub modified_at: String,
}

/// Storage account row (local vault, NAS, cloud backend mount point).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub account_type: String,
    pub path: Option<String>,
    pub color: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Named file set (album / smart collection); membership lives in `item_ids`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub collection_type: String,
    pub color: String,
    pub description: Option<String>,
    pub item_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One file's membership in a [`Collection`], with an optional note.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionItem {
    pub id: String,
    pub collection_id: String,
    pub file_id: String,
    pub note: Option<String>,
    pub added_at: String,
}

/// Cluster of faces believed to be one person; naming it is how users label people.
/// Engine-labeled via `detection_engine` — never a fabricated detection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FaceGroup {
    pub id: String,
    pub name: String,
    pub file_ids: Vec<String>,
    pub centroid_embedding: Option<Vec<f32>>,
    pub binary_hash: Option<u64>,
    pub cohesion: Option<f32>,
    pub embedding_count: u32,
    pub algorithm: Option<String>,
    /// Which detector filled the group (`onnx` / `heuristic-v2` / mixed).
    /// `None` on rows written before engine reporting existed — readers
    /// must treat absence as "unknown", never assume ONNX.
    #[serde(default)]
    pub detection_engine: Option<String>,
    pub created_at: String,
}

/// Stored keypair row (ML-KEM-1024 + X25519 hybrid); private halves stay sealed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EncryptionKey {
    pub id: String,
    pub algorithm: String,
    pub public_key: String,
    pub private_key: String,
    pub label: Option<String>,
    pub created_at: String,
    /// Independent Hybrid X25519 static public key, base64 (P0-13). `None`
    /// on rows generated before the field existed and on non-Hybrid rows.
    #[serde(default)]
    pub x25519_public_key: Option<String>,
    /// Sealed (`sealed:v1:`) X25519 static secret, base64 (P0-13). Same
    /// at-rest posture as `private_key`.
    #[serde(default)]
    pub x25519_private_key: Option<String>,
}

/// Ad-hoc user group of files (lighter than a [`Collection`], no item rows).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LooseGroup {
    pub id: String,
    pub name: String,
    pub color: String,
    pub file_ids: Vec<String>,
    pub created_at: String,
}

/// Dashboard user row; `role` drives RBAC (`Claims{role}`), fail-closed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Per-file grant row: who may access what, and who granted it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserFilePermission {
    pub id: String,
    pub user_id: String,
    pub file_id: String,
    pub access: String,
    pub granted_by: String,
    pub granted_at: String,
}

/// GPS fix attached to a file (EXIF-derived); powers the map view.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub id: String,
    pub file_id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
    pub place_name: Option<String>,
    pub created_at: String,
}

/// Soft-deleted file snapshot; restore replays `original_file` + `restore_path`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrashItem {
    pub id: String,
    pub original_file: FileNode,
    pub deleted_at: String,
    pub deleted_by: Option<String>,
    pub restore_path: Option<String>,
}

/// Append-only audit row: every mutating dashboard action lands here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: String,
    pub user_id: Option<String>,
    pub details: Option<serde_json::Value>,
    pub timestamp: String,
}

/// Versioned content snapshot (1 MiB cap); saves bump `version_number`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileVersion {
    pub id: String,
    pub file_id: String,
    pub version_number: u32,
    pub hash_blake3: Option<String>,
    pub size_bytes: u64,
    pub snapshot_data: Option<String>,
    pub created_at: String,
}

/// Token-gated public download link; `url` is filled per-request by `with_url`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShareLink {
    pub id: String,
    pub file_id: String,
    pub token: String,
    pub expires_at: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl ShareLink {
    /// Fill `url` with a path relative to whatever origin served the request.
    /// A hardcoded absolute origin breaks every deployment that is not
    /// `localhost:3456` (Docker, reverse proxy, LAN).
    pub fn with_url(mut self) -> Self {
        self.url = Some(format!("/api/shared/{}", self.token));
        self
    }
}

/// Device identity row (§2B.1): one ML-DSA keypair per device plus a registry
/// entry. The private half never leaves the device (sealed in its keystore);
/// the row carries the name + public key so peers and the dashboard can
/// attribute ops, leases and bundles without trusting a bare hostname.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRow {
    pub id: String,
    pub name: String,
    /// Authenticated account that enrolled this device; empty legacy rows
    /// are not authorized for peer-sync or relay until explicitly claimed.
    #[serde(default)]
    pub owner_id: String,
    /// Base64 ML-DSA public key (empty when the device has not attested yet).
    #[serde(default)]
    pub public_key: String,
    /// `ml-dsa-65` today; the string travels so verification never assumes.
    #[serde(default)]
    pub key_algorithm: String,
    pub created_at: String,
    pub last_seen_at: String,
    /// Admin-revoked devices keep their rows (history) but fail closed.
    #[serde(default)]
    pub revoked: bool,
}

/// Server-side todo (§4.9 agent bridge): the same shape as the browser
/// `TodoItem` plus owner + source, so the agent (`todo_add`/`todo_done`)
/// and the Archive TASKS tab share one list per owner.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoRow {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub done: bool,
    pub created_at: String,
    #[serde(default)]
    pub done_at: Option<String>,
    #[serde(default)]
    pub owner_id: String,
    /// `local` (dashboard UI) or `agent:<config_id>`.
    #[serde(default)]
    pub source: String,
}

/// Op-log entry (§2C/P3-2): every mutation appends one row; devices exchange
/// rows using the version vectors `leases` already models. `seq` is the
/// local append order (u64, zero-padded key); `op_id` is globally unique
/// (`<device_id>-<seq>`); `bases` carries the writer's vector at append
/// time so a receiver can tell advance from fork without re-reading.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpRow {
    pub seq: u64,
    pub op_id: String,
    pub device_id: String,
    pub created_at: String,
    /// `create` | `edit` | `move` | `tag` | `star` | `rename` | `delete` |
    /// `face-rename` | `todo` … open vocabulary, unknown kinds merge as data.
    pub kind: String,
    #[serde(default)]
    pub file_id: Option<String>,
    #[serde(default)]
    pub payload: serde_json::Value,
    /// Writer's version vector (`{device: counter}`) at append time.
    #[serde(default)]
    pub bases: std::collections::BTreeMap<String, u64>,
}
