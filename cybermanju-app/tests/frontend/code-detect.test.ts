// CyberManju OS — Code detect.test (tests/frontend/code-detect.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest.
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it } from 'vitest'
import {
  checkBrackets,
  detectLanguage,
  highlightSyntax,
  langOf,
} from '../../src/utils/codeDetect'

describe('detectLanguage', () => {
  it('maps extensions', () => {
    expect(detectLanguage('main.rs', '').language).toBe('rust')
    expect(detectLanguage('app.tsx', '').language).toBe('typescript')
  })
  it('resolves basenames', () => {
    expect(detectLanguage('Dockerfile', '').language).toBe('docker')
    expect(detectLanguage('Makefile', '').language).toBe('make')
  })
  it('reads shebangs for extensionless scripts', () => {
    expect(detectLanguage('run', '#!/usr/bin/env python3\nprint(1)').via).toBe('shebang')
    expect(detectLanguage('run', '#!/usr/bin/env python3\nprint(1)').language).toBe('python')
  })
  it('reads vim modelines', () => {
    expect(detectLanguage('x', '/* vim: ft=rust */\nfn main(){}').language).toBe('rust')
  })
  it('sniffs pasted content without a file name', () => {
    expect(detectLanguage('', 'def foo(bar):\n    pass').language).toBe('python')
  })
  it('disambiguates .h with C++ content', () => {
    const r = detectLanguage('x.h', 'class Foo { };')
    expect(r.language).toBe('cpp')
    expect(r.via).toBe('content')
  })
  it('falls back to text', () => {
    expect(detectLanguage('', '').language).toBe('text')
  })
})

describe('langOf', () => {
  it('keeps the legacy filename-only contract', () => {
    expect(langOf('a.py')).toBe('python')
  })
})

describe('highlightSyntax', () => {
  it('marks Python comments and keywords', () => {
    const html = highlightSyntax('# hello\ndef foo():\n    pass', 'python')
    expect(html).toContain('sx-c')
    expect(html).toContain('sx-k')
  })
  it('marks SQL keywords', () => {
    const html = highlightSyntax('SELECT a FROM t', 'sql')
    expect(html).toContain('sx-k')
  })
})

describe('checkBrackets', () => {
  it('is quiet on balanced code', () => {
    expect(checkBrackets('fn f() { let x = [1, 2]; }', 'rust')).toEqual([])
  })
  it('reports unclosed brackets at the opening line', () => {
    const issues = checkBrackets('fn f() {\n  let x = 1;\n', 'rust')
    expect(issues.some((i) => i.message.includes('Unclosed'))).toBe(true)
  })
  it('ignores brackets inside strings', () => {
    expect(checkBrackets('const s = "(oops";', 'javascript')).toEqual([])
  })
})
