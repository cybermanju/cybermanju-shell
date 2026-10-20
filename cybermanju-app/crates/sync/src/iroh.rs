// CyberManju OS — WAN transport seam (crates/sync/src/iroh.rs).
// Role: WAN leg of §2B (ANALYSIS.md) — device identity ✅, LAN discovery ✅,
// CAS leases ✅, sneakernet ✅, direct LAN sync ✅, HTTPS probe ✅, signed
// envelopes ✅ and relay-over-HTTPS NAT traversal ✅ are all landed. Direct
// HTTP dial works wherever `/api` is reachable; peers behind NAT rendezvous
// through any dashboard configured as `relay_url` (both sides make outbound
// HTTPS — `POST /api/relay/drop`, `POST /api/relay/poll` — so no inbound
// port is needed). Only the QUIC *datagram* upgrade stays sequenced (new
// dependency + networked CI proving); the relay *capability* is live.
// Exports: IrohConfig, IrohEndpoint, parse_endpoint, dial_plan, http_base,
// http_dial_steps, http_probe, relay_envelope, relay_dial_steps,
// relay_drop_path, relay_poll_path, endpoint_fingerprint, is_relay_addr,
// normalize_base, wan_way_out, peer_readyz_url, peer_search_url,
// peer_oplog_since_url, federated_search_plan.
// Integrates: sync crate (std + reqwest blocking with rustls-tls for the
// HTTPS probe/relay forward — no new deps); the op-log push/pull payloads
// ride unchanged on every path — only `dial` grows a backend.
// Contracts: every constructor validates; only `http_probe` and
// `relay_forward` open sockets (bounded timeouts, `network:` on failure) —
// `dial_plan` answers `unsupported:` with the exact way out only when a
// relay ticket arrives with no `relay_url` configured, so callers degrade to
// HTTP/LAN/bundle instead of hanging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrohConfig {
    /// Relay URL (e.g. `https://relay.cybermanju.example`). Empty = direct only.
    pub relay_url: String,
    /// Seconds to wait for NAT traversal before giving up (1..=120).
    pub dial_timeout_secs: u64,
}

impl Default for IrohConfig {
    fn default() -> Self {
        Self {
            relay_url: String::new(),
            dial_timeout_secs: 15,
        }
    }
}

/// A parsed peer endpoint: `<device-id>@<addr>` where addr is still opaque
/// (DNS name, IP or a future iroh ticket — the dial backend decides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrohEndpoint {
    pub device_id: String,
    pub addr: String,
}

/// Parse `<device-id>@<addr>`; both halves required, whitespace refused.
pub fn parse_endpoint(text: &str) -> Result<IrohEndpoint, String> {
    let (device_id, addr) = text
        .split_once('@')
        .ok_or_else(|| "invalid: endpoint must be <device-id>@<addr>".to_string())?;
    let device_id = device_id.trim().to_string();
    let addr = addr.trim().to_string();
    if device_id.is_empty() || addr.is_empty() {
        return Err("invalid: endpoint needs both a device id and an address".to_string());
    }
    if device_id.contains(char::is_whitespace) || addr.contains(char::is_whitespace) {
        return Err("invalid: endpoint halves must not contain whitespace".to_string());
    }
    Ok(IrohEndpoint { device_id, addr })
}

/// Dial plan for a WAN endpoint. Direct `host:port` / URL addrs return the
/// live HTTP plan (probe + op-log pull/push — same steps as
/// `http_dial_steps`); relay tickets (`relay…`, `iroh:…`, `/p2p/…`) return
/// the live relay-envelope plan when `config.relay_url` is set (both sides
/// rendezvous over outbound HTTPS, no inbound port), and `Err("unsupported:
/// …")` with the LAN/bundle way out only when no relay is configured — so
/// the UI offers a next step instead of a hang.
pub fn dial_plan(endpoint: &IrohEndpoint, config: &IrohConfig) -> Result<String, String> {
    if config.dial_timeout_secs == 0 || config.dial_timeout_secs > 120 {
        return Err("invalid: dial_timeout_secs must be 1..=120".to_string());
    }
    if endpoint.device_id.trim().is_empty() {
        return Err("invalid: dial needs a device id".to_string());
    }
    if is_relay_addr(&endpoint.addr) {
        // Canonical tickets carry their own relay; the configured URL is
        // the fallback. Only neither answers `unsupported:`.
        let Some(relay) = ticket_or_config_relay(&endpoint.addr, config) else {
            return Err(format!(
                "unsupported: relay dial to '{}' needs a relay_url (any dashboard serves POST /api/relay/drop|poll — set one, or use HTTP dial (GET /api/readyz + op-log pull/push), direct LAN sync or a sneakernet bundle)",
                endpoint.device_id,
            ));
        };
        return Ok(format!(
            "relay: POST {}/api/relay/drop {{ to: '{}', kind: op-envelope }} ({} s abort, outbound HTTPS — no inbound port) + POST {}/api/relay/poll {{ for: '{}' }} for the reply",
            relay,
            endpoint.device_id,
            config.dial_timeout_secs,
            relay,
            endpoint.device_id,
        ));
    }
    let base = http_base(endpoint);
    Ok(format!(
        "http: probe GET {base}/api/readyz ({} s abort, fail-closed) + op-log pull/push over {base}",
        config.dial_timeout_secs,
    ))
}

