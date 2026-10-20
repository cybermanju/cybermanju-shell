// CyberManju OS — Zstd layer (crates/compression/src/layers/zstd_layer.rs).
// Role: LZ4→ZSTD-15→Brotli-11 triple cascade + .cyb3.
// Exports: ZstdLayer, NAME, COLOR, DEFAULT_LEVEL, compress, decompress, compress_with_detail.
// Integrates: anyhow.
// Contracts: none special (pure logic; errors bubble to caller).
use crate::types::LayerDetail;
use anyhow::Result;

pub struct ZstdLayer;

impl ZstdLayer {
    pub const NAME: &'static str = "Layer 2: Zstandard";
    pub const COLOR: &'static str = "#00FF41";
    pub const DEFAULT_LEVEL: i32 = 15;

    pub fn compress(data: &[u8], level: i32) -> Result<Vec<u8>> {
        Ok(zstd::encode_all(data, level)?)
    }

    pub fn decompress(data: &[u8]) -> Result<Vec<u8>> {
        Ok(zstd::decode_all(data)?)
    }

    pub fn compress_with_detail(data: &[u8], level: i32) -> Result<(Vec<u8>, LayerDetail)> {
        let input_size = data.len() as u64;
        let output = Self::compress(data, level)?;
        let output_size = output.len() as u64;
        Ok((
            output,
            LayerDetail {
                name: Self::NAME.into(),
                algorithm: format!("zstd level {level}"),
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
}
