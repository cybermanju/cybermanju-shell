// CyberManju OS — Signed peer-sync envelopes (crates/sync/src/auth.rs).
// Role: device-attested op-log exchange (§2B.1/§2B.2) — the sender signs the
// canonical ops JSON with its ML-DSA device seed (private half sealed in its
// keystore, never transmitted); the receiver verifies against the sender's
// `public_key` + `key_algorithm` in the device registry, so a valid JWT alone
// is no longer enough to impersonate a device.
// Exports: ENVELOPE_DOMAIN, canonical_envelope_bytes, sign_envelope,
// verify_envelope, SignedSyncBody.
// Integrates: sync crate (cybermanju-crypto + types already deps);
// `POST /api/peers/{id}/sync` requires `signature`+`algorithm` and reports
// `verified`; the TS twin is `src/utils/p2pPresence.ts`
// (`canonicalOpsEnvelope`, `SignedSyncBody`); signing on desktop rides the
// `sign_peer_ops_for_device` Tauri command, on Pages the wasm `ml_dsa65_sign`.
// Contracts: canonical bytes are `ENVELOPE_DOMAIN + serde_json(ops)` on every
// transport (no whitespace/pretty variants — `serde_json::to_string` only);
// `invalid:` on bad inputs, `auth:` on missing/mismatched/invalid signatures,
// never a panic, never a key logged.
use cybermanju_types::schema::OpRow;

/// Domain separation between a sync envelope and every other signed blob
/// (bundles, challenges, files). Both sides prepend these exact bytes.
pub const ENVELOPE_DOMAIN: &str = "cybermanju-peer-sync/v1:";

/// Wire body for `POST /api/peers/{id}/sync`: the ops plus a mandatory
/// device attestation. Missing `signature`/`algorithm` is rejected.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedSyncBody {
    #[serde(default)]
    pub ops: Vec<OpRow>,
    #[serde(default)]
    pub signature: String,
    #[serde(default)]
    pub algorithm: String,
}

/// Canonical bytes covered by the signature: domain + compact ops JSON.
/// Pure — the TS twin `canonicalOpsEnvelope` pins the identical string.
///
/// Cross-language determinism: ops round-trip through `serde_json::Value`
/// first, so nested objects serialize with sorted (BTreeMap) keys — the same
/// order the TS `stableStringify` twin emits. Top-level op keys keep struct
/// order (`seq,opId,deviceId,createdAt,kind,fileId,payload,bases`), which the
/// TS twin constructs explicitly. Never pretty-print here: whitespace is
/// part of the signed bytes.
pub fn canonical_envelope_bytes(ops: &[OpRow]) -> Result<Vec<u8>, String> {
    let value =
        serde_json::to_value(ops).map_err(|e| format!("integrity: ops unserializable: {e}"))?;
    let json =
        serde_json::to_string(&value).map_err(|e| format!("integrity: ops unserializable: {e}"))?;
    let mut out = Vec::with_capacity(ENVELOPE_DOMAIN.len() + json.len());
    out.extend_from_slice(ENVELOPE_DOMAIN.as_bytes());
    out.extend_from_slice(json.as_bytes());
    Ok(out)
}

