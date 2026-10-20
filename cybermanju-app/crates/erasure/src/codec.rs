// CyberManju OS — Reed–Solomon codec over GF(2^8) (AGENT-7 item 3)
//
// `k` data shards + `m` parity shards: any `k` of the `k + m` shards
// reconstruct the original payload byte-for-byte, so a striped chunk survives
// `m` provider losses instead of needing `m + 1` full copies.
//
// Two framing layers, deliberately separated:
//   * `encode`/`decode` operate on raw bytes and pad the payload to
//     `shard_len = max(1, ceil(len / k))` equal-length shards;
//   * `encode_framed`/`decode_framed` wrap the payload in a 12-byte header
//     (`CJRS` magic + little-endian u64 length) first, so a decoder can cut
//     the padding back off without being told the original length.
// The framing is what lets `manifest.rs` shard an opaque pipeline artifact
// and reassemble it exactly.

use std::fmt;

use reed_solomon_erasure::galois_8::ReedSolomon;

/// `galois_8` (GF(2^8)) supports `k + m <= 256` shards.
pub const MAX_TOTAL_SHARDS: usize = 256;

/// Magic prefix of a framed erasure payload.
pub const FRAME_MAGIC: &[u8; 4] = b"CJRS";

/// Size of the frame header: 4-byte magic + 8-byte little-endian length.
pub const FRAME_HEADER_LEN: usize = 12;

/// Why a codec operation failed. `Display` emits the repo's machine-readable
/// error prefixes (`unsupported:` / `integrity:`) so callers can pass the
/// string straight into an API response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// `k == 0`, `m == 0` or `k + m > 256`.
    InvalidLayout { data: u8, parity: u8 },
    /// Two present shards disagree on length (or one is empty).
    ShardSizeMismatch,
    /// Fewer than `k` usable shards — reconstruction is impossible.
    TooFewShards { need: usize, have: usize },
    /// A shard index outside `0..k+m`.
    ShardIndexOutOfRange(u8),
    /// The decoded bytes are not a well-formed frame.
    Framing(String),
    /// The underlying RS library refused the operation.
    Reconstruct(String),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodecError::InvalidLayout { data, parity } => write!(
                f,
                "unsupported: erasure layout k={} m={} is invalid (need k>=1, m>=1, k+m<={})",
                data, parity, MAX_TOTAL_SHARDS
            ),
            CodecError::ShardSizeMismatch => {
                write!(f, "integrity: erasure shards disagree on size")
            }
            CodecError::TooFewShards { need, have } => write!(
                f,
                "integrity: erasure decode needs {} shards, only {} are usable",
                need, have
            ),
            CodecError::ShardIndexOutOfRange(index) => write!(
                f,
                "integrity: erasure shard index {} is out of range",
                index
            ),
            CodecError::Framing(detail) => {
                write!(f, "integrity: erasure frame invalid: {}", detail)
            }
            CodecError::Reconstruct(detail) => {
                write!(f, "integrity: erasure reconstruction failed: {}", detail)
            }
        }
    }
}

impl From<CodecError> for String {
    fn from(err: CodecError) -> Self {
        err.to_string()
    }
}

/// One encode result: the `k + m` shard byte vectors, index-aligned with the
/// layout they were produced for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoded {
    pub data_shards: u8,
    pub parity_shards: u8,
    /// Length of the payload that went in (before framing, when framed).
    pub payload_len: u64,
    pub shards: Vec<Vec<u8>>,
}

impl Encoded {
    /// Number of bytes each shard holds.
    pub fn shard_len(&self) -> usize {
        self.shards.first().map(|s| s.len()).unwrap_or(0)
    }
}

/// Validate a `k`/`m` pair against the field size.
pub fn check_layout(data: u8, parity: u8) -> Result<(u8, u8), CodecError> {
    if data == 0 || parity == 0 || data as usize + parity as usize > MAX_TOTAL_SHARDS {
        return Err(CodecError::InvalidLayout { data, parity });
    }
    Ok((data, parity))
}

