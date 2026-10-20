// CyberManju OS — Cybsh script.test (tests/frontend/cybsh-script.test.ts).
// `.cybsh` scripts — interpreter twin + static `run`/`theme`/`ui` verbs.
// Pure logic (node env); browser effects (fetch/localStorage) are stubbed.
import { describe, expect, it } from 'vitest'
import {
  checkCybshScript,
  cybshFingerprint,
  runCybshScript,
} from '../../src/utils/cybshScript'
import {
  runStaticCybshLine,
  type StaticCybshDeps,
} from '../../src/utils/staticCybsh'

const echoExec = (line: string): string => {
  if (line.startsWith('echo ')) return line.slice(5)
  if (line === 'echo') return ''
  return `ran:${line}`
}

async function runOk(source: string): Promise<string> {
  const out = await runCybshScript(source, { execCybsh: echoExec })
  return out.output
}

describe('cybsh script: print / vars / branches', () => {
  it('prints, binds, and branches python-style', async () => {
    const out = await runOk('let x = 2\nif x == 2:\n  print "two"\nelse:\n  print "other"\n')
    expect(out).toBe('two')
  })

  it('handles elif chains and TS annotations', async () => {
    const out = await runOk(
      'let x: number = 1\nif x == 9:\n  print "nine"\nelif x == 1:\n  print "one"\nelse:\n  print "other"\n',
    )
    expect(out).toBe('one')
  })

  it('supports const, reassign, and interpolation', async () => {
    const out = await runOk('const who = "manju"\nprint "hi ${who}"\nlet who = "os"\nprint who\n')
    expect(out).toBe('hi manju\nos')
  })
})

describe('cybsh script: loops', () => {
  it('iterates range() and lists', async () => {
    expect(await runOk('for i in range(3):\n  print i\n')).toBe('0\n1\n2')
    expect(await runOk('for w in ["a", "b"]:\n  print w\n')).toBe('a\nb')
  })

  it('loops while with reassignment', async () => {
    expect(await runOk('let n = 0\nwhile n < 3:\n  let n = n + 1\nprint n\n')).toBe('3')
  })

  it('stops unbounded loops with a budget', async () => {
    await expect(runOk('while true:\n  print 1\n')).rejects.toThrow(/1000|steps/)
  })
})

describe('cybsh script: inline shell + js', () => {
  it('runs $, sh and bare verbs, keeps `_`', async () => {
    const out = await runCybshScript('let name = "world"\n$ echo hello ${name}\nprint "kept=" + _\n', {
      execCybsh: echoExec,
    })
    expect(out.output).toBe('hello world\nkept=hello world')
    expect(out.vars).toBeGreaterThanOrEqual(2)
  })

  it('evaluates sh() with expression arguments', async () => {
    expect(await runOk('let base = "hi"\nlet x = sh("echo " + base)\nprint x\n')).toBe('hi')
    expect(await runOk('print sh("echo yo")\n')).toBe('yo')
  })

  it('evaluates the js subset with JS spellings', async () => {
    expect(await runOk('let x = js: 1 + 2 * 3\nprint x\n')).toBe('7')
    expect(await runOk('js 1 === 1 && "a" !== "b"\n')).toBe('true')
    expect(await runOk('print "ab" * 3\n')).toBe('ababab')
  })
})

describe('cybsh script: memory + errors', () => {
  it('lists, frees, and collects', async () => {
    const out = await runOk('let a = 1\n$ echo hi\nvars\ngc\nfree a\n')
    expect(out).toContain('a: number = 1')
    expect(out).toContain('gc:')
    expect(out).toContain('freed a')
  })

  it('refuses fetch without a client, honestly', async () => {
    await expect(runOk('fetch "https://example.com"\n')).rejects.toThrow(/unsupported:/)
  })

  it('fetches through the provided client', async () => {
    const out = await runCybshScript('fetch "https://example.com/x" as body\nprint len(body)\n', {
      execCybsh: echoExec,
      fetchText: async (req) => `page:${req.url}`,
    })
    expect(out.output).toContain(`fetched https://example.com/x (26 bytes → body)`)
    expect(out.output).toContain('\n26')
  })

  it('harvests ui: effects from inline shell', async () => {
    const out = await runCybshScript('$ ui theme os-dark\n', {
      execCybsh: () => 'theme: os-dark · accent: system\nui: theme=os-dark',
    })
    expect(out.effects).toEqual([{ kind: 'theme', detail: 'os-dark' }])
  })

  it('errors loudly with line numbers', async () => {
    await expect(runOk('frobnicate 1\n')).rejects.toThrow(/syntax: line 1/)
    await expect(runOk('if true\n  print 1\n')).rejects.toThrow(/trailing `:`/)
    await expect(runOk('print nosuchvar\n')).rejects.toThrow(/not_found:/)
  })

  it('dry-runs without executing', () => {
    expect(checkCybshScript('print "hi"\n$ echo yo\n')).toMatch(/dry: 2 statement/)
  })
})

