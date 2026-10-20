// CyberManju OS — scene heuristic matcher.
//
// An HONEST classifier: it never claims vision it does not have. Scores come
// only from filename / path / user-tag text matched against the multilingual
// keyword tables in `data/scenes.json` (the same file the TypeScript UI
// parses, so desktop, server and WASM agree exactly).
//
// Each reported category carries a confidence in (0, 1] plus the keywords
// that fired, so every UI can render it as a bearing honestly ("BEACH 87% —
// matched praia") instead of a fake detection.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

// ── tables ────────────────────────────────────────────────────────────────

/// Parsed form of `data/scenes.json` — the scene keyword tables.
///
/// This is the analyze-side vocabulary, not the search index itself:
/// full-text retrieval stays on Tantivy BM25 over
/// `filename + content_text + tags` (see `cybermanju-search`), while this
/// struct holds the heuristic classifier's 17 categories × 10 languages
/// (`en pt es fr de it nl ru zh ja`, ~740 keyword entries) that turns
/// filename / path / user-tag text into honest bearings (`BEACH 87% —
/// matched praia`) instead of fake vision.
///
/// Shape: `parents` is the child→parent inheritance map (`dog ⇒ animal`,
/// `wedding ⇒ party`, …) applied at a 0.6 discount; `categories` is the
/// per-category row parsed below. Loaded once via [`tables()`] from
/// `include_str!`, so desktop, server and WASM agree byte-for-byte with
/// the TypeScript twin (`src/utils/scene.ts`) parsing the same JSON.
/// Vocabulary: 17 categories × 10 languages (~740 keywords) complementing Tantivy BM25; empty evidence stays silent.
#[derive(Debug, Clone, Deserialize)]
struct Tables {
    /// Child→parent category ids (e.g. `"dog" → "animal"`).
    #[serde(default)]
    parents: HashMap<String, String>,
    /// One row per scene category (17 total); see [`Category`].
    categories: Vec<Category>,
}

/// One scene-category row inside [`Tables`].
///
/// `id` is the stable machine key (`beach`, `dog`, …) reported in
/// [`SceneScore::category`]; `label` is the display name. `keywords` maps
/// a language code (`en`, `pt`, … `ja`) to that language's trigger list —
/// every entry is matched through the analyzer ([`norm`] +
/// tokenization + scoring in [`classify`]), never raw.
/// Row: stable `id` + display `label` + per-language keyword lists (10 langs).
#[derive(Debug, Clone, Deserialize)]
struct Category {
    /// Stable key (`human`, `beach`, `dog`, …); parent targets live here too.
    id: String,
    /// Display label (`Beach`, `Dog`, …) for UI pickers and badges.
    label: String,
    /// Per-language keyword lists (`lang → phrases/words`).
    keywords: HashMap<String, Vec<String>>,
}

/// Load (once) the scene tables from `data/scenes.json`.
///
/// `OnceLock`-cached `include_str!` parse — panics only when the baked-in
/// JSON is malformed, which is a build-time bug, never user input.
fn tables() -> &'static Tables {
    static ONCE: OnceLock<Tables> = OnceLock::new();
    ONCE.get_or_init(|| {
        serde_json::from_str(include_str!("../data/scenes.json")).expect("scenes.json must parse")
    })
}

// ── normalization ─────────────────────────────────────────────────────────

/// Lowercase + latin diacritic fold. Cyrillic/CJK/kana pass through
/// untouched (Rust `to_lowercase` is Unicode-aware; combining marks U+0300–
/// U+036F are stripped so `plage` matches `plagé`-style spellings).
pub fn norm(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    for c in lower.chars() {
        if ('\u{0300}'..='\u{036f}').contains(&c) {
            continue;
        }
        out.push(c);
    }
    // Decompose precomposed latin letters without pulling in a unicode
    // crate: cover the latin-1 + latin-ext vowels used by our tables.
    out.chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ę' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' => 'i',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => 'o',
            'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' => 'u',
            'ý' | 'ÿ' => 'y',
            'ç' | 'ć' | 'č' => 'c',
            'ñ' | 'ń' => 'n',
            'š' | 'ś' => 's',
            'ž' | 'ź' | 'ż' => 'z',
            'ß' => 's', // folded single-char; pairs with "ss" via substring rule
            'ł' => 'l',
            'æ' => 'a', // folded; "ae" spellings still hit via substring
            'œ' => 'o',
            _ => c,
        })
        .collect()
}

/// Split normalized text into analyzer tokens.
///
/// Separators are whitespace plus filename/path punctuation
/// (`/ \ _ - . , ; : ( ) [ ] ' "`); empty pieces are dropped. This is the
/// scene analyzer's tokenizer — the counterpart to Tantivy's tokenizer on
/// the BM25 side — so `praia_grande.jpg` and `praia grande` produce the
/// same token stream before [`keyword_signal`] runs.
fn split_tokens(s: &str) -> Vec<String> {
    s.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '/' | '\\' | '_' | '-' | '.' | ',' | ';' | ':' | '(' | ')' | '[' | ']' | '\'' | '"'
            )
    })
    .filter(|t| !t.is_empty())
    .map(|t| t.to_string())
    .collect()
}

