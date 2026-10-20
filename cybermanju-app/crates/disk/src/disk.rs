// CyberManju OS — Disk (crates/disk/src/disk.rs).
//! Disk lifecycle: `create` · `attach` · `detach` · `resize` · `check` ·
//! `destroy`, plus the sealed container that outlives the local catalog.
//!
//! Two truths, reconciled on every `attach`:
//!
//! * **redb (`disks` + `block_map`)** — the working catalog: what the volume
//!   can address right now, written transactionally.
//! * **the `.cybermanju` container** — a sealed, portable checkpoint: magic,
//!   capacity, provider binding and the block map, readable when the laptop
//!   (and its database) is gone.
//!
//! The catalog is authoritative while it exists. `attach` imports container
//! blocks the catalog is missing (restore after catalog loss), accepts
//! catalog blocks the container has not been told about yet (up to
//! [`catalog::CHECKPOINT_EVERY`] pending writes), and refuses outright
//! conflicts — a block that means two different things is an `integrity:`
//! error, because one of the two is lying.

use crate::allocator::Allocator;
use crate::catalog::{self, BlockRow, DiskRow};
use crate::superblock::{self, Superblock};
use crate::DEFAULT_BLOCK_SIZE;
use cybermanju_db::Database;
use cybermanju_sync::quota;
use cybermanju_types::sync::SyncConfig;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// ─── passphrase keyring ──────────────────────────────────────────────────────
//
// `keystore::seal` is passphrase-derived (Argon2id), so sealing a container
// or reading a block needs the passphrase again after the call that created
// the disk. It is cached here for the lifetime of an *attached* disk and
// dropped on detach/destroy — the lifetime a kernel would give a mounted
// volume's key.

fn keyring() -> &'static Mutex<HashMap<String, String>> {
    static KEYRING: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    KEYRING.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn remember_passphrase(disk_id: &str, passphrase: &str) {
    if let Ok(mut guard) = keyring().lock() {
        guard.insert(disk_id.to_string(), passphrase.to_string());
    }
}

/// The passphrase of an unlocked disk, or `auth:` when it is locked.
pub(crate) fn passphrase_for(disk_id: &str) -> Result<String, String> {
    peek_passphrase(disk_id)
        .ok_or_else(|| format!("auth: disk {} is locked — attach it first", disk_id))
}

fn peek_passphrase(disk_id: &str) -> Option<String> {
    keyring()
        .lock()
        .ok()
        .and_then(|guard| guard.get(disk_id).cloned())
}

pub(crate) fn forget_passphrase(disk_id: &str) {
    if let Ok(mut guard) = keyring().lock() {
        guard.remove(disk_id);
    }
}

// ─── paths ───────────────────────────────────────────────────────────────────

/// Where a disk's container lives when the caller does not say: the app data
/// directory (`CYBERMANJU_DATA_DIR` → `DB_PATH`'s parent → platform data dir
/// → `~/.cybermanju`), then `disks/<id>.cybermanju`.
pub fn default_container_path(disk_id: &str) -> PathBuf {
    let base =
        cybermanju_crypto::keystore::data_dir().unwrap_or_else(|| PathBuf::from(".cybermanju"));
    base.join("disks").join(format!("{}.cybermanju", disk_id))
}

// ─── create ──────────────────────────────────────────────────────────────────

/// Create a disk of `size_bytes` bound to the provider `config_id`.
///
/// `size_bytes` is what the user picked, and it is enforced: zero is
/// refused, anything smaller than one block could not hold anything, and
/// anything larger than the provider's reported free space is refused up
/// front so `df` never promises space that is not there.
pub fn create(
    db: &Database,
    config_id: &str,
    size_bytes: u64,
    passphrase: &str,
) -> Result<DiskRow, String> {
    let disk_id = format!("disk-{}", uuid::Uuid::new_v4());
    let path = default_container_path(&disk_id);
    create_inner(db, config_id, size_bytes, passphrase, &disk_id, &path)
}

