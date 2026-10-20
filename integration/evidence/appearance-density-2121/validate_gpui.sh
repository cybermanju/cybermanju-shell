#!/usr/bin/env bash
# Local GPUI implementation checks only; no credentials, publication or GPU acceptance.
set -eo pipefail
cd /workspace/asterinas-gpui/gpui-shell
source ../evidence/gpui-second-review/dev-env.sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=6
OUT=/workspace/asterinas-gpui/evidence/appearance-density-2121
log="$OUT/gpui-rust-tests.log"
trap 'tail -n 50 "$log"' ERR
printf 'phase=rust-workspace-tests\n'
cargo test --workspace --offline --locked > "$log" 2>&1
log="$OUT/gpui-build.log"
printf 'phase=real-binary-build\n'
cargo build -p gpuishell --offline --locked > "$log" 2>&1
log="$OUT/gpui-python-tests.log"
printf 'phase=real-binary-cli-regressions\n'
python3 -m unittest discover -s scripts -p 'test_*.py' -v > "$log" 2>&1
log="$OUT/gpui-clippy.log"
printf 'phase=workspace-all-targets-clippy\n'
cargo clippy --workspace --all-targets --offline --locked > "$log" 2>&1
printf 'phase=all-final-gpui-checks-complete\n'
