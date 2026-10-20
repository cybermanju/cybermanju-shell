#!/usr/bin/env bash
# CyberManju OS — acceptance gate.
#
# Proves the vision end-to-end. Three tiers, matching MISSING.md:
#   Tier 0 — .cybermanju disks with choosable sizes merge into one df (AGENT-6)
#   Tier 1 — the pool survives provider loss, corruption and DB loss (AGENT-7)
#   Tier 2 — terminal + tasks + compute + auth (AGENT-8)
#
# Exit 0 = 100%. Non-zero = the exact missing piece, printed on stdout, which
# the supervisor feeds straight back into the owning agent's next prompt.
#
# Usage: scripts/os-acceptance.sh [0|1|2]     (default: all tiers)

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export ORT_CACHE_DIR="${ORT_CACHE_DIR:-$HOME/.cache/ort}"

FAILURES=()
fail() { FAILURES+=("$1"); echo "FAIL  $1"; }
ok()   { echo "  ok  $1"; }

want_tests() { # want_tests <label> <expected-min> <cargo-test-args...>
  local label="$1" min="$2"; shift 2
  local out count
  out=$(cargo test "$@" 2>&1)
  local st=$?
  count=$(grep -Eo '[0-9]+ passed' <<<"$out" | grep -Eo '[0-9]+' | awk '{s+=$1} END {print s+0}')
  count=${count:-0}
  if [ "$st" -ne 0 ]; then
    fail "$label: cargo test exited $st (see output below)"
    echo "$out" | tail -25 | sed 's/^/        /'
    return 1
  fi
  if [ "$count" -lt "$min" ]; then
    fail "$label: only $count tests, need >= $min — the feature is not actually tested"
    return 1
  fi
  ok "$label ($count tests)"
}

want_symbol() { # want_symbol <label> <file-glob> <grep-pattern>
  if grep -rqE -- "$3" $2 2>/dev/null; then ok "$1"; else fail "$1 — no match for '$3' in $2"; fi
}

# ─── Tier 0 — the disk exists, is sized, and merges ──────────────────────────
tier0() {
  echo "── TIER 0 — .cybermanju disk & merged volume (AGENT-6)"
  want_tests "crates/disk suite"            5 -p cybermanju-disk
  want_tests "crates/tests disk module"     2 -p cybermanju-tests disk
  want_symbol "superblock magic CYBMJU1"        "crates/disk/src/*.rs"  'CYBMJU1'
  want_symbol "adjustable capacity (capacity_bytes)" "crates/disk/src/*.rs" 'capacity_bytes'
  want_symbol "volume df (merged total/used/free)"   "crates/disk/src/*.rs" '(fn df|free_bytes)'
  want_symbol "admission control refuses overflow"   "crates/disk/src/*.rs" '(disk full|admit)'
  want_symbol "spanned placement"                   "crates/disk/src/*.rs" '(span|high.?water|spill)'
  want_symbol "disk lifecycle create/attach/resize"  "crates/disk/src/*.rs" 'fn (create|attach|resize)'
  want_symbol "HTTP block API route"                "crates/web/src/api/disk_api.rs" '(volume/block|/api/disk|block_api)'
}

# ─── Tier 1 — the pool heals itself ──────────────────────────────────────────
tier1() {
  echo "── TIER 1 — durability, repair & catalog replication (AGENT-7)"
  want_tests "crates/erasure suite"        3 -p cybermanju-erasure
  want_tests "crates/tests repair module"  2 -p cybermanju-tests repair
  want_tests "crates/sync suite"          20 -p cybermanju-sync
  want_symbol "scrubber exists"            "crates/sync/src/*.rs" 'fn (run_)?scrub'
  want_symbol "repair/rebuild exists"      "crates/sync/src/*.rs" 'fn (run_)?repair'
  want_symbol "rebuild-from-remote exists" "crates/sync/src/*.rs" 'rebuild_from_remote'
  want_symbol "chunk GC exists"            "crates/sync/src/*.rs" 'fn (run_)?gc\b|collect_garbage'
  want_symbol "multi-writer lease"         "crates/sync/src/*.rs" 'fn .*lease'
  want_symbol "provider health"            "crates/sync/src/*.rs" 'health'
  want_symbol "repair routes"              "crates/web/src/api/repair_api.rs" '(scrub|repair|lease)'
}

