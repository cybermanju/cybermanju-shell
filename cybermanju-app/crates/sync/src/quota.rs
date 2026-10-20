// CyberManju OS — quota / usage probing (AGENT-1)
//
// `quota::usage(config)` asks the provider how much room is left. Where an
// endpoint exists (Drive quotaInfo, GitHub rate-limit) the numbers are real;
// where the provider exposes none (local disk) the answer
// is an honest `QuotaUsage` with `None` fields plus a `detail` note — never
// a fabricated number.
//
// <<< AGENT-1 QUOTA >>>

use crate::backends::{gitlab_instance_base, http_client, send_classified, urlencoding};
use crate::oauth;
use crate::rate_limit;
use cybermanju_types::sync::{SyncBackendType, SyncConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaUsage {
    pub backend_type: SyncBackendType,
    /// Storage quota in bytes when the provider publishes one.
    pub total_bytes: Option<u64>,
    /// Storage already consumed in bytes when the provider publishes it.
    pub used_bytes: Option<u64>,
    /// Remaining API requests in the current window (rolling limits only).
    pub remaining_requests: Option<u64>,
    /// Window size for `remaining_requests`.
    pub request_limit: Option<u64>,
    /// Unix seconds at which the request window resets.
    pub reset_at: Option<u64>,
    /// How the numbers were obtained (or why they are unknown).
    pub detail: String,
}

impl QuotaUsage {
    fn unknown(backend_type: SyncBackendType, detail: &str) -> Self {
        Self {
            backend_type,
            total_bytes: None,
            used_bytes: None,
            remaining_requests: None,
            request_limit: None,
            reset_at: None,
            detail: detail.to_string(),
        }
    }

    /// Bytes still available, when both sides of the quota are known.
    pub fn remaining_bytes(&self) -> Option<u64> {
        Some(self.total_bytes?.saturating_sub(self.used_bytes?))
    }
}

/// Probe provider quota/usage for a sync configuration.
pub fn usage(config: &SyncConfig) -> Result<QuotaUsage, String> {
    match config.backend_type {
        SyncBackendType::GoogleDrive => drive_usage(config),
        SyncBackendType::GitHub => github_usage(config),
        SyncBackendType::GitLab => gitlab_usage(config),
        SyncBackendType::Local => Ok(QuotaUsage::unknown(
            SyncBackendType::Local,
            "local filesystem quota is not tracked (best-effort: unknown)",
        )),
    }
}

// ─── Google Drive: about?fields=quotaInfo ────────────────────────────

fn drive_usage(config: &SyncConfig) -> Result<QuotaUsage, String> {
    let _permit = rate_limit::acquire(&SyncBackendType::GoogleDrive)?;
    let token = oauth::resolve_token(config)?;
    let client = http_client()?;
    let url = "https://www.googleapis.com/drive/v3/about?fields=user,quotaInfo";

    let resp = send_classified("Google Drive", "quota", &[], || {
        Ok(client
            .get(url)
            .header("Authorization", format!("Bearer {}", token)))
    })?;

    let body = resp
        .text()
        .map_err(|e| format!("network: Google Drive quota response unreadable: {}", e))?;
    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("network: Google Drive quota response unparseable: {}", e))?;

    let quota = &json["quotaInfo"];
    let used = quota["usage"]
        .as_u64()
        .or_else(|| quota["usage"].as_str().and_then(|s| s.parse().ok()));
    let trash = quota["usageInDriveTrash"]
        .as_u64()
        .or_else(|| {
            quota["usageInDriveTrash"]
                .as_str()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0);
    let total = quota["limit"]
        .as_u64()
        .or_else(|| quota["limit"].as_str().and_then(|s| s.parse().ok()));

    let mut usage = QuotaUsage::unknown(SyncBackendType::GoogleDrive, "Drive v3 about/quotaInfo");
    usage.used_bytes = used.map(|u| u.saturating_add(trash));
    usage.total_bytes = total;
    if usage.used_bytes.is_none() && usage.total_bytes.is_none() {
        usage.detail = "Drive returned no quotaInfo for this account".to_string();
    }
    Ok(usage)
}

// ─── GitHub: /rate_limit ─────────────────────────────────────────────

fn github_usage(config: &SyncConfig) -> Result<QuotaUsage, String> {
    let _permit = rate_limit::acquire(&SyncBackendType::GitHub)?;
    let token = oauth::resolve_token(config)?;
    let client = http_client()?;
    let url = "https://api.github.com/rate_limit";

    let resp = send_classified("GitHub", "quota", &[], || {
        Ok(client
            .get(url)
            .header("Authorization", format!("token {}", token))
            .header("Accept", "application/vnd.github+json"))
    })?;

    let body = resp
        .text()
        .map_err(|e| format!("network: GitHub rate-limit response unreadable: {}", e))?;
    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("network: GitHub rate-limit response unparseable: {}", e))?;

    let core = &json["resources"]["core"];
    let mut usage = QuotaUsage::unknown(
        SyncBackendType::GitHub,
        "GitHub REST rate-limit window (storage quota not published)",
    );
    usage.request_limit = core["limit"].as_u64();
    usage.remaining_requests = core["remaining"].as_u64();
    usage.reset_at = core["reset"].as_u64();
    Ok(usage)
}

