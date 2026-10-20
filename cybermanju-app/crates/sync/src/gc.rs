// CyberManju OS — chunk GC, eviction & rebalance (AGENT-7 items 6 + 7)
//
// Three ways space gets reclaimed without ever losing a byte that is still
// referenced:
//
//   * **GC** — sweep content-addressed objects no manifest points at. The
//     referenced set is computed from the *live manifests* (the strongest
//     possible authority) and cross-checked against the `chunk_refs` table,
//     which AGENT-6's allocator can feed with its own refcounts. A chunk is
//     never deleted while anything references it — the test writes data, runs
//     GC and asserts a byte-identical read-back.
//   * **Eviction** — under volume pressure, drop *parity copies* (never a
//     chunk's last copy, never an erasure shard — losing one shard would
//     change the chunk's `k`/`m`), preferring the least healthy providers.
//   * **Rebalance** — when an idle provider appears (a newly attached disk
//     shows up as an eligible config), move a copy onto it: upload first,
//     then delete the old object only once no manifest points at it there.

use std::collections::{BTreeMap, BTreeSet};

use cybermanju_db::Database;
use cybermanju_types::sync::{RemoteFile, SyncConfig};
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

use crate::backends::create_backend;
use crate::health::{self, HealthStatus};
use crate::manifest;
use crate::repair::{self, Snapshot};

/// Objects younger than this are never swept: a sync run can upload a chunk
/// before the manifest row that references it lands.
pub const GC_GRACE_SECS: i64 = 3600;
/// Free-space ratio below which eviction starts.
pub const LOW_WATERMARK: f64 = 0.10;
/// Upper bound on one eviction pass (space pressure is a hint, not a raid).
pub const EVICT_BATCH: usize = 32;

// ─── Reference counts (`chunk_refs`) ────────────────────────────────────────

/// Reference counts derived from the manifests: `remote_path → refs`.
pub fn count_refs(snapshot: &Snapshot) -> BTreeMap<String, u32> {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for (_config_id, path) in snapshot.referenced_pairs() {
        *counts.entry(path).or_insert(0) += 1;
    }
    counts
}

/// Persist reference counts to `chunk_refs` (key `config/path`, so a path
/// that exists on two providers has two rows).
pub fn persist_ref_counts(db: &Database, snapshot: &Snapshot) -> Result<usize, String> {
    let pairs: Vec<(String, String)> = snapshot.referenced_pairs().into_iter().collect();
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_chunk_refs_table())
            .map_err(|e| e.to_string())?;
        for (config_id, path) in &pairs {
            table
                .insert(format!("{}/{}", config_id, path).as_str(), "1")
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(pairs.len())
}

/// Reference counts currently recorded in `chunk_refs`.
pub fn read_ref_counts(db: &Database) -> Result<BTreeMap<String, u32>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_chunk_refs_table())
        .map_err(|e| e.to_string())?;
    let mut counts = BTreeMap::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (key, _value) = entry.map_err(|e| e.to_string())?;
        *counts.entry(key.value().to_string()).or_insert(0u32) += 1;
    }
    Ok(counts)
}

// ─── GC ─────────────────────────────────────────────────────────────────────

/// What one GC pass did.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GcReport {
    pub checked: u32,
    pub kept: u32,
    pub deleted: u32,
    pub bytes_freed: u64,
    pub dry_run: bool,
    pub providers: Vec<String>,
    pub warnings: Vec<String>,
}

/// RFC3339 → epoch seconds, `None` when the provider does not say.
fn modified_secs(file: &RemoteFile) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(&file.modified_at)
        .ok()
        .map(|dt| dt.timestamp())
}

/// Is this object too fresh to sweep? Unparseable timestamps count as fresh:
/// GC only ever deletes things it is sure about.
fn is_fresh(file: &RemoteFile, grace_secs: i64) -> bool {
    if grace_secs <= 0 {
        return false;
    }
    match modified_secs(file) {
        Some(ts) => {
            let now = chrono::Utc::now().timestamp();
            now.saturating_sub(ts) < grace_secs
        }
        None => true,
    }
}

