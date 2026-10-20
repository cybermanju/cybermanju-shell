#!/usr/bin/env bash
# CyberManju OS — Backup retention tests (scripts/backup.test.sh).
# Role: exercise backup creation and safe snapshot pruning.
# Exports: none.
# Integrates: scripts/backup.sh, bash, coreutils.
# Contracts: rotation removes only completed marked snapshots; keep=0 retains all.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT
mkdir -p "$tmp/data" "$tmp/backups"
printf 'db state\n' > "$tmp/data/cybermanju.db"

run_backup() {
  DB_PATH="$tmp/data/cybermanju.db" \
    BACKUP_DIR="$tmp/backups" \
    bash "$repo/scripts/backup.sh" "$@"
}

first="$(run_backup --keep 1 | sed -n 's/^backup: //p')"
[[ -d "$first" && -f "$first/MANIFEST.txt" ]]
grep -Fxq 'format=cybermanju-backup-v1' "$first/MANIFEST.txt"

# A timestamp-shaped directory that is not one of our completed snapshots is
# not eligible for recursive pruning.
unrelated="$tmp/backups/20000101T000000Z"
mkdir "$unrelated"
printf 'keep me\n' > "$unrelated/important.txt"
sleep 2
second="$(run_backup --keep 1 | sed -n 's/^backup: //p')"
[[ -d "$second" && ! -e "$first" ]]
[[ -f "$unrelated/important.txt" ]]

# Zero retention is an explicit no-pruning mode.
sleep 2
third="$(run_backup --keep 0 | sed -n 's/^backup: //p')"
[[ -d "$second" && -d "$third" ]]
printf 'backup retention tests passed\n'