/// Sign `ops` with a device seed (base64 ML-DSA seed, the `private_key` half
/// `generate_keypair` stores). Returns the base64 signature — the caller
/// places it in `SignedSyncBody.signature` with the algorithm name.
pub fn sign_envelope(seed_b64: &str, algorithm: &str, ops: &[OpRow]) -> Result<String, String> {
    use base64::Engine;
    let algo = cybermanju_crypto::pqc::algorithm_from_str(algorithm.trim())
        .ok_or_else(|| format!("invalid: unknown signature algorithm '{algorithm}'"))?;
    let seed = base64::engine::general_purpose::STANDARD
        .decode(seed_b64.trim())
        .map_err(|e| format!("invalid: device seed is not base64: {e}"))?;
    let keypair = cybermanju_crypto::pqc::KeyPair {
        id: String::new(),
        algorithm: algo,
        public_key: Vec::new(),
        private_key: seed,
        x25519_public_key: None,
        x25519_private_key: None,
        created_at: String::new(),
    };
    let message = canonical_envelope_bytes(ops)?;
    let sig = cybermanju_crypto::pqc::sign_message(&message, &keypair)
        .map_err(|e| format!("invalid: signing failed: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&sig))
}

/// Verify a received envelope against the sender registry row. Returns
/// `Ok(true)` only when a mandatory signature matches the registered public
/// key and algorithm; all missing or mismatched attestations fail closed.
pub fn verify_envelope(
    ops: &[OpRow],
    signature: &str,
    algorithm: &str,
    registry_public_key_b64: &str,
    registry_algorithm: &str,
) -> Result<bool, String> {
    if signature.trim().is_empty() {
        return Err("auth: signed peer-sync attestation is required".to_string());
    }
    if registry_public_key_b64.trim().is_empty() {
        return Err(
            "auth: sender supplied a signature but this device never attested a public key — re-enroll it via POST /api/devices first"
                .to_string(),
        );
    }
    let supplied =
        cybermanju_crypto::pqc::algorithm_from_str(algorithm.trim()).ok_or_else(|| {
            "auth: peer-sync signature algorithm is required or unsupported".to_string()
        })?;
    let registered = cybermanju_crypto::pqc::algorithm_from_str(registry_algorithm.trim())
        .ok_or_else(|| "auth: device registry has no supported signature algorithm".to_string())?;
    if supplied != registered {
        return Err(
            "auth: peer-sync signature algorithm does not match the registered device key"
                .to_string(),
        );
    }
    let message = canonical_envelope_bytes(ops)?;
    let valid = cybermanju_crypto::pqc::verify_with_public_key(
        &message,
        signature,
        registry_public_key_b64,
        registry_algorithm,
    )
    .map_err(|e| {
        if e.starts_with("auth:") || e.starts_with("invalid:") || e.starts_with("integrity:") {
            e
        } else {
            format!("auth: signature check failed: {e}")
        }
    })?;
    if valid {
        Ok(true)
    } else {
        Err("auth: peer-sync signature mismatch — the ops changed in transit or the sender is not who it claims".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn device_seed() -> (String, String) {
        use base64::Engine;
        let mut engine = cybermanju_crypto::pqc::PqcEngine::new();
        let kp = engine
            .generate_keypair(cybermanju_crypto::pqc::EncryptionAlgo::MlDsa65)
            .expect("keygen");
        let seed_b64 = base64::engine::general_purpose::STANDARD.encode(&kp.private_key);
        let pub_b64 = base64::engine::general_purpose::STANDARD.encode(&kp.public_key);
        (seed_b64, pub_b64)
    }

    #[test]
    fn envelope_sign_verify_round_trip() {
        let (seed, publ) = device_seed();
        let ops = vec![op("laptop", "tag"), op("laptop", "star")];
        let sig = sign_envelope(&seed, "ml-dsa-65", &ops).expect("sign");
        assert!(verify_envelope(&ops, &sig, "ml-dsa-65", &publ, "ml-dsa-65").expect("verify"));
        // Canonical bytes are domain-separated and deterministic.
        let bytes = canonical_envelope_bytes(&ops).expect("canon");
        assert!(bytes.starts_with(ENVELOPE_DOMAIN.as_bytes()));
        assert_eq!(bytes, canonical_envelope_bytes(&ops).expect("canon"));
        // Pinned cross-language vector (mirrored by the TS twin test): key
        // order is struct order at the top level, sorted inside maps.
        let fixed = vec![OpRow {
            seq: 7,
            op_id: "laptop-7".to_string(),
            device_id: "laptop".to_string(),
            created_at: "2026-10-11T00:00:00Z".to_string(),
            kind: "tag".to_string(),
            file_id: Some("f1".to_string()),
            payload: serde_json::json!({"z": 1, "a": [3, 2]}),
            bases: [("laptop".to_string(), 7u64)].into_iter().collect(),
        }];
        let canon =
            String::from_utf8(canonical_envelope_bytes(&fixed).expect("canon")).expect("utf8");
        assert_eq!(
            canon,
            "cybermanju-peer-sync/v1:[{\"bases\":{\"laptop\":7},\"createdAt\":\"2026-10-11T00:00:00Z\",\"deviceId\":\"laptop\",\"fileId\":\"f1\",\"kind\":\"tag\",\"opId\":\"laptop-7\",\"payload\":{\"a\":[3,2],\"z\":1},\"seq\":7}]"
        );
    }

    #[test]
    fn envelope_verify_fails_closed() {
        let (seed, publ) = device_seed();
        let ops = vec![op("laptop", "tag")];
        let sig = sign_envelope(&seed, "ml-dsa-65", &ops).expect("sign");
        // Unsigned legacy senders are rejected rather than merged.
        assert!(verify_envelope(&ops, "", "", &publ, "ml-dsa-65")
            .unwrap_err()
            .starts_with("auth:"));
        // A valid signature cannot select an algorithm different from the
        // one bound to the registered public key.
        let err = verify_envelope(&ops, &sig, "ml-dsa-44", &publ, "ml-dsa-65")
            .expect_err("algorithm mismatch");
        assert!(err.starts_with("auth:"), "{err}");
        // Tampered ops fail with auth:, never merge.
        let mut tampered = ops.clone();
        tampered.push(op("mallory", "delete"));
        let err =
            verify_envelope(&tampered, &sig, "ml-dsa-65", &publ, "ml-dsa-65").expect_err("tamper");
        assert!(err.starts_with("auth:"), "{err}");
        // Signature with no registry key fails with auth:, never a guess.
        let err = verify_envelope(&ops, &sig, "ml-dsa-65", "", "ml-dsa-65").expect_err("nokey");
        assert!(err.starts_with("auth:"), "{err}");
        assert!(sign_envelope("!!!", "ml-dsa-65", &ops)
            .unwrap_err()
            .starts_with("invalid:"));
    }
}
