// CyberManju OS — Redact (crates/agent/src/redact.rs).
// Secret redaction for tool outputs (omp-style stream hygiene).
//
// Tool results flow back into the transcript, which persists to disk and
// syncs across providers — a pasted API key in `env` output or an
// `Authorization:` header echo must never land there. `redact` scans for
// known secret markers (ASCII case-insensitive, so shouting env files are
// covered) plus JWT shapes and replaces the secret run with `***`, keeping
// the marker so the model still sees *which* credential kind appeared.
// Pure string surgery: no regex crate, no I/O, wasm-safe. Offsets are byte
// indices into the original text, so multibyte content can never mis-slice.

/// Markers whose trailing token run is secret. Matching is case-sensitive
/// on purpose (`PASSWORD=` in shouting config files is still caught by the
/// lowercase pass — see `redact`).
/// Lowercase twins are unnecessary: matching below is ASCII
/// case-insensitive, so `PASSWORD=` and `password=` hit the same rule.
const MARKERS: &[&str] = &[
    "sk-ant-",
    "sk-",
    "AKIA",
    "ghp_",
    "gho_",
    "ghu_",
    "github_pat_",
    "xox",
    "glpat-",
    "-----BEGIN PRIVATE KEY-----",
    "-----BEGIN RSA PRIVATE KEY-----",
    "-----BEGIN OPENSSH PRIVATE KEY-----",
    "Bearer ",
    "api_key=",
    "apiKey=",
    "apikey=",
    "key=",
    "password=",
    "passwd=",
    "passphrase=",
    "secret=",
    "token=",
    "jwt_secret",
    "ya29.",
];

/// Characters that may continue a secret token.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '+' | '=' | '~')
}

/// Redact secrets in `text`. Returns `(redacted, replacements)`.
pub fn redact(text: &str) -> (String, usize) {
    // Cheap path first: no marker, no JWT shape, no work.
    let mut hits: Vec<(usize, usize)> = Vec::new();

    // All markers in ONE pass over the original bytes (ASCII
    // case-insensitive): byte offsets always stay in `text` coordinates,
    // which keeps every later slice on a char boundary.
    // Longest marker first, so `sk-ant-…` wins over its prefix `sk-` and
    // the more specific credential kind stays visible in the redacted text.
    let mut markers: Vec<&str> = MARKERS.to_vec();
    markers.sort_by_key(|m| std::cmp::Reverse(m.len()));
    let mut claimed: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for marker in markers {
        let needle = marker.as_bytes();
        let mut from = 0;
        let bytes = text.as_bytes();
        while from + needle.len() <= bytes.len() {
            let window = &bytes[from..from + needle.len()];
            let matches = window
                .iter()
                .zip(needle.iter())
                .all(|(a, b)| a.eq_ignore_ascii_case(b));
            if !matches {
                from += 1;
                continue;
            }
            let start = from;
            let secret_start = start + needle.len();
            let mut end = secret_start;
            if marker.starts_with("-----BEGIN") {
                // A PEM header has no token run after it — the secret is the
                // whole following line. Skip the newline, redact that line.
                let mut probe = secret_start;
                if probe < bytes.len() && bytes[probe] == b'\n' {
                    probe += 1;
                }
                end = probe;
                while end < bytes.len() && bytes[end] != b'\n' {
                    end += 1;
                }
            } else {
                let mut run = 0;
                while end < bytes.len() && run < 512 && is_token_char(bytes[end] as char) {
                    end += 1;
                    run += 1;
                }
            }
            if end > secret_start && claimed.insert(start) {
                // Redact only the secret run — the marker itself stays so the
                // model still sees *which* credential kind appeared.
                hits.push((secret_start, end));
                from = end;
            } else {
                from = secret_start;
            }
        }
    }
    scan_jwt(text, &mut hits);

    if hits.is_empty() {
        return (text.to_string(), 0);
    }
    hits.sort_unstable();
    hits.dedup();
    // Merge overlaps so one secret yields one `***`.
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(hits.len());
    for (start, end) in hits {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end) in &merged {
        out.push_str(&text[cursor..*start]);
        out.push_str("***");
        cursor = *end;
    }
    out.push_str(&text[cursor..]);
    let count = merged.len();
    (out, count)
}

