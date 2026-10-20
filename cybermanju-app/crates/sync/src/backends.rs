// CyberManju OS — Storage Sync Backends
// Four backends: Local, GitHub, GitLab, Google Drive.
// All HTTP goes through the shared client + `send_classified`/`send_probe`
// wrappers, so every provider call gets retries, rate-limit gating and the
// error prefix contract (see retry.rs).
//
// Error contract: every `Err` starts with exactly one of `auth:`,
// `rate_limited:`, `not_found:`, `unsupported:`, `too_large:`, `integrity:`,
// `network:` followed by `: `. Failed downloads never leave a partial file
// behind. Missing targets on delete are `not_found:`; listing a directory
// that does not exist is an empty list (same as every hierarchical backend).

use crate::oauth::{self, OAuthCredentials};
use crate::rate_limit;
use crate::retry;
use crate::transfer;
use cybermanju_types::sync::*;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// Page/iteration cap for provider listings — a listing larger than this is
/// refused with `too_large:` instead of looping forever.
const MAX_PAGES: usize = 200;

// ===========================================================================
// Shared HTTP plumbing
// ===========================================================================

/// One shared `reqwest::blocking::Client` (connection pooling across calls).
pub(crate) fn http_client() -> Result<reqwest::blocking::Client, String> {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client.clone());
    }
    let built = reqwest::blocking::Client::builder()
        .user_agent("CyberManjuOS/0.1")
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|err| format!("{}: cannot build HTTP client: {}", retry::NETWORK, err))?;
    Ok(CLIENT.get_or_init(|| built).clone())
}

fn body_looks_rate_limited(status: u16, body: &str) -> bool {
    status == 403
        && (body.contains("rateLimitExceeded")
            || body.contains("userRateLimitExceeded")
            || body.contains("RATE_LIMIT_EXCEEDED"))
}

fn truncate_str(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

/// Build the request once per attempt, send it, classify non-success.
///
/// `allow` statuses bypass classification (existence probes need `404`,
/// chunked uploads need `308`). `probe_format` renders
/// `<provider> <op> failed (HTTP <status>)` so connection tests can report
/// the raw status.
fn send_once(
    provider: &str,
    op: &str,
    allow: &[u16],
    probe_format: bool,
    build: &mut impl FnMut() -> Result<reqwest::blocking::RequestBuilder, String>,
) -> Result<reqwest::blocking::Response, String> {
    let request = match build() {
        Ok(request) => request,
        Err(err) => {
            return Err(
                if retry::classify(&err) == retry::ErrorClass::Unclassified {
                    format!(
                        "{}: {} {} could not be prepared: {}",
                        retry::NETWORK,
                        provider,
                        op,
                        err
                    )
                } else {
                    err
                },
            );
        }
    };
    let resp = request.send().map_err(|err| {
        // without_url(): some providers embed secrets in the URL, so URLs
        // must never surface in error strings.
        let err = err.without_url();
        if err.is_builder() {
            format!(
                "{}: {} {} request is malformed: {}",
                retry::UNSUPPORTED,
                provider,
                op,
                err
            )
        } else {
            format!(
                "{}: {} {} request failed: {}",
                retry::NETWORK,
                provider,
                op,
                err
            )
        }
    })?;
    let status = resp.status().as_u16();
    if (200..300).contains(&status) || allow.contains(&status) {
        return Ok(resp);
    }
    let headers = resp.headers().clone();
    let retry_after = retry::retry_after_from_headers(&headers);
    let body = resp
        .text()
        .unwrap_or_else(|err| format!("<unreadable response body: {}>", err));
    let prefix =
        if retry::is_rate_limited(status, &headers) || body_looks_rate_limited(status, &body) {
            retry::RATE_LIMITED
        } else {
            retry::class_for_status(status).prefix()
        };
    if probe_format {
        let hint = match retry_after {
            Some(wait) => format!(" [retry_after={}s]", wait.as_secs()),
            None => String::new(),
        };
        Err(format!(
            "{}: {} {} failed (HTTP {}){}: {}",
            prefix,
            provider,
            op,
            status,
            hint,
            truncate_str(&body, 400)
        ))
    } else {
        Err(retry::http_error(
            prefix,
            provider,
            op,
            status,
            retry_after,
            &body,
        ))
    }
}

/// Send a classified request with retry/backoff. The closure returns a
/// `RequestBuilder` (or a pre-classified build error) and is re-invoked on
/// every attempt, so multi-part bodies and file reads stay fresh.
pub(crate) fn send_classified(
    provider: &str,
    op: &str,
    allow: &[u16],
    mut build: impl FnMut() -> Result<reqwest::blocking::RequestBuilder, String>,
) -> Result<reqwest::blocking::Response, String> {
    retry::with_retry(&retry::RetryPolicy::default(), || {
        send_once(provider, op, allow, false, &mut build)
    })
}

/// Connection probe: same plumbing, `(HTTP <status>)` in the message.
pub(crate) fn send_probe(
    provider: &str,
    mut build: impl FnMut() -> Result<reqwest::blocking::RequestBuilder, String>,
) -> Result<reqwest::blocking::Response, String> {
    retry::with_retry(&retry::RetryPolicy::default(), || {
        send_once(provider, "connection test", &[], true, &mut build)
    })
}

/// Percent-encode everything outside the RFC 3986 unreserved set.
/// Used for query values and single path segments (kept `pub(crate)` for
/// quota.rs).
pub(crate) fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            b => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Percent-decode (byte-safe: never slices the string at non-ASCII offsets).
fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match (hex_value(bytes[i + 1]), hex_value(bytes[i + 2]))
            {
                (Some(hi), Some(lo)) => {
                    out.push((hi << 4) | lo);
                    i += 3;
                }
                _ => {
                    out.push(bytes[i]);
                    i += 1;
                }
            },
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Percent-encode a path but keep `/` separators.
fn encode_path(path: &str) -> String {
    path.split('/')
        .map(urlencoding)
        .collect::<Vec<_>>()
        .join("/")
}

fn file_name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "upload".to_string())
}

/// The remote path a backend should store: trimmed, no surrounding slashes,
/// and a local file name when the caller gave nothing.
fn normalize_remote(remote: &str, local_path: &str) -> String {
    let clean = remote.trim().trim_matches('/');
    if clean.is_empty() {
        file_name_of(local_path)
    } else {
        clean.to_string()
    }
}

fn json_u64(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}

fn parse_json(
    resp: reqwest::blocking::Response,
    provider: &str,
    op: &str,
) -> Result<serde_json::Value, String> {
    let body = resp.text().map_err(|err| {
        format!(
            "{}: {} {} response unreadable: {}",
            retry::NETWORK,
            provider,
            op,
            err
        )
    })?;
    serde_json::from_str(&body).map_err(|err| {
        format!(
            "{}: {} {} response unparseable: {}",
            retry::NETWORK,
            provider,
            op,
            err
        )
    })
}

// ===========================================================================
// OAuth token source (per-request refresh for GitHub/GitLab/Drive)
// ===========================================================================

struct TokenSource {
    credentials: Mutex<Option<OAuthCredentials>>,
    /// Raw token used when there are no OAuth credentials (PAT configs) or a
    /// refresh fails but a raw token is still stored.
    fallback: String,
    token_url: String,
}

impl TokenSource {
    fn new(credentials: Option<OAuthCredentials>, fallback: String, token_url: String) -> Self {
        Self {
            credentials: Mutex::new(credentials),
            fallback,
            token_url,
        }
    }

    fn get(&self) -> Result<String, String> {
        let mut guard = self
            .credentials
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(credentials) = guard.as_mut() {
            match oauth::get_valid_token(credentials, &self.token_url, 300) {
                Ok(token) if !token.is_empty() => return Ok(token),
                Ok(_) => {}
                Err(err) => {
                    if self.fallback.is_empty() {
                        return Err(
                            if retry::classify(&err) == retry::ErrorClass::Unclassified {
                                format!("{}: token refresh failed: {}", retry::AUTH, err)
                            } else {
                                err
                            },
                        );
                    }
                    warn!(
                        "OAuth refresh failed, using the stored raw token instead: {}",
                        err
                    );
                }
            }
        }
        if self.fallback.is_empty() {
            return Err(format!("{}: no access token for this backend", retry::AUTH));
        }
        Ok(self.fallback.clone())
    }

    fn bearer(&self) -> Result<String, String> {
        Ok(format!("Bearer {}", self.get()?))
    }
}

fn token_url_for(slug: &str) -> String {
    oauth::provider_endpoints(slug)
        .map(|endpoints| endpoints.token_url.to_string())
        .unwrap_or_default()
}

// ===========================================================================
// Local path helpers
// ===========================================================================

