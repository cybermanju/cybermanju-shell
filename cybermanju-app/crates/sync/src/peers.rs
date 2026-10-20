// CyberManju OS — Peer transports + federated search (crates/sync/src/peers.rs).
// Role: cross-platform P2P seam (§2B) — one transport enum over the paths that
// really work today (LAN HTTP via mDNS, direct WAN HTTP op-log/search, USB
// sneakernet bundles) with the iroh QUIC relay honestly sequenced as
// `RelayFuture`. Plus the cross-device search port: fetch a peer's
// `GET /api/search` over plain HTTP and merge ranked hits (dedup by file id,
// max score wins, stable desc sort).
// Exports: PeerTransport, PeerEndpoint, parse_peer_endpoint, normalize_base_url,
// classify_addr, is_lan_base, direct_http_plan, FederatedHit,
// merge_federated_hits, fetch_peer_hits, platform_notes, platform_setup,
// peer_readyz_url, peer_search_url, peer_oplog_since_url, peer_oplog_push_url,
// peer_conflicts_url, deduplicate_peers, wasm_peer_warning,
// MAX_FEDERATED_PEERS, FEDERATED_MARKER_HEADER.
// Integrates: sync crate (std + reqwest blocking + serde_json already deps);
// `POST /api/search/federated` and the `search_files_federated` Tauri command
// ride on `merge_federated_hits` + `fetch_peer_hits`; the TS twin is
// `src/utils/peerSearch.ts` (same grammar, same merge order).
// Contracts: every constructor validates (`invalid:`); relay tickets answer
// `unsupported:` with the HTTP/LAN/bundle way out, never a hang;
// `fetch_peer_hits` never exceeds its timeout and normalizes failures to
// `network:` / `auth:` — never a guessed address, never a panic.
use std::{
    io::Read,
    net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs},
    time::Duration,
};

use cybermanju_db::Database;

/// How we reach a peer. Only `RelayFuture` is aspirational — everything else
/// is live on all six targets (android/ios/windows/linux/docker/wasm-fetch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerTransport {
    /// Same Wi-Fi via mDNS discovery + `http://host:port` op-log/search.
    LanHttp,
    /// Direct `http(s)://host:port` op-log/search wherever `/api` is reachable.
    WanHttp,
    /// USB sneakernet bundle (`GET/POST /api/sync/bundle/*`), no network.
    Bundle,
    /// iroh QUIC relay / NAT traversal — sequenced, not shipped.
    RelayFuture,
}

impl PeerTransport {
    /// Stable wire string (TS twin `peerTransportName` matches it).
    pub fn as_str(self) -> &'static str {
        match self {
            PeerTransport::LanHttp => "lan-http",
            PeerTransport::WanHttp => "wan-http",
            PeerTransport::Bundle => "bundle",
            PeerTransport::RelayFuture => "relay-future",
        }
    }
}

/// One reachable peer: who it is + where its `/api` lives + how we dial it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerEndpoint {
    pub device_id: String,
    pub base_url: String,
    pub transport: PeerTransport,
}

/// `http(s)://` base for a raw addr half. The addr stays opaque (DNS name,
/// `host:port`, or a full URL); nothing is guessed — a bare addr becomes
/// `http://<addr>`, an existing scheme is kept verbatim (trailing `/` cut).
pub fn normalize_base_url(addr: &str) -> Result<String, String> {
    let base = addr.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("invalid: peer address is required".to_string());
    }
    if base.contains(char::is_whitespace) {
        return Err("invalid: peer address must not contain whitespace".to_string());
    }
    if base.len() >= 7
        && (base.starts_with("http://")
            || base.starts_with("HTTP://")
            || base.starts_with("https://")
            || base.starts_with("HTTPS://"))
    {
        Ok(base.to_string())
    } else {
        Ok(format!("http://{base}"))
    }
}