// ── scoring ───────────────────────────────────────────────────────────────

/// One reported category: confidence in (0, 1] + the keywords that fired.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneScore {
    pub category: String,
    pub label: String,
    /// 0..1 (never exactly 0 — unreported categories are omitted).
    pub score: f32,
    pub hits: Vec<String>,
}

/// Evidence fed to the matcher.
#[derive(Debug, Clone, Default)]
pub struct SceneInput {
    pub file_name: String,
    pub tags: Vec<String>,
    /// Parent folders / full path — weaker signal than the name itself.
    pub path: String,
    /// The item is already known to depict a person (face cluster hit).
    pub has_faces: bool,
    /// Image-ish mimetypes get a whisper of prior; anything else none.
    pub mime_type: String,
}

/// Per-source evidence weights for the analyzer: filename is strongest,
/// user tags just below it, parent path weakest. Mirrors the TypeScript
/// twin exactly; tuned so tags alone can carry a match but path alone
/// rarely does.
const W_NAME: f32 = 1.0;
/// See [`W_NAME`]: user-tag evidence weight.
const W_TAG: f32 = 0.95;
/// See [`W_NAME`]: parent-folder / full-path evidence weight.
const W_PATH: f32 = 0.7;
/// Token == keyword (or whole-string phrase hit) signal strength.
const S_EXACT: f32 = 0.9; // token == keyword, or whole-string phrase hit
/// Edge-aligned substring (`praias`, `beaches`) signal strength.
const S_SUB: f32 = 0.55; // keyword is a strict substring of a longer token
/// Single-character CJK keyword containment — deliberately weak.
const S_TINY: f32 = 0.4; // single-character CJK-style keyword substring hit
/// Minimum noisy-OR accumulation before a category is reported.
const MIN_REPORT: f32 = 0.30;
/// Hard confidence ceiling — the matcher never claims 1.0 (honest limits).
const MAX_SCORE: f32 = 0.97;

/// Noisy-OR accumulation capped at [`MAX_SCORE`].
///
/// Each new keyword hit adds `w * s` of the *remaining* doubt
/// (`1.0 - acc`), so more evidence always ranks higher but can never
/// reach certainty — the score stays a bearing, not a detection.
fn combine(acc: f32, w: f32, s: f32) -> f32 {
    (acc + w * s * (1.0 - acc)).min(MAX_SCORE)
}

/// Score one normalized keyword against token/haystack evidence.
fn keyword_signal(keyword: &str, tokens: &[String], hay: &str) -> f32 {
    if keyword.is_empty() {
        return 0.0;
    }
    if keyword.contains(' ') {
        // Multi-word phrase ("lune de miel"): whole-string containment.
        return if hay.contains(keyword) { S_EXACT } else { 0.0 };
    }
    let kw_chars = keyword.chars().count();
    let mut best: f32 = 0.0;
    for t in tokens {
        if t == keyword {
            return S_EXACT;
        }
        if kw_chars == 1 {
            // Single CJK-style char: containment anywhere is only a weak
            // signal (whole-token equality already returned exact above).
            if t.contains(keyword) {
                best = best.max(S_TINY);
            }
        } else if kw_chars >= 4 && t.chars().count() > kw_chars {
            // Mid-token containment ("cume" in "document") is noise, but
            // edge-aligned forms are real morphology ("praias", "beaches").
            if t.starts_with(keyword) || t.ends_with(keyword) {
                best = best.max(S_SUB);
            }
        }
    }
    // Unsegmented scripts (CJK strings with no token boundaries at all):
    // raw containment is the only signal available. Latin single tokens
    // already had their edge-aligned chance above — "cume" in "document"
    // must stay silent.
    if best == 0.0 && kw_chars >= 2 && hay.contains(keyword) && is_unsegmented(hay) {
        // Token equality already failed, so this is a within-string hit.
        best = S_SUB.min(S_EXACT - 0.1);
    }
    best
}

/// True when the haystack is CJK-style continuous script (Han, hiragana,
/// katakana, Hangul) — the only case where raw containment is legitimate.
fn is_unsegmented(hay: &str) -> bool {
    hay.chars().any(|c| {
        matches!(c,
            '\u{4e00}'..='\u{9fff}'   // CJK unified ideographs
            | '\u{3040}'..='\u{309f}' // hiragana
            | '\u{30a0}'..='\u{30ff}' // katakana
            | '\u{ac00}'..='\u{d7af}' // hangul syllables
        )
    })
}

