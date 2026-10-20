// CyberManju OS — Search api (crates/web/src/api/search_api.rs).
// Role: REST dashboard (:3456): routes, auth, security headers.
// Exports: SearchHit, PaginatedHits, search, search_paginated, suggest,
// caller-scoped search/suggest/federation APIs, FederatedPeer, FederatedRequest.
// Integrates: cybermanju_db, cybermanju_search, cybermanju_types, redb.
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — Search (shared by Tauri IPC and REST)

use std::sync::{Arc, RwLock};

use crate::security::Claims;
use cybermanju_db::Database;
use cybermanju_search::{SearchIndex, SearchRequest};
use cybermanju_types::schema::FileNode;
use redb::ReadableTable;

/// Search result item returned to the frontend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub file_id: String,
    pub file_name: String,
    pub score: f64,
    pub snippet: Option<String>,
}

/// Paginated search results with a total match count.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedHits {
    pub results: Vec<SearchHit>,
    pub total: usize,
}

type SharedIndex = Option<Arc<RwLock<SearchIndex>>>;
const MAX_SCOPED_SEARCH_CANDIDATES: usize = 10_000;

fn ensure_candidate_budget(candidate_count: usize) -> Result<(), String> {
    if candidate_count > MAX_SCOPED_SEARCH_CANDIDATES {
        return Err(format!(
            "too_large: scoped search exceeds the {} candidate limit",
            MAX_SCOPED_SEARCH_CANDIDATES
        ));
    }
    Ok(())
}

