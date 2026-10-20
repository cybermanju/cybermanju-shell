#!/usr/bin/env bash
# CyberManju OS — Os disk janitor (scripts/os-disk-janitor.sh).
# Keep the shared build cache from filling the disk while three agents build.
#
#   setsid bash scripts/os-disk-janitor.sh >> logs/janitor.log 2>&1 </dev/null &
#
# Every MINUTE_MIN it drops target/debug/incremental (a pure rebuild-speed
# cache — cargo regenerates it) when free space falls under FREE_MIN_MB, but
# never while a cargo build is in flight. Refuses to run twice.

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

LOCK="$ROOT/logs/janitor.lock"
if [ -f "$LOCK" ] && kill -0 "$(cat "$LOCK" 2>/dev/null)" 2>/dev/null; then
  echo "janitor already running (pid $(cat "$LOCK"))"
  exit 0
fi
echo $$ > "$LOCK"
trap 'rm -f "$LOCK"' EXIT

FREE_MIN_MB=${FREE_MIN_MB:-1600}
INTERVAL_S=${INTERVAL_S:-300}

log() { echo "[$(date '+%F %T')] $*"; }

log "janitor started (min free ${FREE_MIN_MB}MB, every ${INTERVAL_S}s)"
while :; do
  sleep "$INTERVAL_S"
  free=$(df -Pk "$ROOT" | awk 'NR==2{print int($4/1024)}')
  if [ "$free" -ge "$FREE_MIN_MB" ]; then
    continue
  fi
  if pgrep -x cargo >/dev/null 2>&1 && [ "$free" -ge 900 ]; then
    log "free ${free}MB but cargo is building — skipping this round"
    continue
  fi
  before=$free
  rm -rf "$ROOT/target/debug/incremental"
  after=$(df -Pk "$ROOT" | awk 'NR==2{print int($4/1024)}')
  log "free was ${before}MB (< ${FREE_MIN_MB}) — dropped incremental cache, now ${after}MB"
done
