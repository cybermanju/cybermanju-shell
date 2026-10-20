// CyberManju OS — LAN peers (crates/sync/src/lan.rs).
// Role: mDNS device discovery, std-only (§2B.2) — desktop ↔ phone on the same
// Wi-Fi with no internet and no server. Real DNS-packet mDNS subset on
// 224.0.0.251:5353 (`_cybermanju._tcp.local` PTR/SRV/TXT/A), so announcements
// are also visible to avahi/Bonjour browsers — not a proprietary beacon.
// Exports: LanPeer, lan_service_name, announce, discover, answer_queries_once.
// Integrates: sync crate (no new deps); `GET /api/peers` + `list_lan_peers`
// invoke it; the direct device↔device sync protocol is the follow-up.
// Contracts: `network:` errors carry the OS cause; time bounds everywhere
// (no blocking call exceeds its timeout); passive answering degrades
// honestly when the system mDNS daemon owns port 5353.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// mDNS multicast endpoint (RFC 6762 §5).
pub const MDNS_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
/// mDNS port. Owned by avahi/mDNSResponder on most systems — binding it for
/// passive answering fails there, which is why discovery is active-scan
/// first (queries from an ephemeral port get unicast answers back).
pub const MDNS_PORT: u16 = 5353;
/// Service type every CyberManju device announces and queries.
pub const SERVICE: &str = "_cybermanju._tcp.local";

/// One discovered device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanPeer {
    pub device_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub last_seen_secs: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn write_name(buf: &mut Vec<u8>, name: &str) {
    for label in name.split('.') {
        let bytes = label.as_bytes();
        buf.push(bytes.len().min(63) as u8);
        buf.extend_from_slice(&bytes[..bytes.len().min(63)]);
    }
    buf.push(0);
}

fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn instance_name(device_id: &str) -> String {
    let slug: String = device_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "device" } else { slug };
    format!("{slug}.{SERVICE}")
}

/// PTR query for our service type (transaction id always 0, per mDNS).
pub fn encode_query() -> Vec<u8> {
    let mut buf = Vec::with_capacity(64);
    write_u16(&mut buf, 0); // id
    write_u16(&mut buf, 0); // flags: query
    write_u16(&mut buf, 1); // qdcount
    write_u16(&mut buf, 0); // ancount
    write_u16(&mut buf, 0); // nscount
    write_u16(&mut buf, 0); // arcount
    write_name(&mut buf, SERVICE);
    write_u16(&mut buf, 12); // PTR
    write_u16(&mut buf, 1); // IN
    buf
}