/// [`create`] with an explicit container path (tests, migrations, imports).
/// The disk id is taken from the file stem, so importing a container keeps
/// its identity.
pub fn create_at(
    db: &Database,
    config_id: &str,
    size_bytes: u64,
    passphrase: &str,
    container: &Path,
) -> Result<DiskRow, String> {
    let stem = container
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let disk_id = if stem.is_empty() {
        format!("disk-{}", uuid::Uuid::new_v4())
    } else {
        stem.trim_end_matches(".cybermanju").to_string()
    };
    create_inner(db, config_id, size_bytes, passphrase, &disk_id, container)
}

fn create_inner(
    db: &Database,
    config_id: &str,
    size_bytes: u64,
    passphrase: &str,
    disk_id: &str,
    container: &Path,
) -> Result<DiskRow, String> {
    if passphrase.is_empty() {
        return Err("unsupported: a disk passphrase must not be empty".to_string());
    }
    if size_bytes == 0 {
        return Err("unsupported: size_bytes must be greater than 0".to_string());
    }
    if size_bytes < u64::from(DEFAULT_BLOCK_SIZE) {
        return Err(format!(
            "unsupported: {} bytes is smaller than one block ({} bytes)",
            size_bytes, DEFAULT_BLOCK_SIZE
        ));
    }
    if container.exists() {
        return Err(format!(
            "conflict: '{}' already exists — refusing to overwrite a container",
            container.display()
        ));
    }

    let config = find_config(db, config_id)?;
    if let Some(headroom) = quota_headroom(&config) {
        if size_bytes > headroom {
            return Err(format!(
                "disk_full: {} bytes exceeds the {} free bytes reported by provider '{}'",
                size_bytes, headroom, config_id
            ));
        }
    }

    let volume = catalog::get_volume(db)?;
    let block_size = DEFAULT_BLOCK_SIZE;
    let slots = (size_bytes / u64::from(block_size)) as usize;
    let allocator = Allocator::new(block_size, slots);
    let section = allocator.encode();
    let now = catalog::now_rfc3339();
    let mut header = Superblock::new(
        disk_id,
        &volume.uuid,
        config_id,
        size_bytes,
        block_size,
        format!("disk/{}", disk_id),
        &now,
        superblock::blake3_hex(&section),
    );
    header.created_at = now.clone();

    superblock::write_container(container, passphrase, &header, &section).inspect_err(|_| {
        let _ = std::fs::remove_file(container);
    })?;

    let row = DiskRow {
        id: disk_id.to_string(),
        name: format!("Disk {}", &disk_id[..disk_id.len().min(12)]),
        provider: config.backend_type.to_string(),
        config_id: config_id.to_string(),
        volume_uuid: volume.uuid,
        capacity_bytes: size_bytes,
        block_size,
        used_bytes: 0,
        placement_order: next_placement_order(db)?,
        state: "attached".to_string(),
        health: "ok".to_string(),
        container_path: container.to_string_lossy().to_string(),
        blocks_written_since_checkpoint: 0,
        holds_keys: false,
        created_at: now.clone(),
        updated_at: now,
    };

    if let Err(err) = catalog::put_disk(db, &row) {
        let _ = std::fs::remove_file(container);
        return Err(err);
    }
    catalog::seed_allocator(disk_id, allocator);
    remember_passphrase(disk_id, passphrase);
    Ok(row)
}

fn find_config(db: &Database, config_id: &str) -> Result<SyncConfig, String> {
    db.list_sync_configs()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|config| config.id == config_id)
        .ok_or_else(|| {
            format!(
                "not_found: sync config '{}' does not exist — connect a provider first",
                config_id
            )
        })
}

/// Remaining bytes on a provider, when it publishes one.
///
/// Best effort: a provider that cannot be asked (offline, or a backend with
/// no quota endpoint) yields `None` and nothing is refused on a guess.
fn quota_headroom(config: &SyncConfig) -> Option<u64> {
    match quota::usage(config) {
        Ok(usage) => usage.remaining_bytes().or(usage.total_bytes),
        Err(err) => {
            log::warn!(
                "could not verify provider quota for '{}': {}",
                config.id,
                err
            );
            None
        }
    }
}

