#!/usr/bin/env bash
# CyberManju OS — universal `cyb` installer.
#   curl -fsSL https://cybermanju.github.io/install.sh | bash
# Installs the `cyb` terminal (Charm UI) for Linux, macOS, Windows
# (Git Bash / MSYS2 / WSL) and Android (Termux). Verifies release SHA256SUMS.
#
# Env knobs:
#   CYB_VERSION   release tag (default: latest). Example: CYB_VERSION=v0.2.0
#   INSTALL_DIR   where the binary lands (default: $HOME/.local/bin)
#   REPO          owner/repo override (default: cybermanju/cybermanju.github.io)
#   CYB_SETUP     1 run `cyb setup` · 0 skip · auto prompt when interactive (default: auto)
#   CYB_BACKEND   remote|docker|native|supabase (default: prompt when interactive, else skip)
#   CYB_SERVE     1 = shorthand for CYB_BACKEND=docker (runs `cyb serve up`)
#   CYB_SERVER    dashboard URL for remote/setup (falls back to CYBERMANJU_SERVER)
#   CYB_PORT      loopback port for docker/native (default: 3456)
#   CYB_DATA      host data dir for `serve up` (default ~/.local/share/cybermanju-os/data)
#   CYB_IMAGE     dashboard image for `serve up` (default ghcr.io/cybermanju/cybermanju-os:latest)
#   CYB_USERNAME / CYB_PASSWORD / CYB_DISPLAY  unattended `cyb setup` credentials
#   CYB_SUPA_URL / CYB_SUPA_KEY                broker pair for CYB_BACKEND=supabase
#   CYB_YES       1 = assume yes to prompts
# Flags (curl -fsSL https://cybermanju.github.io/install.sh | bash -s -- [flags]):
#   --setup / --no-setup, --backend NAME, --serve / --no-serve,
#   --server URL, --port N, --data DIR, --image IMG,
#   --username U, --password P, --display-name D, --url U, --key K,
#   -y / --yes, -h / --help. Flags win over env.
# Piped installs hold stdin, so every prompt reads from /dev/tty instead.
# No TTY (CI/cron) means install-only unless CYB_SETUP=1 or flags force it.
set -u

REPO="${REPO:-cybermanju/cybermanju.github.io}"
INSTALL_DIR="${INSTALL_DIR:-}"
WANT_VERSION="${CYB_VERSION:-latest}"
SETUP_PREF="${CYB_SETUP:-auto}"
BACKEND="${CYB_BACKEND:-}"
SERVE_FLAG="${CYB_SERVE:-0}"
SERVER_WANT="${CYB_SERVER:-${CYBERMANJU_SERVER:-}}"
PORT_WANT="${CYB_PORT:-3456}"
DATA_WANT="${CYB_DATA:-}"
IMAGE_WANT="${CYB_IMAGE:-}"
SETUP_USER="${CYB_USERNAME:-}"
SETUP_PASS="${CYB_PASSWORD:-}"
SETUP_DISPLAY="${CYB_DISPLAY:-}"
SUPA_URL="${CYB_SUPA_URL:-}"
SUPA_KEY="${CYB_SUPA_KEY:-}"
ASSUME_YES=0
[ "${CYB_YES:-0}" = "1" ] && ASSUME_YES=1

say() { printf '◈ %s\n' "$*"; }
die() { printf '✗ %s\n' "$*" >&2; exit 1; }

