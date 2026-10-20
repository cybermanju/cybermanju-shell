// CyberManju OS — multi-writer safety (AGENT-7 item 8 / MISSING F4)
//
// Two devices pointed at one pool must not silently clobber each other.
// Two mechanisms, both required by the brief:
//
//   * **volume lease** — a single active writer per scope, with a TTL,
//     renewal, and *steal-after-expiry*: an expired lease is not a lock, it
//     is a courtesy. Acquiring over a live foreign lease fails with a
//     `conflict:` error instead of quietly taking over.
//   * **version vectors** — every catalog entry (sync file) carries a
//     per-device counter. A write whose vector is not a descendant of the
//     stored one is a *conflict*, and [`decide_write`] turns that into the
//     existing `ConflictPolicy::{Skip,Overwrite,KeepBoth}` decision — the
//     policy decides what happens, never a lost update.
//
// Storage: the `leases` table only (the brief forbids touching
// `database.rs`). Keys are namespaced — `lease/{scope}` for the lock,
// `vv/{file_id}` for version vectors — so both share one table safely.

use std::collections::BTreeMap;

use cybermanju_db::Database;
use cybermanju_types::sync::ConflictPolicy;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

/// Default lease lifetime. Long enough to survive a slow operation, short
/// enough that a crashed writer stops blocking within one refresh window.
pub const DEFAULT_LEASE_TTL_SECS: u64 = 30;

/// The scope every caller uses unless it asks for its own.
pub const DEFAULT_SCOPE: &str = "volume";

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn rfc3339(secs: u64) -> String {
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

/// Identity of the writer asking for the lease: an explicit override first,
/// then the host, so two machines never share a holder id by accident.
pub fn default_holder() -> String {
    if let Ok(id) = std::env::var("CYBERMANJU_DEVICE") {
        if !id.trim().is_empty() {
            return id;
        }
    }
    let host = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| format!("device-{}", std::process::id()));
    host
}

fn lease_key(scope: &str) -> String {
    format!("lease/{}", scope)
}

fn vv_key(file_id: &str) -> String {
    format!("vv/{}", file_id)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Lease {
    pub lease_id: String,
    pub holder: String,
    pub scope: String,
    pub acquired_at: String,
    pub renewed_at: String,
    /// RFC3339 form of `expires_at_secs`, for humans and logs.
    pub expires_at: String,
    pub expires_at_secs: u64,
    /// Previous holder when this lease was taken over after expiry.
    pub stolen_from: Option<String>,
}

impl Lease {
    /// A lease is live while `now < expires_at`. Expiry is inclusive of the
    /// steal: the moment the TTL passes, the lease is free for anyone.
    pub fn is_active(&self, now: u64) -> bool {
        now < self.expires_at_secs
    }
}

fn new_lease(holder: &str, scope: &str, ttl_secs: u64, now: u64) -> Lease {
    Lease {
        lease_id: format!(
            "{}-{}-{}",
            scope,
            now,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ),
        holder: holder.to_string(),
        scope: scope.to_string(),
        acquired_at: rfc3339(now),
        renewed_at: rfc3339(now),
        expires_at: rfc3339(now.saturating_add(ttl_secs)),
        expires_at_secs: now.saturating_add(ttl_secs),
        stolen_from: None,
    }
}

fn read_lease(db: &Database, scope: &str) -> Result<Option<Lease>, String> {
    let key = lease_key(scope);
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_leases_table())
        .map_err(|e| e.to_string())?;
    match table.get(key.as_str()).map_err(|e| e.to_string())? {
        Some(value) => {
            let lease: Lease = serde_json::from_str(value.value())
                .map_err(|e| format!("integrity: lease row unreadable: {}", e))?;
            Ok(Some(lease))
        }
        None => Ok(None),
    }
}

/// The current lease for `scope`, if any row exists.
pub fn inspect_lease(db: &Database, scope: &str) -> Result<Option<Lease>, String> {
    read_lease(db, scope)
}

/// Take (or renew, or steal) the lease for the default volume scope.
pub fn acquire_lease(db: &Database, holder: &str) -> Result<Lease, String> {
    acquire_lease_in(db, holder, DEFAULT_SCOPE, DEFAULT_LEASE_TTL_SECS)
}

