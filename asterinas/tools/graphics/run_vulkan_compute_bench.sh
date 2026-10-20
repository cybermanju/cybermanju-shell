#!/bin/sh
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM
"$script_dir/build_vulkan_compute_bench.sh" "$tmp_dir/vulkan_compute_bench"
"$tmp_dir/vulkan_compute_bench" --shader "$tmp_dir/vulkan_compute_bench.spv" "$@"
