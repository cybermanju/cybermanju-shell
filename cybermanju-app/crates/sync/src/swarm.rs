// CyberManju OS — Erasure-coded relay swarming (crates/sync/src/swarm.rs).
// Role: bulk bytes over lossy paths (§2B.4 follow-up) — a payload is
// Reed–Solomon coded with the in-tree `cybermanju-erasure` codec (framed, so
// decoders never need the original length) and sprayed as self-describing
// symbols across relay messages (`kind: swarm-symbol`), possibly from
// several peers at once. Any `k` of the `k + m` symbols reconstruct the
// payload byte-identically — a dropped relay message is a non-event, not a
// failed transfer. A fountain-code upgrade (RaptorQ, RFC 6330) stays
// sequenced for GB scale; RS is exact and dependency-free at bundle scale.
// Exports: SwarmSymbol, SWARM_KIND, swarm_layout, swarm_encode,
// swarm_decode, swarm_symbol_json.
// Integrates: sync crate (erasure + blake3 + base64 already deps); symbols
// ride `POST /api/relay/drop` like every other envelope (256 KiB cap per
// message); the TS twin is `src/utils/p2pSwarm.ts` (same JSON, same caps;
// RS math runs natively and in wasm via `swarm_encode_json`).
// Contracts: every symbol carries its layout + object hash + own hash, so a
// mixed/foreign/corrupt subset fails closed with `integrity:` (never a
// mis-decode); `too_large:` beyond the bundle payload caps; never a panic.
use cybermanju_erasure::{decode_framed, encode_framed};

/// Relay `kind` for swarm symbols (one symbol per message).
pub const SWARM_KIND: &str = "swarm-symbol";

/// Target shard size for relay symbols (64 KiB — each symbol stays far
/// under the 256 KiB relay cap even after base64).
pub const SWARM_TARGET_SHARD: u64 = 64 * 1024;

/// One coded symbol: self-describing (`k`, `m`, index, object hash) so any
/// subset collected from any peers decodes without side state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwarmSymbol {
    /// BLAKE3 hex of the original (pre-code) payload — the object identity.
    pub object: String,
    /// Data shards (`k`: any `k` symbols decode).
    pub data_shards: u8,
    /// Parity shards (`m`).
    pub parity_shards: u8,
    /// This symbol's index (`0..k+m`).
    pub index: u8,
    /// Bytes per shard (all symbols of one object agree).
    pub shard_len: usize,
    /// BLAKE3 hex of the decoded shard bytes (per-symbol tamper seal).
    pub blake3: String,
    /// The shard bytes, base64.
    pub bytes_base64: String,
}

/// Default `(k, m)` layout for a payload: shards near 64 KiB, `m ≈ k/2`
/// (at least 1, at most 8), always inside the GF(2^8) field.
pub fn swarm_layout(payload_len: u64) -> (u8, u8) {
    let framed = payload_len.saturating_add(cybermanju_erasure::FRAME_HEADER_LEN as u64);
    let k = framed.div_ceil(SWARM_TARGET_SHARD).clamp(1, 32) as u8;
    let m = (k.div_ceil(2)).clamp(1, 8);
    debug_assert!((k as usize) + (m as usize) <= cybermanju_erasure::MAX_TOTAL_SHARDS);
    (k, m)
}

/// Encode a payload into `k + m` relay-ready symbols. Payloads beyond the
/// bundle payload cap refuse with `too_large:` (ship them via the sync
/// backends); a symbol that would overflow a relay message refuses the same
/// way (raise `k` — smaller shards — and retry).
pub fn swarm_encode(payload: &[u8], data: u8, parity: u8) -> Result<Vec<SwarmSymbol>, String> {
    use base64::Engine;
    if payload.len() as u64 > super::bundle::BUNDLE_MAX_PAYLOAD_TOTAL as u64 {
        return Err(format!(
            "too_large: swarm object is {} bytes (cap {} — ship it via the sync backends)",
            payload.len(),
            super::bundle::BUNDLE_MAX_PAYLOAD_TOTAL
        ));
    }
    let encoded = encode_framed(payload, data, parity).map_err(|e| e.to_string())?;
    let object = blake3::hash(payload).to_hex().to_string();
    let shard_len = encoded.shard_len();
    let mut out = Vec::with_capacity(encoded.shards.len());
    for (index, shard) in encoded.shards.iter().enumerate() {
        let symbol = SwarmSymbol {
            object: object.clone(),
            data_shards: encoded.data_shards,
            parity_shards: encoded.parity_shards,
            index: index as u8,
            shard_len,
            blake3: blake3::hash(shard).to_hex().to_string(),
            bytes_base64: base64::engine::general_purpose::STANDARD.encode(shard),
        };
        let wire = swarm_symbol_json(&symbol)?;
        if wire.len() > super::peers::RELAY_MAX_BYTES {
            return Err(format!(
                "too_large: swarm symbol {index} is {} bytes (cap {} — raise k for smaller shards)",
                wire.len(),
                super::peers::RELAY_MAX_BYTES
            ));
        }
        out.push(symbol);
    }
    Ok(out)
}

