#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Host-only ownership/rollback checks with instrumented mock pages, NOT kernel execution.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cd "$ROOT"
rustc --edition=2024 --deny warnings --test tools/ci/test_module_staging.rs -o "$WORK/staging-tests"
"$WORK/staging-tests"
