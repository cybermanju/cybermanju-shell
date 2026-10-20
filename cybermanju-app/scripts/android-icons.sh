#!/usr/bin/env bash
# CyberManju OS — Android icons (scripts/android-icons.sh).
# Android launcher icon sync for CyberManju OS.
#
# Root cause it fixes: `tauri android init` stamps the Tauri template's
# DEFAULT launcher icons into `gen/android/app/src/main/res/`, and nothing
# ever replaced them — every APK/AAB shipped the Tauri logo as its launcher
# icon (reported 2026-10-08). The official Tauri flow is to run
# `tauri icon <source>` AFTER `tauri android init`
# (https://v2.tauri.app/distribute/google-play/#changing-app-icon).
#
# What this does:
#   1. Runs `npx tauri icon <source>` into a throwaway temp dir (NOT the
#      default `src-tauri/icons` output, so no tracked desktop icon is
#      touched — the working tree stays clean).
#   2. Copies only the generated `android/` subtree into the generated
#      project's `app/src/main/res/` (mipmap-{mdpi,hdpi,xhdpi,xxhdpi,
#      xxxhdpi} + mipmap-anydpi-v26 + values/ic_launcher_background.xml).
#   3. Verifies every expected file exists, is non-empty, and that the
#      foreground PNGs carry the documented densities (108/162/216/324/432).
#
# Source of truth: `public/icon-512.png` (512x512 RGBA — the canonical app
# mark; `public/bhumisparsha.png` is 250x280 non-square and unsuitable).
#
# Usage: bash scripts/android-icons.sh [gen/android-dir] [source-icon]
# Idempotent: re-running overwrites the same bytes.
set -euo pipefail

cd "$(dirname "$0")/.."

ANDROID_DIR="${1:-src-tauri/gen/android}"
SRC_ICON="${2:-public/icon-512.png}"
RES="$ANDROID_DIR/app/src/main/res"

if [ ! -d "$ANDROID_DIR" ]; then
  echo "::error::$ANDROID_DIR not found — run \`tauri android init\` first" >&2
  exit 1
fi
if [ ! -f "$SRC_ICON" ]; then
  echo "::error::source icon $SRC_ICON not found" >&2
  exit 1
fi
command -v npx >/dev/null || { echo "::error::npx not found (install Node.js first)" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "generating launcher icons from $SRC_ICON ..."
npx tauri icon "$SRC_ICON" --output "$TMP/icons-out" >/dev/null

GEN="$TMP/icons-out/android"
if [ ! -d "$GEN" ]; then
  echo "::error::tauri icon produced no android/ subtree in $TMP/icons-out" >&2
  exit 1
fi

mkdir -p "$RES"
# Copy, never delete: the template's res/ holds files outside the icon set
# (strings.xml, styles.xml, …) that a --delete sync would destroy.
cp -r "$GEN/." "$RES/"

# ─── Verify ─────────────────────────────────────────────────────
missing=0
for d in mdpi hdpi xhdpi xxhdpi xxxhdpi; do
  for f in ic_launcher.png ic_launcher_round.png ic_launcher_foreground.png; do
    p="$RES/mipmap-$d/$f"
    if [ ! -s "$p" ]; then
      echo "::error::missing or empty $p after icon sync" >&2
      missing=1
    fi
  done
done
for p in "$RES/mipmap-anydpi-v26/ic_launcher.xml" "$RES/values/ic_launcher_background.xml"; do
  if [ ! -s "$p" ]; then
    echo "::error::missing or empty $p after icon sync" >&2
    missing=1
  fi
done
[ "$missing" -eq 0 ] || exit 1

# Foreground densities must match the Tauri-documented sizes
# (mdpi 108, hdpi 162, xhdpi 216, xxhdpi 324, xxxhdpi 432); anything else
# means the generator silently changed layout and the copy above is stale.
node - "$RES" <<'NODEEOF'
const fs = require('fs');
const path = require('path');
const res = process.argv[2];
const expected = { mdpi: 108, hdpi: 162, xhdpi: 216, xxhdpi: 324, xxxhdpi: 432 };
let bad = 0;
for (const [d, size] of Object.entries(expected)) {
  const p = path.join(res, `mipmap-${d}`, 'ic_launcher_foreground.png');
  const b = fs.readFileSync(p);
  const w = b.readUInt32BE(16), h = b.readUInt32BE(20);
  console.log(`mipmap-${d}/ic_launcher_foreground.png: ${w}x${h}`);
  if (w !== size || h !== size) {
    console.error(`::error::${p} is ${w}x${h}, expected ${size}x${size}`);
    bad = 1;
  }
}
process.exit(bad);
NODEEOF

echo "android launcher icons synced from $SRC_ICON into $RES"
