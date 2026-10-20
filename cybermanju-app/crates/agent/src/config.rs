// CyberManju OS — Config (crates/agent/src/config.rs).
// Permission evaluation: opencode/omp-compatible rulesets.
//
// Shape: `{"default": "ask", "bash": "allow", "edit": {"*": "deny", …}}`.
// Granular maps evaluate last-match-wins (catch-all first, specifics after);
// `*` spans any run, `?` exactly one char. Plan-kind sessions deny
// edit/write/bash unconditionally. `deny` always beats `auto_approve`.

use cybermanju_types::agent::{AgentKind, PermissionAction, PermissionRuleset};

/// Record an "allow always" decision: the tool becomes unconditionally
/// allowed. Explicit and reversible — the row shows exactly what "always"
/// meant, unlike an invisible always-list.
pub fn remember_allow(rules: &mut PermissionRuleset, tool: &str) {
    rules.rules.insert(
        tool.to_string(),
        cybermanju_types::agent::PermissionRule::Simple(PermissionAction::Allow),
    );
}

/// Standing orders: files folded into the system prompt by
/// `load_project_rules`, so whoever rewrites one owns every later turn.
/// Mirrors Hermes `security.protected_instruction_files` — case-insensitive
/// basename in any directory, plus the `.cybermanju/rules.md` path form.
pub fn is_protected_instruction_path(path: &str) -> bool {
    let norm = path.replace('\\', "/");
    let lower = norm.to_ascii_lowercase();
    let base = lower.rsplit('/').next().unwrap_or("");
    if base == "agents.md" || base == "skill.md" {
        return true;
    }
    lower.ends_with("/.cybermanju/rules.md") || lower == ".cybermanju/rules.md"
}

/// What the loop should do with a proposed tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    /// Pause the job and ask the user (carries a human summary).
    Ask {
        summary: String,
    },
    /// Refuse with an `unsupported:`-style machine prefix.
    Deny {
        reason: String,
    },
}

/// `*` = any run, `?` = exactly one char, everything else literal.
pub fn match_wildcard(pattern: &str, input: &str) -> bool {
    fn go(px: &[u8], ix: &[u8]) -> bool {
        if px.is_empty() {
            return ix.is_empty();
        }
        match px[0] {
            b'*' => {
                // Collapse runs, then try every split point.
                let mut p = 1;
                while p < px.len() && px[p] == b'*' {
                    p += 1;
                }
                (0..=ix.len()).any(|skip| go(&px[p..], &ix[skip..]))
            }
            b'?' => !ix.is_empty() && go(&px[1..], &ix[1..]),
            b => !ix.is_empty() && ix[0] == b && go(&px[1..], &ix[1..]),
        }
    }
    go(pattern.as_bytes(), input.as_bytes())
}

/// Glob-match `path` against `pattern`: `/`-separated segments where `*`
/// and `?` stay inside one segment and `**` crosses separators
/// (`src/**/*.rs`, `**/Cargo.toml`). Both sides are slash-normalized, so
/// Windows separators never break a match.
pub fn match_glob(pattern: &str, path: &str) -> bool {
    fn segments(text: &str) -> Vec<String> {
        text.replace('\\', "/")
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    }
    fn go(px: &[&str], ix: &[&str]) -> bool {
        if px.is_empty() {
            return ix.is_empty();
        }
        if px[0] == "**" {
            // `**` eats zero or more whole segments (collapse runs first).
            let mut p = 1;
            while p < px.len() && px[p] == "**" {
                p += 1;
            }
            return (0..=ix.len()).any(|skip| go(&px[p..], &ix[skip..]));
        }
        if ix.is_empty() {
            return false;
        }
        match_wildcard(px[0], ix[0]) && go(&px[1..], &ix[1..])
    }
    let pattern = if pattern.trim().is_empty() {
        "**"
    } else {
        pattern
    };
    let (psegs, isegs) = (segments(pattern), segments(path));
    let psegs: Vec<&str> = psegs.iter().map(String::as_str).collect();
    let isegs: Vec<&str> = isegs.iter().map(String::as_str).collect();
    go(&psegs, &isegs)
}

/// A grep pattern: real regex when it compiles, literal substring when it
/// does not (an invalid regex must search literally, never fail the tool).
pub enum GrepPattern {
    Regex(regex::Regex),
    Literal(String),
}

