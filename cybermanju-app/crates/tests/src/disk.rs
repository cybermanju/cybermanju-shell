// CyberManju OS — AGENT-6 disk & volume tests
//
// Pre-created and registered by the supervisor so `cargo test -p cybermanju-tests`
// picks the module up as soon as AGENT-6 adds cases. Acceptance lives in
// `scripts/os-acceptance.sh` Tier 0; unit/contract cases belong here.
//
// These cases drive the disk object the way a client actually meets it:
// through the HTTP routes, with a real local provider behind the disks.

use crate::web::{bearer, body_of, call, mint, mk_dashboard, now_secs, status_of};
use std::fs;
use std::path::{Path, PathBuf};

const PASSPHRASE: &str = "test-volume-passphrase";
/// Dashboards are per test, so the jti only has to be unique inside one.
const JTI: &str = "jti-disk-6";
const MIB: u64 = 1024 * 1024;

// ─── helpers ────────────────────────────────────────────────────────────────

/// base64 (standard alphabet, padded) — the wire encoding of block payloads.
/// `crates/tests` does not carry the `base64` crate, so this stays local to
/// this module rather than widening the test crate's dependency list.
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn b64_decode(text: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => 0,
    };
    let bytes: Vec<u8> = text.bytes().filter(|c| *c != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, byte) in chunk.iter().enumerate() {
            n |= u32::from(val(*byte)) << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    out
}

fn auth(d: &cybermanju_web::WebDashboard) -> String {
    bearer(&mint(d, "user", now_secs() + 3_600, JTI))
}

/// A local provider bucket plus the id of the sync config bound to it.
fn provider(d: &cybermanju_web::WebDashboard, auth: &str) -> (tempfile::TempDir, String) {
    let base = tempfile::tempdir().expect("provider dir");
    let body = format!(
        r#"{{"config":{{"id":"","backendType":"local","enabled":true,"name":"disk-test",
         "basePath":"{}","autoSync":false,"compressBeforeUpload":false,
         "createPreviews":false,"deleteRawAfterSync":false,
         "maxConcurrentUploads":1,"token":""}}}}"#,
        base.path().display()
    );
    let resp = call(d, "POST", "/api/sync/configs", &body, Some(auth));
    assert_eq!(status_of(&resp), 200, "create config: {resp}");
    let id = serde_json::from_str::<serde_json::Value>(body_of(&resp))
        .expect("config json")
        .get("id")
        .and_then(|v| v.as_str())
        .expect("config id")
        .to_string();
    (base, id)
}

/// Create a disk of `size_bytes` through the route, with its container in a
/// caller-chosen temp directory (the route's `containerPath` is the same
/// entry point imports and migrations use). `create` attaches it.
fn create_disk(
    d: &cybermanju_web::WebDashboard,
    auth: &str,
    config_id: &str,
    size_bytes: u64,
    container: &Path,
) -> String {
    let body = format!(
        r#"{{"configId":"{config_id}","sizeBytes":{size_bytes},
            "passphrase":"{PASSPHRASE}","containerPath":"{}"}}"#,
        container.display()
    );
    let resp = call(d, "POST", "/api/disk/create", &body, Some(auth));
    assert_eq!(status_of(&resp), 200, "create disk: {resp}");
    serde_json::from_str::<serde_json::Value>(body_of(&resp))
        .expect("disk json")
        .get("id")
        .and_then(|v| v.as_str())
        .expect("disk id")
        .to_string()
}

fn df(d: &cybermanju_web::WebDashboard, auth: &str) -> serde_json::Value {
    let resp = call(d, "GET", "/api/volume/df", "", Some(auth));
    assert_eq!(status_of(&resp), 200, "df: {resp}");
    serde_json::from_str(body_of(&resp)).expect("df json")
}

fn put_block(d: &cybermanju_web::WebDashboard, auth: &str, lba: u64, data: &[u8]) -> (u16, String) {
    let body = format!(r#"{{"data":"{}"}}"#, b64_encode(data));
    let resp = call(
        d,
        "PUT",
        &format!("/api/volume/block/{lba}"),
        &body,
        Some(auth),
    );
    (status_of(&resp), resp)
}

fn get_block(
    d: &cybermanju_web::WebDashboard,
    auth: &str,
    lba: u64,
    range: Option<(usize, usize)>,
) -> (u16, serde_json::Value) {
    let body = match range {
        Some((start, end)) => format!(r#"{{"start":{start},"end":{end}}}"#),
        None => String::new(),
    };
    let resp = call(
        d,
        "GET",
        &format!("/api/volume/block/{lba}"),
        &body,
        Some(auth),
    );
    let json = serde_json::from_str(body_of(&resp)).unwrap_or(serde_json::Value::Null);
    (status_of(&resp), json)
}

/// Payload for block `lba`: distinct content per block, because identical
/// bytes dedup onto one slot and would not fill a disk.
fn payload(lba: u64, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (lba as u8).wrapping_add(i as u8))
        .collect()
}

/// Files a local provider currently holds — the "no partial upload" witness.
fn count_files(root: &Path) -> usize {
    let mut total = 0;
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                total += 1;
            }
        }
    }
    total
}