/// Relay envelope for one NAT-traversed message: `{ v: 1, to, kind,
/// payload }`. Pure constructor — delivery is `POST <relay>/api/relay/drop`
/// (see `relay_forward`), pickup is `POST <relay>/api/relay/poll`. The TS
/// twin `buildRelayEnvelope` pins the identical JSON shape.
pub fn relay_envelope(
    to_device: &str,
    kind: &str,
    payload: &serde_json::Value,
) -> Result<String, String> {
    if to_device.trim().is_empty() {
        return Err("invalid: relay envelope needs a destination device id".to_string());
    }
    if kind.trim().is_empty() {
        return Err("invalid: relay envelope needs a kind".to_string());
    }
    serde_json::to_string(&serde_json::json!({
        "v": 1,
        "to": to_device,
        "kind": kind,
        "payload": payload,
    }))
    .map_err(|e| format!("integrity: relay envelope unserializable: {e}"))
}

/// Live relay dial steps for a relay-ticket endpoint: drop the envelope at
/// the relay, poll for the reply, then run the op-log merge locally.
/// Pure — the TS twin `relayDialSteps` pins the same order.
pub fn relay_dial_steps(
    endpoint: &IrohEndpoint,
    config: &IrohConfig,
    since: u64,
) -> Result<Vec<String>, String> {
    if !is_relay_addr(&endpoint.addr) {
        return Err(
            "invalid: relay steps need a relay ticket addr (direct addrs use http_dial_steps)"
                .to_string(),
        );
    }
    let Some(relay) = ticket_or_config_relay(&endpoint.addr, config) else {
        return Err(format!(
            "unsupported: relay dial to '{}' needs a relay_url (any dashboard serves POST /api/relay/drop|poll — set one, or use HTTP dial, direct LAN sync or a sneakernet bundle)",
            endpoint.device_id,
        ));
    };
    Ok(vec![
        format!(
            "drop: POST {}/api/relay/drop {{ to: '{}', kind: op-envelope }}",
            relay, endpoint.device_id
        ),
        format!(
            "poll: POST {}/api/relay/poll {{ for: '{}' }} ({} s abort)",
            relay, endpoint.device_id, config.dial_timeout_secs
        ),
        format!("pull: verify polled signed envelope (since={since}) against registered device key, then append locally"),
        "push: POST /api/relay/drop { to, kind: op-envelope, payload: ours }".to_string(),
        "render: GET /api/oplog/conflicts".to_string(),
    ])
}

/// `POST <relay>/api/relay/drop` path for a relay base URL.
pub fn relay_drop_path(relay_base: &str) -> Result<String, String> {
    let base = normalize_base(relay_base)?;
    Ok(format!("{}/api/relay/drop", base.trim_end_matches('/')))
}

/// `POST <relay>/api/relay/poll` path for a relay base URL.
pub fn relay_poll_path(relay_base: &str) -> Result<String, String> {
    let base = normalize_base(relay_base)?;
    Ok(format!("{}/api/relay/poll", base.trim_end_matches('/')))
}

/// Deliver one envelope to a relay over outbound HTTPS (bounded timeout).
/// Returns the relay's `{ accepted: true, id }` receipt as raw JSON.
/// `network:` on failure — never a hang, never a panic. Pure-HTTP relays and
/// plain-HTTP relay URLs both ride here (the TLS decision is the URL's).
pub fn relay_forward(
    relay_base: &str,
    envelope_json: &str,
    timeout_secs: u64,
) -> Result<String, String> {
    if timeout_secs == 0 || timeout_secs > 120 {
        return Err("invalid: dial_timeout_secs must be 1..=120".to_string());
    }
    let url = relay_drop_path(relay_base)?;
    let body: serde_json::Value = serde_json::from_str(envelope_json)
        .map_err(|e| format!("invalid: relay envelope is not JSON: {e}"))?;
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let client = reqwest::blocking::Client::builder()
        .user_agent("CyberManjuOS/0.1")
        .connect_timeout(timeout)
        .timeout(timeout)
        .build()
        .map_err(|e| format!("network: relay client failed for '{url}': {e}"))?;
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| format!("network: relay drop {url} failed: {e}"))?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("network: relay drop {url} answered HTTP {status}"));
    }
    resp.text()
        .map_err(|e| format!("network: relay drop {url} returned an unreadable body: {e}"))
}

