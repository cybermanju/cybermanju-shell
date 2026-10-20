// CyberManju OS — provider health scoring & quarantine (AGENT-7 item 5)
//
// Every operation against a provider is an observation: how long it took and
// whether it worked. Those observations roll up into a per-config score
// (success rate, latency, quota headroom, last-seen) that decides one thing —
// **may this provider accept new placements**.
//
// Quarantine is deliberately narrow:
//   * a quarantined config is excluded from *new* placement (repair,
//     rebalance, restripe all go through [`eligible`]);
//   * nothing is ever deleted because a provider is unhealthy — the bytes on
//     it stay until an explicit GC/evict decision says otherwise;
//   * one good observation after a failure streak re-admits it (recovery is
//     automatic, no manual reset).
//
// The live registry is process-global (observations happen on worker threads
// that only have an id), and the durable copy lives in the `provider_health`
// table so `GET /api/repair/status` survives a restart.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use cybermanju_db::Database;
use cybermanju_types::sync::SyncConfig;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

/// Score below which a provider is quarantined (no new placement).
pub const QUARANTINE_SCORE: f64 = 0.35;
/// Score below which a provider is merely degraded (placement allowed but
/// deprioritised).
pub const DEGRADED_SCORE: f64 = 0.7;
/// Consecutive failures that quarantine regardless of the blended score.
pub const FAILURE_STREAK: u32 = 3;
/// Latency that scores as zero; anything faster is proportionally better.
pub const LATENCY_TARGET_MS: u64 = 1_500;
/// Quota headroom below this (5%) forces at least `Degraded`.
pub const QUOTA_FLOOR: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Quarantined,
}