// ─── GitLab: project statistics (best-effort) ────────────────────────

fn gitlab_usage(config: &SyncConfig) -> Result<QuotaUsage, String> {
    let _permit = rate_limit::acquire(&SyncBackendType::GitLab)?;
    let token = oauth::resolve_token(config)?;
    let project_id = config
        .repo_name
        .as_deref()
        .ok_or("not_found: GitLab backend requires project_id (use repo_name field)")?;

    // Same instance-URL resolution the backend uses (see backends.rs).
    let base = gitlab_instance_base(config);
    let client = http_client()?;
    let url = format!(
        "{}/api/v4/projects/{}?statistics=true",
        base,
        urlencoding(project_id)
    );

    // 401 stays un-allowed so a bad token surfaces as `auth:`; 403/404 mean
    // "statistics not visible to this token" — best-effort, not a failure.
    let resp = send_classified("GitLab", "quota", &[403, 404], || {
        Ok(client.get(&url).header("PRIVATE-TOKEN", &token))
    })?;

    let status = resp.status().as_u16();
    if status != 200 {
        return Ok(QuotaUsage::unknown(
            SyncBackendType::GitLab,
            &format!(
                "project statistics need maintainer access (HTTP {}) — best-effort: unknown",
                status
            ),
        ));
    }

    let body = resp
        .text()
        .map_err(|e| format!("network: GitLab quota response unreadable: {}", e))?;
    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("network: GitLab quota response unparseable: {}", e))?;

    let stats = &json["statistics"];
    let used = stats["storage_size"]
        .as_u64()
        .or_else(|| stats["repository_size"].as_u64());
    let mut usage = QuotaUsage::unknown(
        SyncBackendType::GitLab,
        "project statistics.storage_size (project storage has no provider quota)",
    );
    usage.used_bytes = used;
    Ok(usage)
}

// ===========================================================================
// <<< AGENT-6 VOLUME ACCOUNTING: "more providers → more space" needs one
// number the write path can consult before a single byte leaves — how much
// room the merged `.cybermanju` volume still has (MISSING.md D1).
//
// The `disks` rows live in redb, which this crate deliberately does not
// depend on: the dependency arrow points *into* `cybermanju-sync`
// (`cybermanju-disk` needs `create_backend` and `usage`), and a cycle is not
// allowed. So the substrate *publishes* its accounting through these hooks
// and `cybermanju-disk` registers them from a `#[ctor]`, which arms them
// before `main` — no call-ordering requirement on the HTTP layer, the
// auto-sync scheduler, or a test.
//
// A build without `crates/disk` has no volume at all: `admit_write` and
// `charge_disk` then have nothing to police and return `Ok`, while
// `volume_usage` reports `unsupported:` rather than inventing a number.
// >>>

use cybermanju_db::Database;

