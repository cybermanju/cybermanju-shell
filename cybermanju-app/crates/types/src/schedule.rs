// CyberManju OS — scheduler rows: `schedules` (recurring triggers) and
// `schedule_runs` (fire history). Values are JSON strings, same convention
// as every other table.

use serde::{Deserialize, Serialize};

/// One recurring schedule: a `.cybsh` script + an expression, plus last/next
/// fire bookkeeping. Stored in the `schedules` table keyed by `id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRow {
    pub id: String,
    /// Volume path of the `.cybsh` script to run (absolute inside the volume).
    pub path: String,
    /// Cron expression (`"30 2 * * *"`) or interval (`"every 10m"`).
    pub expr: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fired_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_fire_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_id: Option<String>,
    /// Fire once on daemon start even if `next_fire_at` is in the future.
    #[serde(default)]
    pub run_on_boot: bool,
}

/// One completed (or failed) fire of a schedule. Stored in `schedule_runs`
/// keyed by `run_id`, pruned to [`SCHEDULE_RUN_HISTORY_LIMIT`] rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRun {
    pub run_id: String,
    pub schedule_id: String,
    pub started_at: String,
    pub finished_at: String,
    /// `ok` | `error` | `not_found` — same family as sync statuses.
    pub status: String,
    /// First line of the script output, or the AGENT-1-prefixed error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tail: Option<String>,
}

/// Rows kept in `schedule_runs` — enough for a UI history drawer, few
/// enough that the prune scan stays trivial.
pub const SCHEDULE_RUN_HISTORY_LIMIT: usize = 20;