/// Serialize one symbol for the relay wire (what `swarm_encode` size-checks).
pub fn swarm_symbol_json(symbol: &SwarmSymbol) -> Result<String, String> {
    serde_json::to_string(symbol).map_err(|e| format!("integrity: symbol unserializable: {e}"))
}

/// Decode a payload from collected symbols (any `k` distinct indices of one
/// object). Every symbol is base64-decoded + re-hashed first — a corrupt,
// foreign, or mixed subset fails with `integrity:`, never a mis-decode —
// then the whole object hash is re-checked before returning.
pub fn swarm_decode(symbols: &[SwarmSymbol]) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let first = symbols
        .first()
        .ok_or_else(|| "invalid: swarm decode needs at least one symbol".to_string())?;
    let (k, m) = (first.data_shards, first.parity_shards);
    let mut present: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut seen: std::collections::HashSet<u8> = Default::default();
    for s in symbols {
        if s.object != first.object
            || s.data_shards != k
            || s.parity_shards != m
            || s.shard_len != first.shard_len
        {
            return Err(
                "integrity: swarm symbols mix objects or layouts — collect one object's symbols"
                    .to_string(),
            );
        }
        if s.index as usize >= k as usize + m as usize {
            return Err(format!(
                "integrity: swarm symbol index {} is out of range",
                s.index
            ));
        }
        if !seen.insert(s.index) {
            continue;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(s.bytes_base64.trim())
            .map_err(|e| format!("integrity: swarm symbol {} is not base64: {e}", s.index))?;
        if bytes.len() != s.shard_len {
            return Err(format!(
                "integrity: swarm symbol {} declares {} bytes but carries {}",
                s.index,
                s.shard_len,
                bytes.len()
            ));
        }
        if blake3::hash(&bytes).to_hex().to_string() != s.blake3.to_ascii_lowercase() {
            return Err(format!(
                "integrity: swarm symbol {} failed its BLAKE3 check",
                s.index
            ));
        }
        present.push((s.index, bytes));
    }
    if present.len() < k as usize {
        return Err(format!(
            "integrity: swarm decode needs {} symbols, only {} usable",
            k,
            present.len()
        ));
    }
    let payload = decode_framed(k, m, &present).map_err(|e| e.to_string())?;
    if blake3::hash(&payload).to_hex().to_string() != first.object {
        return Err("integrity: swarm object hash mismatch after decode".to_string());
    }
    Ok(payload)
}

// ─── RaptorQ fountain symbols (RFC 6330, GB-scale upgrade path) ──
//
// Where the RS path above needs exactly `k` of `k + m`, the fountain path
// needs *any* `K + h` symbols (`h` usually 0–2 — reconstruction probability
// `1 - 1/256^(h+1)`): the sender sprays `K + repair` symbols across relay
// messages (possibly via several peers at once) and the receiver decodes
// from whatever arrives first. Same relay `drop`/`poll` transport, same
// BLAKE3 seals, new `kind: fountain-symbol`. Pure `raptorq` (no async
// runtime, no C bindings) — the TS twin validates envelopes in
// `p2pSwarm.ts`; the math runs natively here and in wasm.

/// Relay `kind` for fountain symbols.
pub const FOUNTAIN_KIND: &str = "fountain-symbol";

/// Default RaptorQ symbol size for relay spray (~32 KiB symbols stay far
/// under the 256 KiB relay cap after base64).
pub const FOUNTAIN_DEFAULT_MTU: u16 = 32768;

/// One fountain symbol: self-describing (object hash + OTI travel with
/// every symbol) so subsets collected from any peers decode without state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FountainSymbol {
    /// BLAKE3 hex of the original (pre-code) payload — the object identity.
    pub object: String,
    /// `oti.serialize()` (12 bytes) base64 — identical on every symbol.
    pub oti_base64: String,
    /// Encoder MTU the symbols were produced with.
    pub mtu: u16,
    /// BLAKE3 hex of the serialized packet bytes (per-symbol tamper seal).
    pub blake3: String,
    /// `packet.serialize()` bytes, base64.
    pub bytes_base64: String,
}