/// Bytes per shard for a payload of `payload_len` split `k` ways. Never 0 —
/// the RS library rejects empty shards, and a 0-length payload still has to
/// produce a decodable frame.
pub fn shard_len(payload_len: u64, data: u8) -> u64 {
    let k = data.max(1) as u64;
    payload_len.div_ceil(k).max(1)
}

/// Split `payload` into `k` padded data shards, compute `m` parity shards.
pub fn encode(payload: &[u8], data: u8, parity: u8) -> Result<Encoded, CodecError> {
    let (k, m) = check_layout(data, parity)?;
    let s = shard_len(payload.len() as u64, k) as usize;
    let total = k as usize + m as usize;

    let mut shards: Vec<Vec<u8>> = Vec::with_capacity(total);
    for i in 0..k as usize {
        let start = i * s;
        let end = ((i + 1) * s).min(payload.len());
        let mut shard = vec![0u8; s];
        if start < payload.len() {
            shard[..end - start].copy_from_slice(&payload[start..end]);
        }
        shards.push(shard);
    }
    for _ in 0..m {
        shards.push(vec![0u8; s]);
    }

    let rs = ReedSolomon::new(k as usize, m as usize)
        .map_err(|e| CodecError::Reconstruct(e.to_string()))?;
    rs.encode(&mut shards)
        .map_err(|e| CodecError::Reconstruct(e.to_string()))?;

    Ok(Encoded {
        data_shards: k,
        parity_shards: m,
        payload_len: payload.len() as u64,
        shards,
    })
}

/// Reconstruct every shard from the present ones, returning all `k + m`
/// shards (missing ones rebuilt). The shard length is taken from the data —
/// a decoder never has to know the original payload length.
pub fn reconstruct_shards(
    data: u8,
    parity: u8,
    present: &[(u8, Vec<u8>)],
) -> Result<Vec<Vec<u8>>, CodecError> {
    let (k, m) = check_layout(data, parity)?;
    let total = k as usize + m as usize;

    let shard_len = present.first().map(|(_, b)| b.len()).unwrap_or(0);
    if shard_len == 0 {
        return Err(CodecError::ShardSizeMismatch);
    }
    if present.iter().any(|(_, b)| b.len() != shard_len) {
        return Err(CodecError::ShardSizeMismatch);
    }

    let mut shards: Vec<Option<Vec<u8>>> = vec![None; total];
    let mut have = 0usize;
    for (index, bytes) in present {
        let i = *index as usize;
        if i >= total {
            return Err(CodecError::ShardIndexOutOfRange(*index));
        }
        if shards[i].is_none() {
            shards[i] = Some(bytes.clone());
            have += 1;
        }
    }
    if have < k as usize {
        return Err(CodecError::TooFewShards {
            need: k as usize,
            have,
        });
    }

    let rs = ReedSolomon::new(k as usize, m as usize)
        .map_err(|e| CodecError::Reconstruct(e.to_string()))?;
    rs.reconstruct(&mut shards)
        .map_err(|e| CodecError::Reconstruct(e.to_string()))?;

    Ok(shards
        .into_iter()
        .map(|shard| shard.expect("reconstruct fills every shard"))
        .collect())
}

/// Decode a raw payload: take any `k` present shards, rebuild, concatenate
/// the data shards and cut the zero padding off at `out_len`.
pub fn decode(
    data: u8,
    parity: u8,
    out_len: u64,
    present: &[(u8, Vec<u8>)],
) -> Result<Vec<u8>, CodecError> {
    let (k, _) = check_layout(data, parity)?;
    let shards = reconstruct_shards(data, parity, present)?;
    let mut out = Vec::with_capacity(shards.first().map(|s| s.len()).unwrap_or(0) * k as usize);
    for shard in shards.iter().take(k as usize) {
        out.extend_from_slice(shard);
    }
    if out.len() < out_len as usize {
        return Err(CodecError::Framing(format!(
            "reconstructed {} bytes, expected {}",
            out.len(),
            out_len
        )));
    }
    out.truncate(out_len as usize);
    Ok(out)
}

