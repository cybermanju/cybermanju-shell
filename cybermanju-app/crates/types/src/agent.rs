// CyberManju OS — shared AI agent types.
//
// Used by the Tauri desktop app, the Docker web server, and the WASM
// dispatcher. Raw provider keys NEVER appear here: configs carry no key
// field at all (keys live in the `sync_secrets` side table under
// `agent:key:<config_id>`); list/get responses report `hasKey` only.

use serde::{Deserialize, Serialize};

/// How a provider authenticates.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AuthScheme {
    /// `Authorization: Bearer <key>`.
    #[default]
    Bearer,
    /// Custom header, e.g. `x-api-key`.
    Header,
    /// `?key=<key>` query parameter.
    Query,
    /// No credential (local runtimes like Ollama).
    None,
}

/// Which chat wire format a provider speaks.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LlmDialect {
    /// OpenAI `/chat/completions` shape (also serves OpenRouter, Ollama
    /// `/v1`, Gemini OpenAI-compat, Groq, Mistral, DeepSeek, xAI, Cerebras).
    #[default]
    OpenAi,
    /// Anthropic `/v1/messages` shape.
    Anthropic,
}

/// One provider preset. `baseUrl` is the chat-completions-compatible root
/// (dialect decides the suffix); every field is user-overridable per config.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreset {
    pub id: String,
    pub label: String,
    pub family: String,
    pub base_url: String,
    pub default_model: String,
    pub dialect: LlmDialect,
    pub auth: AuthScheme,
    /// Header name when `auth == Header`, query name when `Query`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_name: Option<String>,
    /// Env var hint for BYOK paste (e.g. `ANTHROPIC_API_KEY`).
    pub key_env: String,
    /// True when no key is needed at all.
    #[serde(default)]
    pub keyless: bool,
    /// Extra headers always sent (e.g. OpenRouter attribution).
    #[serde(default)]
    pub extra_headers: Vec<(String, String)>,
}

/// One approval decision.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionAction {
    /// Run without asking.
    Allow,
    /// Pause the job and ask (default).
    #[default]
    Ask,
    /// Block, even in auto mode.
    Deny,
}

/// One tool's rule: either a single action or granular pattern → action
/// pairs (`*` matches any run, `?` one char; last match wins — put the
/// catch-all first, specifics after, like opencode/omp).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum PermissionRule {
    Simple(PermissionAction),
    Granular(Vec<(String, PermissionAction)>),
}

/// Full permission set for a config. Absent tools fall back to `default`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRuleset {
    #[serde(default = "default_ask")]
    pub default: PermissionAction,
    #[serde(default)]
    pub rules: std::collections::BTreeMap<String, PermissionRule>,
}

fn default_ask() -> PermissionAction {
    PermissionAction::Ask
}

impl Default for PermissionRuleset {
    fn default() -> Self {
        Self {
            default: PermissionAction::Ask,
            rules: std::collections::BTreeMap::new(),
        }
    }
}

/// Which agent persona runs a session.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentKind {
    /// Full access within the permission ruleset.
    #[default]
    Build,
    /// Read-only: edits/writes/bash denied regardless of rules.
    Plan,
}

/// Which shell the agent's `bash` tool runs on native transports.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ShellMode {
    /// cybsh volume commands, device-shell fallback for the rest.
    #[default]
    Auto,
    /// Always cybsh: unknown commands answer `unsupported:`, never shell out.
    Cybsh,
    /// Always the device shell (`sh -c`): cybsh commands are NOT translated.
    Device,
}

