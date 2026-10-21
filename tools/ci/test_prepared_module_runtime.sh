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
bash "$ROOT/tools/ci/test_linux_module_noimports.sh" "$OUT/fixture" > "$OUT/fixture-build.log" 2>&1
export CYBERMANJU_STAGING_FIXTURE="$OUT/fixture/cybermanju_noimports_fixture.ko"
bash "$ROOT/tools/ci/test_module_staging_runtime.sh" "$OUT" --prepared-module