/// True for image-ish mimetypes (`image/*`, `photo`, `picture`).
///
/// Used for the whisper-of-prior step in [`classify`]: visual categories
/// get +0.02 (never enough to report alone), so `beach.jpg` outranks
/// `beach.txt` without fabricating signal from the mimetype itself.
fn is_image_mime(mime: &str) -> bool {
    let m = mime.to_lowercase();
    m.starts_with("image/") || m.contains("photo") || m.contains("picture")
}

/// Classify evidence → sorted descending scene scores (empty = no signal).
///
/// Analyzer pipeline: [`norm`] (lowercase + latin diacritic fold) →
/// tokenize (filename stem, tags, path) → per-keyword [`keyword_signal`]
/// (exact 0.9 / edge-aligned-substring 0.55 / CJK-single-char 0.4) folded
/// with source weights ([`W_NAME`] 1.0 / [`W_TAG`] 0.95 / [`W_PATH`] 0.7)
/// → noisy-OR [`combine`] capped at [`MAX_SCORE`], reported at
/// [`MIN_REPORT`] → child→parent inheritance at 0.6 → face-cluster human
/// boost → image-mime +0.02 whisper → score-descending sort. Complements
/// Tantivy BM25 (which ranks *documents* for a query); this ranks
/// *categories* for a file, each with the keywords that fired as receipts.
pub fn classify(input: &SceneInput) -> Vec<SceneScore> {
    let t = tables();
    let stem = input
        .file_name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(&input.file_name);
    let name_n = norm(stem);
    let path_n = norm(&input.path);
    let tags_n: Vec<String> = input.tags.iter().map(|s| norm(s)).collect();

    let name_tokens = split_tokens(&name_n);
    let path_tokens = split_tokens(&path_n);
    let tag_tokens: Vec<String> = tags_n.iter().flat_map(|s| split_tokens(s)).collect();
    let tags_hay = tags_n.join(" ");

    let mut out: Vec<SceneScore> = Vec::new();
    for cat in &t.categories {
        let mut acc = 0.0f32;
        let mut hits: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        // Deterministic language order for stable hits output.
        let mut langs: Vec<&String> = cat.keywords.keys().collect();
        langs.sort();
        for lang in langs {
            for kw_raw in &cat.keywords[lang] {
                let kw = norm(kw_raw);
                let s_name = keyword_signal(&kw, &name_tokens, &name_n);
                let s_tag = keyword_signal(&kw, &tag_tokens, &tags_hay);
                let s_path = keyword_signal(&kw, &path_tokens, &path_n);
                // Source weight folded into the signal; combine additively.
                let s = (W_NAME * s_name).max(W_TAG * s_tag).max(W_PATH * s_path);
                if s > 0.0 {
                    acc = combine(acc, 1.0, s);
                    if seen.insert(kw.clone()) {
                        hits.push(kw_raw.clone());
                    }
                }
            }
        }
        if cat.id == "human" && input.has_faces {
            acc = combine(acc, 1.0, 0.55);
            if !seen.contains("face-cluster") {
                hits.push("face-cluster".to_string());
            }
        }
        if acc >= MIN_REPORT {
            out.push(SceneScore {
                category: cat.id.clone(),
                label: cat.label.clone(),
                score: (acc * 100.0).round() / 100.0,
                hits,
            });
        }
    }

    // Child → parent inheritance at a discount (dog ⇒ animal @60%).
    let mut extra: Vec<SceneScore> = Vec::new();
    for s in &out {
        if let Some(parent) = t.parents.get(&s.category) {
            let ps = (s.score * 0.6 * 100.0).round() / 100.0;
            if ps >= MIN_REPORT && !out.iter().any(|o| &o.category == parent) {
                if let Some(cat) = t.categories.iter().find(|c| &c.id == parent) {
                    extra.push(SceneScore {
                        category: cat.id.clone(),
                        label: cat.label.clone(),
                        score: ps,
                        hits: vec![format!("inferred:{}", s.category)],
                    });
                }
            }
        }
    }
    out.extend(extra);

    if is_image_mime(&input.mime_type) {
        // Whisper of prior for visual categories — never enough alone.
        for s in out.iter_mut() {
            s.score = ((s.score + 0.02).min(MAX_SCORE) * 100.0).round() / 100.0;
        }
    }

    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.category.cmp(&b.category))
    });
    out
}

/// Which scene categories does a free-text query name? Powers the dynamic
/// search listing ("praia" → BEACH) in every language of the tables.
pub fn match_query(query: &str) -> Vec<SceneScore> {
    classify(&SceneInput {
        file_name: query.to_string(),
        ..Default::default()
    })
}

/// All category ids + labels (for UI pickers / tests).
pub fn category_list() -> Vec<(String, String)> {
    tables()
        .categories
        .iter()
        .map(|c| (c.id.clone(), c.label.clone()))
        .collect()
}

#[cfg(test)]
mod tests;