/// Encode a payload into `K + repair` fountain symbols. `mtu` sizes symbols
/// (`0` selects the default); `repair_packets` is the extra spray beyond
/// the source symbols (lossy paths want `repair ≈ K`; pass an explicit
/// count — fountain codes never run out). Caps mirror the RS path.
pub fn fountain_encode(
    payload: &[u8],
    mtu: u16,
    repair_packets: u32,
) -> Result<Vec<FountainSymbol>, String> {
    use base64::Engine;
    if payload.is_empty() {
        return Err("invalid: fountain encode needs a non-empty payload".to_string());
    }
    if mtu == 0 {
        return Err("invalid: fountain MTU must be 1..=65535".to_string());
    }
    if payload.len() as u64 > super::bundle::BUNDLE_MAX_PAYLOAD_TOTAL as u64 {
        return Err(format!(
            "too_large: fountain object is {} bytes (cap {} — ship it via the sync backends)",
            payload.len(),
            super::bundle::BUNDLE_MAX_PAYLOAD_TOTAL
        ));
    }
    let encoder = raptorq::Encoder::with_defaults(payload, mtu);
    let oti = encoder.get_config();
    let oti_b64 = base64::engine::general_purpose::STANDARD.encode(oti.serialize());
    let packets = encoder.get_encoded_packets(repair_packets);
    if packets.is_empty() {
        return Err("integrity: fountain encoder produced no symbols".to_string());
    }
    let object = blake3::hash(payload).to_hex().to_string();
    let mut out = Vec::with_capacity(packets.len());
    for packet in &packets {
        let wire = packet.serialize();
        let symbol = FountainSymbol {
            object: object.clone(),
            oti_base64: oti_b64.clone(),
            mtu,
            blake3: blake3::hash(&wire).to_hex().to_string(),
            bytes_base64: base64::engine::general_purpose::STANDARD.encode(&wire),
        };
        let envelope = serde_json::to_string(&symbol)
            .map_err(|e| format!("integrity: symbol unserializable: {e}"))?;
        if envelope.len() > super::peers::RELAY_MAX_BYTES {
            return Err(format!(
                "too_large: fountain symbol is {} bytes (cap {} — lower the MTU for smaller symbols)",
                envelope.len(),
                super::peers::RELAY_MAX_BYTES
            ));
        }
        out.push(symbol);
    }
    Ok(out)
}