/// Full-text search. Falls back to a case-insensitive substring scan over
/// the files table when no index is attached (e.g. before the index has
/// been built).
pub fn search(
    index: &SharedIndex,
    db: &Database,
    query: &str,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<SearchHit>, String> {
    let limit = limit.unwrap_or(50);

    match index {
        Some(idx) => {
            let idx = idx.read().map_err(|e| e.to_string())?;
            let request = SearchRequest {
                query: query.to_string(),
                limit: Some(limit),
                offset,
            };
            let results = idx.search(&request).map_err(|e| e.to_string())?;
            Ok(results.into_iter().map(into_hit).collect())
        }
        None => Ok(substring_scan(db, query, limit, offset)),
    }
}

/// Search with pagination: returns a page of results plus the total count.
pub fn search_paginated(
    index: &SharedIndex,
    db: &Database,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<PaginatedHits, String> {
    let results = search(index, db, query, Some(limit), Some(offset))?;
    let total = match index {
        Some(idx) => {
            let idx = idx.read().map_err(|e| e.to_string())?;
            let count_request = SearchRequest {
                query: query.to_string(),
                limit: None,
                offset: None,
            };
            idx.search(&count_request).map_err(|e| e.to_string())?.len()
        }
        None => substring_scan(db, query, usize::MAX, None).len(),
    };

    Ok(PaginatedHits { results, total })
}

/// Caller-scoped search. ACL checks run before pagination.
pub fn search_for_caller(
    index: &SharedIndex,
    db: &Database,
    query: &str,
    limit: Option<usize>,
    offset: Option<usize>,
    claims: &Claims,
) -> Result<Vec<SearchHit>, String> {
    let matches = scoped_matches(index, db, query, claims)?;
    Ok(matches
        .into_iter()
        .skip(offset.unwrap_or(0))
        .take(limit.unwrap_or(50))
        .collect())
}

/// Caller-scoped pagination; total includes only readable files.
pub fn search_paginated_for_caller(
    index: &SharedIndex,
    db: &Database,
    query: &str,
    limit: usize,
    offset: usize,
    claims: &Claims,
) -> Result<PaginatedHits, String> {
    let matches = scoped_matches(index, db, query, claims)?;
    let total = matches.len();
    Ok(PaginatedHits {
        results: matches.into_iter().skip(offset).take(limit).collect(),
        total,
    })
}

/// Type-ahead suggestions for a prefix query.
pub fn suggest(
    index: &SharedIndex,
    db: &Database,
    prefix: &str,
    limit: usize,
) -> Result<Vec<String>, String> {
    match index {
        Some(idx) => {
            let idx = idx.read().map_err(|e| e.to_string())?;
            let suggestions = idx.suggest(prefix, limit).map_err(|e| e.to_string())?;
            Ok(suggestions.into_iter().map(|s| s.text).collect())
        }
        None => {
            let prefix_lc = prefix.to_lowercase();
            let mut texts: Vec<String> = Vec::new();
            for node in all_files(db)? {
                if !node.name.to_lowercase().contains(&prefix_lc) {
                    continue;
                }
                texts.push(node.name);
                if texts.len() >= limit {
                    break;
                }
            }
            Ok(texts)
        }
    }
}

/// Suggestions are generated only from accessible filenames, never identity-free
/// index terms that may include stale plaintext from encrypted documents.
pub fn suggest_for_caller(
    db: &Database,
    prefix: &str,
    limit: usize,
    claims: &Claims,
) -> Result<Vec<String>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let prefix = prefix.to_lowercase();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for node in all_files(db)? {
        if crate::api::files::check_access(db, claims, &node.id, "read").is_err() {
            continue;
        }
        for word in node.name.split(|c: char| !c.is_alphanumeric()) {
            let word = word.to_lowercase();
            if word.starts_with(&prefix) && word.len() > prefix.len() && seen.insert(word.clone()) {
                out.push(word);
                if out.len() >= limit {
                    return Ok(out);
                }
            }
        }
    }
    Ok(out)
}

fn scoped_matches(
    index: &SharedIndex,
    db: &Database,
    query: &str,
    claims: &Claims,
) -> Result<Vec<SearchHit>, String> {
    let candidates: Vec<SearchHit> = match index {
        Some(idx) => {
            let idx = idx.read().map_err(|e| e.to_string())?;
            let candidates = idx
                .search(&SearchRequest {
                    query: query.to_string(),
                    limit: Some(MAX_SCOPED_SEARCH_CANDIDATES + 1),
                    offset: Some(0),
                })
                .map_err(|e| e.to_string())?;
            ensure_candidate_budget(candidates.len())?;
            candidates.into_iter().map(into_hit).collect()
        }
        None => bounded_substring_candidates(db, query)?,
    };
    let mut visible = Vec::new();
    for mut hit in candidates {
        if crate::api::files::check_access(db, claims, &hit.file_id, "read").is_err() {
            continue;
        }
        let node = db.get_file_node(&hit.file_id).map_err(|e| e.to_string())?;
        let Some(node) = node else { continue };
        // Tantivy may still contain plaintext from before encryption. Hiding
        // only the snippet is insufficient: score/membership also leaks a
        // content match. Omit encrypted index hits until the index is rebuilt.
        if node.encrypted && index.is_some() {
            continue;
        }
        // The index may also carry a filename from before a rename.
        hit.file_name = node.name;
        visible.push(hit);
    }
    Ok(visible)
}

fn bounded_substring_candidates(db: &Database, query: &str) -> Result<Vec<SearchHit>, String> {
    let query_lc = query.to_lowercase();
    if query_lc.is_empty() {
        return Ok(Vec::new());
    }

    let mut matches = Vec::new();
    for node in all_files(db)? {
        if !node.name.to_lowercase().contains(&query_lc) {
            continue;
        }
        matches.push(SearchHit {
            file_id: node.id,
            file_name: node.name,
            score: 1.0,
            snippet: None,
        });
        ensure_candidate_budget(matches.len())?;
    }
    Ok(matches)
}

fn into_hit(r: cybermanju_search::SearchResult) -> SearchHit {
    SearchHit {
        file_id: r.file_id,
        file_name: r.file_name,
        score: r.score,
        snippet: if r.snippet.is_empty() {
            None
        } else {
            Some(r.snippet)
        },
    }
}

/// One peer to fan a federated search out to (explicit base URL — this is
/// what makes android/ios/docker/wasm work where mDNS discovery cannot).
/// `token` is mandatory and is the peer's own bearer; the caller's token is
/// never reused or forwarded.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FederatedPeer {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub token: String,
}

/// Federated search request — local BM25 first, then peer fan-out.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FederatedRequest {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub peers: Vec<FederatedPeer>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Federated search: local hits (tagged `local`) plus one bounded
/// `GET /api/search` per peer (tagged with its base URL), merged with
/// `peers::merge_federated_hits` (dedup by file id, max score wins). A dead
/// peer degrades to zero hits for that peer — never a failed search.
/// Only an explicit, non-empty per-peer `token` is sent. Private/LAN targets
/// additionally require a fresh presence entry for an active device owned by
/// the authenticated caller and an exact host/port match.
pub fn search_federated(
    index: &SharedIndex,
    db: &Database,
    req: &FederatedRequest,
    owner_id: &str,
) -> Vec<cybermanju_sync::peers::FederatedHit> {
    let limit = req.limit.unwrap_or(50).clamp(1, 200);
    let local = search(index, db, &req.query, Some(limit), None).unwrap_or_default();
    let hits = local
        .into_iter()
        .map(|h| cybermanju_sync::peers::FederatedHit {
            file_id: h.file_id,
            file_name: h.file_name,
            score: h.score,
            snippet: h.snippet.unwrap_or_default(),
            source: "local".to_string(),
        })
        .collect();
    federate_local_hits(db, req, owner_id, hits)
}