fn next_placement_order(db: &Database) -> Result<u64, String> {
    let disks = catalog::list_disks(db)?;
    Ok(disks
        .iter()
        .map(|row| row.placement_order)
        .max()
        .map(|max| max + 1)
        .unwrap_or(0))
}

// ─── attach / detach ─────────────────────────────────────────────────────────

/// Unlock a disk: verify its container, reconcile it with the catalog, and
/// start counting it towards `df`.
pub fn attach(db: &Database, disk_id: &str, passphrase: &str) -> Result<DiskRow, String> {
    let mut row = get(db, disk_id)?.ok_or_else(|| not_found(disk_id))?;
    let path = Path::new(&row.container_path);

    let (header, section) = superblock::read_container(path, passphrase)?;
    verify_header_matches_row(&header, &row)?;
    let container_blocks = Allocator::decode(&section)?;
    if container_blocks.block_count() != row.block_count() {
        return Err(format!(
            "integrity: container holds {} blocks but the catalog says {}",
            container_blocks.block_count(),
            row.block_count()
        ));
    }

    let mut from_container: BTreeMap<u64, (u64, String, u32)> = BTreeMap::new();
    for (slot, block) in container_blocks.blocks() {
        from_container.insert(block.lba, (slot, block.hash.clone(), block.data_len));
    }
    let catalog_rows = catalog::disk_block_rows(db, disk_id)?;
    let from_catalog: BTreeMap<u64, &BlockRow> = catalog_rows.iter().map(|r| (r.lba, r)).collect();

    let mut import: Vec<BlockRow> = Vec::new();
    for (lba, (slot, hash, len)) in &from_container {
        match from_catalog.get(lba) {
            Some(existing) => {
                if existing.slot != *slot
                    || existing.chunk_hash != *hash
                    || existing.data_len != *len
                    || existing.disk_id != disk_id
                {
                    return Err(format!(
                        "integrity: block {} means different things in the container and the \
                         catalog (container: slot {} hash {}, catalog: slot {} hash {}) — \
                         refusing to attach",
                        lba, slot, hash, existing.slot, existing.chunk_hash
                    ));
                }
            }
            None => import.push(BlockRow {
                lba: *lba,
                disk_id: disk_id.to_string(),
                slot: *slot,
                chunk_hash: hash.clone(),
                data_len: *len,
            }),
        }
    }
    if !import.is_empty() {
        catalog::import_disk_blocks(db, &import)?;
        log::info!(
            "restored {} blocks of {} from its container",
            import.len(),
            disk_id
        );
    }

    row.state = "attached".to_string();
    row.updated_at = catalog::now_rfc3339();
    catalog::put_disk(db, &row)?;
    remember_passphrase(disk_id, passphrase);
    catalog::forget_allocator(disk_id);
    catalog::allocator_for(db, &row)?;
    // Seal with what we just reconciled: the container starts its attached
    // life agreeing with the catalog.
    seal_container(db, &row, passphrase)?;
    let mut sealed = row.clone();
    sealed.blocks_written_since_checkpoint = 0;
    catalog::put_disk(db, &sealed)?;
    Ok(sealed)
}

fn verify_header_matches_row(header: &Superblock, row: &DiskRow) -> Result<(), String> {
    let mut problems = Vec::new();
    if header.disk_id != row.id {
        problems.push(format!("disk id {} vs {}", header.disk_id, row.id));
    }
    if header.config_id != row.config_id {
        problems.push(format!(
            "provider {} vs {}",
            header.config_id, row.config_id
        ));
    }
    if header.capacity_bytes != row.capacity_bytes {
        problems.push(format!(
            "capacity {} vs {}",
            header.capacity_bytes, row.capacity_bytes
        ));
    }
    if header.block_size != row.block_size {
        problems.push(format!(
            "block size {} vs {}",
            header.block_size, row.block_size
        ));
    }
    if header.volume_uuid != row.volume_uuid {
        problems.push(format!(
            "volume {} vs {}",
            header.volume_uuid, row.volume_uuid
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "integrity: container does not match the catalog row for {}: {}",
            row.id,
            problems.join(", ")
        ))
    }
}

