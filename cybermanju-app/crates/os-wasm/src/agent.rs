// CyberManju OS — WASM agent bridge (Pages transport).
//
// The browser cannot spawn the native worker loop, so this module exposes
// two primitives and the TypeScript `useAgent` composable drives the loop:
// `agent_catalog()` lists provider presets (pure, shared with native) and
// `agent_prompt(req)` performs ONE provider turn over browser `fetch`.
// Tool execution + transcript live in TS against the volume dispatcher.
// Non-2xx answers are classified into the house prefixes (`auth:` …),
// never thrown as opaque failures.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::{Request, RequestInit, Response};

/// Provider presets as JSON (endpoints, families, default models, dialects).
/// No key material — presets never held any.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn agent_catalog() -> String {
    serde_json::to_string(&cybermanju_agent::providers::all_presets())
        .unwrap_or_else(|_| "[]".to_string())
}

/// One provider turn: POST the prebuilt body, parse the reply.
///
/// `req_json`: `{url, dialect: "openai"|"anthropic", model, headers:
/// [[name, value]], system, messages: ChatMessage[], tools: bool,
/// extra_tools?: [{name, description, input_schema}]}`.
/// `extra_tools` carries browser-discovered MCP defs (canonical Anthropic
/// shape); they are merged per-dialect like the native `merge_mcp_tools`.
/// Returns `{"ok":true,"turn":{content,tool_calls,usage,finish}}` or
/// `{"ok":false,"error":"prefix: detail"}`. CORS rejections surface as
/// `network:` with a hint (Anthropic direct, for example, blocks browsers —
/// use the dashboard proxy or a CORS-allowing gateway).
/// Browser-only: the native transports drive the loop server-side instead.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn agent_prompt(req_json: &str) -> String {
    match agent_prompt_inner(req_json).await {
        Ok(turn_json) => format!(r#"{{"ok":true,"turn":{turn_json}}}"#),
        Err(error) => serde_json::json!({ "ok": false, "error": error }).to_string(),
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(serde::Deserialize)]
struct PromptRequest {
    url: String,
    #[serde(default)]
    dialect: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    system: String,
    #[serde(default)]
    messages: Vec<cybermanju_types::agent::ChatMessage>,
    #[serde(default)]
    tools: bool,
    #[serde(default)]
    extra_tools: Vec<serde_json::Value>,
}

#[cfg(target_arch = "wasm32")]
async fn agent_prompt_inner(req_json: &str) -> Result<String, String> {
    let req: PromptRequest =
        serde_json::from_str(req_json).map_err(|e| format!("invalid: request is not JSON: {e}"))?;
    if req.url.trim().is_empty() {
        return Err("invalid: url is required".to_string());
    }
    let openai = req.dialect != "anthropic";
    let mut body = if openai {
        cybermanju_agent::protocol::openai_request(
            &req.model,
            &req.system,
            &req.messages,
            req.tools,
        )
    } else {
        cybermanju_agent::protocol::anthropic_request(
            &req.model,
            &req.system,
            &req.messages,
            req.tools,
        )
    };
    // Merge browser-discovered MCP defs (canonical shape) per dialect.
    if req.tools && !req.extra_tools.is_empty() {
        if let Some(tools) = body.get_mut("tools").and_then(|t| t.as_array_mut()) {
            for def in &req.extra_tools {
                if openai {
                    tools.push(cybermanju_agent::protocol::as_openai_tool(def));
                } else {
                    tools.push(def.clone());
                }
            }
        }
    }
    let body_str =
        serde_json::to_string(&body).map_err(|e| format!("invalid: cannot encode body: {e}"))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&JsValue::from_str(&body_str));
    let request = Request::new_with_str_and_init(&req.url, &opts)
        .map_err(|_| "invalid: malformed provider url".to_string())?;
    for (name, value) in &req.headers {
        request
            .headers()
            .set(name, value)
            .map_err(|_| format!("invalid: bad header {name}"))?;
    }
    if request
        .headers()
        .get("Content-Type")
        .unwrap_or_default()
        .is_none()
    {
        let _ = request.headers().set("Content-Type", "application/json");
    }

    let window =
        web_sys::window().ok_or_else(|| "unsupported: no window (not a browser)".to_string())?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| {
            "network: fetch failed (CORS blocks some providers in browsers — try OpenRouter, Ollama via the dashboard proxy, or a CORS-allowing gateway)".to_string()
        })?;
    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| "network: provider reply was not an HTTP response".to_string())?;
    let status = resp.status();
    let text = JsFuture::from(
        resp.text()
            .map_err(|_| "network: could not read provider reply".to_string())?,
    )
    .await
    .map_err(|_| "network: could not read provider reply".to_string())?;
    let text = text.as_string().unwrap_or_default();
    let value: serde_json::Value =
        serde_json::from_str(&text).unwrap_or(serde_json::json!({ "raw": text }));
    if !(200..300).contains(&status) {
        // Same extraction as the native transport (object message,
        // string-form error, code) so a 400 names the real complaint.
        let msg = cybermanju_agent::protocol::provider_error_message(&value, status);
        return Err(cybermanju_agent::protocol::classify_provider_error(
            Some(status),
            &msg,
            "",
        ));
    }
    let turn = if openai {
        cybermanju_agent::protocol::openai_parse(&value)
    } else {
        cybermanju_agent::protocol::anthropic_parse(&value)
    }?;
    serde_json::to_string(&serde_json::json!({
        "content": turn.content,
        "tool_calls": turn.tool_calls,
        "usage": turn.usage,
        "finish": turn.finish,
    }))
    .map_err(|e| format!("network: cannot encode turn: {e}"))
}

#[cfg(test)]
mod tests {

    #[test]
    fn catalog_serializes_ten_presets() {
        let json = serde_json::to_string(&cybermanju_agent::providers::all_presets())
            .unwrap_or_else(|_| "[]".to_string());
        let presets: Vec<serde_json::Value> = serde_json::from_str(&json).expect("json");
        assert_eq!(presets.len(), 10);
        assert!(presets.iter().any(|p| p["id"] == "ollama"));
    }
}
