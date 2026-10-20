#!/usr/bin/env bash
# Android post-init configuration for CyberManju OS.
#
# Applies every in-tree Android setting that `tauri android init` does NOT
# generate, so CI never relies on Tauri template defaults silently:
#
#   0. launcher icons — NOT here: `scripts/android-icons.sh` (`tauri icon`
#      from `public/icon-512.png` into `gen/…/res/mipmap-*`; the template's
#      DEFAULT icons otherwise ship in every APK). Run it BEFORE this script.
#
#   1. versionCode audit — `tauri.conf.json > bundle.android.versionCode` must
#      equal `major*1000000 + minor*1000 + patch` from `package.json`
#      (Tauri auto-derives the same formula when the key is absent; pinning
#      it in-tree makes upgrades auditable and Play-compatible).
#      0.1.1 -> 1001. Bump alongside the release (check-version.sh enforces).
#   2. AndroidManifest patch — INTERNET + optional hardware permissions,
#      `usesCleartextTraffic` (LAN dashboard is plain http), explicit
#      `allowBackup=false` (fail-closed: reinstall wipes the vault; manual
#      export is the backup path — see docs/ANDROID.md), `.cyb3`
#      file-association + `cybermanju://` deep link.
#   2b. Launcher bridge (`cybermanju-launcher` crate, Gaveta de Apps):
#      `<queries>` for ACTION_MAIN/CATEGORY_LAUNCHER so
#      `queryIntentActivities` sees every launchable app on Android 11+
#      WITHOUT the Play-restricted QUERY_ALL_PACKAGES permission, plus a
#      HOME/DEFAULT intent-filter so the app can be picked as the device
#      Home (digital-OS mode). Both are idempotent marker-guarded.
#   2c. Messages hub (top-right icon): a `NotificationListenerService`
#      (`CybermanjuMessages.kt`, written by §4 below) mirrors mail/chat/
#      social notifications into `cybermanju-messages.json` in the
#      app-private files dir (ring buffer, 200 entries); the Rust crate only
#      reads that file. Manifest gets the `<service>` declaration plus
#      explicit `<package>` queries for the top social apps (visibility for
#      labels/icons on Android 11+). The user still grants notification
#      access in system settings (runtime, per-device).
#   2f. FileProvider (OPEN WITH SYSTEM → `launcher_open_file`): manifest
#      `<provider>` + androidx.core in build.gradle.kts (§5) + res/xml
#      cybermanju_paths.xml roots (§6), so files open via content:// grants
#      instead of the file:// URIs Android 7+ refuses.
#   3. ABI / artifact notes — arm64-v8a is the only shipped ABI (ort-sys
#      prebuilts); x86_64 is emulator-smoke only. AAB is built alongside APK
#      (`--apk --aab --split-per-abi`, see CI).
#
# Usage: bash scripts/android-configure.sh [gen/android-dir]
# Idempotent: every patch is guarded by a marker comment.
set -euo pipefail

cd "$(dirname "$0")/.."

ANDROID_DIR="${1:-src-tauri/gen/android}"
MANIFEST="$ANDROID_DIR/app/src/main/AndroidManifest.xml"
MARK="CyberManju OS (scripts/android-configure.sh)"

if [ ! -d "$ANDROID_DIR" ]; then
  echo "::error::$ANDROID_DIR not found — run \`tauri android init\` first" >&2
  exit 1
fi

# ─── 1. versionCode audit ──────────────────────────────────────────
version="$(node -p "require('./package.json').version")"
# package.json may carry a semver prerelease/build suffix (0.2.0-rc.1) —
# computing versionCode from the raw suffix yields a cryptic bash
# `invalid arithmetic operator` ("patch" = "0-rc.1"). Validate, then strip.
if ! printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'; then
  echo "::error::version: '$version' is not a plain X.Y.Z version (optionally suffixed -pre/+build)" >&2
  exit 1
fi
core="${version%%-*}"
core="${core%%+*}"
major="${core%%.*}"
rest="${core#*.}"
minor="${rest%%.*}"
patch="${rest#*.}"
expected=$((major * 1000000 + minor * 1000 + patch))
pinned="$(node -p "require('./src-tauri/tauri.conf.json').bundle?.android?.versionCode ?? 'unpinned'" 2>/dev/null || echo unpinned)"
if [ "$pinned" != "unpinned" ] && [ "$pinned" != "$expected" ]; then
  echo "::error::tauri.conf.json bundle.android.versionCode=$pinned but package.json $version needs $expected (major*1000000+minor*1000+patch)" >&2
  exit 1
