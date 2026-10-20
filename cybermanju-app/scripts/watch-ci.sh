#!/usr/bin/env bash
# CyberManju OS — CI watcher with full step logs.
#
# Polls the GitHub Actions CI run for the current branch/SHA and, once it
# completes, downloads the FULL logs of all jobs/steps (not just the
# one-line conclusions) into a logs directory.
#
# Runnable three ways:
#   1. Standalone:   scripts/watch-ci.sh [--branch main] [--sha HEAD]
#   2. After push:   ./push.sh --watch            # pushes, then watches HEAD
#   3. Detached:     scripts/watch-ci.sh --run <run-id>
#
# Compatibility notes (see AGENTS.md §3):
# - Old `gh` (2.4.0) has no `run view --branch` and weak `run view` status,
#   so ALL state is read via `gh api repos/.../actions/runs/...` + `/jobs`,
#   which that version already supports.
# - When `gh` is absent (or unauthenticated) the script falls back to
#   `curl` + `$GH_TOKEN`/`$GITHUB_TOKEN`, and to `python3` (stdlib only)
#   for JSON parsing and zip extraction — no `jq`/`unzip` required.
#
# Exit codes: 0 = success (or skipped/prose-only), 1 = failed/cancelled/
# timed-out run, 2 = watcher usage/infra error (no run found, no auth).
set -euo pipefail

cd "$(dirname "$0")/.."

REPO="${GH_REPO:-}"
BRANCH="main"
WORKFLOW="ci.yml"
SHA=""
RUN_ID=""
INTERVAL=20
TIMEOUT=3600
WAIT_FOR_RUN=180
LOGS_DIR=""
NO_SAVE=0
PRINT_MODE="failed"   # none|failed|all
FOLLOW=1

usage() {
  cat <<'EOF'
usage: scripts/watch-ci.sh [options]

options:
  --repo OWNER/REPO        GitHub repo (default: parsed from git remote origin,
                           or $GH_REPO; e.g. cybermanju/cybermanju.github.io)
  --branch NAME            branch whose latest CI run to watch (default: main)
  --workflow FILE          workflow file (default: ci.yml)
  --sha REV                pin to the run for this commit (default: none;
                           push.sh --watch passes HEAD; beats --branch)
  --run ID                 watch this run id directly (beats --sha/--branch)
  --interval SECS          poll interval (default: 20)
  --timeout SECS           give up after SECS while run is active (default: 3600)
  --wait-for-run SECS      wait up to SECS for a run to appear after push
                           (default: 180; prose-only pushes start no run)
  --logs-dir DIR           where to save full logs (default: logs/ci-<run-id>)
  --no-save                do not save log files, only print
  --print MODE             what to print inline: failed|all|none (default: failed)
  --failed-only            shorthand for --print failed (the default)
  --full                   shorthand for --print all
  --no-follow              check once: dump logs if completed, else exit 2
  -h, --help               this help

env:
  GH_REPO, GH_TOKEN / GITHUB_TOKEN (curl fallback when gh is missing),
  PUSH_WATCH_* is not read here — see push.sh --watch.

examples:
  scripts/watch-ci.sh                              # latest main run, save + print failures
  scripts/watch-ci.sh --sha HEAD --full            # pin to HEAD, print everything
  scripts/watch-ci.sh --run 123456789 --no-follow  # one-shot log dump
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --repo) REPO="${2:-}"; shift 2 ;;
    --repo=*) REPO="${1#--repo=}"; shift ;;
    --branch) BRANCH="${2:-}"; shift 2 ;;
    --branch=*) BRANCH="${1#--branch=}"; shift ;;
    --workflow) WORKFLOW="${2:-}"; shift 2 ;;
    --workflow=*) WORKFLOW="${1#--workflow=}"; shift ;;
    --sha) SHA="${2:-}"; shift 2 ;;
    --sha=*) SHA="${1#--sha=}"; shift ;;
    --run) RUN_ID="${2:-}"; shift 2 ;;
    --run=*) RUN_ID="${1#--run=}"; shift ;;
    --interval) INTERVAL="${2:-}"; shift 2 ;;
    --interval=*) INTERVAL="${1#--interval=}"; shift ;;
    --timeout) TIMEOUT="${2:-}"; shift 2 ;;
    --timeout=*) TIMEOUT="${1#--timeout=}"; shift ;;
    --wait-for-run) WAIT_FOR_RUN="${2:-}"; shift 2 ;;
    --wait-for-run=*) WAIT_FOR_RUN="${1#--wait-for-run=}"; shift ;;
    --logs-dir) LOGS_DIR="${2:-}"; shift 2 ;;
    --logs-dir=*) LOGS_DIR="${1#--logs-dir=}"; shift ;;
    --no-save) NO_SAVE=1; shift ;;
    --print) PRINT_MODE="${2:-}"; shift 2 ;;
    --print=*) PRINT_MODE="${1#--print=}"; shift ;;
    --failed-only) PRINT_MODE="failed"; shift ;;
    --full) PRINT_MODE="all"; shift ;;
    --no-follow) FOLLOW=0; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "watch-ci.sh: unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

