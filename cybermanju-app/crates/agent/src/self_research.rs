// CyberManju OS — Self research (crates/agent/src/self_research.rs).
// Introspective self-research + persistent skill/MCP + GitHub repo analysis.
// Pure helpers shared by every transport (native fs, WASM volume, Docker).
// No I/O here: callers walk their own volume and call these to validate,
// shape requests and bound outputs — so the behavior is identical on
// desktop, mobile, Docker and Pages.

/// Max files a single `self_research` sweep reads.
pub const SELF_RESEARCH_MAX_FILES: usize = 12;
/// Per-file snippet cap for `self_research` output.
pub const SELF_RESEARCH_SNIPPET_CHARS: usize = 2000;
/// Whole-output cap for `self_research` / `repo_analyze`.
pub const RESEARCH_OUTPUT_CAP: usize = 32_768;
/// Per-skill file cap (mirrors the standing-orders budget).
pub const SKILL_FILE_BUDGET: usize = 8192;
/// GitHub tree paths returned per `repo_analyze` call.
pub const REPO_TREE_CAP: usize = 500;

/// Skill names live in volume paths and tool ids — keep them tight.
pub fn valid_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Volume-relative path of a persisted skill (inside the `.cybermanju`
/// container so it rides with provider data across devices).
pub fn skill_rel_path(name: &str) -> String {
    format!(".cybermanju/skills/{name}/SKILL.md")
}

/// Render a persisted skill file: YAML frontmatter + markdown body.
/// The TS mirror (`useAgent.ts`) must produce byte-identical output.
pub fn render_skill_file(name: &str, description: &str, content: &str) -> String {
    let desc = description.chars().take(500).collect::<String>();
    let body = content.chars().take(SKILL_FILE_BUDGET).collect::<String>();
    format!("---\nname: {name}\ndescription: {desc}\nversion: 1\ntools: [read, list, grep, glob]\n---\n{body}\n")
}

/// Split `owner/repo` (accepts `owner/repo`, `github.com/owner/repo`,
/// full `https://github.com/owner/repo` URLs, trailing `/` and `.git`).
pub fn parse_repo_slug(input: &str) -> Result<(String, String), String> {
    let mut s = input.trim().to_string();
    if s.is_empty() {
        return Err("invalid: repo is required (owner/repo)".to_string());
    }
    for prefix in [
        "https://github.com/",
        "http://github.com/",
        "github.com/",
        "git@github.com:",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
            break;
        }
    }
    s = s.trim_matches('/').to_string();
    if let Some(stripped) = s.strip_suffix(".git") {
        s = stripped.to_string();
    }
    // Drop any trailing path after owner/repo (tree/blob URLs).
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 {
        return Err(format!(
            "invalid: '{input}' is not owner/repo — try `owner/repo`"
        ));
    }
    let (owner, repo) = (parts[0], parts[1]);
    if !valid_repo_part(owner) || !valid_repo_part(repo) {
        return Err(format!("invalid: bad repo slug '{owner}/{repo}'"));
    }
    Ok((owner.to_string(), repo.to_string()))
}