/// Sweep unreferenced objects on every enabled provider.
///
/// `counts` is the extra authority (the `chunk_refs` table — AGENT-6's
/// allocator refcounts land here): a path with a recorded reference is kept
/// even if the live manifests do not mention it, because *keeping* is always
/// the safe direction.
pub fn sweep(
    snapshot: &Snapshot,
    configs: &[SyncConfig],
    counts: &BTreeMap<String, u32>,
    dry_run: bool,
    grace_secs: i64,
) -> Result<GcReport, String> {
    let referenced = snapshot.referenced_by_provider();
    let mut report = GcReport {
        dry_run,
        ..GcReport::default()
    };

    for config in configs.iter().filter(|c| c.enabled) {
        let backend = match create_backend(config) {
            Ok(backend) => backend,
            Err(e) => {
                report.warnings.push(format!(
                    "provider '{}' could not be listed ({}); nothing swept there",
                    config.id, e
                ));
                continue;
            }
        };
        let files = match backend.list_files(manifest::CHUNKS_DIR) {
            Ok(files) => files,
            Err(e) => {
                report.warnings.push(format!(
                    "provider '{}' could not be listed ({}); nothing swept there",
                    config.id, e
                ));
                continue;
            }
        };
        report.providers.push(config.id.clone());
        let keep = referenced.get(&config.id);

        for file in files {
            if !file.path.starts_with(manifest::CHUNKS_DIR) {
                continue;
            }
            report.checked += 1;
            let referenced_here = keep.map(|set| set.contains(&file.path)).unwrap_or(false);
            let counted = counts
                .get(&format!("{}/{}", config.id, file.path))
                .copied()
                .unwrap_or(0)
                > 0;
            if referenced_here || counted {
                report.kept += 1;
                continue;
            }
            if is_fresh(&file, grace_secs) {
                report.kept += 1;
                report.warnings.push(format!(
                    "kept '{}': too fresh (or untimestamped) to prove unreferenced",
                    file.path
                ));
                continue;
            }
            if dry_run {
                report.deleted += 1;
                report.bytes_freed += file.size_bytes;
                continue;
            }
            match backend.delete_file(&file.path) {
                Ok(()) => {
                    report.deleted += 1;
                    report.bytes_freed += file.size_bytes;
                }
                Err(e) => {
                    report.warnings.push(format!(
                        "could not delete '{}' on '{}': {}",
                        file.path, config.id, e
                    ));
                    health::observe(&config.id, &health::Outcome::Failure { message: e });
                }
            }
        }
    }
    Ok(report)
}

/// Full GC pass: snapshot, reference counts, sweep, refresh `chunk_refs`.
pub fn gc(db: &Database) -> Result<GcReport, String> {
    gc_with_grace(db, GC_GRACE_SECS)
}

/// [`gc`] with a caller-chosen grace window (tests pass `0`).
pub fn gc_with_grace(db: &Database, grace_secs: i64) -> Result<GcReport, String> {
    let snapshot = repair::snapshot(db)?;
    let counts = read_ref_counts(db)?;
    let report = sweep(&snapshot, &snapshot.configs, &counts, false, grace_secs)?;
    persist_ref_counts(db, &snapshot)?;
    Ok(report)
}

/// What a dry run *would* delete (nothing is touched).
pub fn gc_dry_run(db: &Database) -> Result<GcReport, String> {
    let snapshot = repair::snapshot(db)?;
    let counts = read_ref_counts(db)?;
    sweep(&snapshot, &snapshot.configs, &counts, true, 0)
}

// ─── Eviction (item 6) ──────────────────────────────────────────────────────

/// One copy an eviction pass may drop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EvictionCandidate {
    pub file_id: String,
    pub chunk_index: u32,
    pub config_id: String,
    pub remote_path: String,
    /// Why this copy is the one to lose: `parity copy …`.
    pub reason: String,
    /// Provider score at selection time — lower goes first.
    pub score: f64,
}

