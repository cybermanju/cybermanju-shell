#!/usr/bin/env bash
# CyberManju OS — iOS signing (scripts/ios-signing.sh).
# Role: Apple device signing for CI, mirroring scripts/android-signing.sh
# (prepare/verify/cleanup). The Xcode project is generated (`tauri ios init`,
# gitignored like Android's `gen/`), so nothing signing-related is committed.
# Secrets live only in GitHub Actions secrets: APPLE_CERTIFICATE_P12_B64
# (distribution .p12, base64), APPLE_CERTIFICATE_PASSWORD, APPLE_PROVISIONING_PROFILE_B64
# (`.mobileprovision`, base64), KEYCHAIN_PASSWORD (ephemeral CI keychain).
#
#   scripts/ios-signing.sh prepare   # after `tauri ios init`, before the build
#   scripts/ios-signing.sh verify    # after the build
#   scripts/ios-signing.sh cleanup   # delete keychain + profiles (before artifact upload)
#
# Simulator builds need no signing and skip prepare gracefully (warning, not
# error) unless REQUIRE_SIGNING=1 (release workflow — must never publish an
# uninstallable IPA). macOS-only: every entry refuses on other OSes with an
# honest message instead of failing obscurely inside xcodebuild.
# Contracts: idempotent; `verify` fails closed when signing was expected;
# `cleanup` shreds secrets and never fails the job.
set -euo pipefail

cd "$(dirname "$0")/.."

IOS_DIR="src-tauri/gen/apple"
# Written by `prepare` when it actually configured signing, so `verify` can
# tell "signing was expected but did not happen" from "simulator build".
EXPECTED_MARKER="src-tauri/gen/apple/.signing-expected"
KEYCHAIN_NAME="cybermanju-ios-signing"

require_signing() { [ "${REQUIRE_SIGNING:-0}" = "1" ]; }

need_macos() {
  if [ "$(uname -s)" != "Darwin" ]; then
    echo "::error::iOS signing needs macOS with Xcode (found $(uname -s)) — run the ios-build CI job on macos-latest" >&2
    exit 1
  fi
}

prepare() {
  need_macos
  if [ ! -d "$IOS_DIR" ]; then
    echo "::error::$IOS_DIR not found — run \`tauri ios init\` first" >&2
    exit 1
  fi

  if [ -z "${APPLE_CERTIFICATE_P12_B64:-}" ] || [ -z "${APPLE_PROVISIONING_PROFILE_B64:-}" ]; then
    if require_signing; then
      echo "::error::Apple signing secrets are unavailable; refusing to publish an IPA that cannot be installed" >&2
      exit 1
    fi
    echo "::warning::Apple signing secrets are unavailable; device IPA will be skipped (simulator .app still builds)" >&2
    return 0
  fi

  command -v security >/dev/null \
    || { echo "::error::macOS \`security\` CLI not found — Xcode CLT missing" >&2; exit 1; }

  local cert_p12="$RUNNER_TEMP/cybermanju-ios.p12"
  local profile_dir="$HOME/Library/MobileDevice/Provisioning Profiles"
  mkdir -p "$profile_dir"
  printf '%s' "$APPLE_CERTIFICATE_P12_B64" | base64 -d > "$cert_p12" \
    || { echo "::error::APPLE_CERTIFICATE_P12_B64 is not valid base64" >&2; exit 1; }
  chmod 600 "$cert_p12"

  # Ephemeral keychain so the CI login keychain is never polluted and the
  # secret never survives the job (see `cleanup`).
  local keychain_pass="${KEYCHAIN_PASSWORD:-cybermanju-ios-ci}"
  security create-keychain -p "$keychain_pass" "$KEYCHAIN_NAME" || true
  security set-keychain-settings -lut 21600 "$KEYCHAIN_NAME"
  security unlock-keychain -p "$keychain_pass" "$KEYCHAIN_NAME"
  security import "$cert_p12" -k "$KEYCHAIN_NAME" -P "${APPLE_CERTIFICATE_PASSWORD:-}" -T /usr/bin/codesign
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$keychain_pass" "$KEYCHAIN_NAME"
  security list-keychains -s "$KEYCHAIN_NAME" "$(security list-keychains -d user | tr -d '\" ')"
  rm -f "$cert_p12"

  # Provisioning profile: name on disk must match its UUID for Xcode to find it.
  local profile_tmp="$RUNNER_TEMP/cybermanju.mobileprovision"
  printf '%s' "$APPLE_PROVISIONING_PROFILE_B64" | base64 -d > "$profile_tmp" \
    || { echo "::error::APPLE_PROVISIONING_PROFILE_B64 is not valid base64" >&2; exit 1; }
  local uuid
  uuid="$(/usr/libexec/PlistBuddy -c 'Print :UUID' /dev/stdin < <(security cms -D -i "$profile_tmp") 2>/dev/null || true)"
  if [ -z "$uuid" ]; then
    echo "::error::provisioning profile is not a valid CMS plist (wrong secret or corrupt)" >&2
    rm -f "$profile_tmp"
    exit 1
  fi
  cp "$profile_tmp" "$profile_dir/$uuid.mobileprovision"
  chmod 600 "$profile_dir/$uuid.mobileprovision"
  rm -f "$profile_tmp"
  echo "installed provisioning profile $uuid"

  touch "$EXPECTED_MARKER"
  echo "iOS signing configured (ephemeral keychain $KEYCHAIN_NAME)"
}

