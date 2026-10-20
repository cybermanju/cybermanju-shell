// CyberManju OS — Sneakernet bundles (crates/sync/src/bundle.rs).
// Role: offline device↔device deltas on a USB stick (§2B.4) — a delta of
// content-addressed op-log rows, BLAKE3-manifested, importable anywhere.
// Genuinely decentralized: no server, no LAN, no account. Small file bytes
// ride *inside* the bundle (`payloads`, each BLAKE3-verified); larger files
// travel through the existing sync backends on next contact while the bundle
// carries the *operations* (create/edit/move/tag/star/…) so the receiver
// knows exactly what changed, with merge idempotency from `merge_ops` (same
// op_id twice is a no-op, never a duplicate).
// Exports: Bundle, BundlePayload, export_bundle, export_bundle_with_files,
// import_bundle, import_bundle_detailed, BUNDLE_FORMAT, BUNDLE_MAX_OPS,
// BUNDLE_MAX_PAYLOADS, BUNDLE_MAX_PAYLOAD_BYTES, BUNDLE_MAX_PAYLOAD_TOTAL.
// Integrates: sync crate (blake3 + base64 + db already deps); `sync bundle
// export|import` cybsh verbs and `POST /api/sync/bundle/*` ride on these.
// Contracts: `too_large:` beyond BUNDLE_MAX_OPS / payload caps; `integrity:`
// on marker or hash mismatch (ops manifest AND every payload hash);
// foreign files stay untouched (read-then-validate before any write);
// pre-payload bundles (no `payloads` field) import unchanged.

use cybermanju_db::Database;
use cybermanju_types::schema::OpRow;

/// Bundle envelope marker — import rejects anything else, loudly.
pub const BUNDLE_FORMAT: &str = "cyb-bundle/1";
/// Refuse absurd bundles before they touch memory (100k ops ≈ tens of MB).
pub const BUNDLE_MAX_OPS: usize = 100_000;
/// Most file payloads per bundle (bulk bytes ride the sync backends instead).
pub const BUNDLE_MAX_PAYLOADS: usize = 100;
/// Largest single file payload (5 MiB — the Drive multipart leg size).
pub const BUNDLE_MAX_PAYLOAD_BYTES: usize = 5 * 1024 * 1024;
/// Largest total payload section per bundle (50 MiB).
pub const BUNDLE_MAX_PAYLOAD_TOTAL: usize = 50 * 1024 * 1024;

/// One file's bytes inside a bundle: content-addressed by BLAKE3, base64 on
/// the wire (same alphabet as every other JSON blob here).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundlePayload {
    pub file_id: String,
    pub name: String,
    pub bytes_base64: String,
    pub blake3: String,
    pub size: u64,
}

/// One sneakernet delta.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub format: String,
    pub from_device: String,
    pub created_at: String,
    pub head: u64,
    pub ops: Vec<OpRow>,
    /// BLAKE3 hex of the canonical ops JSON (everything above minus this
    /// field) — tamper-evident in transit on untrusted sticks. Unchanged by
    /// the payload section (pre-payload bundles verify byte-identically).
    pub manifest_blake3: String,
    /// File bytes riding along (empty on ops-only / legacy bundles).
    #[serde(default)]
    pub payloads: Vec<BundlePayload>,
    /// BLAKE3 hex of the canonical payloads JSON (`[]` when empty) — a
    /// second tamper seal so one corrupt file cannot hide behind a valid
    /// ops manifest.
    #[serde(default)]
    pub payloads_blake3: String,
}

