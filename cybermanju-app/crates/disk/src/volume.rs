// CyberManju OS — Volume (crates/disk/src/volume.rs).
//! The merged volume: N attached disks seen as one address space.
//!
//! This is where "more providers → more space" becomes a number:
//! [`df`] sums the choosable capacities of every attached disk, [`admit`]
//! refuses a write the merged volume cannot hold (MISSING.md D1), and
//! [`place_block`] fills a disk to its high-water mark before spilling to
//! the next (D3, spanned — the default policy; round-robin stays available
//! through the volume row's `policy`).
//!
//! Blocks travel the same way every sync artifact does: **compress, then
//! seal** under the disk's passphrase behind the `CYBE1` magic, and are
//! verified by BLAKE3 on the way back in.

use crate::catalog::{self, BlockRow, BlockWrite, DiskRow};
use crate::disk;
use cybermanju_compression::TripleCompressor;
use cybermanju_crypto::keystore;
use cybermanju_db::Database;
use cybermanju_sync::create_backend;
use cybermanju_types::sync::StorageBackend;
use serde::Serialize;

/// Artifact magic, shared with `cybermanju_sync::pipeline::CYBE_MAGIC`.
const BLOCK_MAGIC: &[u8; 5] = b"CYBE1";
/// Payload flag: the bytes after the header are triple-compressed.
const FLAG_COMPRESSED: u8 = 0x01;

/// Where a chunk lives on a provider — flat, content-addressed, and under a
/// directory name `list_files(prefix)` can walk (backends are not recursive).
pub fn remote_path(disk_id: &str, chunk_hash: &str) -> String {
    format!("cybermanju_disk/{}/c/{}", disk_id, chunk_hash)
}

// ─── df ──────────────────────────────────────────────────────────────────────

/// One disk's line in `df`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    pub id: String,
    pub provider: String,
    pub size: u64,
    pub used: u64,
    pub free: u64,
    pub health: String,
}

/// The merged volume — `df(1)`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeDf {
    pub total_bytes: u64,
    pub used_bytes: u64,
    /// Usable bytes: Σ `max(0, capacity − used)` per attached disk, so one
    /// disk running over never hides another disk's free space.
    pub free_bytes: u64,
    pub disk_count: usize,
    pub disks: Vec<DiskUsage>,
}

/// Merged numbers for every attached disk. **Grows when a disk attaches.**
pub fn df(db: &Database) -> Result<VolumeDf, String> {
    let disks = catalog::attached_disks(db)?;
    let mut total = 0u64;
    let mut used = 0u64;
    let mut free = 0u64;
    let mut lines = Vec::with_capacity(disks.len());
    for row in &disks {
        total = total.saturating_add(row.capacity_bytes);
        used = used.saturating_add(row.used_bytes);
        let row_free = row.free_bytes();
        free = free.saturating_add(row_free);
        lines.push(DiskUsage {
            id: row.id.clone(),
            provider: row.provider.clone(),
            size: row.capacity_bytes,
            used: row.used_bytes,
            free: row_free,
            health: row.health.clone(),
        });
    }
    Ok(VolumeDf {
        total_bytes: total,
        used_bytes: used,
        free_bytes: free,
        disk_count: disks.len(),
        disks: lines,
    })
}

/// Usable bytes in the merged volume (0 when there is no volume to ask).
pub fn free_bytes(db: &Database) -> u64 {
    df(db).map(|usage| usage.free_bytes).unwrap_or(0)
}

/// Admission control (MISSING.md D1): refuse a payload the volume cannot
/// hold, before a single byte leaves.
///
/// No attached disk means nothing to enforce — the sync engine keeps working
/// for installs that never created a disk — so that case admits.
pub fn admit(db: &Database, bytes: u64) -> Result<(), String> {
    let disks = catalog::attached_disks(db)?;
    if disks.is_empty() {
        return Ok(());
    }
    let free = disks
        .iter()
        .map(DiskRow::free_bytes)
        .fold(0u64, u64::saturating_add);
    if bytes > free {
        return Err(format!(
            "disk_full: {} bytes needed but the volume has {} free across {} disk{}",
            bytes,
            free,
            disks.len(),
            if disks.len() == 1 { "" } else { "s" }
        ));
    }
    Ok(())
}

