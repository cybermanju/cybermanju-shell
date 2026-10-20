#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
output=${1:-"$script_dir/vulkan_compute_bench"}
mkdir -p "$(dirname -- "$output")"
glslangValidator -V --target-env vulkan1.2 "$script_dir/vector.comp" -o "$output.spv" >&2
cc -std=c11 -Wall -Wextra -Werror -O2 \
    "$script_dir/vulkan_compute_bench.c" -lvulkan -o "$output"
printf 'Built %s (shader: %s.spv)\n' "$output" "$output" >&2