case "$PRINT_MODE" in
  none|failed|all) ;;
  *) echo "watch-ci.sh: --print must be none|failed|all" >&2; exit 2 ;;
esac

if [ -z "$REPO" ]; then
  origin_url="$(git remote get-url origin 2>/dev/null || true)"
  # Handles git@github.com:OWNER/REPO(.git) and https://github.com/OWNER/REPO(.git)
  REPO="$(printf '%s' "$origin_url" \
    | sed -n -e 's#.*github\.com[:/]\(.*/.*\)\(\.git\)\?$#\1#p' | head -1)"
  if [ -z "$REPO" ]; then
    echo "watch-ci.sh: cannot determine repo (no --repo, origin is '$origin_url')" >&2
    exit 2
  fi
fi

HAVE_GH=0
if command -v gh > /dev/null 2>&1 && gh auth status > /dev/null 2>&1; then
  HAVE_GH=1
fi
TOKEN="${GH_TOKEN:-${GITHUB_TOKEN:-}}"

if [ "$HAVE_GH" -eq 0 ] && [ -z "$TOKEN" ]; then
  echo "watch-ci.sh: no authenticated gh and no \$GH_TOKEN/\$GITHUB_TOKEN for $REPO" >&2
  echo "  fix: gh auth login  (credentials are stored, see AGENTS.md §3)" >&2
  echo "   or: export GH_TOKEN=<token>  (repo 'Actions: Read' scope is enough)" >&2
  exit 2
fi

# GET <api-path> → response body on stdout. api-path is relative to
# https://api.github.com/repos/$REPO/, e.g. "actions/runs/123".
api_get() {
  if [ "$HAVE_GH" -eq 1 ]; then
    gh api "repos/$REPO/$1"
  else
    curl -sSf -H "Accept: application/vnd.github+json" \
      -H "Authorization: Bearer $TOKEN" \
      "https://api.github.com/repos/$REPO/$1"
  fi
}

# JSON field extraction via python3 stdlib (no jq dependency).
# usage: printf '%s' "$json" | jget 'expr over d'
jget() {
  python3 -c 'import json,sys; d=json.load(sys.stdin); print('"$1"')'
}

api_download() {
  # $1 = api-path, $2 = output file. Follows the 302 GitHub returns for
  # archived logs. Prefers gh (stored auth), falls back to curl + token.
  if [ "$HAVE_GH" -eq 1 ]; then
    if gh api "repos/$REPO/$1" > "$2" 2>/dev/null; then
      return 0
    fi
    echo "watch-ci.sh: gh download failed, retrying with curl" >&2
  fi
  curl -sSfL -H "Accept: application/vnd.github+json" \
    -H "Authorization: Bearer $TOKEN" \
    -o "$2" "https://api.github.com/repos/$REPO/$1"
}

resolve_sha() {
  if [ "$1" = "HEAD" ] || [ -z "$1" ]; then
    git rev-parse HEAD
  else
    git rev-parse --verify "$1^{commit}" 2>/dev/null || printf '%s' "$1"
  fi
}

find_run_by_sha() {
  api_get "actions/runs?head_sha=$1&per_page=5" | python3 -c '
import json,sys
d = json.load(sys.stdin)
for r in d.get("workflow_runs", []):
    print(r["id"], r.get("name",""), r.get("head_branch",""), r.get("head_sha","")[:12], r.get("status",""), r.get("conclusion",""))
'
}

find_run_by_branch() {
  api_get "actions/workflows/$1/runs?branch=$2&per_page=5" | python3 -c '
import json,sys
d = json.load(sys.stdin)
for r in d.get("workflow_runs", []):
    print(r["id"], r.get("name",""), r.get("head_branch",""), r.get("head_sha","")[:12], r.get("status",""), r.get("conclusion",""))
'
}

