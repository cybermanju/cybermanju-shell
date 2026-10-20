// CyberManju OS — Faces (src-tauri/src/commands/faces.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: FaceDetectionResult, ReclusterResult, detect_faces, detect_faces_batch_cmd, recluster_faces, rename_face_group, merge_face_groups, delete_face_group.
// Integrates: chrono, redb, serde, tauri.
// Contracts: signals: engine.
use chrono::Utc;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::State;

use crate::db::schema::{FaceGroup, FileNode};
use crate::faces::{
    detect_faces_batch_with_engines, embedding_distance, recluster_all, to_binary_hash,
    update_medoid_incremental, ClusteringStrategy, DEFAULT_MATCH_THRESHOLD,
};
use crate::AppState;

/// Majority detection engine over a cluster's member file ids — unanimous
/// keeps the engine, anything else is honestly `mixed`.
fn cluster_engine(
    members: &[String],
    file_engines: &std::collections::HashMap<String, String>,
) -> String {
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for m in members {
        if let Some(e) = file_engines.get(m) {
            *counts.entry(e.as_str()).or_default() += 1;
        }
    }
    let mut best: Option<(&str, usize)> = None;
    let mut tied = false;
    for (engine, n) in counts {
        match best {
            None => {
                best = Some((engine, n));
            }
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

/// Sorted unique engine list for batch responses.
fn sorted_engines(file_engines: &std::collections::HashMap<String, String>) -> Vec<String> {
    let mut out: Vec<String> = file_engines.values().cloned().collect();
    out.sort();
    out.dedup();
    out
}

/// Result of face detection for a single file.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceDetectionResult {
    pub file_id: String,
    pub faces_detected: usize,
    pub face_group_ids: Vec<String>,
    pub strategy_used: String,
    /// Which detector ran: `onnx`, `heuristic-v1`, or `none` (no bytes).
    pub engine: String,
}

/// Cluster info returned by recluster command.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReclusterResult {
    pub clusters_created: usize,
    pub total_faces: usize,
    pub noise_faces: usize,
    pub avg_cohesion: f32,
    pub strategy_used: String,
    /// Distinct detection engines behind the clusters (`onnx` /
    /// `heuristic-v1` / `none`), sorted — the UI prints them verbatim.
    #[serde(default)]
    pub detection_engines: Vec<String>,
}

/// Detect faces on a single file and assign to groups.
#[tauri::command(async)]
pub fn detect_faces(
    file_id: String,
    state: State<'_, AppState>,
) -> Result<FaceDetectionResult, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;

    // Verify file exists
    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let file_table = tx_read
        .open_table(crate::db::Database::get_files_table())
        .map_err(|e| e.to_string())?;
    let file_value = file_table
        .get(file_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("File not found: {}", file_id))?;
    let mut file_node: FileNode =
        serde_json::from_str(file_value.value()).map_err(|e| e.to_string())?;
    drop(tx_read);

    // Delegate to faces module for detection + embedding. The engine label
    // travels with the result so groups (and the UI) say which detector ran.
    let (detected_faces, engine) = crate::faces::detect_faces_in_file_with_engine(&file_node);
    let engine = engine.to_string();

    let now = Utc::now().to_rfc3339();

    // Load all existing face groups once (avoid per-face DB roundtrips)
    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let fg_table = tx_read
        .open_table(crate::db::Database::get_face_groups_table())
        .map_err(|e| e.to_string())?;

    let mut existing_groups: Vec<(String, FaceGroup)> = Vec::new();
    for entry in fg_table.iter().map_err(|e| e.to_string())? {
        let (key, value) = entry.map_err(|e| e.to_string())?;
        let group: FaceGroup = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        existing_groups.push((key.value().to_string(), group));
    }
    drop(tx_read);

    // Track any new groups to create
    let mut new_groups: Vec<FaceGroup> = Vec::new();
    let mut face_group_ids = Vec::new();

    // Process each detected face
    for embedding in detected_faces.iter() {
        // Try to match against existing face groups (in-memory)
        let mut matched_group_idx: Option<usize> = None;

        for (idx, (_group_id, group)) in existing_groups.iter().enumerate() {
            if let Some(ref centroid) = group.centroid_embedding {
                let dist = embedding_distance(embedding, centroid);
                let group_threshold =
                    DEFAULT_MATCH_THRESHOLD - (group.file_ids.len() as f32 * 0.005).min(0.1);
                if dist < group_threshold {
                    matched_group_idx = Some(idx);
                    break;
                }
            }
        }

        if let Some(idx) = matched_group_idx {
            let group = &mut existing_groups[idx].1;

            if !group.file_ids.contains(&file_id) {
                group.file_ids.push(file_id.clone());
            }

            // A group matched across engines is honestly `mixed`.
            if group.detection_engine.as_deref() != Some(engine.as_str()) {
                group.detection_engine = Some("mixed".to_string());
            }

            // Incremental medoid update
            if let Some(ref current_medoid) = group.centroid_embedding {
                group.centroid_embedding =
                    Some(update_medoid_incremental(current_medoid, &[], embedding));
            } else {
                group.centroid_embedding = Some(embedding.clone());
            }

            // Update binary hash from new centroid
            if let Some(ref centroid) = group.centroid_embedding {
                group.binary_hash = Some(to_binary_hash(centroid));
            }
            group.embedding_count += 1;

            face_group_ids.push(existing_groups[idx].0.clone());
        } else {
            // Create a new face group
            let new_group_id = uuid::Uuid::new_v4().to_string();
            let group_name = format!("Person {}", new_group_id[..8].to_uppercase());
            let binary_hash = to_binary_hash(embedding);

            let new_group = FaceGroup {
                id: new_group_id.clone(),
                name: group_name,
                file_ids: vec![file_id.clone()],
                centroid_embedding: Some(embedding.clone()),
                binary_hash: Some(binary_hash),
                cohesion: Some(0.0),
                embedding_count: 1,
                algorithm: Some("cosine_threshold".to_string()),
                detection_engine: Some(engine.clone()),
                created_at: now.clone(),
            };

            face_group_ids.push(new_group_id.clone());
            new_groups.push(new_group);
        }
    }

    // Single write transaction for all updates
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut fg_table = tx
            .open_table(crate::db::Database::get_face_groups_table())
            .map_err(|e| e.to_string())?;

        // Write updated existing groups
        for (group_id, group) in &existing_groups {
            let serialized = serde_json::to_string(group).map_err(|e| e.to_string())?;
            fg_table
                .insert(group_id.as_str(), serialized.as_str())
                .map_err(|e| e.to_string())?;
        }

        // Write new groups
        for group in &new_groups {
            let serialized = serde_json::to_string(group).map_err(|e| e.to_string())?;
            fg_table
                .insert(group.id.as_str(), serialized.as_str())
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;

    // Update the file node with face group references
    for gid in &face_group_ids {
        if !file_node.face_group_ids.contains(gid) {
            file_node.face_group_ids.push(gid.clone());
        }
    }
    file_node.modified_at = now;

    let file_serialized = serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut ft = tx
            .open_table(crate::db::Database::get_files_table())
            .map_err(|e| e.to_string())?;
        ft.insert(file_id.as_str(), file_serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(FaceDetectionResult {
        file_id,
        faces_detected: detected_faces.len(),
        face_group_ids,
        strategy_used: "cosine_threshold".to_string(),
        engine,
    })
}

/// Batch detect faces across all image files.
#[tauri::command(async)]
pub fn detect_faces_batch_cmd(state: State<'_, AppState>) -> Result<ReclusterResult, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;

    // Collect all image file nodes
    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let file_table = tx_read
        .open_table(crate::db::Database::get_files_table())
        .map_err(|e| e.to_string())?;

    let mut image_files: Vec<FileNode> = Vec::new();
    for entry in file_table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let node: FileNode = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        if node.file_type == "file" {
            if let Some(ref mime) = node.mime_type {
                if mime.starts_with("image/") {
                    image_files.push(node);
                }
            }
        }
    }
    drop(tx_read);

    // Detect faces in parallel (engines travel alongside for labeling).
    let results = detect_faces_batch_with_engines(&image_files);

    // Collect all embeddings with file IDs (+ engine per file for groups).
    let mut all_embeddings: Vec<(String, Vec<f32>)> = Vec::new();
    let mut file_engines: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (file_id, embeddings, engine) in &results {
        file_engines
            .entry(file_id.clone())
            .or_insert_with(|| engine.to_string());
        for emb in embeddings {
            all_embeddings.push((file_id.clone(), emb.clone()));
        }
    }

    // Re-cluster with adaptive threshold
    let clusters = recluster_all(&all_embeddings, None);

    // Clear existing face groups
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut fg_table = tx
            .open_table(crate::db::Database::get_face_groups_table())
            .map_err(|e| e.to_string())?;
        // Remove all entries
        let keys: Vec<String> = fg_table
            .iter()
            .map_err(|e| e.to_string())?
            .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_string()))
            .collect();
        for key in &keys {
            fg_table.remove(key.as_str()).map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;

    // Write new clusters as face groups
    let now = Utc::now().to_rfc3339();
    let total_faces = all_embeddings.len();
    let noise_faces = total_faces - clusters.iter().map(|c| c.members.len()).sum::<usize>();

    // First pass: clear all stale face_group_ids from file nodes
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut ft_table = tx
            .open_table(crate::db::Database::get_files_table())
            .map_err(|e| e.to_string())?;
        let file_keys: Vec<String> = ft_table
            .iter()
            .map_err(|e| e.to_string())?
            .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_string()))
            .collect();
        for fid in &file_keys {
            let node_str = ft_table
                .get(fid.as_str())
                .map_err(|e| e.to_string())?
                .map(|fv| fv.value().to_string());
            if let Some(ref node_str) = node_str {
                let mut file_node: FileNode =
                    serde_json::from_str(node_str).map_err(|e| e.to_string())?;
                if !file_node.face_group_ids.is_empty() {
                    file_node.face_group_ids.clear();
                    let file_serialized =
                        serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
                    ft_table
                        .insert(fid.as_str(), file_serialized.as_str())
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;

    // Second pass: write new face groups and update file node references
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut fg_table = tx
            .open_table(crate::db::Database::get_face_groups_table())
            .map_err(|e| e.to_string())?;
        let mut ft_table = tx
            .open_table(crate::db::Database::get_files_table())
            .map_err(|e| e.to_string())?;

        for (idx, cluster) in clusters.iter().enumerate() {
            let group_id = uuid::Uuid::new_v4().to_string();
            let group_name = format!("Person {}", idx + 1);

            let face_group = FaceGroup {
                id: group_id.clone(),
                name: group_name,
                file_ids: cluster.members.clone(),
                centroid_embedding: cluster.medoid.clone(),
                binary_hash: cluster.medoid.as_ref().map(|m| to_binary_hash(m)),
                cohesion: Some(cluster.cohesion),
                embedding_count: cluster.members.len() as u32,
                algorithm: Some("hdbscan_adaptive".to_string()),
                detection_engine: Some(cluster_engine(&cluster.members, &file_engines)),
                created_at: now.clone(),
            };

            let serialized = serde_json::to_string(&face_group).map_err(|e| e.to_string())?;
            fg_table
                .insert(group_id.as_str(), serialized.as_str())
                .map_err(|e| e.to_string())?;

            // Update file nodes to reference this face group
            for file_id in &cluster.members {
                let node_str = ft_table
                    .get(file_id.as_str())
                    .map_err(|e| e.to_string())?
                    .map(|fv| fv.value().to_string());
                if let Some(ref node_str) = node_str {
                    let mut file_node: FileNode =
                        serde_json::from_str(node_str).map_err(|e| e.to_string())?;
                    if !file_node.face_group_ids.contains(&group_id) {
                        file_node.face_group_ids.push(group_id.clone());
                    }
                    let file_serialized =
                        serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
                    ft_table
                        .insert(file_id.as_str(), file_serialized.as_str())
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;

    let avg_cohesion = if clusters.is_empty() {
        0.0
    } else {
        clusters.iter().map(|c| c.cohesion).sum::<f32>() / clusters.len() as f32
    };

    Ok(ReclusterResult {
        clusters_created: clusters.len(),
        total_faces,
        noise_faces,
        avg_cohesion,
        strategy_used: "adaptive".to_string(),
        detection_engines: sorted_engines(&file_engines),
    })
}

/// Re-cluster all faces using a specific strategy.
#[tauri::command(async)]
pub fn recluster_faces(
    strategy: Option<String>,
    state: State<'_, AppState>,
) -> Result<ReclusterResult, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;

    // Collect all image file nodes from face groups
    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let fg_table = tx_read
        .open_table(crate::db::Database::get_face_groups_table())
        .map_err(|e| e.to_string())?;

    // Collect unique file IDs from all face groups
    let mut file_ids_seen: HashSet<String> = HashSet::new();
    for entry in fg_table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let group: FaceGroup = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        for fid in &group.file_ids {
            file_ids_seen.insert(fid.clone());
        }
    }

    // Batch-load all file nodes in a single read transaction
    let file_table = tx_read
        .open_table(crate::db::Database::get_files_table())
        .map_err(|e| e.to_string())?;
    let mut file_nodes: Vec<FileNode> = Vec::new();
    for fid in &file_ids_seen {
        if let Some(fv) = file_table.get(fid.as_str()).map_err(|e| e.to_string())? {
            let file_node: FileNode =
                serde_json::from_str(fv.value()).map_err(|e| e.to_string())?;
            file_nodes.push(file_node);
        }
    }
    drop(tx_read);

    // Re-detect faces for each file to get actual per-face embeddings
    let mut all_embeddings: Vec<(String, Vec<f32>)> = Vec::new();
    let mut file_engines: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for file_node in &file_nodes {
        let (embeddings, engine) = crate::faces::detect_faces_in_file_with_engine(file_node);
        if embeddings.is_empty() {
            continue;
        }
        file_engines
            .entry(file_node.id.clone())
            .or_insert_with(|| engine.to_string());
        for emb in embeddings {
            all_embeddings.push((file_node.id.clone(), emb));
        }
    }

    // Parse strategy
    let strat = match strategy.as_deref() {
        Some("bruteforce") => Some(ClusteringStrategy::BruteForce),
        Some("simhash") => Some(ClusteringStrategy::SimHash),
        Some("chinese_whispers") => Some(ClusteringStrategy::ChineseWhispers),
        Some("hdbscan") => Some(ClusteringStrategy::HDBSCAN),
        _ => None,
    };

    let clusters = recluster_all(&all_embeddings, strat);

    // Clear and rewrite
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut fg_table = tx
            .open_table(crate::db::Database::get_face_groups_table())
            .map_err(|e| e.to_string())?;
        let keys: Vec<String> = fg_table
            .iter()
            .map_err(|e| e.to_string())?
            .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_string()))
            .collect();
        for key in &keys {
            fg_table.remove(key.as_str()).map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;

    // Clear stale face_group_ids from all file nodes
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut ft_table = tx
            .open_table(crate::db::Database::get_files_table())
            .map_err(|e| e.to_string())?;
        let file_keys: Vec<String> = ft_table
            .iter()
            .map_err(|e| e.to_string())?
            .filter_map(|entry| entry.ok().map(|(k, _)| k.value().to_string()))
            .collect();
        for fid in &file_keys {
            let node_str = ft_table
                .get(fid.as_str())
                .map_err(|e| e.to_string())?
                .map(|fv| fv.value().to_string());
            if let Some(ref node_str) = node_str {
                let mut file_node: FileNode =
                    serde_json::from_str(node_str).map_err(|e| e.to_string())?;
                if !file_node.face_group_ids.is_empty() {
                    file_node.face_group_ids.clear();
                    let file_serialized =
                        serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
                    ft_table
                        .insert(fid.as_str(), file_serialized.as_str())
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;

    let now = Utc::now().to_rfc3339();
    let total_faces = all_embeddings.len();
    let noise_faces = total_faces - clusters.iter().map(|c| c.members.len()).sum::<usize>();

    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut fg_table = tx
            .open_table(crate::db::Database::get_face_groups_table())
            .map_err(|e| e.to_string())?;
        let mut ft_table = tx
            .open_table(crate::db::Database::get_files_table())
            .map_err(|e| e.to_string())?;

        for (idx, cluster) in clusters.iter().enumerate() {
            let group_id = uuid::Uuid::new_v4().to_string();
            let group_name = format!("Person {}", idx + 1);

            let face_group = FaceGroup {
                id: group_id.clone(),
                name: group_name,
                file_ids: cluster.members.clone(),
                centroid_embedding: cluster.medoid.clone(),
                binary_hash: cluster.medoid.as_ref().map(|m| to_binary_hash(m)),
                cohesion: Some(cluster.cohesion),
                embedding_count: cluster.members.len() as u32,
                algorithm: Some(format!(
                    "{:?}",
                    strat.unwrap_or(ClusteringStrategy::HDBSCAN)
                )),
                detection_engine: Some(cluster_engine(&cluster.members, &file_engines)),
                created_at: now.clone(),
            };

            let serialized = serde_json::to_string(&face_group).map_err(|e| e.to_string())?;
            fg_table
                .insert(group_id.as_str(), serialized.as_str())
                .map_err(|e| e.to_string())?;

            // Update file nodes to reference this face group
            for file_id in &cluster.members {
                let node_str = ft_table
                    .get(file_id.as_str())
                    .map_err(|e| e.to_string())?
                    .map(|fv| fv.value().to_string());
                if let Some(ref node_str) = node_str {
                    let mut file_node: FileNode =
                        serde_json::from_str(node_str).map_err(|e| e.to_string())?;
                    if !file_node.face_group_ids.contains(&group_id) {
                        file_node.face_group_ids.push(group_id.clone());
                    }
                    let file_serialized =
                        serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
                    ft_table
                        .insert(file_id.as_str(), file_serialized.as_str())
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;

    let avg_cohesion = if clusters.is_empty() {
        0.0
    } else {
        clusters.iter().map(|c| c.cohesion).sum::<f32>() / clusters.len() as f32
    };

    Ok(ReclusterResult {
        clusters_created: clusters.len(),
        total_faces,
        noise_faces,
        avg_cohesion,
        strategy_used: format!("{:?}", strat.unwrap_or(ClusteringStrategy::HDBSCAN)),
        detection_engines: sorted_engines(&file_engines),
    })
}

/// Rename a face group.
#[tauri::command(async)]
pub fn rename_face_group(
    group_id: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(crate::db::Database::get_face_groups_table())
        .map_err(|e| e.to_string())?;
    let value = table
        .get(group_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Face group not found: {}", group_id))?;
    let mut group: FaceGroup = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
    drop(tx);

    group.name = new_name;

    let serialized = serde_json::to_string(&group).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut wt = tx
            .open_table(crate::db::Database::get_face_groups_table())
            .map_err(|e| e.to_string())?;
        wt.insert(group_id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    let _ = db.record_mutation_op(
        "face-rename",
        None,
        serde_json::json!({ "group": group_id, "name": group.name }),
    );
    Ok(())
}

/// Merge two face groups into one.
#[tauri::command(async)]
pub fn merge_face_groups(
    source_group_id: String,
    target_group_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.db.write().map_err(|e| e.to_string())?;

    // Read source group
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(crate::db::Database::get_face_groups_table())
        .map_err(|e| e.to_string())?;
    let source_val = table
        .get(source_group_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Source group not found: {}", source_group_id))?;
    let source: FaceGroup = serde_json::from_str(source_val.value()).map_err(|e| e.to_string())?;

    let target_val = table
        .get(target_group_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Target group not found: {}", target_group_id))?;
    let mut target: FaceGroup =
        serde_json::from_str(target_val.value()).map_err(|e| e.to_string())?;
    drop(tx);

    // Merge: add source file IDs to target (dedup)
    let source_size = source.file_ids.len();
    let target_original_size = target.file_ids.len();
    for fid in &source.file_ids {
        if !target.file_ids.contains(fid) {
            target.file_ids.push(fid.clone());
        }
    }
    target.embedding_count = target.file_ids.len() as u32;

    // Update centroid: pick the original group with more members (more representative medoid)
    if source_size > target_original_size {
        // Source was larger, use its centroid
        target.centroid_embedding = source.centroid_embedding;
    }
    // else: keep target centroid (target was larger or equal)

    // Update binary hash
    if let Some(ref centroid) = target.centroid_embedding {
        target.binary_hash = Some(to_binary_hash(centroid));
    }

    // Write merged target and update FileNode references
    let serialized = serde_json::to_string(&target).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut wt = tx
            .open_table(crate::db::Database::get_face_groups_table())
            .map_err(|e| e.to_string())?;
        wt.insert(target_group_id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
        // Remove source group
        wt.remove(source_group_id.as_str())
            .map_err(|e| e.to_string())?;

        // Update FileNode references: replace source_group_id with target_group_id
        let mut ft_table = tx
            .open_table(crate::db::Database::get_files_table())
            .map_err(|e| e.to_string())?;
        for fid in &source.file_ids {
            let node_str = ft_table
                .get(fid.as_str())
                .map_err(|e| e.to_string())?
                .map(|fv| fv.value().to_string());
            if let Some(ref node_str) = node_str {
                let mut file_node: FileNode =
                    serde_json::from_str(node_str).map_err(|e| e.to_string())?;
                // Remove source reference, add target reference
                file_node.face_group_ids.retain(|id| id != &source_group_id);
                if !file_node.face_group_ids.contains(&target_group_id) {
                    file_node.face_group_ids.push(target_group_id.clone());
                }
                let file_serialized =
                    serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
                ft_table
                    .insert(fid.as_str(), file_serialized.as_str())
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    let _ = db.record_mutation_op(
        "face-rename",
        None,
        serde_json::json!({ "merged": source_group_id, "into": target_group_id }),
    );

    Ok(())
}

/// Delete a face group.
#[tauri::command(async)]
pub fn delete_face_group(group_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.write().map_err(|e| e.to_string())?;

    // Read the group to get its file_ids before deleting
    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let fg_table = tx_read
        .open_table(crate::db::Database::get_face_groups_table())
        .map_err(|e| e.to_string())?;
    let group_val = fg_table
        .get(group_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Face group not found: {}", group_id))?;
    let group: FaceGroup = serde_json::from_str(group_val.value()).map_err(|e| e.to_string())?;
    drop(tx_read);

    // Delete the face group and clean up FileNode references in one write transaction
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut fg_table = tx
            .open_table(crate::db::Database::get_face_groups_table())
            .map_err(|e| e.to_string())?;
        fg_table
            .remove(group_id.as_str())
            .map_err(|e| e.to_string())?;

        // Remove face_group_id from referenced FileNodes
        let mut ft_table = tx
            .open_table(crate::db::Database::get_files_table())
            .map_err(|e| e.to_string())?;
        for fid in &group.file_ids {
            let node_str = ft_table
                .get(fid.as_str())
                .map_err(|e| e.to_string())?
                .map(|fv| fv.value().to_string());
            if let Some(ref node_str) = node_str {
                let mut file_node: FileNode =
                    serde_json::from_str(node_str).map_err(|e| e.to_string())?;
                file_node.face_group_ids.retain(|id| id != &group_id);
                let file_serialized =
                    serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
                ft_table
                    .insert(fid.as_str(), file_serialized.as_str())
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// Find similar faces to a given face group.
#[tauri::command(async)]
pub fn find_similar_faces(
    group_id: String,
    threshold: Option<f32>,
    state: State<'_, AppState>,
) -> Result<Vec<FaceGroup>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    let tx = db.begin_read().map_err(|e| e.to_string())?;

    let fg_table = tx
        .open_table(crate::db::Database::get_face_groups_table())
        .map_err(|e| e.to_string())?;
    let source_val = fg_table
        .get(group_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Face group not found: {}", group_id))?;
    let source: FaceGroup = serde_json::from_str(source_val.value()).map_err(|e| e.to_string())?;

    let thresh = threshold.unwrap_or(DEFAULT_MATCH_THRESHOLD);

    let mut similar = Vec::new();
    for entry in fg_table.iter().map_err(|e| e.to_string())? {
        let (key, value) = entry.map_err(|e| e.to_string())?;
        if key.value() == group_id {
            continue;
        }
        let group: FaceGroup = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;

        if let (Some(ref src_emb), Some(ref grp_emb)) =
            (&source.centroid_embedding, &group.centroid_embedding)
        {
            let dist = embedding_distance(src_emb, grp_emb);
            if dist < thresh {
                similar.push(group);
            }
        }
    }

    Ok(similar)
}

/// List all face groups from the database.
#[tauri::command(async)]
pub fn list_face_groups(state: State<'_, AppState>) -> Result<Vec<FaceGroup>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(crate::db::Database::get_face_groups_table())
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let group: FaceGroup = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        results.push(group);
    }

    Ok(results)
}

/// Get all files that belong to a specific face group.
#[tauri::command(async)]
pub fn get_group_files(
    group_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<FileNode>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;

    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let fg_table = tx_read
        .open_table(crate::db::Database::get_face_groups_table())
        .map_err(|e| e.to_string())?;
    let fg_value = fg_table
        .get(group_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Face group not found: {}", group_id))?;
    let group: FaceGroup = serde_json::from_str(fg_value.value()).map_err(|e| e.to_string())?;

    let file_table = tx_read
        .open_table(crate::db::Database::get_files_table())
        .map_err(|e| e.to_string())?;

    let mut files = Vec::new();
    for fid in &group.file_ids {
        if let Some(fv) = file_table.get(fid.as_str()).map_err(|e| e.to_string())? {
            let file_node: FileNode =
                serde_json::from_str(fv.value()).map_err(|e| e.to_string())?;
            files.push(file_node);
        }
    }

    Ok(files)
}
