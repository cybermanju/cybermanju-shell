// CyberManju OS — Provider fs (crates/os/src/provider_fs.rs).
//! cybsh provider namespace: `/providers/<head>/<remote…>`.
//!
//! Everything outside the merged volume — every Google Drive folder and
//! GitHub/GitLab repo behind a connected sync config — is browsable here.
//! `<head>` is a VFS mount id (`vfs:mount:*` rows the file manager writes)
//! or a sync config id directly, so the namespace works before any mount
//! exists. Reads/writes go through the same classified sync backends as the
//! REST layer (`create_backend`), never through [`crate::api::Kernel`]:
//! `ls`/`cat`/`cp`/`mv`/`rm`/`mkdir`/`stat` branch here, every other verb
//! answers `unsupported:` instead of touching volume paths by accident.
//!
//! One-level listings mirror the browser canal: git trees synthesize
//! folders from recursive blob paths, Drive merges its file listing with
//! an explicit folder query (its file listing hides folders).

use cybermanju_db::Database;
use cybermanju_types::sync::{SyncBackendType, SyncConfig};

/// Cap for one `cat` print (files can be gigabytes; `cp` them to read fully).
pub const CAT_LIMIT_BYTES: usize = 1024 * 1024;
/// Cap for a recursive provider `rm -r` walk (refuse, don't loop forever).
const MAX_RM_ENTRIES: usize = 2000;
/// Mount rows live in the `kv` table under these keys (TS canal contract).
const MOUNT_INDEX_KEY: &str = "vfs:mount:index";

/// One `ls` row: a file or an immediate subdirectory of the prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvEntry {
    pub name: String,
    /// Provider-relative human path (`docs/a.md`, `docs/nested`).
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub modified_at: String,
}

/// `/providers/<head>[/<rest…>]` → `(head, rest)`. `None` outside the
/// namespace. Backslashes are separators (Windows paste), `.` segments are
/// dropped, `..` is refused (a provider listing has no parent above root).
pub fn split_provider_path(abs: &str) -> Option<(String, String)> {
    let norm = abs.replace('\\', "/");
    let segs: Vec<&str> = norm.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 || segs[0] != "providers" {
        return None;
    }
    if segs[1..].contains(&"..") {
        return None;
    }
    let rest: Vec<&str> = segs[2..].iter().filter(|s| **s != ".").cloned().collect();
    Some((segs[1].to_string(), rest.join("/")))
}

/// True when the absolute shell path addresses the provider namespace.
pub fn is_provider_path(abs: &str) -> bool {
    split_provider_path(abs).is_some()
}

