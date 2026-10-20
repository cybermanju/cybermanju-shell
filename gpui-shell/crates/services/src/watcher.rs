//! File watcher service using inotify.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context as _;
use futures_util::StreamExt as _;
use inotify::{EventMask, Inotify, WatchMask};
use tokio::sync::mpsc;
use tokio::time::Instant;

const DEBOUNCE: Duration = Duration::from_millis(200);
const NOTIFICATION_CAPACITY: usize = 1;
const EVENT_BUFFER_SIZE: usize = 4096;

pub struct FileWatcher;

impl FileWatcher {
    /// Start watching `path` and return a bounded stream of reload notifications.
    ///
    /// Registration is completed before this function returns, so path, inotify,
    /// and runtime errors are reported to the caller instead of being deferred to
    /// a background thread. A Tokio runtime must be active on the calling thread.
    pub fn try_watch(path: PathBuf) -> anyhow::Result<mpsc::Receiver<()>> {
        let runtime = tokio::runtime::Handle::try_current()
            .context("FileWatcher::try_watch requires an active Tokio runtime")?;
        let (watch_dir, watch_name) = watch_target(&path)?;

        // Register synchronously: callers can decide whether a watcher is
        // optional before accepting a receiver that can never produce events.
        let inotify = Inotify::init().context("failed to initialize inotify")?;
        inotify
            .watches()
            .add(
                watch_dir,
                WatchMask::MODIFY
                    | WatchMask::CLOSE_WRITE
                    | WatchMask::CREATE
                    | WatchMask::DELETE
                    | WatchMask::MOVED_FROM
                    | WatchMask::MOVED_TO
                    | WatchMask::ATTRIB
                    | WatchMask::MOVE_SELF
                    | WatchMask::DELETE_SELF,
            )
            .with_context(|| format!("failed to watch directory {}", watch_dir.display()))?;
        let stream = inotify
            .into_event_stream([0; EVENT_BUFFER_SIZE])
            .context("failed to create inotify event stream")?;

        let (tx, rx) = mpsc::channel(NOTIFICATION_CAPACITY);
        runtime.spawn(watch_events(stream, watch_name, tx));
        Ok(rx)
    }

    /// Compatibility entrypoint for existing config/theme callers.
    ///
    /// Registration failures are logged and represented as a closed bounded
    /// receiver; callers needing to distinguish that case should use `try_watch`.
    pub fn watch(path: PathBuf) -> mpsc::Receiver<()> {
        match Self::try_watch(path) {
            Ok(receiver) => receiver,
            Err(error) => {
                tracing::warn!("File watcher could not start: {error:#}");
                let (tx, rx) = mpsc::channel(NOTIFICATION_CAPACITY);
                drop(tx);
                rx
            }
        }
    }
}

async fn watch_events(
    mut stream: inotify::EventStream<[u8; EVENT_BUFFER_SIZE]>,
    watch_name: OsString,
    tx: mpsc::Sender<()>,
) {
    let mut debounce_deadline: Option<Instant> = None;

    loop {
        let timer = async {
            match debounce_deadline {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => std::future::pending::<()>().await,
            }
        };

        tokio::select! {
            // Fair branch selection lets the deadline fire even if unrelated
            // files in the watched directory are changing continuously.
            _ = tx.closed() => break,
            event = stream.next() => {
                let Some(event) = event else {
                    // Unexpected stream exhaustion means the watch is no longer
                    // trustworthy; prompt a final reload and close the channel.
                    notify(&tx);
                    break;
                };
                let event = match event {
                    Ok(event) => event,
                    Err(error) => {
                        tracing::warn!("File watcher event stream failed: {error}");
                        notify(&tx);
                        break;
                    }
                };

                let mask = event.mask;
                if mask.intersects(
                    EventMask::Q_OVERFLOW
                        | EventMask::MOVE_SELF
                        | EventMask::DELETE_SELF
                        | EventMask::UNMOUNT
                        | EventMask::IGNORED,
                ) {
                    // Events may have been lost, or the watched directory has
                    // disappeared. Reload once, then fail closed rather than
                    // silently watching a stale directory forever.
                    notify(&tx);
                    break;
                }

                let is_target = event.name.as_deref() == Some(watch_name.as_os_str());
                if is_target
                    && mask.intersects(
                        EventMask::MODIFY
                            | EventMask::CLOSE_WRITE
                            | EventMask::CREATE
                            | EventMask::DELETE
                            | EventMask::MOVED_FROM
                            | EventMask::MOVED_TO
                            | EventMask::ATTRIB,
                    )
                {
                    debounce_deadline = Some(Instant::now() + DEBOUNCE);
                }
            }
            _ = timer => {
                debounce_deadline = None;
                match tx.try_send(()) {
                    Ok(()) | Err(mpsc::error::TrySendError::Full(())) => {
                        // A queued notification already represents a reload of
                        // the latest state. Capacity one coalesces bursts without
                        // blocking event draining or allocating an unbounded queue.
                    }
                    Err(mpsc::error::TrySendError::Closed(())) => break,
                }
            }
        }
    }
}