/// A saved agent configuration. No key material — see module docs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub provider_id: String,
    pub model: String,
    /// Override the preset endpoint (self-hosted gateways, proxies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url_override: Option<String>,
    /// Override the preset dialect (required when `provider_id` is custom).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialect_override: Option<LlmDialect>,
    /// Override the preset auth scheme (required when custom).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_scheme_override: Option<AuthScheme>,
    /// Header/query name when the scheme needs one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_name_override: Option<String>,
    /// Working root for tools. Empty = volume root.
    #[serde(default)]
    pub working_dir: String,
    #[serde(default)]
    pub agent_kind: AgentKind,
    /// Which shell `bash` runs (native transports only). Missing = auto.
    #[serde(default)]
    pub shell_mode: ShellMode,
    #[serde(default)]
    pub permission: PermissionRuleset,
    /// Auto-approve `ask` (never overrides `deny`).
    #[serde(default)]
    pub auto_approve: bool,
    /// Attached MCP servers by name (validated on save, connected per run).
    #[serde(default)]
    pub mcp_servers: std::collections::BTreeMap<String, McpServerConfig>,
    /// Attached MCP servers by name (validated on save, connected per run).
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    /// A key is stored server-side (never echoed back).
    #[serde(default)]
    pub has_key: bool,
    /// Failover route: config ids tried in order when this config's provider
    /// call fails terminally (dead key, throttling, transport failure, empty
    /// credits). Same-provider ids act as extra accounts/keys; other-provider
    /// ids act as provider fallbacks. Missing on old rows = no fallback.
    #[serde(default)]
    pub fallback_ids: Vec<String>,
    /// Embedding model for semantic memory (`{base}/embeddings`,
    /// OpenAI dialect). Defaults to `text-embedding-3-small`; Ollama serves
    /// any `-embed` model here. Unset = provider default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    /// Server-assigned; clients may omit them on create/update.
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

fn default_max_turns() -> u32 {
    25
}

/// One transcript message (provider-neutral; mapped per dialect on send).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,
}

/// A tool invocation the model requested.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Token/cost accounting for one turn or session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

/// A persisted agent session: config snapshot + full transcript.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: String,
    pub title: String,
    pub config_id: String,
    pub provider_id: String,
    pub model: String,
    #[serde(default)]
    pub agent_kind: AgentKind,
    #[serde(default)]
    pub working_dir: String,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub usage: TokenUsage,
    pub created_at: String,
    pub updated_at: String,
    /// JWT `user_id` that owns the transcript (`"local"` on the trusted
    /// Tauri path, `""` on rows predating ownership). Read/write/delete of a
    /// session requires the owner (or admin) — transcripts can hold secrets.
    #[serde(default)]
    pub owner_id: String,
}

/// Where a long-term memory came from.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MemoryOrigin {
    /// The agent explicitly stored it via `memory_remember`.
    #[default]
    Remember,
    /// Auto-stored session-compaction handoff.
    CompactHandoff,
    /// Imported envelope (sync restore / migration).
    Import,
}

/// One long-term memory: curated text + embedding vector, persisted in the
/// `agent_memories` redb table (same DB file as sessions — the `.cybermanju`
/// volume home, synced like any other row).
///
/// Vectors are namespaced per config AND per embedding dimension: recall only
/// compares same-dims vectors, so swapping the embedding model never corrupts
/// ranking — it just starts a fresh partition. A memory without a vector
/// (embedding failed or dialect has no embeddings API) is still recalled via
/// the keyword fallback, never silently dropped.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentMemory {
    pub id: String,
    pub config_id: String,
    /// JWT `user_id` that owns the row (`""` on rows predating ownership).
    /// Reads behave like sessions (P0-10 posture): the owner or an admin;
    /// legacy unowned rows stay readable so old memories never vanish.
    #[serde(default)]
    pub owner_id: String,
    pub text: String,
    /// Embedding vector; empty when never embedded (keyword-only recall).
    #[serde(default)]
    pub embedding: Vec<f32>,
    /// `embedding.len()` at store time — the comparability gate.
    #[serde(default)]
    pub dims: u32,
    #[serde(default)]
    pub origin: MemoryOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub uses: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// One ranked recall hit. Vectors never leave the server in list views —
/// only recall serves text + score; export carries vectors for sync restore.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryHit {
    pub id: String,
    pub text: String,
    pub score: f32,
    pub origin: MemoryOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub updated_at: String,
}

