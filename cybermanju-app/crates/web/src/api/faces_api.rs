// CyberManju OS — face detection over REST (shared by the Docker server).
//
// Same pipeline as the Tauri commands (`src-tauri/src/commands/faces.rs`)
// against the same `face_groups` table: detect → cosine-threshold match →
// group, batch → adaptive recluster. The detector is
// `cybermanju_faces::detect_faces_auto` (ONNX when the feature is on,
// otherwise the pure-Rust `heuristic-v2` skin segmentation), so Docker gets
// working face grouping with no model files — every result carries its
// engine and groups record it (`FaceGroup.detection_engine`).

use cybermanju_db::Database;
use cybermanju_types::schema::{FaceGroup, FileNode};
use redb::ReadableTable;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// Wire shape of the Tauri `FaceDetectionResult` (camelCase, same UI).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceDetectionResponse {
    pub file_id: String,
    pub faces_detected: usize,
    pub face_group_ids: Vec<String>,
    pub strategy_used: String,
    /// Which detector ran: `onnx`, `heuristic-v2`, or `none`.
    pub engine: String,
}

/// Wire shape of the Tauri `ReclusterResult`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResponse {
    pub clusters_created: usize,
    pub total_faces: usize,
    pub noise_faces: usize,
    pub avg_cohesion: f32,
    pub strategy_used: String,
    /// Distinct detection engines behind the clusters, sorted.
    pub detection_engines: Vec<String>,
}

fn read_node(db: &Database, file_id: &str) -> Result<FileNode, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_files_table())
        .map_err(|e| e.to_string())?;
    let value = table
        .get(file_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("not_found: file {file_id}"))?;
    serde_json::from_str(value.value()).map_err(|e| e.to_string())
}

/// File bytes on this machine (`context_data.original_path`, the same
/// source `files::read_content` serves the editor from).
fn load_rgb(node: &FileNode) -> Option<(Vec<u8>, u32, u32)> {
    let path = node
        .context_data
        .as_ref()?
        .get("original_path")?
        .as_str()
        .filter(|p| !p.is_empty())?;
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgb8();
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return None;
    }
    Some((img.into_raw(), w, h))
}

fn list_groups(db: &Database) -> Result<Vec<(String, FaceGroup)>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_face_groups_table())
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (key, value) = entry.map_err(|e| e.to_string())?;
        let group: FaceGroup = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        out.push((key.value().to_string(), group));
    }
    Ok(out)
}

/// Majority engine over member file ids — unanimous keeps it, else `mixed`.
fn cluster_engine(members: &[String], file_engines: &HashMap<String, String>) -> String {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for m in members {
        if let Some(e) = file_engines.get(m) {
            *counts.entry(e.as_str()).or_default() += 1;
        }
    }
    let mut best: Option<(&str, usize)> = None;
    let mut tied = false;
    for (engine, n) in counts {
        match best {
            None => best = Some((engine, n)),
            Some((_, top)) if n > top => {
                best = Some((engine, n));
                tied = false;
            }
            Some((_, top)) if n == top => {
                tied = true;
            }
            _ => {}
        }
    }
    match best {
        Some((engine, _)) if !tied => engine.to_string(),
        _ => "mixed".to_string(),
    }
}

