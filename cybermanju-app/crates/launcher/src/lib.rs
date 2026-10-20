//! CyberManju launcher bridge — reusable crate behind the Tauri commands.
//!
//! Android talks to `PackageManager` the same way every open-source launcher
//! does (Lawnchair/Launcher3, KISS, Unlauncher):
//!
//! * list: `Intent(ACTION_MAIN).addCategory(CATEGORY_LAUNCHER)` +
//!   `queryIntentActivities(..., 0)` (see `QUERY_ACTIVITY_FLAGS`), label via
//!   `loadLabel`, icon via `loadIcon` rastered to a 96px PNG data URL.
//! * open: `getLaunchIntentForPackage(pkg)` + `FLAG_ACTIVITY_NEW_TASK`,
//!   falling back to an explicit `ComponentName` intent resolved through a
//!   `setPackage` query (some launchables expose no launch intent).
//! * social: `is_social_package` flags mail/chat/social rows (email,
//!   WhatsApp, Telegram, Instagram, Facebook, Messenger, …) for the hub.
//! * messages: a `NotificationListenerService` (injected into the host app by
//!   `scripts/android-configure.sh`) mirrors messaging/social notifications
//!   into `cybermanju-messages.json` in the app-private files dir; this crate
//!   only *reads* that file — no notification permission lives in Rust.
//! * links/files: `open_url` (allow-listed schemes → `Intent.ACTION_VIEW`) and
//!   `open_file` (FileProvider `content://` hand-off) are the Android answer to
//!   `plugin-shell open`, which spawns `xdg-open` and dies on Android with
//!   `Scoped shell IO error: No such file or directory (os error 2)` — the
//!   WebView has no browser/file opener of its own. Only `.cyb3` vault
//!   payloads may leave the app-private secrets dir through `open_file`.
//!
//! The JNI layer lives behind `#[cfg(target_os = "android")]` and uses only
//! `jni` + `ndk-context` (no Kotlin, no `gen/android` coupling), so the crate
//! stays reusable from any Rust-on-Android host. Everywhere else the entry
//! points refuse honestly with `unsupported:` — never mocked rows.
//!
//! JNI notes (verified against `jni` 0.21): `JObject` is neither `Clone` nor
//! `Copy`, so every `JValue::Object` arg is a freshly owned object (new
//! strings, `new_local_ref` copies, or call results). Each app row runs in
//! its own `with_local_frame` — without it a 300-app drawer overflows the
//! 512-entry local-reference table and the VM aborts.

use serde::{Deserialize, Serialize};

/// Raster width/height for app icons (PNG data URLs stay small over IPC).
pub const ICON_PX: i32 = 96;

/// Notification mirror written by the host app's listener service.
pub const MESSAGES_FILE: &str = "cybermanju-messages.json";
/// Ring-buffer cap (mirrored in the Kotlin service — keep both in sync).
pub const MESSAGES_MAX: usize = 200;

/// One launchable Android app — the Gaveta row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidApp {
    pub package_name: String,
    pub label: String,
    pub activity_class: String,
    pub system_app: bool,
    /// True for mail/chat/social packages (see `is_social_package`).
    pub social_app: bool,
    /// `data:image/png;base64,…` 96px raster. Empty when the drawable could
    /// not be rastered — the frontend renders a letter tile instead.
    pub icon_base64: String,
}

/// An installed icon-pack theme (discovery intents only, no icon fetch).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IconPack {
    pub package_name: String,
    pub label: String,
}

/// One stored notification from a messaging/social app (newest first).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    pub key: String,
    pub package_name: String,
    pub app_label: String,
    pub title: String,
    pub text: String,
    pub timestamp: i64,
    pub category: String,
}

/// Notification-access state for the messages hub prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationState {
    pub enabled: bool,
    pub detail: String,
}

/// Theme-discovery intents every major launcher queries (Lawnchair +
/// ADW/GO/Nova legacy actions).
pub const ICON_PACK_ACTIONS: &[&str] = &[
    "com.anddoes.launcher.THEME",
    "org.adw.launcher.THEMES",
    "com.gau.go.launcherex.theme",
    "com.novalauncher.THEME",
    "com.teslacoilsw.launcher.THEME",
];

/// Exact package names of mail/chat/social apps (email, WhatsApp,
/// Instagram, Facebook, Messenger, Telegram, Signal, …).
pub const SOCIAL_PACKAGES: &[&str] = &[
    "com.whatsapp",
    "com.whatsapp.w4b",
    "org.telegram.messenger",
    "org.telegram.plus",
    "org.thunderdog.challegram",
    "tw.nekomimi.nekogram",
    "org.signal.private.messenger",
    "com.instagram.android",
    "com.instagram.barcelona",
    "com.facebook.orca",
    "com.facebook.katana",
    "com.facebook.lite",
    "com.facebook.mlite",
    "com.google.android.gm",
    "com.microsoft.office.outlook",
    "com.yahoo.mobile.client.android.mail",
    "ch.protonmail.android",
    "com.tutanota.android",
    "eu.faircode.email",
    "com.fsck.k9",
    "com.readdle.spark",
    "me.bluemail.mail",
    "com.ninefolders.hd3",
    "org.kman.aquamail",
    "de.gmx.mobile.android.mail",
    "de.web.mobile.android.mail",
    "com.samsung.android.email.provider",
    "com.samsung.android.messaging",
    "com.google.android.apps.messaging",
    "com.google.android.apps.tachyon",
    "com.google.android.apps.meet",
    "com.discord",
    "com.slack",
    "com.microsoft.teams",
    "com.twitter.android",
    "com.zhiliaoapp.musically",
    "com.ss.android.ugc.trill",
    "com.snapchat.android",
    "com.linkedin.android",
    "com.reddit.frontpage",
    "com.pinterest",
    "com.skype.raider",
    "com.viber.voip",
    "jp.naver.line.android",
    "com.tencent.mm",
    "com.tencent.mobileqq",
    "com.vk.android",
    "com.kakao.talk",
    "com.imo.android.imoim",
    "com.zing.zalo",
    "com.tumblr",
    "xyz.blueskyweb.app",
    "org.joinmastodon.android",
    "com.keylesspalace.tusky",
    "com.truthsocial.android.app",
];

/// Long distinctive tokens matched with `contains` on the lowercased
/// package (all ≥5 chars — no false positives like `line` in `offline`).
const SOCIAL_TOKENS: &[&str] = &[
    "whatsapp",
    "telegram",
    "messenger",
    "instagram",
    "facebook",
    "snapchat",
    "discord",
    "signal",
    "viber",
    "kakao",
    "wechat",
    "weixin",
    "outlook",
    "tumblr",
    "reddit",
    "skype",
    "teams",
    "slack",
    "pinterest",
    "linkedin",
    "twitter",
    "tiktok",
    "mastodon",
    "tusky",
    "bluesky",
    "proton",
    "tutanota",
    "yahoo",
    "threads",
    "musical",
    "trill",
    "zalo",
    "challegram",
    "nekomimi",
    "orca",
    "katana",
    "faircode",
];

/// Short tokens matched on dot-segments only (`line` must not fire inside
/// `offline` / `airline` / `headline`).
const SOCIAL_SEGMENTS: &[&str] = &["line", "mail", "sms", "mms", "talk", "chat"];