/// Decode a payload from collected fountain symbols (any sufficient subset,
/// any order, any peers). Every symbol is base64-decoded + re-hashed first;
/// a corrupt, foreign, or mixed subset fails with `integrity:`, never a
/// mis-decode. Feeding stops at the first complete decode.
pub fn fountain_decode(symbols: &[FountainSymbol]) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let first = symbols
        .first()
        .ok_or_else(|| "invalid: fountain decode needs at least one symbol".to_string())?;
    let oti_raw = base64::engine::general_purpose::STANDARD
        .decode(first.oti_base64.trim())
        .map_err(|e| format!("integrity: fountain OTI is not base64: {e}"))?;
    let oti_arr: [u8; 12] = oti_raw
        .as_slice()
        .try_into()
        .map_err(|_| "integrity: fountain OTI is not 12 bytes".to_string())?;
    let oti = raptorq::ObjectTransmissionInformation::deserialize(&oti_arr);
    let mut wires = Vec::with_capacity(symbols.len());
    for s in symbols {
        if s.object != first.object || s.oti_base64 != first.oti_base64 {
            return Err(
                "integrity: fountain symbols mix objects or code params — collect one object's symbols"
                    .to_string(),
            );
        }
        let wire = base64::engine::general_purpose::STANDARD
            .decode(s.bytes_base64.trim())
            .map_err(|e| format!("integrity: fountain symbol is not base64: {e}"))?;
        if blake3::hash(&wire).to_hex().to_string() != s.blake3.to_ascii_lowercase() {
            return Err("integrity: fountain symbol failed its BLAKE3 check".to_string());
        }
        wires.push(wire);
    }
    // Validate the entire set before decoding: a short object can complete
    // from its first symbol, but trailing mixed or corrupt symbols still fail.
    let mut decoder = raptorq::Decoder::new(oti);
    for wire in wires {
        // `deserialize` takes raw wire bytes; the BLAKE3 seal above already
        // proved they are unchanged in transit, but a hostile peer can seal
        // its own garbage — never let a malformed packet panic the server.
        let packet = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            raptorq::EncodingPacket::deserialize(&wire)
        }))
        .map_err(|_| "integrity: fountain packet unreadable".to_string())?;
        if let Some(payload) = decoder.decode(packet) {
            if blake3::hash(&payload).to_hex().to_string() != first.object {
                return Err("integrity: fountain object hash mismatch after decode".to_string());
            }
            return Ok(payload);
        }
    }
    Err("integrity: fountain decode needs more symbols — keep polling the mailbox".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swarm_layout_stays_small_and_valid() {
        assert_eq!(swarm_layout(0), (1, 1));
        assert_eq!(swarm_layout(100), (1, 1));
        let (k, m) = swarm_layout(1024 * 1024);
        assert!((16..=32).contains(&k), "k={k}");
        assert!((1..=8).contains(&m), "m={m}");
        assert!((k as usize) + (m as usize) <= cybermanju_erasure::MAX_TOTAL_SHARDS);
    }

    #[test]
    fn swarm_round_trips_from_any_k_symbols() {
        let payload = b"swarm me across the relay, lossy paths and all".to_vec();
        let (k, m) = (4u8, 2u8);
        let symbols = swarm_encode(&payload, k, m).expect("encode");
        assert_eq!(symbols.len(), 6);
        assert_eq!(symbols[0].object.len(), 64);
        // Drop both parity symbols AND one data symbol: any k still decode.
        let subset: Vec<SwarmSymbol> = symbols
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 0 && *i != 4 && *i != 5)
            .map(|(_, s)| s.clone())
            .collect();
        assert_eq!(subset.len(), 3);
        assert!(swarm_decode(&subset).is_err(), "k=4 needs 4 symbols");
        let subset: Vec<SwarmSymbol> = symbols.into_iter().take(4).collect();
        assert_eq!(swarm_decode(&subset).expect("decode"), payload);
    }

    #[test]
    fn fountain_round_trips_lossy_subsets() {
        let payload = b"fountain spray across lossy relay paths".to_vec();
        let symbols = fountain_encode(&payload, 1024, 8).expect("encode");
        assert!(symbols.len() > 8, "source + repair symbols");
        assert_eq!(symbols[0].object.len(), 64);
        // Feed a lossy subset (drop every third symbol) — still decodes.
        let subset: Vec<FountainSymbol> = symbols
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 3 != 0)
            .map(|(_, s)| s.clone())
            .collect();
        assert!(!subset.is_empty());
        assert_eq!(fountain_decode(&subset).expect("decode"), payload);
        // OTI travels with every symbol (self-describing subsets).
        assert!(!symbols[0].oti_base64.is_empty());
    }

    #[test]
    fn fountain_decode_fails_closed() {
        let payload = b"tamper-evident fountain".to_vec();
        let symbols = fountain_encode(&payload, 1024, 4).expect("encode");
        assert!(fountain_decode(&[]).unwrap_err().starts_with("invalid:"));
        // Corrupt bytes without touching the hash.
        let mut bad = symbols.clone();
        bad[0].bytes_base64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"garbage-garbage",
        );
        assert!(fountain_decode(&bad).unwrap_err().starts_with("integrity:"));
        // Mixed objects.
        let other = fountain_encode(b"another object entirely", 1024, 4).expect("encode");
        let mixed = vec![symbols[0].clone(), other[0].clone()];
        assert!(fountain_decode(&mixed)
            .unwrap_err()
            .starts_with("integrity:"));
        assert!(fountain_encode(&[], 1024, 4)
            .unwrap_err()
            .starts_with("invalid:"));
        assert!(fountain_encode(&payload, 0, 4)
            .unwrap_err()
            .starts_with("invalid:"));
    }

    #[test]
    fn swarm_decode_fails_closed() {
        let payload = b"tamper-evident".to_vec();
        let symbols = swarm_encode(&payload, 2, 1).expect("encode");
        // Too few symbols.
        assert!(swarm_decode(&symbols[..1])
            .unwrap_err()
            .starts_with("integrity:"));
        // Corrupt bytes without touching the hash.
        let mut bad = symbols.clone();
        bad[0].bytes_base64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"nope-nope-nope-nope",
        );
        assert!(swarm_decode(&bad).unwrap_err().starts_with("integrity:"));
        // Mixed objects.
        let other = swarm_encode(b"another object", 2, 1).expect("encode");
        let mut mixed = vec![symbols[0].clone(), other[0].clone(), symbols[1].clone()];
        mixed.truncate(3);
        assert!(swarm_decode(&mixed).unwrap_err().starts_with("integrity:"));
        // Empty input validates first.
        assert!(swarm_decode(&[]).unwrap_err().starts_with("invalid:"));
    }
}
