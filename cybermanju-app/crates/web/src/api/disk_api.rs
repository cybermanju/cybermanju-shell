// CyberManju OS — disk & volume routes (AGENT-6)
//
// Pre-created and pre-hooked by the supervisor: `crates/web/src/lib.rs` calls
// `route()` right after the auth gate, and `os`/`disk`/`volume` are already in
// `security::ROUTED_SEGMENTS`. AGENT-6 implements the arms; nobody edits
// `lib.rs`, `security.rs` or `api/mod.rs`.
//
// Contract: return `Some(response)` for a path this family owns, `None` for
// anything else (the router then tries `repair_api`, then `os_api`, then the
// main handler). Do NOT hold this call across long work — spawn a task and
// return immediately.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use cybermanju_db::Database;
use cybermanju_disk::{disk, volume, BlockInfo, DiskRow};
use serde::{Deserialize, Serialize};

/// Render a `Result` the way this crate does everywhere else: 404 when the
/// message reports a missing entity, 501 when the capability does not exist
/// on this platform, 400 otherwise (`disk full: …`, `integrity: …`,
/// `auth: …`).
fn respond<T: Serialize>(result: Result<T, String>, origin: Option<&str>) -> String {
    match result {
        Ok(value) => crate::json_ok(&value, origin),
        Err(message) => {
            let status = if message.to_lowercase().contains("not found") {
                404
            } else if message.starts_with("unsupported:") {
                501
            } else {
                400
            };
            crate::json_error(status, &message, origin)
        }
    }
}

/// `400` for a body this route cannot even parse.
fn bad_request(message: String, origin: Option<&str>) -> String {
    crate::json_error(400, &message, origin)
}