/// Unsolicited announcement: PTR + SRV + TXT + A for one device.
pub fn encode_announce(device_id: &str, name: &str, ipv4: Ipv4Addr, port: u16) -> Vec<u8> {
    let instance = instance_name(device_id);
    let target = format!("{device_id}.local");
    let mut buf = Vec::with_capacity(256);
    write_u16(&mut buf, 0);
    write_u16(&mut buf, 0x8400); // flags: response + authoritative
    write_u16(&mut buf, 0);
    write_u16(&mut buf, 4);
    write_u16(&mut buf, 0);
    write_u16(&mut buf, 0);
    // PTR service -> instance
    write_name(&mut buf, SERVICE);
    write_u16(&mut buf, 12);
    write_u16(&mut buf, 1);
    write_u32(&mut buf, 120);
    let mut rdata = Vec::new();
    write_name(&mut rdata, &instance);
    write_u16(&mut buf, rdata.len() as u16);
    buf.extend_from_slice(&rdata);
    // SRV instance -> target:port
    write_name(&mut buf, &instance);
    write_u16(&mut buf, 33);
    write_u16(&mut buf, 1);
    write_u32(&mut buf, 120);
    let mut rdata = Vec::new();
    write_u16(&mut rdata, 0); // priority
    write_u16(&mut rdata, 0); // weight
    write_u16(&mut rdata, port);
    write_name(&mut rdata, &target);
    write_u16(&mut buf, rdata.len() as u16);
    buf.extend_from_slice(&rdata);
    // TXT instance -> id + name
    write_name(&mut buf, &instance);
    write_u16(&mut buf, 16);
    write_u16(&mut buf, 1);
    write_u32(&mut buf, 120);
    let mut rdata = Vec::new();
    for kv in [format!("id={device_id}"), format!("nm={name}")] {
        let bytes = kv.as_bytes();
        rdata.push(bytes.len().min(255) as u8);
        rdata.extend_from_slice(&bytes[..bytes.len().min(255)]);
    }
    write_u16(&mut buf, rdata.len() as u16);
    buf.extend_from_slice(&rdata);
    // A target -> ipv4
    write_name(&mut buf, &target);
    write_u16(&mut buf, 1);
    write_u16(&mut buf, 1);
    write_u32(&mut buf, 120);
    write_u16(&mut buf, 4);
    buf.extend_from_slice(&ipv4.octets());
    buf
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Option<u8> {
        let v = *self.buf.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }
    fn u16(&mut self) -> Option<u16> {
        let v = u16::from_be_bytes([*self.buf.get(self.pos)?, *self.buf.get(self.pos + 1)?]);
        self.pos += 2;
        Some(v)
    }
    fn u32(&mut self) -> Option<u32> {
        let mut b = [0u8; 4];
        b.copy_from_slice(self.buf.get(self.pos..self.pos + 4)?);
        self.pos += 4;
        Some(u32::from_be_bytes(b))
    }
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let v = self.buf.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(v)
    }
    /// DNS name with compression-pointer support; depth-capped against
    /// malicious packets.
    fn name(&mut self) -> Option<String> {
        let mut labels = Vec::new();
        let mut jumps = 0u8;
        let mut consumed = 0usize;
        let mut pos = self.pos;
        loop {
            if jumps > 8 {
                return None;
            }
            let len = *self.buf.get(pos)?;
            pos += 1;
            if len & 0xC0 == 0xC0 {
                let lo = *self.buf.get(pos)? as usize;
                pos += 1;
                if consumed == 0 {
                    consumed = pos - self.pos;
                }
                pos = ((len as usize & 0x3F) << 8) | lo;
                jumps += 1;
                continue;
            }
            if len == 0 {
                break;
            }
            if len > 63 {
                return None;
            }
            let label = self.buf.get(pos..pos + len as usize)?;
            pos += len as usize;
            labels.push(String::from_utf8_lossy(label).into_owned());
        }
        if consumed == 0 {
            consumed = pos - self.pos;
        }
        self.pos += consumed;
        Some(labels.join("."))
    }
}

#[derive(Debug, Default)]
struct Answers {
    ptr_targets: Vec<String>,
    srv: Vec<(String, u16, String)>,
    txt: Vec<(String, Vec<(String, String)>)>,
    a: Vec<(String, Ipv4Addr)>,
}

fn parse_packet(buf: &[u8]) -> Option<(bool, Answers)> {
    let mut r = Reader { buf, pos: 0 };
    let _id = r.u16()?;
    let flags = r.u16()?;
    let qd = r.u16()? as usize;
    let an = r.u16()? as usize;
    let _ns = r.u16()?;
    let _ar = r.u16()?;
    for _ in 0..qd {
        r.name()?;
        r.u16()?;
        r.u16()?;
    }
    let is_response = flags & 0x8000 != 0;
    let mut out = Answers::default();
    for _ in 0..an {
        let name = r.name()?;
        let rtype = r.u16()?;
        let _class = r.u16()?;
        let _ttl = r.u32()?;
        let rdlen = r.u16()? as usize;
        let rdata = r.bytes(rdlen)?;
        let mut rr = Reader { buf: rdata, pos: 0 };
        match rtype {
            12 => out.ptr_targets.push(rr.name()?),
            33 => {
                let _prio = rr.u16()?;
                let _weight = rr.u16()?;
                let port = rr.u16()?;
                let target = rr.name()?;
                out.srv.push((name, port, target));
            }
            16 => {
                let mut pairs = Vec::new();
                while let Some(len) = rr.u8() {
                    let s = rr.bytes(len as usize)?;
                    let text = String::from_utf8_lossy(s).into_owned();
                    match text.split_once('=') {
                        Some((k, v)) => pairs.push((k.to_string(), v.to_string())),
                        None => pairs.push((text, String::new())),
                    }
                }
                out.txt.push((name, pairs));
            }
            1 if rdlen == 4 => {
                out.a
                    .push((name, Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3])));
            }
            _ => {}
        }
    }
    Some((is_response, out))
}