/** Minimal static deps: volume + cwd + shims; everything else throws. */
function staticDeps(volume: Record<string, string>, fallback?: (line: string) => Promise<string>): StaticCybshDeps {
  const vol = { ...volume }
  return {
    readVolume: () => ({ ...vol }),
    getCwd: async () => '/',
    writeVolumeFile: async (path, content) => {
      vol[path] = content
    },
    deleteVolumePath: async () => {
      throw new Error('unsupported: rm in this test')
    },
    killTask: async () => false,
    listSyncConfigs: async () => [],
    getConfigSecret: async () => '',
    getDiskStatus: async () => ({ attached: false, name: '', savedBytes: 0, dirty: false }),
    getStorageEstimate: async () => null,
    listMounts: async () => [],
    providerRead: async () => {
      throw new Error('unsupported: no providers in this test')
    },
    providerWrite: async () => {
      throw new Error('unsupported: no providers in this test')
    },
    providerDelete: async () => {
      throw new Error('unsupported: no providers in this test')
    },
    providerList: async () => [],
    probeProviderQuota: async () => ({ configId: '', backendType: '', ok: false, detail: 'no probe' }),
    keyGet: async () => null,
    keySet: async () => {},
    chacha: async () => null,
    codecs: async () => null,
    blake3: async () => null,
    ...(fallback ? { execFallback: fallback } : {}),
  }
}

describe('static cybsh: run verb', () => {
  it('runs a volume script through static + fallback', async () => {
    const deps = staticDeps(
      { '/t.cybsh': 'print "hi"\n$ echo yo\nlet v = 1\nprint v\n' },
      async (line) => {
        if (line.startsWith('echo ')) return line.slice(5)
        return `fb:${line}`
      },
    )
    const res = await runStaticCybshLine('run /t.cybsh', deps)
    expect(res?.output).toBe('hi\nyo\n1')
  })

  it('nests run-in-run once', async () => {
    const deps = staticDeps(
      { '/a.cybsh': 'print "A"\nsh "run /b.cybsh"\n', '/b.cybsh': 'print "B"\n' },
      async (line) => line,
    )
    const res = await runStaticCybshLine('run /a.cybsh', deps)
    expect(res?.output).toBe('A\nB')
  })

  it('dry-runs, validates extension, and reports missing files', async () => {
    const deps = staticDeps({ '/t.cybsh': 'print 1\n' }, async (line) => line)
    expect((await runStaticCybshLine('run /t.cybsh --dry', deps))?.output).toMatch(/dry:/)
    expect((await runStaticCybshLine('run /t.txt', deps))?.output).toMatch(/invalid:/)
    expect((await runStaticCybshLine('run /missing.cybsh', deps))?.output).toMatch(/not_found:/)
  })
})