/// Acquire the lease for `scope`:
///
/// * free → taken;
/// * held by `holder` → renewed;
/// * expired → **stolen** (`stolen_from` records who held it);
/// * held by someone else → `Err("conflict: …")`, never a silent takeover.
///
/// P0-19: the read-decide-write runs inside ONE write txn (compare-and-swap),
/// so two threads racing to acquire cannot both believe they won — the old
/// read-then-write across separate txns let the loser silently overwrite the
/// winner. (Cross-*process* mutual exclusion is still out of scope for a
/// single-file redb: one opener per file. The `devices` registry + op-log
/// carry the cross-device story instead.)
pub fn acquire_lease_in(
    db: &Database,
    holder: &str,
    scope: &str,
    ttl_secs: u64,
) -> Result<Lease, String> {
    let now = now_secs();
    let key = lease_key(scope);
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    let result = (|| {
        let mut table = tx
            .open_table(Database::get_leases_table())
            .map_err(|e| e.to_string())?;
        let existing: Option<Lease> = match table.get(key.as_str()).map_err(|e| e.to_string())? {
            Some(value) => Some(
                serde_json::from_str(value.value())
                    .map_err(|e| format!("integrity: lease row unreadable: {e}"))?,
            ),
            None => None,
        };
        if let Some(current) = &existing {
            if current.is_active(now) {
                if current.holder == holder {
                    let mut renewed = current.clone();
                    renewed.renewed_at = rfc3339(now);
                    renewed.expires_at_secs = now.saturating_add(ttl_secs);
                    renewed.expires_at = rfc3339(renewed.expires_at_secs);
                    let serialized = serde_json::to_string(&renewed).map_err(|e| e.to_string())?;
                    table
                        .insert(key.as_str(), serialized.as_str())
                        .map_err(|e| e.to_string())?;
                    return Ok(renewed);
                }
                return Err(format!(
                    "conflict: lease '{}' is held by '{}' until {} (holder: {})",
                    scope, current.holder, current.expires_at, holder
                ));
            }
        }
        let mut lease = new_lease(holder, scope, ttl_secs, now);
        lease.stolen_from = existing
            .filter(|old| old.holder != holder)
            .map(|old| old.holder);
        let serialized = serde_json::to_string(&lease).map_err(|e| e.to_string())?;
        table
            .insert(key.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
        Ok(lease)
    })();
    match result {
        Ok(lease) => {
            tx.commit().map_err(|e| e.to_string())?;
            Ok(lease)
        }
        Err(e) => Err(e),
    }
}

/// Extend an existing lease. Only the current holder may renew; an expired
/// lease is not yours to renew — acquire (steal) it instead.
///
/// Same single-txn CAS discipline as acquire: the holder check and the
/// renewal write commit atomically.
pub fn renew_lease(db: &Database, holder: &str, scope: &str) -> Result<Lease, String> {
    let now = now_secs();
    let key = lease_key(scope);
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    let result = (|| {
        let mut table = tx
            .open_table(Database::get_leases_table())
            .map_err(|e| e.to_string())?;
        let existing: Lease = match table.get(key.as_str()).map_err(|e| e.to_string())? {
            Some(value) => serde_json::from_str(value.value())
                .map_err(|e| format!("integrity: lease row unreadable: {e}"))?,
            None => return Err(format!("conflict: no lease '{scope}' to renew")),
        };
        if !existing.is_active(now) {
            return Err(format!(
                "conflict: lease '{scope}' expired at {} — acquire it instead",
                existing.expires_at
            ));
        }
        if existing.holder != holder {
            return Err(format!(
                "conflict: lease '{scope}' is held by '{}', not '{holder}'",
                existing.holder
            ));
        }
        let mut renewed = existing;
        renewed.renewed_at = rfc3339(now);
        renewed.expires_at_secs = now.saturating_add(DEFAULT_LEASE_TTL_SECS);
        renewed.expires_at = rfc3339(renewed.expires_at_secs);
        let serialized = serde_json::to_string(&renewed).map_err(|e| e.to_string())?;
        table
            .insert(key.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
        Ok(renewed)
    })();
    match result {
        Ok(lease) => {
            tx.commit().map_err(|e| e.to_string())?;
            Ok(lease)
        }
        Err(e) => Err(e),
    }
}

/// Release the lease — only its holder. Returns `false` when there was
/// nothing to release. Holder check + removal commit atomically.
pub fn release_lease(db: &Database, holder: &str, scope: &str) -> Result<bool, String> {
    let key = lease_key(scope);
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    let result = (|| {
        let mut table = tx
            .open_table(Database::get_leases_table())
            .map_err(|e| e.to_string())?;
        let existing: Lease = match table.get(key.as_str()).map_err(|e| e.to_string())? {
            Some(value) => serde_json::from_str(value.value())
                .map_err(|e| format!("integrity: lease row unreadable: {e}"))?,
            None => return Ok(false),
        };
        if existing.holder != holder {
            return Err(format!(
                "conflict: lease '{scope}' is held by '{}', not '{holder}'",
                existing.holder
            ));
        }
        table.remove(key.as_str()).map_err(|e| e.to_string())?;
        Ok(true)
    })();
    match result {
        Ok(released) => {
            tx.commit().map_err(|e| e.to_string())?;
            Ok(released)
        }
        Err(e) => Err(e),
    }
}

// ─── Version vectors ────────────────────────────────────────────────────────

/// Per-device write counters for one catalog entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VersionVector {
    pub entries: BTreeMap<String, u64>,
}

/// How two vectors relate from the *local* point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// Same history — the write is a no-op or a retry.
    Equal,
    /// `incoming` extends `local`: a legitimate newer write.
    IncomingAdvances,
    /// `local` extends `incoming`: the writer is stale.
    LocalAhead,
    /// Both advanced independently — the real conflict.
    Diverged,
}

