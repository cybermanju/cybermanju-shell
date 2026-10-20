#!/usr/bin/env bash
# CyberManju OS — push helper.
# Stages everything, commits (never a bare 'update'), and pushes main + tags
# to GitHub (origin) AND the GitLab mirror (gitlab). No force-push ever:
# the script fetches first and rebases onto origin/main when behind, and
# pushes only tags the remote lacks — existing remote tags are never moved
# (tags/releases move only by explicit command).
#
# Commit message UX:
#   ./push.sh "feat: add watch-ci"     # positional (all words joined)
#   ./push.sh -m "feat: add watch-ci"  # explicit flag (same thing)
#   ./push.sh                          # interactive prompt when on a TTY
#                                      # (Enter accepts the dated default),
#                                      # dated default when piped/agent-driven
#
# CI watch (see scripts/watch-ci.sh — full logs of every step):
#   ./push.sh --watch "feat: ..."      # push, then watch the HEAD run
#   PUSH_WATCH_CI=1 ./push.sh -m "..."  # same via env
#   ./push.sh --watch --full ...        # print ALL logs inline, not just failures
#   scripts/watch-ci.sh --sha HEAD      # watch separately at any time
#
# Release (see .github/workflows/release.yml — full vs reuse modes):
#   ./push.sh --release -m "notes md"   # tag HEAD v<pkg> + push tag (full rebuild
#                                       # release); -m is the release-notes markdown
#                                       # prepended to docs/RELEASE_NOTES.md
#   ./push.sh --release --last
#     -m "notes md"                     # NO rebuild: dispatch the release workflow
#                                       # in reuse mode, shipping the last green
#                                       # CI build's dist-* artifacts for HEAD
#   ./push.sh --release --last --reuse-run 37965443550 -m "..."
#                                       # pin the CI run instead of auto-resolving
#                                       # HEAD's latest green one (needed when HEAD
#                                       # itself is newer, e.g. the release commit)
#   ./push.sh --release --move-tag ...  # EXPLICIT tag delete/recreate (remote tag
#                                       # is force-updated; refuses when a published
#                                       # GitHub release already sits on the tag)
set -u
cd "$(dirname "$0")"

# Heal mixed-ownership `.git/objects` fanout dirs. Sessions here sometimes run
# as root and sometimes as a login uid; object dirs created by one owner
# block `git add` for the other with:
#   "insufficient permission for adding an object to repository database"
# When root, hand stray root-owned `.git` entries back to the worktree owner
# (root can write regardless). As non-root there is nothing safe to chown,
# so just warn early — the `git add` failure below still aborts cleanly.
if [ -d .git/objects ]; then
  if [ "$(id -u)" -eq 0 ]; then
    wt_owner="$(stat -c '%u:%g' . 2>/dev/null || true)"
    if [ -n "$wt_owner" ] && [ "$wt_owner" != "0:0" ]; then
      find .git -user root -exec chown "$wt_owner" {} + 2>/dev/null || true
    fi
  else
    bad_dir="$(find .git/objects -type d ! -writable -print -quit 2>/dev/null || true)"
    if [ -n "$bad_dir" ]; then
      echo "push.sh: WARNING: $bad_dir is not writable by $(id -un) (mixed root/uid .git ownership)." >&2
      echo "push.sh: WARNING: 'git add' may fail — re-run once as root (it heals .git) or run: sudo chown -R $(id -un):$(id -gn) .git" >&2
    fi
  fi
fi

MSG=""
ALLOW_EMPTY=0
DRY_RUN=0
WATCH="auto"   # auto|yes|no
WATCH_ARGS=()
SHOW_HELP=0
DO_RELEASE=0
REUSE_LAST=0
MOVE_TAG=0
REUSE_RUN="${PUSH_REUSE_RUN:-}"
NOTES_FILE=""
NOTES=""
RELEASE_TAG=""
RELEASE_TIMEOUT=1800