usage() {
  cat <<'USAGE'
Usage: install.sh [flags]   (curl -fsSL https://cybermanju.github.io/install.sh | bash -s -- [flags])

Installs `cyb`, then optionally starts a dashboard and runs the setup
walkthrough (backend + account). Prompts only when interactive (/dev/tty).

Flags (win over env):
  --setup / --no-setup      run or skip `cyb setup` (env CYB_SETUP=1/0/auto)
  --backend NAME            remote|docker|native|supabase (env CYB_BACKEND)
  --serve / --no-serve      shorthand for --backend docker (env CYB_SERVE=1)
  --server URL              dashboard URL for remote/setup (env CYB_SERVER)
  --port N                  loopback port for docker/native (env CYB_PORT)
  --data DIR                host data dir for `serve up` (env CYB_DATA)
  --image IMG               dashboard image for `serve up` (env CYB_IMAGE)
  --username U --password P [--display-name D]   unattended setup (env CYB_USERNAME/CYB_PASSWORD/CYB_DISPLAY)
  --url U --key K           supabase broker pair (env CYB_SUPA_URL/CYB_SUPA_KEY)
  -y, --yes                 assume yes to prompts (env CYB_YES=1)
  -h, --help                show this help and exit
USAGE
}

while [ $# -gt 0 ]; do
  case "${1:-}" in
    --setup) SETUP_PREF=1 ;;
    --no-setup) SETUP_PREF=0 ;;
    --serve) BACKEND=docker ;;
    --no-serve) BACKEND=""; SERVE_FLAG=0 ;;
    --backend) BACKEND="${2:-}"; shift ;;
    --backend=*) BACKEND="${1#--backend=}" ;;
    --server) SERVER_WANT="${2:-}"; shift ;;
    --server=*) SERVER_WANT="${1#--server=}" ;;
    --port) PORT_WANT="${2:-}"; shift ;;
    --port=*) PORT_WANT="${1#--port=}" ;;
    --data) DATA_WANT="${2:-}"; shift ;;
    --data=*) DATA_WANT="${1#--data=}" ;;
    --image) IMAGE_WANT="${2:-}"; shift ;;
    --image=*) IMAGE_WANT="${1#--image=}" ;;
    --username) SETUP_USER="${2:-}"; shift ;;
    --username=*) SETUP_USER="${1#--username=}" ;;
    --password) SETUP_PASS="${2:-}"; shift ;;
    --password=*) SETUP_PASS="${1#--password=}" ;;
    --display-name) SETUP_DISPLAY="${2:-}"; shift ;;
    --display-name=*) SETUP_DISPLAY="${1#--display-name=}" ;;
    --url) SUPA_URL="${2:-}"; shift ;;
    --url=*) SUPA_URL="${1#--url=}" ;;
    --key) SUPA_KEY="${2:-}"; shift ;;
    --key=*) SUPA_KEY="${1#--key=}" ;;
    -y|--yes) ASSUME_YES=1 ;;
    -h|--help) usage; exit 0 ;;
    --) shift; break ;;
    *) die "unknown flag: $1 (try --help)" ;;
  esac
  shift
done
if [ "$SERVE_FLAG" = "1" ] && [ -z "$BACKEND" ]; then BACKEND=docker; fi
case "$BACKEND" in ""|remote|docker|native|supabase) ;; *) die "unknown backend: $BACKEND (want remote|docker|native|supabase)" ;; esac

OS="$(uname -s)"
ARCH="$(uname -m)"
IS_TERMUX=0
if [ -n "${TERMUX_VERSION:-}" ]; then
  IS_TERMUX=1
else
  case "${PREFIX:-}" in
    */com.termux/files/usr) IS_TERMUX=1 ;;
  esac
fi
case "$OS" in
  Linux)
    if [ "$IS_TERMUX" = "1" ]; then GOOS=android; else GOOS=linux; fi
    ;;
  Darwin) GOOS=darwin ;;
  MINGW*|MSYS*|CYGWIN*) GOOS=windows ;;
  *) die "unsupported OS: $OS (cyb ships linux/darwin/windows/android-Termux — see the release page for alternatives)" ;;
esac
case "$ARCH" in
  x86_64|amd64)
    [ "$GOOS" != "android" ] || die "Termux prebuilt CLI currently supports Android arm64 only (device reports $ARCH)"
    GOARCH=amd64
    ;;
  arm64|aarch64) GOARCH=arm64 ;;
  *)
    if [ "$GOOS" = "android" ]; then
      die "Termux prebuilt CLI currently supports Android arm64 only (device reports $ARCH)"
    fi
    die "unsupported arch: $ARCH (cyb ships amd64/arm64 for this OS)"
    ;;
esac

# Termux installs are per-app and writable without root. If cyb is already on
# PATH, update that same writable location; otherwise use Termux's own bin dir.
if [ -z "$INSTALL_DIR" ]; then
  if [ "$IS_TERMUX" = "1" ]; then
    EXISTING_CYB="$(command -v cyb 2>/dev/null || true)"
    if [ -n "$EXISTING_CYB" ] && [ -f "$EXISTING_CYB" ] && [ -w "$(dirname "$EXISTING_CYB")" ]; then
      INSTALL_DIR="$(dirname "$EXISTING_CYB")"
    elif [ -n "${PREFIX:-}" ]; then
      INSTALL_DIR="$PREFIX/bin"
    else
      INSTALL_DIR="$HOME/.local/bin"
    fi
  else
    INSTALL_DIR="$HOME/.local/bin"
  fi
