//! Android launcher bridge (Gaveta de Apps) — thin Tauri wrappers.
//!
//! The heavy lifting (PackageManager JNI, icon raster, icon-pack lookup)
//! lives in the reusable `cybermanju-launcher` crate so any Rust-on-Android
//! host can use it. These commands only validate args, forward, and keep the
//! AGENT-1 error prefixes (`unsupported:` off-Android, `invalid:`,
//! `not_found:` from the crate).

use cybermanju_launcher::{AndroidApp, IconPack, NotificationState, StoredMessage};

/// List every launchable Android app, label-sorted, with 96px PNG icons.
///
/// Desktop/web/WASM refuse with `unsupported:` — there is no mock data; the
/// drawer renders that state honestly instead of fake rows.
#[tauri::command(async)]
pub fn launcher_list_apps() -> Result<Vec<AndroidApp>, String> {
    cybermanju_launcher::list_apps()
}

/// Open one app by package name (`FLAG_ACTIVITY_NEW_TASK`).
#[tauri::command(async)]
pub fn launcher_open_app(package_name: String) -> Result<(), String> {
    cybermanju_launcher::validate_package_name(&package_name)?;
    cybermanju_launcher::open_app(package_name.trim())
}

/// Open the system uninstall sheet for one package (`ACTION_DELETE`).
#[tauri::command(async)]
pub fn launcher_uninstall_app(package_name: String) -> Result<(), String> {
    cybermanju_launcher::validate_package_name(&package_name)?;
    cybermanju_launcher::uninstall_app(package_name.trim())
}

/// List installed icon-pack themes (theme-discovery intents).
#[tauri::command(async)]
pub fn launcher_list_icon_packs() -> Result<Vec<IconPack>, String> {
    cybermanju_launcher::list_icon_packs()
}

/// Resolve one app's icon from one icon pack (best-effort PNG data URL).
#[tauri::command(async)]
pub fn launcher_pack_icon(pack_package: String, app_package: String) -> Result<String, String> {
    cybermanju_launcher::validate_package_name(&pack_package)?;
    cybermanju_launcher::validate_package_name(&app_package)?;
    cybermanju_launcher::pack_icon(pack_package.trim(), app_package.trim())
}

/// List installed mail/chat/social apps only (email, WhatsApp, Instagram,
/// Facebook, Messenger, Telegram, …) — the messages-hub quick launch.
#[tauri::command(async)]
pub fn launcher_list_social_apps() -> Result<Vec<AndroidApp>, String> {
    cybermanju_launcher::list_social_apps()
}

/// Stored messaging/social notifications, newest first (from the host app's
/// notification-listener mirror; capped server-side).
#[tauri::command(async)]
pub fn launcher_list_messages(limit: Option<usize>) -> Result<Vec<StoredMessage>, String> {
    cybermanju_launcher::list_messages(limit.unwrap_or(100))
}

/// Wipe the stored notification mirror.
#[tauri::command(async)]
pub fn launcher_clear_messages() -> Result<(), String> {
    cybermanju_launcher::clear_messages()
}

/// Whether the user granted notification access to this app.
#[tauri::command(async)]
pub fn launcher_notification_state() -> Result<NotificationState, String> {
    cybermanju_launcher::notification_state()
}

/// Jump to the system notification-access settings screen.
#[tauri::command(async)]
pub fn launcher_open_notification_settings() -> Result<(), String> {
    cybermanju_launcher::open_notification_settings()
}

/// Hand an external link to the system resolver (`Intent.ACTION_VIEW`).
///
/// Android-only path behind `src/utils/openExternal.ts`: the WebView cannot
/// launch a browser itself and `plugin-shell open` spawns `xdg-open`, which
/// Android does not ship — that failure surfaced as
/// `Scoped shell IO error: No such file or directory (os error 2)` on every
/// Google/GitHub/GitLab sign-in button. Desktop builds keep the shell opener
/// in the frontend, so they never reach this command (`unsupported:` there).
#[tauri::command(async)]
pub fn launcher_open_url(url: String) -> Result<(), String> {
    cybermanju_launcher::open_url(url.trim())
}

/// Hand one device file to a viewer/editor (`OPEN WITH SYSTEM`).
///
/// Same Android-only story as `launcher_open_url`, over a FileProvider
/// `content://` URI — Android 7+ refuses `file://` data
/// (`FileUriExposedException`). Directories refuse with `unsupported:` (no
/// app-to-app folder hand-off on Android).
#[tauri::command(async)]
pub fn launcher_open_file(path: String) -> Result<(), String> {
    cybermanju_launcher::open_file(path.trim())
}