usage() {
  cat <<'EOF'
usage: push.sh [options] [message...]

options:
  -m, --message MSG   commit message (default: positional words joined,
                      else prompt on TTY, else dated "chore: sync YYYY-MM-DD")
  --allow-empty       commit even with nothing staged (re-triggers CI on demand)
  --dry-run           print the git commands without running them
  --watch             push, then watch CI for HEAD (full step logs via
                      scripts/watch-ci.sh); exit nonzero if CI is red
  --no-watch          never watch (default unless --watch or PUSH_WATCH_CI=1)
  --logs-dir DIR      pass through to watch-ci.sh
  --failed-only       pass through (print only failure logs; the default)
  --full              pass through (print ALL job logs inline)
  --timeout SECS      pass through to watch-ci.sh (default 3600)
  --release             publish a GitHub release for v<package.json version>:
                        commit (as "chore(release): vX.Y.Z") + push main, then
                        create the vX.Y.Z tag and push it, triggering the
                        Release workflow (full rebuild of every family).
                        -m/positional text is the release-notes markdown
                        (prepended to docs/RELEASE_NOTES.md by the workflow).
                        Empty notes = atlas-only body.
  --release-notes-file FILE
                        read the release-notes markdown from FILE instead of
                        -m (exclusive with -m/positional)
  --last                with --release: skip ALL rebuilds. Dispatches the
                        Release workflow in reuse mode: it ships the dist-*
                        artifacts of a green CI run unchanged (exe, deb,
                        AppImage, rpm, flatpak, dmg, apk/aab, wasm).
                        Auto-resolves HEAD's latest green CI run; when HEAD
                        itself is newer than any green run, pin one with
                        --reuse-run (or $PUSH_REUSE_RUN).
  --reuse-run ID        pin the CI run whose artifacts --last ships
  --move-tag            with --release (full mode only): when vX.Y.Z already
                        exists elsewhere, delete/recreate it at HEAD and
                        force-update it on both remotes. Explicit and loud;
                        refuses when a published GitHub release sits on it.
  --release-timeout SECS
                        wait up to SECS for the published release before
                        applying -m notes (full mode, default 1800)
  -h, --help          this help
EOF
}

positional=()
while [ $# -gt 0 ]; do
  case "$1" in
    -m|--message) MSG="${2:-}"; shift 2 ;;
    -m=*) MSG="${1#-m=}"; shift ;;
    --message=*) MSG="${1#--message=}"; shift ;;
    --allow-empty) ALLOW_EMPTY=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    --watch) WATCH="yes"; shift ;;
    --no-watch) WATCH="no"; shift ;;
    --logs-dir) WATCH_ARGS+=("$1" "${2:-}"); shift 2 ;;
    --logs-dir=*) WATCH_ARGS+=("$1"); shift ;;
    --failed-only|--full) WATCH_ARGS+=("$1"); shift ;;
    --timeout) WATCH_ARGS+=("$1" "${2:-}"); shift 2 ;;
    --timeout=*) WATCH_ARGS+=("$1"); shift ;;
    --release) DO_RELEASE=1; shift ;;
    --last) REUSE_LAST=1; shift ;;
    --move-tag) MOVE_TAG=1; shift ;;
    --reuse-run) REUSE_RUN="${2:-}"; shift 2 ;;
    --reuse-run=*) REUSE_RUN="${1#--reuse-run=}"; shift ;;
    --release-notes-file) NOTES_FILE="${2:-}"; shift 2 ;;
    --release-notes-file=*) NOTES_FILE="${1#--release-notes-file=}"; shift ;;
    --release-timeout) RELEASE_TIMEOUT="${2:-}"; shift 2 ;;
    --release-timeout=*) RELEASE_TIMEOUT="${1#--release-timeout=}"; shift ;;
    -h|--help) SHOW_HELP=1; shift ;;
    --) shift; while [ $# -gt 0 ]; do positional+=("$1"); shift; done ;;
    -*) echo "push.sh: unknown option: $1" >&2; usage >&2; exit 2 ;;
    *) positional+=("$1"); shift ;;
  esac
done

if [ "$SHOW_HELP" -eq 1 ]; then
  usage
  exit 0
fi

if [ "$REUSE_LAST" -eq 1 ] && [ "$DO_RELEASE" -eq 0 ]; then
  echo "push.sh: --last needs --release (it is the no-rebuild mode of a release)" >&2
  exit 2
fi
if [ "$MOVE_TAG" -eq 1 ] && [ "$DO_RELEASE" -eq 0 ]; then
  echo "push.sh: --move-tag needs --release" >&2
  exit 2
