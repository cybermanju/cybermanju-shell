// CyberManju OS — Stream (crates/agent/src/stream.rs).
// SSE wire helpers: pure event formatting + chunk parsing.
//
// No I/O, no threads, no `TcpStream` here on purpose: the hand-rolled
// servers (`crates/web`, `docker/server`) own timeouts, threads and the
// connection cap, while this module stays dependency-light and fully
// `cargo test`-covered. That split is what keeps blind-Rust CI exposure
// small — a formatting bug fails a unit test, never a socket.
//
// Contract:
//   * `format_event` sanitizes the event name and normalizes data lines so
//     one call can never emit a framing break (`\r\n` injection).
//   * `parse_event_stream` drops what it cannot understand (comments,
//     heartbeats, oversized frames, unknown fields) instead of erroring —
//     a heartbeat must never kill a run.
//   * `[DONE]` is data, not control: the producer sends it after a terminal
//     job state and the consumer treats it as end-of-stream.

/// Seconds between `: ping` heartbeats on an idle job stream.
pub const SSE_HEARTBEAT_SECS: u64 = 15;

/// Hard ceiling for one SSE connection (5 min). The client reconnects
/// and re-polls — a thread must never live as long as a job.
pub const SSE_MAX_STREAM_SECS: u64 = 300;

/// Cap for one event's data payload (64 KiB — mirrors `TOOL_OUTPUT_CAP`).
pub const SSE_DATA_CAP: usize = 65_536;

/// Single header/line cap while parsing a chunk.
pub const SSE_MAX_LINE_BYTES: usize = 65_536;

/// Bound on events decoded from one chunk (a malicious chunk cannot
/// force unbounded allocation).
pub const SSE_MAX_EVENTS_PER_CHUNK: usize = 1_024;

/// Marker appended when `format_event` truncates an oversized payload.
pub const SSE_TRUNCATION_MARKER: &str = "\n… truncated at 64 KiB";

/// One decoded SSE event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

/// True for the terminal sentinel the producer sends after a job reaches
/// `done` / `error` / `cancelled`.
pub fn is_done_data(data: &str) -> bool {
    data.trim() == "[DONE]"
}

/// Event names travel on one line (`event: <name>`). Only ASCII
/// alphanumerics plus `-` and `_` survive; anything else (including the
/// empty string) falls back to `"message"` so framing can never break.
pub fn sanitize_event_name(name: &str) -> &str {
    if name.is_empty() {
        return "message";
    }
    let ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        name
    } else {
        "message"
    }
}

