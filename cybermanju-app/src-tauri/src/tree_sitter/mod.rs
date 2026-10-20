// CyberManju OS — Tree-sitter Code Intelligence Module
// Incremental parsing for 200+ languages
// Extracts symbols, structure, semantic info for file organization
//
// Two engines, one JSON shape: real `tree-sitter` grammars for the six
// hottest languages (rust, python, javascript, typescript, go, bash —
// feature `real-treesitter`, on by default), heuristic regex-based
// extraction for everything else and as the fallback when parsing fails.
// The result carries `"engine": "tree-sitter" | "heuristic"` so callers
// (and tests) can tell which one ran.

use anyhow::Result;
use serde_json::{json, Value};

// Shared core: language detection + heuristic fallback live in
// `cybermanju-types::code` so the Tauri commands, the REST route and the
// WASM dispatcher all parse the same way. Only the real grammars (below)
// are desktop-only.
pub use cybermanju_types::code::detect_language;
use cybermanju_types::code::extract_symbols_heuristic;

// ---------------------------------------------------------------------------
// Real tree-sitter engine (feature `real-treesitter`)
// ---------------------------------------------------------------------------
//
// Query-free by design: instead of per-language tree-sitter queries (whose
// syntax drifts between grammar releases), we walk the concrete syntax tree
// and collect named nodes whose kind is in a per-language set, reading the
// `name` field. New grammar versions keep node kinds far more stable than
// query predicates, so this survives `cargo update`.

/// Hard stop on tree walking — a pathological generated file must slow the
/// command down, never hang it.
#[cfg(feature = "real-treesitter")]
const MAX_TS_NODES: usize = 20_000;

/// Hard stop on symbol output — the REST/Tauri payload stays small.
#[cfg(feature = "real-treesitter")]
const MAX_TS_SYMBOLS: usize = 2_000;

/// Languages with a bundled grammar. Everything else uses the heuristic.
#[cfg(feature = "real-treesitter")]
fn ts_language_for(language: &str) -> Option<tree_sitter::Language> {
    match language {
        "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        "python" => Some(tree_sitter_python::LANGUAGE.into()),
        "javascript" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "typescript" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "bash" => Some(tree_sitter_bash::LANGUAGE.into()),
        _ => None,
    }
}

/// Named node kinds collected per language, with the symbol kind reported.
#[cfg(feature = "real-treesitter")]
fn ts_item_kinds(language: &str) -> &'static [(&'static str, &'static str)] {
    match language {
        "rust" => &[
            ("function_item", "function"),
            ("struct_item", "class"),
            ("enum_item", "class"),
            ("trait_item", "interface"),
            ("mod_item", "class"),
            ("type_item", "class"),
            ("impl_item", "impl"),
        ],
        "python" => &[
            ("function_definition", "function"),
            ("class_definition", "class"),
        ],
        "javascript" => &[
            ("function_declaration", "function"),
            ("generator_function_declaration", "function"),
            ("class_declaration", "class"),
            ("method_definition", "function"),
        ],
        "typescript" => &[
            ("function_declaration", "function"),
            ("generator_function_declaration", "function"),
            ("class_declaration", "class"),
            ("method_definition", "function"),
            ("interface_declaration", "interface"),
            ("type_alias_declaration", "class"),
            ("enum_declaration", "class"),
        ],
        "go" => &[
            ("function_declaration", "function"),
            ("method_declaration", "function"),
            ("type_declaration", "class"),
            ("type_spec", "class"),
        ],
        "bash" => &[("function_definition", "function")],
        _ => &[],
    }
}

#[cfg(feature = "real-treesitter")]
fn ts_kind_of(language: &str, node_kind: &str) -> Option<&'static str> {
    ts_item_kinds(language)
        .iter()
        .find(|(kind, _)| *kind == node_kind)
        .map(|(_, symbol_kind)| *symbol_kind)
}