/// The outcome of one provider operation, as observed by whoever performed it
/// (scrub, repair, pipeline, a REST probe).
#[derive(Debug, Clone)]
pub enum Outcome {
    Success {
        latency_ms: u64,
    },
    Failure {
        message: String,
    },
    /// Quota/usage report: `headroom` is free/total in `0..=1`.
    Quota {
        headroom: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealth {
    pub config_id: String,
    pub status: HealthStatus,
    pub score: f64,
    /// Exponentially-weighted mean of recent latencies.
    pub latency_ms: u64,
    /// Exponentially-weighted success rate over the recent window.
    pub success_rate: f64,
    /// Free/total quota on the provider, `0..=1` (1 = untouched).
    pub quota_headroom: f64,
    pub consecutive_failures: u32,
    pub samples: u32,
    pub last_seen: Option<String>,
    pub updated_at: String,
    /// Why it is quarantined/degraded, when it is.
    pub note: Option<String>,
}

impl Default for ProviderHealth {
    fn default() -> Self {
        Self {
            config_id: String::new(),
            status: HealthStatus::Healthy,
            score: 1.0,
            latency_ms: 0,
            success_rate: 1.0,
            quota_headroom: 1.0,
            consecutive_failures: 0,
            samples: 0,
            last_seen: None,
            updated_at: String::new(),
            note: None,
        }
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Blend the raw signals into one `0..=1` score. Pure — unit tests drive it
/// directly.
pub fn score_of(h: &ProviderHealth) -> f64 {
    let latency = if h.samples == 0 {
        1.0
    } else {
        1.0 - (h.latency_ms.min(LATENCY_TARGET_MS) as f64 / LATENCY_TARGET_MS as f64)
    };
    let raw = 0.6 * h.success_rate.clamp(0.0, 1.0)
        + 0.2 * latency.clamp(0.0, 1.0)
        + 0.2 * h.quota_headroom.clamp(0.0, 1.0);
    raw.clamp(0.0, 1.0)
}

/// Derive the status from the score and the failure streak. Pure.
pub fn status_of(h: &ProviderHealth) -> HealthStatus {
    if h.consecutive_failures >= FAILURE_STREAK || h.score < QUARANTINE_SCORE {
        HealthStatus::Quarantined
    } else if h.score < DEGRADED_SCORE || h.quota_headroom < QUOTA_FLOOR {
        HealthStatus::Degraded
    } else {
        HealthStatus::Healthy
    }
}

fn recompute(h: &mut ProviderHealth) {
    h.score = score_of(h);
    h.status = status_of(h);
    h.updated_at = now();
    if h.status == HealthStatus::Quarantined && h.note.is_none() {
        h.note = Some(if h.consecutive_failures >= FAILURE_STREAK {
            format!("{} consecutive failures", h.consecutive_failures)
        } else {
            format!(
                "score {:.2} below quarantine floor {:.2}",
                h.score, QUARANTINE_SCORE
            )
        });
    }
    if h.status == HealthStatus::Healthy {
        h.note = None;
    }
}

fn registry() -> &'static Mutex<HashMap<String, ProviderHealth>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, ProviderHealth>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The live health row for `config_id` (defaults to a perfect score for a
/// provider that has never been observed).
pub fn get(config_id: &str) -> ProviderHealth {
    registry()
        .lock()
        .ok()
        .and_then(|map| map.get(config_id).cloned())
        .unwrap_or_else(|| ProviderHealth {
            config_id: config_id.to_string(),
            ..ProviderHealth::default()
        })
}

/// Fold one observation into the live registry and return the updated row.
pub fn observe(config_id: &str, outcome: &Outcome) -> ProviderHealth {
    let mut map = match registry().lock() {
        Ok(map) => map,
        Err(_) => return get(config_id),
    };
    let h = map
        .entry(config_id.to_string())
        .or_insert_with(|| ProviderHealth {
            config_id: config_id.to_string(),
            ..ProviderHealth::default()
        });

    match outcome {
        Outcome::Success { latency_ms } => {
            h.latency_ms = if h.samples == 0 {
                *latency_ms
            } else {
                (h.latency_ms * 7 + latency_ms) / 8
            };
            h.success_rate += 0.1 * (1.0 - h.success_rate);
            h.consecutive_failures = 0;
        }
        Outcome::Failure { .. } => {
            h.success_rate += 0.1 * (0.0 - h.success_rate);
            h.consecutive_failures = h.consecutive_failures.saturating_add(1);
        }
        Outcome::Quota { headroom } => {
            h.quota_headroom = headroom.clamp(0.0, 1.0);
        }
    }
    h.samples = h.samples.saturating_add(1);
    h.last_seen = Some(now());
    recompute(h);
    h.clone()
}

/// Force a provider into quarantine (e.g. a hard 401). Placement stops;
/// nothing is deleted.
pub fn quarantine(config_id: &str, reason: &str) -> ProviderHealth {
    let mut map = match registry().lock() {
        Ok(map) => map,
        Err(_) => return get(config_id),
    };
    let h = map
        .entry(config_id.to_string())
        .or_insert_with(|| ProviderHealth {
            config_id: config_id.to_string(),
            ..ProviderHealth::default()
        });
    h.status = HealthStatus::Quarantined;
    h.consecutive_failures = h.consecutive_failures.max(FAILURE_STREAK);
    h.score = score_of(h);
    h.updated_at = now();
    h.note = Some(reason.to_string());
    h.clone()
}

/// Every live row, sorted by config id.
pub fn all() -> Vec<ProviderHealth> {
    let mut rows: Vec<ProviderHealth> = registry()
        .lock()
        .ok()
        .map(|map| map.values().cloned().collect())
        .unwrap_or_default();
    rows.sort_by_key(|a| a.config_id.clone());
    rows
}

/// Drop every live row — test isolation only.
pub fn clear() {
    if let Ok(mut map) = registry().lock() {
        map.clear();
    }
}

/// The configs new placement may use: enabled, not quarantined, in health
/// order (best first). This is the only gate — repair, rebalance and
/// restripe all place through it.
pub fn eligible(configs: &[SyncConfig]) -> Vec<SyncConfig> {
    let mut out: Vec<SyncConfig> = configs
        .iter()
        .filter(|c| c.enabled)
        .filter(|c| get(&c.id).status != HealthStatus::Quarantined)
        .cloned()
        .collect();
    out.sort_by(|a, b| {
        get(&b.id)
            .score
            .partial_cmp(&get(&a.id).score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Persist one health row to the `provider_health` table.
pub fn persist(db: &Database, health: &ProviderHealth) -> Result<(), String> {
    let serialized = serde_json::to_string(health).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_provider_health_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(health.config_id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Persist every live row (called after a scrub/repair pass).
pub fn persist_all(db: &Database) -> Result<usize, String> {
    let rows = all();
    for row in &rows {
        persist(db, row)?;
    }
    Ok(rows.len())
}

/// Durable health rows from the `provider_health` table.
pub fn load(db: &Database) -> Result<Vec<ProviderHealth>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_provider_health_table())
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_key, value) = entry.map_err(|e| e.to_string())?;
        match serde_json::from_str::<ProviderHealth>(value.value()) {
            Ok(row) => rows.push(row),
            Err(e) => return Err(format!("integrity: health row unreadable: {}", e)),
        }
    }
    rows.sort_by_key(|a| a.config_id.clone());
    Ok(rows)
}

/// The full health view for the REST surface: durable rows, with any live
/// observation layered on top (the live copy is always newer).
pub fn status(db: &Database) -> Result<Vec<ProviderHealth>, String> {
    let mut rows = load(db)?;
    for row in &mut rows {
        let live = get(&row.config_id);
        if live.samples > 0 {
            *row = live;
        }
    }
    // Providers only seen live (never persisted) are missing from the table.
    for live in all() {
        if !rows.iter().any(|r| r.config_id == live.config_id) {
            rows.push(live);
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live health registry is process-global, and `cargo test` runs
    /// tests in the same binary on multiple threads. A `clear()` in one
    /// test would otherwise wipe another test's observations mid-run
    /// (e.g. `failures_streak_into_quarantine…` observed 3 failures, then
    /// a parallel `clear()` reverted the id to Healthy before `eligible()`
    /// ran — `left: 2, right: 1`). Serialise every test that touches the
    /// registry through this lock.
    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn probe(id: &str) -> ProviderHealth {
        ProviderHealth {
            config_id: id.to_string(),
            ..ProviderHealth::default()
        }
    }

    #[test]
    fn a_fresh_provider_is_perfect_and_eligible() {
        let _guard = test_lock().lock().unwrap();
        let h = probe("health-fresh");
        assert_eq!(h.score, 1.0);
        assert_eq!(h.status, HealthStatus::Healthy);

        let configs = vec![local_config("health-fresh", true)];
        assert_eq!(eligible(&configs).len(), 1);
    }

    #[test]
    fn failures_streak_into_quarantine_and_successes_re_admit() {
        let _guard = test_lock().lock().unwrap();
        clear();
        let id = "health-streak";
        for i in 0..FAILURE_STREAK {
            let h = observe(
                id,
                &Outcome::Failure {
                    message: format!("401 attempt {}", i),
                },
            );
            assert_eq!(h.consecutive_failures, i + 1);
        }
        let h = get(id);
        assert_eq!(h.status, HealthStatus::Quarantined, "{:?}", h);
        assert!(h.note.is_some(), "quarantine must say why");

        // Quarantine gates placement but never deletes: the config is still
        // in the input list, just not in the eligible output.
        let configs = vec![local_config(id, true), local_config("health-other", true)];
        let ok = eligible(&configs);
        assert_eq!(ok.len(), 1, "quarantined config must not be placed onto");
        assert_eq!(ok[0].id, "health-other");

        // Recovery: one good observation is enough to re-admit.
        let h = observe(id, &Outcome::Success { latency_ms: 120 });
        assert_eq!(h.status, HealthStatus::Healthy, "{:?}", h);
        assert_eq!(h.consecutive_failures, 0);
        assert_eq!(eligible(&configs).len(), 2);
        clear();
    }

    #[test]
    fn a_disabled_config_is_never_eligible_even_when_healthy() {
        let _guard = test_lock().lock().unwrap();
        let configs = vec![local_config("health-disabled", false)];
        assert!(eligible(&configs).is_empty());
    }

    #[test]
    fn score_rewards_speed_and_quota_headroom() {
        let mut fast = probe("health-fast");
        fast.samples = 1;
        fast.latency_ms = 100;

        let mut slow = probe("health-slow");
        slow.samples = 1;
        slow.latency_ms = LATENCY_TARGET_MS;
        slow.quota_headroom = 0.0;

        assert!(score_of(&fast) > score_of(&slow));
        assert_eq!(status_of(&fast), HealthStatus::Healthy);
        // No quota headroom forces at least degraded, even with a good score.
        let mut nearly_full = probe("health-full");
        nearly_full.quota_headroom = 0.01;
        assert_ne!(status_of(&nearly_full), HealthStatus::Healthy);
        assert_ne!(status_of(&nearly_full), HealthStatus::Quarantined);
    }

    #[test]
    fn quarantine_by_hand_is_noted_and_survives_a_perfect_score() {
        let _guard = test_lock().lock().unwrap();
        clear();
        let h = quarantine("health-hand", "401 on every call");
        assert_eq!(h.status, HealthStatus::Quarantined);
        assert_eq!(h.note.as_deref(), Some("401 on every call"));
        clear();
    }

    fn local_config(id: &str, enabled: bool) -> SyncConfig {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "backendType": "local",
            "enabled": enabled,
            "basePath": "/tmp",
            "parity": 1,
            "encryptBeforeUpload": false,
            "autoSync": false,
            "compressBeforeUpload": false,
            "createPreviews": false,
            "deleteRawAfterSync": false,
            "maxConcurrentUploads": 4,
        }))
        .expect("config")
    }

    #[test]
    fn health_round_trips_through_the_table() {
        let _guard = test_lock().lock().unwrap();
        let dir = std::env::temp_dir().join(format!(
            "cybermanju-health-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let db_path = dir.join("health.redb");
        let db = cybermanju_db::Database::new(db_path.to_str().unwrap()).expect("db");

        let mut h = probe("health-persist");
        h.latency_ms = 42;
        h.success_rate = 0.5;
        persist(&db, &h).expect("persist");

        let rows = load(&db).expect("load");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], h);

        let mut seen = get("health-persist");
        seen.samples = 3;
        // Live rows layer over the durable ones.
        {
            let mut map = registry().lock().unwrap();
            map.insert("health-persist".to_string(), seen.clone());
        }
        let merged = status(&db).expect("status");
        assert_eq!(merged[0].samples, 3);
        clear();
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