/// Charge provider space to the disk bound to `config_id`, so bytes the sync
/// engine consumes show up in `df`. A no-op when no disk is bound to it.
pub fn charge(db: &Database, config_id: &str, bytes: u64) -> Result<(), String> {
    let Some(mut row) = catalog::disk_for_config(db, config_id)? else {
        return Ok(());
    };
    row.used_bytes = row.used_bytes.saturating_add(bytes);
    row.updated_at = catalog::now_rfc3339();
    catalog::put_disk(db, &row)
}

// ─── block I/O ───────────────────────────────────────────────────────────────

/// Where a block ended up — what `PUT /api/volume/block/{lba}` returns.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockInfo {
    pub lba: u64,
    pub disk_id: String,
    pub slot: u64,
    pub chunk_hash: String,
    pub data_len: u32,
    pub disk_used: u64,
    pub disk_capacity: u64,
}

fn block_info(db: &Database, row: &BlockRow) -> Result<BlockInfo, String> {
    let disk = catalog::get_disk(db, &row.disk_id)?.ok_or_else(|| {
        format!(
            "integrity: block {} is placed on disk '{}' which no longer exists",
            row.lba, row.disk_id
        )
    })?;
    Ok(BlockInfo {
        lba: row.lba,
        disk_id: row.disk_id.clone(),
        slot: row.slot,
        chunk_hash: row.chunk_hash.clone(),
        data_len: row.data_len,
        disk_used: disk.used_bytes,
        disk_capacity: disk.capacity_bytes,
    })
}

/// Write one block into the volume at `lba`.
///
/// Order matters and is deliberate: **admit → place → upload → commit**. If
/// anything fails before the catalog commit, the uploaded object is removed
/// again, so a refused write never leaves a partial upload behind (D1).
pub fn put_block(db: &Database, lba: u64, data: &[u8]) -> Result<BlockInfo, String> {
    let disks = catalog::attached_disks(db)?;
    let first = disks
        .first()
        .ok_or_else(|| "disk_full: no .cybermanju disk is attached — 0 bytes free".to_string())?;
    let block_size = first.block_size;
    if data.len() > usize::try_from(block_size).unwrap_or(usize::MAX) {
        return Err(format!(
            "too_large: block {} is {} bytes but the block size is {}",
            lba,
            data.len(),
            block_size
        ));
    }

    let hash = blake3::hash(data).to_hex().to_string();
    let old = catalog::get_block_row(db, lba)?;
    if let Some(previous) = &old {
        if previous.chunk_hash == hash && previous.data_len as usize == data.len() {
            return block_info(db, previous);
        }
    }

    // 1. Admission. Replacing a block frees its bytes first, so only the
    //    growth has to fit.
    let old_len = old.as_ref().map(|row| row.data_len as u64).unwrap_or(0);
    let needed = (data.len() as u64).saturating_sub(old_len);
    admit(db, needed)?;

    // 2. Placement: spanned by default — first disk with both room and a free
    //    slot, in creation order; round-robin when the volume asks for it.
    let old_disk = match &old {
        Some(row) => catalog::get_disk(db, &row.disk_id)?,
        None => None,
    };
    let placement = place_block(db, data.len() as u64, old.as_ref(), old_disk.as_ref())?;

    // 3. Seal and upload.
    let passphrase = disk::passphrase_for(&placement.disk.id)?;
    let payload = encode_payload(data, &passphrase)?;
    let backend = backend_for(db, &placement.disk)?;
    let remote = remote_path(&placement.disk.id, &hash);
    if let Err(err) = upload_payload(backend.as_ref(), &payload, &remote) {
        return Err(format!("network: could not store block {}: {}", lba, err));
    }

    // 4. Commit the catalog, then reconcile the cached allocator.
    let mut target = placement.disk;
    let same_disk = old_disk
        .as_ref()
        .map(|d| d.id == target.id)
        .unwrap_or(false);
    if same_disk {
        target.used_bytes = target.used_bytes.saturating_sub(old_len);
    }
    target.used_bytes = target.used_bytes.saturating_add(data.len() as u64);
    target.blocks_written_since_checkpoint =
        target.blocks_written_since_checkpoint.saturating_add(1);
    target.updated_at = catalog::now_rfc3339();

    let old_disk_row = match &old_disk {
        Some(row) if row.id != target.id => {
            let mut released = row.clone();
            released.used_bytes = released.used_bytes.saturating_sub(old_len);
            released.updated_at = catalog::now_rfc3339();
            Some(released)
        }
        _ => None,
    };

    let new_row = BlockRow {
        lba,
        disk_id: target.id.clone(),
        slot: placement.slot,
        chunk_hash: hash,
        data_len: data.len() as u32,
    };
    let write = BlockWrite {
        old: old.clone(),
        new: new_row.clone(),
        target: target.clone(),
        old_disk: old_disk_row.clone(),
    };
    if let Err(err) = catalog::commit_block_write(db, &write) {
        // Nothing may linger: the catalog refused it, so the payload goes too.
        if let Err(remove_err) = backend.delete_file(&remote) {
            log::warn!(
                "could not remove the orphaned upload for block {}: {}",
                lba,
                remove_err
            );
        }
        return Err(err);
    }
    apply_to_allocators(&old, &old_disk_row, &target, &new_row);

    // 5. Periodic checkpoint — sealing is Argon2id, so it is batched.
    if target.blocks_written_since_checkpoint >= catalog::CHECKPOINT_EVERY {
        if let Err(err) = disk::checkpoint(db, &target.id) {
            log::warn!("checkpoint of {} deferred: {}", target.id, err);
        }
    }

    block_info(db, &new_row)
}

