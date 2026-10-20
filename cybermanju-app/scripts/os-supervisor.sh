#!/usr/bin/env bash
# CyberManju OS — supervisor.
#
# Spawns three opencode agents (AGENT-6/7/8) against this worktree and loops
# until the CyberManju OS vision is 100%: every checkbox in the three briefs
# ticked, the workspace gate green, and scripts/os-acceptance.sh passing.
#
#   nohup bash scripts/os-supervisor.sh > logs/supervisor.log 2>&1 &
#
# Per iteration it:
#   1. guards free disk (this box is tight — one shared target/)
#   2. launches/relaunches any idle agent that still has work
#   3. watchdog-kills an agent whose log has not moved for STALL_SECS
#   4. when everyone is idle: runs the serial gate, then acceptance
#   5. commits + pushes to main after every green gate
#   6. feeds gate/acceptance failures back into the next prompt
#
# Agents never run git — only this script does.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export ORT_CACHE_DIR="${ORT_CACHE_DIR:-$HOME/.cache/ort}"

LOGDIR="$ROOT/logs"
mkdir -p "$LOGDIR"

AGENTS=(6 7 8)
STALL_SECS=900          # 15 min with no new log bytes → kill + relaunch
TICK_SECS=30
COOLDOWN_SECS=60        # min gap between launches of the same agent
MIN_FREE_MB=700         # last-resort halt; the janitor keeps us far above this
INSTANT_FAIL_LIMIT=6    # agent exited <60s after launch this many times → halt
GATE_ROUNDS_MAX=25      # safety valve on the outer loop

TS() { date '+%Y-%m-%d %H:%M:%S'; }
log() { echo "[$(TS)] $*" | tee -a "$LOGDIR/supervisor.log"; }

brief()      { echo "$ROOT/AGENT-$1.md"; }
agent_log()  { echo "$LOGDIR/agent-$1.log"; }
alive() {
  local pidf="$LOGDIR/agent-$1.pid" pid
  [ -f "$pidf" ] || return 1
  pid=$(cat "$pidf" 2>/dev/null)
  [ -n "$pid" ] || return 1
  kill -0 "$pid" 2>/dev/null
}
remaining()  { local c; c=$(grep -c '^- \[ \]' "$(brief "$1")" 2>/dev/null); echo "${c:-0}"; }

# Free space guard — one shared target/ for three agents on a 5.5 GB box.
disk_guard() {
  local kb; kb=$(df -Pk "$ROOT" 2>/dev/null | awk 'NR==2{print $4}')
  local mb=$(( ${kb:-0} / 1024 ))
  if [ "$mb" -lt "$MIN_FREE_MB" ]; then
    log "HALT: only ${mb}MB free (< ${MIN_FREE_MB}MB). Free space, then re-run."
    exit 70
  fi
}

# Unchecked boxes, verbatim, so the next prompt is a concrete to-do list.
todo_list() {
  grep '^- \[ \]' "$(brief "$1")" 2>/dev/null | sed 's/^- \[ \] /  - /' | head -40
}

scope_of() {
  case "$1" in
    6) echo ".cybermanju disks, block allocator, volume merge, mount, df, admission control" ;;
    7) echo "scrub, repair, Reed-Solomon erasure coding, catalog replication, provider health, chunk GC, multi-writer leases" ;;
    8) echo "cybsh system terminal, ps/top task table, compute fan-out, syscall boundary, UI, docs" ;;
  esac
}

# Build the prompt for one agent round.
build_prompt() {
  local n="$1" extra="${2:-}"
  cat <<EOF
You are AGENT-$n in a three-agent parallel push that is building the CyberManju
OS. The attached AGENT-$n.md is your brief — read it first, then work through
EVERY unchecked "- [ ]" box until none remain, ticking each one in the file as
you finish it.

Your scope: $(scope_of "$n")

STILL OPEN IN YOUR BRIEF:
$(todo_list "$n")
$extra

GROUND RULES (violating these breaks the other two agents):
- Three agents share this worktree. Edit ONLY the files under "Files you own"
  in your brief. The supervisor pre-wired every shared file (lib.rs,
  security.rs, api/mod.rs, database.rs, root Cargo.toml, tests/lib.rs) —
  those are read-only to you. Need something from one? write it under
  "Requests to the supervisor" at the bottom of your brief.
- NEVER run git. The supervisor commits and pushes after each green gate.
- Before any cargo command: export ORT_CACHE_DIR="\$HOME/.cache/ort"
  (otherwise ort-sys panics on this machine).
- Verify your own work as you go:
    cargo fmt --all -- --check
    cargo clippy -p <your-crate> --all-targets -- -D warnings
    cargo test  -p <your-crate>
  and for the frontend (agent 8): npm run typecheck && npm run lint
- Everything must be REAL: no mocks, no placeholders, no "estimated" returns.
  If something cannot work here, return Err("unsupported: ...") rather than
  faking success.
- Long-running work (scrub, repair, compute) must be spawned as a background
  task — HTTP handlers must never block on it.

When every box in your brief is ticked and your crate's tests pass, tick the
Definition of done boxes and append a dated line to your Log section.
EOF
}

