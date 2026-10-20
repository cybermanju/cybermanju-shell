// CyberManju OS — Watchers (src/utils/watchers.ts).
// Role: proactive agent watchers (ANALYSIS.md §4.6) — "new file synced →
// classify, tag, summarize, notify". Pure rule matching; execution (notify /
// setFileTags / agent classify) lives in the ProcessPanel WATCHERS tab, which
// runs on the cron scheduler the OS already ships, so the OS feels attended.
// Exports: WatchRule, WatchFile, matchWatcher, evaluateWatchers,
// parseWatchersStore, serializeWatchersStore.
// Integrates: standalone (localStorage `cybermanju.watchers.v1`);
// ProcessPanel WATCHERS tab executes matches.
// Contracts: pure, never throws; invalid regex matches nothing (never throws
// into the scheduler tick); disabled rules never fire.
export interface WatchFile {
  id: string
  name: string
  mimeType?: string
  sizeBytes?: number
  tags?: string[]
}

export interface WatchRule {
  id: string
  name: string
  enabled: boolean
  match: {
    tag?: string
    mimePrefix?: string
    namePattern?: string
    minSizeBytes?: number
  }
  actions: {
    notify: boolean
    addTag?: string
    /** Assistant id for fire-and-forget classification (§4.6 hook). */
    agentConfigId?: string
  }
  createdAt: string
}

/** True when the file satisfies every set matcher. */
export function matchWatcher(rule: WatchRule, file: WatchFile): boolean {
  if (!rule.enabled) return false
  const m = rule.match
  if (m.tag && !(file.tags ?? []).includes(m.tag)) return false
  if (m.mimePrefix && !(file.mimeType ?? '').startsWith(m.mimePrefix)) return false
  if (m.namePattern) {
    try {
      if (!new RegExp(m.namePattern, 'i').test(file.name)) return false
    } catch {
      return false
    }
  }
  if (typeof m.minSizeBytes === 'number' && (file.sizeBytes ?? 0) < m.minSizeBytes) return false
  return true
}

/** Every (rule, file) pair that fires, rules in list order. */
export function evaluateWatchers(
  rules: WatchRule[],
  files: WatchFile[],
): Array<{ rule: WatchRule; file: WatchFile }> {
  const out: Array<{ rule: WatchRule; file: WatchFile }> = []
  for (const rule of rules) {
    for (const file of files) {
      if (matchWatcher(rule, file)) out.push({ rule, file })
    }
  }
  return out
}

function uid(existing: Set<string>): string {
  let id = ''
  do {
    id = `w-${Date.now().toString(36)}-${Math.floor(Math.random() * 0xffffff).toString(36)}`
  } while (existing.has(id))
  return id
}

export function createWatcher(
  rules: WatchRule[],
  name: string,
  match: WatchRule['match'],
  actions: WatchRule['actions'],
): { rules: WatchRule[]; rule: WatchRule } | null {
  if (!name.trim()) return null
  const rule: WatchRule = {
    id: uid(new Set(rules.map((r) => r.id))),
    name: name.trim().slice(0, 120),
    enabled: true,
    match,
    actions,
    createdAt: new Date().toISOString(),
  }
  return { rules: [...rules, rule], rule }
}

export function parseWatchersStore(value: unknown): WatchRule[] {
  let raw: unknown = value
  if (typeof raw === 'string') {
    try {
      raw = JSON.parse(raw)
    } catch {
      return []
    }
  }
  if (!Array.isArray(raw)) return []
  return raw.filter((e): e is WatchRule => {
    return (
      !!e && typeof e === 'object' &&
      typeof (e as WatchRule).id === 'string' &&
      typeof (e as WatchRule).name === 'string' &&
      !!(e as WatchRule).match && typeof (e as WatchRule).match === 'object' &&
      !!(e as WatchRule).actions && typeof (e as WatchRule).actions === 'object'
    )
  })
}

export function serializeWatchersStore(rules: WatchRule[]): string {
  return JSON.stringify(rules)
}