/// Read a whole block (optionally a byte range inside it).
///
/// The payload is downloaded, unsealed, decompressed and checked against
/// the BLAKE3 recorded when it was written; a mismatch is an `integrity:`
/// error, never a silently corrupt read.
pub fn read_block(
    db: &Database,
    lba: u64,
    offset: usize,
    len: Option<usize>,
) -> Result<Vec<u8>, String> {
    let row = catalog::get_block_row(db, lba)?
        .ok_or_else(|| format!("not_found: block {} is not written on this volume", lba))?;
    let disk_row = catalog::get_disk(db, &row.disk_id)?.ok_or_else(|| {
        format!(
            "integrity: block {} is placed on disk '{}' which no longer exists",
            lba, row.disk_id
        )
    })?;
    if !disk_row.attached() {
        return Err(format!(
            "unavailable: block {} lives on disk '{}' which is detached — attach it first",
            lba, row.disk_id
        ));
    }

    let passphrase = disk::passphrase_for(&disk_row.id)?;
    let backend = backend_for(db, &disk_row)?;
    let remote = remote_path(&disk_row.id, &row.chunk_hash);
    let payload = download_payload(backend.as_ref(), &remote)?;
    let plain = decode_payload(&payload, &passphrase)?;

    let actual = blake3::hash(&plain).to_hex().to_string();
    if actual != row.chunk_hash {
        return Err(format!(
            "integrity: block {} failed verification (recorded {}, read {})",
            lba, row.chunk_hash, actual
        ));
    }
    if plain.len() != row.data_len as usize {
        return Err(format!(
            "integrity: block {} is {} bytes but the catalog says {}",
            lba,
            plain.len(),
            row.data_len
        ));
    }

    let start = offset.min(plain.len());
    let end = match len {
        Some(len) => start.saturating_add(len).min(plain.len()),
        None => plain.len(),
    };
    Ok(plain[start..end].to_vec())
}

/// `GET /api/volume/block/{lba}` without the range.
pub fn get_block(db: &Database, lba: u64) -> Result<Vec<u8>, String> {
    read_block(db, lba, 0, None)
}

/// What the catalog says about a block, without touching the provider.
pub fn block_info_for(db: &Database, lba: u64) -> Result<Option<BlockInfo>, String> {
    match catalog::get_block_row(db, lba)? {
        Some(row) => block_info(db, &row).map(Some),
        None => Ok(None),
    }
}

// ─── placement ───────────────────────────────────────────────────────────────

struct Placement {
    disk: DiskRow,
    slot: u64,
}