// ─── cases ──────────────────────────────────────────────────────────────────

#[test]
fn superblock_magic_and_format_version_are_stable() {
    // Pins the on-disk container: changing either is a breaking format change
    // and must bump MANIFEST/superblock compatibility handling.
    assert_eq!(cybermanju_disk::DISK_MAGIC, b"CYBMJU1");
    assert_eq!(cybermanju_disk::FORMAT_VERSION, 1);
}

#[test]
fn disk_routes_are_auth_gated_like_everything_else() {
    let (_dir, d) = mk_dashboard(3456);
    for path in ["/api/disk/list", "/api/volume/df"] {
        let resp = call(&d, "GET", path, "", None);
        assert_eq!(status_of(&resp), 401, "{path}: {resp}");
    }
}

#[test]
fn the_volume_merges_disks_and_df_follows_attach_and_detach() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = auth(&d);
    let (_base, config_id) = provider(&d, &auth);
    let containers = tempfile::tempdir().expect("containers");

    let first = create_disk(
        &d,
        &auth,
        &config_id,
        MIB,
        &containers.path().join("disk-merge-a.cybermanju"),
    );
    let mut usage = df(&d, &auth);
    assert_eq!(usage["totalBytes"], MIB, "{usage}");
    assert_eq!(usage["diskCount"], 1, "{usage}");

    // A second disk merges into the same number: more providers, more space.
    let second = create_disk(
        &d,
        &auth,
        &config_id,
        MIB,
        &containers.path().join("disk-merge-b.cybermanju"),
    );
    usage = df(&d, &auth);
    assert_eq!(usage["totalBytes"], 2 * MIB, "{usage}");
    assert_eq!(usage["diskCount"], 2, "{usage}");
    assert_eq!(usage["freeBytes"], 2 * MIB, "{usage}");
    assert_ne!(first, second, "two disks, two identities");

    // Detaching takes its capacity out of the merge; attaching puts it back.
    let resp = call(
        &d,
        "POST",
        "/api/disk/detach",
        &format!(r#"{{"id":"{first}"}}"#),
        Some(auth.as_str()),
    );
    assert_eq!(status_of(&resp), 200, "detach: {resp}");
    usage = df(&d, &auth);
    assert_eq!(usage["totalBytes"], MIB, "{usage}");
    assert_eq!(usage["diskCount"], 1, "{usage}");

    let resp = call(
        &d,
        "POST",
        "/api/disk/attach",
        &format!(r#"{{"id":"{first}","passphrase":"{PASSPHRASE}"}}"#),
        Some(auth.as_str()),
    );
    assert_eq!(status_of(&resp), 200, "attach: {resp}");
    usage = df(&d, &auth);
    assert_eq!(usage["totalBytes"], 2 * MIB, "{usage}");
    assert_eq!(usage["diskCount"], 2, "{usage}");

    // Both disks report the choosable capacity the user picked.
    let disks = usage["disks"].as_array().expect("disks array");
    assert_eq!(disks.len(), 2, "{usage}");
    for disk in disks {
        assert_eq!(disk["size"], MIB, "{disk}");
        assert_eq!(disk["provider"], "local", "{disk}");
        assert_eq!(disk["health"], "ok", "{disk}");
    }
}

#[test]
fn blocks_round_trip_over_http_with_range_reads() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = auth(&d);
    let (_base, config_id) = provider(&d, &auth);
    let containers = tempfile::tempdir().expect("containers");
    create_disk(
        &d,
        &auth,
        &config_id,
        MIB,
        &containers.path().join("disk-roundtrip.cybermanju"),
    );

    let data = payload(0, 4096);
    let (status, resp) = put_block(&d, &auth, 0, &data);
    assert_eq!(status, 200, "put: {resp}");
    let placed: serde_json::Value = serde_json::from_str(body_of(&resp)).expect("block json");
    assert_eq!(placed["dataLen"], 4096, "{placed}");
    assert_eq!(placed["diskCapacity"], MIB, "{placed}");
    assert_eq!(placed["diskUsed"], 4096, "{placed}");

    // The whole block comes back byte-identical…
    let (status, body) = get_block(&d, &auth, 0, None);
    assert_eq!(status, 200, "get: {body}");
    assert_eq!(body["encoding"], "base64", "{body}");
    assert_eq!(
        b64_decode(body["data"].as_str().expect("base64 payload")),
        data,
        "byte-identical read-back"
    );
    assert_eq!(body["range"]["total"], 4096, "{body}");

    // …and so does a range inside it.
    let (status, body) = get_block(&d, &auth, 0, Some((4, 12)));
    assert_eq!(status, 200, "range get: {body}");
    assert_eq!(body["range"]["start"], 4, "{body}");
    assert_eq!(body["range"]["end"], 12, "{body}");
    assert_eq!(
        b64_decode(body["data"].as_str().expect("base64")),
        &data[4..12]
    );

    // An unwritten block is a 404, not a fabricated zero.
    let (status, body) = get_block(&d, &auth, 9, None);
    assert_eq!(status, 404, "{body}");

    let usage = df(&d, &auth);
    assert_eq!(usage["usedBytes"], 4096, "{usage}");
    assert_eq!(usage["freeBytes"], MIB - 4096, "{usage}");
}

