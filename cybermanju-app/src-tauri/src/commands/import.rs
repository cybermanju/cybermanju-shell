// CyberManju OS — Import (src-tauri/src/commands/import.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: ImportResult, UploadResult, import_file, scan_directory, upload_file, rebuild_search_index, import_from_url.
// Integrates: chrono, redb, serde, tauri.
// Contracts: none special (pure logic; errors bubble to caller).
use chrono::Utc;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::schema::FileNode;
use crate::AppState;

/// Result of a directory scan / import operation.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub files_imported: u32,
    pub folders_imported: u32,
    pub errors: Vec<String>,
    pub duration_ms: u64,
}

/// File upload result — stores the actual file on disk and creates a DB entry.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadResult {
    pub file_node: FileNode,
    pub bytes_written: u64,
}

/// Import a single file from the real filesystem into the database.
/// Reads the file's actual metadata (size, mime type), hashes it with BLAKE3,
/// extracts EXIF GPS if it's an image, and stores the FileNode.
#[tauri::command(async)]
pub fn import_file(
    file_path: String,
    parent_path: String,
    state: State<'_, AppState>,
) -> Result<FileNode, String> {
    let start = std::time::Instant::now();

    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Err(format!("not_found: file '{}' not found", file_path));
    }

    let metadata = std::fs::metadata(path)
        .map_err(|e| format!("integrity: cannot read file metadata: {}", e))?;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let is_dir = metadata.is_dir();
    let size_bytes = metadata.len();

    // Detect MIME type from file extension
    let mime_type = if is_dir {
        None
    } else {
        infer::get_from_path(path)
            .ok()
            .flatten()
            .map(|m| m.mime_type())
            .map(|m| m.to_string())
            .or_else(|| mime_guess::from_path(path).first().map(|m| m.to_string()))
    };

    // Hash file contents with BLAKE3 (for files only, not directories)
    let hash_blake3 = if !is_dir && size_bytes < 100 * 1024 * 1024 {
        // Only hash files under 100MB
        match std::fs::read(path) {
            Ok(data) => Some(blake3::hash(&data).to_hex().to_string()),
            Err(_) => None,
        }
    } else {
        None
    };

    let now = Utc::now().to_rfc3339();
    let file_id = uuid::Uuid::new_v4().to_string();

    // Build context_data with the real filesystem path
    let mut context = serde_json::Map::new();
    context.insert("original_path".to_string(), serde_json::json!(file_path));
    context.insert("source".to_string(), serde_json::json!("filesystem_import"));

    // Extract EXIF GPS for image files
    let (gps_lat, gps_lon) = if !is_dir {
        extract_gps_if_image(path)
    } else {
        (None, None)
    };

    if let (Some(lat), Some(lon)) = (gps_lat, gps_lon) {
        context.insert("gps_source".to_string(), serde_json::json!("exif"));
        context.insert("gps_lat".to_string(), serde_json::json!(lat));
        context.insert("gps_lon".to_string(), serde_json::json!(lon));
    }

    let file_node = FileNode {
        id: file_id.clone(),
        name: file_name,
        file_type: if is_dir { "folder" } else { "file" }.to_string(),
        parent_id: if parent_path.is_empty() {
            None
        } else {
            Some(parent_path)
        },
        size_bytes,
        mime_type,
        hash_blake3,
        encrypted: false,
        encryption_algorithm: None,
        compression_layers: Vec::new(),
        thumbnail_path: None,
        context_data: Some(serde_json::Value::Object(context)),
        tags: Vec::new(),
        collection_ids: Vec::new(),
        face_group_ids: Vec::new(),
        loose_group_ids: Vec::new(),
        gps_lat,
        gps_lon,
        created_at: now.clone(),
        modified_at: now,
    };

    // Store in database atomically with parent index
    let db = state.db.write().map_err(|e| e.to_string())?;
    let serialized = serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
    db.insert_file_with_index(
        &file_id,
        serialized.as_str(),
        file_node.parent_id.as_deref(),
    )
    .map_err(|e| e.to_string())?;

    // Index in Tantivy for searchability
    let tantivy_index = state.tantivy_index.write().map_err(|e| e.to_string())?;
    let content_text = if !is_dir && size_bytes < 1024 * 1024 {
        // Read first 64KB for text file content indexing
        std::fs::read(path)
            .ok()
            .and_then(|data| String::from_utf8(data.iter().take(65536).copied().collect()).ok())
            .unwrap_or_default()
    } else {
        String::new()
    };

    if let Err(e) = tantivy_index.add_document(
        &file_id,
        &file_node.name,
        &content_text,
        &file_node.tags,
        &file_node.file_type,
        file_node.encrypted,
        file_node.gps_lat.is_some(),
        &file_node.created_at,
        file_node.hash_blake3.as_deref(),
    ) {
        log::warn!("Failed to index file {} in Tantivy: {}", file_id, e);
    }

    log::info!(
        "Imported {} ({}) in {}ms",
        file_path,
        size_bytes,
        start.elapsed().as_millis()
    );

    Ok(file_node)
}

