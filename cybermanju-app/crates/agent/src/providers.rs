// CyberManju OS — Providers (crates/agent/src/providers.rs).
// Provider catalog: id, endpoint, model family, auth. Every field is a
// *default* — configs override endpoint/model, keys are stored sealed.
// OpenAI-dialect first: one wire shape serves OpenRouter, Ollama `/v1`,
// Gemini OpenAI-compat, Groq, Mistral, DeepSeek, xAI and Cerebras.

use cybermanju_types::agent::{AuthScheme, LlmDialect, ProviderPreset};

/// All built-in provider presets, stable order for pickers.
pub fn all_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            id: "anthropic".into(),
            label: "Anthropic".into(),
            family: "claude".into(),
            base_url: "https://api.anthropic.com".into(),
            default_model: "claude-sonnet-4-5".into(),
            dialect: LlmDialect::Anthropic,
            auth: AuthScheme::Header,
            auth_name: Some("x-api-key".into()),
            key_env: "ANTHROPIC_API_KEY".into(),
            keyless: false,
            extra_headers: vec![("anthropic-version".into(), "2023-06-01".into())],
        },
        ProviderPreset {
            id: "openai".into(),
            label: "OpenAI".into(),
            family: "gpt".into(),
            base_url: "https://api.openai.com/v1".into(),
            default_model: "gpt-5".into(),
            dialect: LlmDialect::OpenAi,
            auth: AuthScheme::Bearer,
            auth_name: None,
            key_env: "OPENAI_API_KEY".into(),
            keyless: false,
            extra_headers: vec![],
        },
        ProviderPreset {
            id: "openrouter".into(),
            label: "OpenRouter".into(),
            family: "openrouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            default_model: "anthropic/claude-sonnet-4-5".into(),
            dialect: LlmDialect::OpenAi,
            auth: AuthScheme::Bearer,
            auth_name: None,
            key_env: "OPENROUTER_API_KEY".into(),
            keyless: false,
            extra_headers: vec![
                (
                    "HTTP-Referer".into(),
                    "https://cybermanju.github.io/".into(),
                ),
                ("X-Title".into(), "CyberManju OS".into()),
            ],
        },
        ProviderPreset {
            id: "ollama".into(),
            label: "Ollama (local)".into(),
            family: "ollama".into(),
            base_url: "http://localhost:11434/v1".into(),
            default_model: "llama3.1:8b".into(),
            dialect: LlmDialect::OpenAi,
            auth: AuthScheme::None,
            auth_name: None,
            key_env: String::new(),
            keyless: true,
            extra_headers: vec![],
        },
        ProviderPreset {
            id: "google".into(),
            label: "Google Gemini".into(),
            family: "gemini".into(),
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai/".into(),
            default_model: "gemini-2.5-flash".into(),
            dialect: LlmDialect::OpenAi,
            auth: AuthScheme::Bearer,
            auth_name: None,
            key_env: "GEMINI_API_KEY".into(),
            keyless: false,
            extra_headers: vec![],
        },
        ProviderPreset {
            id: "groq".into(),
            label: "Groq".into(),
            family: "groq".into(),
            base_url: "https://api.groq.com/openai/v1".into(),
            default_model: "llama-3.3-70b-versatile".into(),
            dialect: LlmDialect::OpenAi,
            auth: AuthScheme::Bearer,
            auth_name: None,
            key_env: "GROQ_API_KEY".into(),
            keyless: false,
            extra_headers: vec![],
        },
        ProviderPreset {
            id: "mistral".into(),
            label: "Mistral".into(),
            family: "mistral".into(),
            base_url: "https://api.mistral.ai/v1".into(),
            default_model: "mistral-large-latest".into(),
            dialect: LlmDialect::OpenAi,
            auth: AuthScheme::Bearer,
            auth_name: None,
            key_env: "MISTRAL_API_KEY".into(),
            keyless: false,
            extra_headers: vec![],
        },
        ProviderPreset {
            id: "deepseek".into(),
            label: "DeepSeek".into(),
            family: "deepseek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            default_model: "deepseek-chat".into(),
            dialect: LlmDialect::OpenAi,
            auth: AuthScheme::Bearer,
            auth_name: None,
            key_env: "DEEPSEEK_API_KEY".into(),
            keyless: false,
            extra_headers: vec![],
        },
        ProviderPreset {
            id: "xai".into(),
            label: "xAI Grok".into(),
            family: "grok".into(),
            base_url: "https://api.x.ai/v1".into(),
            default_model: "grok-4".into(),
            dialect: LlmDialect::OpenAi,
            auth: AuthScheme::Bearer,
            auth_name: None,
            key_env: "XAI_API_KEY".into(),
            keyless: false,
            extra_headers: vec![],
        },
        ProviderPreset {
            id: "cerebras".into(),
            label: "Cerebras".into(),
            family: "cerebras".into(),
            base_url: "https://api.cerebras.ai/v1".into(),
            default_model: "llama-3.3-70b".into(),
            dialect: LlmDialect::OpenAi,
            auth: AuthScheme::Bearer,
            auth_name: None,
            key_env: "CEREBRAS_API_KEY".into(),
            keyless: false,
            extra_headers: vec![],
        },
    ]
}

