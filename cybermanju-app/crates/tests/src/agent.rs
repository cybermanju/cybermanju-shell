// CyberManju OS — Agent (crates/tests/src/agent.rs).
// Native agent REST contract: providers, keyless configs, sessions,
// detached jobs with approvals, and the `cybsh ai` intercept.

use crate::web::{
    bearer, body_of, bootstrap_session, call, mint, mk_dashboard, now_secs, status_of,
};

fn authed() -> (
    tempfile::TempDir,
    std::sync::Arc<cybermanju_web::WebDashboard>,
    String,
) {
    let (dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "agent", "correct horse battery");
    let auth = bearer(&token);
    (dir, d, auth)
}

fn make_config(d: &cybermanju_web::WebDashboard, auth: &str) -> String {
    let body = r#"{"config":{
        "id":"", "name":"Test", "providerId":"ollama", "model":"llama3.1:8b",
        "workingDir":"", "agentKind":"build",
        "permission":{"default":"ask","rules":{}}, "autoApprove":false, "maxTurns":5
    }}"#;
    let resp = call(d, "POST", "/api/agent/configs", body, Some(auth));
    assert_eq!(status_of(&resp), 200, "create config: {resp}");
    let value: serde_json::Value = serde_json::from_str(body_of(&resp)).expect("json");
    assert_eq!(value["hasKey"], false);
    assert!(
        value.get("apiKey").is_none(),
        "key material leaked: {value}"
    );
    value["id"].as_str().expect("config id").to_string()
}

