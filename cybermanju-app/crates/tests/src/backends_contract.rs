// CyberManju OS — Backends contract (crates/tests/src/backends_contract.rs).
// AGENT-4 item 10 — provider contract tests against a local mock server.
//
// Every HTTP assertion below runs against a mock bound to 127.0.0.1, so the
// suite exercises the real `reqwest` client, the real URL building and the
// real status handling with no traffic leaving the machine.
//
// Behaviours AGENT-1 has not implemented yet are deliberately *not* pinned
// here (they would fail): pagination loops (`list_files` follows no
// `nextPageToken` / `page=N`), a retry-with-backoff layer on 429, and a
// size-limit preflight that refuses a file (>100 MB only switches GitHub's
// transport). The request-count bounds below are written so that adding any
// of them stays green — see "Requests to other agents" in AGENT-4.md.

use cybermanju_sync::backends::{GitLabBackend, LocalBackend};
use cybermanju_sync::{classify_error, ErrorClass, StorageBackend};
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Read one HTTP/1.1 request (head + body) or give up.
fn read_request(stream: &mut TcpStream) -> Option<(String, String)> {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..read]);
        if buf.len() > 1 << 20 {
            return None;
        }
    };

    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let content_length = head
        .split("\r\n")
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);

    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    Some((head, String::from_utf8_lossy(&body).into_owned()))
}

fn write_response(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Mock",
    };
    let len = body.len();
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}"
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.shutdown(Shutdown::Write);
}

/// A minimal single-threaded HTTP server. Returns the base URL to point a
/// backend at and a counter of the requests it has seen.
fn mock_server(
    handler: impl Fn(&str, &str) -> (u16, String) + Send + Sync + 'static,
) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("mock bind");
    let addr = listener.local_addr().expect("mock addr");
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&hits);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let Some((head, body)) = read_request(&mut stream) else {
                continue;
            };
            counter.fetch_add(1, Ordering::SeqCst);
            let is_head = head.starts_with("HEAD ");
            let (status, mut payload) = handler(&head, &body);
            // A HEAD response must not carry a body, whatever the handler says.
            if is_head {
                payload.clear();
            }
            write_response(&mut stream, status, &payload);
        }
    });

    (format!("http://{addr}"), hits)
}

fn gitlab(base: &str) -> GitLabBackend {
    GitLabBackend::new("test-token", "42", "main", Some(base))
}

// ─── Listing ────────────────────────────────────────────────────────

#[test]
fn gitlab_list_files_parses_the_tree_and_skips_directories() {
    let (saw_token, base, _hits) = {
        let saw_token = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&saw_token);
        let (base, hits) = mock_server(move |head, _| {
            if head.to_lowercase().contains("private-token: test-token") {
                flag.store(true, Ordering::SeqCst);
            }
            assert!(head.starts_with("GET "), "{head}");
            assert!(head.contains("/api/v4/projects/42/"), "{head}");
            (
                200,
                r#"[
                    {"type":"blob","name":"a.txt","path":"docs/a.txt"},
                    {"type":"blob","name":"b.txt","path":"docs/b.txt"},
                    {"type":"tree","name":"sub","path":"docs/sub"}
                ]"#
                .to_string(),
            )
        });
        (saw_token, base, hits)
    };

    let files = gitlab(&base).list_files("docs").expect("list");
    assert_eq!(
        files.len(),
        2,
        "directories must be filtered out: {files:?}"
    );
    assert_eq!(files[0].name, "a.txt");
    assert_eq!(files[0].path, "docs/a.txt");
    assert!(
        files[0].url.contains("/-/raw/main/docs/a.txt"),
        "{:?}",
        files[0].url
    );
    assert!(
        saw_token.load(Ordering::SeqCst),
        "PRIVATE-TOKEN must be sent"
    );
}

