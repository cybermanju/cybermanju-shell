// CyberManju OS — retry / backoff policy (AGENT-1)
//
// Two things live here:
//
//   1. The **error string contract** every backend returns: an error starts
//      with exactly one of `auth:`, `rate_limited:`, `not_found:`,
//      `unsupported:`, `too_large:`, `integrity:` or `network: `. AGENT-2
//      maps the prefix onto `SyncStatus`/retry policy and AGENT-5 renders
//      it, so the prefix is the stable API. The `pub const`s below are the
//      *bare tokens* — the `: ` separator is added by the message builders.
//   2. `with_retry` — bounded exponential backoff with jitter, honoring
//      `Retry-After` / `X-RateLimit-Reset` / JSON-body `retry_after`.
//
// Retryable classes: `rate_limited:` and `network:` (5xx/408/transport).
// Everything else is a decision that retrying cannot change.
//
// <<< AGENT-1 RETRY >>>

use std::time::Duration;

// ─── Error prefix contract ───────────────────────────────────────────

// The class tokens are *bare*: every message builder in the crate renders
// `format!("{}: …", NETWORK)` itself. Carrying the colon here as well made
// every constant-built message read `network:: …`.
pub const AUTH: &str = "auth";
pub const RATE_LIMITED: &str = "rate_limited";
pub const NOT_FOUND: &str = "not_found";
pub const UNSUPPORTED: &str = "unsupported";
pub const TOO_LARGE: &str = "too_large";
pub const INTEGRITY: &str = "integrity";
pub const NETWORK: &str = "network";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Auth,
    RateLimited,
    NotFound,
    Unsupported,
    TooLarge,
    Integrity,
    Network,
    Unclassified,
}

impl ErrorClass {
    /// The contract prefix this class renders as.
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Auth => AUTH,
            Self::RateLimited => RATE_LIMITED,
            Self::NotFound => NOT_FOUND,
            Self::Unsupported => UNSUPPORTED,
            Self::TooLarge => TOO_LARGE,
            Self::Integrity => INTEGRITY,
            Self::Network => NETWORK,
            Self::Unclassified => "",
        }
    }

    /// Whether `with_retry` may attempt this class again.
    pub fn retryable(self) -> bool {
        matches!(self, Self::RateLimited | Self::Network)
    }
}

/// Classify a returned error string by its prefix.
pub fn classify(err: &str) -> ErrorClass {
    if err.starts_with(AUTH) {
        ErrorClass::Auth
    } else if err.starts_with(RATE_LIMITED) {
        ErrorClass::RateLimited
    } else if err.starts_with(NOT_FOUND) {
        ErrorClass::NotFound
    } else if err.starts_with(UNSUPPORTED) {
        ErrorClass::Unsupported
    } else if err.starts_with(TOO_LARGE) {
        ErrorClass::TooLarge
    } else if err.starts_with(INTEGRITY) {
        ErrorClass::Integrity
    } else if err.starts_with(NETWORK) {
        ErrorClass::Network
    } else {
        ErrorClass::Unclassified
    }
}

/// Map an HTTP status onto the error contract.
///
/// 401/403 → `auth:`, 404/410 → `not_found:`, 413 → `too_large:`,
/// 429 → `rate_limited:`, remaining 4xx → `unsupported:` (the request as
/// built is refused — retrying cannot fix it), everything else (5xx, 408,
/// transport) → `network:`.
pub fn class_for_status(status: u16) -> ErrorClass {
    match status {
        401 | 403 => ErrorClass::Auth,
        404 | 410 => ErrorClass::NotFound,
        413 => ErrorClass::TooLarge,
        429 => ErrorClass::RateLimited,
        408 => ErrorClass::Network,
        400..=499 => ErrorClass::Unsupported,
        _ => ErrorClass::Network,
    }
}

/// `true` when the provider is asking us to slow down, even if it answered
/// with 403 (GitHub burns a 403 on an exhausted rate-limit window).
pub fn is_rate_limited(status: u16, headers: &reqwest::header::HeaderMap) -> bool {
    if status == 429 {
        return true;
    }
    if status != 403 {
        return false;
    }
    for name in ["x-ratelimit-remaining", "ratelimit-remaining"] {
        if let Some(value) = headers.get(name).and_then(|v| v.to_str().ok()) {
            if value.trim() == "0" {
                return true;
            }
        }
    }
    false
}

// ─── Error construction ──────────────────────────────────────────────