/// True when `relay` appears as a standalone token (`relay:443`,
/// `my-relay-host`) — not inside `relayx`. Shared with the TS twins
/// `isRelayToken` / `isRelayAddr` so every transport agrees on one grammar.
pub fn is_relay_token(lower_addr: &str) -> bool {
    let bytes = lower_addr.as_bytes();
    if !lower_addr.contains("relay") {
        return false;
    }
    let mut i = 0;
    while i + 5 <= bytes.len() {
        if &bytes[i..i + 5] == b"relay" {
            let before_ok = if i == 0 {
                true
            } else {
                !bytes[i - 1].is_ascii_alphanumeric()
            };
            let after_ok = if i + 5 >= bytes.len() {
                true
            } else {
                !bytes[i + 5].is_ascii_alphanumeric()
            };
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Host half of a base/addr (scheme + port + path stripped, lower-cased).
/// IPv6 literals keep brackets (`[::1]:3456` → `[::1]`).
pub fn extract_peer_host(lower_base_or_addr: &str) -> String {
    let mut h = lower_base_or_addr.trim().to_lowercase();
    for prefix in ["http://", "https://"] {
        if h.starts_with(prefix) {
            h = h[prefix.len()..].to_string();
            break;
        }
    }
    if let Some(slash) = h.find('/') {
        h.truncate(slash);
    }
    if h.starts_with('[') {
        if let Some(close) = h.find(']') {
            h.truncate(close + 1);
            return h;
        }
        return h;
    }
    // One colon = host:port; strip the port. Bare multi-colon IPv6 without
    // brackets stays opaque for the relay/HTTP gates.
    if h.contains(':') && h.matches(':').count() == 1 {
        if let Some(colon) = h.rfind(':') {
            h.truncate(colon);
        }
    }
    h
}

/// Classify a raw addr half without opening any socket. Relay tickets and
/// multiaddrs (`relay`-token, `iroh:`, `/p2p/`) are the QUIC future; everything
/// else is direct HTTP (LAN vs WAN is decided by `is_lan_base` on the base).
pub fn classify_addr(addr: &str) -> PeerTransport {
    let low = addr.trim().to_lowercase();
    if low.starts_with("iroh:") || low.contains("/p2p/") || is_relay_token(&low) {
        return PeerTransport::RelayFuture;
    }
    if is_lan_base(&low) {
        return PeerTransport::LanHttp;
    }
    PeerTransport::WanHttp
}

/// True when a (lower-cased) base/addr looks like same-network HTTP:
/// localhost, RFC-1918 (`10/8`, `192.168/16`, `172.16/12`), link-local,
/// `.local` mDNS names, or the 3456 default. Host-based (no `110.` false
/// positives) — mirrors TS `isLanBase`.
pub fn is_lan_base(lower_base_or_addr: &str) -> bool {
    let b = lower_base_or_addr.to_lowercase();
    if b.contains("localhost") || b.contains("[::1]") {
        return true;
    }
    let host = extract_peer_host(&b);
    if host.is_empty() {
        return false;
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    if host == "0.0.0.0" {
        return true;
    }
    if host == "local" || host.ends_with(".local") {
        return true;
    }
    if host == "10" || host.starts_with("10.") {
        return true;
    }
    if host.starts_with("192.168.") || host.starts_with("127.") || host.starts_with("169.254.") {
        return true;
    }
    if host.starts_with("172.") {
        let second: i32 = host
            .split('.')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(-1);
        if (16..=31).contains(&second) {
            return true;
        }
    }
    false
}

/// Parse `<device-id>@<addr>` into a dialable endpoint. Both halves required,
/// whitespace refused. Relay addrs parse fine but classify as `RelayFuture`
/// so callers degrade to HTTP/LAN/bundle instead of hanging.
pub fn parse_peer_endpoint(text: &str) -> Result<PeerEndpoint, String> {
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
    let transport = classify_addr(&addr);
    let base_url = normalize_base_url(&addr)?;
    Ok(PeerEndpoint {
        device_id,
        base_url,
        transport,
    })
}

/// Real direct-HTTP sync steps for a dialable peer: probe `/api/readyz`, then
/// the op-log pull → push → merge → render exchange. Relay endpoints refuse
/// with `unsupported:` (QUIC is sequenced) naming the live way out.
pub fn direct_http_plan(peer: &PeerEndpoint, since: u64) -> Result<Vec<String>, String> {
    if peer.device_id.trim().is_empty() {
        return Err("invalid: plan needs a device id".to_string());
    }
    if peer.transport == PeerTransport::RelayFuture {
        return Err(format!(
            "unsupported: QUIC dial to '{}' needs the iroh transport (sequenced: new dependency + networked CI proving) — use HTTP dial (GET /api/readyz + op-log pull/push), direct LAN sync or a sneakernet bundle",
            peer.device_id,
        ));
    }
    let base = peer.base_url.trim_end_matches('/');
    Ok(vec![
        format!("probe: GET {base}/api/readyz (15 s abort, fail-closed)"),
        format!("pull: GET {base}/api/oplog/since?since={since}"),
        format!("push: POST {base}/api/oplog/push {{ deviceId, ops, signature, algorithm }} (ML-DSA required)"),
        "merge: verify peer envelope against its registered key, then append locally".to_string(),
        "render: GET /api/oplog/conflicts".to_string(),
    ])
}

/// One ranked hit from any device (local or peer). `source` is the device id
/// for local hits and the peer base URL for remote ones.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FederatedHit {
    pub file_id: String,
    pub file_name: String,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub snippet: String,
    #[serde(default)]
    pub source: String,
}

/// Merge ranked hits across devices: dedup by `file_id` (max score wins; on a
/// tie the richer snippet wins), stable desc sort by score, truncate to
/// `limit`. Pure — the TS twin `mergeFederatedHits` pins the same order.
pub fn merge_federated_hits(mut hits: Vec<FederatedHit>, limit: usize) -> Vec<FederatedHit> {
    let mut best: std::collections::HashMap<String, FederatedHit> = Default::default();
    for hit in hits.drain(..) {
        if hit.file_id.trim().is_empty() {
            continue;
        }
        let replace = match best.get(&hit.file_id) {
            Some(cur) => {
                hit.score > cur.score
                    || (hit.score == cur.score && hit.snippet.len() > cur.snippet.len())
            }
            None => true,
        };
        if replace {
            best.insert(hit.file_id.clone(), hit);
        }
    }
    let mut out: Vec<FederatedHit> = best.into_values().collect();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.truncate(limit);
    out
}

fn encode_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for ch in query.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(ch),
            ' ' => out.push_str("%20"),
            _ => {
                let encoded = ch.to_string();
                for b in encoded.as_bytes() {
                    out.push_str(&format!("%{b:02X}"));
                }
            }
        }
    }
    out
}

fn hit_from_value(value: &serde_json::Value, source: &str) -> Option<FederatedHit> {
    let obj = value.as_object()?;
    let file_id = obj
        .get("fileId")
        .or_else(|| obj.get("file_id"))
        .or_else(|| obj.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if file_id.trim().is_empty() {
        return None;
    }
    let file_name = obj
        .get("fileName")
        .or_else(|| obj.get("file_name"))
        .or_else(|| obj.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let file_name = if file_name.is_empty() {
        file_id.clone()
    } else {
        file_name
    };
    let score = obj.get("score").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let snippet = obj
        .get("snippet")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let source = obj
        .get("source")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| source.to_string());
    Some(FederatedHit {
        file_id,
        file_name,
        score,
        snippet,
        source,
    })
}

const MAX_PEER_SEARCH_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

fn peer_ip_allowed(ip: IpAddr, allow_private: bool) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            if a == 0 || a == 127 || (a == 169 && b == 254) || a >= 224 {
                return false;
            }
            let private = a == 10 || (a == 172 && (16..=31).contains(&b)) || (a == 192 && b == 168);
            if private {
                return allow_private;
            }
            let shared = a == 100 && (64..=127).contains(&b);
            let reserved = (a == 192 && b == 0)
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113)
                || u32::from(ip) == u32::MAX;
            !shared && !reserved
        }
        IpAddr::V6(ip) => {
            if ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unicast_link_local()
            {
                return false;
            }
            let segments = ip.segments();
            if segments[0] & 0xffc0 == 0xfec0 {
                return false; // deprecated site-local
            }
            if segments[..5] == [0, 0, 0, 0, 0] && segments[5] == 0xffff {
                let mapped = Ipv4Addr::new(
                    (segments[6] >> 8) as u8,
                    segments[6] as u8,
                    (segments[7] >> 8) as u8,
                    segments[7] as u8,
                );
                return peer_ip_allowed(IpAddr::V4(mapped), allow_private);
            }
            if ip.is_unique_local() {
                return allow_private;
            }
            let global_unicast = segments[0] & 0xe000 == 0x2000;
            let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
            global_unicast && !documentation
        }
    }
}

