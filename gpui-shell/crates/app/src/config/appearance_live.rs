//! Explicit cosmetic-file overlay; no preference writes or implicit app-store access.
//! Invalid updates retain the last good palette. File watching is opt-in, not an
//! automatic browser/Tauri transport or proof of an Asterinas desktop session.

use std::path::PathBuf;

use anyhow::Result;
use gpui::{App, Global};
use services::LocalSlot;
use tokio::sync::mpsc;
use ui::Theme;

use super::appearance::AppearanceProfile;
use crate::state::AppState;

/// Ephemeral, validated import; the original app remains the preference owner.
pub struct AppearanceOverlay {
    profile: AppearanceProfile,
}

impl Global for AppearanceOverlay {}

impl AppearanceOverlay {
    /// Prepare the complete replacement before mutating cached/global state.
    fn replace_loaded(&mut self, loaded: Result<AppearanceProfile>, base: &Theme) -> Result<Theme> {
        let profile = loaded?;
        let projected = profile.project_theme(base)?;
        self.profile = profile;
        Ok(projected)
    }

    /// Imported colors take precedence, while native typography remains native.
    fn project(&self, base: &Theme) -> Result<Theme> {
        self.profile.project_theme(base)
    }

    pub fn install(profile: AppearanceProfile, cx: &mut App) -> Result<()> {
        let theme = profile.project_theme(Theme::global(cx))?;
        cx.set_global(Self { profile });
        Theme::set(theme, cx);
        Ok(())
    }

    /// Preserve the imported palette when theme.toml reloads; do not discard a
    /// valid import merely because an unrelated native font setting changes.
    pub fn over_native_theme(base: Theme, cx: &App) -> Result<Theme> {
        match cx.try_global::<Self>() {
            Some(overlay) => overlay.project(&base),
            None => Ok(base),
        }
    }