fn parse_repo(repo_name: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = repo_name.trim_start_matches('/').splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(format!(
            "{}: repo name '{}' is invalid — expected 'owner/repo'",
            retry::UNSUPPORTED,
            repo_name
        ));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

fn canonical_base(base: &str) -> Result<std::path::PathBuf, String> {
    std::path::Path::new(base).canonicalize().map_err(|err| {
        format!(
            "{}: cannot resolve base path '{}': {}",
            retry::NETWORK,
            base,
            err
        )
    })
}

/// Paths the sync engine must never upload: the vault container itself
/// (`*.cybermanju`), in-flight `cybermanju-up-*.cyb3` temps, and the local
/// secret sidecars. Quiet skip at the pipeline layer, hard refusal here —
/// the vault changes hash on every save, so syncing it would loop forever.
pub fn is_protected_remote_path(remote: &str) -> bool {
    let lower = remote.trim().replace('\\', "/").to_lowercase();
    if lower.is_empty() {
        return false;
    }
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    if base.ends_with(".cybermanju") {
        return true;
    }
    if base.starts_with("cybermanju-up-") && base.ends_with(".cyb3") {
        return true;
    }
    if base == "master.passphrase" || base == "keystore.json" {
        return true;
    }
    lower.contains("/.cybermanju/")
}

#[cfg(test)]
mod protected_path_tests {
    use super::is_protected_remote_path;

    #[test]
    fn vault_and_secrets_are_protected() {
        assert!(is_protected_remote_path("vault.cybermanju"));
        assert!(is_protected_remote_path("cybermanju-disks/d.cybermanju"));
        assert!(is_protected_remote_path("cybermanju-up-1-2.cyb3"));
        assert!(is_protected_remote_path("master.passphrase"));
        assert!(!is_protected_remote_path("notes.txt"));
        assert!(!is_protected_remote_path("notes.txt.cyb3"));
    }
}

/// Join a remote path onto a canonical base without ever escaping it.
///
/// Rejects `..`/absolute/NUL-style escapes lexically, then canonicalizes:
/// an existing target must resolve inside the base (defeats symlinks), and a
/// not-yet-existing upload target is validated through its nearest existing
/// ancestor, so writing `notes/note.txt` into an empty base works while a
/// symlinked directory pointing outside the base does not.
fn safe_join(base: &str, remote: &str) -> Result<String, String> {
    let base_path = canonical_base(base)?;
    let clean = remote.trim().trim_matches('/');
    if !clean.is_empty()
        && (std::path::Path::new(clean).is_absolute()
            || clean.split('/').any(|segment| segment == ".."))
    {
        return Err(format!(
            "{}: remote path '{}' escapes the base directory",
            retry::UNSUPPORTED,
            remote
        ));
    }
    let joined = base_path.join(clean);

    if joined.exists() || joined.is_symlink() {
        let real = joined.canonicalize().map_err(|err| {
            format!(
                "{}: cannot resolve '{}': {}",
                retry::NETWORK,
                joined.display(),
                err
            )
        })?;
        if !real.starts_with(&base_path) {
            return Err(format!(
                "{}: '{}' resolves outside the base directory",
                retry::UNSUPPORTED,
                remote
            ));
        }
        return Ok(real.to_string_lossy().to_string());
    }

    let mut ancestor: Option<&Path> = joined.parent();
    while let Some(candidate) = ancestor {
        if candidate.exists() {
            break;
        }
        ancestor = candidate.parent();
    }
    let ancestor = ancestor.ok_or_else(|| {
        format!(
            "{}: no existing parent directory for '{}'",
            retry::NETWORK,
            remote
        )
    })?;
    let real_ancestor = ancestor.canonicalize().map_err(|err| {
        format!(
            "{}: cannot resolve '{}': {}",
            retry::NETWORK,
            ancestor.display(),
            err
        )
    })?;
    if !real_ancestor.starts_with(&base_path) {
        return Err(format!(
            "{}: '{}' resolves outside the base directory",
            retry::UNSUPPORTED,
            remote
        ));
    }
    Ok(joined.to_string_lossy().to_string())
}

// ===========================================================================
// 1. LocalBackend
// ===========================================================================

pub struct LocalBackend {
    base_path: String,
}

impl LocalBackend {
    pub fn new(base_path: &str) -> Self {
        Self {
            base_path: base_path.to_string(),
        }
    }

    fn modified_rfc3339(metadata: &fs::Metadata) -> String {
        metadata
            .modified()
            .ok()
            .map(|time| {
                chrono::DateTime::<chrono::Utc>::from(time)
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string()
            })
            .unwrap_or_default()
    }

    fn copy_verified(&self, source: &str, dest: &str, remote: &str) -> Result<(), String> {
        if let Some(parent) = Path::new(dest).parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "{}: cannot create directory '{}': {}",
                    retry::NETWORK,
                    parent.display(),
                    err
                )
            })?;
        }
        fs::copy(source, dest).map_err(|err| {
            format!(
                "{}: cannot copy '{}' to '{}': {}",
                retry::NETWORK,
                source,
                dest,
                err
            )
        })?;
        let expected = transfer::hash_file(source)?;
        if let Err(err) = transfer::verify_file_blake3(dest, &expected) {
            let _ = fs::remove_file(dest);
            return Err(format!("{} (copy of '{}')", err, remote));
        }
        Ok(())
    }
}

impl StorageBackend for LocalBackend {
    fn name(&self) -> &str {
        "Local Storage"
    }

    fn backend_type(&self) -> SyncBackendType {
        SyncBackendType::Local
    }

    fn upload_file(&self, local_path: &str, remote_path: &str) -> Result<String, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::Local)?;
        if is_protected_remote_path(local_path) || is_protected_remote_path(remote_path) {
            return Err(format!(
                "integrity: '{}' is a vault/secret path and is never synced",
                remote_path
            ));
        }
        transfer::local_size(local_path)?;
        let dest = safe_join(&self.base_path, remote_path)?;
        self.copy_verified(local_path, &dest, remote_path)?;
        Ok(dest)
    }

    fn download_file(&self, remote_path: &str, local_path: &str) -> Result<(), String> {
        let _permit = rate_limit::acquire(&SyncBackendType::Local)?;
        let source = safe_join(&self.base_path, remote_path)?;
        if !Path::new(&source).is_file() {
            return Err(format!(
                "{}: '{}' does not exist in the local base path",
                retry::NOT_FOUND,
                remote_path
            ));
        }
        self.copy_verified(&source, local_path, remote_path)
    }

    fn delete_file(&self, remote_path: &str) -> Result<(), String> {
        let _permit = rate_limit::acquire(&SyncBackendType::Local)?;
        let path = safe_join(&self.base_path, remote_path)?;
        if !Path::new(&path).exists() {
            return Err(format!(
                "{}: '{}' does not exist in the local base path",
                retry::NOT_FOUND,
                remote_path
            ));
        }
        fs::remove_file(&path).map_err(|err| {
            format!(
                "{}: cannot delete '{}': {}",
                retry::NETWORK,
                remote_path,
                err
            )
        })
    }

    fn list_files(&self, prefix: &str) -> Result<Vec<RemoteFile>, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::Local)?;
        let dir = safe_join(&self.base_path, prefix)?;
        let target = Path::new(&dir);
        if !target.exists() || !target.is_dir() {
            return Ok(Vec::new());
        }
        let base = canonical_base(&self.base_path)?;
        let mut files = Vec::new();
        for entry in fs::read_dir(target).map_err(|err| {
            format!(
                "{}: cannot read directory '{}': {}",
                retry::NETWORK,
                dir,
                err
            )
        })? {
            let entry = entry.map_err(|err| {
                format!("{}: cannot read a directory entry: {}", retry::NETWORK, err)
            })?;
            let path = entry.path();
            if path.is_symlink() {
                match path.canonicalize() {
                    Ok(real) if real.starts_with(&base) => {}
                    _ => continue,
                }
            }
            if !path.is_file() {
                continue;
            }
            let metadata = entry.metadata().ok();
            let relative = path
                .strip_prefix(&base)
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default();
            files.push(RemoteFile {
                name: path
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_default(),
                path: relative,
                size_bytes: metadata.as_ref().map(|meta| meta.len()).unwrap_or(0),
                modified_at: metadata
                    .as_ref()
                    .map(Self::modified_rfc3339)
                    .unwrap_or_default(),
                url: path.to_string_lossy().to_string(),
            });
        }
        Ok(files)
    }

    fn get_file_url(&self, remote_path: &str) -> Result<String, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::Local)?;
        safe_join(&self.base_path, remote_path)
    }

    fn test_connection(&self) -> Result<bool, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::Local)?;
        let path = Path::new(&self.base_path);
        if !path.exists() {
            fs::create_dir_all(path).map_err(|err| {
                format!(
                    "{}: cannot create base path '{}': {}",
                    retry::NETWORK,
                    self.base_path,
                    err
                )
            })?;
        }
        Ok(true)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            max_size_bytes: transfer::max_size_bytes(&SyncBackendType::Local),
            supports_delete: true,
            supports_direct_download: true,
            recursive_list: false,
            chunked: false,
            // <<< AGENT-8 COMPUTE: a local disk absorbs as much concurrent
            // work as the machine has cores. >>>
            compute: std::thread::available_parallelism().map_or(1, |n| n.get() as u32),
            // <<< /AGENT-8 COMPUTE >>>
        }
    }

    fn stat(&self, remote_path: &str) -> Result<Option<RemoteFile>, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::Local)?;
        let path = safe_join(&self.base_path, remote_path)?;
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(format!(
                    "{}: '{}' does not exist in the local base path",
                    retry::NOT_FOUND,
                    remote_path
                ));
            }
            Err(err) => {
                return Err(format!(
                    "{}: cannot stat '{}': {}",
                    retry::NETWORK,
                    remote_path,
                    err
                ));
            }
        };
        Ok(Some(RemoteFile {
            name: file_name_of(remote_path),
            path: remote_path.trim().trim_matches('/').to_string(),
            size_bytes: metadata.len(),
            modified_at: Self::modified_rfc3339(&metadata),
            url: path,
        }))
    }
}

// ===========================================================================
// 2. GitHubBackend
// ===========================================================================

enum GitHubLocator {
    /// Blob path in the repository tree (Contents API).
    Content(String),
    /// Release tag for an asset upload (deterministic per remote path).
    Release(String),
}

pub struct GitHubBackend {
    auth: TokenSource,
    repo_name: String,
    branch: String,
}

impl GitHubBackend {
    pub fn new(token: &str, repo_name: &str, branch: &str) -> Self {
        Self::with_credentials(token, repo_name, branch, None)
    }

    pub fn with_credentials(
        token: &str,
        repo_name: &str,
        branch: &str,
        credentials: Option<OAuthCredentials>,
    ) -> Self {
        Self {
            auth: TokenSource::new(credentials, token.to_string(), token_url_for("github")),
            repo_name: repo_name.to_string(),
            branch: branch.to_string(),
        }
    }

    fn contents_url(&self, owner: &str, repo: &str, path: &str) -> String {
        format!(
            "https://api.github.com/repos/{}/{}/contents/{}",
            owner,
            repo,
            encode_path(path)
        )
    }

