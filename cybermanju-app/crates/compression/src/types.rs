// CyberManju OS — Types (crates/compression/src/types.rs).
// Role: LZ4→ZSTD-15→Brotli-11 triple cascade + .cyb3.
// Exports: CompressionType, display_name, color, speed_label, CompressionStats, LayerDetail.
// Integrates: serde.
// Contracts: none special (pure logic; errors bubble to caller).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CompressionType {
    None,
    Lz4,
    Zstd,
    TripleLayer,
}

impl CompressionType {
    pub fn display_name(&self) -> &str {
        match self {
            Self::None => "Uncompressed",
            Self::Lz4 => "LZ4 (Real-Time)",
            Self::Zstd => "Zstandard (Balanced)",
            Self::TripleLayer => "Triple-Layer (LZ4→ZSTD→Brotli)",
        }
    }

    pub fn color(&self) -> &str {
        match self {
            Self::None => "#6B7280",
            Self::Lz4 => "#00D4FF",
            Self::Zstd => "#00FF41",
            Self::TripleLayer => "#FFB800",
        }
    }

    pub fn speed_label(&self) -> &str {
        match self {
            Self::None => "N/A",
            Self::Lz4 => "Ultra-Fast (~400 MB/s)",
            Self::Zstd => "Fast (configurable 1-22)",
            Self::TripleLayer => "Slow (maximum ratio)",
        }
    }
}

// `rename_all = "camelCase"`: these travel over REST and Tauri to
// `src/types/index.ts` (`originalSize`, `layerDetails`, `inputSize`, …),
// which reads camelCase — snake_case would surface as `undefined` in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionStats {
    pub original_size: u64,
    pub compressed_size: u64,
    pub ratio: f64,
    pub layer: String,
    pub layer_details: Vec<LayerDetail>,
    pub blake3_hash: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerDetail {
    pub name: String,
    pub algorithm: String,
    pub input_size: u64,
    pub output_size: u64,
    pub ratio: f64,
    pub color: String,
}
