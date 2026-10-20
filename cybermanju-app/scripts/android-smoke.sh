#!/usr/bin/env bash
# CyberManju OS — Android smoke (scripts/android-smoke.sh).
# Android on-device smoke test (CI, best-effort).
#
# Zero on-device verification was the gap: `apksigner verify` proves the
# signature, not that the app installs + boots. This script installs the
# built APK on a running emulator (if one is reachable) and:
#   1. `adb install` the APK (arm64 APK needs an arm64 emulator image;
#      x86_64 images cannot run it — that mismatch skips, not fails).
#   2. `adb shell am start` the main activity.
#   3. Checks the process is STILL ALIVE at 8s and 13s — the reported
#      "installs, instantly closes, shows nothing" symptom leaves no FATAL
#      marker (LMK/ANR kill), so a dead process IS the failure signal.
#   4. Greps a freshly-cleared logcat window for crash markers.
#
# Graceful skip (exit 0) when: no device/emulator attached, wrong ABI,
# missing platform-tools. CI wires this with `continue-on-error` so KVM-less
# runners stay green; the skip reason is always printed.
set -uo pipefail

cd "$(dirname "$0")/.."

APK="${1:-$(find src-tauri/gen/android/app/build/outputs/apk -name 'CyberManju-OS-*-arm64-v8a.apk' 2>/dev/null | head -1)}"
ACTIVITY="${ANDROID_ACTIVITY:-com.cybermanju.os/.MainActivity}"
# Package for pidof/ps derived from the ACTIVITY we launch (honors
# ANDROID_ACTIVITY instead of hardcoding com.cybermanju.os).
PKG="${ACTIVITY%%/*}"

skip() { echo "android smoke: SKIP — $1"; exit 0; }
fail() { echo "::error::android smoke: $1" >&2; exit 1; }

command -v adb > /dev/null || skip "adb not installed"
[ -n "$APK" ] && [ -f "$APK" ] || skip "no built APK found (build first)"
if ! adb devices | awk 'NR>1 && $2=="device" {found=1} END {exit !found}'; then
  skip "no emulator/device attached (start one with an arm64-v8a system image)"
fi

# ABI check: an arm64 APK cannot install on an x86_64-only emulator.
abi="$(adb shell getprop ro.product.cpu.abi 2>/dev/null | tr -d '\r' || echo unknown)"
case "$abi" in
  arm64-v8a) echo "device ABI: $abi (compatible)" ;;
  *) skip "device ABI is '$abi' — arm64 APK needs an arm64-v8a image" ;;
esac

adb install -r "$APK" || fail "adb install failed for $APK"
# Fresh logcat window so stale crash lines from other apps can't fail us
# (and our own markers don't drown in hours of radio noise).
adb logcat -c 2>/dev/null || true
adb shell am start -n "$ACTIVITY" || fail "am start failed for $ACTIVITY"

# Process pid, `pidof` first with a `ps` fallback (very old images).
app_pid() {
  local p
  p="$(adb shell pidof "$PKG" 2>/dev/null | tr -d '\r' || true)"
  if [ -n "$p" ]; then echo "$p"; return 0; fi
  adb shell ps -A 2>/dev/null | awk -v p="$PKG" 'index($0, p) {print $2}' | tr -d '\r' | head -1 || true
}

dump_markers() {
  adb logcat -d 2>/dev/null | grep -iE "FATAL EXCEPTION|AndroidRuntime|has died|Force finishing activity|CyberManju OS FATAL|Fatal error while running|not writable|quarantining" | head -20 >&2 || true
}

sleep 8
# Liveness is the load-bearing check: an instant close (panic before first
# paint, ANR kill, LMK kill) often leaves NO fatal marker — a dead process
# after a successful `am start` is itself the failure.
if [ -z "$(app_pid)" ]; then
  dump_markers
  fail "process com.cybermanju.os is not running 8s after launch (instant close — see markers above)"
fi
sleep 5
if [ -z "$(app_pid)" ]; then
  dump_markers
  fail "process com.cybermanju.os died between 8s and 13s after launch (ANR/LMK kill — see markers above)"
fi
if adb logcat -d 2>/dev/null | grep -qiE "FATAL EXCEPTION.*com\.cybermanju\.os|AndroidRuntime.*com\.cybermanju\.os|CyberManju OS FATAL|Fatal error while running"; then
  dump_markers
  fail "crash markers in logcat after launch"
fi
echo "android smoke: PASS — installed + launched $APK on $abi (alive at 13s)"