fi
if [ "$MOVE_TAG" -eq 1 ] && [ "$REUSE_LAST" -eq 1 ]; then
  echo "push.sh: --move-tag is meaningless with --last (reuse mode pushes no tag)" >&2
  exit 2
fi
if [ -n "$REUSE_RUN" ] && [ "$REUSE_LAST" -eq 0 ]; then
  echo "push.sh: --reuse-run needs --last" >&2
  exit 2
fi

if [ "$DO_RELEASE" -eq 1 ]; then
  # Release mode: -m/positional text is the release-notes markdown (NOT the
  # commit message — the commit is always "chore(release): vX.Y.Z").
  RELEASE_TAG="v$(node -p "require('./package.json').version")"
  if [ -n "$NOTES_FILE" ]; then
    if [ -z "$MSG" ] && [ "${#positional[@]}" -eq 0 ]; then
      : # file is the only notes source — fine
    else
      echo "push.sh: --release-notes-file is exclusive with -m/positional notes" >&2
      exit 2
    fi
    if [ ! -f "$NOTES_FILE" ]; then
      echo "push.sh: notes file not found: $NOTES_FILE" >&2
      exit 2
    fi
    NOTES="$(cat "$NOTES_FILE")"
  else
    if [ -z "$MSG" ] && [ "${#positional[@]}" -gt 0 ]; then
      MSG="${positional[*]}"
    fi
    if [ -z "$MSG" ] && [ -t 0 ]; then
      printf 'Release notes markdown (Enter for atlas-only body): ' > /dev/tty
      IFS= read -r reply < /dev/tty || reply=""
      MSG="$reply"
    fi
    NOTES="$MSG"   # verbatim — no trimming (markdown is whitespace-sensitive)
  fi
  MSG="chore(release): $RELEASE_TAG"
else
  if [ -z "$MSG" ] && [ "${#positional[@]}" -gt 0 ]; then
    MSG="${positional[*]}"
  fi

  if [ -z "$MSG" ]; then
    default_msg="chore: sync $(date -u +%Y-%m-%d)"
    if [ -t 0 ]; then
      printf 'Commit message [%s]: ' "$default_msg" > /dev/tty
      IFS= read -r reply < /dev/tty || reply=""
      MSG="${reply:-$default_msg}"
    else
      MSG="$default_msg"
    fi
  fi

  # Trim surrounding whitespace.
  MSG="$(printf '%s' "$MSG" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"

  if [ -z "$MSG" ]; then
    echo "push.sh: empty commit message — aborting (pass -m \"...\" or --allow-empty to re-trigger CI)" >&2
    exit 2
  fi
  if printf '%s' "$MSG" | grep -qi '^update$'; then
    echo "push.sh: refusing bare 'update' as a commit message — say what changed" >&2
    exit 2
  fi
fi

if [ "$WATCH" = "auto" ] && [ "${PUSH_WATCH_CI:-0}" = "1" ]; then
  WATCH="yes"
fi

echo "push.sh: branch: $(git branch --show-current)"
echo "push.sh: working tree before staging:"
git status --short --branch || true

# This script only ever pushes `main`. Pushing the `main` ref from any other
# branch would silently push a stale ref instead of HEAD — refuse instead.
cur_branch="$(git branch --show-current)"
if [ "$cur_branch" != "main" ]; then
  echo "push.sh: refusing to push from '$cur_branch' — switch to main first (no force-push by design)" >&2
  exit 2
fi

run() {
  if [ "$DRY_RUN" -eq 1 ]; then
    echo "  (dry-run) $*"
  else
    "$@"
  fi
}

if [ "$DRY_RUN" -eq 0 ]; then
  # A failed `git add` must abort here. Ignoring it stages nothing, prints
  # a confusing "nothing new to commit", then pushes stale HEAD.
  if ! git add -A; then
    echo "push.sh: 'git add -A' FAILED — refusing to commit/push a half-staged tree." >&2
    echo "push.sh: hint: mixed .git ownership? run: sudo chown -R $(stat -c '%U:%G' . 2>/dev/null || echo 'UID:GID') .git" >&2
    exit 1
  fi
else
  echo "  (dry-run) git add -A"
fi

