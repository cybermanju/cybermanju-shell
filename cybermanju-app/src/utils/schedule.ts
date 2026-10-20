// CyberManju OS — schedule expressions: TypeScript twin of
// `crates/os/src/schedule.rs`.
//
// Two shapes, parsed identically on desktop/server (Rust) and in the
// browser (this file — the WASM build has no daemon thread, so the store
// ticks due rows itself):
//
//   * classic 5-field cron: `m h dom mon dow` — `* , - /` subsets,
//     aliases `@hourly/@daily/@weekly/@monthly/@yearly`;
//   * interval: `every 10m`, `every 2h`, `every 30s`, `every 1d`.
//
// `parseSchedule` throws `invalid: …` (AGENT-1 family) — the same strings
// the Rust parser produces, so both sides reject with one voice.

export type EveryUnit = 's' | 'm' | 'h' | 'd'

export type ScheduleSpec =
  | {
      kind: 'cron'
      m: number[]
      h: number[]
      dom: number[]
      mon: number[]
      dow: number[]
    }
  | { kind: 'every'; n: number; unit: EveryUnit }

/** Search cap for `nextAfter`: 5 years of minutes ≈ 2.6 M iterations. */
const MAX_SEARCH_STEPS = 5 * 366 * 24 * 60

const ALIASES: Record<string, string> = {
  '@hourly': '0 * * * *',
  '@daily': '0 0 * * *',
  '@midnight': '0 0 * * *',
  '@weekly': '0 0 * * 0',
  '@monthly': '0 0 1 * *',
  '@yearly': '0 0 1 1 *',
  '@annually': '0 0 1 1 *',
}

/** Parse a user expression. Throws `invalid: …` on anything unusable. */
export function parseSchedule(raw: string): ScheduleSpec {
  const s = raw.trim().toLowerCase()
  if (!s) throw new Error('invalid: empty schedule expression')
  const alias = ALIASES[s]
  if (alias) return parseCron(alias)
  if (s.startsWith('every ')) return parseEvery(s.slice('every '.length).trim())
  return parseCron(s)
}

/** Next fire strictly after `after`, or `null` (impossible date). */
export function scheduleNextAfter(spec: ScheduleSpec, after: Date): Date | null {
  if (spec.kind === 'every') {
    const secs =
      spec.unit === 's'
        ? spec.n
        : spec.unit === 'm'
          ? spec.n * 60
          : spec.unit === 'h'
            ? spec.n * 3600
            : spec.n * 86_400
    if (secs <= 0) return null
    return new Date(after.getTime() + secs * 1000)
  }
  return nextCron(spec, after)
}

/** Human-readable form for the UI preview (full-range fields → `*`). */
export function describeSchedule(spec: ScheduleSpec): string {
  if (spec.kind === 'every') return `every ${spec.n}${spec.unit}`
  return [
    fmtField(spec.m, 0, 59),
    fmtField(spec.h, 0, 23),
    fmtField(spec.dom, 1, 31),
    fmtField(spec.mon, 1, 12),
    fmtField(spec.dow, 0, 6),
  ].join(' ')
}

function fmtField(v: number[], min: number, max: number): string {
  return v.length === 0 || v.length === max - min + 1 ? '*' : v.join(',')
}

/** Convenience: parse + first fire after `after`, or `null`. */
export function nextFire(raw: string, after: Date): Date | null {
  let spec: ScheduleSpec
  try {
    spec = parseSchedule(raw)
  } catch {
    return null
  }
  return scheduleNextAfter(spec, after)
}

export interface SchedulePreview {
  ok: boolean
  /** `invalid: …` when `ok` is false. */
  error?: string
  /** Parsed-and-described expression (round-trip proof for the UI). */
  label?: string
  next?: Date | null
  /** Human countdown — `in 4m`, `in 2h 5m`, `never`. */
  in?: string
}

/** Live validation + "next: in 4m" preview for the schedule editor. */
export function previewSchedule(raw: string, now: Date = new Date()): SchedulePreview {
  try {
    const spec = parseSchedule(raw)
    const next = scheduleNextAfter(spec, now)
    return {
      ok: true,
      label: describeSchedule(spec),
      next,
      in: next ? formatIn(next.getTime() - now.getTime()) : 'never',
    }
  } catch (err) {
    return { ok: false, error: err instanceof Error ? err.message : String(err) }
  }
}

/** Compact countdown: `in 4m` · `in 2h 5m` · `in 3d 4h` · `now`. */
export function formatIn(ms: number): string {
  if (ms <= 0) return 'now'
  const s = Math.floor(ms / 1000)
  if (s < 60) return `in ${s}s`
  const m = Math.floor(s / 60)
  if (m < 60) return `in ${m}m`
  const h = Math.floor(m / 60)
  if (h < 24) return `in ${h}h ${m % 60}m`
  const d = Math.floor(h / 24)
  return `in ${d}d ${h % 24}h`
}

// ── internals ───────────────────────────────────────────────────────────

