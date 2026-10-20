// CyberManju OS — Shell cwd.test (tests/frontend/shell-cwd.test.ts).
// Client-side cybsh cwd mirror (used by the FileManager + CodeStudio panels).
import { describe, expect, it } from 'vitest'
import { trackShellCwd } from '../../src/utils/shellCwd'

describe('trackShellCwd', () => {
  it('tracks absolute, relative, parent and quoted destinations', () => {
    expect(trackShellCwd('/', 'cd /vault/docs')).toBe('/vault/docs')
    expect(trackShellCwd('/vault', 'cd docs')).toBe('/vault/docs')
    expect(trackShellCwd('/vault/docs', 'cd ..')).toBe('/vault')
    expect(trackShellCwd('/vault', 'cd "my folder"')).toBe('/vault/my folder')
    expect(trackShellCwd('/vault', "cd 'my folder'")).toBe('/vault/my folder')
  })

  it('follows chains and takes the last cd', () => {
    expect(trackShellCwd('/', 'cd /a && ls -la')).toBe('/a')
    expect(trackShellCwd('/', 'cd /a && cd b && pwd')).toBe('/a/b')
    expect(trackShellCwd('/', 'ls; cd /x; stat f')).toBe('/x')
  })

  it('leaves the mirror alone for anything that is not a cd', () => {
    expect(trackShellCwd('/vault', 'ls -la')).toBe('/vault')
    expect(trackShellCwd('/vault', 'stat cdrom')).toBe('/vault')
    expect(trackShellCwd('/vault', '')).toBe('/vault')
    expect(trackShellCwd('/vault', 'cd')).toBe('/vault')
    expect(trackShellCwd('/vault', 'cd -')).toBe('/vault')
  })

  it('never escapes the volume root', () => {
    expect(trackShellCwd('/', 'cd ..')).toBe('/')
    expect(trackShellCwd('/a', 'cd ../../..')).toBe('/')
    expect(trackShellCwd('/a/b', 'cd .././c')).toBe('/a/c')
  })
})