/// Every request body is JSON; this is the house error for one that is not.
fn parse<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, String> {
    serde_json::from_str(body).map_err(|err| format!("Invalid JSON: {}", err))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateRequest {
    config_id: String,
    #[serde(alias = "capacityBytes")]
    size_bytes: u64,
    passphrase: String,
    /// Where to write the container. Omitted, the disk picks its own path in
    /// the data directory; supplied, it is `disk::create_at` — the same
    /// entry point imports and migrations use.
    #[serde(default)]
    container_path: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdRequest {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttachRequest {
    id: String,
    passphrase: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResizeRequest {
    id: String,
    #[serde(alias = "capacityBytes")]
    size_bytes: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PutBlockRequest {
    /// The block payload in standard base64. Compressing and sealing it is
    /// the volume's job — the wire format stays dumb.
    data: String,
}

/// Byte range inside a block, as an alternative to asking for all of it.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RangeRequest {
    start: Option<usize>,
    end: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlockRead {
    #[serde(flatten)]
    info: BlockInfo,
    /// Always `base64`: the payload is binary and this is JSON.
    encoding: &'static str,
    /// The whole block, or just `range` of it.
    data: String,
    range: ByteRange,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ByteRange {
    start: usize,
    end: usize,
    /// Full block length in bytes, regardless of the range requested.
    total: usize,
}

/// Dispatch `/api/disk/*` and `/api/volume/*`.
pub fn route(
    db: &Database,
    method: &str,
    path_segments: &[&str],
    body: &str,
    origin: Option<&str>,
) -> Option<String> {
    let response = match path_segments {
        // ─── disks ──────────────────────────────────────────────────────
        ["api", "disk", "list"] if method == "GET" => respond(disk::list(db), origin),

        ["api", "disk", "create"] if method == "POST" => match parse::<CreateRequest>(body) {
            Ok(request) => {
                let created = match request.container_path {
                    Some(path) => disk::create_at(
                        db,
                        &request.config_id,
                        request.size_bytes,
                        &request.passphrase,
                        std::path::Path::new(&path),
                    ),
                    None => disk::create(
                        db,
                        &request.config_id,
                        request.size_bytes,
                        &request.passphrase,
                    ),
                };
                respond(created, origin)
            }
            Err(err) => bad_request(err, origin),
        },

        ["api", "disk", "attach"] if method == "POST" => match parse::<AttachRequest>(body) {
            Ok(request) => respond(disk::attach(db, &request.id, &request.passphrase), origin),
            Err(err) => bad_request(err, origin),
        },

        ["api", "disk", "detach"] if method == "POST" => match parse::<IdRequest>(body) {
            Ok(request) => respond(disk::detach(db, &request.id), origin),
            Err(err) => bad_request(err, origin),
        },

        ["api", "disk", "resize"] if method == "POST" => match parse::<ResizeRequest>(body) {
            Ok(request) => respond(disk::resize(db, &request.id, request.size_bytes), origin),
            Err(err) => bad_request(err, origin),
        },

        ["api", "disk", "destroy"] if method == "POST" => match parse::<IdRequest>(body) {
            Ok(request) => respond(disk::destroy(db, &request.id), origin),
            Err(err) => bad_request(err, origin),
        },

        // fsck: catalog vs. sealed container — local work only, so it is
        // bounded and safe to run while the request lock is held.
        ["api", "disk", "check"] if method == "POST" => match parse::<IdRequest>(body) {
            Ok(request) => respond(disk::check(db, &request.id), origin),
            Err(err) => bad_request(err, origin),
        },

        ["api", "disk", "key-holder"] if method == "POST" => match parse::<IdRequest>(body) {
            Ok(request) => respond(disk::set_key_holder(db, &request.id), origin),
            Err(err) => bad_request(err, origin),
        },

        // `GET /api/disk/{id}` — ids are `disk-<uuid>`, so they can never
        // collide with the `list` arm above.
        ["api", "disk", id] if method == "GET" && *id != "list" => respond(get_one(db, id), origin),

        // ─── volume ─────────────────────────────────────────────────────
        ["api", "volume", "df"] if method == "GET" => respond(volume::df(db), origin),

        ["api", "volume", "block", raw] if method == "GET" => match parse_lba(raw) {
            Ok(lba) => {
                let range = if body.trim().is_empty() {
                    RangeRequest::default()
                } else {
                    match parse::<RangeRequest>(body) {
                        Ok(range) => range,
                        Err(err) => return Some(bad_request(err, origin)),
                    }
                };
                respond(read_block_route(db, lba, range), origin)
            }
            Err(err) => bad_request(err, origin),
        },

        ["api", "volume", "block", raw] if method == "PUT" => match parse_lba(raw) {
            Ok(lba) => match parse::<PutBlockRequest>(body) {
                Ok(request) => {
                    let data = match STANDARD.decode(request.data.trim()) {
                        Ok(data) => data,
                        Err(err) => {
                            return Some(bad_request(format!("invalid: base64: {}", err), origin))
                        }
                    };
                    respond(volume::put_block(db, lba, &data), origin)
                }
                Err(err) => bad_request(err, origin),
            },
            Err(err) => bad_request(err, origin),
        },

        _ => return None,
    };
    Some(response)
}

fn parse_lba(raw: &str) -> Result<u64, String> {
    raw.parse::<u64>()
        .map_err(|_| format!("invalid: block index '{}' is not a number", raw))
}

fn get_one(db: &Database, disk_id: &str) -> Result<DiskRow, String> {
    disk::get(db, disk_id)?.ok_or_else(|| format!("not found: disk {}", disk_id))
}

/// `GET /api/volume/block/{lba}`: the full block or a byte range inside it,
/// with the catalog's placement metadata alongside.
///
/// The router hands this module path segments and a body — never request
/// headers — so the range travels in the JSON body (`{"start":0,"end":4096}`)
/// rather than a `Range:` header. An empty body reads the whole block.
fn read_block_route(db: &Database, lba: u64, range: RangeRequest) -> Result<BlockRead, String> {
    let info = volume::block_info_for(db, lba)?
        .ok_or_else(|| format!("not found: block {} is not written on this volume", lba))?;
    let total = info.data_len as usize;
    let start = range.start.unwrap_or(0).min(total);
    let end = range.end.unwrap_or(total).min(total).max(start);
    let data = volume::read_block(db, lba, start, Some(end - start))?;
    Ok(BlockRead {
        info,
        encoding: "base64",
        data: STANDARD.encode(&data),
        range: ByteRange {
            start,
            end: start + data.len(),
            total,
        },
    })
}