/// Lock a disk: seal the container while we still hold its passphrase, then
/// stop counting it towards `df`.
pub fn detach(db: &Database, disk_id: &str) -> Result<DiskRow, String> {
    let mut row = get(db, disk_id)?.ok_or_else(|| not_found(disk_id))?;
    if !row.attached() {
        return Ok(row);
    }
    match peek_passphrase(disk_id) {
        Some(passphrase) => seal_container(db, &row, &passphrase).map_err(|e| {
            format!(
                "integrity: cannot checkpoint '{}' before detaching: {}",
                disk_id, e
            )
        })?,
        None => log::warn!(
            "detaching {} without its passphrase — the container keeps its last checkpoint",
            disk_id
        ),
    }
    row.state = "detached".to_string();
    row.updated_at = catalog::now_rfc3339();
    row.blocks_written_since_checkpoint = 0;
    catalog::put_disk(db, &row)?;
    forget_passphrase(disk_id);
    catalog::forget_allocator(disk_id);
    Ok(row)
}

// ─── resize ──────────────────────────────────────────────────────────────────

/// Change the choosable size. Growing is free; shrinking refuses while any
/// block would fall off the end, because a disk that silently drops data is
/// not a disk.
pub fn resize(db: &Database, disk_id: &str, new_size: u64) -> Result<DiskRow, String> {
    if new_size == 0 {
        return Err("unsupported: size must be greater than 0".to_string());
    }
    let row = get(db, disk_id)?.ok_or_else(|| not_found(disk_id))?;
    if new_size < u64::from(row.block_size) {
        return Err(format!(
            "unsupported: {} bytes is smaller than one block ({} bytes)",
            new_size, row.block_size
        ));
    }
    if !row.attached() {
        return Err(format!(
            "conflict: disk {} is detached — attach it before resizing",
            disk_id
        ));
    }
    // Fail before mutating anything if the container cannot be re-sealed.
    let passphrase = passphrase_for(disk_id)?;

    catalog::forget_allocator(disk_id);
    let used_slots = {
        let allocator = catalog::allocator_for(db, &row)?;
        let guard = allocator.lock().map_err(|e| e.to_string())?;
        guard.used_slots()
    };
    let old_slots = row.block_count();
    let new_slots = (new_size / u64::from(row.block_size)) as usize;
    if new_slots < used_slots {
        return Err(format!(
            "unsupported: cannot shrink {} to {} bytes — {} of {} blocks are in use",
            disk_id, new_size, used_slots, old_slots
        ));
    }

    let previous = row.clone();
    let mut updated = row;
    updated.capacity_bytes = new_size;
    updated.updated_at = catalog::now_rfc3339();
    catalog::put_disk(db, &updated)?;
    catalog::forget_allocator(disk_id);
    if let Err(err) = catalog::allocator_for(db, &updated).and_then(|_| {
        seal_container(db, &updated, &passphrase)?;
        Ok(())
    }) {
        // Roll back: a row that promises a size its container does not have
        // would fail every later attach.
        let _ = catalog::put_disk(db, &previous);
        catalog::forget_allocator(disk_id);
        let _ = catalog::allocator_for(db, &previous);
        return Err(err);
    }
    let mut sealed = updated;
    sealed.blocks_written_since_checkpoint = 0;
    catalog::put_disk(db, &sealed)?;
    Ok(sealed)
}

// ─── checkpoint ──────────────────────────────────────────────────────────────