function parseEvery(s: string): ScheduleSpec {
  if (!s) throw new Error('invalid: `every` needs a value like `10m`')
  const unit = s.slice(-1)
  const num = s.slice(0, -1)
  const n = Number(num)
  if (!Number.isInteger(n) || !/^\d+$/.test(num)) {
    throw new Error(`invalid: \`every ${s}\` is not a number + unit`)
  }
  if (n === 0) throw new Error('invalid: `every 0…` never fires')
  const map: Record<string, EveryUnit> = {
    s: 's', sec: 's', secs: 's',
    m: 'm', min: 'm', mins: 'm',
    h: 'h', hr: 'h', hrs: 'h',
    d: 'd', day: 'd', days: 'd',
  }
  const u = map[unit]
  if (!u) throw new Error(`invalid: unknown unit \`${unit}\` in \`every ${s}\` (use s|m|h|d)`)
  return { kind: 'every', n, unit: u }
}

function parseCron(s: string): ScheduleSpec {
  const fields = s.split(/\s+/).filter(Boolean)
  if (fields.length !== 5) {
    throw new Error(
      `invalid: cron needs 5 fields (m h dom mon dow), got ${fields.length} — \`* * * * *\` is every minute`,
    )
  }
  const dow = parseField(fields[4]!, 0, 7, 'day-of-week').map(v => (v === 7 ? 0 : v))
  dow.sort((a, b) => a - b)
  const uniq = dow.filter((v, i) => i === 0 || v !== dow[i - 1])
  return {
    kind: 'cron',
    m: parseField(fields[0]!, 0, 59, 'minute'),
    h: parseField(fields[1]!, 0, 23, 'hour'),
    dom: parseField(fields[2]!, 1, 31, 'day-of-month'),
    mon: parseField(fields[3]!, 1, 12, 'month'),
    dow: uniq,
  }
}

// Parse one cron field: `*`, `5`, `1-5`, star-slash-steps like `*/15`,
// `1-30/5`, comma lists → explicit values (ranges expand to a set).
function parseField(field: string, min: number, max: number, label: string): number[] {
  const out: number[] = []
  for (const rawPart of field.split(',')) {
    const p = rawPart.trim()
    if (!p) throw new Error(`invalid: empty ${label} field`)
    let range = p
    let step = 1
    const slash = p.indexOf('/')
    if (slash >= 0) {
      range = p.slice(0, slash)
      const st = Number(p.slice(slash + 1))
      if (!Number.isInteger(st) || st <= 0) {
        if (p.slice(slash + 1) === '0') throw new Error(`invalid: step 0 in ${label}`)
        throw new Error(`invalid: bad step \`${p.slice(slash + 1)}\` in ${label}`)
      }
      step = st
    }
    let lo: number
    let hi: number
    if (range === '*') {
      lo = min
      hi = max
    } else if (range.includes('-')) {
      const [a, b] = range.split('-')
      lo = Number((a ?? '').trim())
      hi = Number((b ?? '').trim())
      if (!Number.isInteger(lo)) {
        throw new Error(`invalid: bad range start in ${label} \`${field}\``)
      }
      if (!Number.isInteger(hi)) {
        throw new Error(`invalid: bad range end in ${label} \`${field}\``)
      }
    } else {
      lo = Number(range.trim())
      hi = lo
      if (!Number.isInteger(lo)) throw new Error(`invalid: bad ${label} value \`${range}\``)
    }
    if (lo < min || hi > max || lo > hi) {
      throw new Error(`invalid: ${label} \`${field}\` out of range ${min}-${max}`)
    }
    for (let v = lo; v <= hi; v += step) {
      if (!out.includes(v)) out.push(v)
    }
  }
  out.sort((a, b) => a - b)
  return out
}

/** Minute-iteration `next_after`, same semantics as the Rust side.
 * All calendar reads are **UTC** — the Rust daemon computes `next_fire_at`
 * in UTC, so the browser preview/tick must agree to the minute. */
function nextCron(
  spec: Extract<ScheduleSpec, { kind: 'cron' }>,
  after: Date,
): Date | null {
  const base = new Date(after.getTime())
  base.setUTCSeconds(0, 0)
  base.setUTCMinutes(base.getUTCMinutes() + 1)
  const domAny = spec.dom.length === 31
  const dowAny = spec.dow.length >= 7
  const dowHit = (w: number) => spec.dow.includes(w % 7)
  const dayHit = (t: Date) => {
    const dom = t.getUTCDate()
    const w = t.getUTCDay() // 0 = Sunday
    if (domAny && dowAny) return true
    if (!domAny && dowAny) return spec.dom.includes(dom)
    if (domAny && !dowAny) return dowHit(w)
    return spec.dom.includes(dom) || dowHit(w)
  }
  const probe = new Date()
  for (let step = 0; step < MAX_SEARCH_STEPS; step++) {
    probe.setTime(base.getTime() + step * 60_000)
    if (!spec.mon.includes(probe.getUTCMonth() + 1)) continue
    if (!dayHit(probe)) continue
    if (!spec.h.includes(probe.getUTCHours())) continue
    if (!spec.m.includes(probe.getUTCMinutes())) continue
    return new Date(probe.getTime())
  }
  return null
}