/// True when a package looks like a mail/chat/social app. Pure and
/// unit-tested; the Kotlin listener mirrors the same rule.
pub fn is_social_package(package: &str) -> bool {
    let lower = package.trim().to_lowercase();
    if lower.is_empty() {
        return false;
    }
    if SOCIAL_PACKAGES.contains(&lower.as_str()) {
        return true;
    }
    if SOCIAL_TOKENS.iter().any(|t| lower.contains(t)) {
        return true;
    }
    lower.split('.').any(|seg| SOCIAL_SEGMENTS.contains(&seg))
}

/// AGENT-1 `invalid:` refusal for bad package names (shared by all entry
/// points so Rust + TS validate identically).
pub fn validate_package_name(package: &str) -> Result<(), String> {
    let p = package.trim();
    if p.is_empty() {
        return Err("invalid: packageName is required".to_string());
    }
    if p.len() > 256 {
        return Err("invalid: packageName is too long".to_string());
    }
    let mut chars = p.chars();
    let first = chars.next().unwrap_or(' ');
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(format!("invalid: bad packageName '{p}'"));
    }
    if !p
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
        return Err(format!("invalid: bad packageName '{p}'"));
    }
    Ok(())
}

/// Case-insensitive label sort — pure, unit-tested, shared by every host.
pub fn sort_apps(mut apps: Vec<AndroidApp>) -> Vec<AndroidApp> {
    apps.sort_by(|a, b| {
        a.label
            .to_lowercase()
            .cmp(&b.label.to_lowercase())
            .then_with(|| a.package_name.cmp(&b.package_name))
    });
    apps
}

/// Drawable-name candidates for an icon-pack lookup of one app package.
pub fn pack_icon_candidates(package: &str) -> Vec<String> {
    let base: String = package
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = base.trim_matches('_').to_string();
    let mut out = vec![
        trimmed.clone(),
        format!("{trimmed}_icon"),
        format!("ic_{trimmed}"),
    ];
    out.sort();
    out.dedup();
    out.retain(|s| !s.is_empty());
    out
}

/// Link schemes an external open may hand to the system resolver — the same
/// set the desktop shell-open scope accepts (`tauri.conf.json > plugins >
/// shell > open`), so the Android Intent bridge never widens it. Everything
/// else (`intent:`, `file:`, `javascript:`, …) stays refused with `invalid:`
/// so a hostile link cannot be routed into the package resolver.
pub const OPEN_SCHEMES: [&str; 5] = ["https://", "http://", "mailto:", "tel:", "cybermanju://"];

/// True when `url` starts with one of `OPEN_SCHEMES` and carries a non-empty
/// target after it (a bare `https://` is not openable).
pub fn is_openable_url(url: &str) -> bool {
    let u = url.trim();
    OPEN_SCHEMES.iter().any(|s| has_scheme(u, s))
}

/// True when `u` starts with `scheme` (case-insensitive) plus more text.
fn has_scheme(u: &str, scheme: &str) -> bool {
    let head = u.get(..scheme.len());
    u.len() > scheme.len() && head.is_some_and(|p| p.eq_ignore_ascii_case(scheme))
}

/// Human label for one link in errors: scheme + path only — OAuth `?code=`
/// / `?state=` (and any token query) never reach an error string.
pub fn link_label(url: &str) -> String {
    let cut = url.find(['?', '#']).unwrap_or(url.len());
    url[..cut].to_string()
}

// ─── Android implementation (JNI, no Kotlin) ──────────────────────────

#[cfg(target_os = "android")]
mod android_impl {
    use super::{
        is_social_package, pack_icon_candidates, sort_apps, validate_package_name, AndroidApp,
        IconPack, NotificationState, StoredMessage, MESSAGES_FILE,
    };
    use jni::errors::Error as JniError;
    use jni::objects::{JObject, JValue};
    use jni::JNIEnv;
    use std::path::{Path, PathBuf};

    use base64::Engine as _;

    pub const FLAG_ACTIVITY_NEW_TASK: i32 = 0x1000_0000;
    const FLAG_SYSTEM: i32 = 1; // ApplicationInfo.FLAG_SYSTEM

    /// Flags for every `queryIntentActivities` call — `0`, exactly what
    /// `getLaunchIntentForPackage` uses internally. Deliberately NOT
    /// `MATCH_DEFAULT_ONLY` (0x0001_0000): that flag only returns activities
    /// whose intent filter declares `CATEGORY_DEFAULT`, which stock
    /// MAIN/LAUNCHER filters never do (only HOME filters do), so it would
    /// blank the Gaveta drawer. The JNI signature is two-arg
    /// `(Intent, int)` — omitting this int makes every query fail with
    /// `Invalid number or type of arguments passed to java method`.
    const QUERY_ACTIVITY_FLAGS: i32 = 0;

    fn is_null(o: &JObject) -> bool {
        o.as_raw().is_null()
    }

    /// Map a JNI failure to a String AND clear the pending Java exception —
    /// without the clear every later JNI call in this thread misbehaves.
    fn fail(env: &JNIEnv, ctx: String) -> String {
        let _ = env.exception_clear();
        ctx
    }