fn resolve_peer_target(
    url: &reqwest::Url,
    allow_private: bool,
) -> Result<(String, Vec<SocketAddr>, bool), String> {
    let host = url
        .host_str()
        .ok_or_else(|| "invalid: peer URL must include a hostname".to_string())?;
    let bare_host = host.trim_start_matches('[').trim_end_matches(']');
    let lower = bare_host.to_ascii_lowercase();
    if lower == "localhost"
        || lower.ends_with(".localhost")
        || lower == "metadata"
        || lower == "metadata.google.internal"
    {
        return Err("invalid: peer host is a loopback or cloud-metadata target".to_string());
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "invalid: peer URL must use HTTP or HTTPS".to_string())?;
    if let Ok(ip) = bare_host.parse::<IpAddr>() {
        if !peer_ip_allowed(ip, allow_private) {
            return Err(
                "invalid: peer address is not an allowed public or authorized LAN target"
                    .to_string(),
            );
        }
        return Ok((bare_host.to_string(), vec![SocketAddr::new(ip, port)], true));
    }
    let addrs = (bare_host, port)
        .to_socket_addrs()
        .map_err(|e| format!("invalid: peer DNS resolution failed: {e}"))?
        .collect::<Vec<_>>();
    if addrs.is_empty() {
        return Err("invalid: peer hostname resolved to no addresses".to_string());
    }
    if addrs
        .iter()
        .any(|addr| !peer_ip_allowed(addr.ip(), allow_private))
    {
        return Err(
            "invalid: peer hostname resolves to a blocked or unauthorized address".to_string(),
        );
    }
    Ok((bare_host.to_string(), addrs, false))
}

/// Match a peer URL to an authenticated presence announcement. Used by the
/// web layer before enabling private-address dialing; comparison ignores
/// hostname case/trailing dots but requires the exact announced port.
pub fn peer_base_matches_presence(
    base_url: &str,
    announced_host: &str,
    announced_port: u16,
) -> bool {
    let Ok(base) = normalize_base_url(base_url) else {
        return false;
    };
    let Ok(url) = reqwest::Url::parse(&base) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let normalize = |value: &str| {
        value
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim_end_matches('.')
            .to_ascii_lowercase()
    };
    normalize(host) == normalize(announced_host)
        && url.port_or_known_default() == Some(announced_port)
}

/// Query one peer's `GET /api/search?q=&limit=` with a bounded timeout.
/// Private/LAN addresses are rejected unless the caller has independently
/// verified a matching, owner-scoped device presence; DNS answers are pinned
/// and redirects/proxies are disabled to prevent SSRF bypasses.
pub fn fetch_peer_hits(
    base_url: &str,
    query: &str,
    limit: usize,
    timeout_secs: u64,
    bearer: Option<&str>,
) -> Result<Vec<FederatedHit>, String> {
    fetch_peer_hits_with_policy(base_url, query, limit, timeout_secs, bearer, false)
}

/// LAN-enabled twin; callers must verify the URL matches a registered,
/// owner-scoped peer presence before passing `allow_private = true`.
pub fn fetch_peer_hits_with_policy(
    base_url: &str,
    query: &str,
    limit: usize,
    timeout_secs: u64,
    bearer: Option<&str>,
    allow_private: bool,
) -> Result<Vec<FederatedHit>, String> {
    if timeout_secs == 0 || timeout_secs > 120 {
        return Err("invalid: timeout_secs must be 1..=120".to_string());
    }
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    if let Some((scheme, _)) = base_url.trim().split_once("://") {
        if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
            return Err("invalid: peer URL must use HTTP or HTTPS".to_string());
        }
    }
    let base = normalize_base_url(base_url)?;
    let parsed =
        reqwest::Url::parse(&base).map_err(|e| format!("invalid: malformed peer URL: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(
            "invalid: peer URL must be HTTP(S), without credentials, query or fragment".to_string(),
        );
    }
    let (host, addresses, is_ip) = resolve_peer_target(&parsed, allow_private)?;
    let limit = limit.clamp(1, 200);
    let url = format!(
        "{}/api/search?q={}&limit={}",
        parsed.as_str().trim_end_matches('/'),
        encode_query(query),
        limit
    );
    let timeout = Duration::from_secs(timeout_secs);
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent("CyberManjuOS/0.1")
        .connect_timeout(timeout)
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy();
    if !is_ip {
        builder = builder.resolve_to_addrs(&host, &addresses);
    }
    let client = builder
        .build()
        .map_err(|e| format!("network: peer search client failed for '{base}': {e}"))?;
    let mut req = client.get(&url);
    if let Some(token) = bearer.filter(|t| !t.trim().is_empty()) {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .map_err(|e| format!("network: peer search {} failed: {e}", base))?;
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return Err(format!("auth: peer search {base} refused (HTTP {status})"));
    }
    if !(200..300).contains(&status) {
        return Err(format!(
            "network: peer search {base} answered HTTP {status}"
        ));
    }
    let mut body_bytes = Vec::new();
    resp.take(MAX_PEER_SEARCH_RESPONSE_BYTES + 1)
        .read_to_end(&mut body_bytes)
        .map_err(|e| format!("network: peer search {base} response read failed: {e}"))?;
    if body_bytes.len() as u64 > MAX_PEER_SEARCH_RESPONSE_BYTES {
        return Err(format!(
            "too_large: peer search response exceeds {MAX_PEER_SEARCH_RESPONSE_BYTES} bytes"
        ));
    }
    let body: serde_json::Value = serde_json::from_slice(&body_bytes)
        .map_err(|e| format!("network: peer search {base} returned bad JSON: {e}"))?;
    let items: &[serde_json::Value] = if let Some(arr) = body.as_array() {
        arr
    } else if let Some(arr) = body.get("data").and_then(|v| v.as_array()) {
        arr
    } else if let Some(arr) = body.get("results").and_then(|v| v.as_array()) {
        arr
    } else {
        &[]
    };
    Ok(items
        .iter()
        .filter_map(|v| hit_from_value(v, &base))
        .take(limit)
        .collect())
}

