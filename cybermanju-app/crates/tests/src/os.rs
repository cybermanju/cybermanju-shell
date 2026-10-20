// CyberManju OS — AGENT-8 OS layer tests (cybsh, tasks, compute)
//
// Pre-created and registered by the supervisor. Acceptance lives in
// `scripts/os-acceptance.sh` Tier 2; unit/contract cases belong here.
//
// These cases drive the OS layer the way a client meets it: through the real
// router, with the auth gate in front of it, plus the command-table contract
// the terminal's tab-completion is built on.

use crate::web::{bearer, body_of, call, mint, mk_dashboard, now_secs, status_of};
use std::sync::OnceLock;

const JTI: &str = "jti-agent8-os";

/// Point the process-wide volume at a scratch directory *before* the kernel
/// resolves its root — first caller wins, and every test then shares the same
/// throwaway directory instead of the developer's real volume.
fn scratch_volume() -> &'static std::path::Path {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    let dir = DIR.get_or_init(|| {
        let dir = tempfile::tempdir().expect("scratch volume");
        std::env::set_var("CYBERMANJU_DATA_DIR", dir.path());
        dir
    });
    dir.path()
}

fn auth(d: &cybermanju_web::WebDashboard) -> String {
    bearer(&mint(d, "user", now_secs() + 3_600, JTI))
}

fn json_of(response: &str) -> serde_json::Value {
    serde_json::from_str(body_of(response)).unwrap_or(serde_json::Value::Null)
}

#[test]
fn shell_prompt_is_stable() {
    assert_eq!(cybermanju_os::PROMPT, "cybsh> ");
}

/// The command table is the contract behind `help`, did-you-mean and
/// tab-completion — every command the brief lists has to be reachable.
#[test]
fn cybsh_command_table_covers_the_brief() {
    let table = cybermanju_os::command_table();
    for command in [
        "help",
        "history",
        "clear",
        "version",
        "ls",
        "cd",
        "pwd",
        "cat",
        "cp",
        "mv",
        "rm",
        "mkdir",
        "touch",
        "stat",
        "du",
        "df",
        "mount",
        "umount",
        "disk",
        "providers",
        "quota",
        "sync",
        "scrub",
        "repair",
        "gc",
        "lease",
        "ps",
        "top",
        "kill",
        "jobs",
        "compute",
        "workers",
        "keygen",
        "encrypt",
        "decrypt",
        "search",
    ] {
        assert!(
            table.contains(&command),
            "`{command}` missing from the command table"
        );
    }

    // Tab-completion is fed from this table: prefixes resolve to the real
    // commands, and unknown prefixes return nothing rather than guessing.
    let d_hits = cybermanju_os::completions("d");
    assert!(
        d_hits.contains(&"df".to_string()),
        "completions(d) = {d_hits:?}"
    );
    assert!(
        d_hits.contains(&"disk".to_string()),
        "completions(d) = {d_hits:?}"
    );
    let k_hits = cybermanju_os::completions("k");
    assert!(k_hits.contains(&"kill".to_string()) && k_hits.contains(&"keygen".to_string()));
    assert!(cybermanju_os::completions("zzz").is_empty());
}