/// Short identity fingerprint for a device registry key: BLAKE3 hex (full 64
/// chars) of the decoded public key bytes. Display truncates; comparison
/// never does. Pure — the TS twin `endpointFingerprint` is byte-identical
/// (same base64 alphabet, same hash).
pub fn endpoint_fingerprint(public_key_b64: &str) -> Result<String, String> {
    use base64::Engine;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(public_key_b64.trim())
        .map_err(|e| format!("invalid: device public key is not base64: {e}"))?;
    if raw.is_empty() {
        return Err("invalid: device public key is empty".to_string());
    }
    Ok(blake3::hash(&raw).to_hex().to_string())
}

/// `http(s)://` base for a WAN endpoint. The addr half stays opaque (DNS
/// name, `host:port`, or a full URL); a bare `host` without a port defaults
/// to the dashboard `:3456` (same default as `http_probe`), an existing
/// scheme is kept verbatim — mirrors TS `wanHttpBase`.
pub fn http_base(endpoint: &IrohEndpoint) -> String {
    let addr = endpoint.addr.trim().trim_end_matches('/');
    if addr.len() >= 7
        && (addr.starts_with("http://")
            || addr.starts_with("HTTP://")
            || addr.starts_with("https://")
            || addr.starts_with("HTTPS://"))
    {
        return addr.to_string();
    }
    if addr.contains(':') || addr.contains('/') {
        return format!("http://{addr}");
    }
    format!("http://{addr}:3456")
}

/// Real HTTP dial steps for an off-LAN peer: probe `/api/readyz`, then the
/// op-log pull → push → merge → render exchange over plain HTTP. Same shape
/// as the frontend `wanDialSteps` (pinned by `wan-dial.test.ts` + `spine.rs`).
pub fn http_dial_steps(endpoint: &IrohEndpoint, since: u64) -> Vec<String> {
    let base = http_base(endpoint);
    vec![
        format!("probe: GET {base}/api/readyz ({} s abort, fail-closed)", 15),
        format!("pull: GET {base}/api/oplog/since?since={since}"),
        format!("push: POST {base}/api/oplog/push {{ deviceId, ops, signature, algorithm }} (ML-DSA required)"),
        "merge: verify peer envelope against its registered key, then append locally".to_string(),
        "render: GET /api/oplog/conflicts".to_string(),
    ]
}

