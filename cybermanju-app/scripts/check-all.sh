#!/usr/bin/env bash
# CyberManju OS — Check all (scripts/check-all.sh).
# Role: ops script (CI/local runbook).
# Exports: module root (see items below).
# Integrates: standalone (no imports).
# Contracts: none special (pure logic; errors bubble to caller).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

echo "=== CyberManju OS — Full Check ==="
echo ""

echo "[1/3] TypeScript type check..."
npm run typecheck

echo "[2/3] Rust cargo check..."
npm run rust:check

echo "[3/3] Rust clippy..."
npm run rust:clippy

echo ""
echo "=== All checks passed ==="