impl VersionVector {
    pub fn bump(&mut self, node: &str) {
        let counter = self.entries.entry(node.to_string()).or_insert(0);
        *counter = counter.saturating_add(1);
    }

    pub fn merge(&mut self, other: &VersionVector) {
        for (node, count) in &other.entries {
            let slot = self.entries.entry(node.clone()).or_insert(0);
            *slot = (*slot).max(*count);
        }
    }

    /// `self >= other` componentwise, with at least one strict edge.
    pub fn dominates(&self, other: &VersionVector) -> bool {
        let mut strict = false;
        for (node, count) in &other.entries {
            match self.entries.get(node) {
                // Behind on any node → we do not dominate.
                Some(ours) if *ours < *count => return false,
                Some(ours) if *ours > *count => strict = true,
                Some(_) => {}
                None => return false,
            }
        }
        for (node, count) in &self.entries {
            if !other.entries.contains_key(node) && *count > 0 {
                strict = true;
            }
        }
        strict
    }
}

/// Classify `incoming` against the stored `local`.
pub fn relate(local: &VersionVector, incoming: &VersionVector) -> Relation {
    if local == incoming {
        return Relation::Equal;
    }
    let local_ge = local.dominates(incoming);
    let incoming_ge = incoming.dominates(local);
    match (local_ge, incoming_ge) {
        (true, false) => Relation::LocalAhead,
        (false, true) => Relation::IncomingAdvances,
        // Both dominate each other only when equal (handled above); a
        // componentless local and an empty incoming is `Equal` too.
        (true, true) => Relation::Equal,
        (false, false) => Relation::Diverged,
    }
}

/// What the caller should do with an incoming write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteDecision {
    /// Write it and record the version.
    Apply,
    /// Write the incoming bytes AND store the merged vector (P0-20): an
    /// Overwrite that just `Apply`ies erases the loser's history — the
    /// caller records `merged` so both devices' counters survive.
    ApplyMerged { merged: VersionVector },
    /// Write it, keeping both histories (the caller stores a second entry).
    KeepBoth { suggested_id: String },
}

/// Map a version-vector relation onto the existing `ConflictPolicy`.
///
/// * no conflict (equal / incoming advances) → [`WriteDecision::Apply`];
/// * conflict + `Skip` → `Err("conflict: …")`, local state untouched;
/// * conflict + `Overwrite` → [`WriteDecision::ApplyMerged`] with the
///   component-wise max of both vectors, so the losing device's history is
///   preserved in the stored vector instead of erased;
/// * conflict + `KeepBoth` → [`WriteDecision::KeepBoth`] with a deterministic
///   id (stable across retries, unique per fork pair — no PIDs).
pub fn decide_write(
    file_id: &str,
    local: &VersionVector,
    incoming: &VersionVector,
    policy: ConflictPolicy,
) -> Result<WriteDecision, String> {
    match relate(local, incoming) {
        Relation::Equal | Relation::IncomingAdvances => Ok(WriteDecision::Apply),
        Relation::LocalAhead | Relation::Diverged => match policy {
            ConflictPolicy::Overwrite => {
                let mut merged = local.clone();
                merged.merge(incoming);
                Ok(WriteDecision::ApplyMerged { merged })
            }
            ConflictPolicy::KeepBoth => Ok(WriteDecision::KeepBoth {
                suggested_id: keep_both_id(file_id, local, incoming),
            }),
            ConflictPolicy::Skip => Err(format!(
                "conflict: write to '{}' from another device diverged (local {:?}, incoming {:?}); \
                 policy 'skip' keeps the local copy",
                file_id, local.entries, incoming.entries
            )),
        },
    }
}