describe('static cybsh: theme + ui verbs', () => {
  it('gets, sets, and accents with ui: effect lines', async () => {
    const deps = staticDeps({}, async (line) => line)
    expect((await runStaticCybshLine('theme', deps))?.output).toContain('os-dark')
    const set = await runStaticCybshLine('ui theme os-light', deps)
    expect(set?.output).toContain('ui: theme=os-light')
    expect((await runStaticCybshLine('theme --json', deps))?.output).toContain('os-light')
    const accent = await runStaticCybshLine('ui accent #ff2d55', deps)
    expect(accent?.output).toContain('ui: accent=#ff2d55')
    expect((await runStaticCybshLine('ui accent bogus', deps))?.output).toMatch(/invalid:/)
    expect((await runStaticCybshLine('ui theme nosuch', deps))?.output).toMatch(/invalid:/)
    // Legacy ids migrate by mode.
    const matrix = await runStaticCybshLine('ui theme matrix-night', deps)
    expect(matrix?.output).toContain('ui: theme=os-dark')
    const punk = await runStaticCybshLine('ui theme cyberpunk-night', deps)
    expect(punk?.output).toContain('ui: theme=os-dark')
    const graphite = await runStaticCybshLine('ui theme os-graphite', deps)
    expect(graphite?.output).toContain('ui: theme=os-graphite')
    const plasma = await runStaticCybshLine('ui theme plasma-dark', deps)
    expect(plasma?.output).toContain('ui: theme=plasma-dark')
    const plasmaLight = await runStaticCybshLine('ui theme plasma-light', deps)
    expect(plasmaLight?.output).toContain('ui: theme=plasma-light')
  })
  it('drives the whole interface: density, glass, motion, glow, per-theme accents', async () => {
    const deps = staticDeps({}, async (line) => line)
    const get = await runStaticCybshLine('ui get', deps)
    expect(get?.output).toContain('density: comfortable')
    expect(get?.output).toContain('ui: density=comfortable')
    const getJson = JSON.parse((await runStaticCybshLine('ui get --json', deps))?.output ?? '{}') as Record<string, unknown>
    expect(getJson).toMatchObject({ theme: 'os-dark', accent: null, density: 'comfortable', glass: 2, motion: 'auto', glow: false })

    expect((await runStaticCybshLine('ui density compact', deps))?.output).toContain('ui: density=compact')
    expect((await runStaticCybshLine('ui density bogus', deps))?.output).toMatch(/invalid:/)
    expect((await runStaticCybshLine('ui glass translucent', deps))?.output).toContain('ui: glass=2')
    expect((await runStaticCybshLine('ui glass solid', deps))?.output).toContain('ui: glass=0')
    expect((await runStaticCybshLine('ui glass 9', deps))?.output).toMatch(/invalid:/)
    expect((await runStaticCybshLine('ui motion reduced', deps))?.output).toContain('ui: motion=reduced')
    expect((await runStaticCybshLine('ui glow off', deps))?.output).toContain('ui: glow=off')

    const themed = await runStaticCybshLine('ui accent #ff2d78 --for os-dark', deps)
    expect(themed?.output).toContain('ui: accent-for=os-dark:#ff2d78')
    expect((await runStaticCybshLine('ui accent #ff2d78 --for nosuch', deps))?.output).toMatch(/invalid:/)
    const after = JSON.parse((await runStaticCybshLine('ui get --json', deps))?.output ?? '{}') as Record<string, unknown>
    expect(after).toMatchObject({
      density: 'compact',
      glass: 0,
      motion: 'reduced',
      glow: false,
      accents: { 'os-dark': '#ff2d78' },
    })
    const cleared = await runStaticCybshLine('ui accent default --for os-dark', deps)
    expect(cleared?.output).toContain('ui: accent-for=os-dark:system')
  })
})

describe('cybsh script: fingerprints pin replay journals', () => {
  it('matches the FNV-1a/64 vector both transports share', () => {
    expect(cybshFingerprint('a')).toBe('af63dc4c8601ec8c')
    expect(cybshFingerprint('a')).toHaveLength(16)
    expect(cybshFingerprint('a')).not.toBe(cybshFingerprint('b'))
  })
})

describe('cybsh script: dicts are structured values', () => {
  it('builds, sorts, and indexes dicts', async () => {
    expect(await runOk('let d = {"b": 2, "a": 1}\nprint d\nprint d.a\nprint d["b"]\n')).toBe(
      '{"a": 1, "b": 2}\n1\n2',
    )
  })

  it('updates functionally with set/push/del/keys/values', async () => {
    expect(await runOk('let d = {a: 1}\nlet e = set(d, "b", 2)\nprint keys(e)\nprint len(e)\nprint len(d)\n')).toBe(
      '[a, b]\n2\n1',
    )
    expect(await runOk('let l = push([1], 2)\nprint l[1]\nprint del({a: 1, b: 2}, "a")\nprint "xy"[1]\n')).toBe(
      '2\n{"b": 2}\ny',
    )
  })

  it('iterates dict keys in sorted order', async () => {
    expect(await runOk('for k in {b: 2, a: 1}:\n  print k\n')).toBe('a\nb')
  })

  it('materializes json() and --json shell output', async () => {
    expect(await runOk('let v = json("{\\"a\\": [1, 2]}")\nprint v.a[1]\nprint v.a\n')).toBe('2\n[1, 2]')
    const out = await runCybshScript('let v = sh("x --json")\nprint v.a\n', {
      execCybsh: (line) => (line.includes('--json') ? '{"b": 2, "a": 1}' : `ran:${line}`),
    })
    expect(out.output).toBe('1')
  })

  it('rejects cross-type indexing loudly', async () => {
    await expect(runOk('print {a: 1}.b\n')).rejects.toThrow(/not_found:/)
    await expect(runOk('print [1][5]\n')).rejects.toThrow(/not_found:/)
    await expect(runOk('print (1).x\n')).rejects.toThrow(/syntax:/)
  })

  it('keeps dict commas out of print splitting and assignments', async () => {
    expect(await runOk('print {a: 1, b: 2}, "x"\n')).toBe('{"a": 1, "b": 2} x')
    expect(await runOk('let d = {a: 1, b: 2}\nprint len(d)\n')).toBe('2')
  })
})