/// Probe `GET <addr>/api/readyz` with a bounded timeout. `addr` accepts
/// `host:port`, a full `http(s)://host:port` URL (bare hosts default to the
/// dashboard `:3456`); relay tickets answer `unsupported:` (they rendezvous
/// via `relay_forward`, not this socket). Returns `Ok(true)` on any HTTP
/// response with a 2xx status, else `Err("network: …")` — never a hang,
/// never a panic.
///
/// Plain-HTTP forms ride std TcpStream (no TLS stack in the hot path);
/// `https://` forms ride the reqwest blocking client with rustls-tls (the
/// same client `peers::fetch_peer_hits` uses — no new deps). IPv6 literals
/// (`[::1]:3456`, `[fe80::1]:3456`) are dialed verbatim.
pub fn http_probe(addr: &str, timeout_secs: u64) -> Result<bool, String> {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;

    if timeout_secs == 0 || timeout_secs > 120 {
        return Err("invalid: dial_timeout_secs must be 1..=120".to_string());
    }
    let raw = addr.trim();
    let is_tls = raw.len() >= 8 && (raw.starts_with("https://") || raw.starts_with("HTTPS://"));
    if is_tls {
        return https_probe(raw, timeout_secs);
    }
    let hostport = raw
        .trim_start_matches("http://")
        .trim_start_matches("HTTP://")
        .trim_end_matches('/')
        .split('/')
        .next()
        .unwrap_or("")
        .to_string();
    if hostport.is_empty() {
        return Err("invalid: probe needs a host:port or URL".to_string());
    }
    if hostport.contains(char::is_whitespace) {
        return Err("invalid: probe addr must not contain whitespace".to_string());
    }
    // Relay tickets rendezvous via `relay_forward`, not this socket — one
    // shared grammar (`peers::classify_addr`) so every seam agrees.
    if is_relay_addr(&hostport) {
        return Err(
            "unsupported: relay tickets rendezvous via POST /api/relay/drop|poll (configure a relay_url) — this socket only dials direct host:port peers"
                .to_string(),
        );
    }
    // IPv6 literals already carry brackets + port (`[::1]:3456`); bare
    // hostnames/IPv4 without a colon get the dashboard default. A bare
    // bracketed literal without a port (`[::1]`) also gets :3456.
    let socket_addr = if hostport.starts_with('[') {
        if hostport.contains("]:") {
            hostport.clone()
        } else {
            format!("{hostport}:3456")
        }
    } else if hostport.contains(':') {
        hostport.clone()
    } else {
        format!("{hostport}:3456")
    };
    let timeout = Duration::from_secs(timeout_secs);
    let resolved = socket_addr
        .to_socket_addrs()
        .map_err(|e| format!("network: WAN probe failed to resolve '{socket_addr}': {e}"))?
        .next()
        .ok_or_else(|| format!("network: WAN probe resolved no address for '{socket_addr}'"))?;
    let mut stream = TcpStream::connect_timeout(&resolved, timeout)
        .map_err(|e| format!("network: WAN probe failed for '{socket_addr}': {e}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| format!("network: WAN probe failed for '{socket_addr}': {e}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| format!("network: WAN probe failed for '{socket_addr}': {e}"))?;
    let req =
        format!("GET /api/readyz HTTP/1.0\r\nHost: {socket_addr}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("network: WAN probe failed for '{socket_addr}': {e}"))?;
    // Read until the status line arrives (up to 4 KiB) — a single 512 B
    // `read` can split `HTTP/1.1 200` across packets on slow links.
    let mut head_bytes: Vec<u8> = Vec::with_capacity(1024);
    let mut buf = [0u8; 512];
    for _ in 0..8 {
        let n = stream
            .read(&mut buf)
            .map_err(|e| format!("network: WAN probe failed for '{socket_addr}': {e}"))?;
        if n == 0 {
            break;
        }
        head_bytes.extend_from_slice(&buf[..n]);
        if head_bytes.windows(2).any(|w| w == b"\r\n") || head_bytes.len() >= 4096 {
            break;
        }
    }
    let head = String::from_utf8_lossy(&head_bytes);
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    if (200..300).contains(&status) {
        Ok(true)
    } else {
        Err(format!(
            "network: WAN probe {socket_addr} answered HTTP {status}"
        ))
    }
}

/// HTTPS leg of `http_probe`: `GET <base>/api/readyz` through the reqwest
/// blocking client with rustls-tls (same client as `peers::fetch_peer_hits`
/// — no new deps). Bounded by `timeout_secs`, `network:` on every failure.
fn https_probe(raw: &str, timeout_secs: u64) -> Result<bool, String> {
    let base = raw.trim().trim_end_matches('/');
    if base.contains(char::is_whitespace) {
        return Err("invalid: probe addr must not contain whitespace".to_string());
    }
    if is_relay_addr(base) {
        return Err(
            "unsupported: relay tickets rendezvous via POST /api/relay/drop|poll (configure a relay_url) — this socket only dials direct host:port peers"
                .to_string(),
        );
    }
    let url = format!("{}/api/readyz", base.trim_end_matches('/'));
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let client = reqwest::blocking::Client::builder()
        .user_agent("CyberManjuOS/0.1")
        .connect_timeout(timeout)
        .timeout(timeout)
        .build()
        .map_err(|e| format!("network: WAN probe client failed for '{url}': {e}"))?;
    let resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("network: WAN probe failed for '{url}': {e}"))?;
    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        Ok(true)
    } else {
        Err(format!("network: WAN probe {url} answered HTTP {status}"))
    }
}

/// Classify a raw addr half without opening any socket: relay tickets and
/// multiaddrs rendezvous via the relay, everything else is direct HTTP. Pure
/// alias over `peers::classify_addr` so both seams agree on one grammar.
pub fn classify_addr(addr: &str) -> super::peers::PeerTransport {
    super::peers::classify_addr(addr)
}

/// True when the addr half rendezvouses via the relay (tickets, multiaddrs,
/// relay-token hosts). Pure — the TS twin `isRelayAddr` pins the same
/// grammar so the UI plans before dialing.
pub fn is_relay_addr(addr: &str) -> bool {
    classify_addr(addr) == super::peers::PeerTransport::RelayFuture
}

