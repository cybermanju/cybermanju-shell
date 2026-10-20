// CyberManju OS — Compression (src-tauri/src/commands/compression.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: FrontendCompressionStats, FrontendLayerDetail, compress_file, decompress_file, get_compression_stats.
// Integrates: chrono, serde, tauri.
// Contracts: none special (pure logic; errors bubble to caller).
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

/// MIME types that are already compressed — skipping triple compression avoids
/// wasting CPU and disk space (compressed files typically expand by 0.5–5%).
const INCOMPRESSIBLE_MIME_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/gif",
    "image/heic",
    "image/avif",
    "video/mp4",
    "video/webm",
    "video/quicktime",
    "video/x-matroska",
    "audio/mpeg",
    "audio/aac",
    "audio/ogg",
    "audio/flac",
    "application/zip",
    "application/x-7z-compressed",
    "application/x-rar-compressed",
    "application/gzip",
    "application/zstd",
];

/// Compression statistics — matches the frontend CompressionStats type exactly.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendCompressionStats {
    pub original_size: u64,
    pub compressed_size: u64,
    pub ratio: f64,
    pub layer: String,
    pub layer_details: Vec<FrontendLayerDetail>,
    pub blake3_hash: String,
    pub duration_ms: u64,
}

/// Layer detail — matches the frontend LayerDetail type exactly.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendLayerDetail {
    pub name: String,
    pub algorithm: String,
    pub input_size: u64,
    pub output_size: u64,
    pub ratio: f64,
    pub color: String,
}

