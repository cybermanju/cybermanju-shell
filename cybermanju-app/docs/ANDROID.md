# docs/ANDROID.md — Android runbook (CyberManju OS)

> `com.cybermanju.os` on Android: local vault (Tauri IPC), arm64-v8a,
> APK + AAB, `minSdk 24`, `targetSdk 36`. Generated project lives in
> gitignored `src-tauri/gen/` — everything below is applied by
> `scripts/android-configure.sh` (manifest/versionCode) +
> `scripts/android-icons.sh` (launcher icons) after `tauri android init`.

## 1. Packaging

| Item | Value |
|---|---|
| Formats | `--apk --aab` (direct-install APK + Play upload bundle), `--split-per-abi` |
| ABI (release) | `arm64-v8a` only — `ort-sys` ships prebuilt ONNX Runtime only for `aarch64-linux-android` |
| ABI (smoke) | `x86_64` emulator supported by `scripts/android-smoke.sh` (install skipped on ABI mismatch, never fails the build) |
| `minSdkVersion` | 24 (pinned in `tauri.conf.json > bundle.android`) |
| `versionCode` | pinned in `tauri.conf.json > bundle.android.versionCode` = `major*1000000 + minor*1000 + patch` (0.1.1 → 1001). Bump with the release; enforced by `check-version.sh` + `android-configure.sh` |
| Launcher icon | `public/icon-512.png` (512x512 RGBA) → `gen/…/res/mipmap-*` via `scripts/android-icons.sh` (`tauri icon` per the official flow). `tauri android init` stamps the Tauri template's DEFAULT icons — without the sync step every APK ships the Tauri logo |
| Play track | manual upload of the `.aab` (internal track first). No auto-publish yet |

ABI debt: `armv7`/`i686` targets are NOT installed in CI (waste) and not
shipped. If `ort-sys` ever publishes other Android ABIs, re-add the target
+ extend the `--split-per-abi` matrix here.

## 2. Signing parity

* `release.yml` / GitLab: `REQUIRE_SIGNING=1` — missing keystore secrets is a
  hard failure (never publish an uninstallable APK).
* `ci.yml` PRs/forks (no secrets): build continues but the artifact is
  renamed `CyberManju-OS-<ver>-arm64-v8a-unsigned.apk` — **unsigned artifacts
  always say `unsigned` in the file name** (TASKS P2-8).
* `scripts/android-signing.sh cleanup` shreds `cybermanju-release.keystore` +
  `release-signing.properties` before artifact upload (self-hosted hygiene).

## 3. Manifest (applied by `android-configure.sh`)

* Launcher bridge (`cybermanju-launcher` crate, Gaveta de Apps): a
  `<queries>` block for `ACTION_MAIN`/`CATEGORY_LAUNCHER` so
  `queryIntentActivities` sees every launchable app on Android 11+ WITHOUT
  the Play-restricted `QUERY_ALL_PACKAGES` permission, plus a
  `HOME`/`DEFAULT` intent-filter so CyberManju OS can be picked as the
  device Home (digital-OS mode). Both are marker-guarded + idempotent.
* Messages hub (top-right icon): `CybermanjuMessages.kt` (a
  `NotificationListenerService`, written fresh by §4 on every configure
  run — marker-guarded, foreign files are never overwritten) mirrors
  mail/chat/social notifications into `cybermanju-messages.json` in the
  app-private files dir (200-entry ring, dismissed items kept as history).
  The hub's **Clear all** button wipes the file and fires an explicit
  `CLEAR_NOTIFICATIONS` intent at the running service, which also dismisses
  the live shade (`cancelAllNotifications`, best-effort).
  The manifest gets the `<service>` declaration (bound to
  `BIND_NOTIFICATION_LISTENER_SERVICE`) plus explicit `<package>`
  queries for the top social apps. The Rust crate only *reads* the file
  (`launcher_list_messages` / `launcher_clear_messages` /
  `launcher_notification_state`); notification access itself is granted by
  the user in system settings via the hub's enable prompt
  (`launcher_open_notification_settings`). No message content ever leaves
  the device.

* Permissions: `INTERNET`, `ACCESS_NETWORK_STATE` (LAN REST + OAuth),
  `ACCESS_FINE/COARSE_LOCATION` (GPS MapView), `CAMERA` (face capture),
  `RECORD_AUDIO` (voice input), `POST_NOTIFICATIONS`. Manifest declarations do
  not display a prompt by themselves: location/camera/microphone/notification
  prompts are tied to the corresponding user-triggered feature/API call. CPU,
  RAM/process metrics and WebView storage quota are not Android runtime
  permissions, so they must not trigger a system dialog. File access uses the
  system picker (SAF) rather than broad shared-storage permission.