// ─── iroh endpoint addressing (QUIC identity layer, no async runtime) ──
//
// The async `Endpoint` dial stays sequenced on a tokio runtime decision;
// everything here is sync and load-bearing today: strict validation +
// canonicalization of iroh endpoint IDs (`iroh::PublicKey`: hex-or-base32
// in, hex out) and relay URLs, plus canonical `iroh:<id>@<relay>` tickets
// that carry their own rendezvous — a ticketed dial needs no configured
// `relay_url`. The TS twin validates the same shapes in `wanDial.ts`.

/// Parse + validate an iroh endpoint ID (ed25519 public key). Returns the
/// canonical hex form. `invalid:` on garbage — never a guess.
pub fn parse_endpoint_id(text: &str) -> Result<String, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("invalid: endpoint id is required".to_string());
    }
    if trimmed.contains(char::is_whitespace) {
        return Err("invalid: endpoint id must not contain whitespace".to_string());
    }
    let key: iroh::PublicKey = trimmed
        .parse()
        .map_err(|e: iroh::KeyParsingError| format!("invalid: not an iroh endpoint id: {e:?}"))?;
    Ok(key.to_string())
}

/// Parse + validate a relay URL. Returns the canonical form.
pub fn parse_relay_url(text: &str) -> Result<String, String> {
    let trimmed = text.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("invalid: relay URL is required".to_string());
    }
    if trimmed.contains(char::is_whitespace) {
        return Err("invalid: relay URL must not contain whitespace".to_string());
    }
    let url: iroh::RelayUrl = trimmed
        .parse()
        .map_err(|e: iroh::RelayUrlParseError| format!("invalid: not a relay URL: {e:?}"))?;
    Ok(url.to_string())
}

/// Canonical `iroh:<endpoint-id>@<relay-url>` ticket: both halves strictly
/// validated, both normalized. Tickets are self-contained dials — the
/// relay travels with the address, so no configured `relay_url` is needed.
pub fn iroh_ticket(endpoint_id: &str, relay_url: &str) -> Result<String, String> {
    Ok(format!(
        "iroh:{}@{}",
        parse_endpoint_id(endpoint_id)?,
        parse_relay_url(relay_url)?
    ))
}

/// Split a ticket back into `(endpoint-id, relay-url)` (both normalized).
pub fn parse_iroh_ticket(text: &str) -> Result<(String, String), String> {
    let rest = text
        .trim()
        .strip_prefix("iroh:")
        .ok_or_else(|| "invalid: iroh ticket starts with `iroh:`".to_string())?;
    let (id, relay) = rest
        .split_once('@')
        .ok_or_else(|| "invalid: iroh ticket is `iroh:<endpoint-id>@<relay-url>`".to_string())?;
    Ok((parse_endpoint_id(id)?, parse_relay_url(relay)?))
}

/// Rendezvous for a relay-ticket addr: the ticket's own relay when the addr
/// is a canonical ticket, else the configured `relay_url`. `None` when
/// neither exists (callers answer `unsupported:` with the way out).
fn ticket_or_config_relay(addr: &str, config: &IrohConfig) -> Option<String> {
    if let Ok((_, relay)) = parse_iroh_ticket(addr) {
        if !relay.is_empty() {
            return Some(relay);
        }
    }
    let configured = config.relay_url.trim().trim_end_matches('/');
    if configured.is_empty() {
        None
    } else {
        Some(configured.to_string())
    }
}

/// `http(s)://` base for a raw addr half (addr stays opaque, never guessed).
/// Pure alias over `peers::normalize_base_url` so the WAN seam and the peer
/// seam share one normalizer; `invalid:` when the addr is blank/whitespace.
pub fn normalize_base(addr: &str) -> Result<String, String> {
    super::peers::normalize_base_url(addr)
}

/// The live way out offered alongside every `unsupported:` QUIC refusal —
/// LAN sync, sneakernet bundle, and op-log pull on next contact. Shared by
/// the Devices → WAN card, the `wan plan` cybsh verb, and the TS twin
/// `wanWayOut` (same three lines, same order).
pub fn wan_way_out(endpoint: &IrohEndpoint) -> Vec<String> {
    vec![
        format!(
            "lan: POST /api/peers/{}/sync (same Wi-Fi, Devices → LAN peers → SYNC)",
            endpoint.device_id
        ),
        "bundle: sneakernet op-log delta via GET/POST /api/sync/bundle/export|import (untrusted sticks stay tamper-evident)".to_string(),
        format!(
            "op-log: pull GET /api/oplog/since on next contact — no rescan, payloads ride unchanged when the endpoint arrives ({})",
            endpoint.addr
        ),
    ]
}

