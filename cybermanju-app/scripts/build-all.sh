#!/usr/bin/env bash
# CyberManju OS — Build all (scripts/build-all.sh).
# Role: ops script (CI/local runbook).
# Exports: module root (see items below).
# Integrates: standalone (no imports).
# Contracts: none special (pure logic; errors bubble to caller).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "=== CyberManju OS — Full Build ==="
echo ""

# Check dependencies
command -v cargo >/dev/null 2>&1 || { echo "Error: cargo not found"; exit 1; }
command -v node >/dev/null 2>&1 || { echo "Error: node not found"; exit 1; }

echo "[1/4] Installing frontend dependencies..."
cd "$PROJECT_ROOT"
npm ci

echo "[2/4] Type-checking frontend..."
npm run typecheck

echo "[3/4] Building frontend..."
npm run build

echo "[4/4] Building Tauri application..."
npm run tauri:build

echo ""
echo "=== Build Complete ==="
echo "Artifacts in: target/release/bundle/"
ls -la target/release/bundle/ 2>/dev/null || echo "(no bundle directory found)"