fi
echo "versionCode ok: package.json $version -> $expected (pinned: $pinned)"

# ─── 2. AndroidManifest patch ──────────────────────────────────────
if [ ! -f "$MANIFEST" ]; then
  echo "::error::$MANIFEST not found — unexpected Tauri template layout" >&2
  exit 1
fi

if grep -q "$MARK" "$MANIFEST"; then
  echo "AndroidManifest marker present — §2a–§2c already applied; re-checking §2d–§2f"
else
  echo "patching AndroidManifest ($MANIFEST)"
fi

# Section-aware marker guard: the global $MARK only gates the sections that
# are NOT idempotent on their own (§2a permissions, §2b application attrs,
# §2c intents). §2d–§2f carry their own content-aware presence checks below,
# so a manifest stamped by an OLDER script version still receives newly-added
# sections (e.g. §2f's FileProvider <provider>) instead of being skipped
# wholesale — a marker-only skip ships the androidx.core dependency without
# the manifest <provider> entry and OPEN WITH SYSTEM then dies with
# FileProvider.getUriForFile's IllegalArgumentException. Every insert is
# individually idempotent; the file is rewritten only when something changed.
python3 - "$MANIFEST" "$MARK" <<'PYEOF'
import re
import sys
import xml.dom.minidom

path, mark = sys.argv[1], sys.argv[2]
src = open(path).read()
original = src
anchor = "<application"

if mark in src:
    # 2a–2c insert unconditionally — the marker means a previous run already
    # applied them (§2d–§2f below still re-check their own content).
    print("marker present: §2a–§2c already applied")
else:
    # 2a. Permissions (before <application>): INTERNET is required for the LAN
    # REST transport / OAuth; location/camera/mic are optional hardware features
    # (GPS MapView, voice input) — declared here so the WebView permission
    # bridge and the Permissions-Policy override (docs/ANDROID.md) have manifest
    # backing. Scoped storage is the default: no MANAGE_EXTERNAL_STORAGE, the
    # app only writes its app-private files dir.
    perms = """
        <!-- %s: network + optional hardware (MapView GPS, voice). No
             MANAGE_EXTERNAL_STORAGE by design — app-private files dir only. -->
        <uses-permission android:name="android.permission.INTERNET" />
        <uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" />
        <uses-permission android:name="android.permission.ACCESS_FINE_LOCATION" />
        <uses-permission android:name="android.permission.ACCESS_COARSE_LOCATION" />
        <uses-permission android:name="android.permission.CAMERA" />
        <uses-permission android:name="android.permission.RECORD_AUDIO" />
        <uses-permission android:name="android.permission.POST_NOTIFICATIONS" />
    """ % mark
    assert anchor in src, "no <application> in AndroidManifest"
    src = src.replace(anchor, perms + "    " + anchor, 1)

    # 2b. application attributes: cleartext for http://nas:3456 LAN servers +
    # explicit backup policy (fail-closed; manual export is the backup path).
    # The Tauri template pins android:usesCleartextTraffic="${usesCleartextTraffic}"
    # (a manifestPlaceholder); prepending ours without removing it yields a
    # duplicate attribute — not well-formed XML — and manifest merging fails.
    # Strip any template-set copies first, then set ours exactly once.
    m = re.search(r"<application\b[^>]*>", src)
    assert m, "no <application> tag in AndroidManifest"
    tag = m.group(0)
    for attr in ("android:usesCleartextTraffic", "android:allowBackup", "android:fullBackupOnly"):
        tag = re.sub(r'\s+' + attr + r'="[^"]*"', "", tag)
    tag = tag.replace(
        "<application",
        "<application "
        'android:usesCleartextTraffic="true" '
        'android:allowBackup="false" '
        'android:fullBackupOnly="false"',
        1,
    )
    src = (
        src[: m.start()]
        + "<!-- %s: cleartext=LAN http dashboard, allowBackup=false (manual export only) -->\n    " % mark
        + tag
        + src[m.end():]
    )

    # 2c. Deep link + .cyb3 association inside the main activity. Tauri's
    # deep-link plugin may already add the scheme filter from tauri.conf.json.
    link_intent = ""
    if 'android:scheme="cybermanju"' not in src:
        link_intent = """
            <!-- %s: cybermanju:// deep link. -->
            <intent-filter>
                <action android:name="android.intent.action.VIEW" />
                <category android:name="android.intent.category.DEFAULT" />
                <category android:name="android.intent.category.BROWSABLE" />
                <data android:scheme="cybermanju" />
            </intent-filter>
    """ % mark
    file_intent = """
            <!-- %s: .cyb3 payload association. -->
            <intent-filter>
                <action android:name="android.intent.action.VIEW" />
                <category android:name="android.intent.category.DEFAULT" />
                <data android:scheme="content" android:mimeType="*/*" android:pathPattern=".*\\.cyb3" />
                <data android:scheme="file" android:mimeType="*/*" android:pathPattern=".*\\.cyb3" />
            </intent-filter>
    """ % mark
    intent = link_intent + file_intent
    close = "</activity>"
    assert close in src, "no </activity> in AndroidManifest"
    src = src.replace(close, intent + "        " + close, 1)

