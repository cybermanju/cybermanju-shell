// CyberManju OS — Compression (crates/os-wasm/src/compression.rs).
// Role: browser volume backend (OPFS/localStorage) for Pages.
// Exports: compress_lz4, decompress_lz4, compress_brotli, decompress_brotli, compress_lz4_probe_ratio, decompress_zstd.
// Integrates: wasm_bindgen.
// Contracts: none special (pure logic; errors bubble to caller).
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn compress_lz4(data: &[u8]) -> Vec<u8> {
    lz4_flex::compress_prepend_size(data)
}

#[wasm_bindgen]
pub fn decompress_lz4(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    lz4_flex::decompress_size_prepended(data)
        .map_err(|e| JsValue::from_str(&format!("LZ4 decompression failed: {}", e)))
}

#[wasm_bindgen]
pub fn compress_brotli(data: &[u8], quality: u32) -> Vec<u8> {
    use std::io::Write;
    let mut compressor = brotli::CompressorWriter::new(Vec::new(), 4096, quality, 22);
    compressor.write_all(data).expect("Brotli write failed");
    compressor.into_inner()
}

#[wasm_bindgen]
pub fn decompress_brotli(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    use std::io::Write;
    let mut decompressor = brotli::DecompressorWriter::new(Vec::new(), 4096);
    decompressor
        .write_all(data)
        .map_err(|e| JsValue::from_str(&format!("Brotli decompression failed: {}", e)))?;
    // `into_inner` returns `Result<W, W>`: the `Err` arm hands back the
    // partial output, not an error value, so report it as bytes written.
    decompressor.into_inner().map_err(|partial| {
        JsValue::from_str(&format!(
            "Brotli stream incomplete ({} bytes of output)",
            partial.len()
        ))
    })
}

#[wasm_bindgen]
pub fn compress_lz4_probe_ratio(data: &[u8]) -> f64 {
    let compressed = lz4_flex::compress_prepend_size(data);
    if data.is_empty() {
        1.0
    } else {
        compressed.len() as f64 / data.len() as f64
    }
}

// ─── zstd (decode only) ─────────────────────────────────────────────
// The encoder lives in `cybermanju-compression` (`zstd-sys`, C via `cc`)
// and can never build for wasm32-unknown-unknown, so browser-side zstd
// *encoding* goes through `@dweb-browser/zstd-wasm` (`src/utils/zstd.ts`,
// same standard frames). Decoding is pure Rust (`ruzstd`), so it lives
// here: single-layer zstd blobs and the middle leg of the triple chain
// open without any JS dependency — the same decoder `artifact.rs` uses
// to unwrap desktop artifacts.

#[wasm_bindgen]
pub fn decompress_zstd(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    use std::io::Read;

    let mut decoder = ruzstd::decoding::StreamingDecoder::new(std::io::Cursor::new(data))
        .map_err(|_| JsValue::from_str("integrity: not a valid zstd frame"))?;
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|_| JsValue::from_str("integrity: zstd frame truncated"))?;
    Ok(out)
}