    fn with_env<T>(
        f: impl FnOnce(&mut JNIEnv, &JObject) -> Result<T, String>,
    ) -> Result<T, String> {
        let ctx = ndk_context::android_context();
        // Inferred cast: `from_raw` decides the pointer type, so this line
        // cannot drift from the `jni` version's expectation.
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }
            .map_err(|e| format!("network: cannot reach the Android runtime: {e}"))?;
        let mut guard = vm
            .attach_current_thread()
            .map_err(|e| format!("network: cannot attach to the Android runtime: {e}"))?;
        let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
        if is_null(&activity) {
            return Err(
                "network: Android activity is gone — retry after the app resumes".to_string(),
            );
        }
        // Explicit reborrow through the guard's `DerefMut` (AttachGuard → JNIEnv).
        let env: &mut JNIEnv = &mut *guard;
        f(env, &activity)
    }

    /// Fresh owned `jobject` for one Rust string (args take owned values —
    /// `JObject` is neither `Clone` nor `Copy`).
    fn new_obj<'a>(env: &mut JNIEnv<'a>, s: &str) -> Result<JObject<'a>, String> {
        env.new_string(s)
            .map(|js| js.into())
            .map_err(|e| fail(env, format!("invalid: bad string: {e}")))
    }

    /// Java `String` (borrowed) → Rust `String`. Undecodable labels degrade
    /// to `""` so one odd app never fails the whole list.
    fn jstr(env: &mut JNIEnv, o: &JObject) -> Result<String, String> {
        if is_null(o) {
            return Ok(String::new());
        }
        let js = <&jni::objects::JString>::from(o);
        env.get_string(js)
            .map_err(|e| fail(env, format!("integrity: cannot decode text: {e}")))
            .map(|s| s.to_str().unwrap_or("").to_owned())
    }

    fn field_str(env: &mut JNIEnv, o: &JObject, field: &str) -> Result<String, JniError> {
        let v: JObject = env.get_field(o, field, "Ljava/lang/String;")?.l()?;
        if is_null(&v) {
            return Ok(String::new());
        }
        let js = jni::objects::JString::from(v);
        let out = env.get_string(&js)?.to_str().unwrap_or("").to_owned();
        Ok(out)
    }

    fn package_manager<'a>(
        env: &mut JNIEnv<'a>,
        activity: &JObject,
    ) -> Result<JObject<'a>, String> {
        let v = env
            .call_method(
                activity,
                "getPackageManager",
                "()Landroid/content/pm/PackageManager;",
                &[],
            )
            .map_err(|e| fail(env, format!("network: cannot reach PackageManager: {e}")))?;
        let pm: JObject = v
            .l()
            .map_err(|e| fail(env, format!("network: bad PackageManager: {e}")))?;
        if is_null(&pm) {
            return Err("network: PackageManager is unavailable".to_string());
        }
        Ok(pm)
    }

    fn launcher_query<'a>(
        env: &mut JNIEnv<'a>,
        action: &str,
        category: Option<&str>,
    ) -> Result<JObject<'a>, String> {
        let a = new_obj(env, action)?;
        let intent: JObject = env
            .new_object(
                "android/content/Intent",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&a)],
            )
            .map_err(|e| fail(env, format!("invalid: cannot build intent: {e}")))?;
        if let Some(c) = category {
            let jc = new_obj(env, c)?;
            env.call_method(
                &intent,
                "addCategory",
                "(Ljava/lang/String;)Landroid/content/Intent;",
                &[JValue::Object(&jc)],
            )
            .map_err(|e| fail(env, format!("invalid: cannot add category: {e}")))?;
        }
        Ok(intent)
    }

    fn query_activities<'a>(
        env: &mut JNIEnv<'a>,
        pm: &JObject,
        intent: &JObject,
    ) -> Result<JObject<'a>, String> {
        let local: JObject = env
            .new_local_ref(intent)
            .map_err(|e| fail(env, format!("network: app query failed: {e}")))?;
        let v = env
            .call_method(
                pm,
                "queryIntentActivities",
                "(Landroid/content/Intent;I)Ljava/util/List;",
                &[JValue::Object(&local), JValue::Int(QUERY_ACTIVITY_FLAGS)],
            )
            .map_err(|e| fail(env, format!("network: app query failed: {e}")))?;
        v.l()
            .map_err(|e| fail(env, format!("network: app query failed: {e}")))
    }

    fn load_label(env: &mut JNIEnv, pm: &JObject, item: &JObject) -> String {
        let pm_local: JObject = match env.new_local_ref(pm) {
            Ok(o) => o,
            Err(_) => {
                let _ = env.exception_clear();
                return String::new();
            }
        };
        let cs: JObject = match env.call_method(
            item,
            "loadLabel",
            "(Landroid/content/pm/PackageManager;)Ljava/lang/CharSequence;",
            &[JValue::Object(&pm_local)],
        ) {
            Ok(v) => match v.l() {
                Ok(o) => o,
                Err(_) => return String::new(),
            },
            Err(_) => {
                let _ = env.exception_clear();
                return String::new();
            }
        };
        if is_null(&cs) {
            return String::new();
        }
        match env.call_method(&cs, "toString", "()Ljava/lang/String;", &[]) {
            Ok(v) => match v.l() {
                Ok(o) => jstr(env, &o).unwrap_or_default(),
                Err(_) => String::new(),
            },
            Err(_) => {
                let _ = env.exception_clear();
                String::new()
            }
        }
    }

    /// Drawable → 96px PNG `data:` URL. Icon failures never fail the row —
    /// the caller stores `""` and the frontend renders a letter tile.
    fn raster(env: &mut JNIEnv, drawable: &JObject, size: i32) -> Result<String, JniError> {
        if is_null(drawable) {
            return Err(JniError::NullPtr("null drawable"));
        }
        let cfg: JObject = env
            .get_static_field(
                "android/graphics/Bitmap$Config",
                "ARGB_8888",
                "Landroid/graphics/Bitmap$Config;",
            )?
            .l()?;
        let bmp: JObject = env
            .call_static_method(
                "android/graphics/Bitmap",
                "createBitmap",
                "(IILandroid/graphics/Bitmap$Config;)Landroid/graphics/Bitmap;",
                &[JValue::Int(size), JValue::Int(size), JValue::Object(&cfg)],
            )?
            .l()?;
        let canvas: JObject = env.new_object(
            "android/graphics/Canvas",
            "(Landroid/graphics/Bitmap;)V",
            &[JValue::Object(&env.new_local_ref(&bmp)?)],
        )?;
        env.call_method(
            drawable,
            "setBounds",
            "(IIII)V",
            &[
                JValue::Int(0),
                JValue::Int(0),
                JValue::Int(size),
                JValue::Int(size),
            ],
        )?;
        env.call_method(
            drawable,
            "draw",
            "(Landroid/graphics/Canvas;)V",
            &[JValue::Object(&canvas)],
        )?;
        let stream: JObject = env.new_object("java/io/ByteArrayOutputStream", "()V", &[])?;
        let fmt: JObject = env
            .get_static_field(
                "android/graphics/Bitmap$CompressFormat",
                "PNG",
                "Landroid/graphics/Bitmap$CompressFormat;",
            )?
            .l()?;
        let stream_local: JObject = env.new_local_ref(&stream)?;
        env.call_method(
            &bmp,
            "compress",
            "(Landroid/graphics/Bitmap$CompressFormat;ILjava/io/OutputStream;)Z",
            &[
                JValue::Object(&fmt),
                JValue::Int(100),
                JValue::Object(&stream_local),
            ],
        )?;
        let bytes_obj: JObject = env.call_method(&stream, "toByteArray", "()[B", &[])?.l()?;
        let arr = jni::objects::JByteArray::from(bytes_obj);
        let bytes = env.convert_byte_array(&arr)?;
        if bytes.is_empty() {
            return Err(JniError::NullPtr("empty icon"));
        }
        Ok(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        ))
    }

    /// One query row → app. Runs inside a `with_local_frame` (caller-owned):
    /// any `?` return keeps the Java exception pending, which the caller
    /// clears before the next row.
    fn read_app(
        env: &mut JNIEnv,
        pm: &JObject,
        item: &JObject,
    ) -> Result<Option<AndroidApp>, JniError> {
        let ai: JObject = env
            .get_field(item, "activityInfo", "Landroid/content/pm/ActivityInfo;")?
            .l()?;
        if is_null(&ai) {
            return Ok(None);
        }
        let package_name = field_str(env, &ai, "packageName")?;
        if package_name.trim().is_empty() {
            return Ok(None);
        }
        // A failing field read leaves the Java exception pending: `?` would
        // drop the whole row, while a bare `unwrap_or_default()` fails the
        // NEXT JNI call instead (and with it the row anyway). Degrade to
        // "" AND clear, which is what the default was meant to do.
        let activity_class = match field_str(env, &ai, "name") {
            Ok(name) => name,
            Err(_) => {
                let _ = env.exception_clear();
                String::new()
            }
        };
        let app_info: JObject = env
            .get_field(
                &ai,
                "applicationInfo",
                "Landroid/content/pm/ApplicationInfo;",
            )?
            .l()?;
        let flags: i32 = env.get_field(&app_info, "flags", "I")?.i()?;
        let raw_label = load_label(env, pm, item);
        let label = if raw_label.trim().is_empty() {
            package_name.clone()
        } else {
            raw_label
        };
        let pm_local: JObject = env.new_local_ref(pm)?;
        let drawable: JObject = env
            .call_method(
                &ai,
                "loadIcon",
                "(Landroid/content/pm/PackageManager;)Landroid/graphics/drawable/Drawable;",
                &[JValue::Object(&pm_local)],
            )?
            .l()?;
        // Icon failure degrades to "" (letter tile) — but the exception must
        // be cleared or the NEXT row's JNI calls silently no-op.
        let icon_base64 = raster(env, &drawable, super::ICON_PX).unwrap_or_else(|_| {
            let _ = env.exception_clear();
            String::new()
        });
        Ok(Some(AndroidApp {
            social_app: is_social_package(&package_name),
            package_name,
            label,
            activity_class,
            system_app: (flags & FLAG_SYSTEM) != 0,
            icon_base64,
        }))
    }

    /// Add `FLAG_ACTIVITY_NEW_TASK` + start. Consumes the intent.
    fn start(
        env: &mut JNIEnv,
        activity: &JObject,
        intent: JObject,
        what: &str,
    ) -> Result<(), String> {
        env.call_method(
            &intent,
            "addFlags",
            "(I)Landroid/content/Intent;",
            &[JValue::Int(FLAG_ACTIVITY_NEW_TASK)],
        )
        .map_err(|e| fail(env, format!("network: cannot flag the launch: {e}")))?;
        env.call_method(
            activity,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[JValue::Object(&intent)],
        )
        .map_err(|e| {
            fail(
                env,
                format!("not_found: Android refused to open {what}: {e}"),
            )
        })?;
        Ok(())
    }

    /// Explicit `ComponentName` fallback for packages whose
    /// `getLaunchIntentForPackage` returns null.
    fn explicit_intent<'a>(
        env: &mut JNIEnv<'a>,
        pm: &JObject,
        package: &str,
    ) -> Result<JObject<'a>, String> {
        let q = launcher_query(
            env,
            "android.intent.action.MAIN",
            Some("android.intent.category.LAUNCHER"),
        )?;
        let pkg = new_obj(env, package)?;
        env.call_method(
            &q,
            "setPackage",
            "(Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&pkg)],
        )
        .map_err(|e| fail(env, format!("not_found: cannot resolve {package}: {e}")))?;
        let list = query_activities(env, pm, &q)?;
        let size: i32 = env
            .call_method(&list, "size", "()I", &[])
            .map_err(|e| fail(env, format!("not_found: cannot resolve {package}: {e}")))?
            .i()
            .map_err(|e| fail(env, format!("not_found: cannot resolve {package}: {e}")))?;
        if size <= 0 {
            return Err(format!("not_found: {package} has no launch activity"));
        }
        let item: JObject = env
            .call_method(&list, "get", "(I)Ljava/lang/Object;", &[JValue::Int(0)])
            .map_err(|e| fail(env, format!("not_found: cannot resolve {package}: {e}")))?
            .l()
            .map_err(|e| fail(env, format!("not_found: cannot resolve {package}: {e}")))?;
        let ai: JObject = env
            .get_field(&item, "activityInfo", "Landroid/content/pm/ActivityInfo;")
            .map_err(|e| fail(env, format!("not_found: cannot resolve {package}: {e}")))?
            .l()
            .map_err(|e| fail(env, format!("not_found: cannot resolve {package}: {e}")))?;
        let cls = field_str(env, &ai, "name")
            .map_err(|e| fail(env, format!("not_found: cannot resolve {package}: {e}")))?;
        let main = new_obj(env, "android.intent.action.MAIN")?;
        let intent: JObject = env
            .new_object(
                "android/content/Intent",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&main)],
            )
            .map_err(|e| fail(env, format!("not_found: cannot resolve {package}: {e}")))?;
        let p2 = new_obj(env, package)?;
        let c2 = new_obj(env, cls.as_str())?;
        let comp: JObject = env
            .new_object(
                "android/content/ComponentName",
                "(Ljava/lang/String;Ljava/lang/String;)V",
                &[JValue::Object(&p2), JValue::Object(&c2)],
            )
            .map_err(|e| fail(env, format!("not_found: cannot resolve {package}: {e}")))?;
        env.call_method(
            &intent,
            "setComponent",
            "(Landroid/content/ComponentName;)Landroid/content/Intent;",
            &[JValue::Object(&comp)],
        )
        .map_err(|e| fail(env, format!("not_found: cannot resolve {package}: {e}")))?;
        let cat = new_obj(env, "android.intent.category.LAUNCHER")?;
        env.call_method(
            &intent,
            "addCategory",
            "(Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&cat)],
        )
        .map_err(|e| fail(env, format!("not_found: cannot resolve {package}: {e}")))?;
        Ok(intent)
    }

    fn files_dir(env: &mut JNIEnv, activity: &JObject) -> Result<PathBuf, String> {
        let dir: JObject = env
            .call_method(activity, "getFilesDir", "()Ljava/io/File;", &[])
            .map_err(|e| fail(env, format!("network: app files dir is unreachable: {e}")))?
            .l()
            .map_err(|e| fail(env, format!("network: app files dir is unreachable: {e}")))?;
        let p: JObject = env
            .call_method(&dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
            .map_err(|e| fail(env, format!("network: app files dir is unreachable: {e}")))?
            .l()
            .map_err(|e| fail(env, format!("network: app files dir is unreachable: {e}")))?;
        Ok(PathBuf::from(jstr(env, &p)?))
    }

    fn own_package(env: &mut JNIEnv, activity: &JObject) -> Result<String, String> {
        let v: JObject = env
            .call_method(activity, "getPackageName", "()Ljava/lang/String;", &[])
            .map_err(|e| fail(env, format!("network: cannot read our own package: {e}")))?
            .l()
            .map_err(|e| fail(env, format!("network: cannot read our own package: {e}")))?;
        jstr(env, &v)
    }

    pub fn list_apps() -> Result<Vec<AndroidApp>, String> {
        with_env(|env, activity| {
            let pm = package_manager(env, activity)?;
            let intent = launcher_query(
                env,
                "android.intent.action.MAIN",
                Some("android.intent.category.LAUNCHER"),
            )?;
            let list = query_activities(env, &pm, &intent)?;
            let size: i32 = env
                .call_method(&list, "size", "()I", &[])
                .map_err(|e| fail(env, format!("network: app query failed: {e}")))?
                .i()
                .map_err(|e| fail(env, format!("network: app query failed: {e}")))?;
            let mut apps = Vec::new();
            for i in 0..size {
                let item: JObject =
                    match env.call_method(&list, "get", "(I)Ljava/lang/Object;", &[JValue::Int(i)])
                    {
                        Ok(v) => match v.l() {
                            Ok(o) => o,
                            Err(_) => continue,
                        },
                        Err(_) => {
                            let _ = env.exception_clear();
                            continue;
                        }
                    };
                // One row = one local frame: ~10 refs per app would overflow
                // the 512-entry table on a 300-app drawer otherwise.
                let row: Result<Option<AndroidApp>, String> = env
                    .with_local_frame(48, |f| read_app(f, &pm, &item))
                    .map_err(|e| {
                        let _ = env.exception_clear();
                        format!("network: app row failed: {e}")
                    });
                // `get`'s ref belongs to the OUTER frame (JObject has no
                // Drop) — reclaim it every row or ~500 launchable apps
                // overflow ART's 512-entry local-reference table and abort
                // the VM. The per-row frame only frees what `read_app`
                // allocates inside itself.
                let _ = env.delete_local_ref(item);
                match row {
                    Ok(Some(app)) => apps.push(app),
                    Ok(None) => {}
                    Err(_) => {
                        // One bad row never fails the list; the frame already
                        // freed its refs and the exception is cleared above.
                    }
                }
            }
            Ok(sort_apps(apps))
        })
    }

    pub fn list_social_apps() -> Result<Vec<AndroidApp>, String> {
        Ok(list_apps()?.into_iter().filter(|a| a.social_app).collect())
    }

    pub fn open_app(package: &str) -> Result<(), String> {
        validate_package_name(package)?;
        let package = package.trim().to_string();
        with_env(|env, activity| {
            let pm = package_manager(env, activity)?;
            let probe = new_obj(env, package.as_str())?;
            let direct: Option<JObject> = match env.call_method(
                &pm,
                "getLaunchIntentForPackage",
                "(Ljava/lang/String;)Landroid/content/Intent;",
                &[JValue::Object(&probe)],
            ) {
                Ok(v) => match v.l() {
                    Ok(o) => {
                        if is_null(&o) {
                            None
                        } else {
                            Some(o)
                        }
                    }
                    Err(_) => None,
                },
                Err(_) => {
                    let _ = env.exception_clear();
                    None
                }
            };
            let intent = direct.map_or_else(|| explicit_intent(env, &pm, package.as_str()), Ok)?;
            start(env, activity, intent, package.as_str())
        })
    }

    pub fn uninstall_app(package: &str) -> Result<(), String> {
        validate_package_name(package)?;
        let package = package.trim().to_string();
        with_env(|env, activity| {
            let action = new_obj(env, "android.intent.action.DELETE")?;
            let intent: JObject = env
                .new_object(
                    "android/content/Intent",
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(&action)],
                )
                .map_err(|e| fail(env, format!("invalid: cannot build intent: {e}")))?;
            let spec = new_obj(env, format!("package:{package}").as_str())?;
            let uri: JObject = env
                .call_static_method(
                    "android/net/Uri",
                    "parse",
                    "(Ljava/lang/String;)Landroid/net/Uri;",
                    &[JValue::Object(&spec)],
                )
                .map_err(|e| fail(env, format!("invalid: bad package uri: {e}")))?
                .l()
                .map_err(|e| fail(env, format!("invalid: bad package uri: {e}")))?;
            env.call_method(
                &intent,
                "setData",
                "(Landroid/net/Uri;)Landroid/content/Intent;",
                &[JValue::Object(&uri)],
            )
            .map_err(|e| fail(env, format!("invalid: cannot set data: {e}")))?;
            start(
                env,
                activity,
                intent,
                format!("uninstall {package}").as_str(),
            )
        })
    }

    pub fn list_icon_packs() -> Result<Vec<IconPack>, String> {
        with_env(|env, activity| {
            let pm = package_manager(env, activity)?;
            let mut packs: Vec<IconPack> = Vec::new();
            for action in super::ICON_PACK_ACTIONS {
                let intent = match launcher_query(env, action, None) {
                    Ok(i) => i,
                    Err(_) => continue,
                };
                let list = match query_activities(env, &pm, &intent) {
                    Ok(l) => l,
                    Err(_) => continue,
                };
                let size: i32 = match env.call_method(&list, "size", "()I", &[]) {
                    Ok(v) => match v.i() {
                        Ok(n) => n,
                        Err(_) => {
                            let _ = env.exception_clear();
                            continue;
                        }
                    },
                    Err(_) => {
                        let _ = env.exception_clear();
                        continue;
                    }
                };
                for i in 0..size {
                    let row: Option<(String, String)> = env
                        .with_local_frame(24, |f| -> Result<Option<(String, String)>, JniError> {
                            let item: JObject = f
                                .call_method(
                                    &list,
                                    "get",
                                    "(I)Ljava/lang/Object;",
                                    &[JValue::Int(i)],
                                )?
                                .l()?;
                            let ai: JObject = f
                                .get_field(
                                    &item,
                                    "activityInfo",
                                    "Landroid/content/pm/ActivityInfo;",
                                )?
                                .l()?;
                            let pkg = field_str(f, &ai, "packageName")?;
                            if pkg.trim().is_empty() {
                                return Ok(None);
                            }
                            let mut label = load_label(f, &pm, &item);
                            if label.trim().is_empty() {
                                label = pkg.clone();
                            }
                            Ok(Some((pkg, label)))
                        })
                        .map_err(|_| {
                            let _ = env.exception_clear();
                        })
                        .unwrap_or(None);
                    if let Some((package_name, label)) = row {
                        if !packs.iter().any(|p| p.package_name == package_name) {
                            packs.push(IconPack {
                                package_name,
                                label,
                            });
                        }
                    }
                }
            }
            packs.sort_by_key(|a| a.label.to_lowercase());
            Ok(packs)
        })
    }

    pub fn pack_icon(pack: &str, package: &str) -> Result<String, String> {
        validate_package_name(pack).map_err(|e| e.replace("packageName", "icon pack"))?;
        validate_package_name(package)?;
        let (pack, package) = (pack.trim().to_string(), package.trim().to_string());
        with_env(|env, activity| {
            let pm = package_manager(env, activity)?;
            let jpack = new_obj(env, pack.as_str())?;
            let res: JObject = env
                .call_method(
                    &pm,
                    "getResourcesForApplication",
                    "(Ljava/lang/String;)Landroid/content/res/Resources;",
                    &[JValue::Object(&jpack)],
                )
                .map_err(|e| {
                    fail(
                        env,
                        format!("not_found: icon pack {pack} has no resources: {e}"),
                    )
                })?
                .l()
                .map_err(|e| {
                    fail(
                        env,
                        format!("not_found: icon pack {pack} has no resources: {e}"),
                    )
                })?;
            for candidate in pack_icon_candidates(package.as_str()) {
                let found: Option<String> = env
                    .with_local_frame(24, |f| -> Result<Option<String>, JniError> {
                        let jname: JObject = f.new_string(candidate.as_str())?.into();
                        let jdef: JObject = f.new_string("drawable")?.into();
                        let jpkg: JObject = f.new_string(pack.as_str())?.into();
                        let ident: i32 = f
                            .call_method(
                                &res,
                                "getIdentifier",
                                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I",
                                &[
                                    JValue::Object(&jname),
                                    JValue::Object(&jdef),
                                    JValue::Object(&jpkg),
                                ],
                            )?
                            .i()?;
                        if ident == 0 {
                            return Ok(None);
                        }
                        let drawable: JObject = f
                            .call_method(
                                &res,
                                "getDrawable",
                                "(I)Landroid/graphics/drawable/Drawable;",
                                &[JValue::Int(ident)],
                            )?
                            .l()?;
                        if drawable.as_raw().is_null() {
                            return Ok(None);
                        }
                        // A raster failure leaves a Java exception pending —
                        // clear it HERE. `.ok()` would pop the frame with the
                        // exception live whenever the LAST candidate fails
                        // (the outer map_err only runs when the closure
                        // itself errs), poisoning the next JNI user of this
                        // tokio thread. Same contract read_app honors.
                        let icon = raster(f, &drawable, super::ICON_PX);
                        match icon {
                            Ok(url) => Ok(Some(url)),
                            Err(_) => {
                                let _ = f.exception_clear();
                                Ok(None)
                            }
                        }
                    })
                    .map_err(|_| {
                        let _ = env.exception_clear();
                    })
                    .unwrap_or(None);
                if let Some(url) = found {
                    return Ok(url);
                }
            }
            Err(format!("not_found: {pack} has no icon for {package}"))
        })
    }

    // ─── Messages (notification mirror) ──────────────────────────

    fn messages_path(env: &mut JNIEnv, activity: &JObject) -> Result<PathBuf, String> {
        Ok(files_dir(env, activity)?.join(MESSAGES_FILE))
    }

    fn read_messages_file(path: &PathBuf) -> Result<Vec<StoredMessage>, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|_| "not_found: no stored messages yet — enable notification access and wait for a chat or mail".to_string())?;
        let v: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| format!("integrity: stored messages are corrupt: {e}"))?;
        let mut out = Vec::new();
        if let Some(arr) = v.as_array() {
            for e in arr {
                let s = |k: &str| e.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                out.push(StoredMessage {
                    key: s("key"),
                    package_name: s("packageName"),
                    app_label: s("appLabel"),
                    title: s("title"),
                    text: s("text"),
                    timestamp: e.get("timestamp").and_then(|x| x.as_i64()).unwrap_or(0),
                    category: s("category"),
                });
            }
        }
        out.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(out)
    }

    pub fn list_messages(limit: usize) -> Result<Vec<StoredMessage>, String> {
        let cap = limit.max(1).min(500);
        with_env(|env, activity| {
            let path = messages_path(env, activity)?;
            let mut out = read_messages_file(&path)?;
            out.truncate(cap);
            Ok(out)
        })
    }

    pub fn clear_messages() -> Result<(), String> {
        with_env(|env, activity| {
            let path = messages_path(env, activity)?;
            std::fs::write(&path, "[]")
                .map_err(|e| format!("disk_full: cannot clear stored messages: {e}"))?;
            // Best-effort: also dismiss the live shade notifications through
            // the running listener service (explicit intent → onStartCommand
            // → cancelAllNotifications). The file is already cleared, so a
            // missing/old service never fails the hub button.
            dismiss_live(env, activity);
            Ok(())
        })
    }

    /// Fire `ACTION_CLEAR` at our own listener service. Never throws — the
    /// caller already fulfilled the hub request by clearing the file.
    fn dismiss_live(env: &mut JNIEnv, activity: &JObject) {
        let res: Result<(), String> = (|| {
            let own = own_package(env, activity)?;
            let action = new_obj(env, "com.cybermanju.os.action.CLEAR_NOTIFICATIONS")?;
            let intent: JObject = env
                .new_object(
                    "android/content/Intent",
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(&action)],
                )
                .map_err(|e| {
                    fail(
                        env,
                        format!("unsupported: cannot build the clear-notification intent: {e}"),
                    )
                })?;
            let p = new_obj(env, own.as_str())?;
            let c = new_obj(env, format!("{own}.CybermanjuMessages").as_str())?;
            let comp: JObject = env
                .new_object(
                    "android/content/ComponentName",
                    "(Ljava/lang/String;Ljava/lang/String;)V",
                    &[JValue::Object(&p), JValue::Object(&c)],
                )
                .map_err(|e| {
                    fail(
                        env,
                        format!("unsupported: cannot target the messages service: {e}"),
                    )
                })?;
            env.call_method(
                &intent,
                "setComponent",
                "(Landroid/content/ComponentName;)Landroid/content/Intent;",
                &[JValue::Object(&comp)],
            )
            .map_err(|e| {
                fail(
                    env,
                    format!("unsupported: cannot target the messages service: {e}"),
                )
            })?;
            // The ComponentName return is irrelevant (null when the service
            // is absent) — never let it throw.
            match env.call_method(
                activity,
                "startService",
                "(Landroid/content/Intent;)Landroid/content/ComponentName;",
                &[JValue::Object(&intent)],
            ) {
                Ok(_) => Ok(()),
                Err(_) => {
                    let _ = env.exception_clear();
                    Ok(())
                }
            }
        })();
        if let Err(e) = res {
            eprintln!("CyberManju OS: live notification dismiss skipped ({e})");
        }
    }

    pub fn notification_state() -> Result<NotificationState, String> {
        with_env(|env, activity| {
            // Degrades to disabled (never throws): the hub then shows the
            // enable prompt, which is the honest state anyway.
            let own = own_package(env, activity).unwrap_or_default();
            let cr: JObject = env
                .call_method(
                    activity,
                    "getContentResolver",
                    "()Landroid/content/ContentResolver;",
                    &[],
                )
                .map_err(|e| fail(env, format!("network: cannot reach app settings: {e}")))?
                .l()
                .map_err(|e| fail(env, format!("network: cannot reach app settings: {e}")))?;
            let name = new_obj(env, "enabled_notification_listeners")?;
            let v: JObject = env
                .call_static_method(
                    "android/provider/Settings$Secure",
                    "getString",
                    "(Landroid/content/ContentResolver;Ljava/lang/String;)Ljava/lang/String;",
                    &[JValue::Object(&cr), JValue::Object(&name)],
                )
                .map_err(|e| fail(env, format!("network: cannot read listener state: {e}")))?
                .l()
                .map_err(|e| fail(env, format!("network: cannot read listener state: {e}")))?;
            let flat = jstr(env, &v)?;
            let enabled = !own.is_empty() && flat.split(':').any(|c| c.contains(own.as_str()));
            Ok(NotificationState {
                enabled,
                detail: if enabled {
                    "notification access is on — chats and mails are being stored".to_string()
                } else {
                    "notification access is off — open settings and enable CyberManju OS"
                        .to_string()
                },
            })
        })
    }

    pub fn open_notification_settings() -> Result<(), String> {
        with_env(|env, activity| {
            let action = new_obj(
                env,
                "android.settings.ACTION_NOTIFICATION_LISTENER_SETTINGS",
            )?;
            let intent: JObject = env
                .new_object(
                    "android/content/Intent",
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(&action)],
                )
                .map_err(|e| fail(env, format!("unsupported: cannot open settings: {e}")))?;
            start(env, activity, intent, "notification settings")
        })
    }

    /// `Intent.FLAG_GRANT_READ_URI_PERMISSION` — one hand-off, one file.
    const GRANT_READ_URI_PERMISSION: i32 = 0x0000_0001;

    /// `Intent.ACTION_VIEW` whose data is one already-built URI.
    fn view_intent<'a>(env: &mut JNIEnv<'a>, uri: JObject<'a>) -> Result<JObject<'a>, String> {
        let action = new_obj(env, "android.intent.action.VIEW")?;
        let intent: JObject = env
            .new_object(
                "android/content/Intent",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&action)],
            )
            .map_err(|e| fail(env, format!("invalid: cannot build intent: {e}")))?;
        env.call_method(
            &intent,
            "setData",
            "(Landroid/net/Uri;)Landroid/content/Intent;",
            &[JValue::Object(&uri)],
        )
        .map_err(|e| fail(env, format!("invalid: cannot set intent data: {e}")))?;
        Ok(intent)
    }

    /// `Intent.ACTION_VIEW` whose data is one string URI (`Uri.parse`).
    fn view_uri<'a>(env: &mut JNIEnv<'a>, data: &str) -> Result<JObject<'a>, String> {
        let spec = new_obj(env, data)?;
        let uri: JObject = env
            .call_static_method(
                "android/net/Uri",
                "parse",
                "(Ljava/lang/String;)Landroid/net/Uri;",
                &[JValue::Object(&spec)],
            )
            .map_err(|e| fail(env, format!("invalid: bad uri: {e}")))?
            .l()
            .map_err(|e| fail(env, format!("invalid: bad uri: {e}")))?;
        view_intent(env, uri)
    }

    /// `content://` URI for one absolute path, minted by our FileProvider.
    ///
    /// The authority is read from the Activity (`<applicationId>.fileprovider`)
    /// so debug application-id suffixes stay in sync with the manifest entry
    /// `scripts/android-configure.sh` writes.
    fn file_uri<'a>(
        env: &mut JNIEnv<'a>,
        activity: &JObject,
        path: &str,
    ) -> Result<JObject<'a>, String> {
        let pkg_obj: JObject = env
            .call_method(activity, "getPackageName", "()Ljava/lang/String;", &[])
            .map_err(|e| fail(env, format!("network: cannot read the app id: {e}")))?
            .l()
            .map_err(|e| fail(env, format!("network: cannot read the app id: {e}")))?;
        let pkg = jstr(env, &pkg_obj)?;
        let authority = format!("{pkg}.fileprovider");
        let authority_obj = new_obj(env, authority.as_str())?;
        let file_obj = new_obj(env, path)?;
        let java_file: JObject = env
            .new_object(
                "java/io/File",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&file_obj)],
            )
            .map_err(|e| fail(env, format!("invalid: cannot address the file: {e}")))?;
        env.call_static_method(
            "androidx/core/content/FileProvider",
            "getUriForFile",
            "(Landroid/content/Context;Ljava/lang/String;Ljava/io/File;)Landroid/net/Uri;",
            &[
                JValue::Object(activity),
                JValue::Object(&authority_obj),
                JValue::Object(&java_file),
            ],
        )
        .map_err(|e| fail(env, share_fail(path, e)))?
        .l()
        .map_err(|e| fail(env, share_fail(path, e)))
    }

    /// Honest FileProvider failure (path outside the configured roots,
    /// `androidx.core` missing, …) — never a bare JNI code in the UI.
    /// The pending Java exception is cleared by `fail` before this surfaces.
    fn share_fail(path: &str, e: JniError) -> String {
        format!("not_found: cannot hand {path} to another app ({e}) — check cybermanju_paths.xml")
    }

    /// Open one link with the system resolver (`Intent.ACTION_VIEW`).
    ///
    /// This is Android's answer to `plugin-shell open`, which spawns
    /// `xdg-open` and fails on Android with `Scoped shell IO error: No such
    /// file or directory (os error 2)` — a WebView has no browser opener of
    /// its own, so the Activity intent is the only way out (the same route
    /// the CLI takes through the Activity Manager).
    pub fn open_url(url: &str) -> Result<(), String> {
        let url = url.trim();
        let label = super::link_label(url);
        if !super::is_openable_url(url) {
            return Err(format!(
                "invalid: cannot open {label} (allowed: https, http, mailto, tel, cybermanju)"
            ));
        }
        with_env(|env, activity| {
            let intent = view_uri(env, url)?;
            start(env, activity, intent, label.as_str())
        })
    }

    /// App-private files-dir roots (canonicalized when they exist). The
    /// vault's secrets live here — `master.passphrase`, `cybermanju.db`,
    /// `ipc_hmac_secret`, the OAuth side-store — so `open_file` may only
    /// hand `.cyb3` vault payloads out of them. Shared-storage vaults are
    /// untouched by this rule.
    fn app_private_roots() -> Vec<PathBuf> {
        let mut raw: Vec<PathBuf> = Vec::new();
        for key in ["CYBERMANJU_DATA_DIR", "CYBERMANJU_ANDROID_FILES_DIR"] {
            if let Ok(d) = std::env::var(key) {
                if !d.trim().is_empty() {
                    raw.push(PathBuf::from(d));
                }
            }
        }
        // Release package path. Debug applicationIdSuffix builds never use
        // this entry — their dir arrives via CYBERMANJU_DATA_DIR above.
        raw.push(PathBuf::from("/data/data/com.cybermanju.os/files"));
        if let Ok(entries) = std::fs::read_dir("/data/user") {
            let mut ids: Vec<String> = entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.is_empty() && n.as_bytes().iter().all(u8::is_ascii_digit))
                .collect();
            ids.sort();
            for id in ids {
                raw.push(PathBuf::from(format!(
                    "/data/user/{id}/com.cybermanju.os/files"
                )));
            }
        }
        // `/data/data` symlinks to `/data/user/0/…` — canonicalize so both
        // spellings of one dir compare equal and `..` cannot slip past.
        raw.into_iter()
            .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
            .collect()
    }

    /// True when the canonical path sits inside an app-private files dir.
    fn in_app_private(canon: &Path) -> bool {
        app_private_roots()
            .iter()
            .any(|root| canon.starts_with(root))
    }

    /// Hand one local file to a viewer/editor (`ACTION_VIEW` over a
    /// `content://` URI from our FileProvider).
    ///
    /// `file://` data is refused by Android 7+ (`FileUriExposedException`),
    /// so bytes travel through `androidx.core.content.FileProvider`
    /// (manifest `<provider>` + `res/xml/cybermanju_paths.xml` + the
    /// `androidx.core` dependency, all injected by
    /// `scripts/android-configure.sh`) with `FLAG_GRANT_READ_URI_PERMISSION`
    /// for this single hand-off.
    pub fn open_file(path: &str) -> Result<(), String> {
        let path = path.trim();
        let meta =
            std::fs::metadata(path).map_err(|e| format!("not_found: cannot open {path}: {e}"))?;
        if meta.is_dir() {
            return Err(format!(
                "unsupported: {path} is a folder — Android hands files, not folders, to other apps"
            ));
        }
        // Defense in depth: the app-private dir holds every secret the vault
        // owns, so a compromised WebView must not turn ACTION_VIEW into an
        // exfil channel. Extension is checked on the CANONICAL path — a
        // `secret.cyb3` symlink at the secret does not open it.
        let canon = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
        let is_vault = canon
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("cyb3"));
        if in_app_private(&canon) && !is_vault {
            return Err(format!(
                "invalid: refusing to share app-private {path} (.cyb3 vaults only)"
            ));
        }
        with_env(|env, activity| {
            let uri = file_uri(env, activity, path)?;
            let intent = view_intent(env, uri)?;
            env.call_method(
                &intent,
                "addFlags",
                "(I)Landroid/content/Intent;",
                &[JValue::Int(GRANT_READ_URI_PERMISSION)],
            )
            .map_err(|e| fail(env, format!("invalid: cannot grant read access: {e}")))?;
            start(env, activity, intent, path)
        })
    }
}