/// Scan a directory and import all files/folders into the database.
/// Uses walkdir for robust traversal with depth limiting, symlink skipping,
/// and hidden file filtering. Batches Tantivy commits for performance.
#[tauri::command(async)]
pub fn scan_directory(
    dir_path: String,
    parent_id: String,
    recursive: bool,
    state: State<'_, AppState>,
) -> Result<ImportResult, String> {
    let start = std::time::Instant::now();
    let mut files_imported = 0u32;
    let mut folders_imported = 0u32;
    let mut errors = Vec::new();

    let dir = std::path::Path::new(&dir_path);
    if !dir.exists() {
        return Err(format!("Directory does not exist: {}", dir_path));
    }
    if !dir.is_dir() {
        return Err(format!("Path is not a directory: {}", dir_path));
    }

    let parent_path_for_children = if parent_id.is_empty() {
        dir_path.clone()
    } else {
        parent_id.clone()
    };

    // Collect all entries with walkdir
    let mut walker = walkdir::WalkDir::new(&dir_path);

    if !recursive {
        walker = walker.max_depth(1);
    } else {
        walker = walker.max_depth(20);
    }

    // Configure: skip symlinks, skip hidden files/dirs
    let walker = walker
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            // Skip hidden files and directories (names starting with '.')
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if name.starts_with('.') {
                return false;
            }
            true
        });

    // Collect all file nodes in a single pass, then batch-commit
    // We need to process directories in order (parents before children)
    // so the parent_id chain is correct.
    let tantivy_index = state.tantivy_index.write().map_err(|e| e.to_string())?;
    let db = state.db.write().map_err(|e| e.to_string())?;

    for entry_result in walker {
        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("Failed to read directory entry: {}", e));
                continue;
            }
        };

        let path = entry.path();
        let file_path_str = path.to_string_lossy().to_string();
        let depth = entry.depth();

        // Determine the parent for this entry:
        // - depth 0 is the root dir itself (skip)
        // - depth 1+ uses the dir_path as parent for the first level,
        //   and the folder's file_node ID for deeper levels
        // For simplicity, we use parent_path_for_children for all entries
        // at depth >= 1
        if depth == 0 {
            // This is the root directory being scanned — skip importing it
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                errors.push(format!(
                    "Failed to read metadata for {}: {}",
                    path.display(),
                    e
                ));
                continue;
            }
        };

        // Skip symlinks (shouldn't reach here due to follow_links(false), but be safe)
        if metadata.file_type().is_symlink() {
            continue;
        }

        let is_dir = metadata.is_dir();
        let size_bytes = metadata.len();
        let file_name = entry.file_name().to_string_lossy().to_string();

        // Detect MIME type
        let mime_type = if is_dir {
            None
        } else {
            infer::get_from_path(path)
                .ok()
                .flatten()
                .map(|m| m.mime_type())
                .map(|m| m.to_string())
                .or_else(|| mime_guess::from_path(path).first().map(|m| m.to_string()))
        };

        // Hash file contents with BLAKE3 (files only, < 100MB)
        let hash_blake3 = if !is_dir && size_bytes < 100 * 1024 * 1024 {
            match std::fs::read(path) {
                Ok(data) => Some(blake3::hash(&data).to_hex().to_string()),
                Err(_) => None,
            }
        } else {
            None
        };

        let now = Utc::now().to_rfc3339();
        let file_id = uuid::Uuid::new_v4().to_string();

        // Build context_data
        let mut context = serde_json::Map::new();
        context.insert(
            "original_path".to_string(),
            serde_json::json!(&file_path_str),
        );
        context.insert("source".to_string(), serde_json::json!("filesystem_import"));

        // Extract EXIF GPS for image files
        let (gps_lat, gps_lon) = if !is_dir {
            extract_gps_if_image(path)
        } else {
            (None, None)
        };

        if let (Some(lat), Some(lon)) = (gps_lat, gps_lon) {
            context.insert("gps_source".to_string(), serde_json::json!("exif"));
            context.insert("gps_lat".to_string(), serde_json::json!(lat));
            context.insert("gps_lon".to_string(), serde_json::json!(lon));
        }

        let file_node = FileNode {
            id: file_id.clone(),
            name: file_name,
            file_type: if is_dir { "folder" } else { "file" }.to_string(),
            parent_id: Some(parent_path_for_children.clone()),
            size_bytes,
            mime_type,
            hash_blake3: hash_blake3.clone(),
            encrypted: false,
            encryption_algorithm: None,
            compression_layers: Vec::new(),
            thumbnail_path: None,
            context_data: Some(serde_json::Value::Object(context)),
            tags: Vec::new(),
            collection_ids: Vec::new(),
            face_group_ids: Vec::new(),
            loose_group_ids: Vec::new(),
            gps_lat,
            gps_lon,
            created_at: now.clone(),
            modified_at: now,
        };

        // Store in database atomically with parent index
        let serialized = serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
        db.insert_file_with_index(
            &file_id,
            serialized.as_str(),
            Some(&parent_path_for_children),
        )
        .map_err(|e| e.to_string())?;

        // Index in Tantivy (no commit yet — batch at the end)
        let content_text = if !is_dir && size_bytes < 1024 * 1024 {
            std::fs::read(path)
                .ok()
                .and_then(|data| String::from_utf8(data.iter().take(65536).copied().collect()).ok())
                .unwrap_or_default()
        } else {
            String::new()
        };

        if let Err(e) = tantivy_index.add_document_no_commit(
            &file_id,
            &file_node.name,
            &content_text,
            &file_node.tags,
            &file_node.file_type,
            file_node.encrypted,
            file_node.gps_lat.is_some(),
            &file_node.created_at,
            file_node.hash_blake3.as_deref(),
        ) {
            log::warn!("Failed to index file {} in Tantivy (batch): {}", file_id, e);
        }

        if is_dir {
            folders_imported += 1;
        } else {
            files_imported += 1;
        }
    }

    // Single batch commit for all Tantivy documents
    if let Err(e) = tantivy_index.commit() {
        errors.push(format!("Failed to commit search index: {}", e));
    }

    Ok(ImportResult {
        files_imported,
        folders_imported,
        errors,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// Upload file bytes into the vault.
///
/// Two shapes (both callers are real):
/// - dialog/deep-link path: `{ filePath, parentPath }` — streams from disk,
///   never through IPC (the original contract);
/// - byte upload: `{ fileName, fileData, parentPath }` — drag-drop, file
///   pickers and provider→vault saves, which only ever have bytes in hand.
///   Bytes land under `./imports/{id}_{name}` (same layout as URL imports)
///   and then go through [`import_file`] so hashing, EXIF GPS and search
///   indexing behave exactly like every other import; the node is renamed
///   to the real file name afterwards (the stored basename is `{id}_{safe}`).
#[tauri::command(async)]
pub fn upload_file(
    parent_path: String,
    file_path: Option<String>,
    file_name: Option<String>,
    file_data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<UploadResult, String> {
    if let Some(path) = file_path {
        if path.trim().is_empty() {
            return Err("invalid: file_path is required".to_string());
        }
        // Delegate to import_file which already streams from disk
        let file_node = import_file(path, parent_path, state)?;
        let bytes_written = file_node.size_bytes;
        return Ok(UploadResult {
            file_node,
            bytes_written,
        });
    }

    let name = file_name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| "invalid: fileName is required".to_string())?;
    let data = file_data.ok_or_else(|| "invalid: fileData is required".to_string())?;

    let safe_name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let file_id = uuid::Uuid::new_v4().to_string();
    let stored_path = std::path::Path::new("imports").join(format!("{}_{}", file_id, safe_name));
    if let Some(parent) = stored_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("integrity: cannot create imports dir: {}", e))?;
    }
    std::fs::write(&stored_path, &data)
        .map_err(|e| format!("integrity: cannot store uploaded file: {}", e))?;

    let stored_str = stored_path.to_string_lossy().to_string();
    let imported = import_file(stored_str, parent_path, state.clone())?;
    // `import_file` names the node after the stored basename (`{id}_{safe}`);
    // restore the real upload name so the vault shows what the user dropped.
    let db = state.db.write().map_err(|e| e.to_string())?;
    let file_node =
        cybermanju_web::api::files::rename(&db, &imported.id, name).map_err(|e| e.to_string())?;
    let bytes_written = file_node.size_bytes;
    Ok(UploadResult {
        file_node,
        bytes_written,
    })
}