/// Candidates for eviction, least valuable first.
///
/// Rules (all hard):
///   * only *parity* copies — the primary copy of a replicated chunk stays;
///   * never a chunk's last locator;
///   * never an erasure shard (dropping one would change `k`/`m`).
pub fn evict_candidates(snapshot: &Snapshot, max: usize) -> Vec<EvictionCandidate> {
    let mut out = Vec::new();
    for record in &snapshot.records {
        for entry in &record.manifest.chunks {
            let parity = record.manifest.parity;
            if manifest::entry_redundancy(entry, parity).is_erasure() {
                continue;
            }
            if entry.replicas.is_empty() {
                continue;
            }
            for loc in &entry.replicas {
                let health = health::get(&loc.config_id);
                let reason = match health.status {
                    HealthStatus::Quarantined => {
                        format!("parity copy on quarantined provider '{}'", loc.config_id)
                    }
                    _ => format!(
                        "parity copy on '{}' (score {:.2})",
                        loc.config_id, health.score
                    ),
                };
                out.push(EvictionCandidate {
                    file_id: record.file.id.clone(),
                    chunk_index: entry.index,
                    config_id: loc.config_id.clone(),
                    remote_path: loc.remote_path.clone(),
                    reason,
                    score: health.score,
                });
            }
        }
    }
    // Worst provider first, and within a tie the later (parity) copy goes —
    // it is the one the brief calls "least valuable".
    out.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.chunk_index.cmp(&a.chunk_index))
            .then_with(|| a.config_id.cmp(&b.config_id))
    });
    out.truncate(max);
    out
}

/// What an eviction/rebalance pass did.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpaceReport {
    pub removed_locators: u32,
    pub objects_deleted: u32,
    pub bytes_freed: u64,
    pub moved: u32,
    pub dry_run: bool,
    pub warnings: Vec<String>,
}

/// Sizes of every object under `chunks/` on `config` (`path → bytes`).
fn sizes_on(config: &SyncConfig) -> BTreeMap<String, u64> {
    create_backend(config)
        .ok()
        .and_then(|backend| backend.list_files(manifest::CHUNKS_DIR).ok())
        .map(|files| files.into_iter().map(|f| (f.path, f.size_bytes)).collect())
        .unwrap_or_default()
}

/// Drop `removals` (file, chunk, config, path) from their manifests, write
/// the updates, and delete the object wherever nothing references it any
/// more.
fn apply_removals(
    snapshot: &Snapshot,
    removals: &[(String, u32, String, String)],
    dry_run: bool,
) -> Result<(SpaceReport, Vec<(cybermanju_types::sync::SyncFile, String)>), String> {
    let mut report = SpaceReport {
        dry_run,
        ..SpaceReport::default()
    };
    let mut records: Vec<repair::Record> = snapshot.records.clone();
    let mut touched_configs: BTreeSet<String> = BTreeSet::new();

    for (file_id, chunk_index, config_id, remote_path) in removals {
        let record = match records.iter_mut().find(|r| &r.file.id == file_id) {
            Some(record) => record,
            None => {
                report
                    .warnings
                    .push(format!("no manifest for file '{}'", file_id));
                continue;
            }
        };
        let entry = match record.manifest.chunks.get_mut(*chunk_index as usize) {
            Some(entry) => entry,
            None => {
                report
                    .warnings
                    .push(format!("no chunk {} in '{}'", chunk_index, file_id));
                continue;
            }
        };
        let before = entry.replicas.len();
        entry
            .replicas
            .retain(|loc| !(loc.config_id == *config_id && loc.remote_path == *remote_path));
        if entry.replicas.len() == before {
            report.warnings.push(format!(
                "locator {}/{} not found in chunk {} of '{}'",
                config_id, remote_path, chunk_index, file_id
            ));
            continue;
        }
        if manifest::entry_locs(entry).is_empty() {
            report.warnings.push(format!(
                "refusing to evict the last copy of chunk {} in '{}'",
                chunk_index, file_id
            ));
            continue;
        }
        report.removed_locators += 1;
        touched_configs.insert(config_id.clone());
    }

    if report.removed_locators == 0 {
        return Ok((report, Vec::new()));
    }

    // Rebuild the referenced set over the *updated* manifests: an object is
    // deletable only where no manifest points at it any more.
    let updated = Snapshot {
        records: records.clone(),
        configs: snapshot.configs.clone(),
    };
    let referenced = updated.referenced_by_provider();

    for (_file_id, _chunk_index, config_id, remote_path) in removals {
        let still = referenced
            .get(config_id)
            .map(|set| set.contains(remote_path))
            .unwrap_or(false);
        if still {
            continue;
        }
        let size = snapshot
            .configs
            .iter()
            .find(|c| &c.id == config_id)
            .map(|c| sizes_on(c).get(remote_path).copied().unwrap_or(0))
            .unwrap_or(0);
        if dry_run {
            report.objects_deleted += 1;
            report.bytes_freed += size;
            continue;
        }
        let config = match snapshot.configs.iter().find(|c| &c.id == config_id) {
            Some(config) => config,
            None => continue,
        };
        let backend = match create_backend(config) {
            Ok(backend) => backend,
            Err(e) => {
                report.warnings.push(e);
                continue;
            }
        };
        match backend.delete_file(remote_path) {
            Ok(()) => {
                report.objects_deleted += 1;
                report.bytes_freed += size;
            }
            Err(e) => report.warnings.push(format!(
                "could not delete '{}' on '{}': {}",
                remote_path, config_id, e
            )),
        }
    }

    let updates = records
        .into_iter()
        .filter(|r| {
            snapshot
                .records
                .iter()
                .find(|old| old.file.id == r.file.id)
                .map(|old| old.manifest != r.manifest)
                .unwrap_or(true)
        })
        .map(|mut record| {
            let json = serde_json::to_string(&record.manifest).unwrap_or_default();
            record.file.manifest_ref = Some(json.clone());
            (record.file, json)
        })
        .collect();
    let _ = touched_configs;
    Ok((report, updates))
}