/// Truncate `text` to at most `cap` bytes on a char boundary.
fn truncate_bytes(text: &str, cap: usize) -> &str {
    if text.len() <= cap {
        return text;
    }
    let mut end = cap;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Payload bytes per emitted `data:` line. The parser drops any line over
/// [`SSE_MAX_LINE_BYTES`], so `format_event` must never emit one — long
/// payload lines are wrapped (the parser re-joins `data:` lines with
/// `\n`, which is why the truncation test budgets a few extra bytes).
const DATA_LINE_BUDGET: usize = SSE_MAX_LINE_BYTES - 6 - 1; // "data: " + '\n'

/// Bytes held back when truncating: the truncation-marker `data:` line
/// plus the `\n` joins the parser re-inserts between wrapped lines.
/// Without this the marked event re-joins over [`SSE_DATA_CAP`] and the
/// parser drops the very event the marker was meant to save — and a body
/// of exactly [`SSE_DATA_CAP`] bytes would wrap into two lines that
/// re-join one byte over the cap and vanish without any marker at all.
const TRUNCATION_RESERVE: usize = 64;

/// Emit one `data:` line, wrapping payloads the parser would otherwise
/// drop outright (a truncated event must arrive marked, not vanish).
fn emit_data_line(out: &mut String, line: &str) {
    if line.len() + 6 < SSE_MAX_LINE_BYTES {
        out.push_str("data: ");
        out.push_str(line);
        out.push('\n');
        return;
    }
    let mut rest = line;
    while rest.len() + 6 >= SSE_MAX_LINE_BYTES {
        let mut end = DATA_LINE_BUDGET.min(rest.len());
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        out.push_str("data: ");
        out.push_str(&rest[..end]);
        out.push('\n');
        rest = &rest[end..];
    }
    out.push_str("data: ");
    out.push_str(rest);
    out.push('\n');
}

/// Format one SSE event (`event:` + one `data:` line per input line,
/// terminated by a blank line). `\r` is stripped so a `\r\n` in model
/// text cannot inject a framing break; oversized payloads truncate with
/// [`SSE_TRUNCATION_MARKER`].
pub fn format_event(event: &str, data: &str) -> String {
    let name = sanitize_event_name(event);
    let flat = data.replace('\r', "");
    let body_cap = SSE_DATA_CAP - TRUNCATION_RESERVE;
    let truncated = flat.len() > body_cap;
    let body = truncate_bytes(&flat, body_cap);
    let mut out = String::with_capacity(name.len() + body.len() + 32);
    out.push_str("event: ");
    out.push_str(name);
    out.push('\n');
    if body.is_empty() {
        out.push_str("data:\n");
    } else {
        for line in body.split('\n') {
            emit_data_line(&mut out, line);
        }
    }
    if truncated {
        out.push_str("data: ");
        out.push_str(SSE_TRUNCATION_MARKER.trim_start_matches('\n'));
        out.push('\n');
    }
    out.push('\n');
    out
}

/// One heartbeat comment. Proxies and the read timeout must see bytes
/// even when no job state changed.
pub fn heartbeat_frame() -> &'static str {
    ": ping\n\n"
}

/// Terminal sentinel frame, sent once after the final job-state event.
pub fn done_frame() -> &'static str {
    "data: [DONE]\n\n"
}

