// CyberManju OS — Map (src-tauri/src/commands/map.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: GeoFile, get_geo_files, GpsCoordinates, extract_exif_gps.
// Integrates: redb, serde, tauri.
// Contracts: none special (pure logic; errors bubble to caller).
use redb::ReadableTable;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::schema::FileNode;
use crate::AppState;

/// A file with GPS coordinates for map display.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoFile {
    pub id: String,
    pub name: String,
    #[serde(rename = "fileType")]
    pub file_type: String,
    #[serde(rename = "gpsLat")]
    pub lat: f64,
    #[serde(rename = "gpsLon")]
    pub lon: f64,
    #[serde(rename = "thumbnailPath")]
    pub thumbnail_path: Option<String>,
}

/// Get all files that have GPS coordinates stored in their metadata.
#[tauri::command(async)]
pub fn get_geo_files(state: State<'_, AppState>) -> Result<Vec<GeoFile>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(crate::db::Database::get_files_table())
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let file_node: FileNode = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;

        if let (Some(lat), Some(lon)) = (file_node.gps_lat, file_node.gps_lon) {
            results.push(GeoFile {
                id: file_node.id,
                name: file_node.name,
                file_type: file_node.file_type,
                lat,
                lon,
                thumbnail_path: file_node.thumbnail_path,
            });
        }
    }

    Ok(results)
}

/// GPS coordinates extracted from EXIF data.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpsCoordinates {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
}

/// Extract GPS coordinates from a file's EXIF data using kamadak-exif.
/// Returns the extracted coordinates and updates the file node in the database.
#[tauri::command(async)]
pub fn extract_exif_gps(
    file_id: String,
    state: State<'_, AppState>,
) -> Result<GpsCoordinates, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;

    // Read the file node to get the file path
    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let table_read = tx_read
        .open_table(crate::db::Database::get_files_table())
        .map_err(|e| e.to_string())?;
    let value = table_read
        .get(file_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("File not found: {}", file_id))?;
    let mut file_node: FileNode = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
    drop(tx_read);

    // Get the actual file path from the file node's context_data or name
    // The file path should be stored in context_data or as a known location
    let file_path = file_node
        .context_data
        .as_ref()
        .and_then(|ctx| ctx.get("file_path"))
        .and_then(|v| v.as_str())
        .unwrap_or(&file_node.name);

    // Open the file and read EXIF data using kamadak-exif
    let file = std::fs::File::open(file_path)
        .map_err(|e| format!("Failed to open file for EXIF reading: {}", e))?;
    let mut bufreader = std::io::BufReader::new(&file);
    let exif_data = exif::Reader::new()
        .read_from_container(&mut bufreader)
        .map_err(|e| format!("Failed to read EXIF data: {}", e))?;

    // Extract GPS latitude
    let latitude = exif_data
        .get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY)
        .and_then(|field| {
            let components = if let exif::Value::Rational(ref rationals) = field.value {
                Some(rationals.as_slice())
            } else {
                None
            }?;
            if components.len() >= 3 {
                let deg = components[0].num as f64 / components[0].denom as f64;
                let min = components[1].num as f64 / components[1].denom as f64;
                let sec = components[2].num as f64 / components[2].denom as f64;
                Some(deg + min / 60.0 + sec / 3600.0)
            } else {
                None
            }
        })
        .ok_or_else(|| "GPS latitude not found in EXIF".to_string())?;

    // Extract GPS longitude
    let longitude = exif_data
        .get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY)
        .and_then(|field| {
            let components = if let exif::Value::Rational(ref rationals) = field.value {
                Some(rationals.as_slice())
            } else {
                None
            }?;
            if components.len() >= 3 {
                let deg = components[0].num as f64 / components[0].denom as f64;
                let min = components[1].num as f64 / components[1].denom as f64;
                let sec = components[2].num as f64 / components[2].denom as f64;
                Some(deg + min / 60.0 + sec / 3600.0)
            } else {
                None
            }
        })
        .ok_or_else(|| "GPS longitude not found in EXIF".to_string())?;

    // Apply GPS latitude/longitude reference (N/S, E/W)
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

    let final_lat = if lat_ref.as_deref() == Some("S") {
        -latitude
    } else {
        latitude
    };

    let final_lon = if lon_ref.as_deref() == Some("W") {
        -longitude
    } else {
        longitude
    };

    // Extract altitude if available
    let altitude = exif_data
        .get_field(exif::Tag::GPSAltitude, exif::In::PRIMARY)
        .and_then(|field| {
            if let exif::Value::Rational(ref rationals) = field.value {
                rationals.first().map(|r| r.num as f64 / r.denom as f64)
            } else {
                None
            }
        });

    let coordinates = GpsCoordinates {
        latitude: final_lat,
        longitude: final_lon,
        altitude,
    };

    // Update the file node with GPS coordinates
    file_node.gps_lat = Some(final_lat);
    file_node.gps_lon = Some(final_lon);
    file_node.modified_at = chrono::Utc::now().to_rfc3339();

    // Store updated GPS in context_data as well
    let mut ctx = file_node
        .context_data
        .clone()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
    if let Some(obj) = ctx.as_object_mut() {
        obj.insert("gps_source".to_string(), serde_json::json!("exif"));
        obj.insert("gps_altitude".to_string(), serde_json::json!(altitude));
    }
    file_node.context_data = Some(ctx);

    // Write back to database
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

    Ok(coordinates)
}
