#!/usr/bin/env bash
# CyberManju OS — Android signing (scripts/android-signing.sh).
# Android release signing for CI.
#
# `tauri android init` generates app/build.gradle.kts without a release
# signingConfig, so Gradle emits `*-release-unsigned.apk` and Android refuses
# to install it ("App not installed" / parse error). The keystore lives only
# in GitHub Actions secrets (src-tauri/gen/ is gitignored, so nothing
# signing-related is ever committed).
#
#   scripts/android-signing.sh prepare   # after `tauri android init`, before the build
#   scripts/android-signing.sh verify    # after the build
#   scripts/android-signing.sh cleanup   # shred keystore + properties (before artifact upload)
#
# `prepare` appends a signing block to the generated app/build.gradle.kts and
# drops `release-signing.properties` next to it. Values are read from that
# file (not from the environment) because a reused Gradle daemon would report
# its own `System.getenv`.
# REQUIRE_SIGNING=1 turns "signing unavailable" into an error — used by the
# release workflow, which must never publish an uninstallable APK.
set -euo pipefail

cd "$(dirname "$0")/.."

APP_DIR="src-tauri/gen/android/app"
GRADLE_FILE="$APP_DIR/build.gradle.kts"
# Written by `prepare` when it actually configured signing, so `verify` can
# tell "signing was expected but did not happen" from "signing unavailable".
EXPECTED_MARKER="src-tauri/gen/android/.signing-expected"
UNSIGNED_MARK="Android release signing (CI)"

require_signing() { [ "${REQUIRE_SIGNING:-0}" = "1" ]; }

# ABI token of a Gradle output name (`app-arm64-v8a-release.apk`,
# `app-x86_64-release.apk`, `app-universal-release.apk`): read it from the
# ORIGINAL filename so a rename never stamps arm64-v8a onto another ABI.
# `arm64-v8a` is the fallback only when the name carries no ABI token — it
# is the one shipped release ABI (ort-sys ships no other Android dist).
abi_of() {
  case "$(basename "$1")" in
    *arm64-v8a*) echo "arm64-v8a" ;;
    *armeabi-v7a*) echo "armeabi-v7a" ;;
    *x86_64*) echo "x86_64" ;;
    *x86*) echo "x86" ;;
    *) echo "arm64-v8a" ;;
  esac
}