/// Decode one SSE chunk into events. Blank lines dispatch; `event:` sets
/// the pending name (last one wins); every `data:` line appends (joined
/// with `\n`); comments (`:` prefix) and unknown fields are ignored.
/// Events with no data are heartbeats and are dropped. A single data
/// payload over [`SSE_DATA_CAP`] (or a line over [`SSE_MAX_LINE_BYTES`])
/// drops that event only — never the whole chunk.
pub fn parse_event_stream(chunk: &str) -> Vec<SseEvent> {
    let mut out = Vec::new();
    let mut name = String::from("message");
    let mut data_lines: Vec<String> = Vec::new();
    let mut dropped = false;
    let flush = |name: &mut String,
                 data_lines: &mut Vec<String>,
                 dropped: &mut bool,
                 out: &mut Vec<SseEvent>| {
        if out.len() >= SSE_MAX_EVENTS_PER_CHUNK {
            return;
        }
        if *dropped {
            *dropped = false;
            *name = String::from("message");
            data_lines.clear();
            return;
        }
        if data_lines.is_empty() {
            *name = String::from("message");
            return;
        }
        let data = data_lines.join("\n");
        out.push(SseEvent {
            event: std::mem::replace(name, String::from("message")),
            data,
        });
        data_lines.clear();
    };
    for raw in chunk.split_inclusive('\n') {
        let line = raw.strip_suffix('\n').unwrap_or(raw);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.len() > SSE_MAX_LINE_BYTES {
            dropped = true;
            continue;
        }
        if line.is_empty() {
            flush(&mut name, &mut data_lines, &mut dropped, &mut out);
            continue;
        }
        if line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            name = sanitize_event_name(value.trim()).to_string();
            continue;
        }
        if let Some(value) = line.strip_prefix("data:") {
            let value = value.strip_prefix(' ').unwrap_or(value);
            let current: usize = data_lines.iter().map(|l| l.len() + 1).sum();
            if current + value.len() > SSE_DATA_CAP {
                dropped = true;
                continue;
            }
            data_lines.push(value.to_string());
            continue;
        }
        // Unknown field (`id:`, `retry:`, …) — ignored per the SSE spec.
    }
    // A chunk that ends mid-event without a trailing blank line still
    // carries a complete event for our producer (it always terminates
    // frames with `\n\n`); flush it so a single-frame chunk parses.
    flush(&mut name, &mut data_lines, &mut dropped, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_names_fall_back_safely() {
        assert_eq!(sanitize_event_name("job"), "job");
        assert_eq!(sanitize_event_name("job-1_ok"), "job-1_ok");
        assert_eq!(sanitize_event_name(""), "message");
        assert_eq!(sanitize_event_name("has space"), "message");
        assert_eq!(sanitize_event_name("a:b"), "message");
        assert_eq!(sanitize_event_name("é"), "message");
    }

    #[test]
    fn format_round_trips_through_the_parser() {
        let frame = format_event("job", "{\"status\":\"running\"}");
        assert!(frame.starts_with("event: job\n"));
        assert!(frame.ends_with("\n\n"));
        let events = parse_event_stream(&frame);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "job");
        assert_eq!(events[0].data, "{\"status\":\"running\"}");
    }

    #[test]
    fn multiline_data_splits_and_rejoins() {
        let frame = format_event("job", "line one\nline two\nline three");
        assert!(frame.contains("data: line one\n"));
        let events = parse_event_stream(&frame);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line one\nline two\nline three");
    }

    #[test]
    fn crlf_in_model_text_cannot_break_framing() {
        let frame = format_event("job", "a\r\nb\r\n");
        assert!(!frame.contains('\r'));
        let events = parse_event_stream(&frame);
        assert_eq!(events[0].data, "a\nb\n");
    }

    #[test]
    fn bad_event_name_never_escapes_the_frame() {
        let frame = format_event("evil\ninjected", "x");
        assert!(frame.starts_with("event: message\n"));
        assert_eq!(parse_event_stream(&frame)[0].event, "message");
    }

    #[test]
    fn oversized_payload_truncates_with_a_marker() {
        let big = "y".repeat(SSE_DATA_CAP + 100);
        let frame = format_event("job", &big);
        let events = parse_event_stream(&frame);
        assert_eq!(events.len(), 1);
        assert!(events[0].data.contains("truncated at 64 KiB"));
        assert!(events[0].data.len() <= SSE_DATA_CAP + 64);
    }

    #[test]
    fn empty_data_still_emits_a_frame() {
        let frame = format_event("job", "");
        let events = parse_event_stream(&frame);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "");
    }

    #[test]
    fn parser_ignores_comments_heartbeats_and_unknown_fields() {
        let chunk = ": ping\n\nretry: 3000\n\nevent: job\ndata: {\"a\":1}\n\n";
        let events = parse_event_stream(chunk);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "job");
        assert_eq!(events[0].data, "{\"a\":1}");
    }

    #[test]
    fn last_event_name_wins_and_data_joins_with_newlines() {
        let chunk = "event: one\nevent: two\ndata: a\ndata: b\n\n";
        let events = parse_event_stream(chunk);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "two");
        assert_eq!(events[0].data, "a\nb");
    }

    #[test]
    fn done_sentinel_survives_as_data() {
        let events = parse_event_stream(done_frame());
        assert_eq!(events.len(), 1);
        assert!(is_done_data(&events[0].data));
        assert!(!is_done_data("{\"status\":\"done\"}"));
    }

    #[test]
    fn oversized_event_drops_only_itself() {
        let big = "z".repeat(SSE_DATA_CAP + 8);
        let chunk =
            format!("event: a\ndata: ok\n\nevent: b\ndata: {big}\n\nevent: c\ndata: ok2\n\n");
        let events = parse_event_stream(&chunk);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "ok");
        assert_eq!(events[1].data, "ok2");
    }

    #[test]
    fn chunk_without_trailing_blank_line_still_parses() {
        let events = parse_event_stream("event: job\ndata: tail");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "tail");
    }

    #[test]
    fn heartbeat_and_done_frames_have_exact_bytes() {
        assert_eq!(heartbeat_frame(), ": ping\n\n");
        assert_eq!(done_frame(), "data: [DONE]\n\n");
    }
}
