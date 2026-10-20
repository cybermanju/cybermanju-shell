// CyberManju OS — Testkit (crates/disk/src/testkit.rs).
//! Shared scaffolding for the disk/volume unit tests: a scratch database, a
//! local provider directory and a helper to make disks.
//!
//! `#[cfg(test)]` — none of this ships in the crate's public surface.

use crate::catalog::DiskRow;
use crate::disk;
use cybermanju_db::Database;
use cybermanju_types::sync::{SyncBackendType, SyncConfig};
use std::path::{Path, PathBuf};

/// A `SyncConfig` pointing at a local directory, fully populated (the type
/// has no `Default`, and every field matters to `create_backend`).
pub fn local_config(id: &str, base_path: &str) -> SyncConfig {
    SyncConfig {
        id: id.to_string(),
        backend_type: SyncBackendType::Local,
        enabled: true,
        account_id: None,
        name: Some("local provider".into()),
        base_path: Some(base_path.to_string()),
        repo_name: None,
        branch: None,
        token: None,
        folder_id: None,
        auto_sync: false,
        compress_before_upload: true,
        create_previews: false,
        delete_raw_after_sync: false,
        max_concurrent_uploads: 2,
        encrypt_before_upload: true,
        conflict_policy: Default::default(),
        placement: Default::default(),
        parity: 1,
        require_encryption: false,
        obfuscate_names: false,
        mirror: false,
        key_holder: false,
        oauth_credentials: None,
        created_at: None,
        updated_at: None,
    }
}

/// Scratch database + local provider + the config row that binds them.
pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub db: Database,
    pub provider: PathBuf,
    pub config_id: String,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Database::new(dir.path().join("disk.redb").to_str().unwrap()).expect("db");
        let provider = dir.path().join("provider");
        std::fs::create_dir_all(&provider).expect("provider dir");
        let config_id = format!("cfg-{}", uuid::Uuid::new_v4());
        let config = local_config(&config_id, &provider.to_string_lossy());
        let tx = db.begin_write().expect("write txn");
        {
            let mut table = tx
                .open_table(Database::get_sync_configs_table())
                .expect("table");
            let json = serde_json::to_string(&config).expect("json");
            table
                .insert(config_id.as_str(), json.as_str())
                .expect("insert");
        }
        tx.commit().expect("commit");
        Self {
            dir,
            db,
            provider,
            config_id,
        }
    }

    /// Create an attached disk of `size_bytes` with the fixture passphrase.
    pub fn create(&self, size: u64) -> DiskRow {
        self.create_with(size, "hunter2")
    }

    pub fn create_with(&self, size: u64, passphrase: &str) -> DiskRow {
        let id = format!("disk-{}", uuid::Uuid::new_v4());
        let path = self.dir.path().join(format!("{}.cybermanju", id));
        disk::create_at(&self.db, &self.config_id, size, passphrase, &path).expect("create")
    }

    /// Objects stored for one disk (the `…/c/` chunk directory).
    pub fn remote_blocks(&self, disk_id: &str) -> usize {
        let dir = self
            .provider
            .join("cybermanju_disk")
            .join(disk_id)
            .join("c");
        std::fs::read_dir(dir)
            .map(|entries| entries.count())
            .unwrap_or(0)
    }

    /// Every object stored under `cybermanju_disk/` — used to prove that a
    /// refused write uploads nothing at all.
    pub fn remote_files(&self) -> usize {
        count_files(&self.provider.join("cybermanju_disk"))
    }
}

fn count_files(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total += count_files(&path);
        } else {
            total += 1;
        }
    }
    total
}