/// `/api/os/*` sits behind the auth gate like everything else, and the
/// terminal really executes: a pipeline runs, an unknown command reports a
/// did-you-mean error instead of a crash.
#[test]
fn os_routes_are_gated_and_the_terminal_executes() {
    let (_dir, d) = mk_dashboard(3456);

    for path in ["/api/os/ps", "/api/os/top", "/api/os/df", "/api/os/workers"] {
        let resp = call(&d, "GET", path, "", None);
        assert_eq!(status_of(&resp), 401, "{path} must be gated: {resp}");
    }
    let resp = call(&d, "POST", "/api/os/exec", r#"{"line":"echo hi"}"#, None);
    assert_eq!(status_of(&resp), 401, "exec must be gated: {resp}");

    let token = auth(&d);

    let resp = call(
        &d,
        "POST",
        "/api/os/exec",
        r#"{"line":"echo hello cybsh"}"#,
        Some(&token),
    );
    assert_eq!(status_of(&resp), 200, "exec: {resp}");
    let body = json_of(&resp);
    assert_eq!(body["ok"], true, "exec body: {body}");
    assert_eq!(body["output"], "hello cybsh");
    assert_eq!(body["prompt"], "cybsh> ");

    // Pipelines and `&&` are parsed by the interpreter, not by the client.
    let resp = call(
        &d,
        "POST",
        "/api/os/exec",
        r#"{"line":"echo a && echo b"}"#,
        Some(&token),
    );
    let body = json_of(&resp);
    assert_eq!(body["output"], "a\nb", "resp: {body}");

    let resp = call(
        &d,
        "POST",
        "/api/os/exec",
        r#"{"line":"definitely-not-a-command"}"#,
        Some(&token),
    );
    assert_eq!(
        status_of(&resp),
        200,
        "a failing command is still a 200: {resp}"
    );
    let body = json_of(&resp);
    assert_eq!(body["ok"], false, "body: {body}");
    assert!(
        body["output"]
            .as_str()
            .unwrap_or_default()
            .starts_with("unknown command:"),
        "body: {body}"
    );

    // `--json` switches a command to machine-readable output.
    let resp = call(
        &d,
        "POST",
        "/api/os/exec",
        r#"{"line":"ps --json"}"#,
        Some(&token),
    );
    let body = json_of(&resp);
    let inner: serde_json::Value =
        serde_json::from_str(body["output"].as_str().unwrap_or_default())
            .expect("ps --json emits JSON");
    assert!(inner.get("counts").is_some(), "ps snapshot: {inner}");

    // Read side of the family.
    let resp = call(&d, "GET", "/api/os/ps", "", Some(&token));
    assert_eq!(status_of(&resp), 200, "ps: {resp}");
    let ps = json_of(&resp);
    assert!(
        ps.get("counts").is_some() && ps.get("tasks").is_some(),
        "ps: {ps}"
    );

    let resp = call(&d, "GET", "/api/os/workers", "", Some(&token));
    let workers = json_of(&resp);
    assert!(
        workers["total"].as_u64().unwrap_or(0) >= 1,
        "the fan-out pool always has at least one slot: {workers}"
    );
}

/// File commands act on a real volume: create, list, size, remove — all
/// through the interpreter, never a mock.
#[test]
fn file_commands_act_on_a_real_volume() {
    scratch_volume();

    cybermanju_os::execute("mkdir -p /os-suite/nested", None).expect("mkdir");
    cybermanju_os::execute("touch /os-suite/nested/file.txt", None).expect("touch");

    let listed = cybermanju_os::execute("ls /os-suite", None).expect("ls");
    assert!(listed.contains("nested"), "ls: {listed}");

    let stat = cybermanju_os::execute("stat /os-suite/nested/file.txt --json", None).expect("stat");
    let stat: serde_json::Value = serde_json::from_str(&stat).expect("stat json");
    assert_eq!(stat["kind"], "file", "stat: {stat}");

    let du = cybermanju_os::execute("du /os-suite --json", None).expect("du");
    let du: serde_json::Value = serde_json::from_str(&du).expect("du json");
    assert_eq!(du["files"], 1, "du: {du}");

    // Volume accounting without any disk attached — a fresh volume is not
    // "disk full", it is scratch space.
    let df = cybermanju_os::execute("df", None).expect("df");
    assert!(df.contains("scratch") || df.contains("free"), "df: {df}");

    cybermanju_os::execute("rm -r /os-suite", None).expect("rm");
    assert!(cybermanju_os::execute("ls /os-suite", None).is_err());

    let volume = scratch_volume();
    assert!(
        !volume.join("volume/os-suite").exists(),
        "the suite cleaned up after itself"
    );
}
