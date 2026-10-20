// CyberManju OS — Edit (crates/agent/src/edit.rs).
// Hash-anchored edits (hashline-inspired, BLAKE3-addressed).
//
// The model replaces one exact `old_block` with `new_block` instead of
// retyping lines, and may pin the file with `expected_hash` (BLAKE3 hex of
// the whole file before editing). Refusals carry machine prefixes:
// `not_found:` (block absent), `conflict:` (block ambiguous — resend a
// bigger block), `integrity:` (hash moved under you). Pure bytes in/out so
// every transport — and the unit tests — share one applier.

/// Minimum anchor strength: 16 hex chars = 64 bits. Anything shorter is
/// refused outright — a 4-hex-char (16-bit) prefix "matches" one file in
/// 65536 by luck, which is a coin flip, not a race check.
const MIN_ANCHOR_HEX: usize = 16;

/// Apply one anchored replacement. Returns the new file bytes.
pub fn apply_edit(
    current: &str,
    old_block: &str,
    new_block: &str,
    expected_hash: Option<&str>,
) -> Result<String, String> {
    if old_block.is_empty() {
        return Err("invalid: old_block is empty".to_string());
    }
    let anchor = normalize_anchor(expected_hash.unwrap_or(""));
    if !anchor.is_empty() {
        // Prefix-accept: both tools print the full hex, and a model echoing a
        // 16+-char prefix still gets a real race check (64+ bits) instead of
        // a permanent `integrity:` blocker. Shorter than that is not a check
        // at all — refuse rather than flip a coin or block forever.
        if anchor.len() < MIN_ANCHOR_HEX {
            return Err(format!(
                "integrity: anchor '{anchor}' is too short (need ≥{MIN_ANCHOR_HEX} hex chars / 64+ bits) — re-read and retry"
            ));
        }
        let actual = blake3_hex(current.as_bytes());
        if !actual.starts_with(anchor) {
            return Err(format!(
                "integrity: file changed since anchor (expected {anchor}, got {actual}) — re-read and retry"
            ));
        }
    }
    let hits = current.matches(old_block).count();
    if hits == 0 {
        // Second chance: whitespace-normalized match (tabs vs spaces, trailing
        // whitespace). Still requires exactly one candidate, and the
        // replacement keeps the model's own whitespace — only *locating* is
        // fuzzy, never the written bytes.
        match fuzzy_locate(current, old_block) {
            Ok(Some(unique)) => {
                let mut out = current.to_string();
                out.replace_range(unique, new_block);
                return Ok(out);
            }
            Ok(None) => {
                return Err(
                    "not_found: old_block does not occur in the file — re-read and retry"
                        .to_string(),
                )
            }
            Err(count) => {
                return Err(format!(
                    "conflict: old_block occurs {count} times (whitespace-normalized) — resend a larger, unique block"
                ));
            }
        }
    }
    if hits > 1 {
        return Err(format!(
            "conflict: old_block occurs {hits} times — resend a larger, unique block"
        ));
    }
    Ok(current.replacen(old_block, new_block, 1))
}

/// Locate `needle` in `haystack` ignoring runs of whitespace (and
/// indentation width). `Ok(Some(range))` on a unique match, `Ok(None)` when
/// absent, `Err(count)` when it matches more than once (ambiguity must be
/// reported, never silently resolved).
fn fuzzy_locate(haystack: &str, needle: &str) -> Result<Option<std::ops::Range<usize>>, usize> {
    /// Collapse whitespace runs to one space; `map[i]` is the original byte
    /// offset where normalized byte `i` came from (so every mapped span lands
    /// on char boundaries by construction).
    fn squash(text: &str) -> (String, Vec<usize>) {
        let mut out = String::new();
        let mut map = Vec::new();
        let mut in_gap = false;
        for (byte, ch) in text.char_indices() {
            if ch.is_whitespace() {
                if !in_gap {
                    out.push(' ');
                    map.push(byte);
                    in_gap = true;
                }
                continue;
            }
            in_gap = false;
            let before = out.len();
            out.push(ch);
            map.extend(std::iter::repeat_n(byte, out.len() - before));
        }
        (out, map)
    }
    let (hay, map) = squash(haystack);
    let (ndl, _) = squash(needle);
    if ndl.trim().is_empty() {
        return Ok(None);
    }
    let mut occurrences: Vec<usize> = Vec::new();
    let mut from = 0;
    while let Some(at) = hay[from..].find(&ndl) {
        occurrences.push(from + at);
        from += at + ndl.len();
        if from >= hay.len() {
            break;
        }
    }
    if occurrences.len() > 1 {
        return Err(occurrences.len());
    }
    let first = match occurrences.first() {
        Some(first) => *first,
        None => return Ok(None),
    };
    let start = map[first];
    let end = map
        .get(first + ndl.len())
        .copied()
        .unwrap_or(haystack.len());
    if start >= end {
        return Ok(None);
    }
    Ok(Some(start..end))
}