/// Federated variant whose local results are caller-ACL-filtered before merge.
pub fn search_federated_for_caller(
    index: &SharedIndex,
    db: &Database,
    req: &FederatedRequest,
    claims: &Claims,
) -> Vec<cybermanju_sync::peers::FederatedHit> {
    let limit = req.limit.unwrap_or(50).clamp(1, 200);
    let local = scoped_matches(index, db, &req.query, claims).unwrap_or_default();
    let hits = local
        .into_iter()
        .take(limit)
        .map(|h| cybermanju_sync::peers::FederatedHit {
            file_id: h.file_id,
            file_name: h.file_name,
            score: h.score,
            snippet: h.snippet.unwrap_or_default(),
            source: "local".to_string(),
        })
        .collect();
    federate_local_hits(db, req, &claims.user_id, hits)
}

fn federate_local_hits(
    db: &Database,
    req: &FederatedRequest,
    owner_id: &str,
    mut hits: Vec<cybermanju_sync::peers::FederatedHit>,
) -> Vec<cybermanju_sync::peers::FederatedHit> {
    let limit = req.limit.unwrap_or(50).clamp(1, 200);
    let timeout = req.timeout_secs.unwrap_or(8).clamp(1, 30);
    for peer in req.peers.iter().take(8) {
        if peer.base_url.trim().is_empty() {
            continue;
        }
        if cybermanju_sync::peers::classify_addr(&peer.base_url)
            == cybermanju_sync::peers::PeerTransport::RelayFuture
        {
            continue;
        }
        let token = peer.token.trim();
        if token.is_empty() {
            continue;
        }
        let allow_private = authorized_lan_peer(db, owner_id, peer);
        match cybermanju_sync::peers::fetch_peer_hits_with_policy(
            &peer.base_url,
            &req.query,
            limit,
            timeout,
            Some(token),
            allow_private,
        ) {
            Ok(mut remote) => {
                if !peer.device_id.trim().is_empty() {
                    let normalized = cybermanju_sync::peers::normalize_base_url(&peer.base_url)
                        .unwrap_or_else(|_| peer.base_url.clone());
                    for hit in &mut remote {
                        if hit.source == peer.base_url || hit.source == normalized {
                            hit.source = peer.device_id.clone();
                        }
                    }
                }
                hits.append(&mut remote);
            }
            Err(_) => continue,
        }
    }
    cybermanju_sync::peers::merge_federated_hits(hits, limit)
}

fn authorized_lan_peer(db: &Database, owner_id: &str, peer: &FederatedPeer) -> bool {
    if owner_id.trim().is_empty() || peer.device_id.trim().is_empty() {
        return false;
    }
    let Ok(Some(device)) = db.get_device(&peer.device_id) else {
        return false;
    };
    if device.revoked || device.owner_id != owner_id || device.public_key.trim().is_empty() {
        return false;
    }
    let presence = cybermanju_sync::peers::known_presence(
        db,
        cybermanju_sync::peers::presence_now(),
        cybermanju_sync::peers::PRESENCE_TTL_SECS,
    );
    presence.iter().any(|entry| {
        entry.device_id == peer.device_id
            && cybermanju_sync::peers::peer_base_matches_presence(
                &peer.base_url,
                &entry.host,
                entry.port,
            )
    })
}

fn all_files(db: &Database) -> Result<Vec<FileNode>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_files_table())
        .map_err(|e| e.to_string())?;

    let mut files = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        if let Ok(node) = serde_json::from_str::<FileNode>(value.value()) {
            files.push(node);
        }
    }
    Ok(files)
}