fn valid_repo_part(part: &str) -> bool {
    !part.is_empty()
        && part.len() <= 100
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// GitHub REST API URLs for one repo analysis (no `git` binary needed —
/// that is the whole point on WASM/mobile: plain HTTPS works everywhere).
pub fn github_api_urls(owner: &str, repo: &str, branch: &str) -> (String, String, String) {
    let br = if branch.trim().is_empty() {
        "main"
    } else {
        branch.trim()
    };
    (
        format!("https://api.github.com/repos/{owner}/{repo}"),
        format!("https://api.github.com/repos/{owner}/{repo}/git/trees/{br}?recursive=1"),
        format!("https://api.github.com/repos/{owner}/{repo}/readme"),
    )
}

/// Truncate research output with a marker (never silently cut).
pub fn cap_research_output(text: &str) -> String {
    if text.len() <= RESEARCH_OUTPUT_CAP {
        return text.to_string();
    }
    let mut out = text.chars().take(RESEARCH_OUTPUT_CAP).collect::<String>();
    out.push_str("\n… truncated at 32 KiB (narrow path/limit and retry)");
    out
}

/// Filename-stem aliases so interface/cybsh questions reach the files that
/// implement them: a query like "change the theme" carries none of the
/// implementors' path stems (`shell.rs`, `staticCybsh.ts`, `tokens.ts`), so
/// without this bridge path-overlap ranking misses them entirely.
fn research_aliases(word: &str) -> &'static [&'static str] {
    match word {
        "theme" => &["tokens", "shell", "staticcybsh"],
        "accent" => &["tokens", "theme", "shell"],
        "ui" => &["tokens", "theme", "panel", "shell", "staticcybsh"],
        "interface" => &["ui", "tokens", "theme", "panel"],
        "appearance" => &["tokens", "theme"],
        "cybsh" => &["shell", "staticcybsh"],
        "shell" => &["cybsh", "staticcybsh"],
        "terminal" => &["terminal", "shell", "cybsh"],
        "verb" => &["shell", "cybsh", "staticcybsh"],
        "command" => &["shell", "cybsh"],
        "glass" | "density" | "motion" | "glow" | "palette" => &["tokens", "theme"],
        "color" | "colour" => &["tokens", "theme", "palette"],
        "font" | "radius" | "shadow" => &["tokens", "theme"],
        _ => &[],
    }
}

/// Rank candidate files for a `self_research` query: keyword overlap over
/// the path (name 1.0, extension bonus for source files), expanded through
/// [`research_aliases`] so UI/cybsh questions bridge to implementor paths.
/// Deterministic (score desc, path asc). No overlap → `[]`.
pub fn rank_research_files(paths: &[String], query: &str, limit: usize) -> Vec<String> {
    let mut want: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() >= 2)
        .map(str::to_string)
        .collect();
    if want.is_empty() {
        return Vec::new();
    }
    for i in 0..want.len() {
        for alias in research_aliases(&want[i]) {
            if !want.iter().any(|w| w == alias) {
                want.push(alias.to_string());
            }
        }
    }
    let mut scored: Vec<(&String, i64)> = paths
        .iter()
        .map(|p| {
            let lower = p.to_lowercase();
            let mut score: i64 = 0;
            for w in &want {
                if lower.contains(w.as_str()) {
                    score += 1;
                }
            }
            // Source files answer "how does it work" better than assets.
            if score > 0
                && (lower.ends_with(".rs")
                    || lower.ends_with(".ts")
                    || lower.ends_with(".vue")
                    || lower.ends_with(".md"))
            {
                score += 1;
            }
            (p, score)
        })
        .filter(|(_, s)| *s > 0)
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    scored
        .into_iter()
        .take(limit.clamp(1, SELF_RESEARCH_MAX_FILES))
        .map(|(p, _)| p.clone())
        .collect()
}

