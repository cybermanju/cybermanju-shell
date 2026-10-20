# Live profile watcher reliability evidence

- `watcher.rs.before` is the pre-edit source preserved before the reliability change.
- `services-watcher-tests.log` records the focused offline, locked test run.
- Implementation is limited to `gpui-shell/crates/services/src/watcher.rs`.

## API and behavior

- `FileWatcher::try_watch(PathBuf) -> anyhow::Result<tokio::sync::mpsc::Receiver<()>>` synchronously checks for an active Tokio runtime, registers the parent-directory inotify watch, and creates the event stream before returning. The receiver has capacity one.
- Existing `FileWatcher::watch(PathBuf)` remains; it returns the same bounded receiver type and logs registration failures, returning a closed receiver on failure.
- Relevant events for the target filename reset a 200 ms trailing-edge timer. Capacity-one notification coalescing avoids an unbounded queue. Parent self-removal/rename, queue overflow, unmount, ignored-watch notification, and stream failure cause a final best-effort pulse and close.
- The implementation uses the existing inotify 0.11.4 `stream` feature (confirmed enabled by the lockfile) and Tokio timers; no dependency changes or unsafe code were introduced.

## Validation

Command, run in the established credential-free dev environment:

```sh
rustfmt --edition 2024 --check crates/services/src/watcher.rs
cargo test -p services --lib watcher --offline --locked
```

Result: **6 passed, 0 failed**. Tests use real temporary directories/files and inotify, covering synchronous invalid registration and no-runtime failure, invalid-to-valid atomic replacement with trailing-edge delivery, unrelated-file filtering, watched-parent rename notification/closure, and descriptor cleanup after receiver drop. Queue overflow is handled in production code but was not induced in a test.