fn substring_scan(
    db: &Database,
    query: &str,
    limit: usize,
    offset: Option<usize>,
) -> Vec<SearchHit> {
    let query_lc = query.to_lowercase();
    if query_lc.is_empty() {
        return Vec::new();
    }

    let Ok(files) = all_files(db) else {
        return Vec::new();
    };

    files
        .into_iter()
        .filter(|f| f.name.to_lowercase().contains(&query_lc))
        .skip(offset.unwrap_or(0))
        .take(limit)
        .map(|f| SearchHit {
            file_id: f.id,
            file_name: f.name,
            score: 1.0,
            snippet: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cybermanju_types::schema::{User, UserFilePermission};

    #[test]
    fn scoped_search_candidate_budget_fails_only_above_cap() {
        assert!(ensure_candidate_budget(MAX_SCOPED_SEARCH_CANDIDATES).is_ok());
        let error = ensure_candidate_budget(MAX_SCOPED_SEARCH_CANDIDATES + 1).unwrap_err();
        assert!(error.starts_with("too_large:"));
    }

    fn node(id: &str, name: &str, encrypted: bool) -> FileNode {
        FileNode {
            id: id.into(),
            name: name.into(),
            file_type: "file".into(),
            parent_id: None,
            size_bytes: 1,
            mime_type: None,
            hash_blake3: None,
            encrypted,
            encryption_algorithm: None,
            compression_layers: Vec::new(),
            thumbnail_path: None,
            context_data: None,
            tags: Vec::new(),
            collection_ids: Vec::new(),
            face_group_ids: Vec::new(),
            loose_group_ids: Vec::new(),
            gps_lat: None,
            gps_lon: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            modified_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn scoped_search_counts_only_granted_files_and_hides_stale_encrypted_snippet() {
        let root = tempfile::tempdir().unwrap();
        let db = Database::new(root.path().join("search.redb").to_str().unwrap()).unwrap();
        let user = User {
            id: "reader".into(),
            username: "reader".into(),
            password_hash: "x".into(),
            display_name: None,
            role: "user".into(),
            is_active: true,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        let nodes = [
            node("visible", "needle-visible", false),
            node("encrypted", "needle-encrypted", true),
            node("denied", "needle-denied", false),
        ];
        let tx = db.begin_write().unwrap();
        {
            let mut users = tx.open_table(Database::get_users_table()).unwrap();
            users
                .insert("reader", serde_json::to_string(&user).unwrap().as_str())
                .unwrap();
            let mut files = tx.open_table(Database::get_files_table()).unwrap();
            let mut perms = tx
                .open_table(Database::get_user_file_perms_table())
                .unwrap();
            for n in &nodes {
                files
                    .insert(n.id.as_str(), serde_json::to_string(n).unwrap().as_str())
                    .unwrap();
                if n.id != "denied" {
                    let perm = UserFilePermission {
                        id: format!("perm-{}", n.id),
                        user_id: "reader".into(),
                        file_id: n.id.clone(),
                        access: "read".into(),
                        granted_by: "admin".into(),
                        granted_at: "2026-01-01T00:00:00Z".into(),
                    };
                    perms
                        .insert(
                            perm.id.as_str(),
                            serde_json::to_string(&perm).unwrap().as_str(),
                        )
                        .unwrap();
                }
            }
        }
        tx.commit().unwrap();

        let index = SearchIndex::new(root.path().join("index").to_str().unwrap()).unwrap();
        for n in &nodes {
            let content = match n.id.as_str() {
                "encrypted" => "needle stale plaintext secret ghostword",
                "visible" => "needle visible plaintext snippet",
                _ => "needle denied body",
            };
            index
                .add_document(
                    &n.id,
                    &n.name,
                    content,
                    &[],
                    "txt",
                    false,
                    false,
                    "2026-01-01T00:00:00Z",
                    None,
                )
                .unwrap();
        }
        let index = Some(Arc::new(RwLock::new(index)));
        let claims = Claims {
            sub: "reader".into(),
            user_id: "reader".into(),
            role: "user".into(),
            iat: 1,
            exp: u64::MAX,
            jti: "test".into(),
        };
        let page = search_paginated_for_caller(&index, &db, "needle", 10, 0, &claims).unwrap();
        assert_eq!(page.total, 1);
        assert!(!page.results.iter().any(|h| h.file_id == "denied"));
        assert!(!page.results.iter().any(|h| h.file_id == "encrypted"));
        let stale_only =
            search_paginated_for_caller(&index, &db, "ghostword", 10, 0, &claims).unwrap();
        assert_eq!(stale_only.total, 0);
        let visible = page
            .results
            .iter()
            .find(|h| h.file_id == "visible")
            .unwrap();
        assert!(visible
            .snippet
            .as_deref()
            .unwrap()
            .contains("visible plaintext"));
        let suggestions = suggest_for_caller(&db, "den", 10, &claims).unwrap();
        assert!(!suggestions.iter().any(|s| s.contains("denied")));
    }
}