/// Re-seal the container from the live catalog and reset the write counter.
pub fn checkpoint(db: &Database, disk_id: &str) -> Result<(), String> {
    let row = get(db, disk_id)?.ok_or_else(|| not_found(disk_id))?;
    let passphrase = passphrase_for(disk_id)?;
    seal_container(db, &row, &passphrase)?;
    let mut sealed = row;
    sealed.blocks_written_since_checkpoint = 0;
    sealed.updated_at = catalog::now_rfc3339();
    catalog::put_disk(db, &sealed)
}

fn seal_container(db: &Database, row: &DiskRow, passphrase: &str) -> Result<(), String> {
    let allocator = catalog::allocator_for(db, row)?;
    let section = {
        let guard = allocator.lock().map_err(|e| e.to_string())?;
        guard.encode()
    };
    let mut header = Superblock::new(
        &row.id,
        &row.volume_uuid,
        &row.config_id,
        row.capacity_bytes,
        row.block_size,
        format!("disk/{}", row.id),
        &row.updated_at,
        superblock::blake3_hex(&section),
    );
    header.created_at = row.created_at.clone();
    superblock::write_container(
        Path::new(&row.container_path),
        passphrase,
        &header,
        &section,
    )
}

// ─── fsck ────────────────────────────────────────────────────────────────────

/// What `check()` found. `ok` is `true` only when the container opened, its
/// checksum verified, and its block map agreed with the catalog.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckReport {
    pub disk_id: String,
    /// Blocks the catalog can address.
    pub blocks: usize,
    /// Blocks the container commits to.
    pub container_blocks: usize,
    /// Container blocks the catalog cannot reach — payload sitting on the
    /// provider with no volume LBA pointing at it (AGENT-7's GC material).
    pub orphans: usize,
    /// Catalog blocks not sealed into the container yet. Expected to be
    /// small (≤ [`catalog::CHECKPOINT_EVERY`]); reported, not an error.
    pub pending_checkpoint: usize,
    pub locked: bool,
    pub ok: bool,
    pub problems: Vec<String>,
}

pub fn check(db: &Database, disk_id: &str) -> Result<CheckReport, String> {
    let row = get(db, disk_id)?.ok_or_else(|| not_found(disk_id))?;
    let catalog_rows = catalog::disk_block_rows(db, disk_id)?;
    let mut report = CheckReport {
        disk_id: disk_id.to_string(),
        blocks: catalog_rows.len(),
        container_blocks: 0,
        orphans: 0,
        pending_checkpoint: 0,
        locked: false,
        ok: true,
        problems: Vec::new(),
    };

    let passphrase = match peek_passphrase(disk_id) {
        Some(passphrase) => passphrase,
        None => {
            report.locked = true;
            report.ok = false;
            report.problems.push(format!(
                "auth: disk {} is locked — attach it before running check",
                disk_id
            ));
            return Ok(report);
        }
    };

    let (header, section) =
        match superblock::read_container(Path::new(&row.container_path), &passphrase) {
            Ok(value) => value,
            Err(err) => {
                report.ok = false;
                report.problems.push(err);
                return Ok(report);
            }
        };
    if let Err(err) = verify_header_matches_row(&header, &row) {
        report.ok = false;
        report.problems.push(err);
    }
    let container = match Allocator::decode(&section) {
        Ok(allocator) => allocator,
        Err(err) => {
            report.ok = false;
            report.problems.push(err);
            return Ok(report);
        }
    };

    let mut container_by_lba: BTreeMap<u64, (u64, String, u32)> = BTreeMap::new();
    for (slot, block) in container.blocks() {
        container_by_lba.insert(block.lba, (slot, block.hash.clone(), block.data_len));
    }
    report.container_blocks = container_by_lba.len();

    let mut catalog_by_lba: BTreeMap<u64, &BlockRow> = BTreeMap::new();
    for row in &catalog_rows {
        catalog_by_lba.insert(row.lba, row);
    }

    for (lba, (slot, hash, len)) in &container_by_lba {
        match catalog_by_lba.get(lba) {
            Some(found) => {
                if found.slot != *slot || &found.chunk_hash != hash || found.data_len != *len {
                    report.ok = false;
                    report.problems.push(format!(
                        "integrity: block {} disagrees with the container \
                         (catalog slot {} hash {}, container slot {} hash {})",
                        lba, found.slot, found.chunk_hash, slot, hash
                    ));
                }
            }
            None => {
                report.orphans += 1;
                report.ok = false;
                report.problems.push(format!(
                    "integrity: block {} exists in the container but is not reachable \
                     from the volume",
                    lba
                ));
            }
        }
    }
    report.pending_checkpoint = catalog_by_lba
        .keys()
        .filter(|lba| !container_by_lba.contains_key(lba))
        .count();
    Ok(report)
}

