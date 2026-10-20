// CyberManju OS — WASM provider canal (canal B).
//
// The Rust half of the read path: probe / list / fetch against a provider,
// byte-oriented, no `std::fs` anywhere — the same BlockStore-style boundary
// `wnfs` uses, so nothing platform-specific leaks into the parser code.
// TypeScript only orchestrates (token lookup, decode, cache, VFS paths).
//
// CORS-OK only, by decision: `github`, `gitlab`, `googleDrive`. Anything
// else is refused up front with an honest prefix instead of failing as an
// opaque network error.
//
// The provider HTTP shapes mirror `crates/sync/src/backends.rs` (Git trees,
// GitLab repository/tree, Drive files.list + alt=media) so a listing means
// the same thing here as it does on the desktop.

use serde::{Deserialize, Serialize};

/// One row of a provider directory listing.
///
/// `path` is provider-relative and is what the virtual FS shows;
/// `locator` is what `canal_fetch` needs — the same path for the Git
/// providers, the file **id** for Drive (whose native backend also keys by
/// id, `backends.rs::list_files`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanalEntry {
    pub name: String,
    pub path: String,
    pub locator: String,
    pub is_dir: bool,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub modified_at: String,
    #[serde(default)]
    pub url: String,
}

/// The provider half of a mount. Field names and casing mirror
/// `cybermanju_types::sync::SyncConfig`, so a sync config can be handed
/// straight through (`backendType` is accepted as an alias of `backend`).
/// Every field is optional: `SyncConfig.token` is `skip_serializing`, so
/// the caller may legitimately omit it and pass the token separately.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanalConfig {
    /// `github` | `gitlab` | `googleDrive` — anything else is refused.
    #[serde(alias = "backendType")]
    pub backend: Option<String>,
    pub token: Option<String>,
    /// GitHub `owner/repo`; GitLab project id or `group/project`.
    pub repo_name: Option<String>,
    /// Branch/ref — GitHub and GitLab only.
    pub branch: Option<String>,
    /// Mount root on the provider; `list` folds it into the prefix.
    pub base_path: Option<String>,
    /// Google Drive root folder id (None/empty = `root`).
    pub folder_id: Option<String>,
    /// GitLab self-managed instance base URL (None = gitlab.com).
    pub instance_url: Option<String>,
}

impl CanalConfig {
    /// Canonical backend name — tolerant of whatever casing the sync
    /// config serde emitted (`gitHub`, `GitHub`, `github` …).
    pub fn backend(&self) -> String {
        normalize_backend(self.backend.as_deref().unwrap_or_default())
    }

    pub fn token(&self) -> &str {
        self.token.as_deref().unwrap_or_default()
    }

    pub fn repo(&self) -> &str {
        self.repo_name.as_deref().unwrap_or_default()
    }

    pub fn branch_or_main(&self) -> String {
        let branch = self.branch.as_deref().unwrap_or_default().trim();
        if branch.is_empty() {
            "main".to_string()
        } else {
            branch.to_string()
        }
    }

    pub fn gitlab_base(&self) -> String {
        let base = self.instance_url.as_deref().unwrap_or_default().trim();
        if base.is_empty() {
            "https://gitlab.com".to_string()
        } else {
            base.trim_end_matches('/').to_string()
        }
    }

    /// Mount root + a listing prefix, normalised to one slash and no
    /// leading/trailing slashes (or `""` for the root itself).
    pub fn full_prefix(&self, relative: &str) -> String {
        let base = self
            .base_path
            .as_deref()
            .unwrap_or("")
            .trim()
            .trim_matches('/');
        let rel = relative.trim().trim_matches('/');
        match (base.is_empty(), rel.is_empty()) {
            (true, true) => String::new(),
            (true, false) => rel.to_string(),
            (false, true) => base.to_string(),
            (false, false) => format!("{base}/{rel}"),
        }
    }
}

/// The three providers a browser can actually read. Kept next to the code
/// that refuses the others so the two can never drift.
pub const CORS_OK_BACKENDS: [&str; 3] = ["github", "gitlab", "googleDrive"];

/// Hard cap on listing pages, mirroring `backends.rs::MAX_PAGES`
/// (scaled down: a browser mounts directories, it does not crawl repos).
pub const MAX_LISTING_PAGES: usize = 50;