/// `GET <base>/api/readyz` for a WAN endpoint (probe half of the dial).
pub fn peer_readyz_url(endpoint: &IrohEndpoint) -> String {
    super::peers::peer_readyz_url(&http_base(endpoint))
}

/// `GET <base>/api/search?q=&limit=` for a WAN endpoint (search half of the
/// cross-device port — what android/ios/docker/wasm fan out to).
pub fn peer_search_url(endpoint: &IrohEndpoint, query: &str, limit: usize) -> String {
    super::peers::peer_search_url(&http_base(endpoint), query, limit)
}

/// `GET <base>/api/oplog/since?since=&limit=` for a WAN endpoint.
pub fn peer_oplog_since_url(endpoint: &IrohEndpoint, since: u64) -> String {
    super::peers::peer_oplog_since_url(&http_base(endpoint), since, 500)
}

/// Federated-search plan for a WAN endpoint: local BM25 first, then one
/// bounded peer fetch (the exact two lines the server + TS fan-out run).
/// Relay endpoints refuse with `unsupported:` — same gate as the sync plan.
pub fn federated_search_plan(
    endpoint: &IrohEndpoint,
    query: &str,
    limit: usize,
) -> Result<Vec<String>, String> {
    if query.trim().is_empty() {
        return Err("invalid: federated search needs a query".to_string());
    }
    if is_relay_addr(&endpoint.addr) {
        return Err(format!(
            "unsupported: federated search to '{}' is a relay ticket — configure a relay_url and search via the relay envelope, or use direct host:port peers / POST /api/search/federated with reachable base URLs",
            endpoint.device_id,
        ));
    }
    let limit = limit.clamp(1, 200);
    Ok(vec![
        format!("local: BM25 `{query}` (limit {limit}, tagged `local`)"),
        format!(
            "peer: GET {} (timeout 8 s, dead peer → zero hits)",
            peer_search_url(endpoint, query, limit)
        ),
    ])
}

