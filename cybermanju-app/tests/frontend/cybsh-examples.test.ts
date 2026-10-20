// CyberManju OS — Cybsh examples.test (tests/frontend/cybsh-examples.test.ts).
// Skill examples must run: execute every `.cybsh` file shipped in
// `.cybermanju/skills/cybsh-script/examples/` through the TS interpreter
// with a stub shell (echo unwrapped, `--json` legs answered as JSON,
// `cat …lib.cybsh` served from disk, fetch stubbed). Pure logic, node env.
import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { runCybshScript } from '../../src/utils/cybshScript'

const example = (name: string): string =>
  readFileSync(new URL(`../../.cybermanju/skills/cybsh-script/examples/${name}`, import.meta.url), 'utf8')

const libSource = example('lib.cybsh')

function stubExec(line: string): string {
  if (line.startsWith('echo ')) return line.slice(5)
  if (line === 'echo') return ''
  if (line === 'ui get --json')
    return '{"theme": "os-dark", "accent": "system", "density": "comfortable"}'
  if (line === 'sync status --json') return '{"jobs": []}'
  if (line.startsWith('cat "') && line.includes('lib.cybsh')) return libSource
  return `ran:${line}`
}

const deps = {
  execCybsh: stubExec,
  fetchText: async () => 'page body text',
  env: { CYBSH_TOUR: 'set-in-test' },
}

describe('skill examples run end to end', () => {
  it('hello proves run works', async () => {
    const out = await runCybshScript(example('hello.cybsh'), deps)
    expect(out.output).toContain('hello from cybsh scripts')
    expect(out.output).toContain('count 2')
    expect(out.output).toContain('js says 7')
  })

  it('lib defines shared defs and nothing else', async () => {
    const out = await runCybshScript(libSource, deps)
    expect(out.output).toBe('')
  })

  it('functions covers dicts, defs, match, pipes, with', async () => {
    const out = await runCybshScript(example('functions.cybsh'), deps)
    expect(out.output).toContain('echoed: structured')
    expect(out.output).toContain('scoped keys: 3')
    expect(out.output).toContain('outer kept: 3')
    expect(out.output).toContain('functions done')
  })

  it('providers-tidy triages, polls, and signals', async () => {
    const out = await runCybshScript(example('providers-tidy.cybsh'), deps)
    expect(out.output).toContain('sync idle — vault is quiet')
    expect(out.output).toContain('tidy done')
  })

  it('tour covers almost the whole language', async () => {
    const out = await runCybshScript(example('tour.cybsh'), deps, ['mon'])
    for (const marker of [
      'two args 3',
      'n=3',
      'theme now: os-dark',
      'keys: 3',
      'denied as declared: denied:',
      'matched ok: tour',
      'scoped: 3',
      'outer kept: 3',
      'set-in-test',
      'fetched bytes:',
      'immediate truth — no waiting',
      'sync idle',
      'tour done',
    ]) {
      expect(out.output).toContain(marker)
    }
    // argv binding: `arg(0)` sees the passed argv.
    expect(out.output).toContain('mon')
  })
})
