// CyberManju OS — OAuth authorization-code + PKCE routes
//
// <<< AGENT-3 OAUTH: `GET /api/sync/oauth/{provider}/start` and
// `GET /api/sync/oauth/{provider}/callback`. The flow itself (URL building,
// code exchange, refresh, persistence) lives in
// `cybermanju_sync::oauth`. >>>

use crate::security::{self, PendingOAuth};
use cybermanju_db::Database;
use cybermanju_sync::oauth as flows;
use serde::Serialize;

/// How long a `state` (and its PKCE verifier) stays valid.
const STATE_TTL_SECS: u64 = 600;

/// Response body of `GET /api/sync/oauth/{provider}/start`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthStart {
    pub authorize_url: String,
    pub state: String,
    pub expires_in: u64,
}

/// Whether `provider` is one of the supported slugs.
pub fn is_supported_provider(provider: &str) -> bool {
    flows::provider_endpoints(provider).is_ok()
}

/// The redirect URI the client must register with the provider.
///
/// Override with `CYBERMANJU_OAUTH_REDIRECT_URI` (useful when the app sits
/// behind a reverse proxy). Providers require an **exact** match, so the
/// value here is also the value in the provider console.
///
/// The default is the loopback callback served by this process:
/// `http://127.0.0.1:{port}/api/sync/oauth/{provider}/callback`.
pub fn redirect_uri(port: u16, provider: &str) -> String {
    if let Ok(uri) = std::env::var("CYBERMANJU_OAUTH_REDIRECT_URI") {
        let uri = uri.trim();
        if !uri.is_empty() {
            return uri.to_string();
        }
    }
    format!(
        "http://127.0.0.1:{}/api/sync/oauth/{}/callback",
        port, provider
    )
}

/// Begin an authorization-code flow: mint `state` + PKCE verifier and hand
/// back the URL the client must open in a browser.
pub fn start(
    auth: &security::AuthState,
    port: u16,
    provider: &str,
    config_id: &str,
) -> Result<OAuthStart, String> {
    // Fails fast (400) when the operator has not configured this provider.
    let (client_id, _client_secret) = flows::client_credentials(provider)?;

    let pkce = flows::generate_pkce();
    let state = flows::random_url_safe(32);
    let redirect = redirect_uri(port, provider);
    let authorize_url =
        flows::authorize_url(provider, &client_id, &redirect, &state, &pkce.challenge)?;

    auth.put_oauth_pending(
        &state,
        PendingOAuth {
            provider: provider.to_string(),
            config_id: config_id.to_string(),
            code_verifier: pkce.verifier,
            redirect_uri: redirect,
            created_at: security::now_secs(),
        },
    );

    Ok(OAuthStart {
        authorize_url,
        state,
        expires_in: STATE_TTL_SECS,
    })
}

/// Redeem the callback, persist the credentials and return an HTML page the
/// browser can display (the user then returns to the app).
pub fn callback(
    db: &Database,
    auth: &security::AuthState,
    provider: &str,
    code: &str,
    state: &str,
) -> Result<String, String> {
    if code.trim().is_empty() {
        return Err("The provider did not return an authorization code".to_string());
    }

    // One-shot: consuming the state makes replay impossible.
    let pending = auth
        .take_oauth_pending(state)
        .ok_or_else(|| "Unknown or expired OAuth state".to_string())?;

    if pending.provider != provider {
        return Err("OAuth state does not match the provider".to_string());
    }
    if security::now_secs().saturating_sub(pending.created_at) > STATE_TTL_SECS {
        return Err("OAuth state expired".to_string());
    }

    let (client_id, client_secret) = flows::client_credentials(provider)?;
    let credentials = flows::exchange_code(
        provider,
        &client_id,
        client_secret.as_deref(),
        code,
        &pending.redirect_uri,
        &pending.code_verifier,
    )?;
    flows::save_credentials(&pending.config_id, &credentials)?;

    let _ = db.log_audit(
        "oauth_connect",
        "sync_config",
        &pending.config_id,
        None,
        Some(serde_json::json!({ "provider": provider })),
    );

    Ok(page(
        "Account connected",
        &format!(
            "The <strong>{}</strong> account is connected. You can close this window.",
            escape(provider)
        ),
    ))
}

/// Minimal HTML shell used for the redirect landing page.
fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{}</title>\n\
         </head>\n\
         <body>\n\
         <h1>{}</h1>\n\
         <p>{}</p>\n\
         <p><a href=\"/\">Back to the app</a></p>\n\
         </body>\n\
         </html>\n",
        escape(title),
        escape(title),
        body
    )
}

/// Escape the few characters that matter for a text node / attribute.
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// HTML body for a failed callback (rendered with HTTP 400).
pub fn error_page(message: &str) -> String {
    page("Connection failed", &escape(message))
}
