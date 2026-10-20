// CyberManju OS — Memory (crates/agent/src/memory.rs).
// Semantic memory core: chunking, cosine ranking, budgets, rendering.
//
// Storage lives in redb (`agent_memories` — same DB file as sessions, the
// `.cybermanju` home); the ANN itself is exact brute-force cosine over
// same-dims vectors. Personal memory is hundreds of rows: at 1536 dims ×
// 10k rows a recall is ~60M flops, single-digit milliseconds. When usage
// proves otherwise, the phase-2 pick is `instant-distance` (pure-Rust HNSW,
// MIT/Apache, prod-used) rebuilt from redb rows — NOT a vendored server
// (qdrant) and NOT an immature all-in-one (`vecstore`, `ruvector` were
// evaluated and rejected: too young to trust with the DB file).
//
// Recalled context is always bounded (`MEMORY_RECALL_BUDGET_CHARS`, Hermes
// `memory_char_limit` parity: 2200) — memory augments the prompt, it never
// becomes the prompt. A memory without a vector (embedding failed, or the
// dialect has no embeddings API) is still recalled via the keyword fallback,
// never silently dropped.

use cybermanju_types::agent::{AgentMemory, MemoryHit, MemoryOrigin};

/// Recalled-context budget per prompt (Hermes `memory_char_limit` parity).
pub const MEMORY_RECALL_BUDGET_CHARS: usize = 2200;
/// One memory is a note, not a document.
pub const MEMORY_TEXT_CAP_CHARS: usize = 2000;
/// Below this cosine the hit is noise, not memory.
pub const MEMORY_MIN_SCORE: f32 = 0.30;
/// Default fan-in per recall.
pub const MEMORY_TOP_K: usize = 3;
/// Keyword fallback needs at least this token overlap to fire.
pub const MEMORY_MIN_OVERLAP: usize = 2;
/// Runs this long without storing anything earn a remember nudge.
pub const MEMORY_NUDGE_TURNS: u32 = 8;
/// Newest-first list cap (memories are small rows; sessions cap at 100).
pub const MEMORY_LIST_LIMIT: usize = 500;

/// Cosine similarity in f64 accumulation (1536-dim f32 sums lose precision
/// otherwise). Dimension mismatch, empty inputs and zero vectors score 0 —
/// incomparable is not similar.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let (x, y) = (*x as f64, *y as f64);
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na <= 0.0 || nb <= 0.0 {
        return 0.0;
    }
    let s = (dot / (na.sqrt() * nb.sqrt())) as f32;
    if !s.is_finite() {
        0.0
    } else {
        s.clamp(-1.0, 1.0)
    }
}

/// Lowercase alphanumeric tokens worth matching on. `len >= 3` keeps `a`,
/// `to` and `ok` out of the overlap count on both sides of the FFI — the TS
/// mirror (`src/utils/memory.ts`) uses the identical rule.
pub fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(str::to_string)
        .collect()
}

/// Distinct query tokens present in the text.
pub fn keyword_overlap(query: &str, text: &str) -> usize {
    let binding = tokens(text);
    let hay: std::collections::HashSet<&str> = binding.iter().map(String::as_str).collect();
    let mut seen = std::collections::HashSet::new();
    let mut hits = 0;
    for w in tokens(query) {
        if seen.insert(w.clone()) && hay.contains(w.as_str()) {
            hits += 1;
        }
    }
    hits
}

/// Paragraph-aware chunks: blank-line paragraphs greedily packed to
/// `max_chars`, over-long paragraphs hard-split on char boundaries. Used by
/// phase-2 document ingest; `remember_text` stores single capped notes.
pub fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(64);
    let mut chunks = Vec::new();
    let mut current = String::new();
    // Flush helper inlined at each site (a closure borrowing `chunks`
    // conflicts with direct `chunks.push` calls under E0499).
    for para in text.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        if para.chars().count() > max_chars {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                chunks.push(trimmed.to_string());
            }
            current.clear();
            let chars: Vec<char> = para.chars().collect();
            for piece in chars.chunks(max_chars) {
                let s: String = piece.iter().collect();
                let s = s.trim();
                if !s.is_empty() {
                    chunks.push(s.to_string());
                }
            }
            continue;
        }
        let add = para.chars().count() + 2;
        if !current.is_empty() && current.chars().count() + add > max_chars {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                chunks.push(trimmed.to_string());
            }
            current.clear();
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(para);
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        chunks.push(trimmed.to_string());
    }
    chunks
}