impl GrepPattern {
    pub fn compile(pattern: &str) -> Self {
        match regex::Regex::new(pattern) {
            Ok(re) => GrepPattern::Regex(re),
            Err(_) => GrepPattern::Literal(pattern.to_string()),
        }
    }

    pub fn is_match(&self, line: &str) -> bool {
        match self {
            GrepPattern::Regex(re) => re.is_match(line),
            GrepPattern::Literal(lit) => line.contains(lit.as_str()),
        }
    }

    pub fn is_regex(&self) -> bool {
        matches!(self, GrepPattern::Regex(_))
    }
}
/// The match string a tool call is evaluated against: `name` plus its most
/// salient argument, mirroring how `grep *` needs `"grep *"` while
/// bare `"grep"` only matches the argless call.
pub fn match_input(tool: &str, input: &serde_json::Value) -> String {
    let arg = salient_arg(input);
    if arg.is_empty() {
        tool.to_string()
    } else {
        format!("{tool} {arg}")
    }
}

/// The salient argument alone (`git status --porcelain`, not
/// `bash git status --porcelain`): opencode-style `bash` patterns match the
/// parsed command, so rules are tried against all three shapes.
/// `memory_remember` carries its fact in `text`, so content patterns
/// (`*password*`) match what would actually be stored. `repo_analyze` carries
/// its slug in `repo`, `skill_save`/`mcp_attach` in `name` (+`url`).
pub fn salient_arg(input: &serde_json::Value) -> &str {
    input
        .get("command")
        .or_else(|| input.get("pattern"))
        .or_else(|| input.get("path"))
        .or_else(|| input.get("glob"))
        .or_else(|| input.get("query"))
        .or_else(|| input.get("url"))
        .or_else(|| input.get("repo"))
        .or_else(|| input.get("name"))
        .or_else(|| input.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
}

/// Decide a tool call under a ruleset.
pub fn decide(
    rules: &PermissionRuleset,
    kind: AgentKind,
    tool: &str,
    input: &serde_json::Value,
) -> PermissionDecision {
    // Plan persona: read-only, whatever the rules say. `skill_save` writes a
    // file and `mcp_attach` mutates the config — both are mutations.
    // `os_exec` runs volume commands (mutations too). `secret_get` reveals a
    // credential (plan agents stay out of the vault entirely). `self_research`
    // and `repo_analyze` are read-only and stay available.
    if kind == AgentKind::Plan
        && matches!(
            tool,
            "edit"
                | "write"
                | "bash"
                | "skill_save"
                | "mcp_attach"
                | "os_exec"
                | "secret_list"
                | "secret_get"
                | "todo_add"
                | "todo_done"
        )
    {
        return PermissionDecision::Deny {
            reason: format!("deny: plan agent may not run `{tool}`"),
        };
    }
    let action = match rules.rules.get(tool) {
        None => rules.default,
        Some(cybermanju_types::agent::PermissionRule::Simple(a)) => *a,
        Some(cybermanju_types::agent::PermissionRule::Granular(pairs)) => {
            let target = match_input(tool, input);
            let arg = salient_arg(input);
            let mut hit = None;
            for (pattern, action) in pairs {
                if match_wildcard(pattern, &target)
                    || match_wildcard(pattern, tool)
                    || (!arg.is_empty() && match_wildcard(pattern, arg))
                {
                    hit = Some(*action);
                }
            }
            hit.unwrap_or(rules.default)
        }
    };
    match action {
        PermissionAction::Allow => {
            // Standing orders never auto-allow: a write/edit that would pass
            // silently under an `allow` rule is downgraded to a human
            // decision. `deny` still wins, and the plan persona still denies
            // above — this only ever turns an allow into an ask.
            let protected = is_protected_instruction_path(salient_arg(input));
            if matches!(tool, "write" | "edit") && protected {
                return PermissionDecision::Ask {
                    summary: format!("Approve `{tool}` — protected standing orders"),
                };
            }
            PermissionDecision::Allow
        }
        PermissionAction::Deny => PermissionDecision::Deny {
            reason: format!("deny: `{tool}` is denied by the permission ruleset"),
        },
        PermissionAction::Ask => PermissionDecision::Ask {
            summary: format!("Approve `{tool}`?"),
        },
    }
}

/// True when `tool` would be denied for every input under `rules`.
///
/// The probe calls [`decide`] with an empty object, which is exactly the
/// match shape a bare tool name sees (`match_input` falls back to the
/// tool name when no salient argument exists). A granular rule that
/// denies one argument but allows another (e.g. `bash`: deny
/// `git push *`, allow `git *`) therefore keeps the tool — stripping is
/// only for tools that can never run. The runtime `decide` gate stays in
/// place regardless, so stripping is an optimization, never the security
/// boundary.
pub fn is_tool_denied_everywhere(rules: &PermissionRuleset, kind: AgentKind, tool: &str) -> bool {
    matches!(
        decide(rules, kind, tool, &serde_json::json!({})),
        PermissionDecision::Deny { .. }
    )
}

/// Name carried by one entry of a built request `tools` array. Handles
/// all three shapes that flow through here: OpenAI
/// `{function: {name}}`, Anthropic/canonical `{name}`, and MCP
/// canonical defs merged in by the driver.
fn tool_entry_name(entry: &serde_json::Value) -> Option<&str> {
    entry
        .get("function")
        .and_then(|f| f.get("name"))
        .or_else(|| entry.get("name"))
        .and_then(|n| n.as_str())
}

/// Strip tools the ruleset denies everywhere from a built provider
/// request body (in place). Returns how many entries were removed.
///
/// Honesty edges, all covered by tests below:
/// * a default-deny ruleset strips broadly — and the runtime `decide`
///   gate would deny those calls anyway, so the model loses nothing it
///   could have used;
/// * an emptied `tools` array is removed entirely (providers reject an
///   empty array), together with `tool_choice`, so the turn becomes an
///   honest no-tools turn instead of a malformed one;
/// * granular per-argument denies never strip the whole tool (see
///   [`is_tool_denied_everywhere`]);
/// * unknown shapes are kept — a strip pass must never hide a tool it
///   cannot name.
pub fn strip_denied_tools(
    body: &mut serde_json::Value,
    rules: &PermissionRuleset,
    kind: AgentKind,
) -> usize {
    let tools = match body.get_mut("tools").and_then(|t| t.as_array_mut()) {
        Some(tools) => tools,
        None => return 0,
    };
    let before = tools.len();
    tools.retain(|entry| match tool_entry_name(entry) {
        Some(name) => !is_tool_denied_everywhere(rules, kind, name),
        None => true,
    });
    let removed = before - tools.len();
    if tools.is_empty() {
        if let Some(obj) = body.as_object_mut() {
            obj.remove("tools");
            obj.remove("tool_choice");
        }
    }
    removed
}

/// Max provider routes tried per run: the primary plus this many fallbacks
/// minus one. Bounds worst-case latency (each route gets its own backoff).
pub const MAX_FAILOVER_ROUTES: usize = 5;

/// Ordered, de-duplicated route plan: the primary first, then fallbacks (no
/// self-references, no repeats, capped). Pure so every transport shares the
/// same rule; callers resolve each id to an endpoint + key afterwards.
pub fn plan_failover_chain(primary_id: &str, fallback_ids: &[String]) -> Vec<String> {
    let mut chain = Vec::with_capacity(MAX_FAILOVER_ROUTES);
    chain.push(primary_id.to_string());
    for id in fallback_ids {
        let id = id.trim();
        if id.is_empty() || id == primary_id || chain.iter().any(|c| c == id) {
            continue;
        }
        if chain.len() >= MAX_FAILOVER_ROUTES {
            break;
        }
        chain.push(id.to_string());
    }
    chain
}

/// Terminal provider errors worth trying the next route on: dead keys,
/// throttling, transport failures and empty credits fail over. Malformed
/// requests, overflows and integrity refusals would fail identically on
/// every route, so they stop the run instead.
pub fn is_failover_worthy(error: &str) -> bool {
    matches!(
        error.split(':').next().map(str::trim),
        Some("auth") | Some("rate_limited") | Some("network") | Some("limit")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cybermanju_types::agent::PermissionRule;
    use std::collections::BTreeMap;

    fn ruleset(pairs: Vec<(&str, PermissionRule)>) -> PermissionRuleset {
        PermissionRuleset {
            default: PermissionAction::Ask,
            rules: pairs
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn failover_chain_dedupes_caps_and_drops_self() {
        let chain = plan_failover_chain(
            "a",
            &[
                "b".into(),
                "".into(),
                "a".into(),
                "b".into(),
                "c".into(),
                "d".into(),
                "e".into(),
                "f".into(),
            ],
        );
        assert_eq!(chain, vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn failover_worthy_prefixes() {
        for ok in [
            "auth: bad key",
            "rate_limited: slow down",
            "network: timeout",
            "limit: credits exhausted",
        ] {
            assert!(is_failover_worthy(ok), "{ok}");
        }
        for stop in [
            "invalid: bad request",
            "context: too large",
            "integrity: moved",
            "conflict: ambiguous",
            "unsupported: no shell",
            "too_large: 1 MiB cap",
            "cancelled",
            "",
        ] {
            assert!(!is_failover_worthy(stop), "{stop}");
        }
    }

    #[test]
    fn wildcard_matching_follows_shell_rules() {
        assert!(match_wildcard("*", "anything at all"));
        assert!(match_wildcard("git *", "git status"));
        assert!(!match_wildcard("git *", "git"));
        assert!(match_wildcard("rm ?", "rm x"));
        assert!(!match_wildcard("rm ?", "rm xy"));
        assert!(match_wildcard("*.env", ".env"));
        assert!(!match_wildcard("*.env", ".env.example"));
    }

    #[test]
    fn granular_rules_use_last_match_wins() {
        let rules = ruleset(vec![(
            "bash",
            PermissionRule::Granular(vec![
                ("*".into(), PermissionAction::Ask),
                ("git *".into(), PermissionAction::Allow),
                ("git push *".into(), PermissionAction::Deny),
            ]),
        )]);
        let run = |cmd: &str| {
            decide(
                &rules,
                AgentKind::Build,
                "bash",
                &serde_json::json!({ "command": cmd }),
            )
        };
        assert_eq!(run("git status"), PermissionDecision::Allow);
        assert!(matches!(run("rm -rf /"), PermissionDecision::Ask { .. }));
        assert!(matches!(
            run("git push origin main"),
            PermissionDecision::Deny { .. }
        ));
        // Bare-arg patterns match the command itself, opencode-style: the
        // tool prefix must not be required.
        assert!(match_wildcard("git *", "git status"));
        assert!(!match_wildcard("git *", "bash git status"));
        assert_eq!(
            salient_arg(&serde_json::json!({ "command": "git status" })),
            "git status"
        );
        assert_eq!(salient_arg(&serde_json::json!({})), "");
        // `memory_remember` carries its fact in `text`, so content patterns
        // (`*password*`) match what would actually be stored.
        assert_eq!(
            salient_arg(&serde_json::json!({ "text": "remember the deploy quirk" })),
            "remember the deploy quirk"
        );
    }

    #[test]
    fn plan_persona_denies_mutation_whatever_the_rules() {
        let mut rules = PermissionRuleset::default();
        rules.rules.insert(
            "edit".into(),
            PermissionRule::Simple(PermissionAction::Allow),
        );
        let d = decide(
            &rules,
            AgentKind::Plan,
            "edit",
            &serde_json::json!({ "path": "a.rs" }),
        );
        assert!(matches!(d, PermissionDecision::Deny { .. }));
        // …but reads still follow the rules (default ask here).
        assert!(matches!(
            decide(&rules, AgentKind::Plan, "read", &serde_json::json!({})),
            PermissionDecision::Ask { .. }
        ));
    }
}

#[cfg(test)]
mod remember_tests {
    use super::*;

    #[test]
    fn remember_allow_makes_future_calls_pass() {
        let mut rules = PermissionRuleset::default();
        assert!(matches!(
            decide(&rules, AgentKind::Build, "bash", &serde_json::json!({})),
            PermissionDecision::Ask { .. }
        ));
        remember_allow(&mut rules, "bash");
        assert_eq!(
            decide(&rules, AgentKind::Build, "bash", &serde_json::json!({})),
            PermissionDecision::Allow
        );
    }
}

#[cfg(test)]
mod glob_tests {
    use super::*;

    #[test]
    fn glob_stars_segments_and_doublestars_cross() {
        assert!(match_glob("*.rs", "main.rs"));
        assert!(!match_glob("*.rs", "src/main.rs"));
        assert!(match_glob("src/*.rs", "src/main.rs"));
        assert!(match_glob("src/**/*.rs", "src/a/b/main.rs"));
        assert!(match_glob("src/**/*.rs", "src/main.rs"));
        assert!(match_glob("**/Cargo.toml", "crates/agent/Cargo.toml"));
        assert!(match_glob("**/Cargo.toml", "Cargo.toml"));
        assert!(!match_glob("**/Cargo.toml", "Cargo.lock"));
        assert!(match_glob("**", "anything/at/all.txt"));
        assert!(match_glob("", "anything.txt"));
        assert!(match_glob("src/**", "src/a/b/c"));
        assert!(!match_glob("src/**", "other/a"));
    }

    #[test]
    fn grep_patterns_prefer_regex_then_fall_back() {
        let re = GrepPattern::compile("fn\\s+\\w+");
        assert!(re.is_regex());
        assert!(re.is_match("fn alpha() {}"));
        assert!(!re.is_match("struct Beta;"));

        // Invalid regex is a literal, never an error.
        let lit = GrepPattern::compile("a(b");
        assert!(!lit.is_regex());
        assert!(lit.is_match("has a(b inside"));
        assert!(!lit.is_match("has ab inside"));
    }
}

#[cfg(test)]
mod protected_tests {
    use super::*;
    use cybermanju_types::agent::PermissionRule;

    fn allow_ruleset(tool: &str) -> PermissionRuleset {
        let mut rules = PermissionRuleset::default();
        let allow = PermissionRule::Simple(PermissionAction::Allow);
        rules.rules.insert(tool.to_string(), allow);
        rules
    }

    #[test]
    fn standing_orders_never_auto_allow() {
        let rules = allow_ruleset("write");
        let paths = ["AGENTS.md", "AGENTS.MD", "src/AGENTS.md", "docs/SKILL.md"];
        for path in paths {
            let input = serde_json::json!({ "path": path });
            let d = decide(&rules, AgentKind::Build, "write", &input);
            assert!(matches!(d, PermissionDecision::Ask { .. }), "{path}");
        }
        let rules_md = ".cybermanju/rules.md";
        let input = serde_json::json!({ "path": rules_md });
        let d = decide(&rules, AgentKind::Build, "edit", &input);
        assert!(matches!(d, PermissionDecision::Ask { .. }));

        // An ordinary file still passes the very same allow rule.
        let input = serde_json::json!({ "path": "src/main.rs" });
        let d = decide(&rules, AgentKind::Build, "write", &input);
        assert_eq!(d, PermissionDecision::Allow);
        // Reads are untouched — only writes and edits are guarded.
        let reads = allow_ruleset("read");
        let input = serde_json::json!({ "path": "AGENTS.md" });
        let d = decide(&reads, AgentKind::Build, "read", &input);
        assert_eq!(d, PermissionDecision::Allow);
    }

    #[test]
    fn deny_still_beats_the_protection() {
        let mut rules = PermissionRuleset::default();
        let deny = PermissionRule::Simple(PermissionAction::Deny);
        rules.rules.insert("write".to_string(), deny);
        let input = serde_json::json!({ "path": "AGENTS.md" });
        let d = decide(&rules, AgentKind::Build, "write", &input);
        assert!(matches!(d, PermissionDecision::Deny { .. }));
    }

    #[test]
    fn protected_paths_match_hermes_shape() {
        assert!(is_protected_instruction_path("AGENTS.md"));
        assert!(is_protected_instruction_path("docs\\AGENTS.MD"));
        assert!(is_protected_instruction_path("a/.cybermanju/rules.md"));
        assert!(!is_protected_instruction_path("AGENTS.md.bak"));
        assert!(!is_protected_instruction_path("src/main.rs"));
        assert!(!is_protected_instruction_path(""));
    }
}

#[cfg(test)]
mod strip_tests {
    use super::*;
    use cybermanju_types::agent::PermissionRule;
    use std::collections::BTreeMap;

    fn openai_body(names: &[&str]) -> serde_json::Value {
        let tools: Vec<serde_json::Value> = names
            .iter()
            .map(|n| {
                serde_json::json!({
                    "type": "function",
                    "function": { "name": n },
                })
            })
            .collect();
        serde_json::json!({ "model": "m", "tools": tools, "tool_choice": "auto" })
    }

    fn deny_ruleset(denied: &[&str]) -> PermissionRuleset {
        let mut rules = PermissionRuleset {
            default: PermissionAction::Allow,
            rules: BTreeMap::new(),
        };
        for tool in denied {
            rules.rules.insert(
                (*tool).to_string(),
                PermissionRule::Simple(PermissionAction::Deny),
            );
        }
        rules
    }

    #[test]
    fn denied_tools_are_detected_through_the_empty_probe() {
        let rules = deny_ruleset(&["bash"]);
        assert!(is_tool_denied_everywhere(&rules, AgentKind::Build, "bash"));
        assert!(!is_tool_denied_everywhere(&rules, AgentKind::Build, "read"));
    }

    #[test]
    fn default_deny_strips_broadly_but_keeps_allowed() {
        let mut rules = PermissionRuleset {
            default: PermissionAction::Deny,
            rules: BTreeMap::new(),
        };
        rules.rules.insert(
            "read".to_string(),
            PermissionRule::Simple(PermissionAction::Allow),
        );
        let mut body = openai_body(&["read", "bash", "edit"]);
        let removed = strip_denied_tools(&mut body, &rules, AgentKind::Build);
        assert_eq!(removed, 2);
        let tools = body["tools"].as_array().expect("tools remain");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["function"]["name"], "read");
        assert_eq!(body["tool_choice"], "auto");
    }

    #[test]
    fn empty_tools_key_and_choice_are_removed_together() {
        let rules = PermissionRuleset {
            default: PermissionAction::Deny,
            rules: BTreeMap::new(),
        };
        let mut body = openai_body(&["bash"]);
        let removed = strip_denied_tools(&mut body, &rules, AgentKind::Build);
        assert_eq!(removed, 1);
        assert!(body.get("tools").is_none());
        assert!(body.get("tool_choice").is_none());
    }

    #[test]
    fn granular_arg_denies_keep_the_whole_tool() {
        let rules = PermissionRuleset {
            default: PermissionAction::Allow,
            rules: [(
                "bash".to_string(),
                PermissionRule::Granular(vec![
                    ("*".into(), PermissionAction::Allow),
                    ("git push *".into(), PermissionAction::Deny),
                ]),
            )]
            .into_iter()
            .collect(),
        };
        assert!(!is_tool_denied_everywhere(&rules, AgentKind::Build, "bash"));
        let mut body = openai_body(&["bash", "read"]);
        assert_eq!(strip_denied_tools(&mut body, &rules, AgentKind::Build), 0);
        assert_eq!(body["tools"].as_array().map(|a| a.len()), Some(2));
    }

    #[test]
    fn granular_catch_all_deny_strips_the_tool() {
        let rules = PermissionRuleset {
            default: PermissionAction::Allow,
            rules: [(
                "bash".to_string(),
                PermissionRule::Granular(vec![("*".into(), PermissionAction::Deny)]),
            )]
            .into_iter()
            .collect(),
        };
        let mut body = openai_body(&["bash", "read"]);
        assert_eq!(strip_denied_tools(&mut body, &rules, AgentKind::Build), 1);
        assert_eq!(body["tools"][0]["function"]["name"], "read");
    }

    #[test]
    fn plan_persona_strips_mutation_whatever_the_rules() {
        let mut rules = PermissionRuleset {
            default: PermissionAction::Allow,
            rules: BTreeMap::new(),
        };
        rules.rules.insert(
            "edit".to_string(),
            PermissionRule::Simple(PermissionAction::Allow),
        );
        let mut body = openai_body(&["read", "edit", "bash", "write"]);
        let removed = strip_denied_tools(&mut body, &rules, AgentKind::Plan);
        assert_eq!(removed, 3);
        let tools = body["tools"].as_array().expect("read remains");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["function"]["name"], "read");
    }

    #[test]
    fn anthropic_and_mcp_shapes_strip_by_name() {
        let rules = deny_ruleset(&["mcp__fs__read"]);
        let mut body = serde_json::json!({
            "model": "c",
            "tools": [
                { "name": "read" },
                { "name": "mcp__fs__read" },
            ],
        });
        assert_eq!(strip_denied_tools(&mut body, &rules, AgentKind::Build), 1);
        assert_eq!(body["tools"][0]["name"], "read");
    }

    #[test]
    fn missing_or_unnameable_tools_are_left_alone() {
        let rules = deny_ruleset(&["bash"]);
        let mut body = serde_json::json!({ "model": "m" });
        assert_eq!(strip_denied_tools(&mut body, &rules, AgentKind::Build), 0);
        let mut odd = serde_json::json!({
            "model": "m",
            "tools": [{ "type": "function" }],
        });
        assert_eq!(strip_denied_tools(&mut odd, &rules, AgentKind::Build), 0);
        assert_eq!(odd["tools"].as_array().map(|a| a.len()), Some(1));
    }
}