fi

# Per-OS install hint for a missing tool (package name may differ from the
# tool name, e.g. sha256sum lives in coreutils). Distro detection reads
# /etc/os-release in a subshell so no variables leak into the installer.
pkg_hint() {
  tool="$1"; pkg="$2"
  case "$OS" in
    Darwin) printf 'brew install %s' "$pkg" ;;
    Linux)
      if [ "$IS_TERMUX" = "1" ]; then
        printf 'pkg install %s' "$pkg"
        return
      fi
      dist_id=""
      if [ -r /etc/os-release ]; then
        dist_id="$( ( . /etc/os-release 2>/dev/null; printf '%s %s' "${ID:-}" "${ID_LIKE:-}" ) )"
      fi
      case "$dist_id" in
        *debian*|*ubuntu*) printf 'sudo apt-get update && sudo apt-get install -y %s' "$pkg" ;;
        *fedora*|*rhel*|*centos*|*rocky*|*alma*) printf 'sudo dnf install -y %s' "$pkg" ;;
        *arch*|*cachyos*|*manjaro*|*endeavouros*) printf 'sudo pacman -S --needed %s' "$pkg" ;;
        *alpine*) printf 'sudo apk add %s' "$pkg" ;;
        *suse*) printf 'sudo zypper install -y %s' "$pkg" ;;
        *) printf 'your package manager (%s)' "$pkg" ;;
      esac
      ;;
    MINGW*|MSYS*|CYGWIN*) printf 'the Git-Bash/MSYS2 package for %s (pacman -S %s on MSYS2)' "$tool" "$pkg" ;;
    *) printf 'a %s package providing %s' "$pkg" "$tool" ;;
  esac
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "need '$1' on PATH — $(pkg_hint "$1" "$2")"
}
need curl curl
need tar tar
# Fail fast on the hash tool too: macOS ships shasum, everywhere else
# sha256sum — don't waste a download discovering neither exists.
if command -v sha256sum >/dev/null 2>&1; then
  HASH_TOOL=sha256sum
elif command -v shasum >/dev/null 2>&1; then
  HASH_TOOL=shasum
else
  die "need 'sha256sum' or 'shasum' to verify the download — $(pkg_hint sha256sum coreutils)"
fi

if [ "$WANT_VERSION" = "latest" ]; then
  say "resolving latest release…"
  TAG="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | head -1 | cut -d'"' -f4)"
  [ -n "$TAG" ] || die "could not resolve the latest release"
else
  TAG="$WANT_VERSION"
fi

ASSET="cyb-$GOOS-$GOARCH.tar.gz"
BASE="https://github.com/$REPO/releases/download/$TAG"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

say "installing cyb $TAG ($GOOS/$GOARCH)…"
if ! curl -fsSL -o "$TMP/$ASSET" "$BASE/$ASSET"; then
  if [ "$GOOS" = "android" ]; then
    die "download failed: $BASE/$ASSET — older releases may not include the Termux binary; use a release published after Termux support was added"
  fi
  die "download failed: $BASE/$ASSET"
fi
curl -fsSL -o "$TMP/SHA256SUMS.txt" "$BASE/SHA256SUMS.txt" || die "checksum file download failed"

# Verify against the published sums with the tool detected up front.
SUM_LINE="$(grep -E "[[:space:]]$ASSET\$" "$TMP/SHA256SUMS.txt" | head -1)"
[ -n "$SUM_LINE" ] || die "no checksum entry for $ASSET"
WANT_SUM="$(printf '%s' "$SUM_LINE" | awk '{print $1}')"
if [ "$HASH_TOOL" = "sha256sum" ]; then
  GOT_SUM="$(sha256sum "$TMP/$ASSET" | awk '{print $1}')"
else
  GOT_SUM="$(shasum -a 256 "$TMP/$ASSET" | awk '{print $1}')"