/// Fold one response packet into peers keyed by instance name.
pub fn collect_peers(buf: &[u8], from: SocketAddr) -> Vec<LanPeer> {
    let Some((is_response, answers)) = parse_packet(buf) else {
        return Vec::new();
    };
    if !is_response {
        return Vec::new();
    }
    let mut ports: std::collections::HashMap<&str, (u16, &str)> = Default::default();
    for (name, port, target) in &answers.srv {
        ports.insert(name.as_str(), (*port, target.as_str()));
    }
    let mut txts: std::collections::HashMap<&str, Vec<(&str, &str)>> = Default::default();
    for (name, pairs) in &answers.txt {
        txts.insert(
            name.as_str(),
            pairs
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect(),
        );
    }
    let mut addrs: std::collections::HashMap<&str, Ipv4Addr> = Default::default();
    for (name, ip) in &answers.a {
        addrs.insert(name.as_str(), *ip);
    }
    let now = now_secs();
    let mut peers = Vec::new();
    for instance in &answers.ptr_targets {
        let Some((port, target)) = ports.get(instance.as_str()).copied() else {
            continue;
        };
        let pairs = txts.get(instance.as_str()).cloned().unwrap_or_default();
        let get = |k: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        };
        let Some(device_id) = get("id") else {
            continue;
        };
        let host = addrs
            .get(target)
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| from.ip().to_string());
        peers.push(LanPeer {
            device_id,
            name: get("nm").unwrap_or_default(),
            host,
            port,
            last_seen_secs: now,
        });
    }
    peers
}

fn mdns_dest() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(MDNS_ADDR, MDNS_PORT))
}

/// Send one unsolicited announcement to the mDNS group.
pub fn announce(device_id: &str, name: &str, port: u16) -> Result<(), String> {
    if device_id.trim().is_empty() {
        return Err("invalid: device id is required".to_string());
    }
    let sock = UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("network: announce socket failed: {e}"))?;
    let ip = local_ipv4().unwrap_or(Ipv4Addr::new(127, 0, 0, 1));
    let pkt = encode_announce(device_id, name, ip, port);
    sock.send_to(&pkt, mdns_dest())
        .map_err(|e| format!("network: announce send failed: {e}"))?;
    Ok(())
}

/// Best-effort local IPv4 (first non-loopback interface address we can see
/// via the routing table; loopback when offline).
fn local_ipv4() -> Option<Ipv4Addr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("192.0.2.1:9").ok()?;
    match sock.local_addr().ok()? {
        SocketAddr::V4(v4) => Some(*v4.ip()),
        _ => None,
    }
}