/// Choose the disk and slot for a block of `new_len` bytes.
///
/// Spanned (default) walks disks in creation order and takes the first that
/// can hold the block *and* hand out a slot — which fills a disk to its
/// high-water mark before spilling to the next. A disk being rewritten keeps
/// its own slot available even when it has none spare, because the old block
/// frees one as soon as it lands.
fn place_block(
    db: &Database,
    new_len: u64,
    old: Option<&BlockRow>,
    old_disk: Option<&DiskRow>,
) -> Result<Placement, String> {
    let volume = catalog::get_volume(db)?;
    let disks = catalog::attached_disks(db)?;
    if disks.is_empty() {
        return Err("disk_full: no .cybermanju disk is attached — 0 bytes free".to_string());
    }

    let round_robin = volume.policy == "round_robin";
    let len = disks.len();
    let start = if round_robin {
        (volume.rr_cursor as usize) % len
    } else {
        0
    };
    let old_len = old.map(|row| row.data_len as u64).unwrap_or(0);

    for step in 0..len {
        let index = if round_robin {
            (start + step) % len
        } else {
            step
        };
        let candidate = &disks[index];
        let rewrites_same_disk = old_disk.map(|d| d.id == candidate.id).unwrap_or(false);
        let room = candidate
            .free_bytes()
            .saturating_add(if rewrites_same_disk { old_len } else { 0 });
        if room < new_len {
            continue;
        }
        let allocator = catalog::allocator_for(db, candidate)?;
        let slot = {
            let guard = allocator.lock().map_err(|e| e.to_string())?;
            match guard.allocate() {
                Some(slot) => Some(slot),
                None if rewrites_same_disk => old.map(|row| row.slot),
                None => None,
            }
        };
        let Some(slot) = slot else { continue };

        if round_robin {
            let mut updated = volume.clone();
            updated.rr_cursor = ((index + 1) % len) as u64;
            updated.updated_at = catalog::now_rfc3339();
            catalog::put_volume(db, &updated)?;
        }
        return Ok(Placement {
            disk: candidate.clone(),
            slot,
        });
    }

    let biggest_free = disks.iter().map(|row| row.free_bytes()).max().unwrap_or(0);
    Err(format!(
        "disk_full: {} bytes needed and no single disk can hold the block \
         (largest free: {} bytes across {} disk{})",
        new_len,
        biggest_free,
        disks.len(),
        if disks.len() == 1 { "" } else { "s" }
    ))
}

/// Mirror a committed write into the cached allocators. A cache that is
/// missing is fine (it is rebuilt from the rows, which already include the
/// write); a cache that cannot be updated is dropped rather than trusted.
fn apply_to_allocators(
    old: &Option<BlockRow>,
    old_disk: &Option<DiskRow>,
    target: &DiskRow,
    new_row: &BlockRow,
) {
    if let (Some(previous), Some(previous_disk)) = (old, old_disk) {
        if previous.disk_id != target.id {
            if let Some(shared) = catalog::cached_allocator(&previous_disk.id) {
                if let Ok(mut guard) = shared.lock() {
                    guard.vacate(previous.slot);
                }
            }
        }
    }
    let Some(shared) = catalog::cached_allocator(&target.id) else {
        return;
    };
    let Ok(mut guard) = shared.lock() else {
        return;
    };
    if let Some(previous) = old {
        if previous.disk_id == target.id {
            guard.vacate(previous.slot);
        }
    }
    if let Err(err) = guard.occupy(
        new_row.slot,
        new_row.lba,
        &new_row.chunk_hash,
        new_row.data_len,
    ) {
        log::warn!(
            "allocator for {} went out of sync ({}); rebuilding from the catalog",
            target.id,
            err
        );
        drop(guard);
        catalog::forget_allocator(&target.id);
    }
}

// ─── providers ───────────────────────────────────────────────────────────────

fn backend_for(db: &Database, row: &DiskRow) -> Result<Box<dyn StorageBackend>, String> {
    let config = db
        .list_sync_configs()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|config| config.id == row.config_id)
        .ok_or_else(|| {
            format!(
                "not_found: provider config '{}' for disk '{}' does not exist",
                row.config_id, row.id
            )
        })?;
    create_backend(&config)
}

/// The backend interface is path-based, so payloads take a short trip
/// through a temp file.
fn upload_payload(
    backend: &dyn StorageBackend,
    payload: &[u8],
    remote: &str,
) -> Result<(), String> {
    let tmp = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    std::fs::write(tmp.path(), payload).map_err(|e| e.to_string())?;
    let local = tmp
        .path()
        .to_str()
        .ok_or_else(|| "integrity: temporary file path is not UTF-8".to_string())?;
    backend.upload_file(local, remote)?;
    Ok(())
}