/// Parse with a real grammar and walk the tree. Returns `None` when there
/// is no grammar for the language, parsing fails, or nothing was found —
/// all three mean "caller falls back to the heuristic".
#[cfg(feature = "real-treesitter")]
fn extract_symbols_ts(content: &str, language: &str) -> Option<Vec<Value>> {
    let ts_language = ts_language_for(language)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&ts_language).ok()?;
    let tree = parser.parse(content, None)?;
    let bytes = content.as_bytes();
    let first_lines: Vec<&str> = content.lines().collect();

    let mut symbols = Vec::new();
    let mut visited = 0usize;
    let mut stack = vec![tree.root_node()];

    while let Some(node) = stack.pop() {
        visited += 1;
        if visited > MAX_TS_NODES || symbols.len() >= MAX_TS_SYMBOLS {
            break;
        }
        // Single `if let` (never nested `if` + `if let`: `collapsible_if`
        // is denied workspace-wide, and let-chains need a newer toolchain
        // than our MSRV, so the condition folds through `Option` instead).
        if let Some(symbol_kind) = node
            .is_named()
            .then(|| ts_kind_of(language, node.kind()))
            .flatten()
        {
            // `impl` blocks name their type, everything else names `name`.
            let field = if node.kind() == "impl_item" {
                "type"
            } else {
                "name"
            };
            let name = node
                .child_by_field_name(field)
                .and_then(|name_node| name_node.utf8_text(bytes).ok())
                .and_then(|raw| {
                    let trimmed = raw.split_whitespace().next().unwrap_or("").trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                });
            if let Some(name) = name {
                let start_line = node.start_position().row as u32 + 1;
                let end_line = node.end_position().row as u32 + 1;
                let detail = first_lines
                    .get(node.start_position().row)
                    .map(|l| l.trim().to_string())
                    .unwrap_or_default();
                symbols.push(json!({
                    "name": name,
                    "kind": symbol_kind,
                    "start_line": start_line,
                    "end_line": end_line,
                    "detail": detail,
                    "children": [],
                }));
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    if symbols.is_empty() {
        return None;
    }
    // Deterministic order: source order, like the heuristic path.
    symbols.sort_by_key(|s| s["start_line"].as_u64().unwrap_or(0));
    Some(symbols)
}

/// Which engine produced a result. Reported as `"engine"` in `parse_file`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseEngine {
    TreeSitter,
    Heuristic,
}

impl ParseEngine {
    /// Engine label reported on every code-intel result (`tree-sitter` / `heuristic`).
    pub fn as_str(&self) -> &'static str {
        match self {
            ParseEngine::TreeSitter => "tree-sitter",
            ParseEngine::Heuristic => "heuristic",
        }
    }

    /// Pick the engine for a language: real grammars where bundled.
    pub fn for_language(language: &str) -> ParseEngine {
        #[cfg(feature = "real-treesitter")]
        {
            if ts_language_for(language).is_some() {
                return ParseEngine::TreeSitter;
            }
        }
        let _ = language;
        ParseEngine::Heuristic
    }
}

/// Symbols + the engine that produced them. One shape, two producers.
pub fn extract_symbols(content: &str, language: &str) -> (Vec<Value>, ParseEngine) {
    #[cfg(feature = "real-treesitter")]
    {
        if ts_language_for(language).is_some() {
            if let Some(symbols) = extract_symbols_ts(content, language) {
                return (symbols, ParseEngine::TreeSitter);
            }
            // Parsed nothing (empty file, unparseable bytes, cap hit):
            // the heuristic still answers with the same shape.
            return (
                extract_symbols_heuristic(content, language),
                ParseEngine::Heuristic,
            );
        }
    }
    (
        extract_symbols_heuristic(content, language),
        ParseEngine::Heuristic,
    )
}

// ---------------------------------------------------------------------------
// Tauri commands — registered directly in lib.rs invoke_handler
// ---------------------------------------------------------------------------

/// Parse a source file and return a structured JSON result with language,
/// symbols, line count, and timing.
///
/// Returns:
/// ```json
/// {
///   "file_path": "...",
///   "language": "rust",
///   "engine": "tree-sitter",
///   "symbols": [...],
///   "total_lines": 42,
///   "parse_time_ms": 1
/// }
/// ```
/// `"engine"` is `"tree-sitter"` when a bundled grammar ran and
/// `"heuristic"` for the other ~45 extensions or when parsing failed over
/// to the regex path.
#[tauri::command(async)]
pub fn parse_file(file_path: String) -> Result<Value, String> {
    let start = std::time::Instant::now();

    // Read the file from disk
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("integrity: cannot read file {}: {}", file_path, e))?;

    // Extract filename for language detection
    let filename = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let language = detect_language(filename);
    let total_lines = content.lines().count();
    let (symbols, engine) = extract_symbols(&content, language);
    let parse_time_ms = start.elapsed().as_millis() as u64;

    let result = json!({
        "filePath": file_path,
        "language": language,
        "engine": engine.as_str(),
        "symbols": symbols,
        "totalLines": total_lines,
        "parseTimeMs": parse_time_ms,
    });

    Ok(result)
}