describe('cybsh script: explicit failure handling', () => {
  it('catches failing lines with bindings', async () => {
    const out = await runCybshScript(
      'try:\n  $ unknown boom\ncatch e:\n  print "caught"\nprint "after"\n',
      { execCybsh: () => { throw new Error("unknown command: 'unknown'") } },
    )
    expect(out.output).toBe('caught\nafter')
  })

  it('skips catch when nothing fails, and raises with fail', async () => {
    expect(await runOk('try:\n  print "ok"\ncatch:\n  print "never"\n')).toBe('ok')
    await expect(runOk('fail "stop"\n')).rejects.toThrow(/^fail: stop/)
    await expect(runOk('try:\n  fail "nope"\ncatch:\n  fail "worse"\n')).rejects.toThrow(/worse/)
  })

  it('rejects stray catch', async () => {
    await expect(runOk('catch e:\n  print 1\n')).rejects.toThrow(/without `try`/)
  })

  it('runs parenthesized match arms (ok(v)/err(e))', async () => {
    // Regression: the arm gate once recognized only bare `ok:`/`err:`,
    // so `ok(v):` bodies were skipped silently (native + wasm + static).
    expect(await runOk('match ok(5):\n  ok(v):\n    print v + 1\n  err(e):\n    print "bad"\n')).toBe('6')
    expect(await runOk('match err("nope"):\n  ok(v):\n    print "bad"\n  else:\n    print "fell"\n')).toBe('fell')
    const out = await runCybshScript(
      'match sh("echo hi"):\n  ok(v):\n    print "got " + v\n  err(e):\n    print "missed"\n',
      { execCybsh: echoExec },
    )
    expect(out.output).toBe('got hi')
  })

  it('round-trips sh --json with strings through hoisting', async () => {
    // Regression: substituted shell output used display form
    // (`{theme: os-dark}`), which re-parsed bare words as variables.
    const out = await runCybshScript('let ui = sh("ui get --json")\nprint ui.theme\nprint len(ui)\n', {
      execCybsh: () => '{"theme": "os-dark", "accent": "system"}',
    })
    expect(out.output).toBe('os-dark\n2')
  })
})

describe('cybsh script: user functions', () => {
  it('defines, calls, and scopes defs', async () => {
    expect(await runOk('def greet(name):\n  return "hi " + name\nprint greet("manju")\n')).toBe('hi manju')
    expect(await runOk('let x = 1\ndef f():\n  let x = 2\n  return x\nprint f()\nprint x\n')).toBe('2\n1')
    expect(await runOk('def f():\n  print "side"\n  return 7\nprint f() + 1\n')).toBe('side\n8')
  })

  it('sees globals but never mutates them', async () => {
    expect(await runOk('let g = 10\ndef f():\n  return g + 1\nprint f()\nprint g\n')).toBe('11\n10')
  })

  it('rejects bad definitions and arity, bounds recursion', async () => {
    await expect(runOk('return 1\n')).rejects.toThrow(/outside `def`/)
    await expect(runOk('def f(a, a):\n  print a\n')).rejects.toThrow(/duplicate/)
    await expect(runOk('def len(x):\n  return x\n')).rejects.toThrow(/builtin/)
    await expect(runOk('def f(a):\n  return a\nprint f(1, 2)\n')).rejects.toThrow(/takes 1 argument/)
    await expect(runOk('def f():\n  return f()\nprint f()\n')).rejects.toThrow(/call depth/)
  })
})

describe('cybsh script: one way to run code', () => {
  it('evaluates sh() statements and prints values', async () => {
    expect(await runOk('sh("echo yo")\n')).toBe('yo')
    expect(await runOk('1 + 2\n')).toBe('3')
  })
})

