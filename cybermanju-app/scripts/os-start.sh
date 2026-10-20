#!/usr/bin/env bash
# CyberManju OS — Os start (scripts/os-start.sh).
# Start (or restart) the CyberManju OS supervisor, fully detached from the
# shell that launched it — a tool-call interrupt must not take the three
# agents down with it.
#
#   bash scripts/os-start.sh
#
# Returns immediately; everything it spawns has stdin/stdout/stderr pointing
# at files under logs/, never at this terminal.

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
mkdir -p logs

KEEP=0
if [ "${1:-}" = "--keep" ]; then KEEP=1; fi

# Drop a stale lock from a supervisor that was killed mid-flight.
if [ -f logs/supervisor.lock ]; then
  old=$(cat logs/supervisor.lock 2>/dev/null)
  if [ -n "$old" ] && kill -0 "$old" 2>/dev/null; then
    echo "supervisor already running (pid $old)"
    exit 0
  fi
  rm -f logs/supervisor.lock
fi

# Kill any orphaned agent from a previous supervisor so we never run 6 agents.
# `--keep` skips this: used to revive a supervisor that halted on the disk
# guard while its agents were still (correctly) working.
[ "$KEEP" = 1 ] && { setsid bash scripts/os-supervisor.sh >> logs/supervisor.log 2>&1 < /dev/null & exit 0; }
for n in 6 7 8; do
  [ -f "logs/agent-$n.pid" ] || continue
  p=$(cat "logs/agent-$n.pid" 2>/dev/null)
  [ -n "$p" ] && kill "$p" 2>/dev/null
done
sleep 1
pkill -f "cybermanju-agent-" 2>/dev/null
sleep 1
rm -f logs/agent-*.pid

: > logs/supervisor.log

# setsid + full redirection = new session, no inherited fds, no SIGHUP.
setsid bash scripts/os-supervisor.sh >> logs/supervisor.log 2>&1 < /dev/null &

for _ in $(seq 1 20); do
  if [ -f logs/supervisor.lock ]; then
    echo "supervisor started (pid $(cat logs/supervisor.lock))"
    exit 0
  fi
  sleep 0.2
done
echo "FAILED to start — see logs/supervisor.log" >&2
exit 1