/// Rebuild the Tantivy search index from all FileNodes in the database.
/// Useful after bulk imports or database migrations.
#[tauri::command(async)]
pub fn rebuild_search_index(state: State<'_, AppState>) -> Result<u32, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(crate::db::Database::get_files_table())
        .map_err(|e| e.to_string())?;

    let tantivy_index = state.tantivy_index.write().map_err(|e| e.to_string())?;
    let mut count = 0u32;

    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let node: FileNode = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;

        // Read file content for text files (up to 64KB)
        let content_text = if node.file_type == "file" {
            node.context_data
                .as_ref()
                .and_then(|ctx| ctx.get("original_path").and_then(|v| v.as_str()))
                .and_then(|path| std::fs::read(path).ok())
                .and_then(|data| String::from_utf8(data.iter().take(65536).copied().collect()).ok())
                .unwrap_or_default()
        } else {
            String::new()
        };

        if let Err(e) = tantivy_index.add_document_no_commit(
            &node.id,
            &node.name,
            &content_text,
            &node.tags,
            &node.file_type,
            node.encrypted,
            node.gps_lat.is_some(),
            &node.created_at,
            node.hash_blake3.as_deref(),
        ) {
            log::warn!("Failed to index file {} during rebuild: {}", node.id, e);
        }
        count += 1;
    }

    // Single batch commit
    tantivy_index.commit().map_err(|e| e.to_string())?;

    Ok(count)
}

