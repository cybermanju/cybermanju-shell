// CyberManju OS — Agent loop (crates/agent/src/agent_loop.rs).
// Turn state machine: pure message bookkeeping around provider calls.
//
// The loop itself performs no I/O, spawns no threads, and holds no locks —
// the caller (native worker thread, WASM async task) owns HTTP, tool
// execution and approvals, and drives one turn at a time:
//
// ```text
// build_request → POST → ingest_reply → ToolCalls(calls)?
//     → permission check → execute → append_tool_results → build_request → …
// ```
//
// Subagent depth is capped here so `task` cannot recurse forever.

use cybermanju_types::agent::{ChatMessage, LlmDialect, ShellMode, TokenUsage, ToolCall};

/// Nobody nests deeper than one subagent level.
pub const MAX_TASK_DEPTH: u32 = 1;

/// Hard ceiling even when a config asks for more.
pub const MAX_TURNS_HARD_CAP: u32 = 50;

/// What `ingest_reply` reports back to the driver.
#[derive(Debug, Clone, PartialEq)]
pub enum LoopEvent {
    /// Assistant text, no tool calls — the turn is over.
    TextDone,
    /// The model wants tools run (already appended as an assistant message).
    ToolCalls(Vec<ToolCall>),
    /// Turn budget exhausted — the driver must stop.
    LimitReached,
}

/// Mutable turn state. Bounded, cloneable, serializable via the session.
#[derive(Debug, Clone)]
pub struct AgentTurn {
    pub messages: Vec<ChatMessage>,
    pub turns_used: u32,
    pub max_turns: u32,
    pub usage: TokenUsage,
    pub task_depth: u32,
    /// Raw finish reason of the last reply (`stop`, `tool_calls`,
    /// `length`, …) — the driver uses it to notice truncation.
    pub last_finish: String,
}

impl AgentTurn {
    pub fn new(messages: Vec<ChatMessage>, max_turns: u32, task_depth: u32) -> Self {
        Self {
            messages,
            turns_used: 0,
            max_turns: max_turns.clamp(1, MAX_TURNS_HARD_CAP),
            usage: TokenUsage::default(),
            task_depth,
            last_finish: String::new(),
        }
    }

    /// Build the next provider request. Returns URL, headers and body; the
    /// driver POSTs them with whatever transport it owns.
    pub fn build_request(
        &self,
        base_url: &str,
        dialect: LlmDialect,
        model: &str,
        system: &str,
        headers: Vec<(String, String)>,
        tools: bool,
    ) -> (String, Vec<(String, String)>, serde_json::Value) {
        let url = crate::providers::chat_url(base_url, dialect);
        let body = match dialect {
            LlmDialect::OpenAi => {
                crate::protocol::openai_request(model, system, &self.messages, tools)
            }
            LlmDialect::Anthropic => {
                crate::protocol::anthropic_request(model, system, &self.messages, tools)
            }
        };
        (url, headers, body)
    }

    /// Fold a provider reply into the transcript. Counts the turn, adds the
    /// assistant message, accumulates usage.
    pub fn ingest_reply(
        &mut self,
        dialect: LlmDialect,
        reply: &serde_json::Value,
    ) -> Result<LoopEvent, String> {
        if self.turns_used >= self.max_turns {
            return Ok(LoopEvent::LimitReached);
        }
        self.turns_used += 1;
        let turn = match dialect {
            LlmDialect::OpenAi => crate::protocol::openai_parse(reply)?,
            LlmDialect::Anthropic => crate::protocol::anthropic_parse(reply)?,
        };
        self.usage.input_tokens += turn.usage.input_tokens;
        self.usage.output_tokens += turn.usage.output_tokens;
        self.last_finish = turn.finish.clone();
        if !turn.tool_calls.is_empty() {
            let wire = assistant_tool_wire(&turn.tool_calls);
            self.messages.push(ChatMessage {
                role: "assistant_tool".into(),
                content: turn.content.clone(),
                tool_call_id: None,
                tool_name: None,
                tool_input: Some(wire),
            });
            Ok(LoopEvent::ToolCalls(turn.tool_calls))
        } else {
            self.messages.push(ChatMessage {
                role: "assistant".into(),
                content: turn.content,
                tool_call_id: None,
                tool_name: None,
                tool_input: None,
            });
            Ok(LoopEvent::TextDone)
        }
    }