fn notify(tx: &mpsc::Sender<()>) {
    // On a full channel, its pending pulse will be received after the event
    // batch was observed, and therefore still prompts a current-state reload.
    let _ = tx.try_send(());
}

fn watch_target(path: &Path) -> anyhow::Result<(&Path, OsString)> {
    let parent = path.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid config path has no parent directory: {}",
            path.display()
        )
    })?;
    let name = path.file_name().ok_or_else(|| {
        anyhow::anyhow!("Invalid config path has no filename: {}", path.display())
    })?;
    Ok((parent, name.to_os_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::{sleep, timeout};

    static NEXT_TEMP_DIR: AtomicUsize = AtomicUsize::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("gpui-file-watcher-{}-{id}", std::process::id()));
            fs::create_dir(&path).expect("create temporary watcher directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn registration_requires_a_runtime_and_reports_invalid_target() {
        let error = FileWatcher::try_watch(PathBuf::from("/"))
            .expect_err("no-runtime registration must fail rather than panic");
        assert!(error.to_string().contains("active Tokio runtime"));
    }

    #[tokio::test]
    async fn registration_errors_are_synchronous_for_bad_targets() {
        assert!(FileWatcher::try_watch(PathBuf::from("/")).is_err());

        let missing_parent = std::env::temp_dir()
            .join(format!("gpui-file-watcher-missing-{}", std::process::id()))
            .join("settings.toml");
        let error = FileWatcher::try_watch(missing_parent)
            .expect_err("missing parent must fail during registration");
        assert!(error.to_string().contains("failed to watch directory"));
    }

    #[tokio::test]
    async fn atomic_replacement_delivers_one_trailing_edge_for_final_valid_file() {
        let dir = TempDir::new();
        let path = dir.path().join("appearance.toml");
        fs::write(&path, b"invalid = [\n").unwrap();
        let mut rx = FileWatcher::try_watch(path.clone()).unwrap();

        for partial in [b"invalid = [\n".as_slice(), b"invalid = {\n", b"[partial\n"] {
            fs::write(&path, partial).unwrap();
            sleep(Duration::from_millis(25)).await;
        }
        let replacement = dir.path().join("appearance.toml.tmp");
        fs::write(&replacement, b"[profile]\nname = \"valid\"\n").unwrap();
        let final_event_at = Instant::now();
        fs::rename(&replacement, &path).unwrap();

        assert_eq!(
            timeout(Duration::from_secs(3), rx.recv()).await.unwrap(),
            Some(())
        );
        assert_eq!(fs::read(&path).unwrap(), b"[profile]\nname = \"valid\"\n");
        assert!(Instant::now().duration_since(final_event_at) >= Duration::from_millis(150));
        assert!(
            timeout(Duration::from_millis(300), rx.recv())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn unrelated_files_are_filtered() {
        let dir = TempDir::new();
        let path = dir.path().join("appearance.toml");
        fs::write(&path, b"initial").unwrap();
        let mut rx = FileWatcher::try_watch(path).unwrap();

        fs::write(dir.path().join("unrelated.txt"), b"not the profile").unwrap();
        assert!(
            timeout(Duration::from_millis(300), rx.recv())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn parent_rename_notifies_then_closes_the_watcher() {
        let base = TempDir::new();
        let watched_dir = base.path().join("watched");
        fs::create_dir(&watched_dir).unwrap();
        let path = watched_dir.join("appearance.toml");
        fs::write(&path, b"initial").unwrap();
        let mut rx = FileWatcher::try_watch(path).unwrap();

        fs::rename(&watched_dir, base.path().join("renamed")).unwrap();
        assert_eq!(
            timeout(Duration::from_secs(2), rx.recv()).await.unwrap(),
            Some(())
        );
        assert_eq!(
            timeout(Duration::from_secs(2), rx.recv()).await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn dropping_receiver_releases_inotify_descriptor() {
        let dir = TempDir::new();
        let path = dir.path().join("appearance.toml");
        fs::write(&path, b"initial").unwrap();
        let before = inotify_fd_count();
        let rx = FileWatcher::try_watch(path).unwrap();
        timeout(Duration::from_secs(2), async {
            while inotify_fd_count() <= before {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("registered inotify descriptor should be visible");

        drop(rx);
        timeout(Duration::from_secs(2), async {
            while inotify_fd_count() > before {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("dropping receiver should stop the task and close its descriptor");
    }

    fn inotify_fd_count() -> usize {
        fs::read_dir("/proc/self/fd")
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| {
                fs::read_link(entry.path())
                    .map(|target| target.to_string_lossy().contains("inotify"))
                    .unwrap_or(false)
            })
            .count()
    }
}