    fn raw_url(&self, owner: &str, repo: &str, path: &str) -> String {
        format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            owner,
            repo,
            self.branch,
            encode_path(path)
        )
    }

    fn release_tag(remote_path: &str) -> String {
        let digest = transfer::blake3_hex(remote_path.as_bytes());
        format!("cybermanju-sync-{}", &digest[..16])
    }

    /// Accepts plain blob paths, our own raw/content URLs (from
    /// `upload_file`/`list_files`), release tag URLs (our own release
    /// uploads) — i.e. anything this backend itself can return.
    fn resolve_locator(&self, remote: &str) -> Result<GitHubLocator, String> {
        let trimmed = remote.trim().trim_matches('/');
        if trimmed.is_empty() {
            return Err(format!(
                "{}: GitHub path must not be empty",
                retry::UNSUPPORTED
            ));
        }
        let contents_prefix = format!(
            "https://api.github.com/repos/{}/{}/contents/",
            self.owner()?,
            self.repo()?
        );
        if let Some(rest) = trimmed.strip_prefix(&contents_prefix) {
            let clean = rest.split('?').next().unwrap_or(rest);
            return Ok(GitHubLocator::Content(urldecode(clean)));
        }
        let raw_prefix = format!(
            "https://raw.githubusercontent.com/{}/{}/",
            self.owner()?,
            self.repo()?
        );
        if let Some(rest) = trimmed.strip_prefix(&raw_prefix) {
            let path = match rest.find('/') {
                Some(idx) => &rest[idx + 1..],
                None => rest,
            };
            return Ok(GitHubLocator::Content(urldecode(path)));
        }
        let tag_prefix = format!(
            "https://github.com/{}/{}/releases/tag/",
            self.owner()?,
            self.repo()?
        );
        if let Some(tag) = trimmed.strip_prefix(&tag_prefix) {
            if !tag.is_empty() {
                return Ok(GitHubLocator::Release(urldecode(tag)));
            }
        }
        Ok(GitHubLocator::Content(trimmed.to_string()))
    }

    fn owner(&self) -> Result<String, String> {
        parse_repo(&self.repo_name).map(|(owner, _)| owner)
    }

    fn repo(&self) -> Result<String, String> {
        parse_repo(&self.repo_name).map(|(_, repo)| repo)
    }

    fn get_release(
        &self,
        client: &reqwest::blocking::Client,
        token: &str,
        owner: &str,
        repo: &str,
        tag: &str,
    ) -> Result<Option<serde_json::Value>, String> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/tags/{}",
            owner,
            repo,
            urlencoding(tag)
        );
        let resp = send_classified("GitHub", "release lookup", &[404], || {
            Ok(client
                .get(&url)
                .header("Authorization", format!("token {}", token))
                .header("Accept", "application/vnd.github+json"))
        })?;
        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        Ok(Some(parse_json(resp, "GitHub", "release lookup")?))
    }

    fn delete_asset_named(
        &self,
        client: &reqwest::blocking::Client,
        token: &str,
        owner: &str,
        repo: &str,
        release_id: u64,
        asset_name: &str,
    ) -> Result<(), String> {
        let list_url = format!(
            "https://api.github.com/repos/{}/{}/releases/{}/assets",
            owner, repo, release_id
        );
        let resp = send_classified("GitHub", "asset lookup", &[], || {
            Ok(client
                .get(&list_url)
                .header("Authorization", format!("token {}", token))
                .header("Accept", "application/vnd.github+json"))
        })?;
        let json = parse_json(resp, "GitHub", "asset lookup")?;
        let stale: Vec<u64> = json["assets"]
            .as_array()
            .map(|assets| {
                assets
                    .iter()
                    .filter(|asset| asset["name"].as_str() == Some(asset_name))
                    .filter_map(|asset| json_u64(&asset["id"]))
                    .collect()
            })
            .unwrap_or_default();
        for asset_id in stale {
            let url = format!(
                "https://api.github.com/repos/{}/{}/releases/assets/{}",
                owner, repo, asset_id
            );
            send_classified("GitHub", "asset delete", &[404], || {
                Ok(client
                    .delete(&url)
                    .header("Authorization", format!("token {}", token))
                    .header("Accept", "application/vnd.github+json"))
            })?;
        }
        Ok(())
    }

    fn delete_release(
        &self,
        client: &reqwest::blocking::Client,
        token: &str,
        owner: &str,
        repo: &str,
        release_id: u64,
    ) -> Result<(), String> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/{}",
            owner, repo, release_id
        );
        send_classified("GitHub", "release delete", &[404], || {
            Ok(client
                .delete(&url)
                .header("Authorization", format!("token {}", token))
                .header("Accept", "application/vnd.github+json"))
        })?;
        Ok(())
    }

    fn publish_release(
        &self,
        client: &reqwest::blocking::Client,
        token: &str,
        owner: &str,
        repo: &str,
        release_id: u64,
    ) -> Result<(), String> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/{}",
            owner, repo, release_id
        );
        let body = serde_json::json!({ "draft": false });
        send_classified("GitHub", "release publish", &[], || {
            Ok(client
                .patch(&url)
                .header("Authorization", format!("token {}", token))
                .header("Accept", "application/vnd.github+json")
                .json(&body))
        })?;
        Ok(())
    }

    /// Push the local file onto an existing release `upload_url`.
    /// Files up to 8 MB go as a single POST; larger files are chunked
    /// (CL:0 init, then `Content-Range` PATCHes) with stall detection.
    fn push_release_asset(
        &self,
        client: &reqwest::blocking::Client,
        token: &str,
        upload_url: &str,
        local_path: &str,
        asset_name: &str,
        size: u64,
    ) -> Result<(), String> {
        let asset_url = format!("{}?name={}", upload_url, urlencoding(asset_name));

        if size <= transfer::GITHUB_RELEASE_CHUNK {
            let resp = send_classified("GitHub", "asset upload", &[201, 200], || {
                let data = transfer::read_local(local_path)?;
                Ok(client
                    .post(&asset_url)
                    .header("Authorization", format!("token {}", token))
                    .header("Content-Type", "application/octet-stream")
                    .body(data))
            })?;
            let json = parse_json(resp, "GitHub", "asset upload")?;
            if let Some(stored) = json_u64(&json["size"]) {
                if stored != size {
                    return Err(format!(
                        "{}: GitHub stored {} bytes but '{}' has {}",
                        retry::INTEGRITY,
                        stored,
                        asset_name,
                        size
                    ));
                }
            }
            return Ok(());
        }

        // Chunked upload: establish the upload location with an empty POST.
        let init = send_classified("GitHub", "asset upload", &[201, 200], || {
            Ok(client
                .post(&asset_url)
                .header("Authorization", format!("token {}", token))
                .header("Content-Type", "application/octet-stream")
                .body(Vec::<u8>::new()))
        })?;
        let patch_url = init
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
            .unwrap_or_else(|| asset_url.clone());
        let _ = init.text();

        let mut file = fs::File::open(local_path).map_err(|err| {
            format!(
                "{}: cannot open '{}' for chunked upload: {}",
                retry::NETWORK,
                local_path,
                err
            )
        })?;
        let mut start: u64 = 0;
        let mut completed = false;
        while start < size {
            let mut chunk = Vec::new();
            (&mut file)
                .take(transfer::GITHUB_RELEASE_CHUNK)
                .read_to_end(&mut chunk)
                .map_err(|err| {
                    format!(
                        "{}: cannot read '{}' for chunked upload: {}",
                        retry::NETWORK,
                        local_path,
                        err
                    )
                })?;
            if chunk.is_empty() {
                return Err(format!(
                    "{}: '{}' shrank during upload at byte {} of {}",
                    retry::NETWORK,
                    local_path,
                    start,
                    size
                ));
            }
            let end = start + chunk.len() as u64 - 1;
            let next = start + chunk.len() as u64;
            let resp = send_classified("GitHub", "asset chunk", &[201, 200, 308], || {
                Ok(client
                    .patch(&patch_url)
                    .header("Authorization", format!("token {}", token))
                    .header("Content-Type", "application/octet-stream")
                    .header("Content-Range", format!("bytes {}-{}/{}", start, end, size))
                    .body(chunk.clone()))
            })?;
            let status = resp.status().as_u16();
            if (200..300).contains(&status) {
                let json = parse_json(resp, "GitHub", "asset chunk")?;
                if let Some(stored) = json_u64(&json["size"]) {
                    if stored != size {
                        return Err(format!(
                            "{}: GitHub stored {} bytes but the upload sent {}",
                            retry::INTEGRITY,
                            stored,
                            size
                        ));
                    }
                }
                if next < size {
                    return Err(format!(
                        "{}: GitHub finalized the asset at {} of {} bytes",
                        retry::INTEGRITY,
                        next,
                        size
                    ));
                }
                completed = true;
                break;
            }
            if next <= start {
                return Err(format!(
                    "{}: GitHub chunked upload stalled at byte {}",
                    retry::NETWORK,
                    start
                ));
            }
            start = next;
        }
        if !completed {
            return Err(format!(
                "{}: GitHub chunked upload did not complete for '{}'",
                retry::NETWORK,
                asset_name
            ));
        }
        Ok(())
    }

    fn upload_via_release(
        &self,
        local_path: &str,
        remote_path: &str,
        size: u64,
    ) -> Result<String, String> {
        let (owner, repo) = parse_repo(&self.repo_name)?;
        let token = self.auth.get()?;
        let client = http_client()?;
        let tag = Self::release_tag(remote_path);
        let asset_name = file_name_of(remote_path);

        // Deterministic tag per remote path: repeated syncs reuse (and
        // replace) one release instead of creating one per run.
        let existing = self.get_release(&client, &token, &owner, &repo, &tag)?;
        let (release_id, upload_url, owned) = match existing {
            Some(json) => {
                let id = json_u64(&json["id"]).ok_or_else(|| {
                    format!("{}: GitHub release '{}' has no id", retry::NETWORK, tag)
                })?;
                let upload_url = json["upload_url"]
                    .as_str()
                    .unwrap_or_default()
                    .replace("{?name,label}", "");
                if upload_url.is_empty() {
                    return Err(format!(
                        "{}: GitHub release '{}' has no upload_url",
                        retry::NETWORK,
                        tag
                    ));
                }
                self.delete_asset_named(&client, &token, &owner, &repo, id, &asset_name)?;
                (id, upload_url, false)
            }
            None => {
                let url = format!("https://api.github.com/repos/{}/{}/releases", owner, repo);
                let body = serde_json::json!({
                    "tag_name": tag,
                    "name": format!("Sync upload: {}", asset_name),
                    "body": format!("Uploaded via CyberManju OS sync: {}", remote_path),
                    "draft": true,
                    "prerelease": false,
                });
                let resp = send_classified("GitHub", "release create", &[201], || {
                    Ok(client
                        .post(&url)
                        .header("Authorization", format!("token {}", token))
                        .header("Accept", "application/vnd.github+json")
                        .json(&body))
                })?;
                let json = parse_json(resp, "GitHub", "release create")?;
                let id = json_u64(&json["id"]).ok_or_else(|| {
                    format!("{}: GitHub release create returned no id", retry::NETWORK)
                })?;
                let upload_url = json["upload_url"]
                    .as_str()
                    .unwrap_or_default()
                    .replace("{?name,label}", "");
                if upload_url.is_empty() {
                    return Err(format!(
                        "{}: GitHub release create returned no upload_url",
                        retry::NETWORK
                    ));
                }
                (id, upload_url, true)
            }
        };

        let result =
            self.push_release_asset(&client, &token, &upload_url, local_path, &asset_name, size);
        let published = match result {
            Ok(()) if owned => self.publish_release(&client, &token, &owner, &repo, release_id),
            other => other,
        };
        if let Err(err) = published {
            if owned {
                if let Err(cleanup_err) =
                    self.delete_release(&client, &token, &owner, &repo, release_id)
                {
                    warn!(
                        "release cleanup after a failed upload also failed: {}",
                        cleanup_err
                    );
                }
            } else if let Err(cleanup_err) =
                self.delete_asset_named(&client, &token, &owner, &repo, release_id, &asset_name)
            {
                warn!(
                    "asset cleanup after a failed upload also failed: {}",
                    cleanup_err
                );
            }
            return Err(err);
        }
        if owned {
            info!("published GitHub release {} for '{}'", tag, remote_path);
        }
        Ok(format!(
            "https://github.com/{}/{}/releases/tag/{}",
            owner, repo, tag
        ))
    }

    fn upload_inline(
        &self,
        local_path: &str,
        remote_path: &str,
        size: u64,
    ) -> Result<String, String> {
        let (owner, repo) = parse_repo(&self.repo_name)?;
        let token = self.auth.get()?;
        let client = http_client()?;
        let url = self.contents_url(&owner, &repo, remote_path);

        // The Contents API needs the current blob SHA to overwrite a file.
        let probe = send_classified("GitHub", "sha lookup", &[404], || {
            Ok(client
                .get(&url)
                .header("Authorization", format!("token {}", token))
                .header("Accept", "application/vnd.github+json")
                .query(&[("ref", self.branch.as_str())]))
        })?;
        let sha = if probe.status().as_u16() == 404 {
            None
        } else {
            let json = parse_json(probe, "GitHub", "sha lookup")?;
            Some(
                json["sha"]
                    .as_str()
                    .ok_or_else(|| {
                        format!(
                            "{}: GitHub returned no blob sha for '{}'",
                            retry::NETWORK,
                            remote_path
                        )
                    })?
                    .to_string(),
            )
        };

        let data = transfer::read_local(local_path)?;
        let mut body = serde_json::json!({
            "message": format!("Sync upload: {}", remote_path),
            "content": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data),
            "branch": self.branch,
        });
        if let Some(sha) = sha {
            body["sha"] = serde_json::json!(sha);
        }

        let resp = send_classified("GitHub", "upload", &[], || {
            Ok(client
                .put(&url)
                .header("Authorization", format!("token {}", token))
                .header("Accept", "application/vnd.github+json")
                .json(&body))
        })?;
        let json = parse_json(resp, "GitHub", "upload")?;
        if let Some(stored) = json_u64(&json["content"]["size"]) {
            if stored != size {
                return Err(format!(
                    "{}: GitHub stored {} bytes but '{}' has {}",
                    retry::INTEGRITY,
                    stored,
                    remote_path,
                    size
                ));
            }
        }
        Ok(json["content"]["download_url"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| self.raw_url(&owner, &repo, remote_path)))
    }

    fn last_commit_date(
        &self,
        client: &reqwest::blocking::Client,
        token: &str,
        path: &str,
    ) -> Option<String> {
        let (owner, repo) = self.owner().ok().zip(self.repo().ok())?;
        let url = format!(
            "https://api.github.com/repos/{}/{}/commits?path={}&per_page=1",
            owner,
            repo,
            urlencoding(path)
        );
        let resp = match send_classified("GitHub", "stat date", &[], || {
            Ok(client
                .get(&url)
                .header("Authorization", format!("token {}", token))
                .header("Accept", "application/vnd.github+json"))
        }) {
            Ok(resp) => resp,
            Err(err) => {
                debug!("GitHub: no commit date for '{}': {}", path, err);
                return None;
            }
        };
        let json = parse_json(resp, "GitHub", "stat date").ok()?;
        json[0]["commit"]["committer"]["date"]
            .as_str()
            .or_else(|| json[0]["commit"]["author"]["date"].as_str())
            .map(str::to_string)
    }

    fn download_release_asset(&self, tag: &str, local_path: &str) -> Result<(), String> {
        let (owner, repo) = parse_repo(&self.repo_name)?;
        let token = self.auth.get()?;
        let client = http_client()?;
        let release = self
            .get_release(&client, &token, &owner, &repo, tag)?
            .ok_or_else(|| format!("{}: no GitHub release tagged '{}'", retry::NOT_FOUND, tag))?;
        let assets = release["assets"].as_array().cloned().unwrap_or_default();
        if assets.is_empty() {
            return Err(format!(
                "{}: release '{}' has no assets",
                retry::NOT_FOUND,
                tag
            ));
        }
        if assets.len() > 1 {
            return Err(format!(
                "{}: release '{}' holds {} assets — the remote path is ambiguous",
                retry::UNSUPPORTED,
                tag,
                assets.len()
            ));
        }
        let asset = &assets[0];
        let asset_id = json_u64(&asset["id"])
            .ok_or_else(|| format!("{}: release asset has no id", retry::NETWORK))?;
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/assets/{}",
            owner, repo, asset_id
        );
        let resp = send_classified("GitHub", "download", &[], || {
            Ok(client
                .get(&url)
                .header("Authorization", format!("token {}", token))
                .header("Accept", "application/octet-stream"))
        })?;
        let promised = resp.content_length();
        let bytes = resp.bytes().map_err(|err| {
            format!(
                "{}: download body unreadable: {}",
                retry::NETWORK,
                err.without_url()
            )
        })?;
        transfer::write_verified(local_path, &bytes, promised, "GitHub release asset")
    }
}