staged=0
if [ "$DRY_RUN" -eq 1 ]; then
  if [ -n "$(git status --porcelain)" ]; then
    staged=1
  fi
elif ! git diff --cached --quiet; then
  staged=1
fi

if [ "$staged" -eq 1 ]; then
  echo "push.sh: committing with message: $MSG"
  if [ "$DRY_RUN" -eq 0 ]; then
    echo "push.sh: staged change stat:"
    git diff --cached --stat || true
  fi
  if [ "$ALLOW_EMPTY" -eq 1 ]; then
    run git commit --allow-empty -m "$MSG" || { echo "push.sh: commit failed" >&2; exit 1; }
  else
    run git commit -m "$MSG" || { echo "push.sh: commit failed" >&2; exit 1; }
  fi
else
  if [ "$ALLOW_EMPTY" -eq 1 ]; then
    echo "push.sh: nothing staged — creating empty commit: $MSG"
    run git commit --allow-empty -m "$MSG"
  else
    echo "push.sh: nothing new to commit (use --allow-empty to force an empty commit)"
  fi
fi

if [ "$DRY_RUN" -eq 1 ]; then
  echo "  (dry-run) git fetch <each configured remote: origin gitlab>"
  echo "  (dry-run) git pull --rebase origin main (only if behind origin/main)"
  echo "  (dry-run) for each configured remote: git push <r> main + push new tags only"
  if [ "$DO_RELEASE" -eq 1 ] && [ "$REUSE_LAST" -eq 0 ]; then
    echo "  (dry-run) push tag $RELEASE_TAG (triggers full-rebuild Release workflow) + apply notes"
  fi
  if [ "$DO_RELEASE" -eq 1 ] && [ "$REUSE_LAST" -eq 1 ]; then
    echo "  (dry-run) gh workflow run release.yml --ref main -f tag=$RELEASE_TAG -f reuse_run_id=${REUSE_RUN:-auto} -f notes=<(release notes)"
  fi
  if [ "$WATCH" = "yes" ]; then
    # shellcheck disable=SC2145: intentional array expansion display
    echo "  (dry-run) scripts/watch-ci.sh --sha HEAD ${WATCH_ARGS[*]:-}"
  fi
  exit 0
fi

# Release tag (full mode only — reuse mode pushes no tag). Lightweight, like
# the existing v* tags. Never moved silently: a tag that already points
# elsewhere aborts unless --move-tag was passed explicitly.
TAG_ACTION="none"   # created|exists-head|moved
if [ "$DO_RELEASE" -eq 1 ] && [ "$REUSE_LAST" -eq 0 ]; then
  if git rev-parse "$RELEASE_TAG" >/dev/null 2>&1; then
    tag_at="$(git rev-parse "$RELEASE_TAG")"
    head_now="$(git rev-parse HEAD)"
    if [ "$tag_at" = "$head_now" ]; then
      echo "push.sh: tag $RELEASE_TAG already points at HEAD"
      TAG_ACTION="exists-head"
    elif [ "$MOVE_TAG" -eq 1 ]; then
      if command -v gh >/dev/null 2>&1 && gh release view "$RELEASE_TAG" >/dev/null 2>&1; then
        echo "push.sh: refusing --move-tag: a published GitHub release sits on $RELEASE_TAG" >&2
        echo "push.sh: delete it explicitly first (gh release delete $RELEASE_TAG), then re-run" >&2
        exit 2
      fi
      echo "push.sh: --move-tag: recreating $RELEASE_TAG at HEAD (was $tag_at)"
      if [ "$DRY_RUN" -eq 1 ]; then
        echo "  (dry-run) git tag -f $RELEASE_TAG HEAD + force-push tag to origin+gitlab"
      else
        git tag -f "$RELEASE_TAG" HEAD || { echo "push.sh: tag move failed" >&2; exit 1; }
      fi
      TAG_ACTION="moved"
    else
      echo "push.sh: tag $RELEASE_TAG exists at $tag_at, HEAD is $head_now" >&2
      echo "push.sh: refusing to move it — re-run with --move-tag to delete/recreate it explicitly" >&2
      exit 2
    fi
  else
    echo "push.sh: creating tag $RELEASE_TAG at HEAD"
    if [ "$DRY_RUN" -eq 1 ]; then
      echo "  (dry-run) git tag $RELEASE_TAG HEAD"
    else
      git tag "$RELEASE_TAG" HEAD || { echo "push.sh: tag create failed" >&2; exit 1; }
    fi
    TAG_ACTION="created"
  fi