/// Real direct-HTTP dial plan for off-LAN peers (probe + op-log pull/push).
/// Relay endpoints refuse with `unsupported:` — pure alias over
/// `peers::direct_http_plan` so the WAN seam and the peer seam share steps.
pub fn direct_http_plan(endpoint: &IrohEndpoint, since: u64) -> Result<Vec<String>, String> {
    let peer = super::peers::PeerEndpoint {
        device_id: endpoint.device_id.clone(),
        base_url: http_base(endpoint),
        transport: super::peers::classify_addr(&endpoint.addr),
    };
    super::peers::direct_http_plan(&peer, since)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_parses_device_at_addr() {
        let ep = parse_endpoint("phone-7@192.168.1.60:3456").unwrap();
        assert_eq!(ep.device_id, "phone-7");
        assert_eq!(ep.addr, "192.168.1.60:3456");
    }

    #[test]
    fn endpoint_rejects_blanks_and_whitespace() {
        assert!(parse_endpoint("no-at-sign").is_err());
        assert!(parse_endpoint("@addr").is_err());
        assert!(parse_endpoint("id@").is_err());
        assert!(parse_endpoint("my id@addr").is_err());
    }

    #[test]
    fn dial_plan_is_live_for_direct_and_relay_with_a_way_out() {
        // Direct addrs dial today (probe + op-log pull/push).
        let direct = parse_endpoint("phone-7@192.168.1.60:3456").unwrap();
        let plan = dial_plan(&direct, &IrohConfig::default()).expect("direct is live");
        assert!(plan.contains("http:"), "{plan}");
        assert!(plan.contains("/api/readyz"), "{plan}");
        // Relay tickets without a relay_url refuse honestly with the way out.
        let ep = parse_endpoint("phone-7@relay:443").unwrap();
        let err = dial_plan(&ep, &IrohConfig::default()).expect_err("no relay configured");
        assert!(err.starts_with("unsupported:"), "{err}");
        assert!(err.contains("relay_url"), "{err}");
        // With a relay_url any dashboard rendezvouses the pair over HTTPS.
        let relayed = dial_plan(
            &ep,
            &IrohConfig {
                relay_url: "https://relay.example.com".to_string(),
                dial_timeout_secs: 15,
            },
        )
        .expect("relay is live");
        assert!(relayed.contains("/api/relay/drop"), "{relayed}");
        let bad = dial_plan(
            &ep,
            &IrohConfig {
                relay_url: String::new(),
                dial_timeout_secs: 0,
            },
        );
        assert!(
            bad.unwrap_err().starts_with("invalid:"),
            "timeout validated first"
        );
    }

    #[test]
    fn relay_envelope_and_steps_round_trip() {
        let ep = parse_endpoint("phone-7@relay.example.com").unwrap();
        let config = IrohConfig {
            relay_url: "https://relay.example.com/".to_string(),
            dial_timeout_secs: 15,
        };
        let env = relay_envelope(
            &ep.device_id,
            "op-envelope",
            &serde_json::json!({"ops": []}),
        )
        .expect("envelope");
        assert!(env.contains("\"to\":\"phone-7\""), "{env}");
        assert!(env.contains("\"v\":1"), "{env}");
        let steps = relay_dial_steps(&ep, &config, 41).expect("steps");
        assert_eq!(steps.len(), 5);
        assert!(steps[0].contains("/api/relay/drop"), "{steps:?}");
        assert!(steps[1].contains("/api/relay/poll"), "{steps:?}");
        assert!(steps[4].contains("/api/oplog/conflicts"), "{steps:?}");
        assert!(relay_envelope("", "op-envelope", &serde_json::json!({})).is_err());
        assert!(relay_dial_steps(&ep, &IrohConfig::default(), 0)
            .unwrap_err()
            .starts_with("unsupported:"));
        assert_eq!(
            relay_drop_path("https://relay.example.com/").unwrap(),
            "https://relay.example.com/api/relay/drop"
        );
        assert_eq!(
            relay_poll_path("relay.example.com:3456").unwrap(),
            "http://relay.example.com:3456/api/relay/poll"
        );
    }

    #[test]
    fn endpoint_ids_validate_and_tickets_round_trip() {
        // Ed25519 base point (compressed Edwards Y) — a real curve point,
        // no randomness, no SecretKey needed.
        let mut raw = [0x66u8; 32];
        raw[0] = 0x58;
        let key = iroh::PublicKey::from_bytes(&raw).expect("basepoint is on the curve");
        let text = key.to_string();
        assert_eq!(text.len(), 64, "{text}");
        assert_eq!(parse_endpoint_id(&text).unwrap(), text);
        // Garbage fails closed.
        assert!(parse_endpoint_id("").unwrap_err().starts_with("invalid:"));
        assert!(parse_endpoint_id("not-a-key")
            .unwrap_err()
            .starts_with("invalid:"));
        assert!(parse_endpoint_id("has space in it")
            .unwrap_err()
            .starts_with("invalid:"));
        assert!(parse_relay_url("").unwrap_err().starts_with("invalid:"));
        assert!(parse_relay_url("not a url with spaces")
            .unwrap_err()
            .starts_with("invalid:"));
        let relay = parse_relay_url("https://relay.example.com/").expect("relay");
        assert!(!relay.is_empty());
        // Canonical tickets carry their own rendezvous.
        let ticket = iroh_ticket(&text, "https://relay.example.com").expect("ticket");
        assert!(ticket.starts_with("iroh:"), "{ticket}");
        assert!(is_relay_addr(&ticket));
        let (id, url) = parse_iroh_ticket(&ticket).expect("parse");
        assert_eq!(id, text);
        assert_eq!(url, relay);
        assert!(parse_iroh_ticket("https://relay.example.com")
            .unwrap_err()
            .starts_with("invalid:"));
        assert!(parse_iroh_ticket("iroh:no-at-sign")
            .unwrap_err()
            .starts_with("invalid:"));
        // A ticketed dial needs no configured relay_url.
        let ep = IrohEndpoint {
            device_id: "phone-7".to_string(),
            addr: ticket.clone(),
        };
        let plan = dial_plan(&ep, &IrohConfig::default()).expect("ticket is self-contained");
        assert!(plan.contains("/api/relay/drop"), "{plan}");
        let steps = relay_dial_steps(&ep, &IrohConfig::default(), 0).expect("steps");
        assert!(steps[0].contains("https://relay.example.com"), "{steps:?}");
    }

    #[test]
    fn endpoint_fingerprint_is_stable_blake3() {
        use base64::Engine;
        let raw = b"fake-ml-dsa-public-key-bytes";
        let b64 = base64::engine::general_purpose::STANDARD.encode(raw);
        let fp = endpoint_fingerprint(&b64).expect("fingerprint");
        assert_eq!(fp.len(), 64);
        assert_eq!(fp, endpoint_fingerprint(&b64).expect("stable"));
        assert!(endpoint_fingerprint("!!!")
            .unwrap_err()
            .starts_with("invalid:"));
        assert!(endpoint_fingerprint("")
            .unwrap_err()
            .starts_with("invalid:"));
    }

    #[test]
    fn http_base_keeps_scheme_and_defaults_to_http() {
        let bare = parse_endpoint("phone-7@192.168.1.60:3456").unwrap();
        assert_eq!(http_base(&bare), "http://192.168.1.60:3456");
        let url = parse_endpoint("phone-7@https://vault.example.com").unwrap();
        assert_eq!(http_base(&url), "https://vault.example.com");
    }

    #[test]
    fn http_dial_steps_are_oplog_pull_push_merge() {
        let ep = parse_endpoint("phone-7@192.168.1.60:3456").unwrap();
        let steps = http_dial_steps(&ep, 41);
        assert_eq!(steps.len(), 5);
        assert!(steps[0].contains("/api/readyz"), "{steps:?}");
        assert!(steps[1].contains("/api/oplog/since?since=41"), "{steps:?}");
        assert!(steps[2].contains("/api/oplog/push"), "{steps:?}");
        assert!(steps[2].contains("signature, algorithm"), "{steps:?}");
        assert!(steps[3].contains("verify peer envelope"), "{steps:?}");
        assert!(steps[4].contains("/api/oplog/conflicts"), "{steps:?}");
    }

    #[test]
    fn http_probe_validates_and_dials_both_schemes() {
        // Invalid timeout first (no socket opened).
        assert!(http_probe("127.0.0.1:9", 0)
            .unwrap_err()
            .starts_with("invalid:"));
        // Relay tickets rendezvous via the relay, not this socket.
        let relay = http_probe("phone-7@relay.example", 2).unwrap_err();
        assert!(relay.starts_with("unsupported:"), "{relay}");
        // Closed loopback ports answer `network:` on both schemes — never a
        // hang, never a panic, never an external network touch.
        let closed = http_probe("127.0.0.1:9", 2).unwrap_err();
        assert!(closed.starts_with("network:"), "{closed}");
        let closed_tls = http_probe("https://127.0.0.1:9", 2).unwrap_err();
        assert!(closed_tls.starts_with("network:"), "{closed_tls}");
    }

    #[test]
    fn relay_addrs_share_one_grammar_with_peers() {
        assert!(is_relay_addr("relay.cybermanju.example"));
        assert!(is_relay_addr("iroh:ticket-abc"));
        assert!(is_relay_addr("/ip4/1.2.3.4/p2p/abcd"));
        assert!(!is_relay_addr("192.168.1.60:3456"));
        assert!(!is_relay_addr("vault.example.com"));
        assert_eq!(
            normalize_base("192.168.1.60:3456").unwrap(),
            "http://192.168.1.60:3456"
        );
    }

    #[test]
    fn wan_way_out_names_three_live_paths() {
        let ep = parse_endpoint("phone-7@192.168.1.60:3456").unwrap();
        let out = wan_way_out(&ep);
        assert_eq!(out.len(), 3);
        assert!(out[0].contains("/api/peers/phone-7/sync"), "{out:?}");
        assert!(out[1].contains("/api/sync/bundle/export"), "{out:?}");
        assert!(out[2].contains("/api/oplog/since"), "{out:?}");
    }

    #[test]
    fn wan_peer_urls_and_search_plan_agree_with_peers() {
        let ep = parse_endpoint("phone-7@192.168.1.60:3456").unwrap();
        assert_eq!(peer_readyz_url(&ep), "http://192.168.1.60:3456/api/readyz");
        assert!(
            peer_search_url(&ep, "photo", 20).contains("/api/search?q=photo"),
            "{}",
            peer_search_url(&ep, "photo", 20)
        );
        assert!(
            peer_oplog_since_url(&ep, 41).contains("/api/oplog/since?since=41"),
            "{}",
            peer_oplog_since_url(&ep, 41)
        );
        let plan = federated_search_plan(&ep, "photo", 20).unwrap();
        assert_eq!(plan.len(), 2);
        assert!(plan[1].contains("/api/search?q=photo"), "{plan:?}");
        let relay = parse_endpoint("phone-7@relay.example").unwrap();
        assert!(federated_search_plan(&relay, "photo", 20)
            .unwrap_err()
            .starts_with("unsupported:"));
        assert!(federated_search_plan(&ep, "   ", 20)
            .unwrap_err()
            .starts_with("invalid:"));
    }
}