#[cfg(target_os = "android")]
pub use android_impl::{
    clear_messages, list_apps, list_icon_packs, list_messages, list_social_apps,
    notification_state, open_app, open_file, open_notification_settings, open_url, pack_icon,
    uninstall_app,
};

// ─── Non-Android: honest refusal, never mock rows ─────────────────────

#[cfg(not(target_os = "android"))]
pub fn list_apps() -> Result<Vec<AndroidApp>, String> {
    Err("unsupported: Android app listing needs the native Android build — open this vault in the Android app".to_string())
}

#[cfg(not(target_os = "android"))]
pub fn list_social_apps() -> Result<Vec<AndroidApp>, String> {
    Err("unsupported: Android app listing needs the native Android build — open this vault in the Android app".to_string())
}

#[cfg(not(target_os = "android"))]
pub fn open_app(_package: &str) -> Result<(), String> {
    Err("unsupported: opening Android apps needs the native Android build".to_string())
}

#[cfg(not(target_os = "android"))]
pub fn uninstall_app(_package: &str) -> Result<(), String> {
    Err("unsupported: uninstalling Android apps needs the native Android build".to_string())
}

#[cfg(not(target_os = "android"))]
pub fn list_icon_packs() -> Result<Vec<IconPack>, String> {
    Err("unsupported: icon packs need the native Android build".to_string())
}