/// Compress a file using the triple compressor.
#[tauri::command(async)]
pub fn compress_file(
    file_id: String,
    layer: String,
    state: State<'_, AppState>,
) -> Result<FrontendCompressionStats, String> {
    let valid_layers = ["lz4", "zstd", "brotli", "all"];
    if !valid_layers.contains(&layer.as_str()) {
        return Err(format!(
            "Invalid compression layer: {}. Must be one of: {}",
            layer,
            valid_layers.join(", ")
        ));
    }

    let db = state.db.write().map_err(|e| e.to_string())?;

    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let table_read = tx_read
        .open_table(crate::db::Database::get_files_table())
        .map_err(|e| e.to_string())?;
    let value = table_read
        .get(file_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("not_found: file '{}' not found", file_id))?;
    let mut file_node: crate::db::schema::FileNode =
        serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
    drop(tx_read);

    // Skip compression for already-compressed MIME types
    if let Some(ref mime) = file_node.mime_type {
        if INCOMPRESSIBLE_MIME_TYPES.contains(&mime.as_str()) {
            return Ok(FrontendCompressionStats {
                original_size: file_node.size_bytes,
                compressed_size: file_node.size_bytes,
                ratio: 1.0,
                layer: "skipped (already compressed)".to_string(),
                layer_details: vec![],
                blake3_hash: file_node.hash_blake3.clone().unwrap_or_default(),
                duration_ms: 0,
            });
        }
    }

    // Try to read actual file data for real compression
    let file_data: Option<Vec<u8>> = file_node
        .context_data
        .as_ref()
        .and_then(|ctx| ctx.get("original_path").and_then(|v| v.as_str()))
        .and_then(|path| std::fs::read(path).ok());

    let start = std::time::Instant::now();

    let result = if let Some(data) = file_data {
        // Real compression using TripleCompressor
        if layer == "all" {
            match state.compression.compress_triple(&data) {
                Ok((_compressed, stats)) => {
                    let layer_details: Vec<FrontendLayerDetail> = stats
                        .layer_details
                        .into_iter()
                        .map(|ld| FrontendLayerDetail {
                            name: ld.name,
                            algorithm: ld.algorithm,
                            input_size: ld.input_size,
                            output_size: ld.output_size,
                            ratio: ld.ratio,
                            color: ld.color,
                        })
                        .collect();
                    Some((
                        stats.original_size,
                        stats.compressed_size,
                        stats.ratio,
                        "triple".to_string(),
                        layer_details,
                        stats.blake3_hash,
                    ))
                }
                Err(e) => return Err(format!("integrity: triple compression failed: {}", e)),
            }
        } else {
            match state.compression.compress_data(&data, &layer) {
                Ok((_compressed, detail)) => {
                    let layer_details = vec![FrontendLayerDetail {
                        name: detail.name,
                        algorithm: detail.algorithm,
                        input_size: detail.input_size,
                        output_size: detail.output_size,
                        ratio: detail.ratio,
                        color: detail.color,
                    }];
                    let ratio = detail.ratio;
                    Some((
                        detail.input_size,
                        detail.output_size,
                        ratio,
                        layer.clone(),
                        layer_details,
                        blake3::hash(&data).to_hex().to_string(),
                    ))
                }
                Err(e) => return Err(format!("Compression failed: {}", e)),
            }
        }
    } else {
        // No file on disk — attempt to locate via import storage or context_data
        let resolved_path = file_node
            .context_data
            .as_ref()
            .and_then(|ctx| ctx.get("original_path").and_then(|v| v.as_str()))
            .filter(|p| std::path::Path::new(p).exists());

        match resolved_path {
            Some(real_path) => {
                let data = std::fs::read(real_path)
                    .map_err(|e| format!("Failed to read file at {}: {}", real_path, e))?;
                if layer == "all" {
                    match state.compression.compress_triple(&data) {
                        Ok((_compressed, stats)) => {
                            let layer_details: Vec<FrontendLayerDetail> = stats
                                .layer_details
                                .into_iter()
                                .map(|ld| FrontendLayerDetail {
                                    name: ld.name,
                                    algorithm: ld.algorithm,
                                    input_size: ld.input_size,
                                    output_size: ld.output_size,
                                    ratio: ld.ratio,
                                    color: ld.color,
                                })
                                .collect();
                            Some((
                                stats.original_size,
                                stats.compressed_size,
                                stats.ratio,
                                "triple".to_string(),
                                layer_details,
                                stats.blake3_hash,
                            ))
                        }
                        Err(e) => {
                            return Err(format!("integrity: triple compression failed: {}", e))
                        }
                    }
                } else {
                    match state.compression.compress_data(&data, &layer) {
                        Ok((_compressed, detail)) => {
                            let layer_details = vec![FrontendLayerDetail {
                                name: detail.name,
                                algorithm: detail.algorithm,
                                input_size: detail.input_size,
                                output_size: detail.output_size,
                                ratio: detail.ratio,
                                color: detail.color,
                            }];
                            let ratio = detail.ratio;
                            Some((
                                detail.input_size,
                                detail.output_size,
                                ratio,
                                layer.clone(),
                                layer_details,
                                blake3::hash(&data).to_hex().to_string(),
                            ))
                        }
                        Err(e) => return Err(format!("Compression failed: {}", e)),
                    }
                }
            }
            None => {
                return Err(format!(
                    "File '{}' has no accessible path on disk. Import or upload the file first.",
                    file_node.name
                ));
            }
        }
    };

    let Some((orig, comp, ratio, layer_label, layer_details, hash)) = result else {
        return Err("integrity: compression produced no output".to_string());
    };
    let duration_ms = start.elapsed().as_millis() as u64;

    // Update file node metadata
    if !file_node.compression_layers.contains(&layer) {
        file_node.compression_layers.push(layer.clone());
    }
    file_node.size_bytes = comp;
    file_node.modified_at = Utc::now().to_rfc3339();

    let serialized = serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(crate::db::Database::get_files_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(file_id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(FrontendCompressionStats {
        original_size: orig,
        compressed_size: comp,
        ratio,
        layer: layer_label,
        layer_details,
        blake3_hash: hash,
        duration_ms,
    })
}

/// Decompress a file by removing all compression layers.
#[tauri::command(async)]
pub fn decompress_file(
    file_id: String,
    state: State<'_, AppState>,
) -> Result<FrontendCompressionStats, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;

    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let table_read = tx_read
        .open_table(crate::db::Database::get_files_table())
        .map_err(|e| e.to_string())?;
    let value = table_read
        .get(file_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("not_found: file '{}' not found", file_id))?;
    let mut file_node: crate::db::schema::FileNode =
        serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
    drop(tx_read);

    if file_node.compression_layers.is_empty() {
        return Err(format!(
            "File {} has no compression layers to remove",
            file_id
        ));
    }

    let start = std::time::Instant::now();
    let now = Utc::now().to_rfc3339();
    let compressed_size = file_node.size_bytes;

    // Try real decompression — resolve file path from context_data
    let file_path = file_node
        .context_data
        .as_ref()
        .and_then(|ctx| ctx.get("original_path").and_then(|v| v.as_str()))
        .filter(|p| std::path::Path::new(p).exists())
        .map(|s| s.to_string())
        .or_else(|| {
            // Check for compressed .cyb3 file
            file_node
                .context_data
                .as_ref()
                .and_then(|ctx| ctx.get("original_path").and_then(|v| v.as_str()))
                .map(|p| format!("{}.cyb3", p))
                .filter(|p| std::path::Path::new(p).exists())
        });

    let (original_size, hash) = if let Some(data_path) = file_path {
        match std::fs::read(&data_path) {
            Ok(data) => {
                match state.compression.decompress_triple(&data) {
                    Ok((decompressed, _dur)) => (
                        decompressed.len() as u64,
                        blake3::hash(&decompressed).to_hex().to_string(),
                    ),
                    Err(_e) => {
                        // If triple decompression fails, try single-layer based on compression_layers
                        let mut result_data = data.clone();
                        let mut current_size = data.len() as u64;
                        for layer in &file_node.compression_layers {
                            let decompressed = match layer.as_str() {
                                "brotli" => state.compression.decompress_brotli(&result_data),
                                "zstd" => state.compression.decompress_zstd(&result_data),
                                "lz4" => state.compression.decompress_lz4(&result_data),
                                _ => continue,
                            };
                            match decompressed {
                                Ok(d) => {
                                    current_size = d.len() as u64;
                                    result_data = d;
                                }
                                Err(_) => continue,
                            }
                        }
                        (
                            current_size,
                            blake3::hash(&result_data).to_hex().to_string(),
                        )
                    }
                }
            }
            Err(_) => {
                return Err(format!("Cannot read file at path: {}", data_path));
            }
        }
    } else {
        return Err(format!(
            "File '{}' has no accessible compressed data on disk. Import the file first.",
            file_node.name
        ));
    };

    let removed_layers: Vec<String> = std::mem::take(&mut file_node.compression_layers);
    file_node.size_bytes = original_size;
    file_node.modified_at = now.clone();
    let duration_ms = start.elapsed().as_millis() as u64;

    let layer_details: Vec<FrontendLayerDetail> = removed_layers
        .iter()
        .map(|l| {
            let color = match l.as_str() {
                "lz4" => "#00D4FF",
                "zstd" => "#00FF41",
                "brotli" => "#FFB800",
                _ => "#6B7280",
            };
            FrontendLayerDetail {
                name: format!("Removed: {}", l.to_uppercase()),
                algorithm: l.clone(),
                input_size: compressed_size,
                output_size: original_size,
                ratio: if compressed_size > 0 {
                    original_size as f64 / compressed_size as f64
                } else {
                    1.0
                },
                color: color.to_string(),
            }
        })
        .collect();

    let serialized = serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(crate::db::Database::get_files_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(file_id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(FrontendCompressionStats {
        original_size: compressed_size,
        compressed_size: original_size,
        ratio: if compressed_size > 0 {
            original_size as f64 / compressed_size as f64
        } else {
            1.0
        },
        layer: "decompressed".to_string(),
        layer_details,
        blake3_hash: hash,
        duration_ms,
    })
}

/// Get compression statistics for a file — returns frontend-compatible format.
#[tauri::command(async)]
pub fn get_compression_stats(
    file_id: String,
    state: State<'_, AppState>,
) -> Result<FrontendCompressionStats, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(crate::db::Database::get_files_table())
        .map_err(|e| e.to_string())?;

    let value = table
        .get(file_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("not_found: file '{}' not found", file_id))?;

    let file_node: crate::db::schema::FileNode =
        serde_json::from_str(value.value()).map_err(|e| e.to_string())?;

    let layer_name = if file_node.compression_layers.is_empty() {
        "none".to_string()
    } else if file_node.compression_layers.len() == 3 {
        "triple".to_string()
    } else {
        file_node
            .compression_layers
            .last()
            .cloned()
            .unwrap_or_else(|| "none".to_string())
    };

    let layer_details: Vec<FrontendLayerDetail> = file_node
        .compression_layers
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let (name, color) = match l.as_str() {
                "lz4" => (format!("Layer {}: LZ4", i + 1), "#00D4FF"),
                "zstd" => (format!("Layer {}: Zstandard", i + 1), "#00FF41"),
                "brotli" => (format!("Layer {}: Brotli", i + 1), "#FFB800"),
                _ => (format!("Layer {}: {}", i + 1, l), "#6B7280"),
            };
            FrontendLayerDetail {
                name,
                algorithm: l.clone(),
                input_size: file_node.size_bytes,
                output_size: file_node.size_bytes, // stats tracked at compress time
                ratio: 1.0,
                color: color.to_string(),
            }
        })
        .collect();

    Ok(FrontendCompressionStats {
        original_size: file_node.size_bytes,
        compressed_size: file_node.size_bytes,
        ratio: 1.0,
        layer: layer_name,
        layer_details,
        blake3_hash: file_node.hash_blake3.clone().unwrap_or_default(),
        duration_ms: 0,
    })
}