# 2d. Launcher bridge: package-visibility <queries> (Android 11+) + HOME
# filter so the app can serve as the device Home. The <queries> block is
# what makes queryIntentActivities(MAIN/LAUNCHER) return the full Gaveta
# without QUERY_ALL_PACKAGES (Play-restricted, deliberately not requested).
# Content-aware, NOT a bare "<queries> exists" test: the element can already
# be in the manifest (Tauri deep-link template or an older script run)
# WITHOUT the MAIN/LAUNCHER intent, and a presence-only guard silently skips
# the bridge — empty app drawer on Android 11+. Look INSIDE
# <queries>…</queries> for an intent that pairs MAIN with LAUNCHER, and
# merge one in when it is missing (same way §2e merges <package> rows).
close = "</activity>"
queries_intent = """
        <!-- %s: launcher bridge — see every MAIN/LAUNCHER activity (Android 11+ visibility). -->
        <intent>
            <action android:name="android.intent.action.MAIN" />
            <category android:name="android.intent.category.LAUNCHER" />
        </intent>
""" % mark


def has_launcher_intent(block):
    for im in re.finditer(r"<intent\b[^>]*>.*?</intent>", block, re.S):
        if (
            "android.intent.action.MAIN" in im.group(0)
            and "android.intent.category.LAUNCHER" in im.group(0)
        ):
            return True
    return False


qm = re.search(r"(<queries\b[^>]*>)(.*?)(</queries>)", src, re.S)
if qm is None:
    if "<queries" in src:
        sys.exit(
            "::error::%s: <queries> is present but not well-formed (no </queries>) — "
            "cannot place the launcher MAIN/LAUNCHER intent" % path
        )
    queries = """
    <!-- %s: launcher bridge — see every MAIN/LAUNCHER activity (Android 11+ visibility). -->
    <queries>
        <intent>
            <action android:name="android.intent.action.MAIN" />
            <category android:name="android.intent.category.LAUNCHER" />
        </intent>
    </queries>
""" % mark
    src = src.replace(anchor, queries + "    " + anchor, 1)
elif not has_launcher_intent(qm.group(0)):
    src = src[: qm.start(3)] + queries_intent + "    " + src[qm.start(3):]
home_intent = """
            <!-- %s: Home intent — offer CyberManju OS as the device launcher. -->
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.HOME" />
                <category android:name="android.intent.category.DEFAULT" />
            </intent-filter>
""" % mark
if 'android.intent.category.HOME' not in src:
    src = src.replace(close, home_intent + "        " + close, 1)

# 2e. Messages hub: explicit <package> visibility for the top mail/chat/
# social apps (labels/icons on Android 11+) + the notification-mirror
# <service>. Single <queries> element only — merge into the existing one.
social_pkgs = [
    "com.whatsapp", "com.whatsapp.w4b", "org.telegram.messenger",
    "org.signal.private.messenger", "com.instagram.android",
    "com.instagram.barcelona", "com.facebook.orca", "com.facebook.katana",
    "com.google.android.gm", "com.microsoft.office.outlook",
    "com.yahoo.mobile.client.android.mail", "ch.protonmail.android",
    "eu.faircode.email", "com.fsck.k9", "com.discord", "com.slack",
    "com.microsoft.teams", "com.twitter.android", "com.snapchat.android",
    "com.linkedin.android", "com.reddit.frontpage", "com.pinterest",
    "com.skype.raider", "com.viber.voip", "jp.naver.line.android",
    "com.tencent.mm", "com.kakao.talk", "com.google.android.apps.messaging",
    "com.tumblr", "xyz.blueskyweb.app", "org.joinmastodon.android",
]
pkg_lines = "".join(
    '        <package android:name="%s" />\n' % p
    for p in social_pkgs
    if ('<package android:name="%s"' % p) not in src
)
if pkg_lines and "</queries>" in src:
    src = src.replace(
        "</queries>",
        "        <!-- %s: messages hub — social package visibility. -->\n" % mark
        + pkg_lines
        + "    </queries>",
        1,
    )
