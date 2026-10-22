#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Build-time instrumentation for disposable diagnostic guests only.
# Xtrace can expose activation arguments: never enable on configurations with secrets.
set -euo pipefail
[[ $# == 1 ]] || { echo 'Usage: trace-activation.sh ACTIVATE' >&2; exit 2; }
source=$1
[[ -s "$source" && -x "$source" ]] || { echo 'Missing executable activate' >&2; exit 1; }
IFS= read -r shebang < "$source"
case "$shebang" in
  '#!'*/bin/bash|'#!'*/bin/bash\ *) ;;
  *) echo 'Unsupported activate interpreter; refusing to instrument' >&2; exit 1 ;;
esac
# Nix may expose activate through a store symlink. Replace the output link,
# never edit its shared immutable target. Keep the original interpreter/options.
temporary=$(mktemp "${source}.trace.XXXXXX")
trap 'rm -f -- "$temporary"' EXIT
{
  printf '%s\n' "$shebang"
  cat <<'TRACE'
# CI-only diagnostic xtrace; these lines are not boot-success markers.
PS4='+CYBERMANJU_ACTIVATION:${LINENO}: '
set -x
TRACE
  tail -n +2 -- "$source"
} > "$temporary"
bash -n "$temporary"
chmod --reference="$source" "$temporary"
mv -f -- "$temporary" "$source"
