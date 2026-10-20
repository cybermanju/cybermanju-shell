// CyberManju OS — Dedup (src/utils/dedup.ts).
// Role: storage-diet report — group files by BLAKE3 content hash, rank duplicate
// sets by reclaimable bytes, suggest the copy to keep (ANALYSIS.md §4.5).
// Exports: DupFile, DupGroup, DedupSummary, groupDuplicates, dedupSummary.
// Integrates: standalone (no imports); DiskManagerPage DIET tab renders it over
// store.files; deletions reuse the existing store.deleteFile action.
// Contracts: pure, never throws; files without a usable hash are unhashable and
// excluded (honest limit — no hash means no claim, never fabricated).
import type { FileNode } from '@/types'

/** Minimal file shape the report needs (FileNode satisfies it). */
export interface DupFile {
  id: string
  name: string
  sizeBytes: number
  modifiedAt: string
  hashBlake3?: string | null
  path?: string
}

/** One set of files sharing identical content. */
export interface DupGroup {
  /** Normalized (lowercase, trimmed) BLAKE3 hex all members share. */
  hash: string
  /** Representative size — identical content implies identical size. */
  sizeBytes: number
  /** Sum of every member's sizeBytes. */
  totalBytes: number
  /** Bytes freed by keeping one copy: totalBytes − sizeBytes. */
  reclaimableBytes: number
  /** Members, newest first (index 0 is the suggested keep). */
  files: DupFile[]
  /** Id of the suggested keep (newest modifiedAt, tie → first seen). */
  keepId: string
}

export interface DedupSummary {
  /** Duplicate sets found. */
  groupCount: number
  /** Files sitting in duplicate sets. */
  fileCount: number
  /** Bytes held by files in duplicate sets. */
  totalBytes: number
  /** Bytes freed if every set is reclaimed to one copy. */
  reclaimableBytes: number
  /** Files with no usable hash — invisible to the report. */
  unhashableCount: number
}

/** A hash counts only when it looks like a real BLAKE3 hex digest. */
export function normalizeHash(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const hash = value.trim().toLowerCase()
  if (hash.length < 16) return null
  if (!/^[0-9a-f]+$/.test(hash)) return null
  return hash
}

/** Newest-first ordering; unparseable dates sort last, stable on ties. */
function byNewest(a: DupFile, b: DupFile): number {
  const ta = Date.parse(a.modifiedAt)
  const tb = Date.parse(b.modifiedAt)
  const na = Number.isNaN(ta) ? -Infinity : ta
  const nb = Number.isNaN(tb) ? -Infinity : tb
  return nb - na
}

/**
 * Group files with identical content. Singletons and unhashable files are
 * dropped; groups rank by reclaimable bytes (largest win first).
 */
export function groupDuplicates(files: DupFile[]): DupGroup[] {
  const buckets = new Map<string, DupFile[]>()
  for (const file of files) {
    const hash = normalizeHash(file.hashBlake3)
    if (!hash) continue
    const bucket = buckets.get(hash)
    if (bucket) bucket.push(file)
    else buckets.set(hash, [file])
  }
  const groups: DupGroup[] = []
  for (const [hash, members] of buckets) {
    if (members.length < 2) continue
    const ordered = [...members].sort(byNewest)
    const sizeBytes = Math.max(...ordered.map((f) => f.sizeBytes || 0))
    const totalBytes = ordered.reduce((s, f) => s + (f.sizeBytes || 0), 0)
    groups.push({
      hash,
      sizeBytes,
      totalBytes,
      reclaimableBytes: Math.max(0, totalBytes - sizeBytes),
      files: ordered,
      keepId: ordered[0].id,
    })
  }
  groups.sort((a, b) => b.reclaimableBytes - a.reclaimableBytes)
  return groups
}

/** Roll groups up into the headline numbers for the DIET tab. */
export function dedupSummary(files: DupFile[], groups?: DupGroup[]): DedupSummary {
  const resolved = groups ?? groupDuplicates(files)
  let unhashableCount = 0
  for (const file of files) {
    if (!normalizeHash(file.hashBlake3)) unhashableCount += 1
  }
  return {
    groupCount: resolved.length,
    fileCount: resolved.reduce((s, g) => s + g.files.length, 0),
    totalBytes: resolved.reduce((s, g) => s + g.totalBytes, 0),
    reclaimableBytes: resolved.reduce((s, g) => s + g.reclaimableBytes, 0),
    unhashableCount,
  }
}

/** Convenience adapter so callers can pass store FileNodes directly. */
export function fromFileNodes(files: FileNode[]): DupFile[] {
  return files.map((f) => ({
    id: f.id,
    name: f.name,
    sizeBytes: f.sizeBytes,
    modifiedAt: f.modifiedAt,
    hashBlake3: f.hashBlake3,
    path: f.path,
  }))
}
