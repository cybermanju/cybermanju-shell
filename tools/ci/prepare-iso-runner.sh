#!/usr/bin/env bash
# ISO build tools live in Docker. These unrelated host SDKs are never consumed
# by the ISO job. Do not use this script on a local or self-hosted computer.
set -euo pipefail
[[ ${GITHUB_ACTIONS:-} == true && ${RUNNER_ENVIRONMENT:-} == github-hosted &&
   ${RUNNER_OS:-} == Linux && ${GITHUB_WORKSPACE:-} == /home/runner/work/* &&
   $EUID == 0 ]] || {
    echo 'SDK cleanup is restricted to ephemeral GitHub-hosted Linux runners.' >&2
    exit 2
}
# Exact SDK directories only: no Docker, Nix, Rust, toolcache, user home, or
# workspace cleanup. No packages are uninstalled and no caches are pruned.
for sdk in /usr/share/dotnet /usr/local/lib/android /opt/ghc; do
    [[ -e "$sdk" || -L "$sdk" ]] || continue
    [[ -d "$sdk" && ! -L "$sdk" && $(realpath "$sdk") == "$sdk" ]] || {
        echo "Refusing unexpected SDK path: $sdk" >&2; exit 2;
    }
    [[ $(stat -c %d "$sdk") == $(stat -c %d "$GITHUB_WORKSPACE") ]] || {
        echo "Refusing SDK on another filesystem: $sdk" >&2; exit 2;
    }
    if mountpoint -q "$sdk"; then
        echo "Refusing mounted SDK directory: $sdk" >&2; exit 2
    fi
    du -sx -B1 "$sdk"
    rm -rf --one-file-system -- "$sdk"
done
df -h "$GITHUB_WORKSPACE"