/// BLAKE3 hex of bytes (the anchor primitive).
pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// The trailer `read` appends so the model has an anchor to hand back as
/// `expected_hash`: a newline plus `[blake3:<hex>]`.
pub fn anchor_line(hash: &str) -> String {
    format!("\n[blake3:{hash}]")
}

/// Hex of a syntactically valid trailing `[blake3:<64hex>]` line, if the
/// content ends with one (one trailing newline tolerated). Pure syntax —
/// says nothing about whose trailer it is.
pub fn anchor_trailer(content: &str) -> Option<&str> {
    let body = content.strip_suffix('\n').unwrap_or(content);
    let line_start = match body.rfind('\n') {
        Some(i) => i + 1,
        None => 0,
    };
    anchor_trailer_hex(&body[line_start..])
}

/// Drop a trailing `[blake3:<hex>]` line (with its newline) that **we**
/// added — i.e. whose hex is the BLAKE3 of the bytes before it, the exact
/// trailer `read` appends. A file that legitimately ends with an
/// anchor-shaped line survives a write untouched: its hex cannot equal the
/// hash of its own prefix unless a `read` put it there.
pub fn strip_anchor(content: &str) -> &str {
    strip_echo(content, None)
}

/// Write-path policy: like [`strip_anchor`], but also accepts the trailer of
/// `existing` — bytes already on disk (the pre-edit file in an edit flow).
/// A model that edits a file and echoes the *old* trailer is still echoing
/// our metadata, not writing content, so the echo goes. Anything else —
/// including a trailer-shaped line the model composed itself — stays.
pub fn strip_echo<'a>(content: &'a str, existing: Option<&str>) -> &'a str {
    let hex = match anchor_trailer(content) {
        Some(hex) => hex,
        None => return content,
    };
    let body = body_before_trailer(content);
    if blake3_hex(body.as_bytes()) == hex {
        return body;
    }
    if let Some(prev) = existing {
        if blake3_hex(prev.as_bytes()) == hex {
            return body;
        }
    }
    content
}

/// Bytes before the trailing anchor line (never carrying its newline).
/// Only meaningful when [`anchor_trailer`] is `Some`; otherwise the result
/// is unused by the callers above.
fn body_before_trailer(content: &str) -> &str {
    let body = content.strip_suffix('\n').unwrap_or(content);
    match body.rfind('\n') {
        Some(i) => &content[..i],
        None => "",
    }
}

/// Accept every shape a model might echo back — the whole `[blake3:<hex>]`
/// line, a `blake3:` prefix, brackets, stray whitespace — then compare.
fn normalize_anchor(raw: &str) -> &str {
    let t = raw.trim();
    let t = t
        .strip_prefix("[blake3:")
        .or_else(|| t.strip_prefix("blake3:"))
        .unwrap_or(t);
    t.trim_end_matches(']').trim()
}