* `android:usesCleartextTraffic="true"` — the LAN dashboard is plain
  `http://nas:3456`; without this Android 9+ blocks the REST transport.
* `android:allowBackup="false"` (fail-closed): auto-backup of `cybermanju.db`
  + keys to Google cloud is a key-exfiltration risk. Backup path = manual
  encrypted export (Settings → Export), verified by restore test.
* Intents: `cybermanju://` deep link + `.cyb3` payload `VIEW` association.
* FileProvider (`OPEN WITH SYSTEM`): `<provider>` with authority
  `${applicationId}.fileprovider`, `res/xml/cybermanju_paths.xml` roots
  (`<root-path path="/"`, written fresh by §6 each configure run) and an
  explicit `androidx.core:core` dependency — local files travel as
  grantable `content://` URIs with `FLAG_GRANT_READ_URI_PERMISSION`
  because `file://` data throws `FileUriExposedException` on Android 7+.
  Marker-guarded + idempotent like every other manifest patch.

## 4. Storage model (scoped storage)

* Database + Tantivy index + keys live in the app-private files dir
  (`/data/data/com.cybermanju.os/files`, override `CYBERMANJU_DATA_DIR` /
  `DB_PATH`). No `MANAGE_EXTERNAL_STORAGE`, no shared-storage writes.
* `capabilities/mobile.json` mirrors desktop permissions for that dir only.
* Import from shared storage goes through the system picker
  (`dialog:allow-open`), never raw filesystem paths.

## 5. Runtime differences from desktop

* Web Dashboard is **not started** on mobile (`#[cfg(mobile)]` in `lib.rs`):
  no localhost server, no battery drain, no Play-policy localhost-server
  review. Dashboard IPC commands stay registered and report `unsupported:`.
* Startup failures `panic!` (logcat + backtrace) instead of
  `process::exit(1)`, with a `CyberManju OS FATAL (panic): …` breadcrumb on
  stderr first so a crash before first paint is never silent.
* Logging init is re-entry safe (`try_init` + `RUST_LOG` fallback): an
  Android activity recreate never dies on "global subscriber already set".
  `tauri-plugin-log` was removed entirely (2026-10-08): it calls
  `log::set_logger` at `run()`, which collides with our own
  `tracing_subscriber` init AND with a second `run()` in the same process
  (activity recreate) — both surfaced as
  `PluginInitialization("log", "attempted to set a logger after the logging
  system was already initialized")` + `.expect()` panic before first paint.
  The frontend never imported `@tauri-apps/plugin-log`; Rust logs reach
  logcat via stderr/tracing.
* Storage is self-healing, not bricking: a torn `cybermanju.db` is renamed
  to `cybermanju.corrupt-<unix_ts>.bak` beside the DB and recreated fresh;
  a torn `tantivy_index/` is wiped and recreated (fully rebuildable from
  the DB via `rebuild_search_index`). Either event is loud in logcat
  (`quarantining` / `wiping and recreating`).
* The app-private files dir is **probed, not hardcoded**: `DB_PATH` /
  `CYBERMANJU_ANDROID_FILES_DIR` win when set, otherwise every candidate
  (`/data/data/…` symlink, each `/data/user/<id>/…` real dir) is proven
  writable (mkdir + probe file) before use — work profiles / secondary
  users no longer EACCES at first launch.
* Tantivy index is `mmap`'d — large restores should run on Wi-Fi/charger.
  The writer arena is 15 MB on Android (50 MB elsewhere) — 15 MB is the
  smallest heap Tantivy 0.22 accepts (smaller values fail writer creation
  and bricked every launch pre-2026-10-08) — so a low-end phone
  doesn't take an LMK kill before first paint.

## 6. Frontend on phones

* `viewport-fit=cover` + `theme-color` + safe-area padding on
  `.cybermanju-shell` (`env(safe-area-inset-*)`), `100dvh` height.
* System back gesture → `popstate` → close transient UI, else path-history
  back (same as desktop `go_back`).
* `body.cybermanju-offline` (from `online`/`offline` events) dims
  `.server-only` controls instead of hanging.
* CSP covers both WebView asset schemes: `asset: http://asset.localhost
  https://asset.localhost`, plus an explicit `connect-src` (LAN dashboard
  over plain http, provider APIs, Supabase) so the WebView is allowed to
  reach the network it is designed to talk to.