service = """
        <!-- %s: messages hub — notification mirror (user grants access in settings). -->
        <service
            android:name=".CybermanjuMessages"
            android:permission="android.permission.BIND_NOTIFICATION_LISTENER_SERVICE"
            android:exported="false">
            <intent-filter>
                <action android:name="android.service.notification.NotificationListenerService" />
            </intent-filter>
        </service>
""" % mark
if "CybermanjuMessages" not in src:
    assert "</application>" in src, "no </application> in AndroidManifest"
    src = src.replace("</application>", service + "    </application>", 1)

# 2f. FileProvider behind OPEN WITH SYSTEM (`src/utils/openExternal.ts` ->
# launcher_open_file -> cybermanju-launcher::open_file): content:// grants
# instead of the file:// URIs Android 7+ refuses (FileUriExposedException).
# The Rust side rebuilds the same authority from the Activity package name
# ("<applicationId>.fileprovider"); roots live in res/xml/cybermanju_paths.xml
# (§6) and androidx.core is declared in §5.
provider = """
        <!-- %s: FileProvider for OPEN WITH SYSTEM — content:// grants instead of
             the file:// URIs Android 7+ refuses (FileUriExposedException). -->
        <provider
            android:name="androidx.core.content.FileProvider"
            android:authorities="${applicationId}.fileprovider"
            android:exported="false"
            android:grantUriPermissions="true">
            <meta-data
                android:name="android.support.FILE_PROVIDER_PATHS"
                android:resource="@xml/cybermanju_paths" />
        </provider>
""" % mark
if "androidx.core.content.FileProvider" not in src:
    assert "</application>" in src, "no </application> in AndroidManifest"
    src = src.replace("</application>", provider + "    </application>", 1)

# Fail fast: the merger's "Error parsing" surfaces after minutes of Gradle.
# Well-formedness here keeps a broken patch from ever reaching the build.
try:
    xml.dom.minidom.parseString(src)
except Exception as e:
    sys.exit("AndroidManifest.xml is not well-formed after patching: %s" % e)

if src == original:
    print("already configured: " + path)
else:
    open(path, "w").write(src)
    print("patched " + path)
PYEOF

# ─── 4. Messages-hub listener service (§2c) ────────────────────────
# `gen/` is gitignored and regenerated by `tauri android init`, so the
# Kotlin service lives here as a heredoc and is (re)written on every
# configure run. Marker-guarded: an existing file WITHOUT our marker is a
# foreign file and is never touched; a file WITH it is refreshed in place.
APP_ID="$(node -p "require('./src-tauri/tauri.conf.json').identifier")"
KOTLIN_PKG="${APP_ID}"
KOTLIN_DIR="$ANDROID_DIR/app/src/main/java/${APP_ID//.//}"
SERVICE_FILE="$KOTLIN_DIR/CybermanjuMessages.kt"
SERVICE_MARK="CyberManju OS messages hub (scripts/android-configure.sh)"

if [ ! -d "$KOTLIN_DIR" ]; then
  echo "::error::$KOTLIN_DIR not found — unexpected Tauri template layout (identifier $APP_ID)" >&2
  exit 1
fi

if [ -f "$SERVICE_FILE" ] && ! grep -q "$SERVICE_MARK" "$SERVICE_FILE"; then
  echo "::error::$SERVICE_FILE exists without our marker — refusing to overwrite a foreign file" >&2
  exit 1
fi

cat > "$SERVICE_FILE" <<KTEOF
package $KOTLIN_PKG

// $SERVICE_MARK: mirrors mail/chat/social notifications into
// cybermanju-messages.json (app-private files dir, 200-entry ring).
// The Rust launcher crate only READS that file; notification access itself
// is granted by the user in system settings (see launcher hub prompt).

import android.app.Notification
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

class CybermanjuMessages : NotificationListenerService() {