#[test]
fn agent_routes_are_auth_gated() {
    let (_dir, d) = mk_dashboard(3456);
    for (method, path, body) in [
        ("GET", "/api/agent/providers", ""),
        ("GET", "/api/agent/configs", ""),
        ("GET", "/api/agent/sessions", ""),
        ("GET", "/api/agent/jobs", ""),
        (
            "POST",
            "/api/agent/prompt",
            r#"{"configId":"x","prompt":"hi"}"#,
        ),
        ("POST", "/api/os/exec", r#"{"line":"ai ask hi"}"#),
    ] {
        let resp = call(&d, method, path, body, None);
        assert_eq!(status_of(&resp), 401, "{method} {path}: {resp}");
    }
}

#[test]
fn providers_list_without_secrets() {
    let (_dir, d, auth) = authed();
    let resp = call(&d, "GET", "/api/agent/providers", "", Some(&auth));
    assert_eq!(status_of(&resp), 200, "{resp}");
    let presets: Vec<serde_json::Value> = serde_json::from_str(body_of(&resp)).expect("json");
    assert_eq!(presets.len(), 10, "{presets:?}");
    assert!(presets.iter().any(|p| p["id"] == "ollama"));
    let body = body_of(&resp);
    assert!(!body.contains("sk-"), "no key material in catalog");
}

#[test]
fn config_keys_never_serialize_back() {
    let (_dir, d, auth) = authed();
    let id = make_config(&d, &auth);

    let resp = call(
        &d,
        "PUT",
        &format!("/api/agent/configs/{id}/key"),
        r#"{"apiKey":"sk-test-secret"}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 200, "{resp}");

    for path in [
        format!("/api/agent/configs/{id}"),
        "/api/agent/configs".to_string(),
    ] {
        let resp = call(&d, "GET", &path, "", Some(&auth));
        assert_eq!(status_of(&resp), 200, "{resp}");
        let body = body_of(&resp);
        assert!(!body.contains("sk-test-secret"), "key leaked in {path}");
    }
    let resp = call(
        &d,
        "GET",
        &format!("/api/agent/configs/{id}"),
        "",
        Some(&auth),
    );
    let value: serde_json::Value = serde_json::from_str(body_of(&resp)).expect("json");
    assert_eq!(value["hasKey"], true);

    // Empty keys are rejected, unknown configs 404.
    let resp = call(
        &d,
        "PUT",
        &format!("/api/agent/configs/{id}/key"),
        r#"{"apiKey":""}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 400, "{resp}");
    let resp = call(&d, "GET", "/api/agent/configs/nope", "", Some(&auth));
    assert_eq!(status_of(&resp), 404, "{resp}");

    // Custom providers require an endpoint.
    let resp = call(
        &d,
        "POST",
        "/api/agent/configs",
        r#"{"config":{"id":"","name":"C","providerId":"custom","model":"m",
            "workingDir":"","agentKind":"build","permission":{"default":"ask","rules":{}},
            "autoApprove":false,"maxTurns":5}}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 400, "{resp}");
}

#[test]
fn prompt_validates_before_spawning() {
    let (_dir, d, auth) = authed();
    // Unknown config → 404, no thread spawned.
    let resp = call(
        &d,
        "POST",
        "/api/agent/prompt",
        r#"{"configId":"missing","prompt":"hi"}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 404, "{resp}");

    // Known config without a key (non-keyless) → 400 auth, no thread.
    let keyed = call(
        &d,
        "POST",
        "/api/agent/configs",
        r#"{"config":{
            "id":"", "name":"Keyed", "providerId":"openai", "model":"gpt-4o-mini",
            "workingDir":"", "agentKind":"build",
            "permission":{"default":"ask","rules":{}}, "autoApprove":false, "maxTurns":5
        }}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&keyed), 200, "{keyed}");
    let keyed_id: String = serde_json::from_str::<serde_json::Value>(body_of(&keyed))
        .expect("json")["id"]
        .as_str()
        .expect("id")
        .to_string();
    let resp = call(
        &d,
        "POST",
        "/api/agent/prompt",
        &format!(r#"{{"configId":"{keyed_id}","prompt":"hi"}}"#),
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 400, "{resp}");
    assert!(body_of(&resp).contains("auth:"), "{resp}");

    // Empty prompt → 400.
    let resp = call(
        &d,
        "POST",
        "/api/agent/prompt",
        &format!(r#"{{"configId":"{keyed_id}","prompt":""}}"#),
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 400, "{resp}");

    // Unknown jobs 404 on every job route.
    for (method, path, body) in [
        ("GET", "/api/agent/jobs/nope", ""),
        ("POST", "/api/agent/jobs/nope/abort", ""),
        (
            "POST",
            "/api/agent/jobs/nope/approve",
            r#"{"approved":true}"#,
        ),
    ] {
        let resp = call(&d, method, path, body, Some(&auth));
        assert_eq!(status_of(&resp), 404, "{method} {path}: {resp}");
    }
}

#[test]
fn sessions_crud_and_import_rekey() {
    let (_dir, d, auth) = authed();
    let id = make_config(&d, &auth);

    let resp = call(
        &d,
        "POST",
        "/api/agent/sessions",
        &format!(r#"{{"configId":"{id}","title":"T"}}"#),
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 200, "{resp}");
    let created: serde_json::Value = serde_json::from_str(body_of(&resp)).expect("json");
    let sid = created["id"].as_str().expect("session id").to_string();

    let resp = call(&d, "GET", "/api/agent/sessions", "", Some(&auth));
    assert_eq!(status_of(&resp), 200, "{resp}");

    // Import always re-keys — client ids are never trusted.
    let resp = call(
        &d,
        "POST",
        "/api/agent/sessions/import",
        &format!(
            r#"{{"session":{{"id":"{sid}","title":"T","configId":"{id}",
                "providerId":"ollama","model":"m","agentKind":"build","workingDir":"",
                "messages":[],"usage":{{"inputTokens":0,"outputTokens":0}},
                "createdAt":"","updatedAt":""}}}}"#
        ),
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 200, "{resp}");
    let imported: serde_json::Value = serde_json::from_str(body_of(&resp)).expect("json");
    assert_ne!(imported["id"].as_str(), Some(sid.as_str()));

    let resp = call(
        &d,
        "DELETE",
        &format!("/api/agent/sessions/{sid}"),
        "",
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 200, "{resp}");
}