impl StorageBackend for GitHubBackend {
    fn name(&self) -> &str {
        "GitHub"
    }

    fn backend_type(&self) -> SyncBackendType {
        SyncBackendType::GitHub
    }

    fn upload_file(&self, local_path: &str, remote_path: &str) -> Result<String, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GitHub)?;
        let size = transfer::local_size(local_path)?;
        transfer::preflight(&SyncBackendType::GitHub, size)?;
        let remote = normalize_remote(remote_path, local_path);
        if size > transfer::GITHUB_INLINE_MAX {
            info!(
                "file '{}' is larger than the Contents API limit, using GitHub Releases",
                remote
            );
            self.upload_via_release(local_path, &remote, size)
        } else {
            self.upload_inline(local_path, &remote, size)
        }
    }

    fn upload_file_chunked(&self, local_path: &str, remote_path: &str) -> Result<String, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GitHub)?;
        let size = transfer::local_size(local_path)?;
        transfer::preflight(&SyncBackendType::GitHub, size)?;
        let remote = normalize_remote(remote_path, local_path);
        self.upload_via_release(local_path, &remote, size)
    }

    fn download_file(&self, remote_path: &str, local_path: &str) -> Result<(), String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GitHub)?;
        match self.resolve_locator(remote_path)? {
            GitHubLocator::Content(path) => {
                let (owner, repo) = parse_repo(&self.repo_name)?;
                let token = self.auth.get()?;
                let client = http_client()?;
                let url = self.contents_url(&owner, &repo, &path);
                let resp = send_classified("GitHub", "download", &[], || {
                    Ok(client
                        .get(&url)
                        .header("Authorization", format!("token {}", token))
                        .header("Accept", "application/vnd.github.raw")
                        .query(&[("ref", self.branch.as_str())]))
                })?;
                let promised = resp.content_length();
                let bytes = resp.bytes().map_err(|err| {
                    format!(
                        "{}: download body unreadable: {}",
                        retry::NETWORK,
                        err.without_url()
                    )
                })?;
                transfer::write_verified(local_path, &bytes, promised, "GitHub")
            }
            GitHubLocator::Release(tag) => self.download_release_asset(&tag, local_path),
        }
    }

    fn delete_file(&self, remote_path: &str) -> Result<(), String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GitHub)?;
        let (owner, repo) = parse_repo(&self.repo_name)?;
        let token = self.auth.get()?;
        let client = http_client()?;
        match self.resolve_locator(remote_path)? {
            GitHubLocator::Content(path) => {
                let url = self.contents_url(&owner, &repo, &path);
                let probe = send_classified("GitHub", "delete lookup", &[404], || {
                    Ok(client
                        .get(&url)
                        .header("Authorization", format!("token {}", token))
                        .header("Accept", "application/vnd.github+json")
                        .query(&[("ref", self.branch.as_str())]))
                })?;
                if probe.status().as_u16() == 404 {
                    return Err(format!(
                        "{}: '{}' does not exist in {}/{}",
                        retry::NOT_FOUND,
                        remote_path,
                        owner,
                        repo
                    ));
                }
                let json = parse_json(probe, "GitHub", "delete lookup")?;
                let sha = json["sha"].as_str().ok_or_else(|| {
                    format!(
                        "{}: GitHub returned no blob sha for '{}'",
                        retry::NETWORK,
                        path
                    )
                })?;
                let body = serde_json::json!({
                    "message": format!("Sync delete: {}", path),
                    "sha": sha,
                    "branch": self.branch,
                });
                send_classified("GitHub", "delete", &[], || {
                    Ok(client
                        .delete(&url)
                        .header("Authorization", format!("token {}", token))
                        .header("Accept", "application/vnd.github+json")
                        .json(&body))
                })?;
                Ok(())
            }
            GitHubLocator::Release(tag) => {
                let release = self
                    .get_release(&client, &token, &owner, &repo, &tag)?
                    .ok_or_else(|| {
                        format!("{}: no GitHub release tagged '{}'", retry::NOT_FOUND, tag)
                    })?;
                let release_id = json_u64(&release["id"]).ok_or_else(|| {
                    format!("{}: GitHub release '{}' has no id", retry::NETWORK, tag)
                })?;
                self.delete_release(&client, &token, &owner, &repo, release_id)
            }
        }
    }

    fn list_files(&self, prefix: &str) -> Result<Vec<RemoteFile>, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GitHub)?;
        let (owner, repo) = parse_repo(&self.repo_name)?;
        let token = self.auth.get()?;
        let client = http_client()?;
        let url = format!(
            "https://api.github.com/repos/{}/{}/git/trees/{}",
            owner,
            repo,
            urlencoding(&self.branch)
        );
        let resp = send_classified("GitHub", "list", &[], || {
            Ok(client
                .get(&url)
                .header("Authorization", format!("token {}", token))
                .header("Accept", "application/vnd.github+json")
                .query(&[("recursive", "1")]))
        })?;
        let json = parse_json(resp, "GitHub", "list")?;
        if json["truncated"].as_bool().unwrap_or(false) {
            return Err(format!(
                "{}: the tree for {}/{} exceeds the GitHub tree API limit",
                retry::TOO_LARGE,
                owner,
                repo
            ));
        }
        let clean = prefix.trim().trim_matches('/');
        let wanted = if clean.is_empty() {
            String::new()
        } else {
            format!("{}/", clean)
        };
        let mut files = Vec::new();
        for entry in json["tree"]
            .as_array()
            .map(|tree| tree.as_slice())
            .unwrap_or(&[])
        {
            if entry["type"].as_str() != Some("blob") {
                continue;
            }
            let path = entry["path"].as_str().unwrap_or_default();
            if path.is_empty() || (!wanted.is_empty() && !path.starts_with(&wanted)) {
                continue;
            }
            files.push(RemoteFile {
                name: file_name_of(path),
                path: path.to_string(),
                size_bytes: json_u64(&entry["size"]).unwrap_or(0),
                modified_at: String::new(),
                url: self.raw_url(&owner, &repo, path),
            });
        }
        Ok(files)
    }

    fn get_file_url(&self, remote_path: &str) -> Result<String, String> {
        let (owner, repo) = parse_repo(&self.repo_name)?;
        match self.resolve_locator(remote_path)? {
            GitHubLocator::Content(path) => Ok(self.raw_url(&owner, &repo, &path)),
            GitHubLocator::Release(tag) => {
                let name = file_name_of(remote_path);
                Ok(format!(
                    "https://github.com/{}/{}/releases/download/{}/{}",
                    owner,
                    repo,
                    tag,
                    encode_path(&name)
                ))
            }
        }
    }

    fn test_connection(&self) -> Result<bool, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GitHub)?;
        let token = self.auth.get()?;
        let client = http_client()?;
        let resp = send_probe("GitHub", || {
            Ok(client
                .get("https://api.github.com/user")
                .header("Authorization", format!("token {}", token))
                .header("Accept", "application/vnd.github+json"))
        })?;
        let _ = resp.text();
        Ok(true)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            max_size_bytes: transfer::max_size_bytes(&SyncBackendType::GitHub),
            supports_delete: true,
            supports_direct_download: true,
            recursive_list: true,
            chunked: true,
            // <<< AGENT-8 COMPUTE >>>
            compute: 1,
            // <<< /AGENT-8 COMPUTE >>>
        }
    }

    fn stat(&self, remote_path: &str) -> Result<Option<RemoteFile>, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GitHub)?;
        let (owner, repo) = parse_repo(&self.repo_name)?;
        let token = self.auth.get()?;
        let client = http_client()?;
        match self.resolve_locator(remote_path)? {
            GitHubLocator::Content(path) => {
                let url = self.contents_url(&owner, &repo, &path);
                let resp = send_classified("GitHub", "stat", &[404], || {
                    Ok(client
                        .get(&url)
                        .header("Authorization", format!("token {}", token))
                        .header("Accept", "application/vnd.github+json")
                        .query(&[("ref", self.branch.as_str())]))
                })?;
                if resp.status().as_u16() == 404 {
                    return Err(format!(
                        "{}: '{}' does not exist in {}/{}",
                        retry::NOT_FOUND,
                        remote_path,
                        owner,
                        repo
                    ));
                }
                let json = parse_json(resp, "GitHub", "stat")?;
                if json.is_array() {
                    return Ok(None);
                }
                let Some(size) = json_u64(&json["size"]) else {
                    return Ok(None);
                };
                Ok(Some(RemoteFile {
                    name: file_name_of(&path),
                    path: path.clone(),
                    size_bytes: size,
                    modified_at: self
                        .last_commit_date(&client, &token, &path)
                        .unwrap_or_default(),
                    url: self.raw_url(&owner, &repo, &path),
                }))
            }
            GitHubLocator::Release(tag) => {
                let release = self
                    .get_release(&client, &token, &owner, &repo, &tag)?
                    .ok_or_else(|| {
                        format!("{}: no GitHub release tagged '{}'", retry::NOT_FOUND, tag)
                    })?;
                let wanted = file_name_of(remote_path);
                let asset = release["assets"].as_array().and_then(|assets| {
                    assets
                        .iter()
                        .find(|asset| asset["name"].as_str() == Some(wanted.as_str()))
                });
                let Some(asset) = asset else {
                    return Err(format!(
                        "{}: release '{}' has no asset named '{}'",
                        retry::NOT_FOUND,
                        tag,
                        wanted
                    ));
                };
                let Some(size) = json_u64(&asset["size"]) else {
                    return Ok(None);
                };
                Ok(Some(RemoteFile {
                    name: wanted.clone(),
                    path: remote_path.trim().trim_matches('/').to_string(),
                    size_bytes: size,
                    modified_at: release["published_at"]
                        .as_str()
                        .or_else(|| release["created_at"].as_str())
                        .unwrap_or("")
                        .to_string(),
                    url: format!("https://github.com/{}/{}/releases/tag/{}", owner, repo, tag),
                }))
            }
        }
    }
}