fi
[ "$GOT_SUM" = "$WANT_SUM" ] || die "checksum mismatch for $ASSET (integrity: refusing to install)"

tar -xzf "$TMP/$ASSET" -C "$TMP" || die "extract failed"
BIN="$TMP/cyb"
[ "$GOOS" = "windows" ] && BIN="$TMP/cyb.exe"
[ -f "$BIN" ] || die "tarball contains no cyb binary"
DEST_NAME=cyb
[ "$GOOS" = "windows" ] && DEST_NAME=cyb.exe

mkdir -p "$INSTALL_DIR" || die "cannot create $INSTALL_DIR"
if [ -e "$INSTALL_DIR/$DEST_NAME" ]; then
  say "updating existing installation in $INSTALL_DIR"
fi
mv -f "$BIN" "$INSTALL_DIR/$DEST_NAME" || die "cannot write to $INSTALL_DIR"
chmod +x "$INSTALL_DIR/$DEST_NAME" 2>/dev/null || true

# On Windows shells the binary is cyb.exe — report the real path so the
# version probe below actually executes it instead of falling back.
CYB_BIN="$INSTALL_DIR/$DEST_NAME"
say "installed: $CYB_BIN ($("$CYB_BIN" version 2>/dev/null || echo "$TAG"))"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) printf '→ add to PATH:  export PATH="%s:$PATH"\n' "$INSTALL_DIR" ;;
esac
# The binary runs from here on via its absolute path, but later `cyb`
# invocations in this shell still need the dir on PATH.
export PATH="$INSTALL_DIR:$PATH"

# --- onboarding: backend + setup --------------------------------------
# Piped installs (`curl | bash`) hold stdin, so prompts read from /dev/tty.
have_tty() { [ -r /dev/tty ] && [ -w /dev/tty ]; }

ask_yes() { # ask_yes "prompt" [default Y|N] -> 0 on yes
  _q="$1"; _d="${2:-Y}"
  [ "$ASSUME_YES" = "1" ] && return 0
  have_tty || return 1
  _hint="[Y/n]"; [ "$_d" = "N" ] && _hint="[y/N]"
  printf '◈ %s %s ' "$_q" "$_hint" > /dev/tty
  _a=""; IFS= read -r _a < /dev/tty || return 1
  case "$_a" in "") [ "$_d" = "Y" ];; [Yy]*) return 0;; *) return 1;; esac
}

ask_val() { # ask_val "prompt" [default] -> prints the value (default when headless)
  _q="$1"; _d="${2:-}"
  if ! have_tty; then printf '%s' "$_d"; return 1; fi
  if [ -n "$_d" ]; then printf '◈ %s [%s]: ' "$_q" "$_d" > /dev/tty
  else printf '◈ %s: ' "$_q" > /dev/tty; fi
  _a=""; IFS= read -r _a < /dev/tty || { printf '%s' "$_d"; return 1; }
  [ -z "$_a" ] && _a="$_d"
  printf '%s' "$_a"
}

ask_secret() { # ask_secret "prompt" -> prints the value (empty when headless)
  _q="$1"
  have_tty || return 1
  printf '◈ %s: ' "$_q" > /dev/tty
  _a=""; IFS= read -rs _a < /dev/tty || return 1
  printf '\n' > /dev/tty
  printf '%s' "$_a"
}

choose_backend() { # prints remote|docker|native|supabase, 1 on skip/headless
  have_tty || return 1
  cat > /dev/tty <<'MENU'
◈ Where should cyb connect? (backend)
  1) docker    local dashboard container — recommended (`cyb serve up`)
  2) native    installed CyberManju OS app dashboard (localhost)
  3) remote    dashboard URL you already run
  4) CLI Limited  Supabase broker-direct, read-only
  5) skip      configure later with `cyb backend use`
MENU
  printf '◈ backend [1]: ' > /dev/tty
  _c=""; IFS= read -r _c < /dev/tty || return 1
  case "${_c:-1}" in
    1|docker) printf 'docker' ;;
    2|native) printf 'native' ;;
    3|remote) printf 'remote' ;;
    4|supabase) printf 'supabase' ;;
    *) return 1 ;;
  esac
}