#[cfg(not(target_os = "android"))]
pub fn pack_icon(_pack: &str, _package: &str) -> Result<String, String> {
    Err("unsupported: icon packs need the native Android build".to_string())
}

#[cfg(not(target_os = "android"))]
pub fn list_messages(_limit: usize) -> Result<Vec<StoredMessage>, String> {
    Err(
        "unsupported: stored messages need the native Android build with notification access"
            .to_string(),
    )
}

#[cfg(not(target_os = "android"))]
pub fn clear_messages() -> Result<(), String> {
    Err("unsupported: stored messages need the native Android build".to_string())
}

#[cfg(not(target_os = "android"))]
pub fn notification_state() -> Result<NotificationState, String> {
    Err("unsupported: notification access needs the native Android build".to_string())
}

#[cfg(not(target_os = "android"))]
pub fn open_notification_settings() -> Result<(), String> {
    Err("unsupported: notification settings need the native Android build".to_string())
}

/// The Intent link bridge is Android-only: desktop, REST, Docker and WASM
/// builds hand links to the system opener in the frontend instead
/// (`src/utils/openExternal.ts` routes on `isAndroidApp()`).
#[cfg(not(target_os = "android"))]
pub fn open_url(_url: &str) -> Result<(), String> {
    Err("unsupported: the Intent link bridge needs the native Android build".to_string())
}