pub const DRIVE_FOLDER_MIME: &str = "application/vnd.google-apps.folder";
/// Drive API base — only used by the wasm32 browser backend below, so it is
/// cfg-gated like its callers (otherwise host `clippy --all-targets` fails
/// on dead code under `-D warnings`).
#[cfg(target_arch = "wasm32")]
const DRIVE_FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";

/// Fold any spelling of a backend name onto the canonical one the
/// allowlist and refusal messages use.
pub fn normalize_backend(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "github" => "github".to_string(),
        "gitlab" => "gitlab".to_string(),
        "googledrive" => "googleDrive".to_string(),
        "local" => "local".to_string(),
        other => other.to_string(),
    }
}

pub fn backend_supported(backend: &str) -> bool {
    CORS_OK_BACKENDS.contains(&normalize_backend(backend).as_str())
}

/// Why a backend cannot be used from a browser — honest, never a generic
/// failure. `local` is the local volume, not a remote canal; everything
/// else outside the allowlist is simply not one of the browser canals.
pub fn refusal(backend: &str) -> String {
    match normalize_backend(backend).as_str() {
        "local" => {
            "unsupported: 'local' is not a remote provider — it is the local volume".to_string()
        }
        other => format!(
            "unsupported: '{other}' is not a browser canal (CORS-OK only: {})",
            CORS_OK_BACKENDS.join(", ")
        ),
    }
}

pub fn ensure_supported(cfg: &CanalConfig) -> Result<(), String> {
    if backend_supported(&cfg.backend()) {
        Ok(())
    } else {
        Err(refusal(&cfg.backend()))
    }
}

pub fn ensure_token(cfg: &CanalConfig) -> Result<(), String> {
    if cfg.token().trim().is_empty() {
        return Err(format!(
            "auth: no token for {} — connect the provider first",
            cfg.backend()
        ));
    }
    Ok(())
}

// ─── URL / path helpers (mirrors `backends.rs`) ──────────────────────

/// Percent-encode everything outside the unreserved set.
pub fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => {
                out.push('%');
                out.push_str(&format!("{other:02X}"));
            }
        }
    }
    out
}

/// Percent-encode a path but keep `/` separators.
pub fn encode_path(path: &str) -> String {
    path.split('/').map(urlencode).collect::<Vec<_>>().join("/")
}