/// One MCP (Model Context Protocol) server attached to a config.
///
/// `stdio` spawns a local command (admin-gated — it executes processes);
/// `http` speaks Streamable HTTP (usable anywhere, CORS permitting).
/// Tools surface namespaced as `mcp__<server>__<tool>` so permission rules
/// match them like any other tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    /// `"stdio"` or `"http"`.
    pub transport: String,
    /// Command for stdio (e.g. `"npx"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    /// Endpoint for Streamable HTTP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// Disabled servers are skipped at connect time, never deleted silently.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl McpServerConfig {
    /// Validate without connecting (empty command / bad URL fail here).
    pub fn validate(&self) -> Result<(), String> {
        match self.transport.as_str() {
            "stdio" => {
                let cmd = self.command.as_deref().unwrap_or("").trim();
                if cmd.is_empty() {
                    return Err("invalid: stdio MCP servers need a command".to_string());
                }
                if cmd.contains('/') || cmd.contains('\\') {
                    return Err(
                        "invalid: stdio command must be a bare binary name on PATH".to_string()
                    );
                }
                Ok(())
            }
            "http" => {
                let url = self.url.as_deref().unwrap_or("");
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err("invalid: http MCP servers need an http(s) url".to_string());
                }
                Ok(())
            }
            other => Err(format!("invalid: unknown MCP transport '{other}'")),
        }
    }
}

/// Default web-search MCP server name (Exa, keyless Streamable HTTP).
pub const DEFAULT_EXA_MCP_NAME: &str = "exa";

/// Default web-search MCP endpoint (Exa, no API key required for the
/// `web_search_exa` / `web_fetch_exa` tools; rate-limited free tier).
pub const DEFAULT_EXA_MCP_URL: &str = "https://mcp.exa.ai/mcp";

/// The default Exa MCP server config attached to every agent config.
pub fn default_exa_mcp_server() -> McpServerConfig {
    McpServerConfig {
        transport: "http".to_string(),
        command: None,
        args: Vec::new(),
        env: std::collections::BTreeMap::new(),
        url: Some(DEFAULT_EXA_MCP_URL.to_string()),
        headers: Vec::new(),
        enabled: true,
    }
}

/// Insert the default Exa MCP server when the map has no `exa` entry.
/// Existing user entries (including a disabled `exa`) are never touched.
pub fn ensure_default_mcp_servers(
    servers: &mut std::collections::BTreeMap<String, McpServerConfig>,
) {
    if !servers.contains_key(DEFAULT_EXA_MCP_NAME) {
        servers.insert(DEFAULT_EXA_MCP_NAME.to_string(), default_exa_mcp_server());
    }
}

/// Default-allow the safe network-fetch commands and the default Exa
/// search tools so a fresh config can search the web without an approval
/// round-trip. Read-only research tools (`self_research`, `repo_analyze`)
/// are allowed too — they never mutate. Only fills gaps: explicit user
/// rules are never overwritten.
pub fn ensure_default_agent_permissions(rules: &mut PermissionRuleset) {
    for tool in [
        "mcp__exa__web_search_exa",
        "mcp__exa__web_fetch_exa",
        "self_research",
        "repo_analyze",
        // UI tools are read-only from the volume's point of view — they
        // open a panel or show a toast, never mutate state.
        "ui_open_panel",
        "ui_notify",
        // Vault metadata is safe (no values); `secret_get` (reveal) stays
        // on the ruleset default = ask — the approval card shows the title.
        "secret_list",
    ] {
        if !rules.rules.contains_key(tool) {
            rules.rules.insert(
                tool.to_string(),
                PermissionRule::Simple(PermissionAction::Allow),
            );
        }
    }
    match rules.rules.get_mut("bash") {
        Some(PermissionRule::Granular(pairs)) => {
            for pat in ["curl *", "wget *"] {
                if !pairs.iter().any(|(p, _)| p == pat) {
                    pairs.push((pat.to_string(), PermissionAction::Allow));
                }
            }
        }
        None => {
            rules.rules.insert(
                "bash".to_string(),
                PermissionRule::Granular(vec![
                    ("*".to_string(), PermissionAction::Ask),
                    ("curl *".to_string(), PermissionAction::Allow),
                    ("wget *".to_string(), PermissionAction::Allow),
                ]),
            );
        }
        // An explicit simple `bash` rule is the user's voice: keep it.
        Some(PermissionRule::Simple(_)) => {}
    }
}