/// Evict the given candidates (writes manifests, deletes freed objects).
pub fn evict(
    snapshot: &Snapshot,
    candidates: &[EvictionCandidate],
    dry_run: bool,
) -> Result<(SpaceReport, Vec<(cybermanju_types::sync::SyncFile, String)>), String> {
    let removals: Vec<(String, u32, String, String)> = candidates
        .iter()
        .map(|c| {
            (
                c.file_id.clone(),
                c.chunk_index,
                c.config_id.clone(),
                c.remote_path.clone(),
            )
        })
        .collect();
    apply_removals(snapshot, &removals, dry_run)
}

/// Read free/capacity out of an AGENT-6 volume row, tolerating both the
/// camelCase and snake_case spellings (the row shape is theirs, so stay
/// defensive). `None` means "we cannot tell" — and eviction never runs blind.
pub fn volume_pressure(value: &serde_json::Value) -> Option<(u64, u64)> {
    let number = |keys: &[&str]| -> Option<u64> {
        for key in keys {
            match value.get(key) {
                Some(serde_json::Value::Number(n)) => {
                    if let Some(v) = n.as_u64() {
                        return Some(v);
                    }
                }
                Some(serde_json::Value::String(s)) => {
                    if let Ok(v) = s.parse::<u64>() {
                        return Some(v);
                    }
                }
                _ => {}
            }
        }
        None
    };
    let free = number(&[
        "freeBytes",
        "free_bytes",
        "free",
        "availableBytes",
        "available_bytes",
    ])?;
    let capacity = number(&[
        "capacityBytes",
        "capacity_bytes",
        "capacity",
        "totalBytes",
        "total_bytes",
        "sizeBytes",
        "size_bytes",
    ])?;
    if capacity == 0 {
        return None;
    }
    Some((free, capacity))
}

/// Volume rows currently below the low watermark: `(volume_id, free, capacity)`.
pub fn pressured_volumes(db: &Database) -> Result<Vec<(String, u64, u64)>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_volumes_table())
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (key, value) = entry.map_err(|e| e.to_string())?;
        let parsed: serde_json::Value = match serde_json::from_str(value.value()) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        if let Some((free, capacity)) = volume_pressure(&parsed) {
            if (free as f64) / (capacity as f64) < LOW_WATERMARK {
                out.push((key.value().to_string(), free, capacity));
            }
        }
    }
    Ok(out)
}

