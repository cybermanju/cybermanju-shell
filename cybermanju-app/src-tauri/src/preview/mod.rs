// CyberManju OS — File Preview Generation Module
// Creates lightweight preview files linked to originals
// Uses image crate for thumbnails, auto-thumbnail pattern for media

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Preview file metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewMeta {
    pub original_file_id: String,
    pub preview_path: String,
    pub thumbnail_path: Option<String>,
    pub width: u32,
    pub height: u32,
    pub format: String, // "png" | "jpg" | "webp" | "sprite_sheet"
    pub size_bytes: u64,
}

/// Generate a thumbnail preview for an image file
/// In production: uses the image crate for images, auto-thumbnail for videos/PDFs
pub fn generate_thumbnail(
    data: &[u8],
    max_size: u32,
    file_id: &str,
    preview_dir: &str,
) -> Result<PreviewMeta> {
    let img = image::load_from_memory(data)?;
    let (w, h) = (img.width(), img.height());

    // Scale down to fit within max_size
    let scale = if w > h {
        max_size as f64 / w as f64
    } else {
        max_size as f64 / h as f64
    };
    let new_w = (w as f64 * scale) as u32;
    let new_h = (h as f64 * scale) as u32;

    let thumbnail = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);

    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    thumbnail.write_to(&mut cursor, image::ImageFormat::Png)?;

    let preview_path = format!("{}/{}_thumb.png", preview_dir, file_id);
    std::fs::create_dir_all(preview_dir)?;
    std::fs::write(&preview_path, &buf)?;

    Ok(PreviewMeta {
        original_file_id: file_id.to_string(),
        preview_path,
        thumbnail_path: None,
        width: new_w,
        height: new_h,
        format: "png".into(),
        size_bytes: buf.len() as u64,
    })
}

/// Extract metadata for a preview card (without generating image)
pub fn extract_preview_metadata(
    filename: &str,
    size: u64,
    mime: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "filename": filename,
        "size": size,
        "size_human": format_size_human(size),
        "mime_type": mime.unwrap_or("unknown"),
        "is_image": mime.is_some_and(|m| m.starts_with("image/")),
        "is_video": mime.is_some_and(|m| m.starts_with("video/")),
        "is_code": is_code_file(filename),
        "is_audio": mime.is_some_and(|m| m.starts_with("audio/")),
        "language": detect_language_for_preview(filename),
    })
}

fn format_size_human(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    match bytes {
        b if b < KB => format!("{} B", b),
        b if b < MB => format!("{:.1} KB", b as f64 / KB as f64),
        b if b < GB => format!("{:.1} MB", b as f64 / MB as f64),
        b => format!("{:.2} GB", b as f64 / GB as f64),
    }
}

fn is_code_file(filename: &str) -> bool {
    matches!(
        filename.rsplit('.').next().unwrap_or(""),
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "py"
            | "go"
            | "c"
            | "cpp"
            | "java"
            | "rb"
            | "swift"
            | "kt"
            | "html"
            | "css"
            | "json"
            | "toml"
            | "yaml"
            | "md"
            | "sql"
            | "sh"
            | "lua"
            | "zig"
            | "vue"
            | "svelte"
    )
}

fn detect_language_for_preview(filename: &str) -> &str {
    crate::tree_sitter::detect_language(filename)
}