/// `<prefix>: <provider> <op> failed (<status>) [retry_after=Ns]: <detail>`
///
/// The `(status)` and `retry_after=` fragments are part of the contract —
/// AGENT-4's contract tests match on them.
pub fn http_error(
    prefix: &str,
    provider: &str,
    op: &str,
    status: u16,
    retry_after: Option<Duration>,
    detail: &str,
) -> String {
    let hint = match retry_after {
        Some(wait) => format!(" [retry_after={}s]", wait.as_secs()),
        // Some providers carry the hint inside the JSON body, not in a header.
        None => json_retry_after(detail)
            .map(|secs| format!(" [retry_after={}s]", secs))
            .unwrap_or_default(),
    };
    format!(
        "{}{} {} failed ({}){}: {}",
        prefix_head(prefix),
        provider,
        op,
        status,
        hint,
        truncate(detail.trim(), 400)
    )
}

/// Render a class prefix as the head of a message: `"auth: "`, or `""` when
/// the error is unclassified.
///
/// Accepts the token with or without its colon — callers pass both the bare
/// constants above and hand-written literals such as `"rate_limited:"` — and
/// never emits `auth:: …`.
pub fn prefix_head(prefix: &str) -> String {
    let token = prefix.trim_end_matches(':');
    if token.is_empty() {
        String::new()
    } else {
        format!("{}: ", token)
    }
}

/// Shorthand: `http_error` with the class derived from the status.
pub fn provider_error(
    provider: &str,
    op: &str,
    status: u16,
    retry_after: Option<Duration>,
    detail: &str,
) -> String {
    let prefix = class_for_status(status).prefix();
    http_error(prefix, provider, op, status, retry_after, detail)
}

/// Extract `"retry_after": <secs>` (some providers nest it under `parameters`).
fn json_retry_after(detail: &str) -> Option<u64> {
    let idx = detail.find("\"retry_after\"")?;
    let rest = &detail[idx + "\"retry_after\"".len()..];
    let rest = rest.trim_start().trim_start_matches(':').trim_start();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Pull the `retry_after=Ns` hint back out of a formatted error.
pub fn retry_after_hint(err: &str) -> Option<Duration> {
    let idx = err.find("retry_after=")?;
    let rest = &err[idx + "retry_after=".len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    Some(Duration::from_secs(digits.parse().ok()?))
}

/// `Retry-After` (seconds or HTTP-date), then `X-RateLimit-Reset` when the
/// remaining budget is exhausted.
pub fn retry_after_from_headers(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    if let Some(value) = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
    {
        let value = value.trim();
        if let Ok(secs) = value.parse::<u64>() {
            return Some(Duration::from_secs(secs));
        }
        if let Ok(when) = chrono::DateTime::parse_from_rfc2822(value) {
            let delta = when.with_timezone(&chrono::Utc) - chrono::Utc::now();
            return Some(Duration::from_secs(delta.num_seconds().max(0) as u64));
        }
    }

    let remaining_zero = headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim() == "0")
        .unwrap_or(false);
    if remaining_zero {
        if let Some(reset) = headers
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
        {
            let now = chrono::Utc::now().timestamp().max(0) as u64;
            return Some(Duration::from_secs(reset.saturating_sub(now)));
        }
    }
    None
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

// ─── Backoff ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Total attempts, including the first one.
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    /// Jitter spread as a fraction (0.25 = ±25%) so parallel workers do not
    /// retry in lockstep.
    pub jitter: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            jitter: 0.25,
        }
    }
}

impl RetryPolicy {
    /// Delay before the attempt that follows `failed_attempt` (1-based).
    pub fn delay(&self, failed_attempt: u32) -> Duration {
        let exponent = failed_attempt.saturating_sub(1).min(10);
        let factor = 2u32.saturating_pow(exponent);
        let raw = self.base_delay.saturating_mul(factor).min(self.max_delay);

        // Deterministic-per-instant jitter: no extra dependency, good enough
        // to decorrelate a rayon pool.
        let subsec = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let unit = (subsec % 1000) as f64 / 999.0; // 0.0 ..= 1.0
        let spread = 1.0 + self.jitter * (unit * 2.0 - 1.0);
        let millis = (raw.as_secs_f64() * 1000.0 * spread).max(0.0) as u64;
        Duration::from_millis(millis)
    }
}