/// Reduce a recursive blob listing to the immediate children of `prefix`:
/// direct files plus the folders implied by deeper paths. `prefix` is
/// provider-relative (`""` = root); `base` folds a config `base_path` in.
pub fn one_level(
    blobs: impl Iterator<Item = (String, u64, String)>,
    prefix: &str,
) -> Vec<ProvEntry> {
    let clean = prefix.trim().trim_matches('/');
    let wanted = if clean.is_empty() {
        String::new()
    } else {
        format!("{clean}/")
    };
    let mut dirs: Vec<String> = Vec::new();
    let mut out: Vec<ProvEntry> = Vec::new();
    for (path, size, modified) in blobs {
        if path.is_empty() || !path.starts_with(&wanted) {
            continue;
        }
        let rest = &path[wanted.len()..];
        if rest.is_empty() {
            continue;
        }
        match rest.find('/') {
            None => {
                let name = rest.to_string();
                out.push(ProvEntry {
                    name,
                    path: format!("{wanted}{rest}"),
                    is_dir: false,
                    size_bytes: size,
                    modified_at: modified,
                });
            }
            Some(slash) => {
                let dir_path = format!("{wanted}{}", &rest[..slash]);
                if !dirs.iter().any(|seen| seen == &dir_path) {
                    dirs.push(dir_path.clone());
                    let name = dir_path.rsplit('/').next().unwrap_or(&dir_path).to_string();
                    out.push(ProvEntry {
                        name,
                        path: dir_path,
                        is_dir: true,
                        size_bytes: 0,
                        modified_at: String::new(),
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| (!a.is_dir, &a.name).cmp(&(!b.is_dir, &b.name)));
    out
}

fn kv_get(db: &Database, key: &str) -> Result<Option<String>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_kv_table())
        .map_err(|e| e.to_string())?;
    Ok(table
        .get(key)
        .map_err(|e| e.to_string())?
        .map(|guard| guard.value().to_string()))
}

/// `(head, label)` rows for `ls /providers`: every VFS mount plus every
/// enabled config without one, so all repos/folders are visible up front.
pub fn list_mounts(db: &Database) -> Result<Vec<(String, String)>, String> {
    let mut rows: Vec<(String, String)> = Vec::new();
    let mut mounted: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Ok(Some(raw)) = kv_get(db, MOUNT_INDEX_KEY) {
        let ids: Vec<String> = serde_json::from_str(&raw).unwrap_or_default();
        for id in ids {
            // One corrupt mount row never hides the other providers.
            let body = kv_get(db, &format!("vfs:mount:{id}")).unwrap_or(None);
            let Some(body) = body else { continue };
            let json: serde_json::Value =
                serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
            let config_id = json
                .get("configId")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let name = json
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(id.as_str())
                .to_string();
            if config_id.is_empty() {
                continue;
            }
            mounted.insert(config_id.to_string());
            rows.push((id, name));
        }
    }
    let configs = db.list_sync_configs().map_err(|e| e.to_string())?;
    for c in configs.iter().filter(|c| c.enabled) {
        if mounted.contains(&c.id) {
            continue;
        }
        rows.push((c.id.clone(), mount_label(c)));
    }
    rows.sort_by_key(|a| a.1.clone());
    Ok(rows)
}

fn mount_label(c: &SyncConfig) -> String {
    let repo = c
        .repo_name
        .clone()
        .or_else(|| c.base_path.clone())
        .or_else(|| c.name.clone())
        .unwrap_or_else(|| c.id.clone());
    let short = repo.rsplit('/').next().unwrap_or(&repo);
    let backend = short_backend(&c.backend_type);
    format!("{backend} · {}", short.chars().take(32).collect::<String>())
}

fn short_backend(b: &SyncBackendType) -> &'static str {
    match b {
        SyncBackendType::Local => "local",
        SyncBackendType::GitHub => "github",
        SyncBackendType::GitLab => "gitlab",
        SyncBackendType::GoogleDrive => "drive",
    }
}

/// Load a sync config with its sealed secret merged (same merge the REST
/// config list performs — the row never serializes the token).
fn load_config(db: &Database, config_id: &str) -> Result<SyncConfig, String> {
    let mut config = db
        .list_sync_configs()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|c| c.id == config_id)
        .ok_or_else(|| format!("not_found: no provider '{config_id}' — `providers` lists them"))?;
    if config.token.is_none() {
        config.token = db.get_sync_secret(config_id).map_err(|e| e.to_string())?;
    }
    if !config.enabled {
        return Err(format!("unsupported: provider '{config_id}' is disabled"));
    }
    Ok(config)
}

/// `<head>` → live config: a VFS mount row first, the config id itself
/// second (works before any mount exists).
pub fn resolve_config(db: &Database, head: &str) -> Result<SyncConfig, String> {
    if head.trim().is_empty() {
        return Err("invalid: provider path needs a mount or config id".to_string());
    }
    if let Ok(Some(body)) = kv_get(db, &format!("vfs:mount:{head}")) {
        let json: serde_json::Value =
            serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
        if let Some(config_id) = json.get("configId").and_then(|v| v.as_str()) {
            if !config_id.is_empty() {
                return load_config(db, config_id);
            }
        }
    }
    load_config(db, head)
}

/// Mount root + listing prefix, one slash, no padding (`""` = provider root).
fn full_prefix(config: &SyncConfig, rest: &str) -> String {
    let base = config
        .base_path
        .as_deref()
        .unwrap_or("")
        .trim()
        .trim_matches('/');
    let rel = rest.trim().trim_matches('/');
    match (base.is_empty(), rel.is_empty()) {
        (true, true) => String::new(),
        (true, false) => rel.to_string(),
        (false, true) => base.to_string(),
        (false, false) => format!("{base}/{rel}"),
    }
}

fn refuse_local(config: &SyncConfig) -> Result<(), String> {
    if config.backend_type == SyncBackendType::Local {
        let base = config.base_path.as_deref().unwrap_or("?");
        return Err(format!(
            "unsupported: 'local' is a host path ({base}), not a remote — cybsh only reaches remotes here"
        ));
    }
    Ok(())
}

/// Immediate children of `rest` (files + folders, folders first).
pub fn list_dir(db: &Database, head: &str, rest: &str) -> Result<Vec<ProvEntry>, String> {
    let config = resolve_config(db, head)?;
    refuse_local(&config)?;
    let backend = cybermanju_sync::create_backend(&config)?;
    let prefix = full_prefix(&config, rest);
    let blobs = backend.list_files(&prefix)?;
    let mut out = one_level(
        blobs.into_iter().map(|f| {
            // Drive keys rows by file id, not by path — re-root them under
            // their human name so children of `prefix` stay addressable.
            let rel = if config.backend_type == SyncBackendType::GoogleDrive {
                if f.path.contains('/') {
                    f.path.clone()
                } else {
                    join_prefix(&prefix, &f.name)
                }
            } else {
                f.path.clone()
            };
            (rel, f.size_bytes, f.modified_at.clone())
        }),
        &prefix,
    );
    // Drive's file listing hides folders — merge the explicit folder query
    // (git trees already synthesize them from blob paths).
    if config.backend_type == SyncBackendType::GoogleDrive {
        for dir in backend.list_dirs(&prefix)? {
            let name = dir_name(&dir.path);
            if out.iter().any(|e| e.is_dir && e.name == name) {
                continue;
            }
            out.push(ProvEntry {
                name,
                path: dir.path.clone(),
                is_dir: true,
                size_bytes: 0,
                modified_at: dir.modified_at.clone(),
            });
        }
        out.sort_by(|a, b| (!a.is_dir, &a.name).cmp(&(!b.is_dir, &b.name)));
    }
    Ok(out)
}

fn join_prefix(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

fn dir_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// Download one provider file to bytes (staged through a temp file — the
/// backends verify partial downloads never leave residue behind).
pub fn read_file(db: &Database, head: &str, rest: &str) -> Result<Vec<u8>, String> {
    let clean = rest.trim().trim_matches('/');
    if clean.is_empty() {
        return Err("invalid: provider path must name a file".to_string());
    }
    let config = resolve_config(db, head)?;
    refuse_local(&config)?;
    let backend = cybermanju_sync::create_backend(&config)?;
    let prefix = full_prefix(&config, clean);
    let stage = stage_path("cybsh-read");
    let result = backend.download_file(&prefix, &stage);
    let bytes = result.and_then(|()| {
        std::fs::read(&stage).map_err(|e| format!("network: staged read failed: {e}"))
    });
    let _ = std::fs::remove_file(&stage);
    bytes
}

/// Upload bytes to a provider path. Returns the provider URL.
pub fn write_file(db: &Database, head: &str, rest: &str, data: &[u8]) -> Result<String, String> {
    let clean = rest.trim().trim_matches('/');
    if clean.is_empty() {
        return Err("invalid: cannot overwrite the provider root".to_string());
    }
    let config = resolve_config(db, head)?;
    refuse_local(&config)?;
    let backend = cybermanju_sync::create_backend(&config)?;
    let prefix = full_prefix(&config, clean);
    let stage = stage_path("cybsh-write");
    std::fs::write(&stage, data).map_err(|e| format!("network: cannot stage upload: {e}"))?;
    let result = backend.upload_file(&stage, &prefix);
    let _ = std::fs::remove_file(&stage);
    result
}

/// Delete one provider file.
pub fn delete_file(db: &Database, head: &str, rest: &str) -> Result<(), String> {
    let clean = rest.trim().trim_matches('/');
    if clean.is_empty() {
        return Err("invalid: cannot delete the provider root".to_string());
    }
    let config = resolve_config(db, head)?;
    refuse_local(&config)?;
    let backend = cybermanju_sync::create_backend(&config)?;
    backend.delete_file(&full_prefix(&config, clean))
}

/// File, directory, or missing — files win over same-named folders.
pub enum ProvKind {
    File {
        size_bytes: u64,
        modified_at: String,
    },
    Dir,
    Missing,
}

pub fn classify(db: &Database, head: &str, rest: &str) -> Result<ProvKind, String> {
    let clean = rest.trim().trim_matches('/');
    if clean.is_empty() {
        // Mount roots always exist once the head resolves.
        resolve_config(db, head)?;
        return Ok(ProvKind::Dir);
    }
    let config = resolve_config(db, head)?;
    refuse_local(&config)?;
    let backend = cybermanju_sync::create_backend(&config)?;
    let prefix = full_prefix(&config, clean);
    match backend.stat(&prefix) {
        Ok(Some(file)) => {
            // A zero-byte Drive row is ambiguous (empty file vs folder) —
            // the directory probe decides, folders winning ties.
            if config.backend_type == SyncBackendType::GoogleDrive
                && file.size_bytes == 0
                && dir_exists(db, head, clean)?
            {
                return Ok(ProvKind::Dir);
            }
            return Ok(ProvKind::File {
                size_bytes: file.size_bytes,
                modified_at: file.modified_at.clone(),
            });
        }
        Ok(None) => {
            if dir_exists(db, head, clean)? {
                return Ok(ProvKind::Dir);
            }
        }
        Err(err) if err.starts_with("not_found:") => {
            if dir_exists(db, head, clean)? {
                return Ok(ProvKind::Dir);
            }
        }
        Err(err) => return Err(err),
    }
    Ok(ProvKind::Missing)
}

/// True when `rest` lists as a non-empty directory (git trees cannot hold
/// empty folders, so emptiness is indistinguishable from missing there).
fn dir_exists(db: &Database, head: &str, rest: &str) -> Result<bool, String> {
    Ok(!list_dir(db, head, rest)?.is_empty())
}

/// Delete every file under a provider directory (bounded walk).
pub fn remove_tree(db: &Database, head: &str, rest: &str) -> Result<(usize, u64), String> {
    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut stack = vec![rest.trim().trim_matches('/').to_string()];
    while let Some(dir) = stack.pop() {
        let entries = list_dir(db, head, &dir)?;
        for entry in entries {
            let child = if dir.is_empty() {
                entry.name.clone()
            } else {
                format!("{dir}/{}", entry.name)
            };
            if entry.is_dir {
                stack.push(child);
            } else {
                bytes += entry.size_bytes;
                delete_file(db, head, &child)?;
                files += 1;
            }
            if files > MAX_RM_ENTRIES {
                return Err(format!(
                    "too_large: '{rest}' holds more than {MAX_RM_ENTRIES} files — delete it in parts"
                ));
            }
        }
    }
    Ok((files, bytes))
}

fn stage_path(tag: &str) -> String {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir()
        .join(format!("{}-{}-{}.bin", tag, std::process::id(), nonce))
        .to_string_lossy()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_paths_split_like_the_file_manager() {
        assert_eq!(
            split_provider_path("/providers/m1/docs/a.md"),
            Some(("m1".to_string(), "docs/a.md".to_string()))
        );
        assert_eq!(
            split_provider_path("/providers/m1"),
            Some(("m1".to_string(), String::new()))
        );
        assert_eq!(
            split_provider_path("/providers/m1/"),
            Some(("m1".to_string(), String::new()))
        );
        assert_eq!(split_provider_path("/providers"), None);
        assert_eq!(split_provider_path("/photos/a.jpg"), None);
        // `..` never escapes the namespace.
        assert_eq!(split_provider_path("/providers/m1/../x"), None);
    }

    #[test]
    fn recursive_blobs_become_one_level() {
        let rows = one_level(
            vec![
                ("docs/a.md".to_string(), 12, String::new()),
                ("docs/nested/b.md".to_string(), 34, String::new()),
                ("README.md".to_string(), 5, String::new()),
            ]
            .into_iter(),
            "",
        );
        assert_eq!(rows.len(), 2);
        assert!(rows[0].is_dir && rows[0].name == "docs");
        assert!(!rows[1].is_dir && rows[1].name == "README.md");

        let inner = one_level(
            vec![
                ("docs/a.md".to_string(), 12, String::new()),
                ("docs/nested/b.md".to_string(), 34, String::new()),
            ]
            .into_iter(),
            "docs",
        );
        assert_eq!(inner.len(), 2);
        assert!(inner[0].is_dir && inner[0].path == "docs/nested");
        assert_eq!(inner[1].path, "docs/a.md");
    }

    #[test]
    fn prefixes_filter_and_base_paths_fold() {
        let rows = one_level(
            vec![("vault/a.cyb3".to_string(), 1, String::new())].into_iter(),
            "other",
        );
        assert!(rows.is_empty());
        let cfg = SyncConfig {
            id: "c".to_string(),
            backend_type: SyncBackendType::GitHub,
            enabled: true,
            account_id: None,
            name: None,
            base_path: Some("/backups/2026/".to_string()),
            repo_name: Some("o/r".to_string()),
            branch: Some("main".to_string()),
            token: None,
            folder_id: None,
            auto_sync: false,
            compress_before_upload: false,
            create_previews: false,
            delete_raw_after_sync: false,
            max_concurrent_uploads: 1,
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
        };
        assert_eq!(super::full_prefix(&cfg, ""), "backups/2026");
        assert_eq!(super::full_prefix(&cfg, "/x/y/"), "backups/2026/x/y");
    }

    #[test]
    fn provider_verbs_need_the_database() {
        // Without a db every provider verb refuses honestly (never touches
        // volume paths by accident).
        for line in [
            "ls /providers/m1",
            "cat /providers/m1/a.md",
            "stat /providers/m1/a.md",
        ] {
            let err = crate::shell::execute(line, None).expect_err("must refuse");
            assert!(
                err.contains("needs the database") || err.contains("unsupported:"),
                "got {err}"
            );
        }
    }
}