/// Import a file from a URL — downloads the file and processes it like a local import.
#[tauri::command(async)]
pub fn import_from_url(
    url: String,
    parent_path: String,
    state: State<'_, AppState>,
) -> Result<FileNode, String> {
    let start = std::time::Instant::now();

    // Download the file
    let response = reqwest::blocking::get(&url)
        .map_err(|e| format!("Failed to download URL '{}': {}", url, e))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Download failed with HTTP {}", status));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let bytes = response
        .bytes()
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    let size_bytes = bytes.len() as u64;

    // Detect file name from URL
    let url_path = url::Url::parse(&url)
        .map(|u| {
            u.path_segments()
                .and_then(|mut s| s.next_back().map(String::from))
                .unwrap_or_else(|| "download".to_string())
        })
        .unwrap_or_else(|_| "download".to_string());

    let file_name = if url_path.is_empty() || url_path == "/" {
        "download".to_string()
    } else {
        url_path.to_string()
    };

    let hash_blake3 = Some(blake3::hash(&bytes).to_hex().to_string());

    // Detect MIME type from content-type header or file extension
    let mime_type = content_type.or_else(|| {
        mime_guess::from_path(&file_name)
            .first()
            .map(|m| m.to_string())
    });

    let now = Utc::now().to_rfc3339();
    let file_id = uuid::Uuid::new_v4().to_string();

    // Persist the downloaded bytes so the FileNode describes a file that
    // actually exists (AUDIT F19). Layout: `./imports/{id}_{sanitized-name}`.
    let safe_name: String = file_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stored_path = std::path::Path::new("imports").join(format!("{}_{}", file_id, safe_name));
    if let Some(parent) = stored_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create imports dir: {}", e))?;
    }
    std::fs::write(&stored_path, &bytes)
        .map_err(|e| format!("Failed to store downloaded file: {}", e))?;

    let mut context = serde_json::Map::new();
    context.insert("source".to_string(), serde_json::json!("url_import"));
    context.insert("original_url".to_string(), serde_json::json!(url));
    context.insert(
        "original_path".to_string(),
        serde_json::json!(stored_path.to_string_lossy().to_string()),
    );

    let file_node = FileNode {
        id: file_id.clone(),
        name: file_name,
        file_type: "file".to_string(),
        parent_id: if parent_path.is_empty() {
            None
        } else {
            Some(parent_path)
        },
        size_bytes,
        mime_type,
        hash_blake3,
        encrypted: false,
        encryption_algorithm: None,
        compression_layers: Vec::new(),
        thumbnail_path: None,
        context_data: Some(serde_json::Value::Object(context)),
        tags: Vec::new(),
        collection_ids: Vec::new(),
        face_group_ids: Vec::new(),
        loose_group_ids: Vec::new(),
        gps_lat: None,
        gps_lon: None,
        created_at: now.clone(),
        modified_at: now,
    };

    // Store in database
    let db = state.db.write().map_err(|e| e.to_string())?;
    let serialized = serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
    db.insert_file_with_index(
        &file_id,
        serialized.as_str(),
        file_node.parent_id.as_deref(),
    )
    .map_err(|e| e.to_string())?;

    // Index in Tantivy
    let content_text =
        String::from_utf8(bytes.iter().take(65536).copied().collect()).unwrap_or_default();
    let tantivy_index = state.tantivy_index.write().map_err(|e| e.to_string())?;
    if let Err(e) = tantivy_index.add_document(
        &file_id,
        &file_node.name,
        &content_text,
        &file_node.tags,
        &file_node.file_type,
        file_node.encrypted,
        file_node.gps_lat.is_some(),
        &file_node.created_at,
        file_node.hash_blake3.as_deref(),
    ) {
        log::warn!("Failed to index URL-imported file {}: {}", file_id, e);
    }

    log::info!(
        "Imported from URL {} ({}) in {}ms",
        url,
        size_bytes,
        start.elapsed().as_millis()
    );
    Ok(file_node)
}

