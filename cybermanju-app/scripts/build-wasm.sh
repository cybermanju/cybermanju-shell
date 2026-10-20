#!/usr/bin/env bash
# CyberManju OS — Build wasm (scripts/build-wasm.sh).
# Role: ops script (CI/local runbook).
# Exports: module root (see items below).
# Integrates: standalone (no imports).
# Contracts: none special (pure logic; errors bubble to caller).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "=== CyberManju OS — WASM Build for GitHub Pages ==="
echo ""

cd "$PROJECT_ROOT"
npm ci

echo "[1/2] Building WASM frontend..."
npx vite build --config vite.config.wasm.ts

echo "[2/2] WASM build complete!"
echo "Output: dist-wasm/"
ls -la dist-wasm/ 2>/dev/null || echo "(no output found)"