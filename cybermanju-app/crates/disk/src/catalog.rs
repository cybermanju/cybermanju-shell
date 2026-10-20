// CyberManju OS — Catalog (crates/disk/src/catalog.rs).
//! Rows and redb access for the three tables AGENT-6 owns: `disks`,
//! `volumes` and `block_map`.
//!
//! Two rules shape this module:
//!
//! * **One write transaction per logical operation.** A block write updates a
//!   `block_map` row *and* the disk's `used_bytes` in the same commit
//!   ([`commit_block_write`]), so a crash can never leave the catalog saying
//!   one thing and `df` saying another.
//! * **The allocator is cached, not re-scanned.** Rebuilding a disk's
//!   allocator means walking every block row for that disk, which is fine
//!   once per attach and ruinous once per 64 KiB write. [`allocator_for`]
//!   keeps one per attached disk and [`forget_allocator`] drops it when the
//!   disk goes away.
//!
//! Keys are strings, values are JSON — same convention as every other table
//! in `crates/db`.

use crate::allocator::Allocator;
use cybermanju_db::Database;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Key of the single logical volume row (there is one merged volume).
pub const VOLUME_KEY: &str = "default";

/// Checkpoint the `.cybermanju` container after this many block writes.
/// Sealing is Argon2id (≈200 ms), so it happens at lifecycle events and then
/// periodically — not on every 64 KiB write.
pub const CHECKPOINT_EVERY: u32 = 16;

// ─── disks ───────────────────────────────────────────────────────────────────

/// One `.cybermanju` disk: a provider binding with a choosable capacity.
///
/// The `id`, `name`, `provider`, `capacityBytes`, `state`, `health` and
/// `createdAt` keys are also read — leniently — by AGENT-8's `df`, so they
/// keep their names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskRow {
    pub id: String,
    pub name: String,
    /// Backend kind for display (`"local"`, `"github"`, …).
    pub provider: String,
    /// `sync_configs.id` this disk stores its blocks on.
    pub config_id: String,
    /// Volume this disk belongs to.
    pub volume_uuid: String,
    /// The size the user chose.
    pub capacity_bytes: u64,
    pub block_size: u32,
    /// Logical bytes placed here (blocks + space charged to sync).
    pub used_bytes: u64,
    /// Spanned placement order — lower fills first.
    pub placement_order: u64,
    /// `"attached"` or `"detached"`.
    pub state: String,
    /// `"ok"` today; AGENT-7 owns degradation.
    pub health: String,
    /// Host path of the sealed `.cybermanju` container.
    pub container_path: String,
    /// Block writes since the container was last sealed.
    pub blocks_written_since_checkpoint: u32,
    /// User-chosen key holder: this disk unwraps the other disks' keys.
    #[serde(default)]
    pub holds_keys: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl DiskRow {
    pub fn attached(&self) -> bool {
        self.state == "attached"
    }

    /// Whole blocks this disk can hold.
    pub fn block_count(&self) -> usize {
        if self.block_size == 0 {
            return 0;
        }
        (self.capacity_bytes / u64::from(self.block_size)) as usize
    }

    /// Bytes of this disk still usable — never negative.
    pub fn free_bytes(&self) -> u64 {
        self.capacity_bytes.saturating_sub(self.used_bytes)
    }
}

pub fn get_disk(db: &Database, id: &str) -> Result<Option<DiskRow>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_disks_table())
        .map_err(|e| e.to_string())?;
    let value = table.get(id).map_err(|e| e.to_string())?;
    match value {
        Some(raw) => serde_json::from_str(raw.value())
            .map(Some)
            .map_err(|e| format!("integrity: disk row '{}' is corrupt: {}", id, e)),
        None => Ok(None),
    }
}

pub fn put_disk(db: &Database, row: &DiskRow) -> Result<(), String> {
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_disks_table())
            .map_err(|e| e.to_string())?;
        let json = serde_json::to_string(row).map_err(|e| e.to_string())?;
        table
            .insert(row.id.as_str(), json.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

pub fn remove_disk(db: &Database, id: &str) -> Result<bool, String> {
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    let removed = {
        let mut table = tx
            .open_table(Database::get_disks_table())
            .map_err(|e| e.to_string())?;
        let found = table.remove(id).map_err(|e| e.to_string())?;
        found.is_some()
    };
    tx.commit().map_err(|e| e.to_string())?;
    Ok(removed)
}

/// Every disk, in spanned placement order (creation order, then id).
pub fn list_disks(db: &Database) -> Result<Vec<DiskRow>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_disks_table())
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (key, value) = entry.map_err(|e| e.to_string())?;
        let row: DiskRow = serde_json::from_str(value.value())
            .map_err(|e| format!("integrity: disk row '{}' is corrupt: {}", key.value(), e))?;
        rows.push(row);
    }
    rows.sort_by(|a, b| {
        a.placement_order
            .cmp(&b.placement_order)
            .then(a.id.cmp(&b.id))
    });
    Ok(rows)
}