# ─── Tier 2 — it is an OS ────────────────────────────────────────────────────
tier2() {
  echo "── TIER 2 — terminal, tasks, compute, UI (AGENT-8)"
  want_tests "crates/os suite"            5 -p cybermanju-os
  want_tests "crates/tests os module"     2 -p cybermanju-tests os
  want_symbol "cybsh shell interpreter"   "crates/os/src/*.rs"        '(fn exec|fn run_command|struct Shell)'
  want_symbol "system commands (df/ps/disk)" "crates/os/src/*.rs"     '"(df|ps|top|disk)"'
  want_symbol "task table (ps/top/kill)"  "crates/os/src/*.rs"        '(fn (tasks|ps|top|kill))'
  want_symbol "compute fan-out"           "crates/os/src/*.rs"        '(fan.?out|parallel|workers)'
  want_symbol "syscall boundary"          "crates/os/src/api.rs"      'fn (open|read|readdir|df)'
  want_symbol "terminal UI component"     "src/components/*.vue"      '(Terminal|cybsh)'
  want_symbol "disk manager UI"           "src/components/*.vue"      '(DiskManager|volume_df|attachDisk)'
  want_symbol "os exec route"             "crates/web/src/api/os_api.rs" '(os/exec|/api/os)'

  echo "  ..  typecheck + lint"
  if npm run typecheck > logs/acc-typecheck.log 2>&1; then ok "npm run typecheck"; else
    fail "npm run typecheck"; tail -15 logs/acc-typecheck.log | sed 's/^/        /'; fi
  if npm run lint > logs/acc-lint.log 2>&1; then ok "npm run lint"; else
    fail "npm run lint"; tail -15 logs/acc-lint.log | sed 's/^/        /'; fi

  live_auth_probe
}

# Boot the real HTTP server and prove the new surfaces are auth-gated — the
# same property crates/tests/src/web.rs pins for the older families.
live_auth_probe() {
  echo "  ..  live REST auth probe"
  if ! cargo build -p cybermanju-os-server > logs/acc-build-server.log 2>&1; then
    fail "could not build cybermanju-os-server"; tail -10 logs/acc-build-server.log | sed 's/^/        /'
    return
  fi
  local tmp; tmp=$(mktemp -d)
  local port=$(( 3500 + RANDOM % 400 ))
  PORT="$port" DB_PATH="$tmp/acc.redb" STATIC_DIR="$tmp" RUST_LOG=warn \
    ./target/debug/cybermanju-os-server > logs/acc-server.log 2>&1 &
  local srv=$!
  local up=0
  for _ in $(seq 1 40); do
    curl -fsS "http://127.0.0.1:$port/api/health" >/dev/null 2>&1 && { up=1; break; }
    sleep 0.5
  done
  if [ "$up" -ne 1 ]; then
    fail "server did not come up on :$port"; tail -10 logs/acc-server.log | sed 's/^/        /'
    kill "$srv" 2>/dev/null; rm -rf "$tmp"; return
  fi
  ok "server up on :$port"

  probe() { # probe <path> <method> <expected>
    local code
    code=$(curl -s -o /dev/null -w '%{http_code}' -X "$2" "http://127.0.0.1:$port$1")
    if [ "$code" = "$3" ]; then ok "$2 $1 -> $code"; else
      fail "$2 $1 -> $code (expected $3)"; fi
  }
  probe /api/health       GET  200
  probe /api/os/exec      POST 401
  probe /api/disk/list    GET  401
  probe /api/volume/df    GET  401
  probe /api/repair/status GET 401
  probe /api/scrub/runs   GET  401

  kill "$srv" 2>/dev/null; wait "$srv" 2>/dev/null; rm -rf "$tmp"
}

TIER="${1:-all}"
case "$TIER" in
  0) tier0 ;;
  1) tier1 ;;
  2) tier2 ;;
  all) tier0; tier1; tier2 ;;
  *) echo "usage: $0 [0|1|2]"; exit 2 ;;
esac

echo
if [ "${#FAILURES[@]}" -eq 0 ]; then
  echo "ACCEPTANCE PASSED — tier '$TIER'"
  exit 0
fi
echo "ACCEPTANCE FAILED — ${#FAILURES[@]} finding(s):"
i=1; for f in "${FAILURES[@]}"; do printf '  %d. %s\n' "$i" "$f"; i=$((i+1)); done
exit 1