LAUNCH_STAMP=()
INSTANT_FAILS=()

launch() {
  local n="$1" extra="${2:-}"
  local logf; logf=$(agent_log "$n")
  local promptf="$LOGDIR/prompt-$n.txt"
  build_prompt "$n" "$extra" > "$promptf"

  log "launch AGENT-$n (open todos: $(remaining "$n")) -> $(basename "$logf")"
  # --auto: headless must never block on an `ask` permission.
  nohup opencode run --auto --title "cybermanju-agent-$n" \
    "$(cat "$promptf")" -f "AGENT-$n.md" >> "$logf" 2>&1 &
  local pid=$!
  echo "$pid" > "$LOGDIR/agent-$n.pid"
  LAUNCH_STAMP[$n]=$(date +%s)
  echo "" >> "$logf"; log "  pid $pid"
}

kill_agent() {
  local n="$1" why="$2" pid
  log "kill AGENT-$n: $why"
  pid=$(cat "$LOGDIR/agent-$n.pid" 2>/dev/null)
  if [ -n "$pid" ]; then
    kill "$pid" 2>/dev/null; sleep 3
    kill -9 "$pid" 2>/dev/null; sleep 2
  fi
  # opencode forks a child worker; sweep anything still holding this title.
  pkill -f "cybermanju-agent-$n" 2>/dev/null
  sleep 2
}

# Has this agent's log been idle too long?
stalled() {
  local n="$1" logf; logf=$(agent_log "$n")
  [ -f "$logf" ] || return 1
  local age=$(( $(date +%s) - $(stat -c %Y "$logf" 2>/dev/null || date +%s) ))
  [ "$age" -gt "$STALL_SECS" ]
}

# ── Gate ──────────────────────────────────────────────────────────────────────
# Serial on purpose: cargo locks target/ anyway, and one log per round is
# readable. Returns 0 only if all five checks are green.
gate() {
  local n="$1" out="$LOGDIR/gate-$n.log"
  log "gate round $n"
  {
    echo "=== fmt ===";      cargo fmt --all -- --check; echo "FMT=$?"
    echo "=== clippy ===";   cargo clippy --workspace --all-targets -- -D warnings; echo "CLIPPY=$?"
    echo "=== test ===";     cargo test --workspace --no-fail-fast; echo "TEST=$?"
    echo "=== typecheck ==="; npm run typecheck; echo "TYPECHECK=$?"
    echo "=== lint ===";     npm run lint; echo "LINT=$?"
  } > "$out" 2>&1
  local bad
  bad=$(grep -E '^(FMT|CLIPPY|TEST|TYPECHECK|LINT)=[1-9]' "$out" | tr '\n' ' ')
  if [ -z "$bad" ]; then log "gate round $n: GREEN"; return 0; fi
  log "gate round $n: RED ($bad)"
  GATE_FAIL_SNIPPET=$(grep -E '^(error|warning: .*generated|---- .* ----|failures:)|^(FMT|CLIPPY|TEST|TYPECHECK|LINT)=[1-9]' "$out" | tail -60)
  return 1
}

acceptance() {
  log "acceptance run"
  if bash "$ROOT/scripts/os-acceptance.sh" all > "$LOGDIR/acceptance.log" 2>&1; then
    log "acceptance: GREEN"; return 0
  fi
  log "acceptance: RED"
  ACC_FAIL_SNIPPET=$(grep -E '^(FAIL|[0-9]+\.)' "$LOGDIR/acceptance.log" | tail -40)
  return 1
}

# Commit + push to main after every green gate. Retry around .git/index.lock —
# three agents are writing files and nothing else in this repo runs git.
commit_and_push() {
  local msg="$1"
  for _ in $(seq 1 30); do
    if git add -A 2>/dev/null && git commit -q -m "$msg" 2>/dev/null; then
      log "committed: $msg"
      for _ in $(seq 1 10); do
        git push origin main >> "$LOGDIR/push.log" 2>&1 && { log "pushed to origin/main"; return 0; }
        sleep 5
      done
      log "WARN: push failed (see logs/push.log) — continuing locally"
      return 1
    fi
    sleep 2
  done
  log "WARN: could not commit (index.lock stuck?)"
  return 1
}