/// Wrap a payload so the padding can be trimmed without knowing the length:
/// `CJRS` + u64 LE length + payload.
pub fn frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    out.extend_from_slice(FRAME_MAGIC);
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// Inverse of [`frame`]. Rejects anything that is not a well-formed frame.
pub fn unframe(bytes: &[u8]) -> Result<Vec<u8>, CodecError> {
    if bytes.len() < FRAME_HEADER_LEN {
        return Err(CodecError::Framing(format!(
            "only {} bytes, header needs {}",
            bytes.len(),
            FRAME_HEADER_LEN
        )));
    }
    if &bytes[..4] != FRAME_MAGIC {
        return Err(CodecError::Framing("bad magic".to_string()));
    }
    let mut len_bytes = [0u8; 8];
    len_bytes.copy_from_slice(&bytes[4..12]);
    let len = u64::from_le_bytes(len_bytes) as usize;
    if bytes.len() < FRAME_HEADER_LEN + len {
        return Err(CodecError::Framing(format!(
            "payload claims {} bytes, only {} available",
            len,
            bytes.len() - FRAME_HEADER_LEN
        )));
    }
    Ok(bytes[FRAME_HEADER_LEN..FRAME_HEADER_LEN + len].to_vec())
}

/// Encode with framing applied first (the form `manifest.rs` shards).
pub fn encode_framed(payload: &[u8], data: u8, parity: u8) -> Result<Encoded, CodecError> {
    encode(&frame(payload), data, parity)
}