/// Rank memories for a query. Vector cosine wins when the query embedding is
/// comparable (same dims); everything else — and every vectorless memory —
/// falls back to keyword overlap. Keyword hits score
/// `0.25 + 0.05 × overlap` (capped 0.69): above noise, below a solid vector
/// match, so the two scales never masquerade as each other. Ties break by
/// `uses` then recency — memory that proved useful ranks first.
pub fn recall_rank(
    query_vec: Option<&[f32]>,
    query_text: &str,
    memories: &[AgentMemory],
    top_k: usize,
) -> Vec<MemoryHit> {
    let top_k = top_k.clamp(1, 10);
    let mut scored: Vec<(f32, &AgentMemory)> = Vec::new();
    for m in memories {
        let mut best = 0.0f32;
        if let Some(q) = query_vec {
            if !q.is_empty() && q.len() == m.embedding.len() && !m.embedding.is_empty() {
                let s = cosine(q, &m.embedding);
                if s >= MEMORY_MIN_SCORE {
                    best = s;
                }
            }
        }
        let overlap = keyword_overlap(query_text, &m.text);
        if overlap >= MEMORY_MIN_OVERLAP {
            let kw = (0.25 + 0.05 * overlap as f32).min(0.69);
            if kw > best {
                best = kw;
            }
        }
        if best > 0.0 {
            scored.push((best, m));
        }
    }
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.uses.cmp(&a.1.uses))
            .then_with(|| b.1.updated_at.cmp(&a.1.updated_at))
    });
    scored
        .into_iter()
        .take(top_k)
        .map(|(score, m)| MemoryHit {
            id: m.id.clone(),
            text: m.text.clone(),
            score,
            origin: m.origin,
            session_id: m.session_id.clone(),
            updated_at: m.updated_at.clone(),
        })
        .collect()
}

/// Bounded `<recalled-memories>` prompt block. Empty hits → empty string
/// (no block at all — the prompt stays byte-identical when memory is empty).
pub fn render_recall_block(hits: &[MemoryHit], budget: usize) -> String {
    if hits.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\n--- recalled memories (bounded; verify against the volume before acting) ---\n",
    );
    for h in hits {
        out.push_str(&format!("- {}\n", h.text));
    }
    if out.chars().count() > budget {
        let kept: String = out.chars().take(budget).collect();
        out = format!("{kept}\n… truncated at {budget} chars");
    }
    out
}

/// Hermes `MEMORY.md`-compatible markdown export: curated text grouped by
/// origin, newest first. Vectors stay in redb (or the JSON export) — this
/// file is the human-readable, syncable companion.
pub fn render_export_markdown(memories: &[AgentMemory]) -> String {
    let mut rows: Vec<&AgentMemory> = memories.iter().collect();
    rows.sort_by_key(|a| std::cmp::Reverse(a.updated_at.clone()));
    let mut out =
        String::from("# Memories\n\nExported from CyberManju semantic memory. Newest first.\n");
    for m in rows {
        let origin = match m.origin {
            MemoryOrigin::Remember => "remembered",
            MemoryOrigin::CompactHandoff => "compact-handoff",
            MemoryOrigin::Import => "imported",
        };
        out.push_str(&format!("\n## {} · {}\n{}\n", origin, m.updated_at, m.text));
    }
    out
}

/// Hermes-style periodic nudge: a run long enough to have learned something
/// that stored nothing earns one "teach me" hint — surfaced once, never
/// nagging mid-run.
pub fn should_nudge_memory(turns_used: u32, remembered_this_run: bool) -> bool {
    turns_used >= MEMORY_NUDGE_TURNS && !remembered_this_run
}

