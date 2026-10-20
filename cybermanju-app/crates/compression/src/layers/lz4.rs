// CyberManju OS — Lz4 (crates/compression/src/layers/lz4.rs).
// Role: LZ4→ZSTD-15→Brotli-11 triple cascade + .cyb3.
// Exports: Lz4Layer, NAME, ALGORITHM, COLOR, compress, decompress, compress_with_detail, probe_ratio.
// Integrates: anyhow.
// Contracts: none special (pure logic; errors bubble to caller).
use crate::types::LayerDetail;
use anyhow::Result;

/// LZ4 layer — ultra-fast compression (~400 MB/s)
pub struct Lz4Layer;

impl Lz4Layer {
    pub const NAME: &'static str = "Layer 1: LZ4";
    pub const ALGORITHM: &'static str = "lz4_flex";
    pub const COLOR: &'static str = "#00D4FF";

    pub fn compress(data: &[u8]) -> Result<Vec<u8>> {
        Ok(lz4_flex::compress_prepend_size(data))
    }

    pub fn decompress(data: &[u8]) -> Result<Vec<u8>> {
        Ok(lz4_flex::decompress_size_prepended(data)?)
    }

    pub fn compress_with_detail(data: &[u8]) -> Result<(Vec<u8>, LayerDetail)> {
        let input_size = data.len() as u64;
        let output = Self::compress(data)?;
        let output_size = output.len() as u64;
        Ok((
            output,
            LayerDetail {
                name: Self::NAME.into(),
                algorithm: Self::ALGORITHM.into(),
                input_size,
                output_size,
                ratio: if input_size > 0 {
                    output_size as f64 / input_size as f64
                } else {
                    1.0
                },
                color: Self::COLOR.into(),
            },
        ))
    }

    pub fn probe_ratio(data: &[u8]) -> f64 {
        let original_size = data.len() as u64;
        if original_size == 0 {
            return 1.0;
        }
        let compressed = Self::compress(data).unwrap_or_default();
        compressed.len() as f64 / original_size as f64
    }
}