fi

# Push only tags the remote lacks. A tag that exists remotely with a
# different object is NEVER moved (releases move by explicit command only) —
# a blanket `git push --tags` turns that harmless skew into a FAILED remote.
push_new_tags() {
  remote="$1"
  rc=0
  for t in $(git tag --list); do
    [ -n "$t" ] || continue
    local_hash="$(git rev-parse "$t" 2>/dev/null || true)"
    remote_hash="$(git ls-remote "$remote" "refs/tags/$t" 2>/dev/null | awk '{print $1}')"
    if [ -z "$remote_hash" ]; then
      echo "push.sh: pushing new tag $t to $remote"
      if ! git push "$remote" "refs/tags/$t"; then
        echo "push.sh: tag $t -> $remote FAILED" >&2
        rc=1
      fi
    elif [ "$remote_hash" != "$local_hash" ]; then
      if [ "$MOVE_TAG" -eq 1 ] && [ "$DO_RELEASE" -eq 1 ] && [ "$t" = "$RELEASE_TAG" ]; then
        echo "push.sh: --move-tag: force-updating tag $t on $remote ($remote_hash -> $local_hash)"
        if ! git push "$remote" "+refs/tags/$t"; then
          echo "push.sh: tag $t -> $remote FAILED" >&2
          rc=1
        fi
      else
        echo "push.sh: WARNING: tag $t differs on $remote ($remote_hash vs local $local_hash) — leaving the remote tag alone (explicit move only)" >&2
      fi
    fi
  done
  return "$rc"
}

fail=0
origin_ok=0

# Push only to configured remotes. A clone without the `gitlab` mirror (or
# without `origin`) used to fail the whole run at push time; skipping the
# missing remote with a loud note keeps the exit code meaningful.
PUSH_REMOTES=""
for r in origin gitlab; do
  if git remote | grep -qx "$r"; then
    PUSH_REMOTES="$PUSH_REMOTES $r"
  else
    echo "push.sh: remote '$r' is not configured in this clone — skipping it"
  fi
done
if [ -z "$PUSH_REMOTES" ]; then
  echo "push.sh: no push remotes configured — aborting" >&2
  exit 1
fi

# Integrate remote work before pushing (no force-push, ever): fetch both
# remotes; if we are behind origin (source of truth — Pages deploys from it),
# rebase onto it. Anything still not fast-forward after that aborts below.
if echo "$PUSH_REMOTES" | grep -qw origin; then
  if ! git fetch origin; then
    echo "push.sh: fetch origin FAILED — aborting (cannot verify fast-forward)" >&2
    exit 1
  fi
else
  echo "push.sh: origin not configured — cannot verify fast-forward, pushing anyway" >&2
fi
if echo "$PUSH_REMOTES" | grep -qw gitlab; then
  if ! git fetch gitlab; then
    echo "push.sh: WARNING: fetch gitlab failed — will still try the mirror push" >&2
  fi
fi
behind_origin="$(git rev-list --count HEAD..origin/main 2>/dev/null || echo 0)"
ahead_origin="$(git rev-list --count origin/main..HEAD 2>/dev/null || echo 0)"
if [ "${behind_origin:-0}" -gt 0 ]; then
  if [ "${ahead_origin:-0}" -gt 0 ]; then
    echo "push.sh: origin/main is $behind_origin ahead and we are $ahead_origin ahead — rebasing onto origin/main ..."
  else
    echo "push.sh: we are $behind_origin behind origin/main — fast-forwarding ..."
  fi
  if git pull --rebase origin main; then
    echo "push.sh: rebase onto origin/main OK"
  else
    echo "push.sh: rebase onto origin/main FAILED — resolve conflicts, then re-run (no -f)" >&2
    exit 1
  fi
fi