// ===========================================================================
// 3. GitLabBackend
// ===========================================================================

pub struct GitLabBackend {
    auth: TokenSource,
    project_id: String,
    branch: String,
    base_url: String,
}

impl GitLabBackend {
    pub fn new(token: &str, project_id: &str, branch: &str, base_url: Option<&str>) -> Self {
        Self::with_credentials(token, project_id, branch, base_url, None)
    }

    pub fn with_credentials(
        token: &str,
        project_id: &str,
        branch: &str,
        base_url: Option<&str>,
        credentials: Option<OAuthCredentials>,
    ) -> Self {
        let base = base_url
            .unwrap_or("https://gitlab.com")
            .trim_end_matches('/')
            .to_string();
        let token_url = format!("{}/oauth/token", base);
        Self {
            auth: TokenSource::new(credentials, token.to_string(), token_url),
            project_id: project_id.to_string(),
            branch: branch.to_string(),
            base_url: base,
        }
    }

    fn api_base(&self) -> String {
        format!(
            "{}/api/v4/projects/{}",
            self.base_url,
            urlencoding(&self.project_id)
        )
    }

    fn file_api_url(&self, path: &str) -> Result<String, String> {
        if path.trim().is_empty() {
            return Err(format!(
                "{}: GitLab file path must not be empty",
                retry::UNSUPPORTED
            ));
        }
        Ok(format!(
            "{}/repository/files/{}",
            self.api_base(),
            urlencoding(path)
        ))
    }

    fn last_commit_date(
        &self,
        client: &reqwest::blocking::Client,
        token: &str,
        path: &str,
    ) -> Option<String> {
        let url = format!(
            "{}/repository/commits?path={}&ref_name={}&per_page=1",
            self.api_base(),
            urlencoding(path),
            urlencoding(&self.branch)
        );
        let resp = match send_classified("GitLab", "stat date", &[], || {
            Ok(client.get(&url).header("PRIVATE-TOKEN", token))
        }) {
            Ok(resp) => resp,
            Err(err) => {
                debug!("GitLab: no commit date for '{}': {}", path, err);
                return None;
            }
        };
        let json = parse_json(resp, "GitLab", "stat date").ok()?;
        json[0]["committed_date"].as_str().map(str::to_string)
    }

    /// Size from the files API; falls back to deriving it from the base64
    /// content. Both absent → unknown (`None`), never a fabricated 0.
    fn size_hint(json: &serde_json::Value) -> Option<u64> {
        json_u64(&json["size"]).or_else(|| {
            let text = json["content"].as_str()?;
            let stripped: String = text
                .chars()
                .filter(|c| !c.is_ascii_whitespace() && *c != '=')
                .collect();
            Some((stripped.len() as u64) * 3 / 4)
        })
    }
}

impl StorageBackend for GitLabBackend {
    fn name(&self) -> &str {
        "GitLab"
    }

    fn backend_type(&self) -> SyncBackendType {
        SyncBackendType::GitLab
    }

    fn upload_file(&self, local_path: &str, remote_path: &str) -> Result<String, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GitLab)?;
        let size = transfer::local_size(local_path)?;
        transfer::preflight(&SyncBackendType::GitLab, size)?;
        let remote = normalize_remote(remote_path, local_path);
        let url = self.file_api_url(&remote)?;
        let data = transfer::read_local(local_path)?;
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data);
        let token = self.auth.get()?;
        let client = http_client()?;

        // HEAD needs `ref` — without it real GitLab answers 400 and the
        // update path below would never run.
        let probe = send_classified("GitLab", "existence check", &[404], || {
            Ok(client
                .head(&url)
                .header("PRIVATE-TOKEN", &token)
                .query(&[("ref", self.branch.as_str())]))
        })?;
        let exists = probe.status().as_u16() != 404;
        let _ = probe.text();

        let body = serde_json::json!({
            "branch": self.branch,
            "content": encoded,
            "encoding": "base64",
            "commit_message": if exists {
                format!("Sync update: {}", remote)
            } else {
                format!("Sync upload: {}", remote)
            },
        });
        let updating = exists;
        let resp = send_classified("GitLab", "upload", &[], || {
            let builder = if updating {
                client.put(&url)
            } else {
                client.post(&url)
            };
            Ok(builder.header("PRIVATE-TOKEN", &token).json(&body))
        })?;
        let json = parse_json(resp, "GitLab", "upload")?;
        let file_path = json["file_path"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| remote.clone());
        Ok(format!(
            "{}/-/blob/{}/{}",
            self.base_url,
            urlencoding(&self.branch),
            encode_path(&file_path)
        ))
    }

    fn download_file(&self, remote_path: &str, local_path: &str) -> Result<(), String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GitLab)?;
        let clean = remote_path.trim().trim_matches('/');
        if clean.is_empty() {
            return Err(format!(
                "{}: GitLab download requires a remote path",
                retry::UNSUPPORTED
            ));
        }
        let url = format!(
            "{}?ref={}",
            self.file_api_url(clean)?,
            urlencoding(&self.branch)
        );
        let token = self.auth.get()?;
        let client = http_client()?;
        let resp = send_classified("GitLab", "download", &[], || {
            Ok(client.get(&url).header("PRIVATE-TOKEN", &token))
        })?;
        let promised = resp.content_length();
        let bytes = resp.bytes().map_err(|err| {
            format!(
                "{}: download body unreadable: {}",
                retry::NETWORK,
                err.without_url()
            )
        })?;
        transfer::write_verified(local_path, &bytes, promised, "GitLab")
    }

    fn delete_file(&self, remote_path: &str) -> Result<(), String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GitLab)?;
        let clean = remote_path.trim().trim_matches('/');
        let url = self.file_api_url(clean)?;
        let token = self.auth.get()?;
        let client = http_client()?;
        let body = serde_json::json!({
            "branch": self.branch,
            "commit_message": format!("Sync delete: {}", clean),
        });
        send_classified("GitLab", "delete", &[], || {
            Ok(client
                .delete(&url)
                .header("PRIVATE-TOKEN", &token)
                .json(&body))
        })?;
        Ok(())
    }

    fn list_files(&self, prefix: &str) -> Result<Vec<RemoteFile>, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GitLab)?;
        let token = self.auth.get()?;
        let client = http_client()?;
        let clean = prefix.trim().trim_matches('/');
        let mut files = Vec::new();
        let mut page: usize = 1;
        loop {
            let mut url = format!(
                "{}/repository/tree?ref={}&recursive=true&per_page=100&page={}",
                self.api_base(),
                urlencoding(&self.branch),
                page
            );
            if !clean.is_empty() {
                url.push_str(&format!("&path={}", urlencoding(clean)));
            }
            let resp = send_classified("GitLab", "list", &[], || {
                Ok(client.get(&url).header("PRIVATE-TOKEN", &token))
            })?;
            let json = parse_json(resp, "GitLab", "list")?;
            let items = json.as_array().ok_or_else(|| {
                format!("{}: GitLab tree listing was not an array", retry::NETWORK)
            })?;
            let count = items.len();
            for item in items {
                if item["type"].as_str() != Some("blob") {
                    continue;
                }
                let name = item["name"].as_str().unwrap_or("unknown").to_string();
                let path = item["path"].as_str().unwrap_or(&name).to_string();
                files.push(RemoteFile {
                    name,
                    path: path.clone(),
                    size_bytes: 0,
                    modified_at: String::new(),
                    url: format!(
                        "{}/-/raw/{}/{}",
                        self.base_url,
                        urlencoding(&self.branch),
                        encode_path(&path)
                    ),
                });
            }
            if count < 100 {
                break;
            }
            page += 1;
            if page > MAX_PAGES {
                return Err(format!(
                    "{}: GitLab listing holds more than {} entries",
                    retry::TOO_LARGE,
                    MAX_PAGES * 100
                ));
            }
        }
        Ok(files)
    }

    fn get_file_url(&self, remote_path: &str) -> Result<String, String> {
        let clean = remote_path.trim().trim_matches('/');
        if clean.is_empty() {
            return Err(format!(
                "{}: GitLab path must not be empty",
                retry::UNSUPPORTED
            ));
        }
        Ok(format!(
            "{}/-/raw/{}/{}",
            self.base_url,
            urlencoding(&self.branch),
            encode_path(clean)
        ))
    }

    fn test_connection(&self) -> Result<bool, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GitLab)?;
        let token = self.auth.get()?;
        let client = http_client()?;
        let url = format!(
            "{}/api/v4/projects/{}",
            self.base_url,
            urlencoding(&self.project_id)
        );
        let resp = send_probe("GitLab", || {
            Ok(client.get(&url).header("PRIVATE-TOKEN", &token))
        })?;
        let _ = resp.text();
        Ok(true)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            max_size_bytes: transfer::max_size_bytes(&SyncBackendType::GitLab),
            supports_delete: true,
            supports_direct_download: true,
            recursive_list: true,
            chunked: false,
            // <<< AGENT-8 COMPUTE >>>
            compute: 1,
            // <<< /AGENT-8 COMPUTE >>>
        }
    }

    fn stat(&self, remote_path: &str) -> Result<Option<RemoteFile>, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GitLab)?;
        let clean = remote_path.trim().trim_matches('/');
        if clean.is_empty() {
            return Err(format!(
                "{}: GitLab stat requires a file path",
                retry::UNSUPPORTED
            ));
        }
        let url = format!(
            "{}?ref={}",
            self.file_api_url(clean)?,
            urlencoding(&self.branch)
        );
        let token = self.auth.get()?;
        let client = http_client()?;
        let resp = send_classified("GitLab", "stat", &[404], || {
            Ok(client.get(&url).header("PRIVATE-TOKEN", &token))
        })?;
        if resp.status().as_u16() == 404 {
            return Err(format!(
                "{}: '{}' does not exist in the repository",
                retry::NOT_FOUND,
                clean
            ));
        }
        let json = parse_json(resp, "GitLab", "stat")?;
        let Some(size) = Self::size_hint(&json) else {
            return Ok(None);
        };
        let modified = self
            .last_commit_date(&client, &token, clean)
            .unwrap_or_default();
        Ok(Some(RemoteFile {
            name: file_name_of(clean),
            path: clean.to_string(),
            size_bytes: size,
            modified_at: modified,
            url: format!(
                "{}/-/raw/{}/{}",
                self.base_url,
                urlencoding(&self.branch),
                encode_path(clean)
            ),
        }))
    }
}