/// Parse a source file and return just the symbols array as JSON values.
#[tauri::command(async)]
pub fn get_symbols(file_path: String) -> Result<Vec<Value>, String> {
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("integrity: cannot read file {}: {}", file_path, e))?;

    let filename = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let language = detect_language(filename);
    let (symbols, _engine) = extract_symbols(&content, language);

    Ok(symbols)
}

/// Parse source text directly (no filesystem read).
///
/// This is the transport-agnostic entry point: the desktop panel sends the
/// already-loaded file content (or pasted code), and the REST route
/// `POST /api/code/parse` serves the same shape for web/Pages clients.
/// Same real-engine dispatch as `parse_file`; `file_name` only drives
/// language detection.
#[tauri::command(async)]
pub fn parse_text(file_name: String, content: String) -> Result<Value, String> {
    const MAX_CONTENT_BYTES: usize = 1024 * 1024;
    if content.len() > MAX_CONTENT_BYTES {
        return Err(format!(
            "too_large: content is {} bytes, limit is {}",
            content.len(),
            MAX_CONTENT_BYTES
        ));
    }
    let start = std::time::Instant::now();
    let language = detect_language(&file_name);
    let total_lines = content.lines().count();
    let (symbols, engine) = extract_symbols(&content, language);
    let parse_time_ms = start.elapsed().as_millis() as u64;

    Ok(json!({
        "filePath": file_name,
        "language": language,
        "engine": engine.as_str(),
        "symbols": symbols,
        "totalLines": total_lines,
        "parseTimeMs": parse_time_ms,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_detection_covers_the_hot_extensions() {
        assert_eq!(detect_language("main.rs"), "rust");
        assert_eq!(detect_language("app.py"), "python");
        assert_eq!(detect_language("index.ts"), "typescript");
        assert_eq!(detect_language("run.go"), "go");
        assert_eq!(detect_language("notes.md"), "markdown");
    }

    #[test]
    fn engine_selection_prefers_grammars_where_bundled() {
        if cfg!(feature = "real-treesitter") {
            assert_eq!(ParseEngine::for_language("rust"), ParseEngine::TreeSitter);
            assert_eq!(ParseEngine::for_language("python"), ParseEngine::TreeSitter);
        } else {
            assert_eq!(ParseEngine::for_language("rust"), ParseEngine::Heuristic);
        }
        // No bundled grammar here in either configuration.
        assert_eq!(
            ParseEngine::for_language("markdown"),
            ParseEngine::Heuristic
        );
    }

    #[cfg(feature = "real-treesitter")]
    #[test]
    fn tree_sitter_finds_rust_items_with_source_order() {
        let content = "fn alpha() {}\nstruct Beta;\nfn gamma() {}\n";
        let (symbols, engine) = extract_symbols(content, "rust");
        assert_eq!(engine, ParseEngine::TreeSitter);
        let names: Vec<&str> = symbols.iter().filter_map(|s| s["name"].as_str()).collect();
        assert!(names.contains(&"alpha"), "got {names:?}");
        assert!(names.contains(&"Beta"), "got {names:?}");
        assert!(names.contains(&"gamma"), "got {names:?}");
        let kinds: Vec<&str> = symbols.iter().filter_map(|s| s["kind"].as_str()).collect();
        assert!(kinds.contains(&"function"));
        assert!(kinds.contains(&"class"));
    }

    #[cfg(feature = "real-treesitter")]
    #[test]
    fn tree_sitter_finds_python_defs_and_empty_falls_back() {
        let content = "def hello():\n    pass\n\nclass World:\n    pass\n";
        let (symbols, engine) = extract_symbols(content, "python");
        assert_eq!(engine, ParseEngine::TreeSitter);
        let names: Vec<&str> = symbols.iter().filter_map(|s| s["name"].as_str()).collect();
        assert!(names.contains(&"hello"), "got {names:?}");
        assert!(names.contains(&"World"), "got {names:?}");

        // Nothing to find: honest heuristic fallback, same shape.
        let (symbols, engine) = extract_symbols("", "rust");
        assert_eq!(engine, ParseEngine::Heuristic);
        assert!(symbols.is_empty());
    }

    #[test]
    fn heuristic_still_covers_unbundled_languages() {
        let content = "# Title\n\nSome text\n";
        let (symbols, engine) = extract_symbols(content, "markdown");
        assert_eq!(engine, ParseEngine::Heuristic);
        assert!(symbols.is_empty());
    }
}