    /// Append one executed tool result to the transcript.
    pub fn append_tool_result(&mut self, call: &ToolCall, output: String) {
        self.messages.push(ChatMessage {
            role: "tool".into(),
            content: output,
            tool_call_id: Some(call.id.clone()),
            tool_name: Some(call.name.clone()),
            tool_input: None,
        });
    }
}

/// Wire-format tool-call array for an assistant message (OpenAI shape; the
/// Anthropic builder re-derives its blocks from the same array).
pub fn assistant_tool_wire(calls: &[ToolCall]) -> serde_json::Value {
    calls
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "name": c.name,
                "input": c.input,
            })
        })
        .collect()
}

/// Which sandbox the agent runs in. The metaprompt below is honest about
/// capabilities per transport — a browser agent has the cybsh volume
/// subset, depth-1 subagents and HTTP MCP (device shell + stdio MCP stay
/// native and answer `unsupported:` there).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sandbox {
    /// Desktop/server: bash, task subagents, MCP servers.
    Native,
    /// Browser volume: file tools + cybsh-subset bash + depth-1 task + HTTP MCP.
    Browser,
}

/// Compact system prompt. The caller fills `repo_overview` (a top-level
/// listing or tree-sitter outline, capped) so every turn starts grounded.
/// `shell_mode` selects the `bash` paragraph so the model is never told it
/// can run a shell the config forbids.
pub fn system_prompt(
    working_dir: &str,
    agent_kind: &str,
    repo_overview: &str,
    sandbox: Sandbox,
    shell_mode: ShellMode,
) -> String {
    let sandbox_rules = match sandbox {
        Sandbox::Native => {
            "SANDBOX: full tools — bash (cybsh volume commands + shell \
             fallback for curl/wget/git), one level of task subagents, and \
             attached MCP servers (mcp__* tools, incl. keyless Exa web \
             search). Destructive tools pause for human approval;"
        }
        Sandbox::Browser => {
            "SANDBOX: browser file volume — read/list/grep/glob/write/edit \
              plus cybsh-subset bash, one bounded task subagent (full file/shell tools), \
             and attached HTTP MCP servers (mcp__*). Device shell \
             (curl/wget/git/python) and stdio MCP need the dashboard and \
             answer unsupported: there;"
        }
    };
    let shell_para: &str = match (sandbox, shell_mode) {
        (Sandbox::Browser, _) => {
            "- bash {{command, timeout_secs?}}: cybsh volume subset in this \
             browser sandbox (ls/cat/cp/mv/rm/mkdir/search/compute, same \
             shell as the Terminal panel; pass explicit paths). Device verbs \
             answer unsupported: — never promise their results.\n"
        }
        (_, ShellMode::Cybsh) => {
            "- bash {{command, timeout_secs?}}: cybsh ONLY — the same shell as \
             the Terminal panel: ls/cd/pwd/cat/cp/mv/rm/mkdir/touch/stat/du/df/\
             disk/mount/search/sync/scrub/repair/gc/lease/ps/compute/keygen/\
             encrypt/decrypt/ai (pass explicit paths, cwd is shared). Anything \
             else fails with unsupported: — never use curl/wget/git/python \
             here. For web search use mcp__exa__web_search_exa, for pages \
             mcp__exa__web_fetch_exa. Never run interactive commands.\n"
        }
        (_, ShellMode::Device) => {
            "- bash {{command, timeout_secs?}}: device shell ONLY (`sh -c` on \
             this machine, default 120s, 5–600). cybsh volume commands do NOT \
             exist here — use POSIX tools (ls/cat/grep/find) on the working \
             root instead. curl/wget for raw fetch, git/python/node as needed. \
             For web search prefer mcp__exa__web_search_exa, for pages \
             mcp__exa__web_fetch_exa. Never run interactive commands.\n"
        }
        (_, ShellMode::Auto) => {
            "- bash {{command, timeout_secs?}}: cybsh FIRST for volume work — \
             ls/cd/pwd/cat/cp/mv/rm/mkdir/touch/stat/du/df/disk/mount/search/\
             sync/scrub/repair/gc/lease/ps/compute/keygen/encrypt/decrypt/ai \
             (same shell as the Terminal panel; pass explicit paths, cwd is \
             shared). Shell fallback for network and VCS: curl/wget for raw \
             fetch, git/python/node as needed. For web search prefer \
             mcp__exa__web_search_exa, for pages mcp__exa__web_fetch_exa. \
             Never run interactive commands.\n"
        }
    };
    format!(
        "You are CyberManju, an AI coding agent operating inside a decentralized file volume.\n\
         Working root: {working_dir}\n\
         Agent mode: {agent_kind} (plan = read-only: never call edit, write, or bash).\n\
         {sandbox_rules}\n\
         TOOLS — call them with exact JSON arguments. Paths are volume paths: \
         a leading `/` means the volume root, anything else is relative to \
         the working root.\n\
         - read {{path}}: UTF-8 text up to 1 MiB, ending with a `[blake3:<hex>]` line you \
         can hand back as expected_hash. Always read a file before editing it, and never \
         write that line back — write/edit strip it automatically.\n\
         - list {{path?}}: one directory level. Orient here first (`/` first, then drill in).\n\
         - grep {{pattern, path?, limit?}}: regex over file contents (an invalid regex \
         searches literally). Locate code with it; never guess locations.\n\
         - glob {{pattern, path?}}: find files by pattern (`src/**/*.rs`, `**/Cargo.toml`). \
         Prefer it over listing whole trees.\n\
         - edit {{path, old_block, new_block, expected_hash?}}: replace ONE exact block. \
         It fails when the block is missing (not_found:) or ambiguous (conflict:) — then \
         re-read and send a larger unique block. Pass the `[blake3:<hex>]` line a prior read \
         printed (or the blake3: a prior write/edit returned) as expected_hash when writers \
         may race you.\n\
         - write {{path, content}}: full-file create/overwrite (versioned where supported). \
         Prefer edit for small changes.\n\
         {shell_para}\
          - task {{goal, context?, background?}}: one bounded subagent with the full file/shell toolset \
          (read/edit/write/bash/memory included) for delegated work — spawn as many as the work needs. \
          Omit background (or false) to wait for the result; pass background:true to detach: you get an id \
          back immediately, keep working, and the result arrives automatically as a new message.\n\
          - question {{question}}: ask the human when genuinely blocked — sparingly.\n\
          - self_research {{query, path?, limit?}}: introspect YOUR OWN source code — \
          sweep the repo for how something works (agent loop, permissions, MCP, \
          memory, tools, interface themes, cybsh verbs) and get file:line-grounded snippets back. Read-only and \
          plan-safe. Call it before claiming how this system works; never answer \
          from memory when you can read the code.\n\
          - skill_save {{name, description, content}}: persist a reusable skill into \
          the `.cybermanju` container (`.cybermanju/skills/<name>/SKILL.md`) so it \
          survives restarts and syncs across devices with provider data. Self-research \
          first, then save one focused skill per call.\n\
          - mcp_attach {{name, url, headers?}}: attach a Streamable-HTTP MCP server to \
          this assistant persistently (verified before saving; works on every device). \
          stdio servers are refused here — attach those from the desktop UI.\n\
          - repo_analyze {{repo, branch?}}: analyze any public GitHub repo WITHOUT \
          cloning (metadata + file tree + README over HTTPS; no git binary needed, so \
          it works on desktop, Docker, mobile and WASM). Returns layout, languages, \
          entry files and next reads.\n\
          - os_exec {{command}}: run ONE cybsh line against the OS volume — the same \
          verbs as bash's cybsh subset, but never a device shell (a non-cybsh verb \
          answers unsupported:). One line per call; no pipes or chains.\n\
          - ui_open_panel {{panel, tab?, path?}}: open a UI panel for the user \
          (files, terminal, agent, settings, …). Unknown ids answer not_found:; \
          optional tab/path navigate inside the panel. Read-only and always allowed.\n\
          - ui_notify {{level, message}}: show the user a notification \
          (info/success/warning/error). Always allowed; use sparingly.\n\
          - secret_list {{}}: list vault secret METADATA only (id/title/kind/\
          username/url/tags/hasValue) — never values. Pair with secret_get \
          only when the user explicitly asked you to use a stored credential.\n\
          - secret_get {{id}}: reveal ONE stored secret's plaintext. The \
          result carries the plaintext to you — default permission is ASK \
          (the approval card shows the title, never the value). Only call \
          it when the user explicitly asked you to use this credential.\n\
         - memory_recall {{query, top_k?}}: search long-term memory (past sessions, \
         stored facts). Recalled context is bounded and may be stale — verify against \
         the volume before acting on it.\n\
         - memory_remember {{text}}: store ONE durable fact for future sessions \
         (a decision, preference, environment quirk, lesson learned). One fact per \
         call; check memory_recall first so facts are not stored twice; never store \
         secrets or whole files.\n\
         - todo_add {{title}} / todo_list {{}} / todo_done {{id, done?}}: the user's \
         shared task list (same list as the Archive TASKS tab). List before \
         completing so ids are exact; one clear title per add.\n\
         INTERFACE: the Terminal panel runs cybsh — the same shell as your bash \
         tool. Its `theme`/`ui` verbs customize the whole interface on every \
         transport and persist to the volume mirror: `ui theme <id>`, \
         `ui accent <#hex|default> [--for <theme>]`, \
         `ui density|glass|motion|glow`, and inspect with `ui get`, \
         `ui vars [filter]`, `ui palette [theme]`. When asked about the \
         interface or cybsh verbs, call self_research first (topics: themes, \
         cybsh verbs) and cite file:line — never invent verb names.\n\
         MEMORY: recalled memories arrive bounded in the system prompt — that is \
         semantic memory, not the transcript. Store durable facts with \
         memory_remember instead of repeating them every turn; the transcript \
         compacts, memory persists.\n\
          WORKFLOW: orient (list/glob) → read → act (edit/write) → verify (re-read, \
          grep, run tests via bash). Small verified steps; never invent file contents.\n\
          SELF-UNDERSTANDING: when asked how YOU work, call self_research first and \
          cite the files it returns; extend yourself with skill_save (knowledge) and \
          mcp_attach (HTTP tools), both persisted in `.cybermanju`.\n\
         APPROVALS: some calls pause for human approval (allow/deny). A denial is \
         information — work around it or explain; never retry identically (three \
         identical repeats are auto-denied).\n\
         ERRORS carry machine prefixes — report them verbatim: auth: (key missing or \
         rejected — tell the user, do not retry), rate_limited: (back off; the harness \
         retries), context: (transcript too large for this model — ask the human to \
         compact the session; retrying unchanged cannot work), not_found:, \
         unsupported: (capability absent on this transport), \
         too_large: (narrow scope), integrity: (hash moved under you — re-read), network:.\n\
         OUTPUT: answer concisely; lead with what changed (file:line), then how to \
         verify. No chain-of-thought dumps.\n\
         Repository overview:\n{repo_overview}"
    )
}