/// Run `op`, retrying only `network:`/`rate_limited:` failures with backoff.
///
/// A provider-mandated wait longer than `policy.max_delay` is *not* slept on:
/// the error (still carrying its `retry_after=` hint) is surfaced so the
/// caller can reschedule instead of blocking a worker for an hour.
pub fn with_retry<T, F>(policy: &RetryPolicy, mut op: F) -> Result<T, String>
where
    F: FnMut() -> Result<T, String>,
{
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        let err = match op() {
            Ok(value) => return Ok(value),
            Err(err) => err,
        };
        if !classify(&err).retryable() || attempt >= policy.max_attempts {
            return Err(err);
        }
        let wait = match retry_after_hint(&err) {
            Some(hint) if hint > policy.max_delay => return Err(err),
            Some(hint) => hint,
            None => policy.delay(attempt),
        };
        log::warn!(
            "transient provider error, retry {}/{} in {:?}: {}",
            attempt,
            policy.max_attempts,
            wait,
            err
        );
        std::thread::sleep(wait);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_every_contract_prefix() {
        assert_eq!(classify("auth: nope"), ErrorClass::Auth);
        assert_eq!(classify("rate_limited: nope"), ErrorClass::RateLimited);
        assert_eq!(classify("not_found: nope"), ErrorClass::NotFound);
        assert_eq!(classify("unsupported: nope"), ErrorClass::Unsupported);
        assert_eq!(classify("too_large: nope"), ErrorClass::TooLarge);
        assert_eq!(classify("integrity: nope"), ErrorClass::Integrity);
        assert_eq!(classify("network: nope"), ErrorClass::Network);
        assert_eq!(classify("whatever"), ErrorClass::Unclassified);
    }

    #[test]
    fn status_classification_is_stable() {
        assert_eq!(class_for_status(401), ErrorClass::Auth);
        assert_eq!(class_for_status(404), ErrorClass::NotFound);
        assert_eq!(class_for_status(413), ErrorClass::TooLarge);
        assert_eq!(class_for_status(429), ErrorClass::RateLimited);
        assert_eq!(class_for_status(422), ErrorClass::Unsupported);
        assert_eq!(class_for_status(500), ErrorClass::Network);
        assert!(class_for_status(503).retryable());
        assert!(!class_for_status(422).retryable());
    }

    #[test]
    fn http_error_keeps_status_and_retry_hint_in_the_message() {
        let err = http_error(
            "rate_limited:",
            "GitLab",
            "list",
            429,
            Some(Duration::from_secs(7)),
            "{\"message\":\"slow down\"}",
        );
        assert!(err.starts_with("rate_limited: "), "{err}");
        assert!(err.contains("list failed (429)"), "{err}");
        assert!(err.contains("[retry_after=7s]"), "{err}");
        assert_eq!(retry_after_hint(&err), Some(Duration::from_secs(7)));
    }

    #[test]
    fn json_body_carries_the_retry_hint() {
        let detail = r#"{"ok":false,"error_code":429,"description":"Too Many Requests: retry after 12","parameters":{"retry_after":12}}"#;
        let err = http_error("rate_limited:", "GitHub", "upload", 429, None, detail);
        assert!(err.contains("[retry_after=12s]"), "{err}");
        assert_eq!(retry_after_hint(&err), Some(Duration::from_secs(12)));
    }

    #[test]
    fn retries_are_bounded_and_respect_max_attempts() {
        let policy = RetryPolicy {
            max_attempts: 3,
            ..RetryPolicy::default()
        };
        let mut calls = 0;
        let result: Result<(), String> = with_retry(&policy, || {
            calls += 1;
            Err("network: boom".to_string())
        });
        assert!(result.is_err());
        assert_eq!(calls, 3, "bounded attempts");
    }

    #[test]
    fn non_retryable_errors_return_immediately() {
        let policy = RetryPolicy::default();
        let mut calls = 0;
        let result: Result<(), String> = with_retry(&policy, || {
            calls += 1;
            Err("unsupported: nope".to_string())
        });
        assert!(result.is_err());
        assert_eq!(calls, 1, "unsupported must not be retried");
    }

    #[test]
    fn a_provider_wait_longer_than_the_cap_is_surfaced_not_slept() {
        let policy = RetryPolicy::default();
        let result: Result<(), String> = with_retry(&policy, || {
            Err("rate_limited: GitLab list failed (429) [retry_after=3600s]: slow".to_string())
        });
        let err = result.expect_err("must not sleep an hour");
        assert!(err.contains("retry_after=3600s"), "{err}");
    }

    #[test]
    fn backoff_stays_within_the_configured_window() {
        let policy = RetryPolicy::default();
        for attempt in 1..10 {
            let d = policy.delay(attempt);
            assert!(d <= policy.max_delay * 2, "{d:?} too long");
        }
    }
}