fn download_payload(backend: &dyn StorageBackend, remote: &str) -> Result<Vec<u8>, String> {
    let tmp = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let local = tmp
        .path()
        .to_str()
        .ok_or_else(|| "integrity: temporary file path is not UTF-8".to_string())?;
    backend.download_file(remote, local)?;
    std::fs::read(tmp.path()).map_err(|e| e.to_string())
}

// ─── payload transform ───────────────────────────────────────────────────────

/// `compress → seal → CYBE1 | flags | sealed`.
///
/// Compression is applied only when it actually helps: `compress_triple`
/// returns incompressible input untouched, and the flag records which shape
/// went in, so the read side never guesses.
fn encode_payload(data: &[u8], passphrase: &str) -> Result<Vec<u8>, String> {
    let compressor = TripleCompressor::new();
    let (compressed, _) = compressor
        .compress_triple(data)
        .map_err(|e| format!("integrity: compression failed: {}", e))?;
    let (inner, flags) = if compressed.len() < data.len() {
        (compressed, FLAG_COMPRESSED)
    } else {
        (data.to_vec(), 0u8)
    };
    let sealed = keystore::seal(passphrase, &inner)
        .map_err(|e| format!("integrity: encryption failed: {}", e))?;
    let mut out = Vec::with_capacity(BLOCK_MAGIC.len() + 1 + sealed.len());
    out.extend_from_slice(BLOCK_MAGIC);
    out.push(flags);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Inverse of [`encode_payload`].
fn decode_payload(payload: &[u8], passphrase: &str) -> Result<Vec<u8>, String> {
    if payload.len() < BLOCK_MAGIC.len() + 1 {
        return Err("integrity: block payload is too short".to_string());
    }
    if &payload[..BLOCK_MAGIC.len()] != BLOCK_MAGIC {
        return Err(format!(
            "integrity: block does not carry the {} magic",
            String::from_utf8_lossy(BLOCK_MAGIC)
        ));
    }
    let flags = payload[BLOCK_MAGIC.len()];
    let sealed = &payload[BLOCK_MAGIC.len() + 1..];
    let inner = keystore::open_sealed(passphrase, sealed)
        .map_err(|e| format!("auth: block did not open: {}", e))?;
    if flags & FLAG_COMPRESSED != 0 {
        let compressor = TripleCompressor::new();
        let (plain, _) = compressor
            .decompress_triple(&inner)
            .map_err(|e| format!("integrity: block could not be decompressed: {}", e))?;
        Ok(plain)
    } else {
        Ok(inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Fixture;
    use crate::DEFAULT_BLOCK_SIZE;

    #[test]
    fn df_grows_when_a_disk_is_attached_and_shrinks_when_it_leaves() {
        let fx = Fixture::new();
        let empty = df(&fx.db).expect("df");
        assert_eq!(empty.total_bytes, 0);
        assert_eq!(empty.disk_count, 0);
        assert_eq!(empty.free_bytes, 0);

        let a = fx.create(1024 * 1024);
        let one = df(&fx.db).expect("df");
        assert_eq!(
            one.total_bytes,
            1024 * 1024,
            "one disk contributes its size"
        );
        assert_eq!(one.disk_count, 1);
        assert_eq!(one.disks[0].free, 1024 * 1024);

        let b = fx.create(2 * 1024 * 1024);
        let two = df(&fx.db).expect("df");
        assert_eq!(
            two.total_bytes,
            3 * 1024 * 1024,
            "attaching a second disk grows the volume"
        );
        assert_eq!(two.disk_count, 2);

        crate::disk::detach(&fx.db, &b.id).expect("detach");
        let after_detach = df(&fx.db).expect("df");
        assert_eq!(
            after_detach.total_bytes,
            1024 * 1024,
            "detaching gives the space back"
        );
        crate::disk::attach(&fx.db, &b.id, "hunter2").expect("attach");
        assert_eq!(
            df(&fx.db).expect("df").total_bytes,
            3 * 1024 * 1024,
            "reattaching restores it"
        );
        let _ = a;
    }

    #[test]
    fn spanned_placement_fills_a_disk_before_spilling_to_the_next() {
        let fx = Fixture::new();
        // Disk A: exactly one block. Disk B: three.
        let a = fx.create(u64::from(DEFAULT_BLOCK_SIZE));
        let b = fx.create(3 * u64::from(DEFAULT_BLOCK_SIZE));

        for lba in 0..4u64 {
            put_block(&fx.db, lba, &[lba as u8; 64]).expect("write");
        }
        let rows = catalog::list_block_rows(&fx.db).expect("rows");
        assert_eq!(rows.len(), 4);
        let on_a = rows.iter().filter(|row| row.disk_id == a.id).count();
        let on_b = rows.iter().filter(|row| row.disk_id == b.id).count();
        assert_eq!(on_a, 1, "A is filled to its high-water mark first");
        assert_eq!(on_b, 3, "then the write spills to B");
        let a_row = catalog::get_disk(&fx.db, &a.id).expect("row").expect("row");
        let allocator = catalog::allocator_for(&fx.db, &a_row).expect("allocator");
        assert!(
            allocator.lock().expect("lock").allocate().is_none(),
            "A is out of slots — it cannot take a second block"
        );
    }

    #[test]
    fn blocks_round_trip_and_are_verified_on_read() {
        let fx = Fixture::new();
        let row = fx.create(1024 * 1024);
        let payload: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
        let info = put_block(&fx.db, 3, &payload).expect("write");
        assert_eq!(info.data_len, payload.len() as u32);
        assert_eq!(info.disk_id, row.id);
        assert_eq!(info.disk_capacity, 1024 * 1024);

        assert_eq!(get_block(&fx.db, 3).expect("read"), payload);
        assert_eq!(
            read_block(&fx.db, 3, 10, Some(20)).expect("range"),
            payload[10..30].to_vec(),
            "range reads slice the block"
        );
        assert!(read_block(&fx.db, 3, 9000, None).expect("eof").is_empty());

        // Rewriting the same bytes is idempotent: same placement, no double
        // charge, no second object.
        let again = put_block(&fx.db, 3, &payload).expect("rewrite");
        assert_eq!(again.slot, info.slot);
        assert_eq!(again.chunk_hash, info.chunk_hash);
        assert_eq!(
            catalog::get_disk(&fx.db, &row.id)
                .expect("row")
                .expect("row")
                .used_bytes,
            payload.len() as u64,
            "identical rewrite is not charged twice"
        );

        let missing = get_block(&fx.db, 99).expect_err("missing");
        assert!(missing.starts_with("not_found:"), "{missing}");
    }

    #[test]
    fn admission_refuses_what_the_volume_cannot_hold() {
        let fx = Fixture::new();
        assert!(
            admit(&fx.db, u64::MAX).is_ok(),
            "no disk attached → nothing to enforce"
        );

        fx.create(1024 * 1024);
        admit(&fx.db, 1024 * 1024).expect("exactly the capacity fits");
        let err = admit(&fx.db, 1024 * 1024 + 1).expect_err("one byte too far");
        assert!(err.starts_with("disk_full:"), "{err}");

        // A payload that does not fit is refused *before* anything is stored.
        let before = fx.remote_files();
        let err = put_block(&fx.db, 42, &vec![7u8; 1024 * 1024]).expect_err("too big");
        assert!(err.starts_with("too_large:"), "{err}");
        assert_eq!(fx.remote_files(), before, "nothing was uploaded");
    }

    #[test]
    fn sync_charges_show_up_in_df_and_in_admission() {
        let fx = Fixture::new();
        let row = fx.create(1024 * 1024);
        charge(&fx.db, &row.config_id, 700 * 1024).expect("charge");
        let usage = df(&fx.db).expect("df");
        assert_eq!(usage.used_bytes, 700 * 1024, "df sees the sync bytes");
        assert_eq!(
            usage.free_bytes,
            1024 * 1024 - 700 * 1024,
            "free space accounts for them"
        );
        admit(&fx.db, 324 * 1024).expect("still room");
        let err = admit(&fx.db, 324 * 1024 + 1).expect_err("overflow");
        assert!(err.starts_with("disk_full:"), "{err}");
        // A config with no disk bound is a no-op, not a failure.
        charge(&fx.db, "unbound-config", 10).expect("unbound");
    }
}