    /// The caller registers the watch before rereading the startup profile, so
    /// a save between the initial validation and watch setup cannot be lost.
    pub fn start_watch(path: PathBuf, mut rx: mpsc::Receiver<()>, cx: &mut App) {
        cx.spawn(async move |cx| {
            while rx.recv().await.is_some() {
                let read_path = path.clone();
                // At most one bounded load is in flight; regular filesystem I/O
                // may still wait, but it does not run on the GPUI foreground.
                let loaded = cx
                    .background_executor()
                    .spawn(async move { AppearanceProfile::load(&read_path) })
                    .await;
                cx.update(|cx| {
                    let base = Theme::global(cx).clone();
                    let result = cx.global_mut::<Self>().replace_loaded(loaded, &base);
                    match result {
                        Ok(theme) => {
                            Theme::set(theme, cx);
                            AppState::notification(cx).clear_local(LocalSlot::AppearanceError);
                            cx.refresh_windows();
                        }
                        Err(error) => super::notify_parse_failure(
                            LocalSlot::AppearanceError,
                            &path.display().to_string(),
                            &error,
                            cx,
                        ),
                    }
                });
            }
            // A lost watcher must be visible, not silently advertised as live.
            let error = anyhow::anyhow!(
                "appearance file watcher stopped; restart the shell to resume live-file import"
            );
            cx.update(|cx| {
                super::notify_parse_failure(
                    LocalSlot::AppearanceError,
                    &path.display().to_string(),
                    &error,
                    cx,
                )
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::{
        fs,
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn fixture() -> Value {
        serde_json::from_slice(include_bytes!(
            "appearance/fixtures/appearance-profile-v1.json"
        ))
        .unwrap()
    }

    fn parsed(value: Value) -> AppearanceProfile {
        AppearanceProfile::parse(&serde_json::to_vec(&value).unwrap()).unwrap()
    }

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("gpui-live-profile-{}-{nonce}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn file(&self) -> PathBuf {
            self.0.join("appearance.json")
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn write(path: &Path, value: &Value) {
        fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    }

    #[test]
    fn real_file_invalid_delete_symlink_and_recovery_keep_last_good_tokens() {
        let dir = Directory::new();
        let path = dir.file();
        write(&path, &fixture());
        let mut overlay = AppearanceOverlay {
            profile: AppearanceProfile::load(&path).unwrap(),
        };
        let base = Theme::default();
        let initial = overlay.project(&base).unwrap();
        fs::write(&path, b"{partial").unwrap();
        assert!(
            overlay
                .replace_loaded(AppearanceProfile::load(&path), &base)
                .is_err()
        );
        assert_eq!(
            overlay.project(&base).unwrap().colors().accent,
            initial.colors().accent
        );
        fs::remove_file(&path).unwrap();
        assert!(
            overlay
                .replace_loaded(AppearanceProfile::load(&path), &base)
                .is_err()
        );
        let target = dir.0.join("target.json");
        write(&target, &fixture());
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(
            overlay
                .replace_loaded(AppearanceProfile::load(&path), &base)
                .is_err()
        );
        assert_eq!(
            overlay.project(&base).unwrap().palette.base00,
            initial.palette.base00
        );
        fs::remove_file(&path).unwrap();
        let mut changed = fixture();
        changed["settings"]["accent"] = json!("#12ab34");
        write(&target, &changed);
        fs::rename(&target, &path).unwrap();
        let recovered = overlay
            .replace_loaded(AppearanceProfile::load(&path), &base)
            .unwrap();
        assert_ne!(recovered.colors().accent, initial.colors().accent);
        assert_eq!(
            overlay.project(&base).unwrap().colors().accent,
            recovered.colors().accent
        );
    }

    #[test]
    fn native_theme_reload_preserves_imported_colors_but_updates_native_font() {
        let mut profile = fixture();
        profile["settings"]["density"] = json!("compact");
        let overlay = AppearanceOverlay {
            profile: parsed(profile),
        };
        let before = overlay.project(&Theme::default()).unwrap();
        let native = Theme {
            font_size: gpui::px(19.0),
            ..Theme::default()
        };
        let after = overlay.project(&native).unwrap();
        assert_eq!(after.colors().accent, before.colors().accent);
        assert_eq!(after.palette.base00, before.palette.base00);
        assert_eq!(after.font_size, gpui::px(19.0));
        assert_eq!(after.density, ui::LayoutDensity::Compact);
    }

    #[tokio::test]
    async fn real_file_watch_feeds_theme_updates_and_keeps_bad_saves_out() {
        let dir = Directory::new();
        let path = dir.file();
        write(&path, &fixture());
        let mut overlay = AppearanceOverlay {
            profile: AppearanceProfile::load(&path).unwrap(),
        };
        let mut rx = services::FileWatcher::try_watch(path.clone()).unwrap();
        let base = Theme::default();
        let initial = overlay.project(&base).unwrap();
        let replacement = dir.0.join("replacement.json");
        let mut changed = fixture();
        changed["settings"]["accent"] = json!("#12ab34");
        changed["settings"]["density"] = json!("compact");
        write(&replacement, &changed);
        fs::rename(&replacement, &path).unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv())
                .await
                .unwrap()
                .is_some()
        );
        let accepted = overlay
            .replace_loaded(AppearanceProfile::load(&path), &base)
            .unwrap();
        assert_ne!(accepted.colors().accent, initial.colors().accent);
        assert_eq!(accepted.density, ui::LayoutDensity::Compact);
        fs::write(&path, b"{incomplete").unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv())
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            overlay
                .replace_loaded(AppearanceProfile::load(&path), &base)
                .is_err()
        );
        assert_eq!(
            overlay.project(&base).unwrap().colors().accent,
            accepted.colors().accent
        );
        assert_eq!(
            overlay.project(&base).unwrap().density,
            ui::LayoutDensity::Compact
        );
        changed["settings"]["accent"] = json!("#ef0012");
        changed["settings"]["density"] = json!("comfortable");
        write(&replacement, &changed);
        fs::rename(&replacement, &path).unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv())
                .await
                .unwrap()
                .is_some()
        );
        let recovered = overlay
            .replace_loaded(AppearanceProfile::load(&path), &base)
            .unwrap();
        assert_ne!(recovered.colors().accent, accepted.colors().accent);
        assert_eq!(recovered.density, ui::LayoutDensity::Comfortable);
    }

    #[test]
    fn failed_projection_cannot_replace_a_previously_valid_profile() {
        let mut overlay = AppearanceOverlay {
            profile: parsed(fixture()),
        };
        let base = Theme::default();
        let previous = overlay.project(&base).unwrap();
        let mut invalid = parsed(fixture());
        invalid.settings.accent = Some("not-a-color".into());
        assert!(overlay.replace_loaded(Ok(invalid), &base).is_err());
        assert_eq!(
            overlay.project(&base).unwrap().colors().accent,
            previous.colors().accent
        );
    }
}