run_backend() {
  if [ "$IS_TERMUX" = "1" ]; then
    case "$BACKEND" in
      docker|native)
        say "$BACKEND is not part of the Termux CLI-only install; configure a remote or CLI Limited backend with cyb"
        return 1
        ;;
    esac
  fi
  case "$BACKEND" in
    docker)
      say "starting local dashboard (cyb serve up)…"
      _cmd=("$CYB_BIN" serve up)
      [ -n "$PORT_WANT" ] && _cmd+=(--port "$PORT_WANT")
      [ -n "$DATA_WANT" ] && _cmd+=(--data "$DATA_WANT")
      [ -n "$IMAGE_WANT" ] && _cmd+=(--image "$IMAGE_WANT")
      "${_cmd[@]}" || say "serve up failed — retry with: cyb serve up"
      ;;
    native)
      say "selecting native-app backend…"
      _cmd=("$CYB_BIN" backend use native)
      [ -n "$PORT_WANT" ] && _cmd+=(--port "$PORT_WANT")
      "${_cmd[@]}" || say "native backend needs the app running — retry with: cyb backend use native"
      ;;
    remote)
      _srv="$SERVER_WANT"
      [ -z "$_srv" ] && _srv="$(ask_val "Dashboard URL" "http://127.0.0.1:3456")"
      if [ -z "$_srv" ]; then
        say "remote backend needs a URL — skipped (cyb backend use remote --server URL)"
        return 1
      fi
      "$CYB_BIN" backend use remote --server "$_srv" || say "backend select failed — retry with: cyb backend use remote --server $_srv"
      SERVER_WANT="$_srv"
      ;;
    supabase)
      [ -z "$SUPA_URL" ] && SUPA_URL="$(ask_val "Supabase project URL" "")"
      [ -z "$SUPA_KEY" ] && SUPA_KEY="$(ask_secret "Supabase anon key")"
      if [ -z "$SUPA_URL" ] || [ -z "$SUPA_KEY" ]; then
        say "supabase needs --url + --key — skipped (cyb backend use supabase --url URL --key KEY)"
        return 1
      fi
      "$CYB_BIN" backend use supabase --url "$SUPA_URL" --key "$SUPA_KEY" \
        || say "backend select failed — retry with: cyb backend use supabase"
      ;;
  esac
}

run_setup() {
  _srv_args=()
  [ -n "$SERVER_WANT" ] && _srv_args=(--server "$SERVER_WANT")
  if [ -n "$SETUP_USER" ] && [ -n "$SETUP_PASS" ]; then
    say "running cyb setup (non-interactive)…"
    _cmd=("$CYB_BIN" setup --username "$SETUP_USER" --password "$SETUP_PASS")
    [ -n "$SETUP_DISPLAY" ] && _cmd+=(--display-name "$SETUP_DISPLAY")
    _cmd+=("${_srv_args[@]}")
    "${_cmd[@]}" || say "setup failed — re-run with: cyb setup"
    return 0
  fi
  if ! have_tty; then
    say "setup skipped (non-interactive) — run: cyb setup"
    return 1
  fi
  say "launching the setup walkthrough…"
  "$CYB_BIN" setup "${_srv_args[@]}" < /dev/tty || say "setup exited — re-run with: cyb setup"
}

WANT_SETUP=0
case "$SETUP_PREF" in
  1|yes|true|always) WANT_SETUP=1 ;;
  0|no|false|never|skip) WANT_SETUP=0 ;;
  *)
    if [ -n "$SETUP_USER" ]; then WANT_SETUP=1
    elif ask_yes "Run the setup walkthrough now (cyb setup)?" Y; then WANT_SETUP=1
    else WANT_SETUP=0; fi
    ;;
esac

if [ -n "$BACKEND" ]; then
  run_backend || true
elif [ "$WANT_SETUP" = "1" ] && [ "$ASSUME_YES" != "1" ] && [ "$IS_TERMUX" != "1" ]; then
  if _pick="$(choose_backend)"; then BACKEND="$_pick"; run_backend || true; fi
elif [ "$WANT_SETUP" = "1" ] && [ "$IS_TERMUX" = "1" ]; then
  say "Termux installs the Go CLI only; choose Remote dashboard or CLI Limited in cyb setup"
fi

if [ "$WANT_SETUP" = "1" ]; then
  run_setup || true
else
  printf '→ start here:  cyb setup\n'
fi
