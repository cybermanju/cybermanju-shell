// CyberManju OS — host filesystem safety contracts.
// Role: shared byte-budget and path guards for native, REST and WASM host bridges.
// Exports: HOST_WRITE_LIMIT_BYTES, normalizeHostPath, isHostFilesystemRoot,
//   isSameOrDescendantHostPath and joinHostPath.
// Integrates: hostBrowse, hostWasm and their cross-transport regression tests.

/** The shell text writer contract is measured in UTF-8 bytes on every transport. */
export const HOST_WRITE_LIMIT_BYTES = 1024 * 1024

/**
 * Lexically normalize a host path for safety comparisons.
 * This is not a filesystem realpath and intentionally does not resolve symlinks.
 */
export function normalizeHostPath(path: string): string {
  const raw = String(path ?? '').replace(/\\/g, '/')
  if (!raw) return ''

  let prefix = ''
  let rest = raw
  const drive = /^([A-Za-z]:)(\/)?/.exec(raw)
  if (drive) {
    prefix = `${drive[1]}${drive[2] ? '/' : ''}`
    rest = raw.slice(drive[0].length)
  } else if (raw.startsWith('//')) {
    const parts = raw.slice(2).split('/').filter(Boolean)
    if (parts.length >= 2) {
      prefix = `//${parts[0]}/${parts[1]}`
      rest = parts.slice(2).join('/')
    } else {
      prefix = '/'
      rest = raw.replace(/^\/+/, '')
    }
  } else if (raw.startsWith('/')) {
    prefix = '/'
    rest = raw.replace(/^\/+/, '')
  }

  const absolute = prefix === '/' || prefix.endsWith('/') || prefix.startsWith('//')
  const segments: string[] = []
  for (const segment of rest.split('/')) {
    if (!segment || segment === '.') continue
    if (segment === '..') {
      if (segments.length && segments[segments.length - 1] !== '..') segments.pop()
      else if (!absolute) segments.push('..')
      continue
    }
    segments.push(segment)
  }

  if (prefix === '/') return segments.length ? `/${segments.join('/')}` : '/'
  if (prefix.endsWith('/')) return segments.length ? `${prefix}${segments.join('/')}` : prefix
  if (prefix.startsWith('//')) return segments.length ? `${prefix}/${segments.join('/')}` : prefix
  return prefix + segments.join('/')
}

/** True for an empty/default path, a POSIX root, a Windows drive root or a UNC share root. */
export function isHostFilesystemRoot(path: string): boolean {
  const normalized = normalizeHostPath(path)
  return (
    normalized === '' ||
    normalized === '/' ||
    /^[A-Za-z]:\/?$/.test(normalized) ||
    /^\/\/[^/]+\/[^/]+\/?$/.test(normalized)
  )
}

/** True when candidate resolves to parent or lies beneath it. */
export function isSameOrDescendantHostPath(
  parentPath: string,
  candidatePath: string,
  caseInsensitive = false,
): boolean {
  let parent = normalizeHostPath(parentPath)
  let candidate = normalizeHostPath(candidatePath)
  const windowsPath = /^[A-Za-z]:/.test(parent) || parent.startsWith('//')
  if (caseInsensitive || windowsPath) {
    parent = parent.toLocaleLowerCase()
    candidate = candidate.toLocaleLowerCase()
  }
  if (parent === candidate) return true
  if (!parent || !candidate) return false
  const prefix = parent.endsWith('/') ? parent : `${parent}/`
  return candidate.startsWith(prefix)
}

/** Join a single filename to a host directory without turning `/` into `//name`. */
export function joinHostPath(directory: string, name: string): string {
  const part = String(name ?? '').replace(/\\/g, '/')
  if (!part || part === '.' || part === '..' || part.includes('/')) {
    throw new Error('invalid: host destination name must be one path segment')
  }
  const parent = normalizeHostPath(directory)
  if (!parent) return part
  if (parent === '/') return `/${part}`
  return `${parent.replace(/\/+$/, '')}/${part}`
}
