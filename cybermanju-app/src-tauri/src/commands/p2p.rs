// CyberManju OS — P2P compute twins (src-tauri/src/commands/p2p.rs).
// Role: Tauri IPC for pure P2P computation (§2B swarming + pairing + fountain
// codes) — erasure-coded swarm encode/decode and RaptorQ fountain symbols
// over the native `cybermanju-sync` swarm module, plus human pairing-code
// minting. No sockets here (all network rides REST/mailbox); Pages runs the
// same math through the wasm `swarm_*`/`fountain_*` exports, so every
// transport shares one code path.
// Exports: swarm_encode, swarm_decode, fountain_encode, fountain_decode,
// pairing_code.
// Integrates: cybermanju-sync (swarm + peers, already a dependency);
// the relay mailbox (`POST /api/relay/drop|poll`) carries the symbols.
// Contracts: trusted-local IPC like the rest of this module; `invalid:` /
// `integrity:` / `too_large:` prefixes pass straight through to callers.

/// Encode raw bytes (base64 in) into relay-ready swarm symbols.
/// `data`/`parity` default to the `swarm_layout` heuristic when 0.
#[tauri::command(async)]
pub fn swarm_encode(
    bytes_base64: String,
    data: Option<u8>,
    parity: Option<u8>,
) -> Result<Vec<serde_json::Value>, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(bytes_base64.trim())
        .map_err(|e| format!("invalid: payload is not base64: {e}"))?;
    let (k, m) = match (data, parity) {
        (Some(k), Some(m)) if k > 0 && m > 0 => (k, m),
        _ => cybermanju_sync::swarm::swarm_layout(bytes.len() as u64),
    };
    let symbols = cybermanju_sync::swarm::swarm_encode(&bytes, k, m)?;
    serde_json::to_value(&symbols)
        .map_err(|e| format!("integrity: symbols unserializable: {e}"))
        .and_then(|v| {
            v.as_array()
                .cloned()
                .ok_or_else(|| "integrity: symbols did not serialize as an array".to_string())
        })
}

/// Decode collected swarm symbols (JSON array in) back to raw bytes
/// (base64 out). Any `k` distinct symbols of one object suffice.
#[tauri::command(async)]
pub fn swarm_decode(symbols_json: String) -> Result<String, String> {
    use base64::Engine;
    let symbols: Vec<cybermanju_sync::swarm::SwarmSymbol> = serde_json::from_str(&symbols_json)
        .map_err(|e| format!("invalid: symbols are not JSON: {e}"))?;
    let bytes = cybermanju_sync::swarm::swarm_decode(&symbols)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// Mint a fresh human pairing code (`NNN-word-word`) for wormhole-style
/// rendezvous over the relay mailbox (`pair:<code>` recipients).
#[tauri::command(async)]
pub fn pairing_code() -> Result<String, String> {
    cybermanju_sync::peers::pairing_code()
}

/// RaptorQ-fountain-encode raw bytes (base64 in) into relay-ready symbols.
/// `mtu`/`repair` of 0 select the relay defaults (32 KiB symbols, repair =
/// 64). Returns the symbol envelopes as JSON values.
#[tauri::command(async)]
pub fn fountain_encode(
    bytes_base64: String,
    mtu: Option<u16>,
    repair: Option<u32>,
) -> Result<Vec<serde_json::Value>, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(bytes_base64.trim())
        .map_err(|e| format!("invalid: payload is not base64: {e}"))?;
    let mtu = mtu.filter(|m| *m > 0).unwrap_or(32768);
    let repair = repair.filter(|r| *r > 0).unwrap_or(64);
    let symbols = cybermanju_sync::swarm::fountain_encode(&bytes, mtu, repair)?;
    serde_json::to_value(&symbols)
        .map_err(|e| format!("integrity: symbols unserializable: {e}"))
        .and_then(|v| {
            v.as_array()
                .cloned()
                .ok_or_else(|| "integrity: symbols did not serialize as an array".to_string())
        })
}

/// Decode collected fountain symbols (JSON array in) back to raw bytes
/// (base64 out). Any sufficient subset from any peers suffices.
#[tauri::command(async)]
pub fn fountain_decode(symbols_json: String) -> Result<String, String> {
    use base64::Engine;
    let symbols: Vec<cybermanju_sync::swarm::FountainSymbol> = serde_json::from_str(&symbols_json)
        .map_err(|e| format!("invalid: symbols are not JSON: {e}"))?;
    let bytes = cybermanju_sync::swarm::fountain_decode(&symbols)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}