/// Under low free space, evict the least valuable copies until the
/// watermark is clear (or the batch cap is hit). With nothing parseable in
/// the volume table it evicts nothing — never a blind delete.
pub fn evict_if_low(db: &Database, dry_run: bool) -> Result<SpaceReport, String> {
    let pressured = pressured_volumes(db)?;
    if pressured.is_empty() {
        return Ok(SpaceReport {
            dry_run,
            ..SpaceReport::default()
        });
    }
    let (free, capacity) = match pressured.first() {
        Some((_, free, capacity)) => (*free as f64, *capacity as f64),
        None => unreachable!("pressured_volumes returned nothing"),
    };
    let need = ((capacity * LOW_WATERMARK) - free).max(0.0) as u64;

    let snapshot = repair::snapshot(db)?;
    let candidates = evict_candidates(&snapshot, EVICT_BATCH);
    if candidates.is_empty() {
        return Ok(SpaceReport {
            dry_run,
            warnings: vec!["no evictable parity copies".to_string()],
            ..SpaceReport::default()
        });
    }

    // Free enough to clear the watermark, in least-valuable-first order.
    let sizes: BTreeMap<String, BTreeMap<String, u64>> = snapshot
        .configs
        .iter()
        .map(|c| (c.id.clone(), sizes_on(c)))
        .collect();
    let mut selected = Vec::new();
    let mut freed = 0u64;
    for candidate in candidates {
        if freed >= need && !selected.is_empty() {
            break;
        }
        freed += sizes
            .get(&candidate.config_id)
            .and_then(|map| map.get(&candidate.remote_path))
            .copied()
            .unwrap_or(0);
        selected.push(candidate);
    }

    let (report, updates) = evict(&snapshot, &selected, dry_run)?;
    if !dry_run {
        for (file, json) in updates {
            let mut file = file;
            file.manifest_ref = Some(json);
            db.upsert_sync_file(&file).map_err(|e| e.to_string())?;
        }
        let fresh = repair::snapshot(db)?;
        persist_ref_counts(db, &fresh)?;
    }
    Ok(report)
}

// ─── Rebalance (item 6) ─────────────────────────────────────────────────────