// ===========================================================================
// 4. GoogleDriveBackend
// ===========================================================================

const DRIVE_FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";
const DRIVE_UPLOAD_URL: &str = "https://www.googleapis.com/upload/drive/v3/files";
const DRIVE_FOLDER_MIME: &str = "application/vnd.google-apps.folder";

pub struct GoogleDriveBackend {
    auth: TokenSource,
    folder_id: Option<String>,
    /// Folder-id memo keyed by `parent\x01segment`.
    folders: Mutex<HashMap<String, String>>,
}

impl GoogleDriveBackend {
    pub fn new(token: &str, folder_id: Option<&str>) -> Self {
        Self::with_credentials(token, folder_id, None)
    }

    pub fn with_credentials(
        token: &str,
        folder_id: Option<&str>,
        credentials: Option<OAuthCredentials>,
    ) -> Self {
        Self {
            auth: TokenSource::new(credentials, token.to_string(), token_url_for("google")),
            folder_id: folder_id.map(str::to_string),
            folders: Mutex::new(HashMap::new()),
        }
    }

    fn root_parent(&self) -> String {
        self.folder_id.clone().unwrap_or_else(|| "root".to_string())
    }

    fn find_child(
        &self,
        client: &reqwest::blocking::Client,
        parent: &str,
        name: &str,
        folder_only: bool,
    ) -> Result<Option<String>, String> {
        let escaped = name.replace('\'', "''");
        let mut query = format!(
            "name='{}' and '{}' in parents and trashed=false",
            escaped, parent
        );
        if folder_only {
            query = format!("mimeType='{}' and {}", DRIVE_FOLDER_MIME, query);
        }
        let url = format!(
            "{}?q={}&fields=files(id,name)&pageSize=10",
            DRIVE_FILES_URL,
            urlencoding(&query)
        );
        let resp = send_classified("Google Drive", "folder lookup", &[], || {
            Ok(client
                .get(&url)
                .header("Authorization", self.auth.bearer()?))
        })?;
        let json = parse_json(resp, "Google Drive", "folder lookup")?;
        Ok(json["files"][0]["id"].as_str().map(str::to_string))
    }

    fn create_folder(
        &self,
        client: &reqwest::blocking::Client,
        parent: &str,
        name: &str,
    ) -> Result<String, String> {
        let body = serde_json::json!({
            "name": name,
            "mimeType": DRIVE_FOLDER_MIME,
            "parents": [parent],
        });
        let url = format!("{}?fields=id", DRIVE_FILES_URL);
        let resp = send_classified("Google Drive", "folder create", &[], || {
            Ok(client
                .post(&url)
                .header("Authorization", self.auth.bearer()?)
                .json(&body))
        })?;
        let json = parse_json(resp, "Google Drive", "folder create")?;
        json["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("{}: Drive folder create returned no id", retry::NETWORK))
    }

    /// Resolve `a/b/c` into folder ids under the configured root.
    /// `Ok(None)` = a segment does not exist (lookup without `create`).
    fn folder_chain(&self, dir: &str, create: bool) -> Result<Option<Vec<String>>, String> {
        let segments: Vec<&str> = dir
            .split('/')
            .map(str::trim)
            .filter(|segment| !segment.is_empty() && *segment != ".")
            .collect();
        let mut ids = Vec::with_capacity(segments.len());
        let mut parent = self.root_parent();
        if segments.is_empty() {
            return Ok(Some(ids));
        }
        let client = http_client()?;
        for segment in segments {
            let key = format!("{}\u{1}{}", parent, segment);
            let cached = self
                .folders
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get(&key)
                .cloned();
            let id = match cached {
                Some(id) => id,
                None => {
                    let found = self.find_child(&client, &parent, segment, true)?;
                    match found {
                        Some(id) => id,
                        None if create => self.create_folder(&client, &parent, segment)?,
                        None => return Ok(None),
                    }
                }
            };
            self.folders
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert(key, id.clone());
            parent = id.clone();
            ids.push(id);
        }
        Ok(Some(ids))
    }

    fn resolve_id(&self, remote: &str) -> Result<String, String> {
        let trimmed = remote.trim().trim_matches('/');
        if trimmed.is_empty() {
            return Err(format!(
                "{}: Drive path must not be empty",
                retry::UNSUPPORTED
            ));
        }
        const FILE_D: &str = "/file/d/";
        if let Some(idx) = trimmed.find(FILE_D) {
            let after = &trimmed[idx + FILE_D.len()..];
            let id = after.split('/').next().unwrap_or(after);
            if !id.is_empty() {
                return Ok(id.to_string());
            }
        }
        if !trimmed.contains('/') && !trimmed.contains('.') {
            return Ok(trimmed.to_string());
        }
        self.id_from_path(trimmed)
    }

    fn id_from_path(&self, path: &str) -> Result<String, String> {
        let segments: Vec<&str> = path
            .split('/')
            .map(str::trim)
            .filter(|segment| !segment.is_empty() && *segment != ".")
            .collect();
        if segments.is_empty() {
            return Err(format!(
                "{}: Drive path must not be empty",
                retry::UNSUPPORTED
            ));
        }
        let client = http_client()?;
        let mut parent = self.root_parent();
        for (index, segment) in segments.iter().enumerate() {
            let last = index + 1 == segments.len();
            let key = format!("{}\u{1}{}", parent, segment);
            let cached = self
                .folders
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get(&key)
                .cloned();
            let id = match cached {
                Some(id) => id,
                None => {
                    let found = self.find_child(&client, &parent, segment, !last)?;
                    let Some(id) = found else {
                        return Err(format!("{}: no Drive item at '{}'", retry::NOT_FOUND, path));
                    };
                    if !last {
                        self.folders
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .insert(key, id.clone());
                    }
                    id
                }
            };
            if last {
                return Ok(id);
            }
            parent = id;
        }
        Err(format!(
            "{}: could not resolve Drive path '{}'",
            retry::NOT_FOUND,
            path
        ))
    }

    fn parent_for(&self, remote: &str, create: bool) -> Result<String, String> {
        let clean = remote.trim().trim_matches('/');
        let parent_dir = match clean.rfind('/') {
            Some(idx) => &clean[..idx],
            None => "",
        };
        match self.folder_chain(parent_dir, create)? {
            Some(ids) => Ok(ids.last().cloned().unwrap_or_else(|| self.root_parent())),
            None => Err(format!(
                "{}: Drive folder '{}' does not exist",
                retry::NOT_FOUND,
                parent_dir
            )),
        }
    }

    fn upload_multipart(
        &self,
        local_path: &str,
        remote: &str,
        parent_id: &str,
    ) -> Result<String, String> {
        let file_name = file_name_of(remote);
        let client = http_client()?;
        let metadata = serde_json::json!({
            "name": file_name,
            "parents": [parent_id],
        });
        let metadata = serde_json::to_string(&metadata).map_err(|err| {
            format!(
                "{}: cannot serialize upload metadata: {}",
                retry::NETWORK,
                err
            )
        })?;
        let url = format!("{}?uploadType=multipart", DRIVE_UPLOAD_URL);
        let resp = send_classified("Google Drive", "upload", &[], || {
            let form = reqwest::blocking::multipart::Form::new()
                .part(
                    "metadata",
                    reqwest::blocking::multipart::Part::text(metadata.clone())
                        .mime_str("application/json; charset=UTF-8")
                        .map_err(|err| {
                            format!("{}: invalid multipart metadata: {}", retry::NETWORK, err)
                        })?,
                )
                .part(
                    "file",
                    reqwest::blocking::multipart::Part::bytes(transfer::read_local(local_path)?)
                        .file_name(file_name.clone())
                        .mime_str("application/octet-stream")
                        .map_err(|err| {
                            format!("{}: invalid multipart file part: {}", retry::NETWORK, err)
                        })?,
                );
            Ok(client
                .post(&url)
                .header("Authorization", self.auth.bearer()?)
                .multipart(form))
        })?;
        let json = parse_json(resp, "Google Drive", "upload")?;
        let id = json["id"]
            .as_str()
            .ok_or_else(|| format!("{}: Drive upload returned no file id", retry::NETWORK))?;
        Ok(format!("https://drive.google.com/file/d/{}/view", id))
    }

    fn upload_resumable(
        &self,
        local_path: &str,
        remote: &str,
        parent_id: &str,
        size: u64,
    ) -> Result<String, String> {
        let metadata = serde_json::json!({
            "name": file_name_of(remote),
            "parents": [parent_id],
        });
        let client = http_client()?;
        let start_url = format!("{}?uploadType=resumable", DRIVE_UPLOAD_URL);
        let session = send_classified("Google Drive", "resumable start", &[], || {
            Ok(client
                .post(&start_url)
                .header("Authorization", self.auth.bearer()?)
                .header("X-Upload-Content-Type", "application/octet-stream")
                .json(&metadata))
        })?;
        let location = session
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
            .ok_or_else(|| {
                format!(
                    "{}: Drive returned no resumable session URL",
                    retry::NETWORK
                )
            })?;
        let _ = session.text();

        let mut file = fs::File::open(local_path).map_err(|err| {
            format!(
                "{}: cannot open '{}' for resumable upload: {}",
                retry::NETWORK,
                local_path,
                err
            )
        })?;
        let mut start: u64 = 0;
        while start < size {
            let mut chunk = Vec::new();
            (&mut file)
                .take(transfer::DRIVE_RESUMABLE_CHUNK)
                .read_to_end(&mut chunk)
                .map_err(|err| {
                    format!(
                        "{}: cannot read '{}' for resumable upload: {}",
                        retry::NETWORK,
                        local_path,
                        err
                    )
                })?;
            if chunk.is_empty() {
                return Err(format!(
                    "{}: '{}' shrank during upload at byte {} of {}",
                    retry::NETWORK,
                    local_path,
                    start,
                    size
                ));
            }
            let end = start + chunk.len() as u64 - 1;
            let next = start + chunk.len() as u64;
            let resp =
                send_classified("Google Drive", "resumable chunk", &[308, 201, 200], || {
                    Ok(client
                        .put(&location)
                        .header("Authorization", self.auth.bearer()?)
                        .header("Content-Range", format!("bytes {}-{}/{}", start, end, size))
                        .body(chunk.clone()))
                })?;
            let status = resp.status().as_u16();
            if status == 308 {
                if next <= start {
                    return Err(format!(
                        "{}: Drive resumable upload stalled at byte {}",
                        retry::NETWORK,
                        start
                    ));
                }
                start = next;
                continue;
            }
            let json = parse_json(resp, "Google Drive", "resumable chunk")?;
            let id = json["id"].as_str().ok_or_else(|| {
                format!(
                    "{}: Drive resumable upload returned no file id",
                    retry::NETWORK
                )
            })?;
            return Ok(format!("https://drive.google.com/file/d/{}/view", id));
        }
        Err(format!(
            "{}: Drive resumable upload did not complete for '{}'",
            retry::NETWORK,
            remote
        ))
    }
}

impl StorageBackend for GoogleDriveBackend {
    fn name(&self) -> &str {
        "Google Drive"
    }