// ─── destroy / list / get ────────────────────────────────────────────────────

/// Delete a disk's catalog rows and its container.
///
/// The remote objects are **not** deleted: they are content-addressed and
/// may be referenced elsewhere, so sweeping them is AGENT-7's GC.
pub fn destroy(db: &Database, disk_id: &str) -> Result<(), String> {
    let row = get(db, disk_id)?.ok_or_else(|| not_found(disk_id))?;
    if row.attached() {
        detach(db, disk_id)?;
    }
    catalog::remove_disk_blocks(db, disk_id)?;
    catalog::forget_allocator(disk_id);
    forget_passphrase(disk_id);
    catalog::remove_disk(db, disk_id)?;
    let path = Path::new(&row.container_path);
    if path.exists() {
        if let Err(err) = std::fs::remove_file(path) {
            log::warn!("could not remove container '{}': {}", path.display(), err);
        }
    }
    Ok(())
}

pub fn get(db: &Database, disk_id: &str) -> Result<Option<DiskRow>, String> {
    catalog::get_disk(db, disk_id)
}

pub fn list(db: &Database) -> Result<Vec<DiskRow>, String> {
    catalog::list_disks(db)
}

/// Designate the single key-holder disk (clears the flag on all others).
pub fn set_key_holder(db: &Database, disk_id: &str) -> Result<DiskRow, String> {
    catalog::set_key_holder(db, disk_id)
}