/// Extract GPS from an image file using the exif crate.
/// Returns (lat, lon) or (None, None) if no GPS data found.
fn extract_gps_if_image(path: &std::path::Path) -> (Option<f64>, Option<f64>) {
    // Only try for image files
    let is_image = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_lowercase().as_str(),
                "jpg" | "jpeg" | "tiff" | "tif" | "heic" | "heif" | "png" | "webp"
            )
        })
        .unwrap_or(false);

    if !is_image {
        return (None, None);
    }

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return (None, None),
    };

    let mut bufreader = std::io::BufReader::new(&file);
    let exif_data = match exif::Reader::new().read_from_container(&mut bufreader) {
        Ok(d) => d,
        Err(_) => return (None, None),
    };

    let latitude = exif_data
        .get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY)
        .and_then(|field| {
            if let exif::Value::Rational(ref rationals) = field.value {
                if rationals.len() >= 3 {
                    let deg = rationals[0].num as f64 / rationals[0].denom as f64;
                    let min = rationals[1].num as f64 / rationals[1].denom as f64;
                    let sec = rationals[2].num as f64 / rationals[2].denom as f64;
                    Some(deg + min / 60.0 + sec / 3600.0)
                } else {
                    None
                }
            } else {
                None
            }
        });

    let longitude = exif_data
        .get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY)
        .and_then(|field| {
            if let exif::Value::Rational(ref rationals) = field.value {
                if rationals.len() >= 3 {
                    let deg = rationals[0].num as f64 / rationals[0].denom as f64;
                    let min = rationals[1].num as f64 / rationals[1].denom as f64;
                    let sec = rationals[2].num as f64 / rationals[2].denom as f64;
                    Some(deg + min / 60.0 + sec / 3600.0)
                } else {
                    None
                }
            } else {
                None
            }
        });

    let lat_ref = exif_data
        .get_field(exif::Tag::GPSLatitudeRef, exif::In::PRIMARY)
        .and_then(|f| {
            if let exif::Value::Ascii(ref bytes) = f.value {
                bytes
                    .first()
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .map(|s| s.to_string())
            } else {
                None
            }
        });

    let lon_ref = exif_data
        .get_field(exif::Tag::GPSLongitudeRef, exif::In::PRIMARY)
        .and_then(|f| {
            if let exif::Value::Ascii(ref bytes) = f.value {
                bytes
                    .first()
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .map(|s| s.to_string())
            } else {
                None
            }
        });

    let final_lat = latitude.map(|lat| {
        if lat_ref.as_deref() == Some("S") {
            -lat
        } else {
            lat
        }
    });
    let final_lon = longitude.map(|lon| {
        if lon_ref.as_deref() == Some("W") {
            -lon
        } else {
            lon
        }
    });

    (final_lat, final_lon)
}