* One opener seam for links and device files — `src/utils/openExternal.ts`
  routes per runtime: Android → `launcher_open_url` / `launcher_open_file`
  (Intent bridge in `cybermanju-launcher`, scheme allow-list
  `https/http/mailto/tel/cybermanju://`, OAuth `?code=`/`?state=` never
  echoed into errors), desktop Tauri → `@tauri-apps/plugin-shell`
  `open()`, plain browser → `window.open` (tabs). `plugin-shell open`
  cannot work on Android: it spawns `xdg-open`, which the OS does not
  ship — every Google/GitHub/GitLab sign-in button used to die with
  `Scoped shell IO error: No such file or directory (os error 2)`.

## 7. Update channel

* Direct APK: manual download + install (unknown sources); `versionCode`
  increments every release so upgrades apply cleanly.
* Play: upload the `.aab`; staged rollout via internal → production tracks.

## 8. Verification

1. `bash scripts/check-version.sh` (versionCode pinned + derived).
2. CI `android-build` (ci.yml, release.yml, GitLab mirror) — the real
   sequence: `tauri android init` → icons (`android-icons.sh`) → configure
   (`android-configure.sh`) → Gradle harden (`android-gradle-harden.sh`) →
   signing prepare (`android-signing.sh prepare`, `REQUIRE_SIGNING=1` on
   release + GitLab) → build (`--apk --aab --split-per-abi`) →
   `apksigner verify` (`android-signing.sh verify`) → on-device smoke
   (`android-smoke.sh`; `continue-on-error` on GitHub, best-effort on
   GitLab — skips without an arm64 emulator) → cleanup
   (`android-signing.sh cleanup`, shreds the keystore — `if: always()` on
   GitHub, `after_script` on GitLab, so a failed build/verify shreds too) →
   upload APK+AAB.

   Required secrets (exact names read by `scripts/android-signing.sh`):
   * `ANDROID_KEYSTORE_B64` — the release keystore, base64
   * `ANDROID_KEYSTORE_PASSWORD` — the keystore password
   * `ANDROID_KEY_ALIAS` — the key entry alias (defaults to `cybermanju-os`)
   * `ANDROID_KEY_PASSWORD` — the key password (defaults to the store
     password when unset)
3. Manual: `adb install CyberManju-OS-<ver>-arm64-v8a.apk` or
   `bash scripts/android-smoke.sh <apk>`.

## 9. Troubleshooting an instant close (installs, never shows a window)

The smoke test now fails on this (process liveness at 8s/13s + logcat
markers), but on a physical phone do it by hand:

```bash
adb logcat -c
# launch the app, wait ~10s, then (the -A 10 matters: the refusal reason
# is printed on the lines FOLLOWING the marker — a bare grep shows only
# the `lib.rs:43` breadcrumb with no cause):
adb logcat -d | grep -A 10 "CyberManju OS FATAL"
# wider net when the above is empty:
adb logcat -d | grep -iE "FATAL EXCEPTION|AndroidRuntime|has died|Force finishing activity|Fatal error while running|quarantining|not writable"
```

What to look for:

| Symptom in logcat | Meaning | Fix |
|---|---|---|
| `CyberManju OS FATAL (panic): …` | Rust panic before first paint; the message names the cause. Always capture with `grep -A 10` — without context lines you only see the `lib.rs:43` breadcrumb | Read the message — DB/index lines below cover the common ones |
| `…quarantining the file…` / `moved corrupt database to …` | Torn `cybermanju.db` was quarantined to `cybermanju.corrupt-<ts>.bak` and recreated — app should now open (vault content needs re-import/restore) | Re-import or restore from backup; the `.bak` stays beside the DB for forensics |
| `…wiping and recreating…` | Torn search index was rebuilt — app opens, search repopulates on next index / `rebuild_search_index` | Nothing (or run Rebuild index from Settings) |
| `Android files-dir candidate not writable` × N | Scoped-storage path problem (work profile, broken symlink) | Set `DB_PATH`/`CYBERMANJU_ANDROID_FILES_DIR` or reinstall from the same user profile |
| `FATAL EXCEPTION … UnsatisfiedLinkError` | ABI mismatch (x86/arm APK on the wrong device) | Install the `arm64-v8a` artifact on an arm64 phone |
| Nothing at all, process just dies | LMK/OOM kill (low RAM + large vault) or ANR | Free RAM, retry on Wi-Fi/charger; file a bug with device model + RAM + Android version |

When reporting, always include: device model, Android version, APK file
name (signed vs `-unsigned`), and the logcat excerpt above.