#[test]
fn cybsh_ai_without_configs_answers_honestly() {
    let (_dir, d, auth) = authed();
    // No configs exist: the intercept answers ok:false, HTTP stays 200.
    let resp = call(
        &d,
        "POST",
        "/api/os/exec",
        r#"{"line":"ai ask hello"}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 200, "{resp}");
    let body: serde_json::Value = serde_json::from_str(body_of(&resp)).expect("json");
    assert_eq!(body["ok"], false);
    assert!(
        body["output"]
            .as_str()
            .unwrap_or_default()
            .contains("no agent configs"),
        "{body}"
    );
}

#[test]
fn code_parse_still_serves_alongside_agent_routes() {
    let (_dir, d, auth) = authed();
    let resp = call(
        &d,
        "POST",
        "/api/code/parse",
        r#"{"fileName":"a.rs","content":"fn a() {}"}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 200, "{resp}");
}

#[test]
fn mcp_management_is_admin_gated_and_validated() {
    let (_dir, d) = mk_dashboard(3456);
    let admin = bearer(&mint(
        &d,
        "admin",
        now_secs() + 3_600,
        "jti-agent-mcp-admin",
    ));
    let member = bearer(&mint(&d, "user", now_secs() + 3_600, "jti-agent-mcp-user"));

    // Any authenticated user may create a config; seed one as member.
    let body = r#"{"config":{
        "id":"","name":"MCP","providerId":"ollama","model":"llama3.1:8b",
        "workingDir":"","agentKind":"build","permission":{"default":"ask","rules":{}},
        "autoApprove":false,"maxTurns":5}}"#;
    let resp = call(&d, "POST", "/api/agent/configs", body, Some(&member));
    assert_eq!(status_of(&resp), 200, "{resp}");
    let id: String = serde_json::from_str::<serde_json::Value>(body_of(&resp)).expect("json")["id"]
        .as_str()
        .expect("id")
        .to_string();

    let attach = |auth: &str, payload: &str| {
        call(
            &d,
            "POST",
            &format!("/api/agent/configs/{id}/mcp"),
            payload,
            Some(auth),
        )
    };
    // Member attach → 403 (stdio spawns processes).
    let resp = attach(
        &member,
        r#"{"name":"fs","server":{"transport":"stdio","command":"npx","args":[]}}"#,
    );
    assert_eq!(status_of(&resp), 403, "{resp}");

    // Unknown transport → 400 even for admins.
    let resp = attach(&admin, r#"{"name":"fs","server":{"transport":"ssh"}}"#);
    assert_eq!(status_of(&resp), 400, "{resp}");

    // Slash in the name → 400 (names become tool ids).
    let resp = attach(
        &admin,
        r#"{"name":"a/b","server":{"transport":"stdio","command":"x","args":[]}}"#,
    );
    assert_eq!(status_of(&resp), 400, "{resp}");

    // Unknown config → 404.
    let resp = call(
        &d,
        "POST",
        "/api/agent/configs/nope/mcp",
        r#"{"name":"fs","server":{"transport":"stdio","command":"x","args":[]}}"#,
        Some(&admin),
    );
    assert_eq!(status_of(&resp), 404, "{resp}");

    // Detach of nothing → 404; detach path is admin-gated too.
    let resp = call(
        &d,
        "DELETE",
        &format!("/api/agent/configs/{id}/mcp/fs"),
        "",
        Some(&admin),
    );
    assert_eq!(status_of(&resp), 404, "{resp}");
    let resp = call(
        &d,
        "DELETE",
        &format!("/api/agent/configs/{id}/mcp/fs"),
        "",
        Some(&member),
    );
    assert_eq!(status_of(&resp), 403, "{resp}");

    // Tools discovery on unknown config → 404.
    let resp = call(
        &d,
        "GET",
        "/api/agent/configs/nope/mcp/tools",
        "",
        Some(&admin),
    );
    assert_eq!(status_of(&resp), 404, "{resp}");
}

#[test]
fn compact_validates_before_any_network() {
    let (_dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "agent-compact", "correct horse battery");
    let auth = bearer(&token);
    let id = make_config(&d, &auth);

    // Unknown session → 404 without touching the network.
    let resp = call(
        &d,
        "POST",
        "/api/agent/sessions/nope/compact",
        &format!(r#"{{"configId":"{id}"}}"#),
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 404, "{resp}");

    // Empty transcript → 400, no provider call.
    let resp = call(
        &d,
        "POST",
        "/api/agent/sessions",
        &format!(r#"{{"configId":"{id}","title":"Empty"}}"#),
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 200, "{resp}");
    let sid: String = serde_json::from_str::<serde_json::Value>(body_of(&resp)).expect("json")
        ["id"]
        .as_str()
        .expect("sid")
        .to_string();
    let resp = call(
        &d,
        "POST",
        &format!("/api/agent/sessions/{sid}/compact"),
        &format!(r#"{{"configId":"{id}"}}"#),
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 400, "{resp}");
    assert!(body_of(&resp).contains("no messages"), "{resp}");
}

#[test]
fn init_validates_config_without_spawning() {
    let (_dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "agent-init", "correct horse battery");
    let auth = bearer(&token);
    let resp = call(
        &d,
        "POST",
        "/api/agent/init",
        r#"{"configId":"missing"}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 404, "{resp}");
}

#[test]
fn mcp_dead_servers_fail_loudly_not_silently() {
    let (_dir, d) = mk_dashboard(3456);
    let admin = bearer(&mint(&d, "admin", now_secs() + 3_600, "jti-agent-mcp-live"));
    let member = bearer(&mint(
        &d,
        "user",
        now_secs() + 3_600,
        "jti-agent-mcp-live-user",
    ));

    let body = r#"{"config":{
        "id":"","name":"MCP2","providerId":"ollama","model":"llama3.1:8b",
        "workingDir":"","agentKind":"build","permission":{"default":"ask","rules":{}},
        "autoApprove":false,"maxTurns":5}}"#;
    let resp = call(&d, "POST", "/api/agent/configs", body, Some(&member));
    assert_eq!(status_of(&resp), 200, "{resp}");
    let id: String = serde_json::from_str::<serde_json::Value>(body_of(&resp)).expect("json")["id"]
        .as_str()
        .expect("id")
        .to_string();

    // Attach a server whose binary cannot exist — validation passes (bare
    // name), connection must fail loudly at discovery, not silently.
    let resp = call(
        &d,
        "POST",
        &format!("/api/agent/configs/{id}/mcp"),
        r#"{"name":"ghost","server":{"transport":"stdio","command":"definitely-not-a-binary-xyz","args":[]}}"#,
        Some(&admin),
    );
    assert_eq!(status_of(&resp), 200, "{resp}");

    let resp = call(
        &d,
        "GET",
        &format!("/api/agent/configs/{id}/mcp/tools"),
        "",
        Some(&admin),
    );
    assert_eq!(status_of(&resp), 400, "{resp}");
    assert!(
        body_of(&resp).contains("ghost"),
        "names the dead server: {resp}"
    );
}

#[test]
fn session_transcripts_are_owner_isolated() {
    use crate::web::{bearer, body_of, call, login_session, mk_dashboard, status_of};
    let (_dir, d) = mk_dashboard(3456);
    // The auth-gated test helper only mints one id per role, so provision
    // two ordinary users through an admin and log them both in.
    let admin = bearer(&crate::web::mint(
        &d,
        "admin",
        crate::web::now_secs() + 3_600,
        "jti-sess-owner-admin",
    ));
    for user in ["alice", "bob"] {
        let create = call(
            &d,
            "POST",
            "/api/users",
            &format!(r#"{{"username":"{user}","password":"correct horse battery","role":"user"}}"#),
            Some(admin.as_str()),
        );
        assert_eq!(status_of(&create), 201, "{create}");
    }
    let alice = bearer(&login_session(&d, "alice", "correct horse battery"));
    let bob = bearer(&login_session(&d, "bob", "correct horse battery"));

    // Alice creates a session on a shared (global) config.
    let admin_cfg = call(
        &d,
        "POST",
        "/api/agent/configs",
        r#"{"config":{"id":"","name":"Shared","providerId":"ollama","model":"llama3.1:8b",
            "workingDir":"","agentKind":"build","permission":{"default":"ask","rules":{}},
            "autoApprove":false,"maxTurns":5}}"#,
        Some(admin.as_str()),
    );
    assert_eq!(status_of(&admin_cfg), 200, "{admin_cfg}");
    let cfg: serde_json::Value = serde_json::from_str(body_of(&admin_cfg)).expect("json");
    let cid = cfg["id"].as_str().expect("config id");
    let created = call(
        &d,
        "POST",
        "/api/agent/sessions",
        &format!(r#"{{"configId":"{cid}","title":"Alice notes"}}"#),
        Some(alice.as_str()),
    );
    assert_eq!(status_of(&created), 200, "{created}");
    let session: serde_json::Value = serde_json::from_str(body_of(&created)).expect("json");
    let sid = session["id"].as_str().expect("session id").to_string();

    // Bob sees neither the row nor its contents, and cannot delete it.
    let list = call(&d, "GET", "/api/agent/sessions", "", Some(bob.as_str()));
    assert_eq!(status_of(&list), 200, "{list}");
    assert!(!body_of(&list).contains(&sid), "bob lists alice's session");
    let get = call(
        &d,
        "GET",
        &format!("/api/agent/sessions/{sid}"),
        "",
        Some(bob.as_str()),
    );
    assert_eq!(status_of(&get), 400, "{get}");
    assert!(body_of(&get).contains("another user"), "{get}");
    let delete = call(
        &d,
        "DELETE",
        &format!("/api/agent/sessions/{sid}"),
        "",
        Some(bob.as_str()),
    );
    assert_eq!(status_of(&delete), 400, "{delete}");

    // Alice still owns her transcript end to end.
    let get = call(
        &d,
        "GET",
        &format!("/api/agent/sessions/{sid}"),
        "",
        Some(alice.as_str()),
    );
    assert_eq!(status_of(&get), 200, "{get}");
    let delete = call(
        &d,
        "DELETE",
        &format!("/api/agent/sessions/{sid}"),
        "",
        Some(alice.as_str()),
    );
    assert_eq!(status_of(&delete), 200, "{delete}");
}