    override fun onListenerConnected() {
        try {
            for (sbn in activeNotifications) record(sbn)
        } catch (_: Exception) {
        }
    }

    override fun onNotificationPosted(sbn: StatusBarNotification) {
        record(sbn)
    }

    // Hub "Clear all": the Rust bridge fires an explicit intent at this
    // running service (no intent-filter needed for explicit intents).
    // Dismisses the live shade AND wipes the mirror file together.
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        try {
            if (intent?.action == ACTION_CLEAR) {
                try {
                    cancelAllNotifications()
                } catch (_: Exception) {
                }
                synchronized(LOCK) {
                    try {
                        File(filesDir, "cybermanju-messages.json").writeText("[]")
                    } catch (_: Exception) {
                    }
                }
            }
        } catch (_: Exception) {
        }
        return START_NOT_STICKY
    }

    // Mirrors Rust is_social_package (crates/launcher/src/lib.rs): exact
    // package match -> token contains -> dot-segment match, then the
    // notification category as the Kotlin-only extra. Keep the three lists
    // in the companion object in sync with the Rust source of truth —
    // tests/frontend/launcher-social-parity.test.ts asserts the parity.
    private fun isSocial(pkg: String, n: Notification): Boolean {
        if (pkg == packageName) return false
        val p = pkg.trim().lowercase()
        if (p.isEmpty()) return false
        if (p in SOCIAL) return true
        for (t in TOKENS) if (p.contains(t)) return true
        // Rust SOCIAL_SEGMENTS: short tokens (line must not fire inside
        // offline) only match a whole dot-segment of the package.
        if (p.split('.').any { it in SEGMENTS }) return true
        val c = n.category
        return c == Notification.CATEGORY_MESSAGE || c == Notification.CATEGORY_EMAIL
    }

    private fun record(sbn: StatusBarNotification) {
        try {
            val pkg = sbn.packageName ?: return
            val n = sbn.notification ?: return
            if (!isSocial(pkg, n)) return
            val ex = n.extras ?: return
            val title = ex.getCharSequence(Notification.EXTRA_TITLE)?.toString().orEmpty()
            var text = ex.getCharSequence(Notification.EXTRA_TEXT)?.toString().orEmpty()
            if (text.isBlank()) {
                text = ex.getCharSequence(Notification.EXTRA_BIG_TEXT)?.toString().orEmpty()
            }
            if (title.isBlank() && text.isBlank()) return
            val label = try {
                val ai = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    packageManager.getApplicationInfo(
                        pkg, PackageManager.ApplicationInfoFlags.of(0)
                    )
                } else {
                    @Suppress("DEPRECATION")
                    packageManager.getApplicationInfo(pkg, 0)
                }
                packageManager.getApplicationLabel(ai)?.toString() ?: pkg
            } catch (_: Exception) {
                pkg
            }
            synchronized(LOCK) {
                val file = File(filesDir, "cybermanju-messages.json")
                val arr = try {
                    if (file.exists()) JSONArray(file.readText()) else JSONArray()
                } catch (_: Exception) {
                    JSONArray()
                }
                var updated = false
                for (i in 0 until arr.length()) {
                    val o = arr.optJSONObject(i) ?: continue
                    if (o.optString("key") == sbn.key) {
                        o.put("title", title)
                        o.put("text", text)
                        o.put("timestamp", sbn.postTime)
                        updated = true
                        break
                    }
                }
                if (!updated) {
                    val o = JSONObject()
                    o.put("key", sbn.key)
                    o.put("packageName", pkg)
                    o.put("appLabel", label)
                    o.put("title", title)
                    o.put("text", text)
                    o.put("timestamp", sbn.postTime)
                    o.put("category", n.category ?: "")
                    arr.put(o)
                }
                while (arr.length() > 200) arr.remove(0)
                try {
                    file.writeText(arr.toString())
                } catch (_: Exception) {
                }
            }
        } catch (_: Exception) {
        }
    }

    companion object {
        private val LOCK = Any()
        const val ACTION_CLEAR = "com.cybermanju.os.action.CLEAR_NOTIFICATIONS"
        // SOCIAL / TOKENS / SEGMENTS mirror the Rust source of truth in
        // crates/launcher/src/lib.rs (SOCIAL_PACKAGES / SOCIAL_TOKENS /
        // SOCIAL_SEGMENTS) verbatim — the hub must never silently drop a
        // app the launcher crate mirrors. Asserted by
        // tests/frontend/launcher-social-parity.test.ts.
        private val SOCIAL = setOf(
            "com.whatsapp", "com.whatsapp.w4b", "org.telegram.messenger",
            "org.telegram.plus", "org.thunderdog.challegram", "tw.nekomimi.nekogram",
            "org.signal.private.messenger", "com.instagram.android",
            "com.instagram.barcelona", "com.facebook.orca", "com.facebook.katana",
            "com.facebook.lite", "com.facebook.mlite", "com.google.android.gm",
            "com.microsoft.office.outlook", "com.yahoo.mobile.client.android.mail",
            "ch.protonmail.android", "com.tutanota.android", "eu.faircode.email",
            "com.fsck.k9", "com.readdle.spark", "me.bluemail.mail",
            "com.ninefolders.hd3", "org.kman.aquamail", "de.gmx.mobile.android.mail",
            "de.web.mobile.android.mail", "com.samsung.android.email.provider",
            "com.samsung.android.messaging", "com.google.android.apps.messaging",
            "com.google.android.apps.tachyon", "com.google.android.apps.meet",
            "com.discord", "com.slack", "com.microsoft.teams", "com.twitter.android",
            "com.zhiliaoapp.musically", "com.ss.android.ugc.trill",
            "com.snapchat.android", "com.linkedin.android", "com.reddit.frontpage",
            "com.pinterest", "com.skype.raider", "com.viber.voip",
            "jp.naver.line.android", "com.tencent.mm", "com.tencent.mobileqq",
            "com.vk.android", "com.kakao.talk", "com.imo.android.imoim",
            "com.zing.zalo", "com.tumblr", "xyz.blueskyweb.app",
            "org.joinmastodon.android", "com.keylesspalace.tusky",
            "com.truthsocial.android.app"
        )
        private val TOKENS = listOf(
            "whatsapp", "telegram", "messenger", "instagram", "facebook",
            "snapchat", "discord", "signal", "viber", "kakao", "wechat",
            "weixin", "outlook", "tumblr", "reddit", "skype", "teams", "slack",
            "pinterest", "linkedin", "twitter", "tiktok", "mastodon", "tusky",
            "bluesky", "proton", "tutanota", "yahoo", "threads", "musical",
            "trill", "zalo", "challegram", "nekomimi", "orca", "katana",
            "faircode"
        )
        private val SEGMENTS = listOf("line", "mail", "sms", "mms", "talk", "chat")
    }
}
KTEOF
echo "messages service ok: $SERVICE_FILE"