for remote in $PUSH_REMOTES; do
  if git push "$remote" main; then
    echo "push.sh: $remote main OK"
    if [ "$remote" = "origin" ]; then
      origin_ok=1
    fi
    if ! push_new_tags "$remote"; then
      echo "push.sh: $remote tags FAILED" >&2
      fail=1
    fi
  else
    echo "push.sh: $remote main FAILED" >&2
    echo "push.sh: hint: 'git fetch $remote' then 'git pull --rebase $remote main' (no -f), then re-run" >&2
    echo "push.sh: hint: if it says 'could not read Username', no credentials are stored in this shell — set up auth (AGENTS.md §3), then re-run" >&2
    fail=1
  fi
done

head_sha="$(git rev-parse HEAD)"
echo "push.sh: HEAD is now $head_sha"

# --- release stage ------------------------------------------------------
repo_slug() {
  # OWNER/REPO from origin, same parsing as scripts/watch-ci.sh.
  origin_url="$(git remote get-url origin 2>/dev/null || true)"
  printf '%s' "$origin_url" \
    | sed -n -e 's#.*github\.com[:/]\(.*/.*\)\(\.git\)\?$#\1#p' | head -1
}

require_gh() {
  if ! command -v gh >/dev/null 2>&1 || ! gh auth status >/dev/null 2>&1; then
    echo "push.sh: release needs authenticated gh (see AGENTS.md section 3)" >&2
    exit 2
  fi
}

do_release_full() {
  # The tag push above triggers the Release workflow (full rebuild).
  # Verify the tag landed on HEAD, then prepend custom notes to the release.
  head_now="$(git rev-parse HEAD)"
  remote_hash="$(git ls-remote origin "refs/tags/$RELEASE_TAG" 2>/dev/null | awk '{print $1}')"
  if [ "$remote_hash" != "$head_now" ]; then
    echo "push.sh: tag $RELEASE_TAG on origin ($remote_hash) is not HEAD ($head_now)" >&2
    echo "push.sh: the Release workflow was NOT triggered — fix the tag, then re-run" >&2
    return 1
  fi
  echo "push.sh: tag $RELEASE_TAG is HEAD on origin — Release workflow triggered (full rebuild)"
  echo "push.sh: https://github.com/$(repo_slug)/actions/workflows/release.yml"
  if [ -z "$NOTES" ]; then
    echo "push.sh: no custom notes — the workflow generates the atlas body"
    return 0
  fi
  require_gh
  echo "push.sh: waiting for the published release (up to ${RELEASE_TIMEOUT}s), then prepending notes ..."
  waited=0
  while [ "$waited" -lt "$RELEASE_TIMEOUT" ]; do
    if gh release view "$RELEASE_TAG" >/dev/null 2>&1; then
      break
    fi
    sleep 30
    waited=$((waited + 30))
  done
  if ! gh release view "$RELEASE_TAG" >/dev/null 2>&1; then
    echo "push.sh: release $RELEASE_TAG not published after ${RELEASE_TIMEOUT}s" >&2
    echo "push.sh: apply the notes later by re-running with --release (idempotent)" >&2
    return 1
  fi
  body_file="$(mktemp)"
  notes_file="$(mktemp)"
  gh release view "$RELEASE_TAG" --json body -q .body > "$body_file"
  if [ "$(head -1 "$body_file")" = "$(printf '%s' "$NOTES" | head -1)" ]; then
    echo "push.sh: notes already lead the release body — nothing to do"
    rm -f "$body_file" "$notes_file"
    return 0
  fi
  { printf '%s\n\n' "$NOTES"; cat "$body_file"; } > "$notes_file"
  if gh release edit "$RELEASE_TAG" --notes-file "$notes_file"; then
    echo "push.sh: release notes applied to $RELEASE_TAG"
  else
    echo "push.sh: 'gh release edit' FAILED — notes kept at: $notes_file" >&2
    rm -f "$body_file"
    return 1
  fi
  rm -f "$body_file" "$notes_file"
  return 0
}