# --- resolve the run id -------------------------------------------------
if [ -z "$RUN_ID" ]; then
  if [ -n "$SHA" ]; then
    want="$(resolve_sha "$SHA")"
    echo "watch-ci.sh: looking for CI run with head_sha $want ..."
    waited=0
    while [ "$waited" -le "$WAIT_FOR_RUN" ]; do
      line="$(find_run_by_sha "$want" | head -1 || true)"
      if [ -n "$line" ]; then
        RUN_ID="${line%% *}"
        echo "watch-ci.sh: found run $RUN_ID ($line)"
        break
      fi
      if [ "$FOLLOW" -eq 0 ] || [ "$waited" -ge "$WAIT_FOR_RUN" ]; then
        break
      fi
      sleep "$INTERVAL"
      waited=$((waited + INTERVAL))
    done
    if [ -z "$RUN_ID" ]; then
      echo "watch-ci.sh: no CI run for sha $want after ${WAIT_FOR_RUN}s." >&2
      echo "  This is EXPECTED for prose-only pushes (**.md, docs/**, LICENSE" >&2
      echo "  start no CI run at all — see ci.yml paths-ignore)." >&2
      echo "  If the push touched code, check https://github.com/$REPO/actions" >&2
      exit 2
    fi
  else
    echo "watch-ci.sh: looking for latest '$WORKFLOW' run on branch '$BRANCH' ..."
    waited=0
    while [ "$waited" -le "$WAIT_FOR_RUN" ]; do
      line="$(find_run_by_branch "$WORKFLOW" "$BRANCH" | head -1 || true)"
      if [ -n "$line" ]; then
        RUN_ID="${line%% *}"
        echo "watch-ci.sh: watching run $RUN_ID ($line)"
        echo "  https://github.com/$REPO/actions/runs/$RUN_ID"
        break
      fi
      if [ "$FOLLOW" -eq 0 ] || [ "$waited" -ge "$WAIT_FOR_RUN" ]; then
        break
      fi
      sleep "$INTERVAL"
      waited=$((waited + INTERVAL))
    done
    if [ -z "$RUN_ID" ]; then
      echo "watch-ci.sh: no '$WORKFLOW' runs found for branch '$BRANCH'." >&2
      exit 2
    fi
  fi
else
  echo "watch-ci.sh: watching run $RUN_ID"
  echo "  https://github.com/$REPO/actions/runs/$RUN_ID"
fi

print_status() {
  # $1 = run json, $2 = jobs json
  python3 -c '
import json,sys
run = json.loads(open(sys.argv[1]).read())
jobs = json.loads(open(sys.argv[2]).read()).get("jobs", [])
print("run: status=%s conclusion=%s sha=%s branch=%s event=%s" % (
    run.get("status"), run.get("conclusion"),
    (run.get("head_sha") or "")[:12], run.get("head_branch"), run.get("event")))
for j in jobs:
    mark = {"success":"ok  ","failure":"FAIL","cancelled":"stop ","skipped":"skip"}.get(
        j.get("conclusion") or "", "... ")
    print("  [%s] %-32s status=%-11s conclusion=%s" % (
        mark, j.get("name"), j.get("status"), j.get("conclusion")))
    for s in j.get("steps", []):
        if (s.get("conclusion") or "") not in ("success", "skipped", None, ""):
            print("         step FAIL: %s (status=%s conclusion=%s)" % (
                s.get("name"), s.get("status"), s.get("conclusion")))
' "$1" "$2"
}

RUN_JSON="$(mktemp)"
JOBS_JSON="$(mktemp)"
trap 'rm -f "$RUN_JSON" "$JOBS_JSON"' EXIT

elapsed=0
while true; do
  api_get "actions/runs/$RUN_ID" > "$RUN_JSON"
  api_get "actions/runs/$RUN_ID/jobs?per_page=100" > "$JOBS_JSON"
  echo "--- $(date -u +%H:%M:%SZ) (+${elapsed}s) ---"
  print_status "$RUN_JSON" "$JOBS_JSON"

  status="$(printf '%s' "$(cat "$RUN_JSON")" | jget 'd.get("status","")')"
  if [ "$status" = "completed" ]; then
    break
  fi
  if [ "$FOLLOW" -eq 0 ]; then
    echo "watch-ci.sh: run not completed (status=$status); --no-follow, exiting" >&2
    exit 2
  fi
  if [ "$elapsed" -ge "$TIMEOUT" ]; then
    echo "watch-ci.sh: timed out after ${TIMEOUT}s waiting for run $RUN_ID" >&2
    exit 1
  fi
  sleep "$INTERVAL"
  elapsed=$((elapsed + INTERVAL))
done