/// Move one copy of each unbalanced chunk onto an idle eligible provider.
///
/// "Idle" = a healthy enabled provider that holds no copy of that chunk —
/// which is exactly what an **attached disk** looks like to the sync layer
/// (a Local config rooted on the new volume). Counts never change, so a
/// replicated chunk can never drift into looking like an erasure chunk.
pub fn rebalance_records(
    snapshot: &Snapshot,
    dry_run: bool,
) -> Result<(SpaceReport, Vec<(cybermanju_types::sync::SyncFile, String)>), String> {
    let eligible = health::eligible(&snapshot.configs);
    if eligible.len() < 2 {
        return Ok((
            SpaceReport {
                dry_run,
                warnings: vec![
                    "fewer than two eligible providers; nothing to rebalance".to_string()
                ],
                ..SpaceReport::default()
            },
            Vec::new(),
        ));
    }

    let mut removals: Vec<(String, u32, String, String)> = Vec::new();
    let mut additions: Vec<(String, u32, String, String, Vec<u8>)> = Vec::new();

    for record in &snapshot.records {
        let parity = record.manifest.parity;
        for entry in &record.manifest.chunks {
            if manifest::entry_redundancy(entry, parity).is_erasure() {
                // Shards are placed as a set; rebalancing moves whole copies.
                continue;
            }
            let locs = manifest::entry_locs(entry);
            if locs.is_empty() {
                continue;
            }
            let occupied: BTreeSet<&str> = locs.iter().map(|l| l.config_id.as_str()).collect();
            let idle = match eligible.iter().find(|c| !occupied.contains(c.id.as_str())) {
                Some(config) => config,
                None => continue,
            };

            // Move the copy on the worst provider (ties → the parity copy).
            let mut order: Vec<usize> = (0..locs.len()).collect();
            order.sort_by(|a, b| {
                let ha = health::get(&locs[*a].config_id);
                let hb = health::get(&locs[*b].config_id);
                let qa = ha.status == HealthStatus::Quarantined;
                let qb = hb.status == HealthStatus::Quarantined;
                qb.cmp(&qa)
                    .then_with(|| {
                        ha.score
                            .partial_cmp(&hb.score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    // highest index (the parity copy) moves first
                    .then_with(|| b.cmp(a))
            });
            let source_index = order[0];
            let source = locs[source_index];
            if source.config_id == idle.id {
                continue;
            }

            let artifact = match snapshot
                .configs
                .iter()
                .find(|c| c.id == source.config_id)
                .map(|c| repair::fetch_verified(c, source))
            {
                Some(Ok(artifact)) => artifact,
                Some(Err(e)) => {
                    // Cannot read it → leave it alone rather than drop it.
                    let _ = e;
                    continue;
                }
                None => continue,
            };

            if dry_run {
                removals.push((
                    record.file.id.clone(),
                    entry.index,
                    source.config_id.clone(),
                    source.remote_path.clone(),
                ));
                additions.push((
                    record.file.id.clone(),
                    entry.index,
                    idle.id.clone(),
                    source.remote_path.clone(),
                    artifact,
                ));
                continue;
            }
            match repair::upload(idle, &source.remote_path, &artifact, "rebalance") {
                Ok(()) => {
                    removals.push((
                        record.file.id.clone(),
                        entry.index,
                        source.config_id.clone(),
                        source.remote_path.clone(),
                    ));
                    additions.push((
                        record.file.id.clone(),
                        entry.index,
                        idle.id.clone(),
                        source.remote_path.clone(),
                        artifact,
                    ));
                }
                Err(_e) => {}
            }
        }
    }

    if removals.is_empty() {
        return Ok((
            SpaceReport {
                dry_run,
                ..SpaceReport::default()
            },
            Vec::new(),
        ));
    }

    // Apply: rewrite each moved locator in place (same index, new provider).
    let mut records = snapshot.records.clone();
    let mut moved = 0u32;
    for (file_id, chunk_index, config_id, remote_path, artifact) in &additions {
        let record = match records.iter_mut().find(|r| &r.file.id == file_id) {
            Some(record) => record,
            None => continue,
        };
        let entry = match record.manifest.chunks.get_mut(*chunk_index as usize) {
            Some(entry) => entry,
            None => continue,
        };
        let mut locs: Vec<crate::manifest::ChunkLoc> =
            manifest::entry_locs(entry).into_iter().cloned().collect();
        let source_config = removals
            .iter()
            .find(|(f, c, _, _)| f == file_id && c == chunk_index)
            .map(|(_, _, config, _)| config.clone());
        let moved_from = match source_config {
            Some(config) => config,
            None => continue,
        };
        if let Some(slot) = locs
            .iter_mut()
            .find(|l| l.config_id == moved_from && l.remote_path == *remote_path)
        {
            slot.config_id = config_id.clone();
            slot.artifact_hash = crate::transfer::blake3_hex(artifact);
            entry.primary = locs[0].clone();
            entry.replicas = locs[1..].to_vec();
            moved += 1;
        }
    }

    // Old objects go only where the updated manifests no longer point.
    let updated = Snapshot {
        records: records.clone(),
        configs: snapshot.configs.clone(),
    };
    let referenced = updated.referenced_by_provider();
    let mut report = SpaceReport {
        dry_run,
        moved,
        ..SpaceReport::default()
    };
    for (file_id, chunk_index, config_id, remote_path) in &removals {
        let _ = (file_id, chunk_index);
        let still = referenced
            .get(config_id)
            .map(|set| set.contains(remote_path))
            .unwrap_or(false);
        if still || dry_run {
            continue;
        }
        if let Some(config) = snapshot.configs.iter().find(|c| &c.id == config_id) {
            if let Ok(backend) = create_backend(config) {
                match backend.delete_file(remote_path) {
                    Ok(()) => report.objects_deleted += 1,
                    Err(e) => report.warnings.push(e),
                }
            }
        }
    }

    let updates = records
        .into_iter()
        .filter(|r| {
            snapshot
                .records
                .iter()
                .find(|old| old.file.id == r.file.id)
                .map(|old| old.manifest != r.manifest)
                .unwrap_or(true)
        })
        .map(|mut record| {
            let json = serde_json::to_string(&record.manifest).unwrap_or_default();
            record.file.manifest_ref = Some(json.clone());
            (record.file, json)
        })
        .collect();
    Ok((report, updates))
}

/// Rebalance and write the results back.
pub fn rebalance(db: &Database, dry_run: bool) -> Result<SpaceReport, String> {
    let snapshot = repair::snapshot(db)?;
    let (report, updates) = rebalance_records(&snapshot, dry_run)?;
    if !dry_run {
        for (file, json) in updates {
            let mut file = file;
            file.manifest_ref = Some(json);
            db.upsert_sync_file(&file).map_err(|e| e.to_string())?;
        }
        if report.moved > 0 {
            let fresh = repair::snapshot(db)?;
            persist_ref_counts(db, &fresh)?;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ChunkManifest;
    use crate::repair::fixtures::Env;

    #[test]
    fn gc_keeps_referenced_chunks_byte_identical_and_sweeps_orphans() {
        let env = Env::new("gc-sweep", 2, 1);
        let orphan_path = format!("{}/deadbeef", manifest::CHUNKS_DIR);
        env.write_orphan(0, &orphan_path, b"orphaned bytes");
        env.write_orphan(1, &orphan_path, b"orphaned bytes");

        let report = gc_with_grace(env.db(), 0).expect("gc");
        assert_eq!(report.deleted, 2, "{:?}", report);
        assert_eq!(report.kept, 2, "{:?}", report);
        assert!(report.bytes_freed > 0);

        // The hard rule: read-back after GC is byte-identical.
        let snap = env.reload();
        env.restore_ok(&snap, "gc");
        assert!(env.object_path(0).exists(), "chunk copy 0 survived");
        assert!(env.object_path(1).exists(), "chunk copy 1 survived");

        // And the orphan really is gone.
        assert!(!env
            .root
            .join(format!("provider-0/{}", orphan_path))
            .exists());
        env.cleanup();
    }

    #[test]
    fn gc_dry_run_touches_nothing() {
        let env = Env::new("gc-dry", 1, 0);
        let orphan_path = format!("{}/cafebabe", manifest::CHUNKS_DIR);
        env.write_orphan(0, &orphan_path, b"still here");

        let report = gc_dry_run(env.db()).expect("dry run");
        assert_eq!(report.deleted, 1, "{:?}", report);
        assert!(report.dry_run);
        assert!(env
            .root
            .join(format!("provider-0/{}", orphan_path))
            .exists());
        env.cleanup();
    }

    #[test]
    fn reference_counts_are_derived_from_the_manifests() {
        let env = Env::new("gc-refs", 2, 1);
        let snapshot = env.snapshot();
        let counts = count_refs(&snapshot);
        let path = manifest::chunk_remote_path(&env.hash);
        // One path, referenced on two providers.
        assert_eq!(counts.get(&path).copied(), Some(2));

        let stored = persist_ref_counts(env.db(), &snapshot).expect("persist");
        assert_eq!(stored, 2);
        let read = read_ref_counts(env.db()).expect("read");
        assert_eq!(
            read.get(&format!("{}/{}", env.configs[0].id, path)),
            Some(&1)
        );
        assert_eq!(
            read.get(&format!("{}/{}", env.configs[1].id, path)),
            Some(&1)
        );
        env.cleanup();
    }

    #[test]
    fn eviction_only_offers_parity_copies_and_keeps_data_readable() {
        let env = Env::new("evict-pick", 2, 1);
        let snapshot = env.snapshot();
        let candidates = evict_candidates(&snapshot, 10);
        assert_eq!(candidates.len(), 1, "{:?}", candidates);
        // The parity copy (index 1) — never the primary.
        assert_eq!(candidates[0].config_id, env.configs[1].id);
        assert!(candidates[0].reason.contains("parity copy"));

        let (report, updates) = evict(&snapshot, &candidates, false).expect("evict");
        assert_eq!(report.removed_locators, 1, "{:?}", report);
        assert_eq!(report.objects_deleted, 1, "{:?}", report);
        assert!(!env.object_path(1).exists(), "freed object removed");
        assert!(env.object_path(0).exists(), "primary copy untouched");

        // Write the new manifest back and prove the data still restores.
        assert_eq!(updates.len(), 1);
        let manifest: ChunkManifest = serde_json::from_str(&updates[0].1).expect("manifest json");
        let snap_after = Snapshot {
            records: vec![repair::Record {
                file: updates[0].0.clone(),
                manifest,
            }],
            configs: env.configs.clone(),
        };
        env.restore_ok(&snap_after, "evict");
        env.cleanup();
    }

    #[test]
    fn eviction_never_offers_the_last_copy_or_an_erasure_shard() {
        // Single copy (parity 0, one provider): nothing to evict.
        let solo = Env::new("evict-solo", 1, 0);
        let snapshot = solo.snapshot();
        assert!(evict_candidates(&snapshot, 10).is_empty());
        solo.cleanup();

        // Erasure shards are not copies: a restriped chunk offers nothing.
        let env = Env::new("evict-rs", 3, 1);
        crate::repair::restripe(env.db(), &env.record.id, 1).expect("restripe");
        let snapshot = env.reload();
        assert!(
            manifest::entry_redundancy(&snapshot.records[0].manifest.chunks[0], 1).is_erasure()
        );
        assert!(
            evict_candidates(&snapshot, 10).is_empty(),
            "shards must never be eviction candidates"
        );
        env.cleanup();
    }

    #[test]
    fn volume_pressure_reads_both_key_spellings() {
        let camel = serde_json::json!({"freeBytes": 5, "capacityBytes": 100});
        assert_eq!(volume_pressure(&camel), Some((5, 100)));

        let snake = serde_json::json!({"free_bytes": 7, "capacity_bytes": 20});
        assert_eq!(volume_pressure(&snake), Some((7, 20)));

        let strings = serde_json::json!({"free": "9", "capacity": "30"});
        assert_eq!(volume_pressure(&strings), Some((9, 30)));

        // Nothing parseable → None: eviction never runs blind.
        assert_eq!(volume_pressure(&serde_json::json!({"used": 1})), None);
        assert_eq!(volume_pressure(&serde_json::json!({})), None);
    }

    #[test]
    fn rebalance_moves_a_copy_onto_an_idle_provider() {
        let env = Env::new("rebalance", 3, 1);
        let snapshot = env.snapshot();
        // Copies start on p0 and p1; p2 is idle.
        assert_eq!(snapshot.records[0].manifest.chunks[0].replicas.len(), 1);

        let (report, updates) = rebalance_records(&snapshot, false).expect("rebalance");
        assert_eq!(report.moved, 1, "{:?}", report);
        assert_eq!(updates.len(), 1);

        let manifest: ChunkManifest = serde_json::from_str(&updates[0].1).expect("manifest json");
        let configs: Vec<String> = manifest.chunks[0]
            .replicas
            .iter()
            .map(|l| l.config_id.clone())
            .chain(std::iter::once(
                manifest.chunks[0].primary.config_id.clone(),
            ))
            .collect();
        assert!(configs.contains(&env.configs[2].id), "{:?}", configs);
        assert!(
            !configs.contains(&env.configs[1].id),
            "moved off p1: {:?}",
            configs
        );
        assert!(
            env.root
                .join(format!(
                    "provider-2/{}",
                    manifest::chunk_remote_path(&env.hash)
                ))
                .exists(),
            "object landed on the idle provider"
        );

        let snap_after = Snapshot {
            records: vec![repair::Record {
                file: {
                    let mut file = updates[0].0.clone();
                    file.manifest_ref = Some(updates[0].1.clone());
                    file
                },
                manifest,
            }],
            configs: env.configs.clone(),
        };
        env.restore_ok(&snap_after, "rebalance");
        env.cleanup();
    }

    #[test]
    fn nothing_is_evicted_when_the_volume_has_room() {
        let env = Env::new("evict-pressure", 2, 1);
        let report = evict_if_low(env.db(), false).expect("no pressure");
        assert_eq!(report.removed_locators, 0);
        assert_eq!(report.objects_deleted, 0);
        env.restore_ok(&env.reload(), "pressure");
        env.cleanup();
    }
}