/// `owner/repo` → `(owner, repo)`, exactly like `backends.rs::parse_repo`.
pub fn parse_repo(repo_name: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = repo_name.trim_start_matches('/').splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(format!(
            "unsupported: repo name '{repo_name}' is invalid — expected 'owner/repo'"
        ));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// The listing prefix used by the Git providers: `""` for the root,
/// otherwise `dir/` with exactly one trailing slash.
pub fn wanted_prefix(prefix: &str) -> String {
    let clean = prefix.trim().trim_matches('/');
    if clean.is_empty() {
        String::new()
    } else {
        format!("{clean}/")
    }
}

pub fn file_name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

// ─── one-level listing ───────────────────────────────────────────────

/// Reduce a *recursive* Git tree listing to the children of `prefix`:
/// direct files plus the directories that must appear as rows. Directory
/// rows are synthesised from the paths of deeper blobs, because the VFS
/// browses lazily and never materialises the whole tree.
pub fn children_from_git_paths<'a, I>(
    paths: I,
    prefix: &str,
    size_of: impl Fn(&str) -> Option<u64>,
    url_of: impl Fn(&str) -> String,
) -> Vec<CanalEntry>
where
    I: Iterator<Item = (&'a str, bool)>,
{
    let wanted = wanted_prefix(prefix);
    let mut dirs: Vec<String> = Vec::new();
    let mut files: Vec<CanalEntry> = Vec::new();
    for (path, explicit_dir) in paths {
        if path.is_empty() || !path.starts_with(&wanted) {
            continue;
        }
        let rest = &path[wanted.len()..];
        if rest.is_empty() {
            continue;
        }
        match rest.find('/') {
            None => {
                if explicit_dir {
                    push_dir(&mut dirs, &mut files, path, &url_of);
                } else {
                    files.push(CanalEntry {
                        name: file_name_of(path),
                        path: path.to_string(),
                        locator: path.to_string(),
                        is_dir: false,
                        size_bytes: size_of(path).unwrap_or(0),
                        modified_at: String::new(),
                        url: url_of(path),
                    });
                }
            }
            Some(slash) => {
                let dir_path = format!("{wanted}{}", &rest[..slash]);
                push_dir(&mut dirs, &mut files, &dir_path, &url_of);
            }
        }
    }
    files.sort_by(|a, b| (!a.is_dir, &a.name).cmp(&(!b.is_dir, &b.name)));
    files
}

fn push_dir(
    dirs: &mut Vec<String>,
    out: &mut Vec<CanalEntry>,
    path: &str,
    url_of: &impl Fn(&str) -> String,
) {
    if dirs.iter().any(|seen| seen == path) {
        return;
    }
    dirs.push(path.to_string());
    out.push(CanalEntry {
        name: file_name_of(path),
        path: path.to_string(),
        locator: path.to_string(),
        is_dir: true,
        size_bytes: 0,
        modified_at: String::new(),
        url: url_of(path),
    });
}

/// Parse a `GET /repos/{o}/{r}/git/trees/{ref}?recursive=1` answer.
pub fn entries_from_github_tree(
    json: &serde_json::Value,
    prefix: &str,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Result<Vec<CanalEntry>, String> {
    if json["truncated"].as_bool().unwrap_or(false) {
        return Err(format!(
            "too_large: the tree for {owner}/{repo} exceeds the GitHub tree API limit — \
             mount a subdirectory instead"
        ));
    }
    let tree = json["tree"]
        .as_array()
        .ok_or("network: GitHub tree listing was not an object")?;
    let raw: Vec<(&str, bool)> = tree
        .iter()
        .filter_map(|entry| {
            let path = entry["path"].as_str()?;
            let kind = entry["type"].as_str().unwrap_or_default();
            let dir = kind == "tree";
            if kind != "blob" && !dir {
                return None;
            }
            Some((path, dir))
        })
        .collect();
    let sizes = |path: &str| -> Option<u64> {
        tree.iter()
            .find(|entry| entry["path"].as_str() == Some(path))?
            .get("size")
            .and_then(|v| v.as_u64())
    };
    let urls = |path: &str| -> String {
        format!(
            "https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{}",
            encode_path(path)
        )
    };
    Ok(children_from_git_paths(
        raw.into_iter(),
        prefix,
        sizes,
        urls,
    ))
}

/// Parse a `GET /projects/{id}/repository/tree` page (or several, already
/// concatenated).
pub fn entries_from_gitlab_tree(
    json: &serde_json::Value,
    prefix: &str,
) -> Result<Vec<CanalEntry>, String> {
    let items = json
        .as_array()
        .ok_or("network: GitLab tree listing was not an array")?;
    let raw: Vec<(&str, bool)> = items
        .iter()
        .filter_map(|item| {
            let path = item["path"].as_str()?;
            let dir = item["type"].as_str() == Some("tree");
            Some((path, dir))
        })
        .collect();
    let urls =
        |path: &str| -> String { format!("https://gitlab.com/-/blob/{}", encode_path(path)) };
    Ok(children_from_git_paths(
        raw.into_iter(),
        prefix,
        |_| None,
        urls,
    ))
}

/// Parse a Drive `files.list` page (folders kept — unlike the native
/// listing, the VFS needs them to descend).
pub fn entries_from_drive_page(json: &serde_json::Value, prefix: &str) -> Vec<CanalEntry> {
    let wanted = wanted_prefix(prefix);
    let empty: Vec<serde_json::Value> = Vec::new();
    let files = json["files"].as_array().unwrap_or(&empty);
    files
        .iter()
        .filter_map(|item| {
            let name = item["name"].as_str()?;
            let id = item["id"].as_str()?;
            let is_dir = item["mimeType"].as_str() == Some(DRIVE_FOLDER_MIME);
            Some(CanalEntry {
                name: name.to_string(),
                path: format!("{wanted}{name}"),
                locator: id.to_string(),
                is_dir,
                size_bytes: drive_size(item),
                modified_at: item["modifiedTime"].as_str().unwrap_or("").to_string(),
                url: format!("https://drive.google.com/file/d/{id}/view"),
            })
        })
        .collect()
}

/// Drive reports `size` as a JSON *string* (`"4096"`), unlike the Git
/// APIs, which use numbers — parse both or every row shows 0 bytes.
fn drive_size(item: &serde_json::Value) -> u64 {
    item["size"]
        .as_u64()
        .or_else(|| item["size"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

// ─── status classification (shared with the transport) ───────────────

/// Map a non-2xx answer onto the house error prefixes. `Ok(())` means 2xx.
pub fn status_error(provider: &str, op: &str, status: u16, body: &[u8]) -> Result<(), String> {
    if (200..300).contains(&status) {
        return Ok(());
    }
    let detail = body_error_detail(body);
    let prefix = match status {
        401 | 403 => "auth",
        404 => "not_found",
        413 => "too_large",
        429 | 503 => "rate_limited",
        _ => "network",
    };
    if detail.is_empty() {
        return Err(format!(
            "{prefix}: {provider} {op} failed with HTTP {status}"
        ));
    }
    Err(format!(
        "{prefix}: {provider} {op} failed with HTTP {status} — {detail}"
    ))
}

/// Best-effort message out of an error body: the JSON `message`/`error`
/// fields when present, otherwise the first line of the raw text.
pub fn body_error_detail(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Some(message) = value["message"].as_str() {
            return message.chars().take(200).collect();
        }
        if let Some(message) = value["error"].as_str() {
            return message.chars().take(200).collect();
        }
    }
    text.lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(200)
        .collect()
}

// ════════════════════════════════════════════════════════════════════
// Browser transport — everything below needs `web_sys::fetch`, so it only
// exists for wasm32 (same gating as `agent.rs`). The pure code above is
// covered by native `cargo test` in CI.
// ════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
async fn fetch_raw(url: &str, headers: &[(&str, &str)]) -> Result<(u16, Vec<u8>), String> {
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Request, RequestInit, Response};

    let opts = RequestInit::new();
    opts.set_method("GET");
    let request = Request::new_with_str_and_init(url, &opts)
        .map_err(|_| format!("invalid: malformed provider url {url}"))?;
    for (name, value) in headers {
        request
            .headers()
            .set(name, value)
            .map_err(|e| format!("invalid: cannot set header ({e:?})"))?;
    }
    // `window()` is `None` inside `db-worker` — a Dedicated Worker has no
    // window, only a WorkerGlobalScope, and both expose `fetch`.
    // web-sys binds WorkerGlobalScope from JsValue, not js_sys::Object.
    let promise = match web_sys::window() {
        Some(window) => window.fetch_with_request(&request),
        None => {
            let global: JsValue = js_sys::global().into();
            let scope: web_sys::WorkerGlobalScope = global.try_into().map_err(|_| {
                "unsupported: this realm has no fetch() global (not a browser)".to_string()
            })?;
            scope.fetch_with_request(&request)
        }
    };
    let resp_value = JsFuture::from(promise).await.map_err(|_| {
        format!(
            "cors: {url} refused the browser request — CORS blocked it (the provider sent no \
             Access-Control-Allow-Origin, or preflight rejected the headers)"
        )
    })?;
    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| "network: provider reply was not an HTTP response".to_string())?;
    let status = resp.status();
    let buffer = resp
        .array_buffer()
        .map_err(|_| "network: could not read the provider reply".to_string())?;
    let buffer = JsFuture::from(buffer)
        .await
        .map_err(|_| "network: could not read the provider reply".to_string())?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    Ok((status, bytes))
}

#[cfg(target_arch = "wasm32")]
async fn get_bytes(
    provider: &str,
    op: &str,
    url: &str,
    headers: &[(&str, &str)],
) -> Result<Vec<u8>, String> {
    let (status, body) = fetch_raw(url, headers).await?;
    status_error(provider, op, status, &body)?;
    Ok(body)
}

#[cfg(target_arch = "wasm32")]
async fn get_json(
    provider: &str,
    op: &str,
    url: &str,
    headers: &[(&str, &str)],
) -> Result<serde_json::Value, String> {
    let body = get_bytes(provider, op, url, headers).await?;
    serde_json::from_slice(&body)
        .map_err(|_| format!("network: {provider} {op} answer was not JSON"))
}

/// `("Authorization", …)` + `("Accept", …)` for the GitHub API. The keys
/// are `'static` literals so the tuple can be borrowed as `&[(&str, &str)]`
/// without lifetime plumbing at each call site.
#[cfg(target_arch = "wasm32")]
fn github_headers(token: &str, accept: &str) -> [(&'static str, String); 2] {
    [
        ("Authorization", format!("token {token}")),
        ("Accept", accept.to_string()),
    ]
}

// ─── provider operations ─────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
async fn probe_backend(cfg: &CanalConfig) -> Result<serde_json::Value, String> {
    ensure_supported(cfg)?;
    ensure_token(cfg)?;
    match cfg.backend().as_str() {
        "github" => {
            let headers = github_headers(cfg.token(), "application/vnd.github+json");
            let refs: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let url = "https://api.github.com/user";
            let json = get_json("GitHub", "probe", url, &refs).await?;
            let who = json["login"].as_str().unwrap_or("");
            Ok(serde_json::json!({ "backend": "github", "who": who }))
        }
        "gitlab" => {
            let base = cfg.gitlab_base();
            let url = if cfg.repo().is_empty() {
                format!("{base}/api/v4/user")
            } else {
                format!("{base}/api/v4/projects/{}", urlencode(cfg.repo()))
            };
            let headers = [("PRIVATE-TOKEN", cfg.token())];
            let json = get_json("GitLab", "probe", &url, &headers).await?;
            let who = json["username"]
                .as_str()
                .or_else(|| json["path_with_namespace"].as_str());
            Ok(serde_json::json!({ "backend": "gitlab", "who": who.unwrap_or("") }))
        }
        "googleDrive" => {
            let headers = [("Authorization", cfg.token())];
            let url = "https://www.googleapis.com/drive/v3/about?fields=user";
            let json = get_json("Google Drive", "probe", url, &headers).await?;
            let who = json["user"]["emailAddress"].as_str().unwrap_or("");
            Ok(serde_json::json!({ "backend": "googleDrive", "who": who }))
        }
        other => Err(refusal(other)),
    }
}

#[cfg(target_arch = "wasm32")]
async fn list_backend(cfg: &CanalConfig, prefix: &str) -> Result<Vec<CanalEntry>, String> {
    ensure_supported(cfg)?;
    ensure_token(cfg)?;
    let full = cfg.full_prefix(prefix);
    match cfg.backend().as_str() {
        "github" => {
            let (owner, repo) = parse_repo(cfg.repo())?;
            let branch = cfg.branch_or_main();
            let url = format!(
                "https://api.github.com/repos/{owner}/{repo}/git/trees/{}?recursive=1",
                urlencode(&branch)
            );
            let headers = github_headers(cfg.token(), "application/vnd.github+json");
            let refs: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let json = get_json("GitHub", "list", &url, &refs).await?;
            entries_from_github_tree(&json, &full, &owner, &repo, &branch)
        }
        "gitlab" => {
            let base = cfg.gitlab_base();
            let branch = cfg.branch_or_main();
            let mut merged = serde_json::json!([]);
            for page in 1..=MAX_LISTING_PAGES {
                let mut url = format!(
                    "{base}/api/v4/projects/{}/repository/tree?ref={}&recursive=true\
                     &per_page=100&page={page}",
                    urlencode(cfg.repo()),
                    urlencode(&branch)
                );
                let clean = full.trim().trim_matches('/');
                if !clean.is_empty() {
                    url.push_str(&format!("&path={}", urlencode(clean)));
                }
                let headers = [("PRIVATE-TOKEN", cfg.token())];
                let json = get_json("GitLab", "list", &url, &headers).await?;
                let count = json.as_array().map(Vec::len).unwrap_or(0);
                if let Some(target) = merged.as_array_mut() {
                    if let Some(items) = json.as_array() {
                        target.extend(items.iter().cloned());
                    }
                }
                if count < 100 {
                    break;
                }
            }
            entries_from_gitlab_tree(&merged, &full)
        }
        "googleDrive" => {
            let parent = drive_parent_id(cfg, &full).await?;
            let mut page_token = String::new();
            let mut merged = serde_json::json!({ "files": [] });
            for _ in 0..MAX_LISTING_PAGES {
                let query = format!("'{parent}' in parents and trashed=false");
                let mut url = format!(
                    "{DRIVE_FILES_URL}?q={}&fields=files(id,name,size,modifiedTime,\
                     mimeType),nextPageToken&pageSize=1000",
                    urlencode(&query)
                );
                if !page_token.is_empty() {
                    url.push_str(&format!("&pageToken={}", urlencode(&page_token)));
                }
                let headers = [("Authorization", cfg.token())];
                let json = get_json("Google Drive", "list", &url, &headers).await?;
                let page = json["files"].as_array();
                if let (Some(all), Some(page)) = (merged["files"].as_array_mut(), page) {
                    all.extend(page.iter().cloned());
                }
                match json["nextPageToken"].as_str() {
                    Some(token) => page_token = token.to_string(),
                    None => break,
                }
            }
            Ok(entries_from_drive_page(&merged, &full))
        }
        other => Err(refusal(other)),
    }
}

/// Walk the prefix segment by segment to the Drive folder id that holds
/// it — Drive has no paths, only parent ids (`backends.rs::folder_chain`).
#[cfg(target_arch = "wasm32")]
async fn drive_parent_id(cfg: &CanalConfig, prefix: &str) -> Result<String, String> {
    let mut parent = match cfg.folder_id.as_deref().map(str::trim) {
        Some(folder) if !folder.is_empty() => folder.to_string(),
        _ => "root".to_string(),
    };
    let segments: Vec<&str> = prefix
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect();
    for segment in segments {
        let escaped = segment.replace('\'', "''");
        let query = format!(
            "name='{escaped}' and '{parent}' in parents and mimeType='{DRIVE_FOLDER_MIME}' \
             and trashed=false"
        );
        let url = format!(
            "{DRIVE_FILES_URL}?q={}&fields=files(id)&pageSize=1",
            urlencode(&query)
        );
        let headers = [("Authorization", cfg.token())];
        let json = get_json("Google Drive", "folder lookup", &url, &headers).await?;
        parent = json["files"][0]["id"]
            .as_str()
            .ok_or_else(|| format!("not_found: no Drive folder '{segment}' under '{prefix}'"))?
            .to_string();
    }
    Ok(parent)
}

#[cfg(target_arch = "wasm32")]
async fn fetch_backend(cfg: &CanalConfig, locator: &str) -> Result<Vec<u8>, String> {
    ensure_supported(cfg)?;
    ensure_token(cfg)?;
    // `locator` is a path (Git) or file id (Drive) exactly as the listing
    // produced it — already absolute, so it is never folded again.
    let clean = locator.trim().trim_matches('/');
    if clean.is_empty() {
        return Err("unsupported: provider path must not be empty".to_string());
    }
    match cfg.backend().as_str() {
        "github" => {
            let (owner, repo) = parse_repo(cfg.repo())?;
            let branch = cfg.branch_or_main();
            let url = format!(
                "https://api.github.com/repos/{owner}/{repo}/contents/{}?ref={}",
                encode_path(clean),
                urlencode(&branch)
            );
            let headers = github_headers(cfg.token(), "application/vnd.github.raw");
            let refs: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
            get_bytes("GitHub", "fetch", &url, &refs).await
        }
        "gitlab" => {
            let base = cfg.gitlab_base();
            let branch = cfg.branch_or_main();
            let url = format!(
                "{base}/api/v4/projects/{}/repository/files/{}/raw?ref={}",
                urlencode(cfg.repo()),
                urlencode(clean),
                urlencode(&branch)
            );
            let headers = [("PRIVATE-TOKEN", cfg.token())];
            get_bytes("GitLab", "fetch", &url, &headers).await
        }
        "googleDrive" => {
            let url = format!("{DRIVE_FILES_URL}/{clean}?alt=media");
            let headers = [("Authorization", cfg.token())];
            get_bytes("Google Drive", "fetch", &url, &headers).await
        }
        other => Err(refusal(other)),
    }
}

// ─── wasm exports ────────────────────────────────────────────────────

/// JSON in, envelope out — the same contract as `db_dispatch`.
///
/// Ops: `probe` → `{backend, who}` · `list` → `CanalEntry[]`.
/// `args_json` = `{"config": <sync config>, "prefix": "<relative path>"}`.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn canal_dispatch(op: &str, args_json: &str) -> String {
    match canal_dispatch_inner(op, args_json).await {
        Ok(data) => crate::db::envelope_json(true, &data, None),
        Err(error) => crate::db::envelope_json(false, &serde_json::Value::Null, Some(error)),
    }
}

#[cfg(target_arch = "wasm32")]
async fn canal_dispatch_inner(op: &str, args_json: &str) -> Result<serde_json::Value, String> {
    let args: serde_json::Value =
        serde_json::from_str(args_json).map_err(|e| format!("invalid: args are not JSON ({e})"))?;
    let cfg = config_from(&args)?;
    let prefix = args
        .get("prefix")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    match op {
        "probe" => probe_backend(&cfg).await,
        "list" => {
            let entries = list_backend(&cfg, &prefix).await?;
            serde_json::to_value(&entries)
                .map_err(|e| format!("network: cannot encode listing ({e})"))
        }
        other => Err(format!("unsupported: unknown canal op '{other}'")),
    }
}

#[cfg(target_arch = "wasm32")]
fn config_from(args: &serde_json::Value) -> Result<CanalConfig, String> {
    let raw = args
        .get("config")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    serde_json::from_value(raw).map_err(|e| format!("invalid: bad canal config ({e})"))
}

/// Fetch one file's bytes — `Uint8Array` out, because a JSON envelope
/// would base64 the payload (33 % bigger and a huge string on the JS
/// heap).
///
/// Errors reject the promise with the same house-prefixed strings the
/// envelope carries, so the caller renders them identically.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn canal_fetch(config_json: &str, locator: &str) -> Result<js_sys::Uint8Array, JsValue> {
    let cfg: CanalConfig = serde_json::from_str(config_json)
        .map_err(|e| JsValue::from_str(&format!("invalid: bad canal config ({e})")))?;
    let bytes = fetch_backend(&cfg, locator)
        .await
        .map_err(|e| JsValue::from_str(&e))?;
    let out = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    out.copy_from(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backends_gate_cors_ok_only() {
        assert!(backend_supported("github"));
        assert!(backend_supported("gitLab"));
        assert!(backend_supported("GoogleDrive"));
        assert!(!backend_supported("telegram"));
        assert!(!backend_supported("googlePhotos"));
        assert!(refusal("telegram").starts_with("unsupported:"));
        assert!(refusal("GooglePhotos").starts_with("unsupported:"));
        assert!(refusal("local").starts_with("unsupported:"));
        assert!(refusal("s3").starts_with("unsupported:"));
    }

    #[test]
    fn config_tolerates_sync_config_shapes() {
        // A raw `SyncConfig` uses `backendType`, may omit the token
        // (`skip_serializing`) and carries nulls for unused fields.
        let cfg: CanalConfig = serde_json::from_value(serde_json::json!({
            "backendType": "gitHub",
            "repoName": "owner/repo",
            "branch": "main",
            "basePath": null,
            "folderId": null
        }))
        .expect("must parse");
        assert_eq!(cfg.backend(), "github");
        assert_eq!(cfg.repo(), "owner/repo");
        assert_eq!(cfg.branch_or_main(), "main");
        assert_eq!(cfg.gitlab_base(), "https://gitlab.com");
        assert!(ensure_token(&cfg).is_err());
    }

    #[test]
    fn missing_token_is_an_auth_error() {
        let cfg = CanalConfig {
            backend: Some("github".to_string()),
            ..CanalConfig::default()
        };
        let err = ensure_token(&cfg).expect_err("must refuse");
        assert!(err.starts_with("auth:"), "house prefix, got {err}");
        assert!(ensure_supported(&cfg).is_ok());
    }

    #[test]
    fn base_path_folds_into_the_prefix() {
        let cfg = CanalConfig {
            base_path: Some("/backups/2026/".to_string()),
            ..CanalConfig::default()
        };
        assert_eq!(cfg.full_prefix(""), "backups/2026");
        assert_eq!(cfg.full_prefix("/x/y/"), "backups/2026/x/y");
        let root = CanalConfig::default();
        assert_eq!(root.full_prefix("/x/"), "x");
        assert_eq!(root.full_prefix(""), "");
    }

    #[test]
    fn url_helpers_match_the_native_backend() {
        assert_eq!(urlencode("a b/c"), "a%20b%2Fc");
        assert_eq!(encode_path("dir/file name.txt"), "dir/file%20name.txt");
        let (owner, repo) = parse_repo("owner/repo").expect("ok");
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
        assert!(parse_repo("noslash").is_err());
    }

    #[test]
    fn status_errors_use_house_prefixes() {
        assert!(status_error("GitHub", "list", 200, b"").is_ok());
        let err = status_error("GitHub", "list", 401, b"").expect_err("401");
        assert!(err.starts_with("auth:"), "house prefix, got {err}");
        let err = status_error("GitHub", "list", 404, b"").expect_err("404");
        assert!(err.starts_with("not_found:"), "house prefix, got {err}");
        let err = status_error("GitLab", "list", 429, b"").expect_err("429");
        assert!(err.starts_with("rate_limited:"), "house prefix, got {err}");
        let body = br#"{"message":"boom"}"#;
        let err = status_error("Google Drive", "fetch", 500, body).expect_err("500");
        assert!(err.starts_with("network:"), "house prefix, got {err}");
        assert!(err.contains("boom"));
    }

    #[test]
    fn github_tree_becomes_one_level_of_children() {
        let json = serde_json::json!({
            "truncated": false,
            "tree": [
                { "path": "docs/a.cyb3", "type": "blob", "size": 12 },
                { "path": "docs/nested/b.cyb3", "type": "blob", "size": 34 },
                { "path": "README.md", "type": "blob", "size": 5 },
                { "path": "src", "type": "tree" }
            ]
        });
        let root = entries_from_github_tree(&json, "", "o", "r", "main").expect("root");
        let names: Vec<&str> = root.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["docs", "src", "README.md"]);
        assert!(root[0].is_dir && root[1].is_dir && !root[2].is_dir);

        let docs = entries_from_github_tree(&json, "docs", "o", "r", "main").expect("docs");
        let names: Vec<&str> = docs.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["nested", "a.cyb3"]);
        assert_eq!(docs[1].size_bytes, 12);
        assert_eq!(docs[1].path, "docs/a.cyb3");
    }

    #[test]
    fn github_truncated_tree_is_refused_honestly() {
        let json = serde_json::json!({ "truncated": true, "tree": [] });
        let err = entries_from_github_tree(&json, "", "o", "r", "main").expect_err("refuse");
        assert!(err.starts_with("too_large:"), "house prefix, got {err}");
    }

    #[test]
    fn gitlab_tree_keeps_folders() {
        let json = serde_json::json!([
            { "type": "tree", "name": "assets", "path": "assets" },
            { "type": "blob", "name": "a.cyb3", "path": "assets/a.cyb3" }
        ]);
        // One level per listing, like the GitHub tree: the root shows the
        // synthesised `assets` dir, the nested blob only under `assets/`.
        let root = entries_from_gitlab_tree(&json, "").expect("parse");
        assert_eq!(root.len(), 1);
        assert!(root[0].is_dir);
        assert_eq!(root[0].path, "assets");
        let inner = entries_from_gitlab_tree(&json, "assets").expect("parse");
        assert_eq!(inner.len(), 1);
        assert!(!inner[0].is_dir);
        assert_eq!(inner[0].locator, "assets/a.cyb3");
    }

    #[test]
    fn drive_page_keeps_folders_and_ids() {
        let json = serde_json::json!({
            "files": [
                {
                    "id": "folder-1",
                    "name": "vaults",
                    "mimeType": DRIVE_FOLDER_MIME
                },
                {
                    "id": "file-1",
                    "name": "vault.cybermanju",
                    "mimeType": "application/octet-stream",
                    "size": "4096",
                    "modifiedTime": "2026-01-01T00:00:00Z"
                }
            ]
        });
        let entries = entries_from_drive_page(&json, "backups");
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].path, "backups/vaults");
        assert!(!entries[1].is_dir);
        assert_eq!(entries[1].locator, "file-1");
        assert_eq!(entries[1].size_bytes, 4096);
    }

    #[test]
    fn error_detail_prefers_the_json_message() {
        let body = br#"{"message":"Bad credentials"}"#;
        assert_eq!(body_error_detail(body), "Bad credentials");
        assert_eq!(body_error_detail(b"plain text\nsecond line"), "plain text");
    }
}
