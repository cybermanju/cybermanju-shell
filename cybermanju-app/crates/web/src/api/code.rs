// CyberManju OS — code-intelligence routes.
//
// `POST /api/code/parse {fileName, content}` parses source text and answers
// the same shape as the Tauri `parse_text` command. The grammar engine lives
// in `src-tauri` (it needs C toolchains the server image does not promise),
// so this route runs the shared heuristic core from `cybermanju-types::code`
// and says so (`"engine": "heuristic"`). Same symbols, honestly labeled.

use serde::Deserialize;

/// At most 1 MiB of source per request — parsing is CPU-bound and the
/// request thread must stay responsive.
pub const MAX_CONTENT_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParseRequest {
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    content: String,
}

/// Parse source text. Pure function of the body — no database needed.
pub fn parse_text(filename: &str, content: &str) -> Result<serde_json::Value, String> {
    if content.len() > MAX_CONTENT_BYTES {
        return Err(format!(
            "too_large: content is {} bytes, limit is {}",
            content.len(),
            MAX_CONTENT_BYTES
        ));
    }
    let start = std::time::Instant::now();
    let language = cybermanju_types::code::detect_language(filename);
    let total_lines = content.lines().count();
    let symbols = cybermanju_types::code::extract_symbols_heuristic(content, language);
    let parse_time_ms = start.elapsed().as_millis() as u64;
    Ok(serde_json::json!({
        "filePath": filename,
        "language": language,
        "engine": "heuristic",
        "symbols": symbols,
        "totalLines": total_lines,
        "parseTimeMs": parse_time_ms,
    }))
}

/// Dispatch `/api/code/*`. Returns `None` for foreign paths.
pub fn route(
    method: &str,
    path_segments: &[&str],
    body: &str,
    origin: Option<&str>,
) -> Option<String> {
    match (path_segments, method) {
        (["api", "code", "parse"], "POST") => {
            let req: ParseRequest = match serde_json::from_str(body) {
                Ok(req) => req,
                Err(err) => {
                    return Some(crate::json_error(
                        400,
                        &format!("Invalid JSON: {err}"),
                        origin,
                    ))
                }
            };
            if req.content.is_empty() {
                return Some(crate::json_error(
                    400,
                    "content is required (source text to parse)",
                    origin,
                ));
            }
            match parse_text(&req.file_name, &req.content) {
                Ok(value) => Some(crate::json_ok(&value, origin)),
                Err(message) if message.starts_with("too_large:") => {
                    Some(crate::json_error(413, &message, origin))
                }
                Err(message) => Some(crate::json_error(400, &message, origin)),
            }
        }
        (["api", "code", ..], _) => Some(crate::json_error(
            405,
            &format!("method not allowed: {method} /api/code/parse"),
            origin,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rust_heuristically_with_shared_shape() {
        let out = parse_text("main.rs", "fn alpha() {}\nstruct Beta;\n").expect("parse");
        assert_eq!(out["language"], "rust");
        assert_eq!(out["engine"], "heuristic");
        let names: Vec<&str> = out["symbols"]
            .as_array()
            .expect("array")
            .iter()
            .filter_map(|s| s["name"].as_str())
            .collect();
        assert!(names.contains(&"alpha"), "got {names:?}");
    }

    #[test]
    fn rejects_oversize_and_empty_bodies() {
        assert!(parse_text("a.rs", &"x".repeat(MAX_CONTENT_BYTES + 1)).is_err());
        let resp = route("POST", &["api", "code", "parse"], "{}", None).expect("owned");
        assert!(resp.contains("400"), "{resp}");
        assert!(route("GET", &["api", "files"], "", None).is_none());
    }
}