# `os-supervisor.sh --prompt N` prints the prompt for one agent without
# launching anything (used to sanity-check the brief wiring).
if [ "${1:-}" = "--prompt" ]; then
  build_prompt "${2:?agent number}" ""
  exit 0
fi

# ── Main loop ────────────────────────────────────────────────────────────────
# Refuse to start twice — duplicate supervisors double-launch the agents.
LOCK="$LOGDIR/supervisor.lock"
if [ -f "$LOCK" ] && kill -0 "$(cat "$LOCK" 2>/dev/null)" 2>/dev/null; then
  echo "supervisor already running (pid $(cat "$LOCK")); exiting" >&2
  exit 73
fi
echo $$ > "$LOCK"
trap 'rm -f "$LOCK"' EXIT

log "=== supervisor start (pid $$) ==="
log "workspace: $ROOT"
log "briefs: $(for n in "${AGENTS[@]}"; do printf 'AGENT-%s:%s-open ' "$n" "$(remaining "$n")"; done)"

ROUNDS=0
GATE_ROUNDS=0
while :; do
  disk_guard

  # 1. watchdog + launch
  local_pending=0
  for n in "${AGENTS[@]}"; do
    if alive "$n"; then
      if stalled "$n"; then
        kill_agent "$n" "no log output for ${STALL_SECS}s"
        launch "$n" "Your previous run stalled with no output for 15 minutes. Resume from the open boxes listed above; if you had started something, finish that first."
        local_pending=1
      fi
      continue
    fi

    # exited — decide whether to relaunch
    rem=$(remaining "$n")
    extra=""
    if [ "$rem" -gt 0 ]; then
      extra="You exited with $rem box(es) still open — they are listed above. Resume and finish them."
    elif [ -n "${GATE_FAIL_SNIPPET:-}${ACC_FAIL_SNIPPET:-}" ]; then
      extra="Your brief is fully ticked, but verification failed. Fix ONLY the parts that belong to your files:

${GATE_FAIL_SNIPPET:-}
${ACC_FAIL_SNIPPET:-}"
    else
      continue    # done and nothing to fix
    fi

    # instant-failure circuit breaker
    st=${LAUNCH_STAMP[$n]:-0}
    if [ "$st" -gt 0 ] && [ $(( $(date +%s) - st )) -lt "$COOLDOWN_SECS" ]; then
      local_pending=1; continue
    fi
    if [ "$st" -gt 0 ] && [ $(( $(date +%s) - st )) -lt 120 ]; then
      INSTANT_FAILS[$n]=$(( ${INSTANT_FAILS[$n]:-0} + 1 ))
      if [ "${INSTANT_FAILS[$n]}" -ge "$INSTANT_FAIL_LIMIT" ]; then
        log "HALT: AGENT-$n exited immediately ${INSTANT_FAIL_LIMIT}x — see $(agent_log "$n")"
        exit 71
      fi
    fi
    launch "$n" "$extra"
    local_pending=1
  done

  # 2. everyone idle with no pending launch → verify
  any_alive=0
  for n in "${AGENTS[@]}"; do alive "$n" && any_alive=1; done
  if [ "$any_alive" -eq 0 ] && [ "$local_pending" -eq 0 ]; then
    open_total=0
    for n in "${AGENTS[@]}"; do open_total=$(( open_total + $(remaining "$n") )); done
    if [ "$open_total" -eq 0 ]; then
      GATE_ROUNDS=$((GATE_ROUNDS + 1))
      GATE_FAIL_SNIPPET=""
      ACC_FAIL_SNIPPET=""

      if [ "$GATE_ROUNDS" -gt "$GATE_ROUNDS_MAX" ]; then
        log "HALT: $GATE_ROUNDS gate rounds without success — needs a human"
        exit 72
      fi
      if gate "$GATE_ROUNDS"; then
        GATE_FAIL_SNIPPET=""
        commit_and_push "gate $GATE_ROUNDS green: workspace fmt/clippy/test/typecheck/lint"
        if acceptance; then
          commit_and_push "cybermanju OS: acceptance tiers 0-2 pass (disks, durability, terminal)"
          log "=== 100% — all briefs ticked, gate green, acceptance green ==="
          exit 0
        fi
      fi
      # failures are now in *_FAIL_SNIPPET → the launch block feeds them back
    else
      log "waiting: $open_total open boxes, agents idle"
    fi
  fi

  ROUNDS=$((ROUNDS + 1))
  sleep "$TICK_SECS"
done