/// Decode a framed payload from any `k` present shards.
pub fn decode_framed(
    data: u8,
    parity: u8,
    present: &[(u8, Vec<u8>)],
) -> Result<Vec<u8>, CodecError> {
    let (k, _) = check_layout(data, parity)?;
    let shard_len = present.first().map(|(_, b)| b.len()).unwrap_or(0);
    if shard_len == 0 {
        return Err(CodecError::ShardSizeMismatch);
    }
    // Keep the whole reconstruction: the frame itself says where the payload
    // ends, so no padding has to be guessed.
    let padded_len = (shard_len as u64).saturating_mul(k as u64);
    let bytes = decode(data, parity, padded_len, present)?;
    unframe(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random payload so a failure is reproducible.
    fn payload(len: usize) -> Vec<u8> {
        (0..len).map(|i| ((i * 31 + 7) % 251) as u8).collect()
    }

    fn all_combos(n: usize, take: usize) -> Vec<Vec<usize>> {
        fn rec(
            n: usize,
            start: usize,
            take: usize,
            acc: &mut Vec<usize>,
            out: &mut Vec<Vec<usize>>,
        ) {
            if acc.len() == take {
                out.push(acc.clone());
                return;
            }
            for i in start..n {
                acc.push(i);
                rec(n, i + 1, take, acc, out);
                acc.pop();
            }
        }
        let mut out = Vec::new();
        rec(n, 0, take, &mut Vec::new(), &mut out);
        out
    }

    #[test]
    fn round_trip_is_byte_identical() {
        for len in [0usize, 1, 7, 64, 4096, 4097, 100_000] {
            for (k, m) in [(1u8, 1u8), (2, 1), (4, 2), (10, 3)] {
                let data = payload(len);
                let encoded = encode(&data, k, m).expect("encode");
                assert_eq!(encoded.shards.len(), k as usize + m as usize);
                let present: Vec<(u8, Vec<u8>)> = encoded
                    .shards
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (i as u8, s.clone()))
                    .collect();
                let back = decode(k, m, data.len() as u64, &present).expect("decode");
                assert_eq!(back, data, "len={} k={} m={}", len, k, m);
            }
        }
    }

    #[test]
    fn survives_every_possible_loss_of_m_shards() {
        let (k, m) = (4u8, 2u8);
        let data = payload(9_973);
        let encoded = encode(&data, k, m).expect("encode");
        let total = encoded.shards.len();

        for take in 0..=m as usize {
            for dropped in all_combos(total, take) {
                let present: Vec<(u8, Vec<u8>)> = encoded
                    .shards
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !dropped.contains(i))
                    .map(|(i, s)| (i as u8, s.clone()))
                    .collect();
                let back = decode(k, m, data.len() as u64, &present)
                    .unwrap_or_else(|e| panic!("decode failed, dropped {:?}: {}", dropped, e));
                assert_eq!(back, data, "dropped {:?}", dropped);
            }
        }
    }

    #[test]
    fn too_few_shards_is_an_error_not_a_panic() {
        let (k, m) = (3u8, 2u8);
        let data = payload(512);
        let encoded = encode(&data, k, m).expect("encode");
        let present: Vec<(u8, Vec<u8>)> = encoded
            .shards
            .iter()
            .take(2)
            .enumerate()
            .map(|(i, s)| (i as u8, s.clone()))
            .collect();
        let err = decode(k, m, data.len() as u64, &present).expect_err("must fail");
        assert!(
            err.to_string().starts_with("integrity:"),
            "unexpected error: {}",
            err
        );
        assert_eq!(err, CodecError::TooFewShards { need: 3, have: 2 });
    }

    #[test]
    fn invalid_layouts_are_rejected() {
        assert!(matches!(
            encode(&payload(16), 0, 1),
            Err(CodecError::InvalidLayout { .. })
        ));
        assert!(matches!(
            encode(&payload(16), 1, 0),
            Err(CodecError::InvalidLayout { .. })
        ));
        assert!(matches!(
            encode(&payload(16), 200, 100),
            Err(CodecError::InvalidLayout { .. })
        ));
        assert!(check_layout(255, 1).is_ok());
        assert!(check_layout(1, 1).is_ok());
    }

    #[test]
    fn framed_round_trip_drops_padding_and_rejects_garbage() {
        let (k, m) = (3u8, 2u8);
        for len in [0usize, 1, 100, 4095, 4096, 4097] {
            let data = payload(len);
            let encoded = encode_framed(&data, k, m).expect("encode");
            let present: Vec<(u8, Vec<u8>)> = encoded
                .shards
                .iter()
                .enumerate()
                .map(|(i, s)| (i as u8, s.clone()))
                .collect();
            let back = decode_framed(k, m, &present).expect("decode");
            assert_eq!(back, data, "len={}", len);
        }

        // A shard set whose payload is not a frame must not decode into
        // pretend data.
        let encoded = encode(&payload(64), 2, 1).expect("encode");
        let present: Vec<(u8, Vec<u8>)> = encoded
            .shards
            .iter()
            .enumerate()
            .map(|(i, s)| (i as u8, s.clone()))
            .collect();
        let err = decode_framed(2, 1, &present).expect_err("garbage must fail");
        assert!(err.to_string().starts_with("integrity:"), "{}", err);
    }

    #[test]
    fn out_of_range_shard_index_is_rejected() {
        let encoded = encode(&payload(32), 2, 1).expect("encode");
        let mut present: Vec<(u8, Vec<u8>)> = encoded
            .shards
            .iter()
            .enumerate()
            .map(|(i, s)| (i as u8, s.clone()))
            .collect();
        present[0].0 = 9;
        let err = decode(2, 1, 32, &present).expect_err("bad index");
        assert_eq!(err, CodecError::ShardIndexOutOfRange(9));
    }

    #[test]
    fn mismatched_shard_sizes_are_rejected() {
        let encoded = encode(&payload(64), 2, 1).expect("encode");
        let mut present: Vec<(u8, Vec<u8>)> = encoded
            .shards
            .iter()
            .enumerate()
            .map(|(i, s)| (i as u8, s.clone()))
            .collect();
        present[1].1.push(0);
        let err = decode(2, 1, 64, &present).expect_err("size mismatch");
        assert_eq!(err, CodecError::ShardSizeMismatch);
    }

    #[test]
    fn shard_len_is_never_zero() {
        assert_eq!(shard_len(0, 4), 1);
        assert_eq!(shard_len(1, 4), 1);
        assert_eq!(shard_len(4, 4), 1);
        assert_eq!(shard_len(5, 4), 2);
        assert_eq!(shard_len(100, 3), 34);
    }
}