/// MISSING.md D1 end to end: a volume at capacity refuses the next write
/// *and* the refused write never leaves a payload behind.
#[test]
fn a_full_volume_refuses_the_next_write_and_leaves_no_partial_upload() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = auth(&d);
    let (base, config_id) = provider(&d, &auth);
    let containers = tempfile::tempdir().expect("containers");
    create_disk(
        &d,
        &auth,
        &config_id,
        MIB,
        &containers.path().join("disk-full.cybermanju"),
    );

    let block_size = cybermanju_disk::DEFAULT_BLOCK_SIZE as usize;
    let capacity = MIB as usize;
    let slots = capacity / block_size;
    for lba in 0..slots as u64 {
        let (status, resp) = put_block(&d, &auth, lba, &payload(lba, block_size));
        assert_eq!(status, 200, "block {lba}: {resp}");
    }

    let full = df(&d, &auth);
    assert_eq!(full["totalBytes"], MIB, "{full}");
    assert_eq!(full["usedBytes"], MIB, "{full}");
    assert_eq!(full["freeBytes"], 0, "{full}");

    let before = count_files(base.path());
    assert_eq!(before, slots, "one object per block, nothing else");

    // The next write is refused by admission — before placement, before the
    // payload is sealed, before anything could be uploaded.
    let (status, resp) = put_block(&d, &auth, slots as u64, &payload(slots as u64, block_size));
    assert_eq!(status, 400, "{resp}");
    assert!(
        body_of(&resp).contains("disk_full"),
        "the refusal names the condition: {resp}"
    );
    assert_eq!(
        count_files(base.path()),
        before,
        "no partial upload was left behind"
    );

    // The refused write changed no accounting either.
    let after = df(&d, &auth);
    assert_eq!(after["usedBytes"], MIB, "{after}");
    assert_eq!(after["freeBytes"], 0, "{after}");

    // Rewriting a block that already fits is still allowed — idempotent
    // writes are not refused for space they already occupy.
    let (status, resp) = put_block(&d, &auth, 0, &payload(0, block_size));
    assert_eq!(status, 200, "rewrite in place: {resp}");
    assert_eq!(
        count_files(base.path()),
        before,
        "same content, same object"
    );
}

/// The pipeline admission hook (D1) answers the same way for a sync write.
#[test]
fn admission_refuses_a_sync_write_when_the_volume_is_full() {
    // The disk crate's static constructor arms the hooks; calling it here
    // both forces that object to be linked and documents the contract (a
    // second registration is refused).
    let _ = cybermanju_disk::arm_volume_hooks();

    let (_dir, d) = mk_dashboard(3456);
    let auth = auth(&d);

    // With nothing attached there is nothing to police — a host without a
    // disk must keep syncing.
    {
        let guard = d.db.read().expect("db lock");
        cybermanju_sync::quota::admit_write(&guard, u64::MAX).expect("no substrate, no ceiling");
    }

    let (_base, config_id) = provider(&d, &auth);
    let containers = tempfile::tempdir().expect("containers");
    create_disk(
        &d,
        &auth,
        &config_id,
        MIB,
        &containers.path().join("disk-admit.cybermanju"),
    );

    let guard = d.db.read().expect("db lock");
    cybermanju_sync::quota::admit_write(&guard, MIB).expect("the whole disk is free");

    // …and nothing beyond it does.
    let err = cybermanju_sync::quota::admit_write(&guard, MIB + 1)
        .expect_err("the volume cannot hold it");
    assert!(err.contains("disk_full"), "{err}");

    // The usage the pipeline sees agrees with what `df` reports.
    let usage = cybermanju_sync::quota::volume_usage(&guard).expect("usage");
    assert_eq!(usage.total_bytes, MIB, "{usage:?}");
    assert_eq!(usage.used_bytes, 0, "{usage:?}");
    assert_eq!(usage.free_bytes, MIB, "{usage:?}");
    assert_eq!(usage.disk_count, 1, "{usage:?}");
}