/// Disks that count towards `df` and accept writes, in placement order.
pub fn attached_disks(db: &Database) -> Result<Vec<DiskRow>, String> {
    Ok(list_disks(db)?
        .into_iter()
        .filter(DiskRow::attached)
        .collect())
}

/// The disk bound to a sync config — where `charge_disk` lands its bytes.
pub fn disk_for_config(db: &Database, config_id: &str) -> Result<Option<DiskRow>, String> {
    Ok(list_disks(db)?
        .into_iter()
        .find(|row| row.config_id == config_id && row.attached()))
}

/// Designate the single key-holder disk: sets `holds_keys` on `id` and
/// clears it on every other disk. The holder's passphrase unwraps the other
/// disks — enforced at policy level, verified by crypto on attach.
pub fn set_key_holder(db: &Database, id: &str) -> Result<DiskRow, String> {
    let mut rows = list_disks(db)?;
    if !rows.iter().any(|r| r.id == id) {
        return Err(format!("not_found: disk '{}' not found", id));
    }
    let now = now_rfc3339();
    for row in rows.iter_mut() {
        row.holds_keys = row.id == id;
        row.updated_at = now.clone();
        put_disk(db, row)?;
    }
    get_disk(db, id)?.ok_or_else(|| format!("not_found: disk '{}' not found", id))
}

// ─── block_map ───────────────────────────────────────────────────────────────

/// One placed block: a volume LBA answered by a slot on one disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockRow {
    pub lba: u64,
    pub disk_id: String,
    pub slot: u64,
    pub chunk_hash: String,
    pub data_len: u32,
}

fn block_key(lba: u64) -> String {
    format!("b/{:020}", lba)
}

pub fn get_block_row(db: &Database, lba: u64) -> Result<Option<BlockRow>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_block_map_table())
        .map_err(|e| e.to_string())?;
    let value = table
        .get(block_key(lba).as_str())
        .map_err(|e| e.to_string())?;
    match value {
        Some(raw) => serde_json::from_str(raw.value())
            .map(Some)
            .map_err(|e| format!("integrity: block row {} is corrupt: {}", lba, e)),
        None => Ok(None),
    }
}

/// Every placed block, ordered by LBA.
pub fn list_block_rows(db: &Database) -> Result<Vec<BlockRow>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_block_map_table())
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (key, value) = entry.map_err(|e| e.to_string())?;
        let row: BlockRow = serde_json::from_str(value.value())
            .map_err(|e| format!("integrity: block row '{}' is corrupt: {}", key.value(), e))?;
        rows.push(row);
    }
    rows.sort_by_key(|row| row.lba);
    Ok(rows)
}

/// Blocks living on one disk (a full scan of `block_map`, filtered).
pub fn disk_block_rows(db: &Database, disk_id: &str) -> Result<Vec<BlockRow>, String> {
    Ok(list_block_rows(db)?
        .into_iter()
        .filter(|row| row.disk_id == disk_id)
        .collect())
}

/// Everything one block write changes, applied in a single transaction.
///
/// The caller computes the resulting rows (it already knows the placement
/// decision); this only commits them atomically.
#[derive(Debug)]
pub struct BlockWrite {
    /// The block this LBA held before, if any.
    pub old: Option<BlockRow>,
    /// The block this LBA holds now.
    pub new: BlockRow,
    /// Disk row as it will be after the write (both disks when a replacement
    /// moves between them — pass `old_disk` for the vacated one).
    pub target: DiskRow,
    /// The disk the old block lived on, when it is not `target`.
    pub old_disk: Option<DiskRow>,
}