verify() {
  # Simulator .app is the unsigned artifact; device .ipa needs signing.
  local app
  app="$(find "$IOS_DIR" -name '*.app' -maxdepth 6 2>/dev/null | head -1 || true)"
  local ipa
  ipa="$(find "$IOS_DIR" -name '*.ipa' 2>/dev/null | head -1 || true)"

  if [ -n "$ipa" ]; then
    echo "verifying device IPA: $ipa"
    if command -v codesign >/dev/null; then
      codesign --verify --deep --strict --verbose=2 "$ipa" 2>&1 || {
        echo "::error::IPA signature verification failed: $ipa" >&2
        exit 1
      }
    fi
    echo "verified signed IPA: $ipa"
    return 0
  fi

  if [ -f "$EXPECTED_MARKER" ] || require_signing; then
    echo "::error::signing was expected but no .ipa was produced under $IOS_DIR" >&2
    find "$IOS_DIR" -name '*.app' 2>/dev/null || true
    exit 1
  fi

  if [ -n "$app" ]; then
    echo "::warning::device IPA skipped (no Apple secrets); simulator artifact: $app" >&2
    return 0
  fi
  echo "::warning::no iOS artifact found under $IOS_DIR (simulator build may have been skipped)" >&2
  return 0
}

# Secret hygiene: delete the ephemeral keychain + installed profiles before
# artifact upload so self-hosted runners / debug archives never retain Apple
# signing secrets. Missing files are fine (fail-safe).
cleanup() {
  local removed=0
  if [ "$(uname -s)" = "Darwin" ]; then
    if security list-keychains -d user 2>/dev/null | grep -q "$KEYCHAIN_NAME"; then
      security delete-keychain "$KEYCHAIN_NAME" || true
      removed=1
    fi
    local profile_dir="$HOME/Library/MobileDevice/Provisioning Profiles"
    if [ -f "$EXPECTED_MARKER" ] && [ -d "$profile_dir" ]; then
      find "$profile_dir" -name '*.mobileprovision' -mmin -120 -delete 2>/dev/null || true
      removed=1
    fi
  fi
  if [ -f "$EXPECTED_MARKER" ]; then
    rm -f "$EXPECTED_MARKER"
    removed=1
  fi
  if [ "$removed" = "1" ]; then
    echo "iOS signing secrets shredded"
  else
    echo "no iOS signing secrets to shred"
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
