#!/usr/bin/env bash
# CyberManju OS — iOS post-init configuration (scripts/ios-configure.sh).
# Role: every in-tree iOS setting that `tauri ios init` does NOT generate, so
# CI never relies on Tauri template defaults silently (mirrors
# scripts/android-configure.sh for Android).
#
#   1. version audit — `package.json` must equal `tauri.conf.json > version`
#      (Tauri stamps the version into Info.plist at build time).
#   2. bundle floor — `bundle.iOS.minimumSystemVersion` must be 17.0+
#      (NSAllowsLocalNetworking needs iOS 17+; see docs/IOS.md).
#   3. framework link — `SystemConfiguration.framework` must be linked by the
#      generated iOS target because `netdev` references its native symbols.
#   4. identifier audit — tauri.conf.json identifier must be com.cybermanju.os
#      (matches the provisioning profile + deep-link scheme).
#   5. ATS note — LAN dashboard over plain http:// needs
#      NSAllowsLocalNetworking; HTTPS behind the Caddy sidecar needs no
#      exception. This script only AUDITS the Xcode project for the key when
#      it exists and prints the procedure otherwise — never patches a plist.
#
# Usage: bash scripts/ios-configure.sh [gen/apple-dir]
# Idempotent: patches the generated Xcode linker flags and audits settings;
# signing changes remain owned by ios-signing.sh. macOS-only for Xcode;
# version/bundle audits run on any OS so Linux meta-checks stay honest.
set -euo pipefail

cd "$(dirname "$0")/.."

APPLE_DIR="${1:-src-tauri/gen/apple}"
MARK="CyberManju OS (scripts/ios-configure.sh)"

# ─── 1. version audit ──────────────────────────────────────────────
version="$(node -p "require('./package.json').version")"
conf_version="$(node -p "require('./src-tauri/tauri.conf.json').version")"
if [ "$conf_version" != "$version" ]; then
  echo "::error::tauri.conf.json version=$conf_version but package.json is $version (Info.plist stamps the Tauri version)" >&2
  exit 1
fi
echo "version ok: package.json $version == tauri.conf.json $conf_version"

# ─── 2. bundle floor ───────────────────────────────────────────────
min_ios="$(node -p "require('./src-tauri/tauri.conf.json').bundle?.iOS?.minimumSystemVersion ?? 'missing'")"
if [ "$min_ios" = "missing" ]; then
  echo "::error::tauri.conf.json bundle.iOS.minimumSystemVersion is missing (need 17.0+ for NSAllowsLocalNetworking)" >&2
  exit 1
fi
major_ios="${min_ios%%.*}"
if [ "$major_ios" -lt 17 ]; then
  echo "::error::bundle.iOS.minimumSystemVersion=$min_ios but LAN cleartext needs iOS 17+ (NSAllowsLocalNetworking)" >&2
  exit 1
fi
echo "iOS floor ok: minimumSystemVersion $min_ios"

# ─── 4. identifier audit ───────────────────────────────────────────
identifier="$(node -p "require('./src-tauri/tauri.conf.json').identifier")"
if [ "$identifier" != "com.cybermanju.os" ]; then
  echo "::error::tauri.conf.json identifier=$identifier but Apple provisioning + deep links expect com.cybermanju.os" >&2
  exit 1
fi
echo "identifier ok: $identifier"

# ─── project-specific settings (only when the Xcode project exists) ─
if [ ! -d "$APPLE_DIR" ]; then
  echo "$MARK: $APPLE_DIR not found — run \`tauri ios init\` on macOS first; linker config and ATS exception apply after init (docs/IOS.md §2)"
  echo "iOS configure complete (basic checks; no Xcode project yet)"
  exit 0
fi

if [ "$(uname -s)" != "Darwin" ]; then
  echo "$MARK: Xcode project checks require macOS — skipping framework link and plist checks on $(uname -s)"
  echo "iOS configure complete"
  exit 0
fi

# ─── 3. native framework link ──────────────────────────────────────
pbxproj="$(find "$APPLE_DIR" -type f -name project.pbxproj -print -quit 2>/dev/null || true)"
if [ -z "$pbxproj" ]; then
  echo "::error::$MARK: generated Xcode project has no project.pbxproj under $APPLE_DIR" >&2
  exit 1
fi
python3 scripts/ios-link-frameworks.py "$pbxproj"

# ─── 5. ATS audit ──────────────────────────────────────────────────
plist="$(find "$APPLE_DIR" -name 'Info.plist' 2>/dev/null | head -1 || true)"
if [ -z "$plist" ]; then
  echo "$MARK: no Info.plist under $APPLE_DIR yet — add NSAppTransportSecurity/NSAllowsLocalNetworking per docs/IOS.md §2 when it appears"
  echo "iOS configure complete"
  exit 0
fi

if /usr/libexec/PlistBuddy -c 'Print :NSAppTransportSecurity:NSAllowsLocalNetworking' "$plist" >/dev/null 2>&1; then
  echo "ATS ok: NSAllowsLocalNetworking set in $plist"
else
  echo "::warning::$MARK: $plist has no NSAllowsLocalNetworking — plain-http LAN dashboard (http://nas:3456) is blocked by ATS; HTTPS via the Caddy sidecar needs no exception (docs/IOS.md §2)" >&2
fi
echo "iOS configure complete"