/// Format a capped directory listing for the system prompt.
pub fn repo_overview_snippet(entries: &[String], cap: usize) -> String {
    let mut out = entries
        .iter()
        .take(cap)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    if entries.len() > cap {
        out.push_str(&format!("\n… {} more entries", entries.len() - cap));
    }
    if out.is_empty() {
        out.push_str("(empty working root)");
    }
    out
}

/// Max detached background subagents per run. Each holds a worker thread
/// plus up to 5 provider calls — the cap bounds fan-out, never the total:
/// finished slots are reusable, so a run may spawn as many subagents as the
/// work needs, 8 at a time.
pub const MAX_BG_SUBAGENTS: usize = 8;

/// Tool result for a detached `task background:true` spawn: the id the
/// completion will arrive under. Shared verbatim by every transport so the
/// model learns one shape.
pub fn bg_started_message(id: &str, goal: &str) -> String {
    format!(
        "background subagent {id} started for `{goal}` — keep working; its result arrives automatically as a new message."
    )
}

/// Transcript injection when a background subagent finishes. Pushed as a
/// `user` message so both dialects carry it without tool-id bookkeeping.
pub fn bg_finished_message(id: &str, summary: &str) -> String {
    format!("background subagent {id} finished:\n{summary}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> ChatMessage {
        ChatMessage {
            role: "user".into(),
            content: text.into(),
            tool_call_id: None,
            tool_name: None,
            tool_input: None,
        }
    }

    #[test]
    fn turn_counts_budget_and_accumulates_usage() {
        let mut turn = AgentTurn::new(vec![user("hi")], 2, 0);
        let (url, _, body) = turn.build_request(
            "https://api.openai.com/v1",
            LlmDialect::OpenAi,
            "gpt-5",
            "sys",
            vec![],
            false,
        );
        assert!(url.ends_with("/chat/completions"));
        assert_eq!(body["messages"][0]["role"], "system");

        let reply = serde_json::json!({
            "choices": [{ "finish_reason": "stop",
                "message": { "content": "hello" } }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 2 },
        });
        assert_eq!(
            turn.ingest_reply(LlmDialect::OpenAi, &reply)
                .expect("ingest"),
            LoopEvent::TextDone
        );
        assert_eq!(turn.turns_used, 1);
        assert_eq!(turn.usage.input_tokens, 5);
        assert_eq!(turn.messages.len(), 2);

        let reply2 = serde_json::json!({
            "choices": [{ "finish_reason": "stop",
                "message": { "content": "again" } }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1 },
        });
        turn.ingest_reply(LlmDialect::OpenAi, &reply2)
            .expect("ingest");
        assert_eq!(
            turn.ingest_reply(LlmDialect::OpenAi, &reply2)
                .expect("limit"),
            LoopEvent::LimitReached
        );
    }

    #[test]
    fn tool_results_append_as_tool_messages() {
        let mut turn = AgentTurn::new(vec![user("go")], 5, 0);
        let call = ToolCall {
            id: "call_9".into(),
            name: "read".into(),
            input: serde_json::json!({ "path": "a.rs" }),
        };
        turn.append_tool_result(&call, "fn a() {}".into());
        let last = turn.messages.last().expect("msg");
        assert_eq!(last.role, "tool");
        assert_eq!(last.tool_call_id.as_deref(), Some("call_9"));
        let wire = assistant_tool_wire(std::slice::from_ref(&call));
        assert_eq!(wire[0]["name"], "read");
    }

    #[test]
    fn system_prompt_names_tools_and_sandbox_honestly() {
        let native = system_prompt("/vol", "build", "a.rs", Sandbox::Native, ShellMode::Auto);
        assert!(native.contains("Working root: /vol"));
        assert!(native.contains("glob"));
        assert!(native.contains("expected_hash"));
        assert!(native.contains("bash"));
        assert!(native.contains("context:"));
        assert!(!native.contains("NO bash"));
        let browser = system_prompt("/vol", "plan", "a.rs", Sandbox::Browser, ShellMode::Auto);
        assert!(browser.contains("cybsh-subset"));
        assert!(browser.contains("read-only"));
        assert!(browser.contains("unsupported:"));
        assert!(native.contains("ui palette"));
        assert!(native.contains("--for <theme>"));
        assert!(browser.contains("ui palette"));
    }

    #[test]
    fn every_prompt_tool_exists_in_the_schema() {
        // The prompt must never advertise a tool the schema omits — providers
        // refuse calls for tool names they were not shown.
        for tool in [
            "question",
            "task",
            "edit",
            "bash",
            "memory_recall",
            "memory_remember",
            "self_research",
            "skill_save",
            "mcp_attach",
            "repo_analyze",
            "os_exec",
            "ui_open_panel",
            "ui_notify",
            "secret_list",
            "secret_get",
            "todo_add",
            "todo_list",
            "todo_done",
        ] {
            assert!(
                crate::protocol::TOOL_NAMES.contains(&tool),
                "prompt mentions `{tool}` but TOOL_NAMES does not"
            );
        }
        let defs = crate::protocol::tool_definitions();
        assert!(defs.iter().any(|d| d["name"] == "question"));
    }

    #[test]
    fn background_messages_carry_stable_ids() {
        let started = bg_started_message("bg-3", "audit auth");
        assert!(started.contains("bg-3"));
        assert!(started.contains("audit auth"));
        assert!(started.contains("keep working"));
        let done = bg_finished_message("bg-3", "all clean");
        assert!(done.starts_with("background subagent bg-3 finished:\n"));
        assert!(done.ends_with("all clean"));
        assert_eq!(MAX_BG_SUBAGENTS, 8);
    }

    #[test]
    fn system_prompt_and_overview_are_bounded() {
        let entries: Vec<String> = (0..300).map(|i| format!("f{i}.rs")).collect();
        let snippet = repo_overview_snippet(&entries, 200);
        assert!(snippet.contains("100 more entries"));
        let prompt = system_prompt("/vol", "build", &snippet, Sandbox::Native, ShellMode::Auto);
        assert!(prompt.contains("/vol"));
        assert!(repo_overview_snippet(&[], 10).contains("empty"));
    }

    #[test]
    fn shell_mode_paragraphs_match_the_runtime() {
        let cybsh = system_prompt("/v", "build", "r", Sandbox::Native, ShellMode::Cybsh);
        assert!(cybsh.contains("cybsh ONLY"));
        assert!(cybsh.contains("unsupported:"));
        let device = system_prompt("/v", "build", "r", Sandbox::Native, ShellMode::Device);
        assert!(device.contains("device shell ONLY"));
        assert!(device.contains("do NOT"));
        assert!(device.contains("exist here"));
        let auto = system_prompt("/v", "build", "r", Sandbox::Native, ShellMode::Auto);
        assert!(auto.contains("cybsh FIRST"));
        assert!(auto.contains("Shell fallback"));
    }
}