describe('cybsh script: versions and capabilities', () => {
  it('accepts the v1 pin and refuses the future', async () => {
    expect(await runOk('# cybsh: 1\nprint 1\n')).toBe('1')
    await expect(runOk('# cybsh: 99\nprint 1\n')).rejects.toThrow(/unsupported:/)
  })

  it('denies verbs and scopes fetch by capability', async () => {
    await expect(runOk('# cap: deny=rm\n$ rm /x\n')).rejects.toThrow(/^denied:/)
    // Deny matches the verb, so host (`-os`) variants are covered too.
    await expect(runOk('# cap: deny=rm\n$ rm -os /x\n')).rejects.toThrow(/^denied:/)
    await expect(runOk('# cap: net=example.com\nfetch "https://evil.test/x"\n')).rejects.toThrow(/^denied:/)
    const out = await runCybshScript('# cap: net=example.com deny=rm\nprint "ok"\n', {
      execCybsh: echoExec,
      fetchText: async (req) => `page:${req.url}`,
    })
    expect(out.output).toBe('ok')
    expect(out.caps).toMatchObject({ active: true, deny: ['rm'] })
    const fetched = await runCybshScript('# cap: net=example.com\nfetch "https://example.com/x" as b\nprint b\n', {
      execCybsh: echoExec,
      fetchText: async (req) => `page:${req.url}`,
    })
    expect(fetched.output).toContain('page:https://example.com/x')
  })

  it('audits calls in the result', async () => {
    const out = await runCybshScript('$ echo hi\nfetch "https://example.com" as b\n', {
      execCybsh: echoExec,
      fetchText: async () => 'body',
    })
    expect(out.shCalls).toBe(1)
    expect(out.fetchCalls).toBe(1)
    expect(out.caps).toBeNull()
  })
})

describe('static cybsh: replay journals', () => {
  it('records sh calls and replays them without executing', async () => {
    const calls: string[] = []
    const deps = staticDeps({ '/j.cybsh': 'print sh("ls /")\n' }, async (line) => {
      calls.push(line)
      return `fb:${line}`
    })
    const rec = await runStaticCybshLine('run /j.cybsh --record /j.json', deps)
    expect(rec?.output).toBe('fb:ls /')
    const journal = JSON.parse(deps.readVolume()['/j.json'] ?? '{}') as {
      cybsh: number
      fingerprint: string
      calls: { sh: Record<string, { ok: boolean; output: string }> }
    }
    expect(journal.cybsh).toBe(1)
    expect(journal.fingerprint).toBe(cybshFingerprint('print sh("ls /")\n'))
    expect(journal.calls.sh['ls /']).toMatchObject({ ok: true, output: 'fb:ls /' })
    // Replay with a throwing transport: nothing executes, journal serves.
    const replayDeps = staticDeps(
      {
        '/j.cybsh': 'print sh("ls /")\n',
        '/j.json': JSON.stringify(journal),
      },
      async () => {
        throw new Error('must not execute during replay')
      },
    )
    expect((await runStaticCybshLine('run /j.cybsh --replay /j.json', replayDeps))?.output).toBe('fb:ls /')
    expect(calls).toHaveLength(1)
  })

  it('refuses stale replays with integrity, misses with not_found', async () => {
    const src = 'print sh("echo hi")\n'
    const journal = JSON.stringify({
      cybsh: 1,
      fingerprint: cybshFingerprint(src),
      script: '/j.cybsh',
      calls: { sh: { 'echo hi': { ok: true, output: 'hi' } }, fetch: {} },
    })
    const stale = staticDeps(
      { '/j.cybsh': 'print sh("echo changed")\n', '/j.json': journal },
      async (line) => line,
    )
    expect((await runStaticCybshLine('run /j.cybsh --replay /j.json', stale))?.output).toMatch(/integrity:/)
    const miss = staticDeps({ '/j.cybsh': src, '/j.json': journal }, async (line) => line)
    expect((await runStaticCybshLine('run /missing.cybsh --replay /j.json', miss))?.output).toMatch(/not_found:/)
  })

  it('serves fetch from replay without a network client', async () => {
    const src = 'fetch "https://x.test/a" as body\nprint body\n'
    const journal = JSON.stringify({
      cybsh: 1,
      fingerprint: cybshFingerprint(src),
      script: '/f.cybsh',
      calls: { sh: {}, fetch: { 'https://x.test/a': { ok: true, output: 'hello-remote' } } },
    })
    const deps = staticDeps({ '/f.cybsh': src, '/f.json': journal }, async (line) => line)
    const res = await runStaticCybshLine('run /f.cybsh --replay /f.json', deps)
    expect(res?.output).toContain('hello-remote')
  })
})
