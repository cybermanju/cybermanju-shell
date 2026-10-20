// CyberManju OS — Mcp (crates/agent/src/mcp.rs).
// MCP (Model Context Protocol) wire shapes — pure JSON in/out.
//
// Covers what the runtime needs: JSON-RPC envelopes, the initialize
// handshake, `tools/list` normalization, `tools/call` result rendering, and
// SSE data-line extraction for Streamable HTTP. Transports (stdio child
// processes, HTTP POST) live in the caller (`agent_api`), which also owns
// timeouts and error prefixes. Compiles everywhere, including wasm.

/// Separator-safe tool namespace: `mcp__<server>__<tool>`.
pub fn tool_name(server: &str, tool: &str) -> String {
    format!("mcp__{server}__{tool}")
}

/// Split a namespaced tool back into `(server, tool)`.
pub fn split_tool_name(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    let (server, tool) = rest.split_once("__")?;
    if server.is_empty() || tool.is_empty() || server.contains("__") || tool.contains("__") {
        return None;
    }
    Some((server, tool))
}

/// Server names are matched into tool ids and shell-adjacent logs — keep
/// them to `[A-Za-z0-9_-]`.
pub fn valid_server_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// One discovered tool, normalized to the Anthropic input_schema shape (the
/// OpenAI wrapper is applied per dialect at send time).
#[derive(Debug, Clone, PartialEq)]
pub struct McpToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A JSON-RPC 2.0 request envelope.
pub fn request(id: u64, method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

/// A JSON-RPC 2.0 notification (no id, no reply expected).
pub fn notification(method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    })
}

/// MCP `initialize` params. We speak `2024-11-05`: old enough that every
/// server accepts it, new enough for tools and prompts.
pub fn initialize_params(client_name: &str) -> serde_json::Value {
    serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": { "tools": {} },
        "clientInfo": { "name": client_name, "version": "0.1.1" },
    })
}

/// Normalize a `tools/list` result into definitions. Malformed entries are
/// skipped (a broken sibling must not hide healthy tools); an empty page
/// yields an empty vec, never an error.
pub fn parse_tools_list(result: &serde_json::Value) -> Vec<McpToolDef> {
    let tools = result
        .get("tools")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for tool in tools {
        let name = tool.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if name.is_empty() || name.len() > 128 {
            continue;
        }
        out.push(McpToolDef {
            name: name.to_string(),
            description: tool
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .chars()
                .take(2000)
                .collect(),
            input_schema: tool
                .get("inputSchema")
                .cloned()
                .unwrap_or(serde_json::json!({
                    "type": "object",
                    "properties": {},
                })),
        });
        if out.len() >= 128 {
            break;
        }
    }
    out
}

/// Render a `tools/call` result into model-facing text. `isError` results
/// become `error:` outputs (the model sees them and recovers); oversized
/// payloads truncate with a marker.
pub fn render_call_result(result: &serde_json::Value) -> String {
    const CAP: usize = 32_768;
    let mut text = String::new();
    if let Some(contents) = result.get("content").and_then(|c| c.as_array()) {
        for block in contents {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(s) = block.get("text").and_then(|t| t.as_str()) {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(s);
                    }
                }
                Some("image") | Some("audio") => {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&format!(
                        "[{} content omitted — describe it in text instead]",
                        block
                            .get("type")
                            .and_then(|t| t.as_str())
                            .unwrap_or("media")
                    ));
                }
                _ => {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&block.to_string());
                }
            }
            if text.len() >= CAP {
                break;
            }
        }
    } else if !result.is_null() {
        text.push_str(&result.to_string());
    }
    if text.is_empty() {
        text.push_str("(empty result)");
    }
    if text.len() > CAP {
        text.truncate(CAP);
        text.push_str("\n… truncated at 32 KiB");
    }
    let is_error = result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if is_error {
        format!("error: MCP tool reported failure:\n{text}")
    } else {
        text
    }
}

/// Extract JSON payloads from an SSE/Streamable-HTTP body: lines starting
/// with `data: `, skipping comments and `[DONE]`. Unparseable lines are
/// dropped — a heartbeat must never kill a run.
pub fn parse_sse_data_lines(body: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.strip_prefix("data:").map(str::trim).unwrap_or("");
        if line.is_empty() || line == "[DONE]" {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            out.push(value);
        }
    }
    out
}

/// Pull the JSON-RPC `result` (or format the `error`) out of a response
/// object. Missing ids are tolerated — some servers echo sloppily.
pub fn unwrap_response(response: &serde_json::Value) -> Result<serde_json::Value, String> {
    if let Some(error) = response.get("error") {
        let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
        let message = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown MCP error");
        return Err(format!("error: MCP server error {code}: {message}"));
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| "integrity: MCP response had neither result nor error".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_namespacing_round_trips() {
        assert_eq!(tool_name("fs", "read"), "mcp__fs__read");
        assert_eq!(split_tool_name("mcp__fs__read"), Some(("fs", "read")));
        assert_eq!(split_tool_name("read"), None);
        assert_eq!(split_tool_name("mcp____x"), None);
        assert_eq!(split_tool_name("mcp__a__b__c"), None);
        assert!(valid_server_name("my-srv_1"));
        assert!(!valid_server_name("has space"));
        assert!(!valid_server_name("has/slash"));
        assert!(!valid_server_name(""));
    }

    #[test]
    fn envelopes_carry_jsonrpc_shape() {
        let req = request(7, "tools/list", serde_json::json!({}));
        assert_eq!(req["jsonrpc"], "2.0");
        assert_eq!(req["id"], 7);
        let note = notification("notifications/initialized", serde_json::json!({}));
        assert!(note.get("id").is_none());
        let init = initialize_params("cybermanju");
        assert_eq!(init["protocolVersion"], "2024-11-05");
    }

    #[test]
    fn tools_list_skips_malformed_entries() {
        let result = serde_json::json!({
            "tools": [
                { "name": "good", "description": "d",
                  "inputSchema": { "type": "object" } },
                { "description": "no name" },
                { "name": "", "description": "empty" },
            ],
        });
        let tools = parse_tools_list(&result);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "good");
        assert!(parse_tools_list(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn call_results_render_text_and_flag_errors() {
        let ok = serde_json::json!({
            "content": [{ "type": "text", "text": "hello" }],
            "isError": false,
        });
        assert_eq!(render_call_result(&ok), "hello");
        let err = serde_json::json!({
            "content": [{ "type": "text", "text": "boom" }],
            "isError": true,
        });
        assert!(render_call_result(&err).starts_with("error:"));
        let media = serde_json::json!({
            "content": [{ "type": "image", "data": "AAA", "mimeType": "image/png" }],
        });
        assert!(render_call_result(&media).contains("omitted"));
    }

    #[test]
    fn sse_parsing_ignores_heartbeats_and_done() {
        let body = ": ping\n\ndata: {\"a\":1}\n\ndata: [DONE]\n\ndata: not json\n";
        let out = parse_sse_data_lines(body);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["a"], 1);
    }

    #[test]
    fn unwrap_surfaces_server_errors() {
        let err = serde_json::json!({ "error": { "code": -32601, "message": "nope" } });
        assert!(unwrap_response(&err).expect_err("err").contains("-32601"));
        let ok = serde_json::json!({ "result": { "tools": [] } });
        assert!(unwrap_response(&ok).expect("ok")["tools"]
            .as_array()
            .is_some());
        assert!(unwrap_response(&serde_json::json!({})).is_err());
    }
}