/// JWT shapes (`eyJ…​.…​.…`) with no marker at all.
fn scan_jwt(text: &str, hits: &mut Vec<(usize, usize)>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 4 < bytes.len() {
        if &bytes[i..i + 3] == b"eyJ" {
            let mut end = i + 3;
            let mut dots = 0;
            while end < bytes.len() && is_token_char(bytes[end] as char) {
                if bytes[end] == b'.' {
                    dots += 1;
                }
                end += 1;
            }
            // Three segments, sane length: almost certainly a JWT.
            if dots == 2 && end - i > 20 && end - i < 4096 {
                hits.push((i, end));
                i = end;
                continue;
            }
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_passes_through_untouched() {
        let (out, n) = redact("fn main() {\n    println!(\"hello\");\n}");
        assert_eq!(n, 0);
        assert!(out.contains("hello"));
    }

    #[test]
    fn known_markers_redact_the_token_run() {
        let (out, n) = redact("key is sk-ant-secretvalue123 and done");
        assert_eq!(n, 1);
        assert!(out.contains("sk-ant-***"), "{out}");
        assert!(!out.contains("secretvalue123"));

        let (out, n) = redact("AKIAIOSFODNN7EXAMPLE here");
        assert_eq!(n, 1, "{out}");
        assert!(out.contains("AKIA***"));

        let (out, n) = redact("Authorization: Bearer abcdef123456");
        assert_eq!(n, 1, "{out}");
        assert!(!out.contains("abcdef123456"));
    }

    #[test]
    fn env_shouting_and_assignments_are_covered() {
        let (out, n) = redact("PASSWORD=hunter2-hunter2\nother=1");
        assert_eq!(n, 1, "{out}");
        assert!(!out.contains("hunter2"));
        assert!(out.contains("other=1"));

        let (out, _) = redact("-----BEGIN PRIVATE KEY-----\nMIIEvgIBADAN");
        assert!(!out.contains("MIIEvg"), "{out}");
    }

    #[test]
    fn server_secret_shapes_are_covered() {
        // `?key=` query auth, env-dumped master passphrase / jwt secret,
        // Google OAuth access tokens.
        let (out, n) = redact("url?key=AIzaSyD-abc123XYZ_ ok");
        assert_eq!(n, 1, "{out}");
        assert!(!out.contains("AIzaSyD"), "{out}");

        let (out, n) = redact("CYBERMANJU_MASTER_PASSPHRASE=hunter2-hunter2\nother=1");
        assert_eq!(n, 1, "{out}");
        assert!(!out.contains("hunter2"));
        assert!(out.contains("other=1"));

        let (out, n) = redact("jwt_secret=hunter2");
        assert_eq!(n, 1, "{out}");
        assert!(!out.contains("hunter2"));

        let (out, n) = redact("token ya29.a0AfH6SMBx1234567890 ok");
        assert_eq!(n, 1, "{out}");
        assert!(!out.contains("AfH6SMB"));
    }

    #[test]
    fn bare_markers_without_secrets_stay_put() {
        let (out, n) = redact("set token= then run");
        assert_eq!(n, 0, "{out}");
        assert!(out.contains("token="));
    }

    #[test]
    fn overlapping_markers_merge_into_one_replacement() {
        // `sk-` is a prefix of the longer run; one secret, one `***`.
        let (out, n) = redact("sk-ant-abc123");
        assert_eq!(n, 1, "{out}");
        assert_eq!(out.matches("***").count(), 1);
    }

    #[test]
    fn jwt_shapes_redact_without_markers() {
        let token =
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let (out, n) = redact(&format!("got {token} ok"));
        assert_eq!(n, 1, "{out}");
        assert!(!out.contains("SflKxw"));
        assert!(out.contains("got ") && out.contains(" ok"));
    }
}