fn not_found(disk_id: &str) -> String {
    format!("not_found: .cybermanju disk '{}' does not exist", disk_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Fixture;
    use crate::volume;

    #[test]
    fn create_refuses_sizes_and_providers_that_could_never_work() {
        let fx = Fixture::new();
        let path = |name: &str| fx.dir.path().join(name);

        let err = create_at(
            &fx.db,
            &fx.config_id,
            0,
            "p",
            path("a.cybermanju").as_path(),
        )
        .expect_err("zero");
        assert!(err.starts_with("unsupported:"), "{err}");

        let err = create_at(
            &fx.db,
            &fx.config_id,
            4096,
            "p",
            path("b.cybermanju").as_path(),
        )
        .expect_err("smaller than a block");
        assert!(err.starts_with("unsupported:"), "{err}");

        let err = create_at(
            &fx.db,
            "no-such-config",
            1024 * 1024,
            "p",
            path("c.cybermanju").as_path(),
        )
        .expect_err("unknown provider");
        assert!(err.starts_with("not_found:"), "{err}");

        let err = create_at(
            &fx.db,
            &fx.config_id,
            1024 * 1024,
            "",
            path("d.cybermanju").as_path(),
        )
        .expect_err("empty passphrase");
        assert!(err.starts_with("unsupported:"), "{err}");

        // A second disk must not be able to clobber the first container.
        let row = fx.create(1024 * 1024);
        let err = create_at(
            &fx.db,
            &fx.config_id,
            1024 * 1024,
            "p",
            Path::new(&row.container_path),
        )
        .expect_err("existing container");
        assert!(err.starts_with("conflict:"), "{err}");
    }

    #[test]
    fn create_attach_detach_round_trips_through_the_sealed_container() {
        let fx = Fixture::new();
        let row = fx.create(4 * 1024 * 1024);
        assert_eq!(row.capacity_bytes, 4 * 1024 * 1024, "choosable size kept");
        assert_eq!(row.block_size, DEFAULT_BLOCK_SIZE);
        assert_eq!(row.state, "attached");
        assert!(
            Path::new(&row.container_path).is_file(),
            "container written"
        );

        let bytes = std::fs::read(&row.container_path).expect("read container");
        assert_eq!(&bytes[..7], crate::DISK_MAGIC, "container starts CYBMJU1");

        let detached = detach(&fx.db, &row.id).expect("detach");
        assert_eq!(detached.state, "detached");
        assert!(
            passphrase_for(&row.id).is_err(),
            "detach drops the cached passphrase"
        );
        assert!(
            volume::df(&fx.db).expect("df").disks.is_empty(),
            "a detached disk is not part of the volume"
        );

        let attached = attach(&fx.db, &row.id, "hunter2").expect("attach");
        assert_eq!(attached.state, "attached");
        assert!(passphrase_for(&row.id).is_ok());

        let err = attach(&fx.db, &row.id, "nope").expect_err("wrong passphrase");
        assert!(err.starts_with("auth:"), "{err}");
    }

    #[test]
    fn resize_grows_and_refuses_a_shrink_that_would_drop_data() {
        let fx = Fixture::new();
        let row = fx.create(1024 * 1024);

        let grown = resize(&fx.db, &row.id, 4 * 1024 * 1024).expect("grow");
        assert_eq!(grown.capacity_bytes, 4 * 1024 * 1024);
        assert_eq!(
            grown.container_path, row.container_path,
            "resize keeps the container"
        );

        volume::put_block(&fx.db, 0, &[7u8; 1024]).expect("block 0");
        volume::put_block(&fx.db, 1, &[8u8; 1024]).expect("block 1");

        let err = resize(&fx.db, &row.id, DEFAULT_BLOCK_SIZE as u64).expect_err("shrink");
        assert!(err.starts_with("unsupported:"), "{err}");

        let shrunk = resize(&fx.db, &row.id, 2 * u64::from(DEFAULT_BLOCK_SIZE))
            .expect("shrink that still fits");
        assert_eq!(shrunk.capacity_bytes, 2 * u64::from(DEFAULT_BLOCK_SIZE));
        assert_eq!(
            volume::get_block(&fx.db, 0).expect("read back"),
            vec![7u8; 1024],
            "data survives a resize"
        );
    }

    #[test]
    fn check_reports_pending_checkpoints_without_calling_them_corruption() {
        let fx = Fixture::new();
        let row = fx.create(1024 * 1024);
        let clean = check(&fx.db, &row.id).expect("check");
        assert!(clean.ok, "{:?}", clean.problems);
        assert_eq!(clean.blocks, 0);

        volume::put_block(&fx.db, 0, &[1u8; 512]).expect("block");
        let pending = check(&fx.db, &row.id).expect("check");
        assert!(pending.ok, "{:?}", pending.problems);
        assert_eq!(pending.pending_checkpoint, 1, "pending is not corruption");
        assert_eq!(pending.orphans, 0);

        forget_passphrase(&row.id);
        let locked = check(&fx.db, &row.id).expect("check");
        assert!(!locked.ok);
        assert!(locked.locked);
        assert!(
            locked.problems[0].starts_with("auth:"),
            "{:?}",
            locked.problems
        );
    }

    #[test]
    fn destroy_removes_the_row_and_container_but_leaves_payload_for_gc() {
        let fx = Fixture::new();
        let row = fx.create(1024 * 1024);
        volume::put_block(&fx.db, 0, &[9u8; 100]).expect("block");
        assert_eq!(fx.remote_blocks(&row.id), 1, "payload on the provider");

        destroy(&fx.db, &row.id).expect("destroy");
        assert!(get(&fx.db, &row.id).expect("get").is_none(), "row gone");
        assert!(!Path::new(&row.container_path).exists(), "container gone");
        assert_eq!(fx.remote_blocks(&row.id), 1, "payload left for GC");
    }
}