conclusion="$(printf '%s' "$(cat "$RUN_JSON")" | jget 'd.get("conclusion","")')"
echo "watch-ci.sh: run $RUN_ID completed with conclusion: $conclusion"

# A newer run on the same branch supersedes this one under CI's
# concurrency group (cancel-in-progress): point at it instead of crying.
if [ "$conclusion" = "cancelled" ] && [ -z "$SHA" ]; then
  newer="$(find_run_by_branch "$WORKFLOW" "$BRANCH" | head -1 || true)"
  newer_id="${newer%% *}"
  if [ -n "$newer_id" ] && [ "$newer_id" != "$RUN_ID" ]; then
    echo "watch-ci.sh: superseded — a newer run exists: $newer" >&2
    echo "  https://github.com/$REPO/actions/runs/$newer_id" >&2
  fi
fi

# --- full logs -----------------------------------------------------------
if [ "$NO_SAVE" -eq 0 ] && [ -z "$LOGS_DIR" ]; then
  LOGS_DIR="logs/ci-$RUN_ID"
fi

if [ "$NO_SAVE" -eq 0 ]; then
  mkdir -p "$LOGS_DIR"
  zip="$LOGS_DIR/run-$RUN_ID-logs.zip"
  echo "watch-ci.sh: downloading FULL run logs → $zip ..."
  if api_download "actions/runs/$RUN_ID/logs" "$zip"; then
    if python3 -c 'import zipfile,sys; zipfile.ZipFile(sys.argv[1]).testzip()' "$zip" 2>/dev/null; then
      python3 -c 'import zipfile,sys; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])' "$zip" "$LOGS_DIR"
      echo "watch-ci.sh: extracted per-job step logs:"
      find "$LOGS_DIR" -type f ! -name '*.zip' | sort
    else
      echo "watch-ci.sh: downloaded file is not a zip (run may have just completed;" >&2
      echo "  logs can lag ~30s behind completion) — retrying once after 30s ..." >&2
      sleep 30
      if api_download "actions/runs/$RUN_ID/logs" "$zip" \
        && python3 -c 'import zipfile,sys; zipfile.ZipFile(sys.argv[1]).testzip()' "$zip" 2>/dev/null; then
        python3 -c 'import zipfile,sys; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])' "$zip" "$LOGS_DIR"
        find "$LOGS_DIR" -type f ! -name '*.zip' | sort
      else
        echo "watch-ci.sh: still no zip; falling back to per-job text logs" >&2
        rm -f "$zip"
      fi
    fi
  else
    echo "watch-ci.sh: run-log download failed; falling back to per-job text logs" >&2
  fi
fi

print_job_log() {
  # $1 = job id, $2 = job name — fetch text log, print (all or tail).
  tmp="$(mktemp)"
  if api_download "actions/jobs/$1/logs" "$tmp" 2>/dev/null; then
    echo "===== job: $2 (id $1) ====="
    if [ "$PRINT_MODE" = "all" ]; then
      cat "$tmp"
    else
      tail -n 80 "$tmp"
    fi
    if [ "$NO_SAVE" -eq 0 ]; then
      safe="$(printf '%s' "$2" | tr -c 'A-Za-z0-9._-' '_')"
      cp "$tmp" "$LOGS_DIR/job-$1-$safe.log"
    fi
  else
    echo "watch-ci.sh: could not fetch log for job '$2' (id $1)" >&2
  fi
  rm -f "$tmp"
}

if [ "$PRINT_MODE" != "none" ]; then
  if [ "$PRINT_MODE" = "all" ]; then
    wanted="$(python3 -c 'import json,sys; [print(str(j["id"])+"\t"+j["name"]) for j in json.load(open(sys.argv[1])).get("jobs",[])]' "$JOBS_JSON")"
  else
    wanted="$(python3 -c 'import json,sys; [print(str(j["id"])+"\t"+j["name"]) for j in json.load(open(sys.argv[1])).get("jobs",[]) if (j.get("conclusion") or "") not in ("success","skipped")]' "$JOBS_JSON")"
    if [ -z "$wanted" ] && [ "$conclusion" = "success" ]; then
      echo "watch-ci.sh: all jobs green — no failure logs to print."
    fi
  fi
  # shellcheck disable=SC2162: names may contain spaces; read carefully
  printf '%s\n' "$wanted" | while IFS="$(printf '\t')" read job_id job_name; do
    [ -n "$job_id" ] || continue
    print_job_log "$job_id" "$job_name"
  done
fi

case "$conclusion" in
  success|skipped) exit 0 ;;
  *) exit 1 ;;
esac
