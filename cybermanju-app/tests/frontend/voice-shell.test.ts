// CyberManju OS — Voice shell.test (tests/frontend/voice-shell.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest, ../….
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it } from 'vitest'
import { bestMatch, jaroWinkler, levenshtein } from '../../src/utils/fuzzyCorrect'
import {
  correctCodeWords,
  correctShellLine,
  CYBSH_COMMANDS,
  detectVoiceLang,
  foldAccents,
  isPtLang,
  normalizeSpoken,
  resolveVoiceLang,
} from '../../src/utils/speechCorrect'

describe('levenshtein', () => {
  it('scores identical strings as zero', () => {
    expect(levenshtein('ls', 'ls')).toBe(0)
  })
  it('counts a single substitution', () => {
    expect(levenshtein('lss', 'ls')).toBe(1)
  })
})

describe('jaroWinkler', () => {
  it('prefers the closer neighbour', () => {
    expect(jaroWinkler('lss', 'ls')).toBeGreaterThan(jaroWinkler('lss', 'ps'))
  })
  it('returns 1 for identical strings', () => {
    expect(jaroWinkler('disk', 'disk')).toBe(1)
  })
})

describe('bestMatch', () => {
  it('fixes a doubled letter', () => {
    expect(bestMatch('lss', CYBSH_COMMANDS)?.match).toBe('ls')
  })
  it('returns null when nothing is close', () => {
    expect(bestMatch('zzzq', CYBSH_COMMANDS)).toBeNull()
  })
  it('knows the static-layer vault verbs', () => {
    for (const verb of ['oauth', 'compress', 'decompress', 'quota', 'providers']) {
      expect(CYBSH_COMMANDS).toContain(verb)
      expect(bestMatch(verb, CYBSH_COMMANDS)?.match).toBe(verb)
    }
    expect(bestMatch('outh', CYBSH_COMMANDS)?.match).toBe('oauth')
  })
})

describe('correctShellLine', () => {
  it('repairs a typoed verb', () => {
    const r = correctShellLine('lss /tmp')
    expect(r.line).toBe('ls /tmp')
    expect(r.fixed).toBe(true)
  })
  it('expands aliases', () => {
    expect(correctShellLine('dir').line).toBe('ls')
  })
  it('repairs known subcommands', () => {
    const r = correctShellLine('disk lis')
    expect(r.line).toBe('disk list')
    expect(r.fixes).toContain('lis→list')
  })
  it('joins spoken two-letter verbs', () => {
    expect(correctShellLine('see dee /tmp').line).toBe('cd /tmp')
  })
  it('leaves unknown lines for the server did-you-mean', () => {
    const r = correctShellLine('frobnicate --json')
    expect(r.fixed).toBe(false)
    expect(r.line).toBe('frobnicate --json')
  })
})

describe('normalizeSpoken', () => {
  it('turns prose punctuation words into marks and capitalises', () => {
    expect(normalizeSpoken('hello world period how are you question mark', 'prose')).toBe(
      'Hello world. How are you?',
    )
  })
  it('turns code words into symbols', () => {
    expect(normalizeSpoken('function foo open paren bar close paren open brace', 'code')).toBe(
      'function foo(bar){',
    )
  })
  it('handles camel case phrases', () => {
    expect(normalizeSpoken('camel case foo bar', 'code')).toBe('fooBar')
  })
  it('glues shell symbols', () => {
    expect(normalizeSpoken('disk list', 'shell')).toBe('disk list')
  })
})

describe('correctCodeWords', () => {
  it('repairs STT homophones', () => {
    expect(correctCodeWords('funkshun foo cost')).toBe('function foo const')
  })
})

describe('voice language (en-US + pt-BR)', () => {
  it('detects Portuguese browsers as pt-BR', () => {
    expect(detectVoiceLang('pt-BR')).toBe('pt-BR')
    expect(detectVoiceLang('pt')).toBe('pt-BR')
    expect(detectVoiceLang('pt-PT')).toBe('pt-BR')
    expect(detectVoiceLang('en-US')).toBe('en-US')
    expect(detectVoiceLang('en')).toBe('en-US')
    expect(isPtLang('pt-BR')).toBe(true)
    expect(isPtLang('en-US')).toBe(false)
  })
  it('resolves explicit picks verbatim and auto via the browser locale', () => {
    expect(resolveVoiceLang('en-US')).toBe('en-US')
    expect(resolveVoiceLang('pt-BR')).toBe('pt-BR')
    expect(resolveVoiceLang('auto', 'pt-BR')).toBe('pt-BR')
    expect(resolveVoiceLang('auto', 'en-US')).toBe('en-US')
  })
  it('folds accents for matching', () => {
    expect(foldAccents('CÊ DÊ')).toBe('ce de')
    expect(foldAccents('vírgula')).toBe('virgula')
  })
})

describe('normalizeSpoken pt-BR prose', () => {
  it('turns Portuguese punctuation words into marks and capitalises', () => {
    expect(normalizeSpoken('olá mundo vírgula como vai ponto de interrogação', 'prose', 'pt-BR')).toBe(
      'Olá mundo, como vai?',
    )
  })
  it('handles ponto final / dois pontos / ponto e vírgula', () => {
    expect(normalizeSpoken('fim ponto final próxima dois pontos vai ponto e vírgula ok', 'prose', 'pt-BR')).toBe(
      'Fim. Próxima: vai; ok',
    )
  })
  it('keeps dictated line breaks', () => {
    expect(normalizeSpoken('primeira linha nova linha segunda linha', 'prose', 'pt-BR')).toBe(
      'Primeira linha\nSegunda linha',
    )
  })
  it('leaves Portuguese words alone in en-US mode', () => {
    expect(normalizeSpoken('olá mundo vírgula', 'prose', 'en-US')).toBe('Olá mundo vírgula')
  })
})

describe('normalizeSpoken pt-BR code', () => {
  it('turns Portuguese symbol words into symbols', () => {
    expect(
      normalizeSpoken('função foo abre parênteses bar fecha parênteses abre chave', 'code', 'pt-BR'),
    ).toBe('function foo(bar){')
  })
  it('still understands English triggers in pt-BR mode', () => {
    expect(normalizeSpoken('function foo open paren bar close paren', 'code', 'pt-BR')).toBe(
      'function foo(bar)',
    )
  })
  it('maps Portuguese keywords to code', () => {
    expect(correctCodeWords('função constante retorna classe', 'pt-BR')).toBe('function const return class')
    expect(correctCodeWords('função', 'en-US')).toBe('função')
  })
})

describe('correctShellLine pt-BR', () => {
  it('joins Portuguese phonetic splits', () => {
    expect(correctShellLine('cê dê /tmp').line).toBe('cd /tmp')
    expect(correctShellLine('CÊ DÊ /tmp').line).toBe('cd /tmp')
    expect(correctShellLine('ele esse /tmp').line).toBe('ls /tmp')
    expect(correctShellLine('erre eme arquivo').line).toBe('rm arquivo')
  })
  it('expands Portuguese aliases', () => {
    expect(correctShellLine('ajuda').line).toBe('help')
    expect(correctShellLine('listar').line).toBe('ls')
    expect(correctShellLine('histórico').line).toBe('history')
  })
})
