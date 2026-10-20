#!/usr/bin/env bash
# CyberManju OS — Backup (scripts/backup.sh).
# AGENT-4 item 12 — snapshot the redb database and the Tantivy search index.
#
#   scripts/backup.sh                  # snapshot now, keep 7
#   scripts/backup.sh --keep 30        # keep the newest 30 snapshots
#   scripts/backup.sh --keep 0          # disable automatic pruning
#   scripts/backup.sh --dir /backups   # write somewhere else
#
# Same variable names the server uses, so a snapshot of a running deployment
# needs no extra configuration. This is a file-copy backup, not an application-
# consistent snapshot; stop the service for a reliable restore point.
#
#   DB_PATH             redb database          (default /data/cybermanju.db)
#   SEARCH_INDEX_PATH   Tantivy index directory (default <dir of DB_PATH>/tantivy_index)
#   BACKUP_DIR          snapshot destination   (default <dir of DB_PATH>/backups)
#   BACKUP_KEEP         snapshots to keep      (default 7)
#
# Consistency: `cp --reflink=auto` is used when available, but even a reflink
# copy is not a coordinated snapshot of the database and index. Without reflink
# the database copy may also race live writes. Stop the service before running
# this script when the backup must be restorable (see docs/SECURITY.md).
set -euo pipefail

cd "$(dirname "$0")/.."

keep="${BACKUP_KEEP:-7}"
dest_root="${BACKUP_DIR:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    --keep) keep="${2:?--keep needs a number}"; shift 2 ;;
    --dir) dest_root="${2:?--dir needs a path}"; shift 2 ;;
    -h|--help) sed -n '2,22p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

case "$keep" in ''|*[!0-9]*) echo "--keep must be a number" >&2; exit 2 ;; esac
if [ "${#keep}" -gt 6 ]; then
  echo "--keep must be between 0 and 100000" >&2
  exit 2
fi
keep=$((10#$keep))
if [ "$keep" -gt 100000 ]; then
  echo "--keep must be between 0 and 100000" >&2
  exit 2
fi

db_path="${DB_PATH:-/data/cybermanju.db}"
if [ ! -f "$db_path" ]; then
  echo "database not found: $db_path (set DB_PATH)" >&2
  exit 1
fi

db_dir="$(dirname "$db_path")"
index_path="${SEARCH_INDEX_PATH:-$db_dir/tantivy_index}"
dest_root="${dest_root:-$db_dir/backups}"

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
dest="$dest_root/$stamp"
mkdir -p "$dest_root"
# Never reuse a timestamp directory: doing so could overwrite a prior backup.
if ! mkdir "$dest"; then
  echo "backup destination already exists: $dest (wait for the next second or choose another --dir)" >&2
  exit 1
fi

# Reflink when the platform supports it (btrfs/xfs/APFS); otherwise fall back
# to a portable copy so the script keeps working on ext4 and busybox.
copy_one() {
  cp --reflink=auto -- "$1" "$2" 2>/dev/null || cp -p -- "$1" "$2"
}

copy_one "$db_path" "$dest/$(basename "$db_path")"
if [ -d "$index_path" ]; then
  cp -R -- "$index_path" "$dest/index"
fi

# Manifest: enough to tell a snapshot apart and to check it was not truncated.
manifest="$dest/MANIFEST.txt"
manifest_tmp="$dest/.MANIFEST.tmp"
{
  echo "format=cybermanju-backup-v1"
  echo "taken_at_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host=$(hostname 2>/dev/null || echo unknown)"
  echo "database=$db_path"
  echo "database_bytes=$(wc -c < "$dest/$(basename "$db_path")" | tr -d ' ')"
  if command -v sha256sum >/dev/null 2>&1; then
    echo "database_sha256=$(sha256sum "$dest/$(basename "$db_path")" | cut -d' ' -f1)"
  fi
  if [ -d "$dest/index" ]; then
    echo "search_index=$index_path"
    echo "search_index_bytes=$(du -sk "$dest/index" | cut -f1)K"
  else
    echo "search_index=absent"
  fi
} > "$manifest_tmp"
mv -- "$manifest_tmp" "$manifest"

# Prune only completed snapshots created by this script. An exact timestamp
# name plus the format marker prevents unrelated directories (or partial
# backups) from being recursively deleted. `--keep 0` disables pruning.
if [ "$keep" -gt 0 ]; then
  snapshots=()
  for candidate in "$dest_root"/20*; do
    [ -d "$candidate" ] && [ ! -L "$candidate" ] || continue
    name="${candidate##*/}"
    [[ "$name" =~ ^[0-9]{8}T[0-9]{6}Z$ ]] || continue
    [ -f "$candidate/MANIFEST.txt" ] && [ ! -L "$candidate/MANIFEST.txt" ] || continue
    grep -Fxq 'format=cybermanju-backup-v1' "$candidate/MANIFEST.txt" || continue
    snapshots+=("$name")
  done
  if [ "${#snapshots[@]}" -gt "$keep" ]; then
    mapfile -t ordered < <(printf '%s\n' "${snapshots[@]}" | sort -r)
    for old in "${ordered[@]:$keep}"; do
      rm -rf -- "$dest_root/$old"
    done
  fi
fi

echo "backup: $dest"
grep -E '^(database_bytes|database_sha256|search_index)=' "$manifest" || true