do_release_last() {
  # No-rebuild publish: dispatch the Release workflow in reuse mode — it
  # ships one green CI run's dist-* artifacts unchanged (exe, deb, AppImage,
  # rpm, flatpak, dmg, apk/aab, wasm). Pushes no tag and compiles nothing.
  require_gh
  REPO="$(repo_slug)"
  if [ -z "$REPO" ]; then
    echo "push.sh: cannot determine repo from origin URL" >&2
    exit 2
  fi
  head_now="$(git rev-parse HEAD)"
  RUN_ID="$REUSE_RUN"
  if [ -z "$RUN_ID" ]; then
    echo "push.sh: resolving HEAD's latest green CI run ($head_now) ..."
    RUN_ID="$(gh api "repos/$REPO/actions/runs?head_sha=$head_now&per_page=10" \
      | python3 -c '
import json,sys
for r in json.load(sys.stdin).get("workflow_runs", []):
    if r.get("status") == "completed" and r.get("conclusion") == "success":
        print(r["id"]); break
')"
    if [ -z "$RUN_ID" ]; then
      echo "push.sh: no green CI run for HEAD — wait for CI, or pin one with --reuse-run <id>" >&2
      return 1
    fi
    echo "push.sh: reuse CI run $RUN_ID"
  else
    echo "push.sh: reuse pinned CI run $RUN_ID"
  fi
  # The run must carry every dist-* family, fresh (CI artifacts expire).
  missing="$(gh api "repos/$REPO/actions/runs/$RUN_ID/artifacts?per_page=50" \
    | python3 -c '
import json,sys
want = ["dist-windows","dist-linux","dist-linux-deb","dist-linux-rpm","dist-rpm","dist-flatpak","dist-arch","dist-macos","dist-android","dist-wasm","dist-cli"]
have = {a["name"] for a in json.load(sys.stdin).get("artifacts", []) if not a.get("expired")}
print(" ".join(w for w in want if w not in have))')"
  if [ -n "$missing" ]; then
    echo "push.sh: CI run $RUN_ID lacks fresh artifacts: $missing" >&2
    echo "push.sh: pick another run with --reuse-run (CI artifacts expire after 7 days)" >&2
    return 1
  fi
  T0="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  export T0
  echo "push.sh: dispatching Release workflow (reuse run $RUN_ID, tag $RELEASE_TAG) ..."
  if ! gh workflow run release.yml --ref main -f tag="$RELEASE_TAG" -f reuse_run_id="$RUN_ID" -f notes="$NOTES"; then
    echo "push.sh: dispatch FAILED" >&2
    return 1
  fi
  echo "push.sh: locating the dispatched run ..."
  REL_RUN=""
  waited=0
  while [ "$waited" -lt 180 ]; do
    REL_RUN="$(gh api "repos/$REPO/actions/workflows/release.yml/runs?branch=main&per_page=5" \
      | python3 -c '
import json,sys,os
t0 = os.environ.get("T0", "")
for r in json.load(sys.stdin).get("workflow_runs", []):
    if r.get("event") == "workflow_dispatch" and r.get("created_at", "") >= t0:
        print(r["id"]); break
')"
    [ -n "$REL_RUN" ] && break
    sleep 15
    waited=$((waited + 15))
  done
  if [ -z "$REL_RUN" ]; then
    echo "push.sh: dispatched but could not locate the run — see https://github.com/$REPO/actions/workflows/release.yml" >&2
    return 1
  fi
  echo "push.sh: watching release run $REL_RUN ..."
  if bash scripts/watch-ci.sh --run "$REL_RUN" "${WATCH_ARGS[@]}"; then
    echo "push.sh: release published: https://github.com/$REPO/releases/tag/$RELEASE_TAG"
  else
    echo "push.sh: release run NOT green — see logs above" >&2
    return 1
  fi
  return 0
}

if [ "$WATCH" = "yes" ]; then
  if [ "$origin_ok" -eq 1 ]; then
    echo "push.sh: watching CI for $head_sha ..."
    if bash scripts/watch-ci.sh --sha "$head_sha" "${WATCH_ARGS[@]}"; then
      echo "push.sh: CI green"
    else
      echo "push.sh: CI NOT green — see logs above" >&2
      fail=1
    fi
  else
    echo "push.sh: skipping CI watch (origin push failed)" >&2
  fi
else
  echo "push.sh: watch CI with: scripts/watch-ci.sh --sha $head_sha  (or re-push with --watch)"
fi

if [ "$DO_RELEASE" -eq 1 ]; then
  if [ "$REUSE_LAST" -eq 1 ]; then
    do_release_last || fail=1
  else
    do_release_full || fail=1
  fi
fi

exit "$fail"
