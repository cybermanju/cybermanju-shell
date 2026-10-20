//! CyberManju cosmetic snapshot bridge. No network, credentials, config writes,
//! compositor commands, wallpaper downloads or claims about the Asterinas runtime.

use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use ui::{Base16Palette, Theme};

const MAX_PROFILE_BYTES: u64 = 64 * 1024;
const MAX_WALLPAPER_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeId {
    OsDark,
    OsLight,
    OsGraphite,
    PlasmaDark,
    PlasmaLight,
}

impl ThemeId {
    fn name(self) -> &'static str {
        match self {
            Self::OsDark => "os-dark",
            Self::OsLight => "os-light",
            Self::OsGraphite => "os-graphite",
            Self::PlasmaDark => "plasma-dark",
            Self::PlasmaLight => "plasma-light",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellStyle {
    Macos,
    Plasma,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    Comfortable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Motion {
    Auto,
    Full,
    Reduced,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub theme: ThemeId,
    pub shell: Option<ShellStyle>,
    pub accent: Option<String>,
    pub accents: BTreeMap<String, String>,
    pub density: Density,
    pub glass: u8,
    pub motion: Motion,
    pub follow_system: bool,
    pub wallpaper: String,
    pub glow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Wallpaper {
    Preset {
        id: String,
    },
    Url {
        url: String,
    },
    LocalFile {
        name: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
        size: u64,
    },
}

/// The original cosmetic preferences remain intact; only the supported token
/// projection is applied. Never treat retained settings as an applied desktop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppearanceProfile {
    schema: String,
    version: u32,
    pub settings: Settings,
    pub shell_style: ShellStyle,
    pub panel_docked: bool,
    pub wallpaper: Wallpaper,
}

fn valid_preset(id: &str) -> bool {
    matches!(id, "slopes-dark" | "slopes-light" | "dunes")
}

fn valid_hex(hex: &str) -> bool {
    matches!(hex.len(), 4 | 7)
        && hex.starts_with('#')
        && hex.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

impl AppearanceProfile {
    /// Read no more than 64 KiB plus a sentinel byte, even if the file grows
    /// between metadata and reading. Local wallpaper contents are not in v1.
    pub fn load(path: &Path) -> Result<Self> {
        ensure!(path.is_file(), "appearance profile must be a regular file");
        let file = File::open(path).context("cannot open appearance profile")?;
        ensure!(
            file.metadata()?.is_file(),
            "appearance profile is not a regular file"
        );
        let mut bytes = Vec::new();
        file.take(MAX_PROFILE_BYTES + 1).read_to_end(&mut bytes)?;
        Self::parse(&bytes)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() as u64 <= MAX_PROFILE_BYTES,
            "appearance profile exceeds 64 KiB"
        );
        let profile: Self =
            serde_json::from_slice(bytes).context("invalid appearance profile JSON")?;
        ensure!(
            profile.schema == "com.cybermanju.appearance",
            "unsupported appearance schema"
        );
        ensure!(profile.version == 1, "unsupported appearance version");
        ensure!(profile.settings.glass <= 3, "invalid glass level");
        ensure!(
            valid_preset(&profile.settings.wallpaper),
            "unknown wallpaper preset"
        );
        ensure!(
            profile
                .settings
                .shell
                .is_none_or(|s| s == profile.shell_style),
            "conflicting shell preferences"
        );
        if let Some(accent) = &profile.settings.accent {
            ensure!(valid_hex(accent), "invalid global accent");
        }
        ensure!(
            profile.settings.accents.len() <= 5,
            "too many theme accents"
        );
        for (id, accent) in &profile.settings.accents {
            ensure!(
                matches!(
                    id.as_str(),
                    "os-dark" | "os-light" | "os-graphite" | "plasma-dark" | "plasma-light"
                ),
                "unknown accent theme"
            );
            ensure!(valid_hex(accent), "invalid per-theme accent");
        }
        match &profile.wallpaper {
            Wallpaper::Preset { id } => {
                ensure!(valid_preset(id), "unknown wallpaper preset");
                ensure!(
                    id == &profile.settings.wallpaper,
                    "conflicting wallpaper presets"
                );
            }
            Wallpaper::Url { url } => {
                ensure!(
                    url.len() <= 8192 && url.trim() == url && !url.chars().any(char::is_control),
                    "invalid wallpaper URL text"
                );
                let parsed = url::Url::parse(url).context("invalid wallpaper URL")?;
                ensure!(
                    matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some(),
                    "wallpaper URL requires HTTP(S) and a host"
                );
                ensure!(
                    parsed.username().is_empty() && parsed.password().is_none(),
                    "wallpaper URL must not contain credentials"
                );
            }
            Wallpaper::LocalFile {
                name,
                mime_type,
                size,
            } => {
                ensure!(
                    !name.is_empty()
                        && name.len() <= 255
                        && !name.contains(['/', '\\'])
                        && !name.chars().any(char::is_control),
                    "wallpaper filename must be a bounded basename"
                );
                ensure!(
                    matches!(
                        mime_type.as_str(),
                        "image/avif"
                            | "image/gif"
                            | "image/jpg"
                            | "image/jpeg"
                            | "image/png"
                            | "image/webp"
                    ),
                    "unsupported wallpaper MIME type"
                );
                ensure!(
                    *size > 0 && *size <= MAX_WALLPAPER_BYTES,
                    "invalid wallpaper file size"
                );
            }
        }
        Ok(profile)
    }

    /// Project only the supported semantic color palette. Geometry, density,
    /// compositor policy and the media-query/system follower are NOT emulated.
    pub fn project_theme(&self, current: &Theme) -> Result<Theme> {
        let (bg, surface, raised, muted, text, built_in_accent) = match self.settings.theme {
            ThemeId::OsDark => (
                "#000000", "#0B0B0D", "#161618", "#929298", "#F5F5F7", "#0A84FF",
            ),
            ThemeId::OsLight => (
                "#ECECEC", "#FFFFFF", "#F5F5F7", "#636369", "#1D1D1F", "#007AFF",
            ),
            ThemeId::OsGraphite => (
                "#000000", "#101012", "#17171A", "#929298", "#F5F5F7", "#8E8E93",
            ),
            ThemeId::PlasmaDark => (
                "#1b1e20", "#232629", "#2a2e32", "#bdc3c7", "#eff0f1", "#3daee9",
            ),
            ThemeId::PlasmaLight => (
                "#dcdfe3", "#fcfcfc", "#f4f5f6", "#4d5559", "#232629", "#178dc2",
            ),
        };
        let accent = self
            .settings
            .accent
            .as_deref()
            .or_else(|| {
                self.settings
                    .accents
                    .get(self.settings.theme.name())
                    .map(String::as_str)
            })
            .unwrap_or(built_in_accent);
        // Base16 is the shell's native semantic representation, not an exact
        // clone of the Vue chrome. Keep typography, replace color overrides so
        // an old native accent cannot silently defeat the imported preference.
        let palette = Base16Palette::from_hex(&[
            bg, surface, raised, muted, muted, text, text, text, "#da4453", "#f67400", "#FF9F0A",
            "#27ae60", "#64D2FF", accent, "#AF52DE", "#bdc3c7",
        ])?;
        let mut theme = Theme::from_palette(
            format!("CyberManju {}", self.settings.theme.name()),
            palette,
        );
        theme.font_size = current.font_size;
        Ok(theme)
    }

    pub fn report(&self) -> String {
        let mut retained = vec![
            "wallpaper",
            "shellStyle",
            "panelDocked",
            "density",
            "glass",
            "motion",
            "glow",
        ];
        if self.settings.follow_system {
            retained.push("followSystem (snapshot theme only)");
        }
        format!(
            "CyberManju appearance v1: theme={} and accent token projection; retained, NOT applied: {}. No live sync or Asterinas/GPU certification.",
            self.settings.theme.name(),
            retained.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn fixture() -> Value {
        json!({"schema":"com.cybermanju.appearance","version":1,
            "settings":{"theme":"os-dark","accent":null,"accents":{},"density":"compact","glass":2,"motion":"reduced","followSystem":false,"wallpaper":"slopes-dark","glow":false},
            "shellStyle":"macos","panelDocked":true,"wallpaper":{"kind":"preset","id":"slopes-dark"}})
    }
    fn parse(value: Value) -> Result<AppearanceProfile> {
        AppearanceProfile::parse(&serde_json::to_vec(&value).unwrap())
    }

    #[test]
    fn shared_vue_export_fixture_projects_through_the_native_gpui_theme() {
        let profile = AppearanceProfile::parse(include_bytes!(
            "appearance/fixtures/appearance-profile-v1.json"
        ))
        .unwrap();
        assert_eq!(profile.version, 1);
        let theme = profile.project_theme(&Theme::default()).unwrap();
        assert_eq!(theme.colors().accent, theme.palette.base0d);
        assert!(profile.report().contains("NOT applied"));
    }

    #[test]
    fn preserves_all_preferences_without_claiming_unsupported_application() {
        let value = fixture();
        let profile = parse(value.clone()).unwrap();
        let encoded = serde_json::to_value(&profile).unwrap();
        for key in ["settings", "shellStyle", "panelDocked", "wallpaper"] {
            if key == "settings" {
                for (name, expected) in value[key].as_object().unwrap() {
                    assert_eq!(&encoded[key][name], expected);
                }
            } else {
                assert_eq!(encoded[key], value[key]);
            }
        }
        assert!(profile.report().contains("retained, NOT applied"));
        assert!(profile.report().contains("No live sync"));
    }

    #[test]
    fn actual_gpui_theme_uses_global_then_per_theme_then_builtin_accent() {
        let base = Theme::default();
        let builtin = parse(fixture()).unwrap().project_theme(&base).unwrap();
        let mut value = fixture();
        value["settings"]["accents"] = json!({"os-dark":"#ff0000","os-light":"#00ff00"});
        let per_theme = parse(value.clone()).unwrap().project_theme(&base).unwrap();
        assert_ne!(builtin.colors().accent, per_theme.colors().accent);
        value["settings"]["accent"] = json!("#0000ff");
        let global = parse(value).unwrap().project_theme(&base).unwrap();
        assert_ne!(global.colors().accent, per_theme.colors().accent);
        assert_eq!(global.font_size, base.font_size);
        assert_eq!(
            global.colors().accent,
            Base16Palette::from_hex(&["#0000ff"; 16]).unwrap().base0d
        );
    }

    #[test]
    fn all_five_app_themes_project_real_light_or_dark_tokens() {
        for (id, dark) in [
            ("os-dark", true),
            ("os-light", false),
            ("os-graphite", true),
            ("plasma-dark", true),
            ("plasma-light", false),
        ] {
            let mut value = fixture();
            value["settings"]["theme"] = json!(id);
            let theme = parse(value)
                .unwrap()
                .project_theme(&Theme::default())
                .unwrap();
            assert_eq!(theme.appearance().is_dark(), dark, "{id}");
            assert_eq!(theme.colors().accent, theme.palette.base0d);
        }
    }

    #[test]
    fn rejects_unknown_fields_versions_bad_types_and_conflicting_preferences() {
        for (path, invalid) in [
            (vec!["version"], json!(2)),
            (vec!["schema"], json!("foreign")),
            (vec!["settings", "glass"], json!(4)),
            (vec!["settings", "theme"], json!("foreign")),
            (vec!["settings", "density"], json!("dense")),
            (vec!["settings", "accent"], json!("red")),
            (vec!["settings", "accents"], json!({"foreign":"#ffffff"})),
            (vec!["settings", "shell"], json!("plasma")),
            (vec!["settings", "wallpaper"], json!("unknown")),
            (vec!["panelDocked"], json!("1")),
            (vec!["serverUrl"], json!("https://private.invalid")),
            (vec!["settings", "apiKey"], json!("must-not-be-exported")),
            (vec!["wallpaper", "token"], json!("must-not-be-exported")),
        ] {
            let mut value = fixture();
            if path.len() == 1 {
                value[path[0]] = invalid;
            } else {
                value[path[0]][path[1]] = invalid;
            }
            assert!(parse(value).is_err(), "{path:?}");
        }
    }

    #[test]
    fn wallpaper_metadata_is_validated_but_never_fetched_or_applied() {
        for url in [
            "https://example.invalid/picture.png",
            "http://[::1]/picture",
        ] {
            let mut value = fixture();
            value["wallpaper"] = json!({"kind":"url","url":url});
            assert!(parse(value).is_ok());
        }
        for url in [
            "file:///tmp/a",
            "blob:https://example.invalid/id",
            "data:image/png;base64,AAAA",
            "https://user:pass@example.invalid/a",
            "https://:pass@example.invalid/a",
            "https://example.invalid/\nsecret",
            "/relative",
            "https://",
        ] {
            let mut value = fixture();
            value["wallpaper"] = json!({"kind":"url","url":url});
            assert!(parse(value).is_err(), "unsafe metadata was accepted");
        }
        let mut value = fixture();
        value["wallpaper"] =
            json!({"kind":"local-file","name":"wallpaper.png","mimeType":"image/png","size":42});
        let profile = parse(value.clone()).unwrap();
        assert!(profile.report().contains("wallpaper"));
        for field in ["path", "blob", "data"] {
            let mut extra = value.clone();
            extra["wallpaper"][field] = json!("not-an-asset-transfer");
            assert!(parse(extra).is_err());
        }
        for name in ["../wallpaper.png", "C:\\wallpaper.png", "", "control\n.png"] {
            let mut extra = value.clone();
            extra["wallpaper"]["name"] = json!(name);
            assert!(parse(extra).is_err());
        }
        for size in [0, MAX_WALLPAPER_BYTES + 1] {
            let mut extra = value.clone();
            extra["wallpaper"]["size"] = json!(size);
            assert!(parse(extra).is_err());
        }
    }

    #[test]
    fn rejects_unbounded_or_duplicate_top_level_input() {
        assert!(AppearanceProfile::parse(&vec![b' '; MAX_PROFILE_BYTES as usize + 1]).is_err());
        let json = serde_json::to_string(&fixture()).unwrap();
        let duplicate = json.replacen("{", "{\"version\":1,", 1);
        assert!(AppearanceProfile::parse(duplicate.as_bytes()).is_err());
    }
}