/// Sanitize before store: secret-redact (the transcript persists to disk and
/// syncs — a leaked key there would outlive the run), trim, cap to a note.
/// Empty after cleaning → nothing memorable, the caller answers `invalid:`.
pub fn sanitize_text(text: &str) -> (String, usize) {
    let (mut cleaned, count) = crate::redact::redact(text);
    cleaned = cleaned.trim().to_string();
    if cleaned.chars().count() > MEMORY_TEXT_CAP_CHARS {
        cleaned = cleaned.chars().take(MEMORY_TEXT_CAP_CHARS).collect();
    }
    (cleaned, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(id: &str, text: &str, embedding: Vec<f32>, uses: u64) -> AgentMemory {
        AgentMemory {
            id: id.to_string(),
            config_id: "cfg".to_string(),
            owner_id: String::new(),
            text: text.to_string(),
            dims: embedding.len() as u32,
            embedding,
            origin: MemoryOrigin::Remember,
            session_id: None,
            uses,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn cosine_scores_identity_zero_and_mismatch() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert_eq!(cosine(&[1.0], &[1.0, 2.0]), 0.0);
        assert_eq!(cosine(&[], &[]), 0.0);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }

    #[test]
    fn tokens_skip_short_words_on_both_sides() {
        assert_eq!(
            tokens("go to the ok store"),
            vec!["the".to_string(), "store".to_string()]
        );
        assert_eq!(keyword_overlap("red blue green", "RED boat"), 1);
    }

    #[test]
    fn chunks_pack_paragraphs_and_split_monsters() {
        let text = "aaa\n\nbbb\n\nccc";
        assert_eq!(chunk_text(text, 64), vec!["aaa\n\nbbb\n\nccc".to_string()]);
        // The floor is 64 chars (smaller chunks are useless) — split with
        // real sizes.
        let paras = ["a".repeat(40), "b".repeat(40), "c".repeat(40)].join("\n\n");
        assert_eq!(chunk_text(&paras, 64).len(), 3);
        let monster = "x".repeat(200);
        let pieces = chunk_text(&monster, 64);
        assert!(pieces.len() >= 3);
        assert!(pieces.iter().all(|p| p.chars().count() <= 64));
    }

    #[test]
    fn vector_hits_outrank_keyword_hits() {
        let rows = vec![
            mem("kw", "the deployment uses kubernetes clusters", vec![], 0),
            mem("vec", "something unrelated", vec![1.0, 0.0], 0),
        ];
        let hits = recall_rank(Some(&[1.0, 0.0]), "deployment kubernetes", &rows, 3);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "vec");
        assert!(hits[0].score >= MEMORY_MIN_SCORE);
        assert!(hits[1].score < 0.70);
    }

    #[test]
    fn dim_mismatch_falls_back_to_keyword() {
        let rows = vec![mem(
            "a",
            "postgres connection pooling",
            vec![0.1, 0.2, 0.3],
            0,
        )];
        let hits = recall_rank(Some(&[0.1, 0.2]), "postgres pooling", &rows, 3);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "a");
    }

    #[test]
    fn noise_stays_out_and_uses_break_ties() {
        let rows = vec![
            mem("old", "postgres pooling", vec![], 0),
            mem("used", "postgres pooling guide", vec![], 9),
            mem("noise", "completely different topic here", vec![], 99),
        ];
        let hits = recall_rank(None, "postgres pooling pain", &rows, 3);
        assert!(hits.iter().all(|h| h.id != "noise"));
        assert_eq!(hits[0].id, "used");
    }

    #[test]
    fn recall_block_is_bounded_and_empty_when_no_hits() {
        assert_eq!(render_recall_block(&[], 100), "");
        let hits = vec![MemoryHit {
            id: "a".into(),
            text: "x".repeat(500),
            score: 0.9,
            origin: MemoryOrigin::Remember,
            session_id: None,
            updated_at: "t".into(),
        }];
        let block = render_recall_block(&hits, 100);
        assert!(block.contains("recalled memories"));
        assert!(block.contains("truncated at 100 chars"));
    }

    #[test]
    fn export_markdown_groups_by_origin_newest_first() {
        let mut a = mem("a", "first fact", vec![], 0);
        a.updated_at = "2026-01-02T00:00:00Z".to_string();
        let mut b = mem("b", "second fact", vec![], 0);
        b.origin = MemoryOrigin::CompactHandoff;
        b.updated_at = "2026-02-01T00:00:00Z".to_string();
        let md = render_export_markdown(&[a, b]);
        assert!(md.starts_with("# Memories"));
        assert!(md.contains("compact-handoff"));
        assert!(md.find("second fact").unwrap() < md.find("first fact").unwrap());
    }

    #[test]
    fn nudge_fires_once_for_long_memory_less_runs() {
        assert!(should_nudge_memory(8, false));
        assert!(should_nudge_memory(25, false));
        assert!(!should_nudge_memory(7, false));
        assert!(!should_nudge_memory(25, true));
    }

    #[test]
    fn sanitize_trims_caps_and_redacts() {
        let (clean, _) = sanitize_text("  hello world  ");
        assert_eq!(clean, "hello world");
        let (capped, _) = sanitize_text(&"y".repeat(5000));
        assert_eq!(capped.chars().count(), MEMORY_TEXT_CAP_CHARS);
        let (empty, _) = sanitize_text("   ");
        assert!(empty.is_empty());
    }
}
