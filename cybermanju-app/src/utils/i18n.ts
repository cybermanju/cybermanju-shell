// CyberManju OS — i18n foundation (src/utils/i18n.ts).
// Role: i18n groundwork (ANALYSIS.md §3) — UI strings are hard-coded English
// today while voice input already ships pt-BR. Tiny locale switch + `t()`
// with English fallback; new surfaces (Archive, Watchers, Recovery) dogfood
// it, older panels migrate as they are touched.
// Exports: Locale, LOCALES, getLocale, setLocale, t.
// Integrates: standalone (localStorage `cybermanju.locale.v1`); dictionaries
// in src/locales/{en,pt}.json; SettingsPage language row.
// Contracts: pure lookup, never throws; unknown keys return the key;
// unknown locales fall back to English.
import en from '@/locales/en.json'
import pt from '@/locales/pt.json'

export type Locale = 'en' | 'pt'

export const LOCALES: Array<{ id: Locale; label: string }> = [
  { id: 'en', label: 'English' },
  { id: 'pt', label: 'Português (BR)' },
]

const DICTS: Record<Locale, Record<string, string>> = {
  en: en as Record<string, string>,
  pt: pt as Record<string, string>,
}

const STORE_KEY = 'cybermanju.locale.v1'

let sessionLocale: Locale | null = null

export function getLocale(): Locale {
  if (sessionLocale) return sessionLocale
  try {
    const raw = localStorage.getItem(STORE_KEY)
    if (raw === 'pt' || raw === 'en') return raw
  } catch {
    /* storage unavailable — fall through */
  }
  return 'en'
}

export function setLocale(locale: Locale): void {
  try {
    localStorage.setItem(STORE_KEY, locale)
  } catch {
    /* storage unavailable — lookup still works in-memory this session */
  }
  // Node/vitest has no DOM — guard so unit tests (node env) stay green.
  if (typeof document !== 'undefined') {
    document.documentElement.lang = locale === 'pt' ? 'pt-BR' : 'en'
  }
}

/** Translate `key`; English fallback, then the key itself. Never throws. */
export function t(key: string, locale?: Locale): string {
  const loc = locale ?? currentLocale()
  return DICTS[loc][key] ?? DICTS.en[key] ?? key
}

export function currentLocale(): Locale {
  return sessionLocale ?? getLocale()
}

/** Switch the session locale (persist + <html lang>). */
export function useLocale(locale: Locale): void {
  sessionLocale = locale
  setLocale(locale)
}