/// Summarize a GitHub tree listing into a bounded human summary.
/// `tree_json` is the decoded `GET .../git/trees/{br}?recursive=1` body.
pub fn summarize_repo_tree(
    slug: &str,
    branch: &str,
    repo_json: &serde_json::Value,
    tree_json: &serde_json::Value,
    readme_head: &str,
) -> String {
    let description = repo_json
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let language = repo_json
        .get("language")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let stars = repo_json
        .get("stargazers_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let default_branch = repo_json
        .get("default_branch")
        .and_then(|v| v.as_str())
        .unwrap_or(branch);
    let mut paths: Vec<String> = Vec::new();
    let mut truncated = false;
    if let Some(arr) = tree_json.get("tree").and_then(|t| t.as_array()) {
        truncated = tree_json
            .get("truncated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        for entry in arr.iter().take(REPO_TREE_CAP) {
            if let Some(path) = entry.get("path").and_then(|p| p.as_str()) {
                if entry.get("type").and_then(|t| t.as_str()) == Some("blob") {
                    paths.push(path.to_string());
                }
            }
        }
    }
    // Language guess from extensions (no clone, no linguist — heuristic).
    let mut exts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for p in &paths {
        if let Some(ext) = p
            .rsplit('.')
            .next()
            .filter(|e| e.len() <= 8 && *e != p.as_str())
        {
            *exts.entry(ext.to_lowercase()).or_insert(0) += 1;
        }
    }
    let mut top_exts: Vec<(String, usize)> = exts.into_iter().collect();
    top_exts.sort_by_key(|a| std::cmp::Reverse(a.1));
    top_exts.truncate(8);
    let top_exts = top_exts
        .iter()
        .map(|(e, n)| format!("{e}×{n}"))
        .collect::<Vec<_>>()
        .join(" ");
    // Build files worth opening first.
    let interesting = [
        "README.md",
        "ARCHITECTURE.md",
        "AGENTS.md",
        "Cargo.toml",
        "package.json",
        "go.mod",
        "pyproject.toml",
        "Dockerfile",
        "docker-compose.yml",
    ];
    let mut entry_points: Vec<String> = Vec::new();
    for name in interesting {
        if paths
            .iter()
            .any(|p| p == name || p.ends_with(&format!("/{name}")))
        {
            entry_points.push(name.to_string());
        }
    }
    let entry_list = if entry_points.is_empty() {
        "(no standard entry files)".to_string()
    } else {
        entry_points.join(", ")
    };
    let mut out = format!(
        "repo: {slug} (branch {default_branch}, ★{stars}, primary {language})\n\
         desc: {}\n\
         files: {} blobs{} | exts: {}\n\
         start here: {}\n",
        if description.is_empty() {
            "(no description)"
        } else {
            description
        },
        paths.len(),
        if truncated || paths.len() >= REPO_TREE_CAP {
            " (tree capped — open subpaths for more)"
        } else {
            ""
        },
        if top_exts.is_empty() {
            "(none)"
        } else {
            &top_exts
        },
        entry_list,
    );
    // Top-level layout (first segment) — the orientation an agent needs.
    let mut top: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for p in &paths {
        let first = p.split('/').next().unwrap_or(p);
        *top.entry(first.to_string()).or_insert(0) += 1;
    }
    let mut top_vec: Vec<(String, usize)> = top.into_iter().collect();
    top_vec.sort_by_key(|a| std::cmp::Reverse(a.1));
    top_vec.truncate(20);
    out.push_str("layout:\n");
    for (dir, n) in &top_vec {
        out.push_str(&format!("  {dir}/ ({n})\n"));
    }
    if !readme_head.trim().is_empty() {
        let head: String = readme_head.chars().take(1500).collect();
        out.push_str(&format!("readme:\n{head}\n"));
    }
    out.push_str("next: read entry files above, then glob/grep the layout dirs.");
    cap_research_output(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_names_are_tight() {
        assert!(valid_skill_name("my-skill_1"));
        assert!(valid_skill_name("a"));
        assert!(!valid_skill_name(""));
        assert!(!valid_skill_name("has space"));
        assert!(!valid_skill_name("has/slash"));
        assert!(!valid_skill_name(&"x".repeat(65)));
    }

    #[test]
    fn skill_paths_live_in_the_cybermanju_container() {
        assert_eq!(
            skill_rel_path("review"),
            ".cybermanju/skills/review/SKILL.md"
        );
        let file = render_skill_file("review", "reviews code", "body here");
        assert!(file.starts_with("---\nname: review\n"));
        assert!(file.contains("body here"));
    }

    #[test]
    fn repo_slugs_accept_urls_and_suffixes() {
        assert_eq!(
            parse_repo_slug("owner/repo").expect("slug"),
            ("owner".to_string(), "repo".to_string())
        );
        assert_eq!(
            parse_repo_slug("https://github.com/owner/repo.git").expect("url"),
            ("owner".to_string(), "repo".to_string())
        );
        assert_eq!(
            parse_repo_slug("github.com/owner/repo/tree/main/docs").expect("tree"),
            ("owner".to_string(), "repo".to_string())
        );
        assert!(parse_repo_slug("justone").is_err());
        assert!(parse_repo_slug("").is_err());
        assert!(parse_repo_slug("a/b/c/d/e").is_ok());
        assert!(parse_repo_slug("bad slug/repo").is_err());
    }

    #[test]
    fn api_urls_need_no_git_binary() {
        let (meta, tree, readme) = github_api_urls("o", "r", "");
        assert!(meta.contains("/repos/o/r"));
        assert!(tree.contains("git/trees/main?recursive=1"));
        assert!(readme.ends_with("/readme"));
        let (_, tree2, _) = github_api_urls("o", "r", "dev");
        assert!(tree2.contains("/dev?recursive=1"));
    }

    #[test]
    fn research_ranking_prefers_source_and_is_deterministic() {
        let paths = vec![
            "logo.png".to_string(),
            "src/agent_loop.rs".to_string(),
            "docs/agent-review.md".to_string(),
        ];
        let ranked = rank_research_files(&paths, "agent loop", 5);
        assert!(ranked.contains(&"src/agent_loop.rs".to_string()));
        assert!(!ranked.contains(&"logo.png".to_string()));
        assert!(rank_research_files(&paths, "", 5).is_empty());
        assert!(rank_research_files(&paths, "zzzqqq", 5).is_empty());
    }

    #[test]
    fn research_aliases_bridge_ui_questions_to_implementors() {
        let paths = vec![
            "crates/os/src/shell.rs".to_string(),
            "src/utils/staticCybsh.ts".to_string(),
            "src/ui/tokens.ts".to_string(),
            "src/composables/useTheme.ts".to_string(),
            "docs/OPERATIONS.md".to_string(),
            "src/components/TerminalPanel.vue".to_string(),
        ];
        let themed = rank_research_files(&paths, "change the interface theme", 6);
        assert!(themed.contains(&"src/ui/tokens.ts".to_string()));
        assert!(themed.contains(&"crates/os/src/shell.rs".to_string()));
        assert!(themed.contains(&"src/composables/useTheme.ts".to_string()));
        let verbs = rank_research_files(&paths, "cybsh ui verbs", 6);
        assert!(verbs.contains(&"src/utils/staticCybsh.ts".to_string()));
        assert!(verbs.contains(&"crates/os/src/shell.rs".to_string()));
        let accent = rank_research_files(&paths, "per theme accent color", 6);
        assert!(accent.contains(&"src/ui/tokens.ts".to_string()));
    }

    #[test]
    fn repo_summary_is_bounded_and_names_entry_files() {
        let repo = serde_json::json!({
            "description": "demo",
            "language": "Rust",
            "stargazers_count": 7,
            "default_branch": "main",
        });
        let tree = serde_json::json!({
            "truncated": false,
            "tree": [
                {"path": "README.md", "type": "blob"},
                {"path": "Cargo.toml", "type": "blob"},
                {"path": "src/main.rs", "type": "blob"},
                {"path": "src", "type": "tree"},
            ],
        });
        let out = summarize_repo_tree("o/r", "main", &repo, &tree, "# hi");
        assert!(out.contains("repo: o/r"));
        assert!(out.contains("README.md"));
        assert!(out.len() <= RESEARCH_OUTPUT_CAP + 128);
    }

    #[test]
    fn research_output_caps_with_a_marker() {
        let big = "x".repeat(RESEARCH_OUTPUT_CAP + 10);
        let capped = cap_research_output(&big);
        assert!(capped.contains("truncated at 32 KiB"));
        assert_eq!(cap_research_output("small"), "small");
    }
}
