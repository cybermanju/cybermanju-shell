#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Compile a trusted .ko, prepare it inside OSTD/QEMU, and verify frame readback.
# NEVER call module init/exit, insmod, modprobe or a Linux syscall for loading.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
OUT=${1:-"$ROOT/artifacts/prepared-module-runtime"}
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
printf '{"status":"not_verified","linux_modules_executed":false}\n' > "$OUT/result.json"
if (( $# > 2 )); then echo 'Runtime modes are mutually exclusive' >&2; exit 2; fi
MODE=${2:---prepared-module}
case "$MODE" in
    --prepared-module|--near-text-prepared|--signed32-prepared|--signed32-mapping-smp|--signed32-search|--internal-exports) ;;
    *) echo 'Unknown prepared runtime mode' >&2; exit 2 ;;
esac
bash "$ROOT/tools/ci/test_linux_module_noimports.sh" "$OUT/fixture" > "$OUT/fixture-build.log" 2>&1
export CYBERMANJU_STAGING_FIXTURE="$OUT/fixture/cybermanju_noimports_fixture.ko"
bash "$ROOT/tools/ci/test_module_staging_runtime.sh" "$OUT" "$MODE"
