// CyberManju OS — Lib (crates/os/src/lib.rs).
//! CyberManju OS layer (AGENT-8): the system terminal, task table, compute
//! fan-out and the typed syscall boundary every app-facing surface sits on.
//!
//! AGENT-8: build the modules here — `shell` (`cybsh`), `task` (`ps`/`top`),
//! `compute` (fan-out scheduler), `api` (open/read/write/…/df). Declare each
//! with `pub mod` as you create the file.

/// Canonical shell prompt.
pub const PROMPT: &str = "cybsh> ";

/// Shell/OS layer format version reported by `cybsh`'s `version` command.
pub const OS_VERSION: u32 = 1;

/// The typed syscall boundary: `open · read · write · seek · close · stat ·
/// unlink · readdir · mkdir · rename · du · df`, plus the merged-volume
/// capacity model (`df` grows as `.cybermanju` disks attach).
pub mod api;

pub use api::{
    dir_size, get_disk, list_disks, put_disk, remove_disk, DirEntry, DiskRecord, Fd, Kernel,
    OpenFlags, Stat, VolumeDf, Whence, DEFAULT_CAPACITY_BYTES,
};

/// The process table behind `ps` / `top` / `kill`, plus `top`'s live system
/// stats (load average, RSS, CPU) read from `/proc`.
pub mod task;

pub use task::{
    kill, ps, repair_rows, top, LoadAvg, MemInfo, PsSnapshot, Task, TaskCounts, TaskState,
    TaskTable, TopSnapshot, MAX_TASKS, REPAIR_ID_BASE,
};

/// Vault snapshot exports: passphrase-sealed CYBMJ01 containers in the
/// volume or on any provider (`backup` verb, scheduled or on demand).
pub mod backup;

/// `cybsh` — the system terminal: tokenizer, parser and the command table.
pub mod shell;

/// Outside-container files: the `/providers/<mount-or-config>/…` namespace
/// every Google Drive folder and GitHub/GitLab repo is browsable through.
/// The shell's file verbs branch here; nothing else touches a provider.
pub mod provider_fs;

/// `.cybsh` scripts — interpreted automation over the same shell (`run`).
pub mod script;

/// Schedule expressions (`# schedule:` frontmatter, `cron` verb) — parse +
/// next-fire computation. Pure, no threads.
pub mod schedule;

pub use schedule::ScheduleSpec;

/// The cron daemon: background thread that fires due schedules through the
/// script interpreter. Wired in `web/lib.rs` + `tauri/commands/sync.rs`
/// (both must call `ensure_started`; a single wiring leaves the other
/// transport dark — the `ensure_scrub_started` lesson).
pub mod scheduler;

pub use shell::{
    command_table, completions, execute, parse_ai_command, parse_sync_start, run, AiCommand,
    SyncStart,
};

/// Compute fan-out: worker scoring (`workers`) and the job scheduler
/// (`jobs`, `compute run`).
pub mod compute;

pub use compute::{available_jobs, workers, JobReport, Workers};

/// Test-only helpers: one scratch volume shared by every unit test so they
/// can never touch the developer's real volume.
#[cfg(test)]
pub(crate) mod testutil {
    use std::path::Path;
    use std::sync::OnceLock;

    /// Point the process-wide kernel at a fresh temp directory and return it.
    /// First caller wins; every other test reuses the same directory, so
    /// concurrent tests cannot race on the kernel's root.
    pub fn volume_dir() -> &'static Path {
        static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
        let dir = DIR.get_or_init(|| {
            let dir = tempfile::tempdir().expect("scratch volume");
            let _ = crate::Kernel::global().set_root_for_tests(dir.path().to_path_buf());
            dir
        });
        dir.path()
    }
}