/// Import outcome with the verified payloads detached for the caller to
/// place (volume write, provider upload, …) — import itself never writes
/// file bytes, only merges ops.
#[derive(Debug, Clone)]
pub struct BundleImport {
    pub appended: usize,
    pub skipped: usize,
    pub from_device: String,
    pub payloads: Vec<BundlePayload>,
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn canonical_ops_json(ops: &[OpRow]) -> Result<String, String> {
    serde_json::to_string(ops).map_err(|e| format!("integrity: bundle ops unserializable: {e}"))
}

fn canonical_payloads_json(payloads: &[BundlePayload]) -> Result<String, String> {
    serde_json::to_string(payloads)
        .map_err(|e| format!("integrity: bundle payloads unserializable: {e}"))
}

/// Collect ops with `seq >= since` into an exportable bundle. `since` is the
/// next-sequence cursor; nothing is written — the caller places the bytes
/// (USB path, provider file, …).
pub fn export_bundle(db: &Database, device_id: &str, since: u64) -> Result<Bundle, String> {
    export_bundle_with_files(db, device_id, since, Vec::new())
}

/// Collect ops plus caller-supplied file bytes (`(file_id, name, bytes)`;
/// the caller reads them from the volume — this crate never touches file
/// storage, so no dependency cycle). Each payload is BLAKE3-sealed; oversize
/// payloads refuse with `too_large:` (ship them via the sync backends and
/// reference them from the ops instead).
pub fn export_bundle_with_files(
    db: &Database,
    device_id: &str,
    since: u64,
    files: Vec<(String, String, Vec<u8>)>,
) -> Result<Bundle, String> {
    use base64::Engine;
    if device_id.trim().is_empty() {
        return Err("invalid: device id is required".to_string());
    }
    let ops = db
        .list_ops_since(since, BUNDLE_MAX_OPS + 1)
        .map_err(|e| e.to_string())?;
    if ops.len() > BUNDLE_MAX_OPS {
        return Err(format!(
            "too_large: {} ops since {since} exceeds the {BUNDLE_MAX_OPS}-op bundle cap — export in slices",
            ops.len()
        ));
    }
    if files.len() > BUNDLE_MAX_PAYLOADS {
        return Err(format!(
            "too_large: bundle holds {} files (cap {BUNDLE_MAX_PAYLOADS} — ship bulk bytes via the sync backends)",
            files.len()
        ));
    }
    let mut total: usize = 0;
    let mut payloads = Vec::with_capacity(files.len());
    for (file_id, name, bytes) in files {
        validate_payload_component(&file_id, "fileId")?;
        validate_payload_component(&name, "name")?;
        if bytes.len() > BUNDLE_MAX_PAYLOAD_BYTES {
            return Err(format!(
                "too_large: payload '{file_id}' is {} bytes (cap {BUNDLE_MAX_PAYLOAD_BYTES} — ship it via the sync backends)",
                bytes.len()
            ));
        }
        total += bytes.len();
        if total > BUNDLE_MAX_PAYLOAD_TOTAL {
            return Err(format!(
                "too_large: bundle payloads exceed {BUNDLE_MAX_PAYLOAD_TOTAL} bytes total — split the bundle"
            ));
        }
        payloads.push(BundlePayload {
            file_id,
            name,
            blake3: blake3::hash(&bytes).to_hex().to_string(),
            size: bytes.len() as u64,
            bytes_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        });
    }
    let head = db.oplog_head().map_err(|e| e.to_string())?;
    let canonical = canonical_ops_json(&ops)?;
    let payloads_canon = canonical_payloads_json(&payloads)?;
    Ok(Bundle {
        format: BUNDLE_FORMAT.to_string(),
        from_device: device_id.to_string(),
        created_at: now_rfc3339(),
        head,
        ops,
        manifest_blake3: blake3::hash(canonical.as_bytes()).to_hex().to_string(),
        payloads_blake3: blake3::hash(payloads_canon.as_bytes()).to_hex().to_string(),
        payloads,
    })
}

/// Serialize a bundle for transport (USB file, provider object, …).
pub fn bundle_bytes(bundle: &Bundle) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(bundle).map_err(|e| format!("integrity: bundle unserializable: {e}"))
}