pub fn commit_block_write(db: &Database, write: &BlockWrite) -> Result<(), String> {
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut blocks = tx
            .open_table(Database::get_block_map_table())
            .map_err(|e| e.to_string())?;
        if let Some(old) = &write.old {
            blocks
                .remove(block_key(old.lba).as_str())
                .map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string(&write.new).map_err(|e| e.to_string())?;
        blocks
            .insert(block_key(write.new.lba).as_str(), json.as_str())
            .map_err(|e| e.to_string())?;
    }
    {
        let mut disks = tx
            .open_table(Database::get_disks_table())
            .map_err(|e| e.to_string())?;
        for row in [Some(&write.target), write.old_disk.as_ref()]
            .into_iter()
            .flatten()
        {
            let json = serde_json::to_string(row).map_err(|e| e.to_string())?;
            disks
                .insert(row.id.as_str(), json.as_str())
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Drop every block on a disk (used by `destroy`; the remote objects stay
/// for AGENT-7's GC to sweep).
pub fn remove_disk_blocks(db: &Database, disk_id: &str) -> Result<usize, String> {
    let rows = disk_block_rows(db, disk_id)?;
    if rows.is_empty() {
        return Ok(0);
    }
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_block_map_table())
            .map_err(|e| e.to_string())?;
        for row in &rows {
            table
                .remove(block_key(row.lba).as_str())
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(rows.len())
}

/// Bulk-insert blocks recovered from a container (restore after catalog
/// loss), in one transaction.
pub fn import_disk_blocks(db: &Database, rows: &[BlockRow]) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_block_map_table())
            .map_err(|e| e.to_string())?;
        for row in rows {
            let json = serde_json::to_string(row).map_err(|e| e.to_string())?;
            table
                .insert(block_key(row.lba).as_str(), json.as_str())
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

// ─── volumes ─────────────────────────────────────────────────────────────────

/// The merged logical volume: how N attached disks compose into one address
/// space.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeRow {
    pub id: String,
    pub uuid: String,
    pub name: String,
    /// `"spanned"` (default) or `"round_robin"`.
    pub policy: String,
    /// Next disk index for round-robin placement.
    pub rr_cursor: u64,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for VolumeRow {
    fn default() -> Self {
        let now = now_rfc3339();
        Self {
            id: VOLUME_KEY.to_string(),
            uuid: uuid::Uuid::new_v4().to_string(),
            name: "CyberManju Volume".to_string(),
            policy: "spanned".to_string(),
            rr_cursor: 0,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

/// RFC 3339 timestamp in UTC, without pulling a date crate in here.
pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year,
        month,
        day,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 → (year, month, day), Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// The volume row, created on first use.
pub fn get_volume(db: &Database) -> Result<VolumeRow, String> {
    let found = {
        let tx = db.begin_read().map_err(|e| e.to_string())?;
        let table = tx
            .open_table(Database::get_volumes_table())
            .map_err(|e| e.to_string())?;
        let value = table.get(VOLUME_KEY).map_err(|e| e.to_string())?;
        match value {
            Some(raw) => Some(
                serde_json::from_str::<VolumeRow>(raw.value())
                    .map_err(|e| format!("integrity: volume row is corrupt: {}", e))?,
            ),
            None => None,
        }
    };
    match found {
        Some(row) => Ok(row),
        None => {
            let row = VolumeRow::default();
            put_volume(db, &row)?;
            Ok(row)
        }
    }
}

pub fn put_volume(db: &Database, row: &VolumeRow) -> Result<(), String> {
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_volumes_table())
            .map_err(|e| e.to_string())?;
        let json = serde_json::to_string(row).map_err(|e| e.to_string())?;
        table
            .insert(VOLUME_KEY, json.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

// ─── allocator cache ─────────────────────────────────────────────────────────

type AllocatorCache = Mutex<HashMap<String, Arc<Mutex<Allocator>>>>;

fn cache() -> &'static AllocatorCache {
    static CACHE: std::sync::OnceLock<AllocatorCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The allocator for an attached disk: cached, rebuilt from `block_map` the
/// first time it is asked for after a start-up or an attach.
pub fn allocator_for(db: &Database, row: &DiskRow) -> Result<Arc<Mutex<Allocator>>, String> {
    if let Ok(guard) = cache().lock() {
        if let Some(found) = guard.get(&row.id) {
            return Ok(Arc::clone(found));
        }
    }
    let rows = disk_block_rows(db, &row.id)?;
    let allocator = Allocator::from_rows(
        row.block_size,
        row.block_count(),
        rows.iter()
            .map(|r| (r.slot, r.lba, r.chunk_hash.clone(), r.data_len)),
    )?;
    let shared = Arc::new(Mutex::new(allocator));
    if let Ok(mut guard) = cache().lock() {
        guard.insert(row.id.clone(), Arc::clone(&shared));
    }
    Ok(shared)
}

/// Forget a disk's allocator (detach / destroy).
pub fn forget_allocator(disk_id: &str) {
    if let Ok(mut guard) = cache().lock() {
        guard.remove(disk_id);
    }
}

/// The cached allocator, if this disk has one loaded. `None` means "rebuild
/// from the rows next time" — which is always a safe thing to do, since the
/// rows are the source of truth.
pub fn cached_allocator(disk_id: &str) -> Option<Arc<Mutex<Allocator>>> {
    cache().lock().ok()?.get(disk_id).map(Arc::clone)
}

/// Install a freshly decoded allocator (used when `attach` imports the
/// container's block map after a catalog rebuild).
pub fn seed_allocator(disk_id: &str, allocator: Allocator) {
    if let Ok(mut guard) = cache().lock() {
        guard.insert(disk_id.to_string(), Arc::new(Mutex::new(allocator)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Database::new(dir.path().join("cat.redb").to_str().unwrap()).expect("db");
        (dir, db)
    }

    fn sample_disk(id: &str) -> DiskRow {
        DiskRow {
            id: id.to_string(),
            name: id.to_string(),
            provider: "local".to_string(),
            config_id: "cfg-1".to_string(),
            volume_uuid: "vol-1".to_string(),
            capacity_bytes: 1024 * 1024,
            block_size: 64 * 1024,
            used_bytes: 0,
            placement_order: 0,
            state: "attached".to_string(),
            health: "ok".to_string(),
            container_path: "/tmp/x.cybermanju".to_string(),
            blocks_written_since_checkpoint: 0,
            holds_keys: false,
            created_at: now_rfc3339(),
            updated_at: now_rfc3339(),
        }
    }

    #[test]
    fn disk_rows_round_trip_and_sort_in_placement_order() {
        let (_dir, db) = scratch();
        let mut first = sample_disk("a");
        first.placement_order = 1;
        let mut second = sample_disk("b");
        second.placement_order = 0;
        second.state = "detached".into();
        put_disk(&db, &first).expect("put a");
        put_disk(&db, &second).expect("put b");

        let all = list_disks(&db).expect("list");
        assert_eq!(all[0].id, "b", "placement order wins over id order");
        let attached = attached_disks(&db).expect("attached");
        assert_eq!(attached.len(), 1, "detached disks are filtered out");
        assert_eq!(attached[0].id, "a");
        assert_eq!(
            get_disk(&db, "a").expect("get").expect("row").free_bytes(),
            1024 * 1024
        );
        assert!(get_disk(&db, "zz").expect("get").is_none());
    }

    #[test]
    fn a_block_write_moves_the_row_and_the_used_bytes_in_one_commit() {
        let (_dir, db) = scratch();
        let mut disk = sample_disk("a");
        put_disk(&db, &disk).expect("put disk");
        let new_row = BlockRow {
            lba: 7,
            disk_id: "a".into(),
            slot: 0,
            chunk_hash: "ab".repeat(32),
            data_len: 1000,
        };
        disk.used_bytes = 1000;
        commit_block_write(
            &db,
            &BlockWrite {
                old: None,
                new: new_row.clone(),
                target: disk.clone(),
                old_disk: None,
            },
        )
        .expect("commit");

        let stored = get_block_row(&db, 7).expect("read").expect("row");
        assert_eq!(stored, new_row);
        assert_eq!(
            get_disk(&db, "a").expect("row").expect("row").used_bytes,
            1000
        );

        // Replacing it releases the old bytes in the same commit.
        let replacement = BlockRow {
            lba: 7,
            disk_id: "a".into(),
            slot: 1,
            chunk_hash: "cd".repeat(32),
            data_len: 400,
        };
        let mut after = disk.clone();
        after.used_bytes = 400;
        commit_block_write(
            &db,
            &BlockWrite {
                old: Some(stored),
                new: replacement,
                target: after,
                old_disk: None,
            },
        )
        .expect("replace");
        assert_eq!(
            get_disk(&db, "a").expect("row").expect("row").used_bytes,
            400
        );
        assert_eq!(
            list_block_rows(&db).expect("rows").len(),
            1,
            "one row per LBA"
        );
        assert_eq!(remove_disk_blocks(&db, "a").expect("drop"), 1);
        assert!(list_block_rows(&db).expect("rows").is_empty());
    }

    #[test]
    fn the_default_volume_appears_once_and_keeps_its_uuid() {
        let (_dir, db) = scratch();
        let first = get_volume(&db).expect("volume");
        assert_eq!(first.policy, "spanned", "spanned is the default policy");
        let second = get_volume(&db).expect("volume again");
        assert_eq!(first.uuid, second.uuid, "uuid is stable across reads");
        assert!(!first.uuid.is_empty());
    }

    #[test]
    fn timestamps_are_rfc3339_utc() {
        let now = now_rfc3339();
        assert_eq!(now.len(), 20, "{now}");
        assert!(now.ends_with('Z') && now.contains('T'), "{now}");
        assert_eq!(&now[4..5], "-", "{now}");
        assert_eq!(&now[7..8], "-", "{now}");
        assert_eq!(&now[10..11], "T", "{now}");
    }
}