/// Find a preset by id.
pub fn find_preset(id: &str) -> Option<ProviderPreset> {
    all_presets().into_iter().find(|p| p.id == id)
}

/// A fully-resolved endpoint: preset values merged with config overrides.
/// `provider_id == "custom"` skips the catalog (endpoint + dialect required).
#[derive(Debug, Clone)]
pub struct ResolvedEndpoint {
    pub base_url: String,
    pub dialect: cybermanju_types::agent::LlmDialect,
    pub auth: cybermanju_types::agent::AuthScheme,
    pub auth_name: Option<String>,
    pub extra_headers: Vec<(String, String)>,
    pub keyless: bool,
}

/// Merge a config with the catalog into a callable endpoint.
pub fn resolve(config: &cybermanju_types::agent::AgentConfig) -> Result<ResolvedEndpoint, String> {
    if config.provider_id == "custom" {
        let base_url = config
            .base_url_override
            .clone()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| "invalid: custom provider needs base_url_override".to_string())?;
        return Ok(ResolvedEndpoint {
            base_url,
            dialect: config
                .dialect_override
                .unwrap_or(cybermanju_types::agent::LlmDialect::OpenAi),
            auth: config
                .auth_scheme_override
                .unwrap_or(cybermanju_types::agent::AuthScheme::Bearer),
            auth_name: config.auth_name_override.clone(),
            extra_headers: Vec::new(),
            keyless: false,
        });
    }
    let preset = find_preset(&config.provider_id)
        .ok_or_else(|| format!("not_found: unknown provider '{}'", config.provider_id))?;
    Ok(ResolvedEndpoint {
        base_url: config
            .base_url_override
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(preset.base_url),
        dialect: config.dialect_override.unwrap_or(preset.dialect),
        auth: config.auth_scheme_override.unwrap_or(preset.auth),
        auth_name: config.auth_name_override.clone().or(preset.auth_name),
        extra_headers: preset.extra_headers,
        keyless: preset.keyless,
    })
}

/// Endpoint a chat call hits for this preset (+ optional override).
pub fn chat_url(base_url: &str, dialect: LlmDialect) -> String {
    let base = base_url.trim_end_matches('/');
    match dialect {
        LlmDialect::OpenAi => format!("{base}/chat/completions"),
        LlmDialect::Anthropic => format!("{base}/v1/messages"),
    }
}

/// Models-list endpoint, if the dialect has one (Anthropic: none — honest
/// `None`, the UI keeps manual entry for it).
pub fn models_url(base_url: &str, dialect: LlmDialect) -> Option<String> {
    match dialect {
        LlmDialect::OpenAi => Some(format!("{}/models", base_url.trim_end_matches('/'))),
        LlmDialect::Anthropic => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_ten_providers_with_sane_defaults() {
        let all = all_presets();
        assert_eq!(all.len(), 10);
        let ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        for want in [
            "anthropic",
            "openai",
            "openrouter",
            "ollama",
            "google",
            "groq",
            "mistral",
            "deepseek",
            "xai",
            "cerebras",
        ] {
            assert!(ids.contains(&want), "missing {want}");
        }
        let ollama = find_preset("ollama").expect("ollama");
        assert!(ollama.keyless);
        assert_eq!(ollama.auth, AuthScheme::None);
        assert!(find_preset("nope").is_none());
    }

    #[test]
    fn urls_derive_from_dialect() {
        assert_eq!(
            chat_url("https://api.openai.com/v1", LlmDialect::OpenAi),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_url("https://api.anthropic.com/", LlmDialect::Anthropic),
            "https://api.anthropic.com/v1/messages"
        );
        assert!(models_url("https://api.anthropic.com", LlmDialect::Anthropic).is_none());
        assert!(models_url("https://api.groq.com/openai/v1", LlmDialect::OpenAi).is_some());
    }
}
