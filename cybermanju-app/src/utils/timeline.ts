// CyberManju OS — Timeline (src/utils/timeline.ts).
// Role: "On this day" / date-slider over the archive (ANALYSIS.md §4.3) —
// pure date bucketing over file/archive rows the DB already holds.
// Exports: TimelineFile, DayBucket, groupByDay, onThisDay, timelineRange.
// Integrates: standalone; ArchivePanel TIMELINE tab renders it over
// store.files (createdAt) + face/GPS counts already on FileNode.
// Contracts: pure, never throws; unparseable dates are excluded, never
// fabricated (honest-limit rule).
export interface TimelineFile {
  id: string
  name: string
  createdAt: string
  modifiedAt?: string
  gpsLat?: number | null
  faceGroupIds?: string[]
}

export interface DayBucket {
  /** `YYYY-MM-DD` (UTC). */
  day: string
  count: number
  withGps: number
  withFaces: number
  files: TimelineFile[]
}

function dayOf(iso: string): string | null {
  const t = Date.parse(iso)
  if (Number.isNaN(t)) return null
  return new Date(t).toISOString().slice(0, 10)
}

/** Bucket files by creation day, newest day first. */
export function groupByDay(files: TimelineFile[]): DayBucket[] {
  const buckets = new Map<string, TimelineFile[]>()
  for (const file of files) {
    const day = dayOf(file.createdAt)
    if (!day) continue
    const bucket = buckets.get(day)
    if (bucket) bucket.push(file)
    else buckets.set(day, [file])
  }
  const out: DayBucket[] = []
  for (const [day, members] of buckets) {
    out.push({
      day,
      count: members.length,
      withGps: members.filter((f) => f.gpsLat !== undefined && f.gpsLat !== null).length,
      withFaces: members.filter((f) => f.faceGroupIds && f.faceGroupIds.length > 0).length,
      files: members,
    })
  }
  out.sort((a, b) => (a.day < b.day ? 1 : -1))
  return out
}

/** Files created on this month/day in any year (the "On this day" card). */
export function onThisDay(files: TimelineFile[], month: number, day: number): TimelineFile[] {
  return files.filter((f) => {
    const t = Date.parse(f.createdAt)
    if (Number.isNaN(t)) return false
    const d = new Date(t)
    return d.getUTCMonth() + 1 === month && d.getUTCDate() === day
  })
}

/** Oldest/newest creation day in the set, or null when nothing parses. */
export function timelineRange(files: TimelineFile[]): { min: string; max: string } | null {
  let min = ''
  let max = ''
  for (const file of files) {
    const day = dayOf(file.createdAt)
    if (!day) continue
    if (!min || day < min) min = day
    if (!max || day > max) max = day
  }
  return min ? { min, max } : null
}