/// Per-platform P2P reality check: (target, one-line note). The UI renders
/// this as the Devices → P2P matrix; every note names the working fallback.
pub fn platform_notes() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "android",
            "mDNS multicast is often blocked (no multicast lock) — use manual device@host:port peers + the cached peer list; search fans out over HTTP with the 15 MiB index heap",
        ),
        (
            "ios",
            "same as android (multicast restricted, background sockets suspended) — manual peers + cached list; HTTP fan-out only while the app is foregrounded",
        ),
        (
            "windows",
            "no system mDNS daemon by default — active scan only (queries from an ephemeral port); allow UDP 5353 / TCP 3456 through the firewall prompt",
        ),
        (
            "linux",
            "port 5353 is usually owned by avahi — passive answering degrades, active scan still finds peers; open UDP 5353 / TCP 3456 in firewalld/ufw",
        ),
        (
            "docker",
            "bridge networks block mDNS — run `--network host` for discovery or set explicit peer base URLs; mount SEARCH_INDEX_PATH persistently or the index rebuilds",
        ),
        (
            "wasm",
            "no UDP sockets / no mDNS in the sandbox — no discovery; search fans out with fetch() to explicit https:// peer URLs only (CORS + mixed-content apply), local search is BM25-lite",
        ),
    ]
}

/// Maximum peers fanned out per federated search (server + TS twin agree).
/// More than this is a latency cliff on mobile (8 × 8 s serial would exceed
/// every foreground budget); extra peers are dropped, never queued.
pub const MAX_FEDERATED_PEERS: usize = 8;

/// Marker header sent on server-side peer fetches so a peer can tell a
/// federated fan-out from a direct user search. Peers serve plain
/// `GET /api/search` for these (never re-fan-out), so loops are impossible
/// by construction; the header exists for logs/rate-limit triage.
pub const FEDERATED_MARKER_HEADER: &str = "x-cybermanju-federated";

fn trim_base(base_url: &str) -> String {
    normalize_base_url(base_url).unwrap_or_else(|_| base_url.trim().to_string())
}

/// `GET <base>/api/readyz` — the fail-closed liveness probe every dial plan
/// starts with. Single source of truth for the URL shape (TS twin
/// `peerReadyzUrl` pins the same string).
pub fn peer_readyz_url(base_url: &str) -> String {
    format!("{}/api/readyz", trim_base(base_url).trim_end_matches('/'))
}

/// `GET <base>/api/search?q=&limit=` — the cross-device search port. Single
/// source of truth (TS twin `peerSearchUrl` pins the same shape).
pub fn peer_search_url(base_url: &str, query: &str, limit: usize) -> String {
    format!(
        "{}/api/search?q={}&limit={}",
        trim_base(base_url).trim_end_matches('/'),
        encode_query(query),
        limit.clamp(1, 200)
    )
}

/// `GET <base>/api/oplog/since?since=&limit=` — pull half of the op-log
/// exchange (TS twin `peerOplogSinceUrl`).
pub fn peer_oplog_since_url(base_url: &str, since: u64, limit: usize) -> String {
    format!(
        "{}/api/oplog/since?since={}&limit={}",
        trim_base(base_url).trim_end_matches('/'),
        since,
        limit.clamp(1, 500)
    )
}

/// `POST <base>/api/oplog/push` — signed envelope push; unsigned ops are rejected.
pub fn peer_oplog_push_url(base_url: &str) -> String {
    format!(
        "{}/api/oplog/push",
        trim_base(base_url).trim_end_matches('/')
    )
}

/// `GET /api/oplog/conflicts` is always local (render half) — kept here so
/// dial-step builders on every transport spell it identically.
pub fn peer_conflicts_url() -> &'static str {
    "/api/oplog/conflicts"
}

/// Dedup peer endpoints by normalized `base_url` (first wins, case-insensitive),
/// drop blanks and relay tickets (QUIC future — never fetched over HTTP),
/// cap at `MAX_FEDERATED_PEERS`. Pure — the TS twin `deduplicatePeers` pins
/// the same order (explicit peers first, cached peers after).
pub fn deduplicate_peers(peers: Vec<PeerEndpoint>) -> Vec<PeerEndpoint> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for peer in peers {
        let base = match normalize_base_url(&peer.base_url) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if base.trim().is_empty() {
            continue;
        }
        if classify_addr(&base) == PeerTransport::RelayFuture {
            continue;
        }
        let key = base.to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        out.push(PeerEndpoint {
            device_id: peer.device_id,
            base_url: base,
            transport: peer.transport,
        });
        if out.len() >= MAX_FEDERATED_PEERS {
            break;
        }
    }
    out
}

/// Browser-side warning for a peer base URL, or `None` when the fetch should
/// just work. `page_is_https` is true when the local page itself is
/// `https://` (Pages): then any `http://` peer is mixed-content-blocked
/// before our code ever runs. `localhost` peers from a remote page are
/// similarly unreachable (they point at the *viewer's* machine, not ours).
/// Pure — the TS twin `wasmPeerWarning` pins the same strings.
pub fn wasm_peer_warning(base_url: &str, page_is_https: bool) -> Option<String> {
    let low = base_url.trim().to_lowercase();
    if low.is_empty() {
        return Some("invalid: peer address is required".to_string());
    }
    if page_is_https && low.starts_with("http://") {
        return Some(
            "mixed-content: this page is https:// but the peer is http:// — the browser blocks the fetch; serve the peer over https or use the server-side POST /api/search/federated fan-out"
                .to_string(),
        );
    }
    if low.contains("localhost") || low.contains("127.") || low.contains("[::1]") {
        return Some(
            "loopback: a remote page's localhost peer points at the viewer, not this device — use the peer's LAN address (e.g. 192.168.1.60:3456) instead"
                .to_string(),
        );
    }
    None
}

/// Actionable per-target setup for the Devices → P2P matrix (the *how* to go
/// with `platform_notes`' *what*). One `; `-joined command list per target —
/// copy-pasteable in OPERATIONS.md and the UI tooltip.
pub fn platform_setup(target: &str) -> &'static str {
    match target {
        "android" => "grant Nearby/MulticastLock via Tauri manifest; else POST /api/peers/announce once per launch and read GET /api/peers/known (same Wi-Fi), keep the app foregrounded during sync",
        "ios" => "enable Bonjour/multicast entitlement; else POST /api/peers/announce once per launch and read GET /api/peers/known, keep the app foregrounded (background sockets suspend)",
        "windows" => "allow UDP 5353 + TCP 3456 at the firewall prompt; active scan only (no system mDNS daemon); /api/peers/known works without any firewall rule",
        "linux" => "open UDP 5353 / TCP 3456 in firewalld/ufw; avahi owns 5353 so passive answering degrades — active scan still finds peers, /api/peers/known needs nothing",
        "docker" => "run --network host for mDNS, else POST /api/peers/announce with the container's reachable host:port and read GET /api/peers/known; persist SEARCH_INDEX_PATH + /data or the index rebuilds",
        "wasm" => "no setup on-device: POST /api/peers/announce is unreachable from the sandbox — use explicit https:// peer URLs (CORS allowlist + no mixed-content) or POST /api/search/federated server-side; WebRTC datachannels rendezvous via POST /api/relay/drop|poll",
        _ => "add an explicit device@host:port peer — it works on every target",
    }
}