/// Detect faces on one file and assign to groups (cosine-threshold match).
pub fn detect(db: &Database, file_id: &str) -> Result<FaceDetectionResponse, String> {
    let mut node = read_node(db, file_id)?;
    let (embeddings, engine) = cybermanju_faces::detect_faces_auto(&node, &|| load_rgb(&node));
    let engine = engine.to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let mut existing = list_groups(db)?;
    let mut face_group_ids = Vec::new();
    let mut created: Vec<FaceGroup> = Vec::new();

    for embedding in embeddings.iter() {
        let mut matched: Option<usize> = None;
        for (idx, (_, group)) in existing.iter().enumerate() {
            if let Some(ref centroid) = group.centroid_embedding {
                let dist = cybermanju_faces::embedding_distance(embedding, centroid);
                let threshold = cybermanju_faces::DEFAULT_MATCH_THRESHOLD
                    - (group.file_ids.len() as f32 * 0.005).min(0.1);
                if dist < threshold {
                    matched = Some(idx);
                    break;
                }
            }
        }
        if let Some(idx) = matched {
            let group = &mut existing[idx].1;
            if !group.file_ids.contains(&file_id.to_string()) {
                group.file_ids.push(file_id.to_string());
            }
            if group.detection_engine.as_deref() != Some(engine.as_str()) {
                group.detection_engine = Some("mixed".to_string());
            }
            if let Some(ref medoid) = group.centroid_embedding {
                group.centroid_embedding = Some(cybermanju_faces::update_medoid_incremental(
                    medoid,
                    &[],
                    embedding,
                ));
            } else {
                group.centroid_embedding = Some(embedding.clone());
            }
            if let Some(ref centroid) = group.centroid_embedding {
                group.binary_hash = Some(cybermanju_faces::to_binary_hash(centroid));
            }
            group.embedding_count += 1;
            face_group_ids.push(existing[idx].0.clone());
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            let name = format!("Person {}", id[..8].to_uppercase());
            face_group_ids.push(id.clone());
            created.push(FaceGroup {
                id,
                name,
                file_ids: vec![file_id.to_string()],
                centroid_embedding: Some(embedding.clone()),
                binary_hash: Some(cybermanju_faces::to_binary_hash(embedding)),
                cohesion: Some(0.0),
                embedding_count: 1,
                algorithm: Some("cosine_threshold".to_string()),
                detection_engine: Some(engine.clone()),
                created_at: now.clone(),
            });
        }
    }

    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_face_groups_table())
            .map_err(|e| e.to_string())?;
        for (id, group) in &existing {
            let raw = serde_json::to_string(group).map_err(|e| e.to_string())?;
            table
                .insert(id.as_str(), raw.as_str())
                .map_err(|e| e.to_string())?;
        }
        for group in &created {
            let raw = serde_json::to_string(group).map_err(|e| e.to_string())?;
            table
                .insert(group.id.as_str(), raw.as_str())
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;

    for gid in &face_group_ids {
        if !node.face_group_ids.contains(gid) {
            node.face_group_ids.push(gid.clone());
        }
    }
    node.modified_at = now;
    let raw = serde_json::to_string(&node).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_files_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(file_id, raw.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(FaceDetectionResponse {
        file_id: file_id.to_string(),
        faces_detected: embeddings.len(),
        face_group_ids,
        strategy_used: "cosine_threshold".to_string(),
        engine,
    })
}

/// Batch-detect every image and rebuild groups with adaptive clustering.
pub fn detect_batch(db: &Database) -> Result<BatchResponse, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_files_table())
        .map_err(|e| e.to_string())?;
    let mut images: Vec<FileNode> = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let node: FileNode = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        if node.file_type == "file" {
            if let Some(ref mime) = node.mime_type {
                if mime.starts_with("image/") {
                    images.push(node);
                }
            }
        }
    }
    drop(tx);

    let mut pairs: Vec<(String, Vec<f32>)> = Vec::new();
    let mut file_engines: HashMap<String, String> = HashMap::new();
    for node in &images {
        let (embeddings, engine) = cybermanju_faces::detect_faces_auto(node, &|| load_rgb(node));
        if embeddings.is_empty() {
            continue;
        }
        file_engines
            .entry(node.id.clone())
            .or_insert_with(|| engine.to_string());
        for emb in embeddings {
            pairs.push((node.id.clone(), emb));
        }
    }

    let clusters = cybermanju_faces::recluster_all(&pairs, None);

    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_face_groups_table())
            .map_err(|e| e.to_string())?;
        let keys: Vec<String> = table
            .iter()
            .map_err(|e| e.to_string())?
            .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_string()))
            .collect();
        for key in &keys {
            table.remove(key.as_str()).map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().to_rfc3339();
    let total_faces = pairs.len();
    let mut seen_files: HashSet<String> = HashSet::new();
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut fg = tx
            .open_table(Database::get_face_groups_table())
            .map_err(|e| e.to_string())?;
        let mut ft = tx
            .open_table(Database::get_files_table())
            .map_err(|e| e.to_string())?;
        for (idx, cluster) in clusters.iter().enumerate() {
            let id = uuid::Uuid::new_v4().to_string();
            let group = FaceGroup {
                id: id.clone(),
                name: format!("Person {}", idx + 1),
                file_ids: cluster.members.clone(),
                centroid_embedding: cluster.medoid.clone(),
                binary_hash: cluster
                    .medoid
                    .as_ref()
                    .map(|m| cybermanju_faces::to_binary_hash(m)),
                cohesion: Some(cluster.cohesion),
                embedding_count: cluster.members.len() as u32,
                algorithm: Some("hdbscan_adaptive".to_string()),
                detection_engine: Some(cluster_engine(&cluster.members, &file_engines)),
                created_at: now.clone(),
            };
            let raw = serde_json::to_string(&group).map_err(|e| e.to_string())?;
            fg.insert(id.as_str(), raw.as_str())
                .map_err(|e| e.to_string())?;
            for fid in &cluster.members {
                // Read the row into an owned value first: the redb read guard
                // must be dropped before `insert` takes `ft` mutably.
                let stored: Option<FileNode> = ft
                    .get(fid.as_str())
                    .map_err(|e| e.to_string())?
                    .map(|value| {
                        serde_json::from_str::<FileNode>(value.value()).map_err(|e| e.to_string())
                    })
                    .transpose()?;
                if let Some(mut node) = stored {
                    if !node.face_group_ids.contains(&id) {
                        node.face_group_ids.push(id.clone());
                    }
                    seen_files.insert(fid.clone());
                    let raw = serde_json::to_string(&node).map_err(|e| e.to_string())?;
                    ft.insert(fid.as_str(), raw.as_str())
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        // Files that lost all groups get a clean slate (mirror of desktop).
        for node in &images {
            if node.face_group_ids.is_empty() || seen_files.contains(&node.id) {
                continue;
            }
            // Same guard discipline as above: own the row before inserting.
            let stored: Option<FileNode> = ft
                .get(node.id.as_str())
                .map_err(|e| e.to_string())?
                .map(|value| {
                    serde_json::from_str::<FileNode>(value.value()).map_err(|e| e.to_string())
                })
                .transpose()?;
            if let Some(mut fresh) = stored {
                if !fresh.face_group_ids.is_empty() {
                    fresh.face_group_ids.clear();
                    let raw = serde_json::to_string(&fresh).map_err(|e| e.to_string())?;
                    ft.insert(node.id.as_str(), raw.as_str())
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;

    let noise_faces = total_faces - clusters.iter().map(|c| c.members.len()).sum::<usize>();
    let avg_cohesion = if clusters.is_empty() {
        0.0
    } else {
        clusters.iter().map(|c| c.cohesion).sum::<f32>() / clusters.len() as f32
    };
    let mut engines: Vec<String> = file_engines.into_values().collect();
    engines.sort();
    engines.dedup();
    Ok(BatchResponse {
        clusters_created: clusters.len(),
        total_faces,
        noise_faces,
        avg_cohesion,
        strategy_used: "adaptive".to_string(),
        detection_engines: engines,
    })
}