    fn backend_type(&self) -> SyncBackendType {
        SyncBackendType::GoogleDrive
    }

    fn upload_file(&self, local_path: &str, remote_path: &str) -> Result<String, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GoogleDrive)?;
        let size = transfer::local_size(local_path)?;
        transfer::preflight(&SyncBackendType::GoogleDrive, size)?;
        let remote = normalize_remote(remote_path, local_path);
        let parent_id = self.parent_for(&remote, true)?;
        if size <= transfer::DRIVE_MULTIPART_MAX {
            self.upload_multipart(local_path, &remote, &parent_id)
        } else {
            self.upload_resumable(local_path, &remote, &parent_id, size)
        }
    }

    fn upload_file_chunked(&self, local_path: &str, remote_path: &str) -> Result<String, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GoogleDrive)?;
        let size = transfer::local_size(local_path)?;
        transfer::preflight(&SyncBackendType::GoogleDrive, size)?;
        let remote = normalize_remote(remote_path, local_path);
        let parent_id = self.parent_for(&remote, true)?;
        if size == 0 {
            return self.upload_multipart(local_path, &remote, &parent_id);
        }
        self.upload_resumable(local_path, &remote, &parent_id, size)
    }

    fn download_file(&self, remote_path: &str, local_path: &str) -> Result<(), String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GoogleDrive)?;
        let id = self.resolve_id(remote_path)?;
        let url = format!("{}/{}?alt=media", DRIVE_FILES_URL, id);
        let client = http_client()?;
        let resp = send_classified("Google Drive", "download", &[], || {
            Ok(client
                .get(&url)
                .header("Authorization", self.auth.bearer()?))
        })?;
        let promised = resp.content_length();
        let bytes = resp.bytes().map_err(|err| {
            format!(
                "{}: download body unreadable: {}",
                retry::NETWORK,
                err.without_url()
            )
        })?;
        transfer::write_verified(local_path, &bytes, promised, "Google Drive")
    }

    fn delete_file(&self, remote_path: &str) -> Result<(), String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GoogleDrive)?;
        let id = self.resolve_id(remote_path)?;
        let url = format!("{}/{}", DRIVE_FILES_URL, id);
        let client = http_client()?;
        send_classified("Google Drive", "delete", &[], || {
            Ok(client
                .delete(&url)
                .header("Authorization", self.auth.bearer()?))
        })?;
        Ok(())
    }

    fn list_files(&self, prefix: &str) -> Result<Vec<RemoteFile>, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GoogleDrive)?;
        let dir = prefix.trim().trim_matches('/');
        let Some(ids) = self.folder_chain(dir, false)? else {
            return Ok(Vec::new());
        };
        let parent = ids.last().cloned().unwrap_or_else(|| self.root_parent());
        let client = http_client()?;
        let query = format!("'{}' in parents and trashed=false", parent);
        let mut page_token: Option<String> = None;
        let mut files = Vec::new();
        let mut pages = 0;
        loop {
            pages += 1;
            if pages > MAX_PAGES {
                return Err(format!(
                    "{}: Drive listing holds more than {} entries",
                    retry::TOO_LARGE,
                    MAX_PAGES * 1000
                ));
            }
            let mut url = format!(
                "{}?q={}&fields=files(id,name,size,modifiedTime,mimeType),nextPageToken&pageSize=1000",
                DRIVE_FILES_URL,
                urlencoding(&query)
            );
            if let Some(token) = &page_token {
                url.push_str(&format!("&pageToken={}", urlencoding(token)));
            }
            let resp = send_classified("Google Drive", "list", &[], || {
                Ok(client
                    .get(&url)
                    .header("Authorization", self.auth.bearer()?))
            })?;
            let json = parse_json(resp, "Google Drive", "list")?;
            for item in json["files"]
                .as_array()
                .map(|entries| entries.as_slice())
                .unwrap_or(&[])
            {
                if item["mimeType"].as_str() == Some(DRIVE_FOLDER_MIME) {
                    continue;
                }
                let id = item["id"].as_str().unwrap_or_default();
                if id.is_empty() {
                    continue;
                }
                files.push(RemoteFile {
                    name: item["name"].as_str().unwrap_or("unknown").to_string(),
                    path: id.to_string(),
                    size_bytes: json_u64(&item["size"]).unwrap_or(0),
                    modified_at: item["modifiedTime"].as_str().unwrap_or("").to_string(),
                    url: format!("https://drive.google.com/file/d/{}/view", id),
                });
            }
            match json["nextPageToken"].as_str() {
                Some(token) => page_token = Some(token.to_string()),
                None => break,
            }
        }
        Ok(files)
    }

    /// Immediate subfolders of `prefix` (one level): Drive has no paths,
    /// only parent ids, so the file listing above can never synthesize
    /// folders — this folder-mime query is what makes `ls` descend.
    /// Returned `path`s are human `prefix/name` joins (reads resolve them
    /// back to ids through `resolve_id`, same as file paths).
    fn list_dirs(&self, prefix: &str) -> Result<Vec<RemoteFile>, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GoogleDrive)?;
        let dir = prefix.trim().trim_matches('/');
        let Some(ids) = self.folder_chain(dir, false)? else {
            return Ok(Vec::new());
        };
        let parent = ids.last().cloned().unwrap_or_else(|| self.root_parent());
        let client = http_client()?;
        let query = format!(
            "'{}' in parents and mimeType='{}' and trashed=false",
            parent, DRIVE_FOLDER_MIME
        );
        let mut page_token: Option<String> = None;
        let mut dirs = Vec::new();
        let mut pages = 0;
        loop {
            pages += 1;
            if pages > MAX_PAGES {
                return Err(format!(
                    "{}: Drive folder listing holds more than {} entries",
                    retry::TOO_LARGE,
                    MAX_PAGES * 1000
                ));
            }
            let mut url = format!(
                "{}?q={}&fields=files(id,name,modifiedTime),nextPageToken&pageSize=1000",
                DRIVE_FILES_URL,
                urlencoding(&query)
            );
            if let Some(token) = &page_token {
                url.push_str(&format!("&pageToken={}", urlencoding(token)));
            }
            let resp = send_classified("Google Drive", "list", &[], || {
                Ok(client
                    .get(&url)
                    .header("Authorization", self.auth.bearer()?))
            })?;
            let json = parse_json(resp, "Google Drive", "list")?;
            for item in json["files"]
                .as_array()
                .map(|entries| entries.as_slice())
                .unwrap_or(&[])
            {
                let name = item["name"].as_str().unwrap_or_default();
                let id = item["id"].as_str().unwrap_or_default();
                if name.is_empty() || id.is_empty() {
                    continue;
                }
                let path = if dir.is_empty() {
                    name.to_string()
                } else {
                    format!("{}/{}", dir, name)
                };
                dirs.push(RemoteFile {
                    name: name.to_string(),
                    path,
                    size_bytes: 0,
                    modified_at: item["modifiedTime"].as_str().unwrap_or("").to_string(),
                    url: format!("https://drive.google.com/drive/folders/{}", id),
                });
            }
            match json["nextPageToken"].as_str() {
                Some(token) => page_token = Some(token.to_string()),
                None => break,
            }
        }
        Ok(dirs)
    }

    fn get_file_url(&self, remote_path: &str) -> Result<String, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GoogleDrive)?;
        let id = self.resolve_id(remote_path)?;
        Ok(format!("https://drive.google.com/file/d/{}/view", id))
    }

    fn test_connection(&self) -> Result<bool, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GoogleDrive)?;
        let client = http_client()?;
        let resp = send_probe("Google Drive", || {
            Ok(client
                .get("https://www.googleapis.com/drive/v3/about?fields=user")
                .header("Authorization", self.auth.bearer()?))
        })?;
        let _ = resp.text();
        Ok(true)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            max_size_bytes: transfer::max_size_bytes(&SyncBackendType::GoogleDrive),
            supports_delete: true,
            supports_direct_download: false,
            recursive_list: false,
            chunked: true,
            // <<< AGENT-8 COMPUTE >>>
            compute: 1,
            // <<< /AGENT-8 COMPUTE >>>
        }
    }

    fn stat(&self, remote_path: &str) -> Result<Option<RemoteFile>, String> {
        let _permit = rate_limit::acquire(&SyncBackendType::GoogleDrive)?;
        let id = self.resolve_id(remote_path)?;
        let url = format!(
            "{}/{}?fields=id,name,size,modifiedTime,mimeType",
            DRIVE_FILES_URL, id
        );
        let client = http_client()?;
        let resp = send_classified("Google Drive", "stat", &[404], || {
            Ok(client
                .get(&url)
                .header("Authorization", self.auth.bearer()?))
        })?;
        if resp.status().as_u16() == 404 {
            return Err(format!(
                "{}: '{}' does not exist in Google Drive",
                retry::NOT_FOUND,
                remote_path
            ));
        }
        let json = parse_json(resp, "Google Drive", "stat")?;
        let Some(size) = json_u64(&json["size"]) else {
            return Ok(None);
        };
        Ok(Some(RemoteFile {
            name: json["name"].as_str().unwrap_or("unknown").to_string(),
            path: id.clone(),
            size_bytes: size,
            modified_at: json["modifiedTime"].as_str().unwrap_or("").to_string(),
            url: format!("https://drive.google.com/file/d/{}/view", id),
        }))
    }
}