// ─── Server-assisted presence + relay mailbox (kv-backed, no schema change) ──
//
// mDNS multicast cannot cross subnets and never reaches mobile/docker/wasm,
// so any dashboard doubles as a rendezvous: peers `POST /api/peers/announce`
// once per launch (5-minute TTL heartbeat) and readers merge
// `GET /api/peers/known` with the mDNS scan. NAT-traversed op envelopes and
// WebRTC handshake messages ride the same kv mailbox (`POST /api/relay/drop`,
// `POST /api/relay/poll`, `POST /api/relay/ack`) — both sides make outbound
// HTTPS, no inbound port needed. All functions are pure kv logic (no
// sockets); the TS twins live in `src/utils/p2pPresence.ts`.

/// kv key holding one presence entry.
pub const PRESENCE_PREFIX: &str = "peer/addr/";
/// kv key holding the presence index (JSON array of device ids).
pub const PRESENCE_INDEX: &str = "peer/addr/index";
/// Heartbeat TTL: entries older than this are invisible to readers.
pub const PRESENCE_TTL_SECS: u64 = 300;
/// kv prefix holding one relay message per key.
pub const RELAY_MSG_PREFIX: &str = "peer/relay/msg/";
/// kv prefix holding one recipient index per key (JSON array of message ids).
pub const RELAY_INDEX_PREFIX: &str = "peer/relay/index/";
/// Mailbox depth per recipient (oldest dropped first, never unbounded).
pub const RELAY_MAX_PER_RECIPIENT: usize = 32;
/// Largest single relay message (envelope JSON, bytes on the wire).
pub const RELAY_MAX_BYTES: usize = 256 * 1024;
/// Largest presence index (never an unbounded array in kv).
pub const PRESENCE_MAX_DEVICES: usize = 128;

/// One announced peer: who it is, where its `/api` lives, when it checked in.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresenceEntry {
    pub device_id: String,
    pub host: String,
    pub port: u16,
    pub seen_at_secs: u64,
}