fn anchor_trailer_hex(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("[blake3:")?;
    let hex = rest.strip_suffix(']')?;
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchored_edit_replaces_exactly_once() {
        let out = apply_edit(
            "fn a() {}\nfn b() {}\n",
            "fn a() {}",
            "fn a() {\n  1\n}",
            None,
        )
        .expect("edit");
        assert!(out.contains("fn a() {\n  1\n}"));
        assert!(out.contains("fn b() {}"));
    }

    #[test]
    fn missing_and_ambiguous_blocks_refuse_honestly() {
        assert!(apply_edit("aaa", "zzz", "b", None)
            .expect_err("missing")
            .starts_with("not_found:"));
        assert!(apply_edit("x\nx\n", "x", "y", None)
            .expect_err("ambiguous")
            .starts_with("conflict:"));
        assert!(apply_edit("", "", "y", None)
            .expect_err("empty")
            .starts_with("invalid:"));
    }

    #[test]
    fn whitespace_differences_fall_back_to_fuzzy_match() {
        // Model retyped with spaces what the file holds as a tab.
        let out = apply_edit(
            "fn a() {\n\treturn 1;\n}\n",
            "fn a() {\n  return 1;\n}",
            "fn a() {\n  return 2;\n}",
            None,
        )
        .expect("fuzzy");
        assert!(out.contains("return 2;"), "{out}");
        assert!(!out.contains("\treturn"), "{out}");

        // Trailing whitespace is ignored while locating.
        let out = apply_edit("x = 1;  \n", "x = 1;", "x = 2;", None).expect("trailing");
        assert!(out.contains("x = 2;"), "{out}");

        // …but ambiguity is still refused, fuzzy or not.
        assert!(apply_edit("a  b\na\tb\n", "a b", "c", None)
            .expect_err("ambiguous")
            .starts_with("conflict:"));
    }

    #[test]
    fn fuzzy_match_is_byte_exact_on_unicode() {
        let content = "héllo wörld\n";
        let out = apply_edit(content, "héllo   wörld", "héllo Rust", None).expect("unicode");
        assert_eq!(out, "héllo Rust\n");
    }

    #[test]
    fn stale_anchor_is_an_integrity_error() {
        let content = "hello";
        let wrong = "0".repeat(64);
        assert!(apply_edit(content, "hello", "bye", Some(&wrong))
            .expect_err("stale")
            .starts_with("integrity:"));
        let right = blake3_hex(content.as_bytes());
        assert_eq!(
            apply_edit(content, "hello", "bye", Some(&right)).expect("fresh"),
            "bye"
        );
        // No anchor at all is not an anchor failure.
        assert_eq!(
            apply_edit(content, "hello", "bye", Some("")).expect("empty"),
            "bye"
        );
    }

    #[test]
    fn a_short_anchor_prefix_still_verifies() {
        let content = "hello";
        let full = blake3_hex(content.as_bytes());
        let short = &full[..16];
        assert!(apply_edit(content, "hello", "bye", Some(short)).is_ok());
        assert!(apply_edit(content, "hello", "bye", Some("deadbeefdeadbeef")).is_err());
        // …and every shape a model might echo back still verifies.
        assert!(apply_edit(content, "hello", "bye", Some(&format!("blake3:{full}"))).is_ok());
        assert!(apply_edit(content, "hello", "bye", Some(&format!("[blake3:{full}]"))).is_ok());
        assert!(apply_edit(content, "hello", "bye", Some(&format!("  {short}  "))).is_ok());
    }

    #[test]
    fn anchors_shorter_than_64_bits_refuse_as_integrity() {
        let content = "hello";
        let full = blake3_hex(content.as_bytes());
        // 15 correct hex chars still refuse: a prefix that short is a coin
        // flip (1 in 4 billion… worse, 16-bit at 4 chars), not a race check.
        let tiny = &full[..15];
        assert!(full.starts_with(tiny));
        let err = apply_edit(content, "hello", "bye", Some(tiny)).expect_err("short");
        assert!(err.starts_with("integrity:"), "{err}");
        // Empty/missing anchor still means "no check", not a failure.
        assert_eq!(
            apply_edit(content, "hello", "bye", None).expect("none"),
            "bye"
        );
        assert_eq!(
            apply_edit(content, "hello", "bye", Some("")).expect("empty"),
            "bye"
        );
    }

    #[test]
    fn read_anchor_round_trips_and_never_survives_a_write() {
        let raw = "fn a() {}\n";
        let read_back = format!("{raw}{}", anchor_line(&blake3_hex(raw.as_bytes())));
        assert!(read_back.contains("\n[blake3:"));
        // write/edit see exactly the bytes the model read.
        assert_eq!(strip_anchor(&read_back), raw);
        // Nothing to strip ⇒ untouched, including near-miss lines.
        assert_eq!(strip_anchor(raw), raw);
        assert_eq!(strip_anchor("[blake3:short]"), "[blake3:short]");
        assert_eq!(
            strip_anchor("a\n[blake3:not-hex]\n"),
            "a\n[blake3:not-hex]\n"
        );
        // Anchor-only content strips only when it is ours (hash of empty);
        // a foreign anchor-shaped line is content and stays.
        assert_eq!(strip_anchor(&anchor_line(&blake3_hex(b""))), "");
        let foreign = anchor_line(&blake3_hex(b"x"));
        assert_eq!(strip_anchor(&foreign), foreign.as_str());
    }

    #[test]
    fn legitimate_anchor_shaped_lines_survive_a_write() {
        // A file that happens to end with an anchor-shaped line is content,
        // not metadata: its hex cannot be the hash of its own prefix.
        let foreign_hex = "0123456789abcdef".repeat(4);
        assert_eq!(foreign_hex.len(), 64);
        let legit = format!("doc about anchors\n[blake3:{foreign_hex}]");
        assert_eq!(strip_anchor(&legit), legit.as_str());
        // …unless it really is the round-trip trailer `read` appended.
        let body = "doc about anchors\n";
        let ours = format!("{body}{}", anchor_line(&blake3_hex(body.as_bytes())));
        assert_eq!(strip_anchor(&ours), body);
    }

    #[test]
    fn edited_echoes_of_the_previous_file_still_strip() {
        let before = "line one\nline two\n";
        let after_body = "line one\nline CHANGED\n";
        let stale_echo = format!(
            "{after_body}{}",
            anchor_line(&blake3_hex(before.as_bytes()))
        );
        // Body-hash alone does not verify (the content changed)…
        assert_eq!(strip_anchor(&stale_echo), stale_echo.as_str());
        // …but against the bytes the model actually read, it is ours.
        assert_eq!(strip_echo(&stale_echo, Some(before)), after_body);
        // A trailer the model composed itself is content even with `existing`.
        let forged = format!("{after_body}[blake3:{}]", "f".repeat(64));
        assert_eq!(strip_echo(&forged, Some(before)), forged.as_str());
    }
}