/// Deterministic KeepBoth id (P0-20): `process::id()` collides across
/// devices and changes every retry (duplicating the second entry). The id
/// below hashes the fork pair itself — stable across retries, distinct per
/// divergence, no PIDs anywhere.
fn keep_both_id(file_id: &str, local: &VersionVector, incoming: &VersionVector) -> String {
    let digest = crate::blake3_hex(
        format!(
            "{file_id}|{}|{}",
            vv_fingerprint(local),
            vv_fingerprint(incoming)
        )
        .as_bytes(),
    );
    format!("{file_id}-keep-{}", &digest[..16])
}

/// Canonical `node=counter,…` rendering for id derivation.
fn vv_fingerprint(vv: &VersionVector) -> String {
    vv.entries
        .iter()
        .map(|(node, count)| format!("{node}={count}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Read the stored vector for `file_id` (empty when the entry is new).
pub fn read_vv(db: &Database, file_id: &str) -> Result<VersionVector, String> {
    let key = vv_key(file_id);
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_leases_table())
        .map_err(|e| e.to_string())?;
    match table.get(key.as_str()).map_err(|e| e.to_string())? {
        Some(value) => {
            let vv: VersionVector = serde_json::from_str(value.value())
                .map_err(|e| format!("integrity: version vector unreadable: {}", e))?;
            Ok(vv)
        }
        None => Ok(VersionVector::default()),
    }
}

/// Store a version vector for `file_id`.
pub fn write_vv(db: &Database, file_id: &str, vv: &VersionVector) -> Result<(), String> {
    let key = vv_key(file_id);
    let serialized = serde_json::to_string(vv).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_leases_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(key.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Bump `file_id`'s vector for `node` and persist it.
pub fn bump_vv(db: &Database, file_id: &str, node: &str) -> Result<VersionVector, String> {
    let mut vv = read_vv(db, file_id)?;
    vv.bump(node);
    write_vv(db, file_id, &vv)?;
    Ok(vv)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _dir: std::path::PathBuf,
        db: Database,
    }

    fn fixture(tag: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!(
            "cybermanju-lease-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let db_path = dir.join("lease.redb");
        let db = cybermanju_db::Database::new(db_path.to_str().unwrap()).expect("db");
        Fixture { _dir: dir, db }
    }

    #[test]
    fn a_live_foreign_lease_is_a_conflict_not_a_takeover() {
        let f = fixture("conflict");
        let first = acquire_lease_in(&f.db, "alice", "vol-a", 60).expect("alice acquires");
        let err = acquire_lease_in(&f.db, "bob", "vol-a", 60).expect_err("bob conflicts");
        assert!(err.starts_with("conflict:"), "{}", err);
        assert!(err.contains("alice"), "{}", err);

        // Alice can renew; bob never can.
        let renewed = renew_lease(&f.db, "alice", "vol-a").expect("renew");
        assert_eq!(renewed.holder, "alice");
        assert!(renew_lease(&f.db, "bob", "vol-a").is_err());

        // Bob cannot release it either.
        assert!(release_lease(&f.db, "bob", "vol-a").is_err());
        assert_eq!(first.holder, "alice");
    }

    #[test]
    fn an_expired_lease_can_be_stolen() {
        let f = fixture("steal");
        // TTL 0: the lease is expired the instant it exists.
        let dead = acquire_lease_in(&f.db, "alice", "vol-b", 0).expect("acquire");
        assert!(!dead.is_active(now_secs()));

        let stolen = acquire_lease_in(&f.db, "bob", "vol-b", 60).expect("steal");
        assert_eq!(stolen.holder, "bob");
        assert_eq!(stolen.stolen_from.as_deref(), Some("alice"));

        // The previous holder is now the outsider.
        let err = acquire_lease_in(&f.db, "alice", "vol-b", 60).expect_err("conflict");
        assert!(err.starts_with("conflict:"), "{}", err);

        assert!(release_lease(&f.db, "bob", "vol-b").expect("release"));
        assert!(!release_lease(&f.db, "bob", "vol-b").expect("nothing left"));
    }

    #[test]
    fn renewing_an_expired_lease_tells_you_to_acquire_instead() {
        let f = fixture("expired-renew");
        acquire_lease_in(&f.db, "alice", "vol-c", 0).expect("acquire");
        let err = renew_lease(&f.db, "alice", "vol-c").expect_err("expired");
        assert!(err.starts_with("conflict:"), "{}", err);
        assert!(err.contains("acquire"), "{}", err);
    }

    #[test]
    fn concurrent_acquirers_elect_exactly_one_winner() {
        // P0-19 regression: the read-decide-write is one write txn, so N
        // racers cannot all believe they won (the old split-txn version let
        // the loser silently overwrite the winner).
        let f = fixture("race");
        let db = std::sync::Arc::new(f.db);
        let mut handles = Vec::new();
        for i in 0..8u32 {
            let db = std::sync::Arc::clone(&db);
            handles.push(std::thread::spawn(move || {
                acquire_lease_in(&db, &format!("racer-{i}"), "vol-race", 60)
                    .map(|l| l.holder)
                    .unwrap_or_else(|e| format!("lost:{e}"))
            }));
        }
        let outcomes: Vec<String> = handles
            .into_iter()
            .map(|h| h.join().expect("thread"))
            .collect();
        let winners: Vec<&String> = outcomes
            .iter()
            .filter(|o| !o.starts_with("lost:"))
            .collect();
        assert_eq!(winners.len(), 1, "{outcomes:?}");
        assert!(
            outcomes
                .iter()
                .filter(|o| o.starts_with("lost:"))
                .all(|o| o.contains("conflict:")),
            "{outcomes:?}"
        );
        let held = inspect_lease(&db, "vol-race")
            .expect("inspect")
            .expect("held");
        assert_eq!(&held.holder, winners[0]);
    }

    #[test]
    fn version_vectors_separate_stale_from_diverged() {
        let mut stored = VersionVector::default();
        stored.bump("laptop");
        stored.bump("laptop");

        // A device that has seen the stored version and moved on advances.
        let mut fresh = stored.clone();
        fresh.bump("phone");
        assert_eq!(relate(&stored, &fresh), Relation::IncomingAdvances);
        assert_eq!(
            decide_write("f", &stored, &fresh, ConflictPolicy::Skip).expect("no conflict"),
            WriteDecision::Apply
        );

        // A device that never saw our second write is stale…
        let mut stale = VersionVector::default();
        stale.bump("laptop");
        // …and one that wrote its own second change is diverged.
        let mut other = stored.clone();
        other.entries.clear();
        other.bump("laptop");
        other.bump("tablet");
        assert_eq!(relate(&stored, &stale), Relation::LocalAhead);
        assert_eq!(relate(&stored, &other), Relation::Diverged);
    }

    #[test]
    fn a_diverged_write_surfaces_through_the_conflict_policy() {
        let mut local = VersionVector::default();
        local.bump("laptop");
        let mut incoming = local.clone();
        incoming.entries.clear();
        incoming.bump("tablet");

        let skip = decide_write("file-1", &local, &incoming, ConflictPolicy::Skip)
            .expect_err("skip surfaces a conflict");
        assert!(skip.starts_with("conflict:"), "{}", skip);

        assert_eq!(
            decide_write("file-1", &local, &incoming, ConflictPolicy::Overwrite)
                .expect("overwrite applies"),
            WriteDecision::ApplyMerged {
                merged: {
                    let mut v = local.clone();
                    v.merge(&incoming);
                    v
                }
            }
        );

        match decide_write("file-1", &local, &incoming, ConflictPolicy::KeepBoth)
            .expect("keep both")
        {
            WriteDecision::KeepBoth { suggested_id } => {
                assert!(suggested_id.starts_with("file-1-keep-"), "{}", suggested_id);
                // Deterministic: the same fork pair re-derives the same id.
                let again = decide_write("file-1", &local, &incoming, ConflictPolicy::KeepBoth)
                    .expect("keep both again");
                assert_eq!(
                    WriteDecision::KeepBoth {
                        suggested_id: suggested_id.clone()
                    },
                    again
                );
            }
            other => panic!("expected KeepBoth, got {:?}", other),
        }
    }

    #[test]
    fn vectors_round_trip_and_merge_takes_the_max() {
        let f = fixture("vv");
        assert_eq!(
            read_vv(&f.db, "some-file").expect("empty"),
            VersionVector::default()
        );

        let vv = bump_vv(&f.db, "some-file", "laptop").expect("bump");
        assert_eq!(vv.entries.get("laptop"), Some(&1));
        let vv = bump_vv(&f.db, "some-file", "laptop").expect("bump");
        assert_eq!(vv.entries.get("laptop"), Some(&2));
        assert_eq!(
            read_vv(&f.db, "some-file")
                .expect("read")
                .entries
                .get("laptop"),
            Some(&2)
        );

        let mut merged = vv;
        let mut other = VersionVector::default();
        other.bump("tablet");
        other.bump("laptop");
        merged.merge(&other);
        assert_eq!(merged.entries.get("laptop"), Some(&2));
        assert_eq!(merged.entries.get("tablet"), Some(&1));
    }
}