prepare() {
  if [ ! -f "$GRADLE_FILE" ]; then
    echo "::error::$GRADLE_FILE not found — run \`tauri android init\` first" >&2
    exit 1
  fi

  if [ -z "${ANDROID_KEYSTORE_B64:-}" ]; then
    if require_signing; then
      echo "::error::Android keystore secrets are unavailable; refusing to publish an APK that cannot be installed" >&2
      exit 1
    fi
    echo "::warning::Android keystore secrets are unavailable; the APK will be unsigned" >&2
    return 0
  fi

  printf '%s' "$ANDROID_KEYSTORE_B64" | base64 -d > "$APP_DIR/cybermanju-release.keystore" \
    || { echo "::error::ANDROID_KEYSTORE_B64 is not valid base64" >&2; exit 1; }
  chmod 600 "$APP_DIR/cybermanju-release.keystore"

  # Fail fast: a bad password/alias otherwise surfaces ~10 minutes later as
  # a Gradle `KeytoolException ... Given final block not properly padded`.
  # The probe below performs the exact calls Gradle/AGP makes (open the store,
  # look up the alias, recover the key), so each secret is blamed precisely.
  # NB: `keytool` cannot do the key-password step — for PKCS12 it silently
  # recovers the key with the *store* password — hence the small Java probe.
  command -v java > /dev/null \
    || { echo "::error::java not found — cannot validate the Android keystore" >&2; exit 1; }
  local probe_dir
  probe_dir="$(mktemp -d)"
  cat > "$probe_dir/KeyCheck.java" <<'EOF'
import java.io.File;
import java.security.KeyStore;

public class KeyCheck {
  public static void main(String[] args) throws Exception {
    File file = new File(args[0]);
    char[] storePass = args[1].toCharArray();
    String alias = args[2];
    char[] keyPass = args[3].toCharArray();
    KeyStore ks;
    try {
      ks = KeyStore.getInstance(file, storePass);
    } catch (Exception e) {
      System.out.println("STORE_UNLOCK_FAILED " + e.getMessage());
      System.exit(10);
      return;
    }
    if (!ks.containsAlias(alias)) {
      System.out.println("ALIAS_MISSING");
      System.exit(20);
    }
    if (!ks.isKeyEntry(alias)) {
      System.out.println("ALIAS_NOT_A_KEY");
      System.exit(30);
    }
    try {
      if (ks.getKey(alias, keyPass) == null) {
        System.out.println("KEY_PASSWORD_REJECTED");
        System.exit(40);
      }
    } catch (Exception e) {
      System.out.println("KEY_PASSWORD_REJECTED " + e.getMessage());
      System.exit(40);
    }
    System.out.println("OK");
  }
}
EOF
  local store_pass="${ANDROID_KEYSTORE_PASSWORD:-}"
  local key_alias="${ANDROID_KEY_ALIAS:-cybermanju-os}"
  local key_pass="${ANDROID_KEY_PASSWORD:-$store_pass}"
  local fell_back=0
  local probe_out probe_code
  probe_code=0
  probe_out="$(java "$probe_dir/KeyCheck.java" \
    "$APP_DIR/cybermanju-release.keystore" \
    "$store_pass" "$key_alias" "$key_pass" 2>&1)" || probe_code=$?
  # Modern keytool forces the key password to equal the store password for
  # PKCS12, so a stale/wrong ANDROID_KEY_PASSWORD is worth one retry with
  # the store password before declaring the secrets broken.
  if [ "$probe_code" = "40" ] && [ -n "${ANDROID_KEY_PASSWORD:-}" ] \
      && [ "$key_pass" != "$store_pass" ]; then
    fell_back=1
    echo "::warning::ANDROID_KEY_PASSWORD was rejected by the keystore; retrying with the store password" >&2
    key_pass="$store_pass"
    probe_code=0
    probe_out="$(java "$probe_dir/KeyCheck.java" \
      "$APP_DIR/cybermanju-release.keystore" \
      "$store_pass" "$key_alias" "$key_pass" 2>&1)" || probe_code=$?
  fi
  rm -rf "$probe_dir"
  case "$probe_code" in
    0) ;;
    10)
      echo "::error::Android keystore unlock failed — ANDROID_KEYSTORE_PASSWORD is wrong (or the keystore is corrupt)" >&2
      echo "$probe_out" >&2
      exit 1
      ;;
    20)
      echo "::error::ANDROID_KEY_ALIAS is not in the keystore (store password is fine)" >&2
      exit 1
      ;;
    30)
      echo "::error::ANDROID_KEY_ALIAS exists but holds no key entry" >&2
      exit 1
      ;;
    40)
      if [ "$fell_back" = "1" ]; then
        echo "::error::Android key password rejected — neither ANDROID_KEY_PASSWORD nor the store password unlocks the key entry (store password and alias are fine)" >&2
      else
        echo "::error::Android key password rejected — ANDROID_KEY_PASSWORD does not match the key entry (store password and alias are fine)" >&2
      fi
      echo "$probe_out" >&2
      exit 1
      ;;
    *)
      echo "::error::Android keystore validation failed (exit $probe_code)" >&2
      echo "$probe_out" >&2
      exit 1
      ;;
  esac

  # When ANDROID_KEY_PASSWORD is unset the key is assumed to share the
  # store password (keytool/Android Studio default); otherwise it must be
  # the key entry's own password, which may differ from the store password.
  cat > "$APP_DIR/release-signing.properties" <<EOF
storePassword=${store_pass}
keyAlias=${key_alias}
keyPassword=${key_pass}
EOF
  chmod 600 "$APP_DIR/release-signing.properties"

  # Idempotent: `tauri android init` regenerates the file on a fresh runner.
  if ! grep -q "$UNSIGNED_MARK" "$GRADLE_FILE"; then
    cat >> "$GRADLE_FILE" <<'EOF'

