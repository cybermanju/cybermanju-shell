// CyberManju OS — Search (src-tauri/src/commands/search.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: search_files, search_files_paginated, suggest.
// Integrates: cybermanju_web, tauri.
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — Search commands (thin wrappers over the shared API)

use std::sync::Arc;

use cybermanju_web::api;
use tauri::State;

use crate::AppState;

/// Search result returned to the frontend.
pub use cybermanju_web::api::search_api::SearchHit as SearchResult;

/// Paginated search results with total count.
pub use cybermanju_web::api::search_api::PaginatedHits as PaginatedSearchResult;

/// Search files using the shared full-text index (falls back to a substring
/// scan when no index is attached).
#[tauri::command(async)]
pub fn search_files(
    query: String,
    limit: Option<usize>,
    offset: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<SearchResult>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    let index = Some(Arc::clone(&state.tantivy_index));
    api::search_api::search(&index, &db, &query, limit, offset)
}

/// Search files with pagination (offset + limit) and total count.
#[tauri::command(async)]
pub fn search_files_paginated(
    query: String,
    limit: usize,
    offset: usize,
    state: State<'_, AppState>,
) -> Result<PaginatedSearchResult, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    let index = Some(Arc::clone(&state.tantivy_index));
    api::search_api::search_paginated(&index, &db, &query, limit, offset)
}

/// Cross-device search port: local BM25 plus one bounded `GET /api/search`
/// per explicit peer base URL (merged, deduped, max score wins). Explicit
/// URLs are what make android/ios/docker/wasm work where mDNS discovery
/// cannot — a dead peer degrades to zero hits for that peer, never a
/// failed search.
#[tauri::command(async)]
pub fn search_files_federated(
    query: String,
    limit: Option<usize>,
    peers: Vec<cybermanju_web::api::search_api::FederatedPeer>,
    timeout_secs: Option<u64>,
    state: State<'_, AppState>,
) -> Result<Vec<cybermanju_sync::peers::FederatedHit>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    let index = Some(Arc::clone(&state.tantivy_index));
    Ok(api::search_api::search_federated(
        &index,
        &db,
        &api::search_api::FederatedRequest {
            query,
            limit,
            peers,
            timeout_secs,
        },
        "local",
    ))
}

/// Get type-ahead suggestions for a prefix query.
#[tauri::command(async)]
pub fn suggest(
    prefix: String,
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    let index = Some(Arc::clone(&state.tantivy_index));
    api::search_api::suggest(&index, &db, &prefix, limit)
}
