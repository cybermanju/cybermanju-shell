// CyberManju OS — Swarm (crates/os-wasm/src/swarm.rs).
// Role: browser twin of the native relay-swarming math (`sync::swarm`) —
// Reed–Solomon and RaptorQ fountain encode/decode of relay symbols in the
// Pages sandbox, so a browser collects symbols from the relay mailbox and
// reconstructs payloads without the dashboard. Same framing, same caps,
// same error prefixes.
// Exports: swarm_layout_json, swarm_encode_json, swarm_decode_json,
// fountain_encode_json, fountain_decode_json.
// Integrates: cybermanju-erasure + raptorq (both pure Rust, wasm-safe); the
// TS caller is `src/utils/p2pSwarm.ts` (`swarmBackendFromWasm` /
// `fountainBackendFromWasm`), JSON in/out so no serializer glue is needed.
// Contracts: `invalid:` on bad inputs, `integrity:` on corrupt subsets,
// `too_large:` beyond the bundle payload caps — never a panic.
use wasm_bindgen::prelude::*;

/// Default `(k, m)` layout for a payload length, as JSON `{k, m}`.
#[wasm_bindgen]
pub fn swarm_layout_json(payload_len: u64) -> String {
    // cybermanju-erasure is a native-path dep here (not re-exported through
    // the wasm crate root), so route through tiny local shims below.
    let (k, m) = swarm_layout_of(payload_len);
    format!("{{\"k\":{k},\"m\":{m}}}")
}

fn swarm_layout_of(payload_len: u64) -> (u8, u8) {
    let framed = payload_len.saturating_add(12);
    let k = framed.div_ceil(64 * 1024).clamp(1, 32) as u8;
    (k, k.div_ceil(2).clamp(1, 8))
}

/// Encode raw bytes into relay-ready swarm symbols (JSON array out).
/// `data`/`parity` of 0 select the default layout for the payload length.
#[wasm_bindgen]
pub fn swarm_encode_json(payload: &[u8], data: u8, parity: u8) -> Result<String, JsValue> {
    swarm_encode_inner(payload, data, parity).map_err(|e| JsValue::from_str(&e))
}

/// Decode collected swarm symbols (JSON array in) back to raw bytes.
#[wasm_bindgen]
pub fn swarm_decode_json(symbols_json: &str) -> Result<Vec<u8>, JsValue> {
    swarm_decode_inner(symbols_json).map_err(|e| JsValue::from_str(&e))
}

/// RaptorQ-fountain-encode raw bytes into relay symbols (JSON array out).
/// `mtu`/`repair` of 0 select the relay defaults (32 KiB symbols, 64 repair).
#[wasm_bindgen]
pub fn fountain_encode_json(payload: &[u8], mtu: u16, repair: u32) -> Result<String, JsValue> {
    fountain_encode_inner(payload, mtu, repair).map_err(|e| JsValue::from_str(&e))
}

/// Decode collected fountain symbols (JSON array in) back to raw bytes —
/// any sufficient subset, any order, any peers.
#[wasm_bindgen]
pub fn fountain_decode_json(symbols_json: &str) -> Result<Vec<u8>, JsValue> {
    fountain_decode_inner(symbols_json).map_err(|e| JsValue::from_str(&e))
}

fn fountain_encode_inner(payload: &[u8], mtu: u16, repair: u32) -> Result<String, String> {
    use base64::Engine as _;
    const MAX_TOTAL: u64 = 50 * 1024 * 1024;
    const MAX_MSG: usize = 256 * 1024;
    if payload.is_empty() {
        return Err("invalid: fountain encode needs a non-empty payload".to_string());
    }
    let mtu = if mtu == 0 { 32768 } else { mtu };
    let repair = if repair == 0 { 64 } else { repair };
    if payload.len() as u64 > MAX_TOTAL {
        return Err(format!(
            "too_large: fountain object is {} bytes (cap {MAX_TOTAL} — ship it via the sync backends)",
            payload.len()
        ));
    }
    let encoder = raptorq::Encoder::with_defaults(payload, mtu);
    let oti_b64 =
        base64::engine::general_purpose::STANDARD.encode(encoder.get_config().serialize());
    let packets = encoder.get_encoded_packets(repair);
    if packets.is_empty() {
        return Err("integrity: fountain encoder produced no symbols".to_string());
    }
    let object = blake3::hash(payload).to_hex().to_string();
    let mut symbols = Vec::with_capacity(packets.len());
    for packet in &packets {
        let wire = packet.serialize();
        let symbol = serde_json::json!({
            "object": object,
            "otiBase64": oti_b64,
            "mtu": mtu,
            "blake3": blake3::hash(&wire).to_hex().to_string(),
            "bytesBase64": base64::engine::general_purpose::STANDARD.encode(&wire),
        });
        let envelope = serde_json::to_string(&symbol)
            .map_err(|e| format!("integrity: symbol unserializable: {e}"))?;
        if envelope.len() > MAX_MSG {
            return Err(format!(
                "too_large: fountain symbol is {} bytes (cap {MAX_MSG} — lower the MTU for smaller symbols)",
                envelope.len()
            ));
        }
        symbols.push(symbol);
    }
    serde_json::to_string(&symbols).map_err(|e| format!("integrity: symbols unserializable: {e}"))
}