// ===========================================================================
// Factory: build a StorageBackend from a SyncConfig
// ===========================================================================

/// Create the appropriate `StorageBackend` from a `SyncConfig`.
pub fn create_backend(config: &SyncConfig) -> Result<Box<dyn StorageBackend>, String> {
    match config.backend_type {
        SyncBackendType::Local => {
            let base = config.base_path.as_deref().ok_or_else(|| {
                format!("{}: Local backend requires base_path", retry::UNSUPPORTED)
            })?;
            Ok(Box::new(LocalBackend::new(base)))
        }
        SyncBackendType::GitHub => {
            let repo = config.repo_name.as_deref().ok_or_else(|| {
                format!("{}: GitHub backend requires repo_name", retry::UNSUPPORTED)
            })?;
            let branch = config.branch.as_deref().unwrap_or("main").to_string();
            let token = oauth::resolve_token(config)?;
            let credentials = oauth::load_credentials(&config.id);
            Ok(Box::new(GitHubBackend::with_credentials(
                &token,
                repo,
                &branch,
                credentials,
            )))
        }
        SyncBackendType::GitLab => {
            let project_id = config.repo_name.as_deref().ok_or_else(|| {
                format!(
                    "{}: GitLab backend requires project_id (use repo_name field)",
                    retry::UNSUPPORTED
                )
            })?;
            let branch = config.branch.as_deref().unwrap_or("main").to_string();
            let base_url = gitlab_instance_base(config);
            let token = oauth::resolve_token(config)?;
            let credentials = oauth::load_credentials(&config.id);
            Ok(Box::new(GitLabBackend::with_credentials(
                &token,
                project_id,
                &branch,
                Some(base_url.as_str()),
                credentials,
            )))
        }
        SyncBackendType::GoogleDrive => {
            let token = oauth::resolve_token(config)?;
            let credentials = oauth::load_credentials(&config.id);
            Ok(Box::new(GoogleDriveBackend::with_credentials(
                &token,
                config.folder_id.as_deref(),
                credentials,
            )))
        }
    }
}

/// GitLab instance base URL: env override → `base_path` → gitlab.com.
pub(crate) fn gitlab_instance_base(config: &SyncConfig) -> String {
    let from_env = std::env::var("CYBERMANJU_GITLAB_INSTANCE_URL").unwrap_or_default();
    let candidate = if from_env.trim().is_empty() {
        config.base_path.clone().unwrap_or_default()
    } else {
        from_env
    };
    let candidate = candidate.trim().trim_end_matches('/').to_string();
    if candidate.starts_with("https://") || candidate.starts_with("http://") {
        candidate
    } else {
        "https://gitlab.com".to_string()
    }
}

// ===========================================================================
// 5. Private vault repository provisioning (GitHub + GitLab)
// ===========================================================================
//
// `create_repository` turns a pasted PAT/OAuth token into a new *private*
// repo that will hold the `.cybermanju` vault file. No local git is needed:
// both providers expose a JSON "create project" endpoint and this uses the
// same classified `send_classified` plumbing as every other provider call,
// so failures carry the AGENT-1 `auth:` / `conflict:` / `network:` prefixes
// the UI already maps to hints.

/// A freshly created remote repository.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedRepo {
    /// `github` | `gitlab` (canonical backend slug).
    pub backend: String,
    /// What to store in `SyncConfig.repo_name`: `owner/repo` (GitHub) or
    /// the numeric project id (GitLab).
    pub repo_name: String,
    /// Human `owner/repo` / `group/path` form (GitLab also fills this).
    pub full_name: String,
    /// Default branch the provider created (`main` unless it says otherwise).
    pub branch: String,
    /// Browser URL of the new repo.
    pub url: String,
    /// GitLab numeric project id (GitHub: `None`).
    pub project_id: Option<String>,
}

/// Input for [`create_repository`]. `name` accepts `repo` or `owner/repo`
/// (GitHub) / `group/path` (GitLab) — only the last segment becomes the
/// project name; the owner/group prefix is resolved by the provider token.
#[derive(Debug, Clone)]
pub struct CreateRepoInput {
    pub backend: String,
    pub token: String,
    pub name: String,
    pub private: bool,
    pub description: String,
    pub branch: String,
    pub base_url: Option<String>,
}

fn valid_repo_segment(seg: &str) -> bool {
    if seg.is_empty() || seg.len() > 100 {
        return false;
    }
    seg.bytes()
        .all(|b| matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.'))
        && seg != "."
        && seg != ".."
}

fn last_segment(name: &str) -> String {
    name.trim()
        .trim_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else if c == ' ' || c == '.' {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "cybermanju-vault".to_string()
    } else {
        trimmed
    }
}

fn create_github_repo(input: &CreateRepoInput, repo: &str) -> Result<CreatedRepo, String> {
    if input.token.trim().is_empty() {
        return Err(format!(
            "{}: GitHub repo creation needs a token",
            retry::AUTH
        ));
    }
    if !valid_repo_segment(repo) {
        return Err(format!(
            "{}: repo name '{}' is invalid — use letters, numbers, '-', '_' or '.' (max 100 chars)",
            retry::UNSUPPORTED,
            repo
        ));
    }
    let branch = if input.branch.trim().is_empty() {
        "main".to_string()
    } else {
        input.branch.trim().to_string()
    };
    let client = http_client()?;
    let token = input.token.trim().to_string();
    let body = serde_json::json!({
        "name": repo,
        "private": input.private,
        "description": input.description,
        "auto_init": true,
        "default_branch": branch,
    });
    let resp = send_classified("GitHub", "repo create", &[201], || {
        Ok(client
            .post("https://api.github.com/user/repos")
            .header("Authorization", format!("token {}", token))
            .header("Accept", "application/vnd.github+json")
            .json(&body))
    })?;
    let json = parse_json(resp, "GitHub", "repo create")?;
    let full = json["full_name"].as_str().unwrap_or_default().to_string();
    let full = if full.is_empty() {
        repo.to_string()
    } else {
        full
    };
    let url = json["html_url"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://github.com/{}", full));
    let branch = json["default_branch"]
        .as_str()
        .map(str::to_string)
        .unwrap_or(branch);
    Ok(CreatedRepo {
        backend: "github".to_string(),
        repo_name: full.clone(),
        full_name: full,
        branch,
        url,
        project_id: None,
    })
}

fn create_gitlab_project(
    input: &CreateRepoInput,
    repo: &str,
    base: &str,
) -> Result<CreatedRepo, String> {
    if input.token.trim().is_empty() {
        return Err(format!(
            "{}: GitLab project creation needs a token",
            retry::AUTH
        ));
    }
    if !valid_repo_segment(repo) {
        return Err(format!(
            "{}: project name '{}' is invalid — use letters, numbers, '-', '_' or '.' (max 100 chars)",
            retry::UNSUPPORTED,
            repo
        ));
    }
    let branch = if input.branch.trim().is_empty() {
        "main".to_string()
    } else {
        input.branch.trim().to_string()
    };
    let client = http_client()?;
    let token = input.token.trim().to_string();
    let body = serde_json::json!({
        "name": repo,
        "path": slugify(repo),
        "visibility": if input.private { "private" } else { "public" },
        "description": input.description,
        "initialize_with_readme": true,
        "default_branch": branch,
    });
    let url = format!("{}/api/v4/projects", base);
    let resp = send_classified("GitLab", "project create", &[201], || {
        Ok(client
            .post(&url)
            .header("PRIVATE-TOKEN", &token)
            .json(&body))
    })?;
    let json = parse_json(resp, "GitLab", "project create")?;
    let id = json["id"].clone();
    let id_str = if let Some(n) = id.as_u64() {
        n.to_string()
    } else if let Some(s) = id.as_str() {
        s.to_string()
    } else {
        return Err(format!(
            "{}: GitLab project create returned no id",
            retry::NETWORK
        ));
    };
    let full = json["path_with_namespace"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| repo.to_string());
    let web_url = json["web_url"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}/{}", base, full));
    let branch = json["default_branch"]
        .as_str()
        .map(str::to_string)
        .unwrap_or(branch);
    Ok(CreatedRepo {
        backend: "gitlab".to_string(),
        repo_name: id_str.clone(),
        full_name: full,
        branch,
        url: web_url,
        project_id: Some(id_str),
    })
}

/// Create a new private vault repository on GitHub or GitLab.
///
/// `input.backend` accepts `github`/`gitlab` (any casing). `input.name` may
/// be `my-vault` or `owner/my-vault`; only the last segment is sent — the
/// provider derives the owner from the token. An existing name surfaces as
/// `conflict:` (HTTP 422/400 from the provider) so the UI can suggest a new
/// name instead of showing a raw API error.
pub fn create_repository(input: &CreateRepoInput) -> Result<CreatedRepo, String> {
    let backend = input.backend.trim().to_ascii_lowercase();
    let repo = last_segment(&input.name);
    if repo.is_empty() {
        return Err(format!(
            "{}: repo name must not be empty",
            retry::UNSUPPORTED
        ));
    }
    match backend.as_str() {
        "github" => create_github_repo(input, &repo),
        "gitlab" => {
            let base = input
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_end_matches('/').to_string())
                .filter(|s| s.starts_with("https://") || s.starts_with("http://"))
                .unwrap_or_else(|| "https://gitlab.com".to_string());
            create_gitlab_project(input, &repo, &base)
        }
        "" => Err(format!(
            "{}: provider is required ('github' or 'gitlab')",
            retry::UNSUPPORTED
        )),
        other => Err(format!(
            "{}: repo creation is not supported for '{}' (github + gitlab only)",
            retry::UNSUPPORTED,
            other
        )),
    }
}
