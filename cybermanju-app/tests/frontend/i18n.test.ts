// CyberManju OS — I18n.test (tests/frontend/i18n.test.ts).
// ANALYSIS.md §3: i18n foundation — fallback chain + locale switching.
import { describe, expect, it } from 'vitest'
import { currentLocale, getLocale, t, useLocale } from '../../src/utils/i18n'

describe('i18n foundation', () => {
  it('translates known keys in both locales', () => {
    expect(t('archive.title', 'en')).toBe('ARCHIVE')
    expect(t('archive.title', 'pt')).toBe('ARQUIVO')
  })

  it('falls back en → key, never throws', () => {
    expect(t('archive.title')).toBeTruthy()
    expect(t('no.such.key')).toBe('no.such.key')
  })

  it('switches the session locale and persists', () => {
    useLocale('pt')
    expect(currentLocale()).toBe('pt')
    expect(t('archive.tasks')).toBe('TAREFAS')
    expect(getLocale()).toBe('pt')
    useLocale('en')
    expect(t('archive.tasks')).toBe('TASKS')
  })
})