/// One relay mailbox message (op envelope or WebRTC handshake fragment).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: String,
    pub payload: serde_json::Value,
    pub created_at_secs: u64,
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Wall-clock seconds for presence/relay calls (single source so route
/// handlers and tests share one clock; tests inject their own).
pub fn presence_now() -> u64 {
    unix_secs()
}

fn read_index(db: &Database, key: &str) -> Vec<String> {
    db.kv_get(key)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default()
}

fn write_index(db: &Database, key: &str, ids: &[String]) -> Result<(), String> {
    let raw =
        serde_json::to_string(ids).map_err(|e| format!("integrity: index unserializable: {e}"))?;
    db.kv_set(key, &raw).map_err(|e| e.to_string())
}

/// Announce this device's reachable `/api` (`POST /api/peers/announce`).
/// `now_secs` is injected so tests pin TTL without sleeping.
pub fn announce_presence(
    db: &Database,
    device_id: &str,
    host: &str,
    port: u16,
    now_secs: u64,
) -> Result<PresenceEntry, String> {
    let device_id = device_id.trim().to_string();
    if device_id.is_empty() {
        return Err("invalid: announce needs a device id".to_string());
    }
    if device_id.contains(char::is_whitespace) {
        return Err("invalid: device id must not contain whitespace".to_string());
    }
    let host = host
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_string();
    if host.is_empty() {
        return Err("invalid: announce needs a host".to_string());
    }
    if host.contains(char::is_whitespace) || host.contains('/') {
        return Err("invalid: announce host must be a bare host or IP".to_string());
    }
    if port == 0 {
        return Err("invalid: announce needs a port (1..=65535)".to_string());
    }
    let entry = PresenceEntry {
        device_id: device_id.clone(),
        host,
        port,
        seen_at_secs: now_secs,
    };
    let raw = serde_json::to_string(&entry)
        .map_err(|e| format!("integrity: entry unserializable: {e}"))?;
    db.kv_set(&format!("{PRESENCE_PREFIX}{device_id}"), &raw)
        .map_err(|e| e.to_string())?;
    let mut index = read_index(db, PRESENCE_INDEX);
    if !index.iter().any(|id| id == &device_id) {
        index.push(device_id);
        index.truncate(PRESENCE_MAX_DEVICES);
        write_index(db, PRESENCE_INDEX, &index)?;
    }
    Ok(entry)
}

/// Read the fresh (TTL-unexpired) presence entries, sorted by device id.
/// Read-only (safe under the shared lock on the `GET` route).
pub fn known_presence(db: &Database, now_secs: u64, max_age_secs: u64) -> Vec<PresenceEntry> {
    let mut out = Vec::new();
    for id in read_index(db, PRESENCE_INDEX) {
        let Some(raw) = db.kv_get(&format!("{PRESENCE_PREFIX}{id}")).ok().flatten() else {
            continue;
        };
        let Ok(entry) = serde_json::from_str::<PresenceEntry>(&raw) else {
            continue;
        };
        if now_secs.saturating_sub(entry.seen_at_secs) <= max_age_secs {
            out.push(entry);
        }
    }
    out.sort_by(|a, b| a.device_id.cmp(&b.device_id));
    out
}

/// Drop one envelope into a recipient's mailbox (`POST /api/relay/drop`).
/// Returns the message id (the sender polls nothing — delivery is the
/// relay's receipt; the recipient polls). Payloads beyond `RELAY_MAX_BYTES`
/// refuse with `too_large:` (ship them as bundle payloads over direct HTTP).
pub fn relay_drop(
    db: &Database,
    from: &str,
    to: &str,
    kind: &str,
    payload: serde_json::Value,
    now_secs: u64,
) -> Result<String, String> {
    let from = from.trim().to_string();
    let to = to.trim().to_string();
    let kind = kind.trim().to_string();
    if from.is_empty() || to.is_empty() {
        return Err("invalid: relay drop needs from and to device ids".to_string());
    }
    if kind.is_empty() {
        return Err("invalid: relay drop needs a kind (op-envelope|offer|answer|ice)".to_string());
    }
    let envelope = serde_json::json!({
        "v": 1u32, "from": from, "to": to, "kind": kind, "payload": payload,
    });
    let wire = serde_json::to_string(&envelope)
        .map_err(|e| format!("integrity: envelope unserializable: {e}"))?;
    if wire.len() > RELAY_MAX_BYTES {
        return Err(format!(
            "too_large: relay message is {} bytes (cap {RELAY_MAX_BYTES} — ship bulk bytes as bundle payloads over direct HTTP)",
            wire.len()
        ));
    }
    let id = blake3::hash(format!("{from}\0{to}\0{kind}\0{now_secs}\0{wire}").as_bytes())
        .to_hex()
        .to_string()
        .chars()
        .take(16)
        .collect::<String>();
    let msg = RelayMessage {
        id: id.clone(),
        from,
        to: to.clone(),
        kind,
        payload,
        created_at_secs: now_secs,
    };
    let raw = serde_json::to_string(&msg)
        .map_err(|e| format!("integrity: message unserializable: {e}"))?;
    db.kv_set(&format!("{RELAY_MSG_PREFIX}{id}"), &raw)
        .map_err(|e| e.to_string())?;
    let index_key = format!("{RELAY_INDEX_PREFIX}{to}");
    let mut index = read_index(db, &index_key);
    if !index.iter().any(|m| m == &id) {
        index.push(id.clone());
    }
    while index.len() > RELAY_MAX_PER_RECIPIENT {
        if let Some(old) = index.first().cloned() {
            let _ = db.kv_delete(&format!("{RELAY_MSG_PREFIX}{old}"));
            index.remove(0);
        } else {
            break;
        }
    }
    write_index(db, &index_key, &index)?;
    Ok(id)
}

/// List a recipient's queued messages, oldest first (`POST /api/relay/poll`).
/// Read-only — messages stay queued until `relay_ack` clears them, so a
/// dropped poll never loses mail.
pub fn relay_poll(db: &Database, recipient: &str) -> Result<Vec<RelayMessage>, String> {
    let recipient = recipient.trim();
    if recipient.is_empty() {
        return Err("invalid: relay poll needs a device id".to_string());
    }
    let mut out = Vec::new();
    for id in read_index(db, &format!("{RELAY_INDEX_PREFIX}{recipient}")) {
        let Some(raw) = db.kv_get(&format!("{RELAY_MSG_PREFIX}{id}")).ok().flatten() else {
            continue;
        };
        if let Ok(msg) = serde_json::from_str::<RelayMessage>(&raw) {
            out.push(msg);
        }
    }
    out.sort_by_key(|m| (m.created_at_secs, m.id.clone()));
    Ok(out)
}

/// Clear delivered messages (`POST /api/relay/ack`). Returns cleared count.
pub fn relay_ack(db: &Database, recipient: &str, ids: &[String]) -> Result<usize, String> {
    let recipient = recipient.trim();
    if recipient.is_empty() {
        return Err("invalid: relay ack needs a device id".to_string());
    }
    let index_key = format!("{RELAY_INDEX_PREFIX}{recipient}");
    let mut index = read_index(db, &index_key);
    let mut cleared = 0;
    for id in ids {
        let id = id.trim();
        if id.is_empty() || !index.iter().any(|m| m == id) {
            continue;
        }
        let _ = db.kv_delete(&format!("{RELAY_MSG_PREFIX}{id}"));
        index.retain(|m| m != id);
        cleared += 1;
    }
    write_index(db, &index_key, &index)?;
    Ok(cleared)
}

// ─── Human pairing codes (wormhole-style rendezvous, no new deps) ──
//
// Short `NNN-word-word` codes two humans read aloud (`482-oak-river`):
// the sender drops to the code-derived recipient, the receiver polls it.
// Codes are *rendezvous*, not PAKE — guessing is bounded by the 4M code
// space, the relay's JWT gate (only vault users reach the mailbox at all),
// per-recipient depth caps and short code TTLs. Attestation still comes
// from ML-DSA envelopes (`auth`), never from the code. The TS twin is
// `src/utils/pairing.ts` (same grammar, same wordlist).

/// Code wordlist (64 short words — `1000 × 64 × 64 ≈ 4.1M` codes).
pub const PAIRING_WORDS: &[&str; 64] = &[
    "oak", "river", "stone", "birch", "comet", "harbor", "meadow", "ridge", "willow", "flint",
    "grove", "summit", "brook", "cedar", "dune", "ember", "fjord", "glacier", "heath", "inlet",
    "juniper", "kelp", "lagoon", "maple", "north", "otter", "pine", "quarry", "reef", "spruce",
    "tundra", "umber", "vale", "wolf", "yonder", "zinc", "acorn", "basalt", "cliff", "drift",
    "elm", "fern", "granite", "holly", "iris", "jade", "kestrel", "lichen", "moss", "nimbus",
    "onyx", "prairie", "quartz", "robin", "sage", "thistle", "upland", "vetch", "wren", "yarrow",
    "alder", "beacon", "cinder", "dawn",
];

/// Recipient prefix for code-derived mailboxes (`pair:482-oak-river`).
pub const PAIRING_PREFIX: &str = "pair:";

/// Mint a fresh pairing code (`NNN-word-word`, lowercase). Randomness comes
/// from the OS (same `OsRng` the OAuth + state modules use) — never
/// `Math.random`-grade, never logged.
pub fn pairing_code() -> Result<String, String> {
    use rand_core::RngCore;
    let mut buf = [0u8; 4];
    rand_core::OsRng
        .try_fill_bytes(&mut buf)
        .map_err(|e| format!("invalid: pairing entropy failed: {e}"))?;
    let n = u32::from_le_bytes(buf);
    Ok(format!(
        "{:03}-{}-{}",
        n % 1000,
        PAIRING_WORDS[(n >> 10) as usize % PAIRING_WORDS.len()],
        PAIRING_WORDS[(n >> 16) as usize % PAIRING_WORDS.len()],
    ))
}

/// Validate + normalize a pairing code (case-insensitive in, lowercase out).
pub fn normalize_pairing_code(code: &str) -> Result<String, String> {
    let low = code.trim().to_lowercase();
    let parts: Vec<&str> = low.split('-').collect();
    if parts.len() != 3 {
        return Err("invalid: pairing code is NNN-word-word (e.g. 482-oak-river)".to_string());
    }
    if parts[0].len() != 3 || !parts[0].bytes().all(|b| b.is_ascii_digit()) {
        return Err("invalid: pairing code starts with 3 digits (e.g. 482-oak-river)".to_string());
    }
    for w in [&parts[1], &parts[2]] {
        if !PAIRING_WORDS.contains(w) {
            return Err(format!(
                "invalid: pairing word '{w}' is not in the code wordlist"
            ));
        }
    }
    Ok(low)
}

/// Mailbox recipient for a code (`pair:482-oak-river`) — what the sender
/// drops to and the receiver polls. Validates first (`invalid:` on typos).
pub fn pairing_recipient(code: &str) -> Result<String, String> {
    Ok(format!("{PAIRING_PREFIX}{}", normalize_pairing_code(code)?))
}

/// True when a mailbox id is a code-derived recipient (never a device id).
pub fn is_pairing_recipient(to: &str) -> bool {
    let Some(code) = to.trim().strip_prefix(PAIRING_PREFIX) else {
        return false;
    };
    normalize_pairing_code(code).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_parses_and_classifies() {
        let lan = parse_peer_endpoint("phone-7@192.168.1.60:3456").unwrap();
        assert_eq!(lan.device_id, "phone-7");
        assert_eq!(lan.base_url, "http://192.168.1.60:3456");
        assert_eq!(lan.transport, PeerTransport::LanHttp);
        let wan = parse_peer_endpoint("vault@https://vault.example.com").unwrap();
        assert_eq!(wan.transport, PeerTransport::WanHttp);
        let relay = parse_peer_endpoint("phone-7@relay.cybermanju.example").unwrap();
        assert_eq!(relay.transport, PeerTransport::RelayFuture);
        assert!(parse_peer_endpoint("no-at-sign").is_err());
        assert!(parse_peer_endpoint("id@").is_err());
        assert!(parse_peer_endpoint("my id@addr").is_err());
    }

    #[test]
    fn direct_plan_runs_for_http_and_refuses_relays() {
        let lan = parse_peer_endpoint("phone-7@192.168.1.60:3456").unwrap();
        let steps = direct_http_plan(&lan, 41).unwrap();
        assert_eq!(steps.len(), 5);
        assert!(steps[0].contains("/api/readyz"), "{steps:?}");
        assert!(steps[1].contains("/api/oplog/since?since=41"), "{steps:?}");
        assert!(steps[2].contains("signature, algorithm"), "{steps:?}");
        assert!(steps[3].contains("verify peer envelope"), "{steps:?}");
        let relay = parse_peer_endpoint("phone-7@relay.example").unwrap();
        let err = direct_http_plan(&relay, 0).expect_err("relay is QUIC future");
        assert!(err.starts_with("unsupported:"), "{err}");
    }

    #[test]
    fn merge_dedups_by_file_id_max_score_wins() {
        let mk = |id: &str, score: f64, source: &str| FederatedHit {
            file_id: id.to_string(),
            file_name: format!("{id}.txt"),
            score,
            snippet: String::new(),
            source: source.to_string(),
        };
        let out = merge_federated_hits(
            vec![
                mk("a", 1.0, "local"),
                mk("a", 9.0, "peer"),
                mk("b", 5.0, "peer"),
                mk("", 99.0, "peer"),
            ],
            10,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].file_id, "a");
        assert_eq!(out[0].score, 9.0);
        assert_eq!(out[0].source, "peer");
        assert_eq!(out[1].file_id, "b");
    }

    #[test]
    fn fetch_validates_without_touching_the_network() {
        assert!(fetch_peer_hits("http://127.0.0.1:9", "q", 5, 0, None)
            .unwrap_err()
            .starts_with("invalid:"));
        assert!(fetch_peer_hits("http://127.0.0.1:9", "   ", 5, 2, None)
            .unwrap()
            .is_empty());
        let closed = fetch_peer_hits("http://127.0.0.1:9", "photo", 5, 2, None).unwrap_err();
        assert!(closed.starts_with("invalid:"), "{closed}");
    }

    #[test]
    fn peer_search_blocks_ssrf_and_allows_private_only_under_policy() {
        assert!(!peer_ip_allowed(IpAddr::V4(Ipv4Addr::LOCALHOST), true));
        assert!(!peer_ip_allowed(
            IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)),
            true
        ));
        assert!(!peer_ip_allowed(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)),
            false
        ));
        assert!(peer_ip_allowed(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)),
            true
        ));
        assert!(peer_ip_allowed(
            IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
            false
        ));
        assert!(!peer_ip_allowed(IpAddr::V6("::1".parse().unwrap()), true));

        let private = reqwest::Url::parse("http://192.168.1.20:3456").unwrap();
        assert!(resolve_peer_target(&private, false).is_err());
        assert!(resolve_peer_target(&private, true).is_ok());
        assert!(peer_base_matches_presence(
            "http://PHONE.local:3456/",
            "phone.local",
            3456
        ));
        assert!(!peer_base_matches_presence(
            "http://phone.local:3457",
            "phone.local",
            3456
        ));
        assert!(
            fetch_peer_hits("http://169.254.169.254/latest/meta-data", "q", 5, 2, None)
                .unwrap_err()
                .starts_with("invalid:")
        );
        assert!(fetch_peer_hits("ftp://example.com", "q", 5, 2, None)
            .unwrap_err()
            .starts_with("invalid:"));
    }

    #[test]
    fn every_target_has_a_note() {
        let notes = platform_notes();
        for target in ["android", "ios", "windows", "linux", "docker", "wasm"] {
            assert!(notes.iter().any(|(t, _)| *t == target), "{notes:?}");
        }
    }

    #[test]
    fn peer_url_builders_share_one_shape() {
        assert_eq!(
            peer_readyz_url("192.168.1.60:3456"),
            "http://192.168.1.60:3456/api/readyz"
        );
        assert_eq!(
            peer_search_url("http://192.168.1.60:3456/", "hello world", 20),
            "http://192.168.1.60:3456/api/search?q=hello%20world&limit=20"
        );
        assert_eq!(
            peer_oplog_since_url("http://vault.example/", 41, 500),
            "http://vault.example/api/oplog/since?since=41&limit=500"
        );
        assert_eq!(
            peer_oplog_push_url("vault.example:3456"),
            "http://vault.example:3456/api/oplog/push"
        );
        assert_eq!(peer_conflicts_url(), "/api/oplog/conflicts");
    }

    #[test]
    fn dedup_keeps_first_base_and_caps_at_eight() {
        let mk = |id: &str, base: &str| PeerEndpoint {
            device_id: id.to_string(),
            base_url: base.to_string(),
            transport: PeerTransport::LanHttp,
        };
        let mut peers = vec![mk("a", "http://10.0.0.1:3456")];
        peers.push(mk("a2", "http://10.0.0.1:3456/"));
        peers.push(mk("b", "  "));
        for i in 0..20 {
            peers.push(mk(&format!("p{i}"), &format!("http://10.0.0.{i}:3456")));
        }
        let out = deduplicate_peers(peers);
        assert_eq!(out.len(), MAX_FEDERATED_PEERS);
        assert_eq!(out[0].device_id, "a");
    }

    #[test]
    fn wasm_warnings_name_mixed_content_and_loopback() {
        let mixed = wasm_peer_warning("http://192.168.1.60:3456", true).unwrap();
        assert!(mixed.starts_with("mixed-content:"), "{mixed}");
        assert!(wasm_peer_warning("https://vault.example.com", true).is_none());
        let loopback = wasm_peer_warning("http://localhost:3456", false).unwrap();
        assert!(loopback.starts_with("loopback:"), "{loopback}");
    }

    #[test]
    fn every_target_has_setup() {
        for target in ["android", "ios", "windows", "linux", "docker", "wasm"] {
            assert!(!platform_setup(target).is_empty(), "{target}");
        }
    }

    #[test]
    fn pairing_codes_mint_validate_and_route() {
        // 100 draws: shape holds, all distinct.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..100 {
            let code = pairing_code().expect("mint");
            let parts: Vec<&str> = code.split('-').collect();
            assert_eq!(parts.len(), 3, "{code}");
            assert_eq!(parts[0].len(), 3, "{code}");
            assert!(parts[0].bytes().all(|b| b.is_ascii_digit()), "{code}");
            assert!(PAIRING_WORDS.contains(&parts[1]), "{code}");
            assert!(PAIRING_WORDS.contains(&parts[2]), "{code}");
            assert!(seen.insert(code));
        }
        // Normalizes case, routes to the pair: namespace.
        assert_eq!(
            pairing_recipient("482-OAK-River").unwrap(),
            "pair:482-oak-river"
        );
        assert!(is_pairing_recipient("pair:482-oak-river"));
        assert!(!is_pairing_recipient("phone-7"));
        assert!(!is_pairing_recipient("pair:nope"));
        assert!(normalize_pairing_code("")
            .unwrap_err()
            .starts_with("invalid:"));
        assert!(normalize_pairing_code("48-oak-river")
            .unwrap_err()
            .starts_with("invalid:"));
        assert!(normalize_pairing_code("482-oak-nope")
            .unwrap_err()
            .starts_with("invalid:"));
    }

    fn presence_db(tag: &str) -> (std::path::PathBuf, Database) {
        let dir = std::env::temp_dir().join(format!(
            "cybermanju-presence-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let db = Database::new(dir.join("presence.redb").to_str().unwrap()).expect("db");
        (dir, db)
    }

    #[test]
    fn presence_announce_known_ttl() {
        let (_dir, db) = presence_db("ttl");
        announce_presence(&db, "phone-7", "192.168.1.60", 3456, 1_000).expect("announce");
        announce_presence(&db, "laptop-1", "10.0.0.9", 3456, 1_000).expect("announce");
        let known = known_presence(&db, 1_100, PRESENCE_TTL_SECS);
        assert_eq!(known.len(), 2);
        assert_eq!(known[0].device_id, "laptop-1");
        // Stale heartbeats are invisible, never an error.
        let known = known_presence(&db, 1_000 + PRESENCE_TTL_SECS + 1, PRESENCE_TTL_SECS);
        assert!(known.is_empty(), "{known:?}");
        // Re-announce refreshes the heartbeat.
        announce_presence(&db, "phone-7", "192.168.1.61", 3456, 2_000).expect("announce");
        let known = known_presence(&db, 2_050, PRESENCE_TTL_SECS);
        assert_eq!(known.len(), 1);
        assert_eq!(known[0].host, "192.168.1.61");
        assert!(announce_presence(&db, "  ", "h", 3456, 0)
            .unwrap_err()
            .starts_with("invalid:"));
        assert!(announce_presence(&db, "x", "", 3456, 0)
            .unwrap_err()
            .starts_with("invalid:"));
        assert!(announce_presence(&db, "x", "h", 0, 0)
            .unwrap_err()
            .starts_with("invalid:"));
    }

    #[test]
    fn relay_mailbox_round_trips_and_acks() {
        let (_dir, db) = presence_db("relay");
        let id = relay_drop(
            &db,
            "laptop-1",
            "phone-7",
            "op-envelope",
            serde_json::json!({"ops": []}),
            5_000,
        )
        .expect("drop");
        assert_eq!(id.len(), 16);
        let queued = relay_poll(&db, "phone-7").expect("poll");
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].from, "laptop-1");
        // Poll is non-destructive — a dropped poll never loses mail.
        assert_eq!(relay_poll(&db, "phone-7").expect("poll").len(), 1);
        assert_eq!(
            relay_ack(&db, "phone-7", std::slice::from_ref(&id)).expect("ack"),
            1
        );
        assert!(relay_poll(&db, "phone-7").expect("poll").is_empty());
        // Unknown recipients poll empty; garbage validates first.
        assert!(relay_poll(&db, "nobody").expect("poll").is_empty());
        assert!(relay_poll(&db, "  ").unwrap_err().starts_with("invalid:"));
        assert!(
            relay_drop(&db, "", "phone-7", "op-envelope", serde_json::json!({}), 0)
                .unwrap_err()
                .starts_with("invalid:")
        );
        let big = serde_json::json!({"blob": "x".repeat(RELAY_MAX_BYTES + 1)});
        assert!(relay_drop(&db, "a", "b", "op-envelope", big, 0)
            .unwrap_err()
            .starts_with("too_large:"));
    }
}