/// Validate + merge a received bundle. Returns (appended, skipped); the
/// receiver's own ops are never rewritten, only extended.
pub fn import_bundle(db: &Database, bytes: &[u8]) -> Result<(usize, usize, String), String> {
    let out = import_bundle_detailed(db, bytes)?;
    Ok((out.appended, out.skipped, out.from_device))
}

fn validate_payload_component(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 255
        || value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
        || value.chars().any(char::is_control)
    {
        return Err(format!(
            "invalid: bundle payload {field} must be a safe basename"
        ));
    }
    Ok(())
}

/// Validate + merge a received bundle, detaching the verified payloads for
/// the caller to place. Every payload is base64-decoded and re-hashed —
/// a single corrupt file fails the whole import with `integrity:` (fail
/// closed; nothing merges on a partial bundle).
pub fn import_bundle_detailed(db: &Database, bytes: &[u8]) -> Result<BundleImport, String> {
    use base64::Engine;
    if bytes.len() > 256 * 1024 * 1024 {
        return Err("too_large: bundle exceeds the 256 MiB import cap".to_string());
    }
    let bundle: Bundle =
        serde_json::from_slice(bytes).map_err(|e| format!("integrity: bundle unreadable: {e}"))?;
    if bundle.format != BUNDLE_FORMAT {
        return Err(format!(
            "integrity: not a sneakernet bundle (format '{}')",
            bundle.format
        ));
    }
    if bundle.ops.len() > BUNDLE_MAX_OPS {
        return Err(format!(
            "too_large: bundle holds {} ops (cap {BUNDLE_MAX_OPS})",
            bundle.ops.len()
        ));
    }
    if bundle.payloads.len() > BUNDLE_MAX_PAYLOADS {
        return Err(format!(
            "too_large: bundle holds {} payloads (cap {BUNDLE_MAX_PAYLOADS})",
            bundle.payloads.len()
        ));
    }
    let canonical = canonical_ops_json(&bundle.ops)?;
    let digest = blake3::hash(canonical.as_bytes()).to_hex().to_string();
    if digest != bundle.manifest_blake3 {
        return Err(
            "integrity: bundle manifest mismatch — the file changed in transit".to_string(),
        );
    }
    // Legacy bundles carry no payload seal: `payloads_blake3` defaults to ""
    // via serde. They verify exactly like before (ops manifest only).
    if !bundle.payloads.is_empty() || !bundle.payloads_blake3.is_empty() {
        let payloads_canon = canonical_payloads_json(&bundle.payloads)?;
        let payloads_digest = blake3::hash(payloads_canon.as_bytes()).to_hex().to_string();
        if payloads_digest != bundle.payloads_blake3 {
            return Err(
                "integrity: bundle payload manifest mismatch — a file changed in transit"
                    .to_string(),
            );
        }
    }
    // Check metadata and encoded lengths before allocating decoded payloads.
    let mut total_declared: u64 = 0;
    let mut seen_ids = std::collections::HashSet::new();
    for p in &bundle.payloads {
        validate_payload_component(&p.file_id, "fileId")?;
        validate_payload_component(&p.name, "name")?;
        if !seen_ids.insert(p.file_id.as_str()) {
            return Err("invalid: bundle contains duplicate payload fileId values".to_string());
        }
        if p.size > BUNDLE_MAX_PAYLOAD_BYTES as u64 {
            return Err(format!(
                "too_large: payload '{}' exceeds the {} byte per-file cap",
                p.file_id, BUNDLE_MAX_PAYLOAD_BYTES
            ));
        }
        let expected_b64_len = p.size.div_ceil(3) * 4;
        if p.bytes_base64.trim() != p.bytes_base64.as_str()
            || p.bytes_base64.len() as u64 != expected_b64_len
        {
            return Err(format!(
                "integrity: payload '{}' has an invalid encoded size",
                p.file_id
            ));
        }
        total_declared = total_declared
            .checked_add(p.size)
            .ok_or_else(|| "too_large: bundle payload byte count overflow".to_string())?;
        if total_declared > BUNDLE_MAX_PAYLOAD_TOTAL as u64 {
            return Err(format!(
                "too_large: bundle payloads exceed {BUNDLE_MAX_PAYLOAD_TOTAL} bytes total"
            ));
        }
    }

    let mut total: usize = 0;
    for p in &bundle.payloads {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&p.bytes_base64)
            .map_err(|e| format!("integrity: payload '{}' is not base64: {e}", p.file_id))?;
        if bytes.len() as u64 != p.size {
            return Err(format!(
                "integrity: payload '{}' declares {} bytes but carries {}",
                p.file_id,
                p.size,
                bytes.len()
            ));
        }
        let digest = blake3::hash(&bytes).to_hex().to_string();
        if digest != p.blake3.to_ascii_lowercase() {
            return Err(format!(
                "integrity: payload '{}' failed its BLAKE3 check — the file changed in transit",
                p.file_id
            ));
        }
        total += bytes.len();
        if total > BUNDLE_MAX_PAYLOAD_TOTAL {
            return Err(format!(
                "too_large: bundle payloads exceed {BUNDLE_MAX_PAYLOAD_TOTAL} bytes total"
            ));
        }
    }
    let (appended, skipped) = db.merge_ops(&bundle.ops).map_err(|e| e.to_string())?;
    Ok(BundleImport {
        appended,
        skipped,
        from_device: bundle.from_device,
        payloads: bundle.payloads,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str) -> (std::path::PathBuf, Database) {
        let dir = std::env::temp_dir().join(format!(
            "cybermanju-bundle-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let db = Database::new(dir.join("bundle.redb").to_str().unwrap()).expect("db");
        (dir, db)
    }

    fn op(device: &str, kind: &str) -> OpRow {
        OpRow {
            seq: 0,
            op_id: String::new(),
            device_id: device.to_string(),
            created_at: "2026-10-11T00:00:00Z".to_string(),
            kind: kind.to_string(),
            file_id: Some("f1".to_string()),
            payload: serde_json::json!({}),
            bases: Default::default(),
        }
    }

    #[test]
    fn bundle_round_trips_and_merges_idempotently() {
        let (_dir, a) = fixture("a");
        let (_dir, b) = fixture("b");
        a.append_op(op("laptop", "tag")).unwrap();
        a.append_op(op("laptop", "star")).unwrap();
        let bundle = export_bundle(&a, "laptop", 0).unwrap();
        assert_eq!(bundle.ops.len(), 2);
        let bytes = bundle_bytes(&bundle).unwrap();
        let (appended, skipped, from) = import_bundle(&b, &bytes).unwrap();
        assert_eq!((appended, skipped), (2, 0));
        assert_eq!(from, "laptop");
        // Re-import is a pure no-op (same op_ids).
        let (appended, skipped, _) = import_bundle(&b, &bytes).unwrap();
        assert_eq!((appended, skipped), (0, 2));
    }

    #[test]
    fn bundle_rejects_tampering_and_foreign_files() {
        let (_dir, a) = fixture("tamper");
        a.append_op(op("laptop", "tag")).unwrap();
        let mut bundle = export_bundle(&a, "laptop", 0).unwrap();
        bundle.ops.push(op("mallory", "delete"));
        let bytes = bundle_bytes(&bundle).unwrap();
        let err = import_bundle(&a, &bytes).expect_err("manifest");
        assert!(err.starts_with("integrity:"), "{err}");
        let err = import_bundle(&a, b"{\"format\":\"nope\"}").expect_err("format");
        assert!(err.starts_with("integrity:"), "{err}");
        let err = import_bundle(&a, b"not json").expect_err("json");
        assert!(err.starts_with("integrity:"), "{err}");
    }

    #[test]
    fn bundle_export_validates_device_and_slices() {
        let (_dir, a) = fixture("slices");
        assert!(export_bundle(&a, "  ", 0).is_err());
        a.append_op(op("laptop", "tag")).unwrap();
        let bundle = export_bundle(&a, "laptop", 0).unwrap();
        assert_eq!(bundle.head, 1);
        let empty = export_bundle(&a, "laptop", 99).unwrap();
        assert!(empty.ops.is_empty());
    }

    #[test]
    fn bundle_payloads_round_trip_verified() {
        let (_dir, a) = fixture("payloads-a");
        let (_dir, b) = fixture("payloads-b");
        a.append_op(op("laptop", "create")).unwrap();
        let bundle = export_bundle_with_files(
            &a,
            "laptop",
            0,
            vec![(
                "f1".to_string(),
                "photo.jpg".to_string(),
                b"file bytes here".to_vec(),
            )],
        )
        .expect("export");
        assert_eq!(bundle.payloads.len(), 1);
        assert_eq!(bundle.payloads[0].size, 15);
        let bytes = bundle_bytes(&bundle).expect("bytes");
        let out = import_bundle_detailed(&b, &bytes).expect("import");
        assert_eq!((out.appended, out.skipped), (1, 0));
        assert_eq!(out.payloads.len(), 1);
        assert_eq!(out.payloads[0].file_id, "f1");
        // Legacy path still merges (payloads detached for the caller).
        let (appended, _skipped, from) = import_bundle(&b, &bytes).expect("legacy import");
        assert_eq!(appended, 0);
        assert_eq!(from, "laptop");
    }

    #[test]
    fn bundle_payloads_reject_tampering_and_oversize() {
        let (_dir, a) = fixture("payloads-tamper");
        let mut bundle = export_bundle_with_files(
            &a,
            "laptop",
            0,
            vec![("f1".to_string(), "a.bin".to_string(), b"bytes".to_vec())],
        )
        .expect("export");
        // Corrupt the sealed bytes without touching the hash.
        bundle.payloads[0].bytes_base64 =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"boooo");
        let bytes = bundle_bytes(&bundle).expect("bytes");
        let err = import_bundle_detailed(&a, &bytes).expect_err("tamper");
        assert!(err.starts_with("integrity:"), "{err}");
        assert!(export_bundle_with_files(
            &a,
            "laptop",
            0,
            vec![(
                "f1".to_string(),
                "big".to_string(),
                vec![0u8; BUNDLE_MAX_PAYLOAD_BYTES + 1]
            )],
        )
        .unwrap_err()
        .starts_with("too_large:"));
        assert!(export_bundle_with_files(
            &a,
            "laptop",
            0,
            vec![("  ".to_string(), "x".to_string(), vec![])]
        )
        .unwrap_err()
        .starts_with("invalid:"));
    }

    #[test]
    fn import_rejects_oversize_metadata_and_unsafe_names_before_decode() {
        let (_dir, db) = fixture("payload-metadata");
        let make_bundle = || {
            export_bundle_with_files(
                &db,
                "laptop",
                0,
                vec![("f1".to_string(), "safe.bin".to_string(), b"ok".to_vec())],
            )
            .expect("export")
        };

        let mut oversize = make_bundle();
        oversize.payloads[0].size = (BUNDLE_MAX_PAYLOAD_BYTES + 1) as u64;
        let canonical = canonical_payloads_json(&oversize.payloads).expect("canonical payloads");
        oversize.payloads_blake3 = blake3::hash(canonical.as_bytes()).to_hex().to_string();
        let err = import_bundle_detailed(&db, &bundle_bytes(&oversize).expect("serialize"))
            .expect_err("oversize declared size");
        assert!(err.starts_with("too_large:"), "{err}");

        let mut traversal = make_bundle();
        traversal.payloads[0].name = "../escape.bin".to_string();
        let canonical = canonical_payloads_json(&traversal.payloads).expect("canonical payloads");
        traversal.payloads_blake3 = blake3::hash(canonical.as_bytes()).to_hex().to_string();
        let err = import_bundle_detailed(&db, &bundle_bytes(&traversal).expect("serialize"))
            .expect_err("unsafe payload name");
        assert!(err.starts_with("invalid:"), "{err}");
    }
}