/// Active scan: multicast one PTR query, collect unicast answers for
/// `timeout`. `dest` is the multicast group in production and a loopback
/// address in tests.
pub fn discover(dest: SocketAddr, timeout: Duration) -> Result<Vec<LanPeer>, String> {
    let sock = UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| format!("network: discover socket failed: {e}"))?;
    sock.set_read_timeout(Some(Duration::from_millis(250)))
        .map_err(|e| format!("network: discover timeout failed: {e}"))?;
    sock.send_to(&encode_query(), dest)
        .map_err(|e| format!("network: discover query failed: {e}"))?;
    let deadline = std::time::Instant::now() + timeout;
    let mut seen: std::collections::HashMap<String, LanPeer> = Default::default();
    let mut buf = [0u8; 2048];
    while std::time::Instant::now() < deadline {
        match sock.recv_from(&mut buf) {
            Ok((n, from)) => {
                for peer in collect_peers(&buf[..n], from) {
                    seen.insert(peer.device_id.clone(), peer);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => return Err(format!("network: discover receive failed: {e}")),
        }
    }
    let mut peers: Vec<LanPeer> = seen.into_values().collect();
    peers.sort_by_key(|a| a.device_id.clone());
    Ok(peers)
}

/// Answer one pending query on a bound socket (the passive half; only
/// reachable when nothing else owns port 5353).
pub fn answer_queries_once(
    sock: &UdpSocket,
    device_id: &str,
    name: &str,
    port: u16,
    timeout: Duration,
) -> Result<bool, String> {
    sock.set_read_timeout(Some(timeout))
        .map_err(|e| format!("network: answer timeout failed: {e}"))?;
    let mut buf = [0u8; 2048];
    let (n, from) = match sock.recv_from(&mut buf) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
        Err(e) => return Err(format!("network: answer receive failed: {e}")),
    };
    let Some((is_response, _)) = parse_packet(&buf[..n]) else {
        return Ok(false);
    };
    if is_response {
        return Ok(false);
    }
    let ip = local_ipv4().unwrap_or(Ipv4Addr::new(127, 0, 0, 1));
    let pkt = encode_announce(device_id, name, ip, port);
    sock.send_to(&pkt, from)
        .map_err(|e| format!("network: answer send failed: {e}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_packet_parses_as_a_query_for_our_service() {
        let pkt = encode_query();
        let mut r = Reader { buf: &pkt, pos: 0 };
        assert_eq!(r.u16(), Some(0));
        assert_eq!(r.u16(), Some(0));
        assert_eq!(r.u16(), Some(1));
        let _ = (r.u16(), r.u16(), r.u16());
        assert_eq!(r.name().as_deref(), Some(SERVICE));
        assert_eq!(r.u16(), Some(12));
    }

    #[test]
    fn announce_round_trips_through_the_collector() {
        let pkt = encode_announce(
            "laptop-1",
            "Ada Laptop",
            Ipv4Addr::new(192, 168, 1, 50),
            3456,
        );
        let from: SocketAddr = "192.168.1.50:5353".parse().unwrap();
        let peers = collect_peers(&pkt, from);
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].device_id, "laptop-1");
        assert_eq!(peers[0].name, "Ada Laptop");
        assert_eq!(peers[0].host, "192.168.1.50");
        assert_eq!(peers[0].port, 3456);
    }

    #[test]
    fn garbage_and_queries_collect_nothing() {
        let from: SocketAddr = "127.0.0.1:1".parse().unwrap();
        assert!(collect_peers(&[0u8; 4], from).is_empty());
        assert!(collect_peers(&encode_query(), from).is_empty());
        // Announcement without the id TXT is not attributable.
        let mut pkt = encode_announce("x", "y", Ipv4Addr::LOCALHOST, 3456);
        pkt.truncate(pkt.len().saturating_sub(8));
        assert!(collect_peers(&pkt, from).is_empty());
    }

    #[test]
    fn discover_finds_a_loopback_responder() {
        let sock = UdpSocket::bind("127.0.0.1:0").expect("bind");
        let addr = sock.local_addr().expect("addr");
        let responder = std::thread::spawn(move || {
            sock.set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
            let mut buf = [0u8; 2048];
            let (n, from) = sock.recv_from(&mut buf).expect("query");
            assert!(parse_packet(&buf[..n]).is_some());
            let pkt = encode_announce("phone-7", "Phone", Ipv4Addr::LOCALHOST, 3456);
            sock.send_to(&pkt, from).expect("answer");
        });
        let peers = discover(addr, Duration::from_secs(3)).expect("discover");
        responder.join().expect("responder");
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].device_id, "phone-7");
    }

    #[test]
    fn announce_rejects_a_blank_device_id() {
        assert!(announce("  ", "x", 3456).is_err());
    }
}