fn fountain_decode_inner(symbols_json: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Symbol {
        object: String,
        oti_base64: String,
        // Required by the wire format; decoding parameters come from the OTI.
        #[serde(rename = "mtu")]
        _mtu: u16,
        blake3: String,
        bytes_base64: String,
    }
    let symbols: Vec<Symbol> = serde_json::from_str(symbols_json)
        .map_err(|e| format!("invalid: symbols are not JSON: {e}"))?;
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
    for s in &symbols {
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
    // Check all supplied symbols before the decoder can complete early.
    let mut decoder = raptorq::Decoder::new(oti);
    for wire in wires {
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

fn swarm_encode_inner(payload: &[u8], data: u8, parity: u8) -> Result<String, String> {
    use base64::Engine as _;
    const MAX_TOTAL: u64 = 50 * 1024 * 1024;
    const MAX_MSG: usize = 256 * 1024;
    if payload.len() as u64 > MAX_TOTAL {
        return Err(format!(
            "too_large: swarm object is {} bytes (cap {MAX_TOTAL} — ship it via the sync backends)",
            payload.len()
        ));
    }
    let (k, m) = if data > 0 && parity > 0 {
        (data, parity)
    } else {
        swarm_layout_of(payload.len() as u64)
    };
    let encoded = cybermanju_erasure::encode_framed(payload, k, m).map_err(|e| e.to_string())?;
    let object = blake3::hash(payload).to_hex().to_string();
    let shard_len = encoded.shard_len();
    let mut symbols = Vec::with_capacity(encoded.shards.len());
    for (index, shard) in encoded.shards.iter().enumerate() {
        let symbol = serde_json::json!({
            "object": object,
            "dataShards": encoded.data_shards,
            "parityShards": encoded.parity_shards,
            "index": index,
            "shardLen": shard_len,
            "blake3": blake3::hash(shard).to_hex().to_string(),
            "bytesBase64": base64::engine::general_purpose::STANDARD.encode(shard),
        });
        let wire = serde_json::to_string(&symbol)
            .map_err(|e| format!("integrity: symbol unserializable: {e}"))?;
        if wire.len() > MAX_MSG {
            return Err(format!(
                "too_large: swarm symbol {index} is {} bytes (cap {MAX_MSG} — raise k for smaller shards)",
                wire.len()
            ));
        }
        symbols.push(symbol);
    }
    serde_json::to_string(&symbols).map_err(|e| format!("integrity: symbols unserializable: {e}"))
}

fn swarm_decode_inner(symbols_json: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Symbol {
        object: String,
        data_shards: u8,
        parity_shards: u8,
        index: u8,
        shard_len: usize,
        blake3: String,
        bytes_base64: String,
    }
    let symbols: Vec<Symbol> = serde_json::from_str(symbols_json)
        .map_err(|e| format!("invalid: symbols are not JSON: {e}"))?;
    let first = symbols
        .first()
        .ok_or_else(|| "invalid: swarm decode needs at least one symbol".to_string())?;
    let mut present: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut seen: std::collections::HashSet<u8> = Default::default();
    for s in &symbols {
        if s.object != first.object
            || s.data_shards != first.data_shards
            || s.parity_shards != first.parity_shards
            || s.shard_len != first.shard_len
        {
            return Err(
                "integrity: swarm symbols mix objects or layouts — collect one object's symbols"
                    .to_string(),
            );
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
    if present.len() < first.data_shards as usize {
        return Err(format!(
            "integrity: swarm decode needs {} symbols, only {} usable",
            first.data_shards,
            present.len()
        ));
    }
    let payload =
        cybermanju_erasure::decode_framed(first.data_shards, first.parity_shards, &present)
            .map_err(|e| e.to_string())?;
    if blake3::hash(&payload).to_hex().to_string() != first.object {
        return Err("integrity: swarm object hash mismatch after decode".to_string());
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_the_native_heuristic() {
        assert_eq!(swarm_layout_json(0), "{\"k\":1,\"m\":1}");
        assert_eq!(swarm_layout_json(100), "{\"k\":1,\"m\":1}");
    }

    #[test]
    fn wasm_round_trips_lossy_subsets() {
        let payload = b"wasm collects symbols from the relay mailbox".to_vec();
        let json = swarm_encode_inner(&payload, 4, 2).expect("encode");
        let symbols: Vec<serde_json::Value> = serde_json::from_str(&json).expect("json");
        assert_eq!(symbols.len(), 6);
        // Keep any 4 of 6 (drop two) — still decodes.
        let subset = serde_json::to_string(&symbols[1..5]).expect("subset");
        assert_eq!(swarm_decode_inner(&subset).expect("decode"), payload);
        assert!(swarm_decode_inner("[]")
            .unwrap_err()
            .starts_with("invalid:"));
    }

    #[test]
    fn wasm_fountain_round_trips_lossy_subsets() {
        let payload = b"wasm fountain spray across lossy relay paths".to_vec();
        let json = fountain_encode_inner(&payload, 1024, 8).expect("encode");
        let symbols: Vec<serde_json::Value> = serde_json::from_str(&json).expect("json");
        assert!(!symbols.is_empty());
        // Drop every third symbol — the fountain still decodes.
        let subset: Vec<serde_json::Value> = symbols
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % 3 != 0)
            .map(|(_, v)| v)
            .collect();
        let subset_json = serde_json::to_string(&subset).expect("subset");
        assert_eq!(
            fountain_decode_inner(&subset_json).expect("decode"),
            payload
        );
        assert!(fountain_decode_inner("[]")
            .unwrap_err()
            .starts_with("invalid:"));
        assert!(fountain_encode_inner(&[], 1024, 8)
            .unwrap_err()
            .starts_with("invalid:"));
    }
}