/// Merged view of every **attached** `.cybermanju` disk — one logical volume.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeUsage {
    /// Sum of the choosable `capacityBytes` of all attached disks.
    pub total_bytes: u64,
    /// Bytes already placed on those disks.
    pub used_bytes: u64,
    /// `total_bytes - used_bytes` (never negative).
    pub free_bytes: u64,
    /// How many disks are attached right now.
    pub disk_count: u32,
}

/// The three operations `cybermanju-disk` installs at process start.
#[derive(Clone, Copy)]
pub struct VolumeHooks {
    /// `df` numbers for the merged volume.
    pub usage: fn(&Database) -> Result<VolumeUsage, String>,
    /// Refuse `bytes` when the volume cannot hold them.
    pub admit: fn(&Database, u64) -> Result<(), String>,
    /// Charge `bytes` of provider space to the disk bound to `config_id`.
    pub charge: fn(&Database, &str, u64) -> Result<(), String>,
}

static VOLUME_HOOKS: std::sync::OnceLock<VolumeHooks> = std::sync::OnceLock::new();

/// Install the volume hooks. Called exactly once, from `cybermanju-disk`'s
/// static constructor; a second registration is a programming error and is
/// refused instead of silently replacing the live hooks.
pub fn register_volume_hooks(hooks: VolumeHooks) -> Result<(), String> {
    VOLUME_HOOKS
        .set(hooks)
        .map_err(|_| "volume hooks are already registered".to_string())
}

/// The live hooks, when a substrate is linked in.
pub fn volume_hooks() -> Option<&'static VolumeHooks> {
    VOLUME_HOOKS.get()
}

/// `df` for the merged volume. `unsupported:` when no substrate is linked —
/// never a fabricated total.
pub fn volume_usage(db: &Database) -> Result<VolumeUsage, String> {
    let hooks = VOLUME_HOOKS.get().ok_or_else(|| {
        "unsupported: no .cybermanju volume substrate is linked into this build".to_string()
    })?;
    (hooks.usage)(db)
}

/// Admission control (D1): refuse a payload the merged volume cannot hold.
///
/// Returns `Ok(())` when no volume exists — there is nothing to enforce, and
/// the sync engine must keep working for installs that never created a disk.
pub fn admit_write(db: &Database, bytes: u64) -> Result<(), String> {
    match VOLUME_HOOKS.get() {
        Some(hooks) => (hooks.admit)(db, bytes),
        None => Ok(()),
    }
}

/// Charge `bytes` to the disk bound to `config_id` after a successful
/// upload, so provider space consumed by the sync engine shows up in `df`.
/// A no-op when no disk is bound to that config (or no substrate is linked).
pub fn charge_disk(db: &Database, config_id: &str, bytes: u64) -> Result<(), String> {
    match VOLUME_HOOKS.get() {
        Some(hooks) => (hooks.charge)(db, config_id, bytes),
        None => Ok(()),
    }
}

#[cfg(test)]
mod volume_tests {
    use super::*;
    use cybermanju_db::Database;

    #[test]
    fn without_a_substrate_admission_stays_open_and_usage_is_honest() {
        // A build (or test binary) that never links `cybermanju-disk` has no
        // volume: writes must keep working, and `df` must not invent numbers.
        if volume_hooks().is_some() {
            return; // substrate linked in this binary — the other arm is moot
        }
        let path = std::env::temp_dir().join(format!(
            "cybermanju-quota-{}-{}.redb",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let db = Database::new(path.to_str().expect("utf8 path")).expect("db");
        assert!(admit_write(&db, u64::MAX).is_ok(), "no volume → admit");
        assert!(charge_disk(&db, "cfg", 1024).is_ok(), "no volume → charge");
        let err = volume_usage(&db).expect_err("must not fabricate a volume");
        assert!(err.starts_with("unsupported:"), "{err}");
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn volume_usage_serializes_camel_case() {
        let json = serde_json::to_string(&VolumeUsage {
            total_bytes: 3,
            used_bytes: 1,
            free_bytes: 2,
            disk_count: 1,
        })
        .expect("json");
        assert!(json.contains("\"totalBytes\":3"), "{json}");
        assert!(json.contains("\"diskCount\":1"), "{json}");
    }
}