# ─── 5. Runtime deps the manifest/plugin code names (androidx) ──────
# tauri-plugin-scoped-storage 1.5.1 (crates.io) imports
# androidx.documentfile.provider.DocumentFile in its Android plugin
# (ScopedStoragePlugin.kt) but its library build.gradle.kts never declares
# androidx.documentfile — the class is present on the compile classpath
# (transitive) yet missing at RUNTIME, so tauri's PluginManager.load dies
# with `NoClassDefFoundError: androidx/documentfile/provider/DocumentFile`
# the instant the app opens (2026-10-18, v0.1.1 release APK on a Galaxy A50,
# Android 11: splash → FATAL EXCEPTION main → process dead). The app module
# owns its runtime classpath, so the dependency lands in
# gen/android/app/build.gradle.kts. Marker-guarded and idempotent; drop it
# when the plugin starts shipping the dependency itself (upstream report).
# §2f's FileProvider adds androidx.core the same way (see the guard below).
APP_GRADLE="$ANDROID_DIR/app/build.gradle.kts"
if [ ! -f "$APP_GRADLE" ]; then
  echo "::error::$APP_GRADLE not found — unexpected Tauri template layout" >&2
  exit 1
fi

# §5 deps, each independent: androidx.core hosts the FileProvider class §2f
# declares (core-ktx pulls it in transitively, but the manifest names it —
# an explicit line keeps the classpath honest if the ktx pin ever moves).
MISSING_DEPS=""
grep -q "androidx.documentfile:documentfile" "$APP_GRADLE" \
  || MISSING_DEPS="$MISSING_DEPS androidx.documentfile:documentfile:1.0.1"
grep -q "androidx.core:core:" "$APP_GRADLE" \
  || MISSING_DEPS="$MISSING_DEPS androidx.core:core:1.13.1"