/// Same honest refusal for `OPEN WITH SYSTEM` outside Android.
#[cfg(not(target_os = "android"))]
pub fn open_file(_path: &str) -> Result<(), String> {
    Err("unsupported: sharing files with other apps needs the native Android build".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(pkg: &str, label: &str) -> AndroidApp {
        AndroidApp {
            package_name: pkg.to_string(),
            label: label.to_string(),
            activity_class: format!("{pkg}.Main"),
            system_app: false,
            social_app: false,
            icon_base64: String::new(),
        }
    }

    #[test]
    fn sorts_case_insensitively_then_package() {
        let apps = sort_apps(vec![
            app("c.z", "telegram"),
            app("a.y", "Browser"),
            app("b.x", "browser"),
        ]);
        assert_eq!(apps[0].package_name, "a.y");
        assert_eq!(apps[1].package_name, "b.x");
        assert_eq!(apps[2].package_name, "c.z");
    }

    #[test]
    fn rejects_bad_package_names() {
        assert!(validate_package_name("com.example.app").is_ok());
        assert!(validate_package_name("")
            .unwrap_err()
            .starts_with("invalid:"));
        assert!(validate_package_name("1bad.name")
            .unwrap_err()
            .starts_with("invalid:"));
        assert!(validate_package_name("has space/x")
            .unwrap_err()
            .starts_with("invalid:"));
    }

    #[test]
    fn pack_candidates_cover_common_shapes() {
        let c = pack_icon_candidates("com.example.App-Name");
        assert!(c.contains(&"com_example_app_name".to_string()));
        assert!(c.contains(&"com_example_app_name_icon".to_string()));
    }

    #[test]
    fn flags_mail_chat_social_packages() {
        // Exact packages: mail, chat, social.
        for pkg in [
            "com.whatsapp",
            "com.whatsapp.w4b",
            "org.telegram.messenger",
            "org.signal.private.messenger",
            "com.instagram.android",
            "com.instagram.barcelona",
            "com.facebook.orca",
            "com.facebook.katana",
            "com.google.android.gm",
            "com.microsoft.office.outlook",
            "eu.faircode.email",
            "com.fsck.k9",
            "com.discord",
            "com.twitter.android",
            "com.snapchat.android",
            "com.google.android.apps.messaging",
            "jp.naver.line.android",
            "com.tencent.mm",
            "com.kakao.talk",
            "xyz.blueskyweb.app",
        ] {
            assert!(is_social_package(pkg), "{pkg} should be social");
        }
        // Long-token contains matches.
        assert!(is_social_package("com.somevendor.whatsappplus"));
        assert!(is_social_package("org.example.telegramx"));
        // Short tokens only on dot-segments (`line` must not fire in `offline`).
        assert!(!is_social_package("com.airline.tickets"));
        assert!(!is_social_package("com.example.offline.reader"));
        assert!(is_social_package("com.vendor.line.tools"));
        assert!(!is_social_package("com.cybermanju.os"));
        assert!(!is_social_package("com.android.chrome"));
        assert!(!is_social_package(""));
    }

    #[test]
    fn opens_only_allowlisted_link_schemes() {
        for ok in [
            "https://accounts.google.com/o/oauth2/v2/auth?x=1",
            "HTTP://example.com/a",
            "mailto:a@b.c",
            "tel:+15551234",
            "cybermanju://oauth/callback?code=s",
        ] {
            assert!(is_openable_url(ok), "{ok}");
        }
        for bad in [
            "",
            "   ",
            "https://",
            "javascript:alert(1)",
            "intent://scan/#Intent;scheme=zxing;end",
            "file:///etc/passwd",
            "data:text/html,<b>x</b>",
            "ftp://example.com",
        ] {
            assert!(!is_openable_url(bad), "{bad}");
        }
    }

    #[test]
    fn link_labels_never_carry_oauth_secrets() {
        assert_eq!(
            link_label("cybermanju://oauth/callback?code=SECRET&state=ST"),
            "cybermanju://oauth/callback"
        );
        assert_eq!(link_label("https://x.dev/a#frag"), "https://x.dev/a");
        assert_eq!(link_label("mailto:a@b.c"), "mailto:a@b.c");
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn non_android_refuses_honestly() {
        for err in [
            list_apps().unwrap_err(),
            list_social_apps().unwrap_err(),
            list_messages(10).unwrap_err(),
            notification_state().unwrap_err(),
            open_url("https://example.com").unwrap_err(),
            open_file("/tmp/cybermanju").unwrap_err(),
        ] {
            assert!(err.starts_with("unsupported:"), "{err}");
        }
    }
}