// Android release signing (CI) — injected by scripts/android-signing.sh.
// Values come from a file (java.util.* is unresolved in Kotlin DSL scripts,
// and a reused Gradle daemon would report a stale environment).
val androidReleaseSigningFile = file("release-signing.properties")
if (androidReleaseSigningFile.exists()) {
    val androidReleaseSigningProps = androidReleaseSigningFile.readLines()
        .filter { it.contains('=') }
        .associate { it.substringBefore('=') to it.substringAfter('=') }
    android {
        signingConfigs {
            create("release") {
                storeFile = file("cybermanju-release.keystore")
                storePassword = androidReleaseSigningProps["storePassword"]
                keyAlias = androidReleaseSigningProps["keyAlias"]
                keyPassword = androidReleaseSigningProps["keyPassword"]
            }
        }
        buildTypes {
            getByName("release") {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }
}
EOF
  fi

  touch "$EXPECTED_MARKER"
  echo "release signing configured"
}

verify() {
  local apk_dir="$APP_DIR/build/outputs/apk"
  local bundle_dir="$APP_DIR/build/outputs/bundle"
  # Signing was configured (or forced) if a signed APK must exist.
  local signing_expected=0
  if [ -f "$EXPECTED_MARKER" ] || require_signing; then
    signing_expected=1
  fi

  local version=""
  if command -v node > /dev/null; then
    version="$(node -p "require('./package.json').version" 2>/dev/null || echo "")"
  fi

  # AABs (Play upload) are listed for the artifact; signature lives in the
  # APKs verified below (same keystore). Never fail on AAB absence: older
  # Tauri templates may emit APK-only.
  if [ -d "$bundle_dir" ]; then
    find "$bundle_dir" -name '*.aab' 2>/dev/null || true
  fi

  # Every signed APK (split-per-abi can emit more than one) — never assume
  # a single artifact.
  local signed
  signed="$(find "$apk_dir" -name '*.apk' ! -name '*-unsigned.apk' 2>/dev/null || true)"

  if [ -z "$signed" ]; then
    if [ "$signing_expected" = "1" ]; then
      echo "::error::no signed APK produced under $apk_dir" >&2
      find "$apk_dir" -name '*.apk' 2>/dev/null || true
      exit 1
    fi
    # Signing parity (P2-8): an unsigned artifact must SAY unsigned — never
    # ship `app-universal-release-unsigned.apk` under a neutral name. Every
    # unsigned APK gets renamed, with the ABI read from the original
    # filename (fallback arm64-v8a) so an x86_64/emulator artifact is never
    # mislabeled; a name collision never overwrites another artifact.
    local unsigned renamed renamed_aab abi found=0
    while IFS= read -r unsigned; do
      [ -n "$unsigned" ] || continue
      found=1
      if [ -n "$version" ]; then
        abi="$(abi_of "$unsigned")"
        renamed="$(dirname "$unsigned")/CyberManju-OS-$version-$abi-unsigned.apk"
        if [ "$unsigned" != "$renamed" ] && [ ! -e "$renamed" ]; then
          mv "$unsigned" "$renamed"
          echo "::warning::APK is UNSIGNED (keystore secrets unavailable): $renamed" >&2
        fi
      fi
    done <<< "$(find "$apk_dir" -name '*-unsigned.apk' 2>/dev/null || true)"
    # Same contract for the bundle: an unsigned AAB would otherwise ship
    # under the neutral `app-release.aab` (docs/ANDROID.md §2 — unsigned
    # artifacts always say `unsigned` in the file name).
    while IFS= read -r unsigned; do
      [ -n "$unsigned" ] || continue
      found=1
      if [ -n "$version" ]; then
        renamed_aab="$(dirname "$unsigned")/CyberManju-OS-$version-unsigned.aab"
        if [ "$unsigned" != "$renamed_aab" ] && [ ! -e "$renamed_aab" ]; then
          mv "$unsigned" "$renamed_aab"
          echo "::warning::AAB is UNSIGNED (keystore secrets unavailable): $renamed_aab" >&2
        fi
      fi
    done <<< "$(find "$bundle_dir" -name '*.aab' 2>/dev/null || true)"
    if [ "$found" != "1" ] || [ -z "$version" ]; then
      echo "::warning::APK/AAB is unsigned (keystore secrets unavailable)" >&2
    fi
    return 0
  fi

  local apksigner
  apksigner="$(ls "${ANDROID_HOME:-/usr/local/lib/android/sdk}"/build-tools/*/apksigner 2>/dev/null | sort -V | tail -1 || true)"
  if [ -z "$apksigner" ]; then
    echo "::error::apksigner not found in \$ANDROID_HOME/build-tools" >&2
    exit 1
  fi

  local apk
  while IFS= read -r apk; do
    [ -n "$apk" ] || continue
    "$apksigner" verify --verbose --print-certs "$apk"
  done <<< "$signed"
  # The unsigned twin (if any) must not ship in the artifact.
  find "$apk_dir" -name '*-unsigned.apk' -delete

  # Gradle's `app-universal-release.apk` tells nobody which device it fits;
  # the build only contains arm64-v8a (ort-sys ships no other Android ABI).
  # The ABI is read from the original filename (arm64-v8a fallback only
  # when the name carries no ABI token), every signed APK is renamed, and a
  # name collision (universal + per-abi twin) never overwrites an artifact.
  if [ -n "$version" ]; then
    local friendly
    while IFS= read -r apk; do
      [ -n "$apk" ] || continue
      friendly="$(dirname "$apk")/CyberManju-OS-$version-$(abi_of "$apk").apk"
      if [ "$apk" != "$friendly" ] && [ ! -e "$friendly" ]; then
        mv "$apk" "$friendly"
        echo "renamed signed APK: $friendly"
      fi
    done <<< "$signed"
  fi

  echo "verified signed APKs under $apk_dir"
}

# Secret hygiene: shred the keystore + signing properties before artifact
# upload so self-hosted runners / debug archives never retain plaintext
# signing secrets. Ephemeral GitHub runners mitigate this, but cleanup is
# cheap and fail-safe (missing files are fine).
cleanup() {
  local removed=0
  for f in "$APP_DIR/cybermanju-release.keystore" "$APP_DIR/release-signing.properties" "$EXPECTED_MARKER"; do
    if [ -f "$f" ]; then
      if command -v shred > /dev/null; then
        shred -u "$f" || rm -f "$f"
      else
        rm -f "$f"
      fi
      removed=1
    fi
  done
  if [ "$removed" = "1" ]; then
    echo "signing secrets shredded"
  else
    echo "no signing secrets to shred"
  fi
}

case "${1:-}" in
  prepare) prepare ;;
  verify) verify ;;
  cleanup) cleanup ;;
  *)
    echo "usage: $0 {prepare|verify|cleanup}" >&2
    exit 2
    ;;
esac