if [ -z "$MISSING_DEPS" ]; then
  echo "runtime deps already present — leaving $APP_GRADLE alone"
else
  # shellcheck disable=SC2086  # intentional word split on dep coordinates
  python3 - "$APP_GRADLE" "$MARK" $MISSING_DEPS <<'PYEOF'
import re
import sys

path, mark = sys.argv[1], sys.argv[2]
src = open(path).read()
# Idempotent at this level too: the bash guard above already skips, but a
# second insert would silently duplicate a dependency line.
deps = [d for d in sys.argv[3:] if d not in src]
if not deps:
    print("already present: " + path)
    sys.exit(0)
# Top-level `dependencies {` first (the template writes it at column 0);
# any indented one is the fallback.
m = re.search(r"(?m)^(dependencies\s*\{)", src)
if not m:
    m = re.search(r"(?m)^(\s*)(dependencies\s*\{)", src)
assert m, "no dependencies { } block in %s" % path
# Indent one level past the `dependencies` line itself.
indent = re.match(r"\s*", m.group(0)).group(0) + "    "
# The explanatory comment carries the $MARK, so it doubles as its own
# marker: it is emitted once only — a partial-state rerun (e.g.
# documentfile already landed from an older script, core still missing)
# inserts just the missing implementation line, never a stacked duplicate
# comment block.
lines = [""]
if mark not in src:
    lines += [
        "%s// %s: runtime deps the manifest/plugin code names but no in-tree"
        % (indent, mark),
        "%s// artifact declares: androidx.documentfile (tauri-plugin-scoped-storage's"
        % indent,
        "%s// ScopedStoragePlugin.kt — absent at runtime it crashes app launch) and"
        % indent,
        "%s// androidx.core (androidx.core.content.FileProvider, §2f manifest entry)."
        % indent,
    ]
lines += ['%simplementation("%s")' % (indent, d) for d in deps]
lines.append("")
src = src[: m.end()] + "\n".join(lines) + src[m.end():]
open(path, "w").write(src)
print("patched: " + " ".join(deps))
PYEOF
fi

# ─── 6. FileProvider paths (@xml/cybermanju_paths, §2f) ─────────────
# The manifest's <provider> meta-data points at this resource; without it
# FileProvider.getUriForFile throws IllegalArgumentException and OPEN WITH
# SYSTEM surfaces an honest error instead of opening. gen/ is regenerated,
# so the XML is written here — marker-guarded like §4 (a foreign file is
# never touched) and refreshed in place when it is ours. <root-path>
# covers the vault wherever it lives (shared storage, SD card, app-private);
# each open grants read on that one content:// URI, nothing persistent.
PROVIDER_XML_DIR="$ANDROID_DIR/app/src/main/res/xml"
PROVIDER_XML="$PROVIDER_XML_DIR/cybermanju_paths.xml"

mkdir -p "$PROVIDER_XML_DIR"
if [ -f "$PROVIDER_XML" ] && ! grep -q "$MARK" "$PROVIDER_XML"; then
  echo "::error::$PROVIDER_XML exists without our marker — refusing to overwrite a foreign file" >&2
  exit 1
fi

cat > "$PROVIDER_XML" <<XMLEOF
<?xml version="1.0" encoding="utf-8"?>
<!-- $MARK: FileProvider roots for launcher_open_file (§2f manifest entry).
     root-path spans the filesystem so a vault on shared/external storage
     resolves; every open grants FLAG_GRANT_READ_URI_PERMISSION on that one
     content:// URI (file:// is refused on Android 7+). -->
<paths xmlns:android="http://schemas.android.com/apk/res/android">
    <root-path name="cybermanju-root" path="/" />
</paths>
XMLEOF

# Fail fast on malformed XML before Gradle's merger reports it minutes later.
python3 -c 'import sys, xml.dom.minidom; xml.dom.minidom.parse(sys.argv[1])' "$PROVIDER_XML"
echo "fileprovider paths ok: $PROVIDER_XML"

# ─── 3. ABI / artifact policy note ─────────────────────────────────
# ort-sys ships prebuilt ONNX Runtime only for aarch64-linux-android, so
# release artifacts are arm64-v8a-only. x86_64 exists for the emulator smoke
# test only (scripts/android-smoke.sh). Tracked debt: docs/ANDROID.md §ABI.
echo "ABI policy: release=arm64-v8a only (ort-sys), emulator=x86_64 smoke only"
echo "android configure complete"