#[test]
fn gitlab_list_files_returns_every_entry_the_server_hands_out() {
    let (base, hits) = mock_server(|_, _| {
        (
            200,
            r#"[{"type":"blob","name":"one","path":"one"},{"type":"blob","name":"two","path":"two"}]"#
                .to_string(),
        )
    });

    let files = gitlab(&base).list_files("").expect("list");
    assert_eq!(files.len(), 2, "{files:?}");
    // One request is the current behaviour; a future pagination loop must stay
    // bounded (an unbounded `while next_page` is a hang, not a feature).
    assert!(
        hits.load(Ordering::SeqCst) <= 10,
        "listing issued {} requests",
        hits.load(Ordering::SeqCst)
    );
}

// ─── Error classification ───────────────────────────────────────────

#[test]
fn gitlab_list_files_never_returns_ok_for_an_error_status() {
    for status in [401u16, 403, 404, 429, 500] {
        let (base, _hits) = mock_server(move |_, _| (status, r#"{"message":"nope"}"#.to_string()));
        let err = gitlab(&base)
            .list_files("")
            .expect_err("an error status must never become Ok");
        // The message must carry a class prefix (`auth: `, `not_found: `, …)
        // — `classify` reads it to decide whether retrying can help.
        assert_ne!(
            classify_error(&err),
            ErrorClass::Unclassified,
            "unclassified: {err}"
        );
        assert!(err.contains(&format!("list failed ({status})")), "{err}");
    }
}

#[test]
fn gitlab_download_never_writes_a_file_for_an_error_status() {
    for status in [401u16, 404, 429] {
        let (base, _hits) =
            mock_server(move |_, _| (status, r#"{"message":"404 File Not Found"}"#.to_string()));
        let out = tempfile::tempdir().expect("tempdir");
        let target = out.path().join("fetched.txt");

        let err = gitlab(&base)
            .download_file("docs/a.txt", target.to_str().unwrap())
            .expect_err("an error status must never become Ok");
        assert_ne!(
            classify_error(&err),
            ErrorClass::Unclassified,
            "unclassified: {err}"
        );
        assert!(!target.exists(), "a failed download left {target:?}");
    }
}

#[test]
fn gitlab_delete_treats_204_as_success_and_404_as_failure() {
    let (base, _hits) = mock_server(|_, _| (204, String::new()));
    gitlab(&base)
        .delete_file("docs/a.txt")
        .expect("204 is success");

    let (base, _hits) = mock_server(|_, _| (404, r#"{"message":"nope"}"#.to_string()));
    let err = gitlab(&base)
        .delete_file("docs/a.txt")
        .expect_err("404 must not be Ok");
    // A 404 is a *decision* (retrying cannot make the file appear), so it
    // must land in the NotFound class rather than the retryable ones.
    assert_eq!(classify_error(&err), ErrorClass::NotFound, "{err}");
    assert!(err.contains("delete failed"), "{err}");
}

#[test]
fn gitlab_upload_classifies_a_rejected_write() {
    let (base, _hits) = mock_server(|head, _| {
        if head.starts_with("HEAD ") {
            (404, String::new())
        } else {
            (500, r#"{"message":"boom"}"#.to_string())
        }
    });

    let src = tempfile::tempdir().expect("tempdir");
    let file = src.path().join("note.txt");
    fs::write(&file, b"payload").expect("write source");

    let err = gitlab(&base)
        .upload_file(file.to_str().unwrap(), "docs/note.txt")
        .expect_err("500 must not be Ok");
    assert_eq!(classify_error(&err), ErrorClass::Network, "{err}");
}

#[test]
fn gitlab_test_connection_reports_the_http_status_it_got() {
    let (base, _hits) = mock_server(|_, _| (200, "{}".to_string()));
    assert!(gitlab(&base).test_connection().expect("probe"));

    let (base, _hits) = mock_server(|_, _| (500, String::new()));
    let err = gitlab(&base)
        .test_connection()
        .expect_err("500 must not be Ok");
    assert!(err.contains("HTTP 500"), "{err}");
    assert_eq!(classify_error(&err), ErrorClass::Network, "{err}");
}

// ─── 429 / preflight contracts ──────────────────────────────────────

#[test]
fn gitlab_429_is_bounded_and_never_reported_as_success() {
    let (base, hits) = mock_server(|_, _| (429, r#"{"message":"slow down"}"#.to_string()));

    let err = gitlab(&base)
        .list_files("")
        .expect_err("429 must not be Ok");
    assert_eq!(classify_error(&err), ErrorClass::RateLimited, "{err}");

    // Today the backend makes exactly one attempt. If a retry/backoff layer
    // is added it must stay bounded — an unbounded loop is an outage.
    let attempts = hits.load(Ordering::SeqCst);
    assert!(
        (1..=10).contains(&attempts),
        "{attempts} attempts for one 429"
    );
}

#[test]
fn gitlab_upload_preflights_the_source_file_before_touching_the_network() {
    let (base, hits) = mock_server(|_, _| (200, "{}".to_string()));

    let err = gitlab(&base)
        .upload_file("/nonexistent/definitely-missing.txt", "docs/x.txt")
        .expect_err("a missing source must fail");
    assert!(err.contains("Failed to read file"), "{err}");
    assert_eq!(
        hits.load(Ordering::SeqCst),
        0,
        "no request may be sent when the source does not exist"
    );
}

// ─── Happy paths ────────────────────────────────────────────────────

#[test]
fn gitlab_download_writes_the_bytes_it_received() {
    let (base, _hits) = mock_server(|_, _| (200, "hello from the mock".to_string()));

    let out = tempfile::tempdir().expect("tempdir");
    let target = out.path().join("fetched.txt");
    gitlab(&base)
        .download_file("docs/a.txt", target.to_str().unwrap())
        .expect("download");
    assert_eq!(
        fs::read_to_string(&target).expect("read"),
        "hello from the mock"
    );
}

#[test]
fn gitlab_upload_creates_then_reports_a_blob_url() {
    let (base, hits) = mock_server(|head, _| {
        if head.starts_with("HEAD ") {
            (404, String::new())
        } else if head.starts_with("POST ") {
            (201, r#"{"file_path":"docs/note.txt"}"#.to_string())
        } else {
            (500, "unexpected method".to_string())
        }
    });

    let src = tempfile::tempdir().expect("tempdir");
    let file = src.path().join("note.txt");
    fs::write(&file, b"payload").expect("write source");

    let url = gitlab(&base)
        .upload_file(file.to_str().unwrap(), "docs/note.txt")
        .expect("upload");
    assert!(url.contains("/-/blob/main/docs/note.txt"), "{url}");
    // HEAD (exists?) + POST (create) — a retry layer may add attempts, but
    // the happy path must not loop.
    let attempts = hits.load(Ordering::SeqCst);
    assert!(
        (2..=10).contains(&attempts),
        "{attempts} requests for one upload"
    );
}

// ─── The local backend: no network at all ───────────────────────────

#[test]
fn local_backend_round_trips_files() {
    let remote = tempfile::tempdir().expect("remote dir");
    let source = tempfile::tempdir().expect("source dir");
    let src_file = source.path().join("note.txt");
    fs::write(&src_file, b"hello local").expect("write source");

    let backend = LocalBackend::new(remote.path().to_str().unwrap());
    assert!(backend.test_connection().expect("connection"));

    backend
        .upload_file(src_file.to_str().unwrap(), "notes/note.txt")
        .expect("upload");

    let listed = backend.list_files("notes").expect("list");
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].name, "note.txt");
    assert_eq!(listed[0].size_bytes, "hello local".len() as u64);

    let dest = source.path().join("roundtrip.txt");
    backend
        .download_file("notes/note.txt", dest.to_str().unwrap())
        .expect("download");
    assert_eq!(fs::read_to_string(&dest).expect("read"), "hello local");

    backend.delete_file("notes/note.txt").expect("delete");
    assert!(backend.list_files("notes").expect("list").is_empty());

    // Downloading something that was never uploaded is an error, not silence.
    let out = source.path().join("absent.txt");
    assert!(backend
        .download_file("notes/absent.txt", out.to_str().unwrap())
        .is_err());
}
