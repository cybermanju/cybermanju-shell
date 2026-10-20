#!/usr/bin/env bash
# CyberManju OS — Dev (scripts/dev.sh).
# Role: ops script (CI/local runbook).
# Exports: module root (see items below).
# Integrates: standalone (no imports).
# Contracts: none special (pure logic; errors bubble to caller).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

echo "=== CyberManju OS — Development Mode ==="
echo "Starting Vite dev server + Tauri..."
echo ""

npm run tauri:dev