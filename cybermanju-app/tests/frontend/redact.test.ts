// CyberManju OS — Redact.test (tests/frontend/redact.test.ts).
// Browser mirror of `crates/agent/src/redact.rs`: tool outputs and memories
// must not carry secrets into the transcript (localStorage) or the provider.
import { describe, expect, it } from 'vitest'
import { cleanToolOutput, redactText } from '../../src/utils/redact'

describe('redactText (mirror of Rust redact)', () => {
  it('passes clean text through untouched', () => {
    const { text, count } = redactText('fn main() {\n    println!("hello");\n}')
    expect(count).toBe(0)
    expect(text).toContain('hello')
  })

  it('redacts known provider key shapes', () => {
    const { text, count } = redactText('key is sk-ant-secretvalue123 and done')
    expect(count).toBe(1)
    expect(text).toContain('sk-ant-***')
    expect(text).not.toContain('secretvalue123')
  })

  it('redacts query-auth keys, env passphrases, jwt secrets and oauth tokens', () => {
    expect(redactText('url?key=AIzaSyD-abc123XYZ_ ok').text).not.toContain('AIzaSyD')
    const env = redactText('CYBERMANJU_MASTER_PASSPHRASE=hunter2-hunter2\nother=1')
    expect(env.count).toBe(1)
    expect(env.text).not.toContain('hunter2')
    expect(env.text).toContain('other=1')
    expect(redactText('jwt_secret=hunter2').text).not.toContain('hunter2')
    expect(redactText('token ya29.a0AfH6SMBx1234567890 ok').text).not.toContain('AfH6SMB')
  })

  it('redacts JWT shapes without markers', () => {
    const token = 'eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c'
    const { text, count } = redactText(`got ${token} ok`)
    expect(count).toBe(1)
    expect(text).not.toContain('SflKxw')
  })

  it('leaves bare markers without secrets alone', () => {
    const { text, count } = redactText('set token= then run')
    expect(count).toBe(0)
    expect(text).toContain('token=')
  })
})

describe('cleanToolOutput (mirror of native clean_output)', () => {
  it('returns output unchanged when clean', () => {
    expect(cleanToolOutput('wrote /a (10 bytes)')).toBe('wrote /a (10 bytes)')
  })

  it('appends a redaction note when something was hidden', () => {
    const out = cleanToolOutput('token=ghp_abc123def456 done')
    expect(out).not.toContain('abc123')
    expect(out).toContain('(redacted 1 secret(s) from tool output)')
  })
})
