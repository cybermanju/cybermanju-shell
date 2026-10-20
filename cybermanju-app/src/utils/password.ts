// CyberManju OS — password generator (Phase 3, secrets keystore).
//
// Uses `crypto.getRandomValues` (never Math.random). Entropy is computed
// exactly: length × log2(charset size) — the strength meter buckets it the
// usual way (< 40 weak / < 60 fair / < 80 good / else strong).

export interface PasswordOptions {
  length: number
  upper: boolean
  lower: boolean
  digits: boolean
  symbols: boolean
  /** Exclude visually ambiguous chars (0/O, 1/l/I). */
  ambiguous: boolean
}

export const PASSWORD_DEFAULTS: PasswordOptions = {
  length: 20,
  upper: true,
  lower: true,
  digits: true,
  symbols: true,
  ambiguous: false,
}

const UPPER = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'
const LOWER = 'abcdefghijklmnopqrstuvwxyz'
const DIGITS = '0123456789'
const SYMBOLS = '!@#$%^&*()-_=+[]{};:,.<>?/'
const AMBIGUOUS = new Set('0O1lI'.split(''))

/** Active charset for the given options (empty when nothing is on). */
export function passwordCharset(opts: PasswordOptions): string {
  let pool = ''
  const add = (chars: string) => {
    pool += opts.ambiguous
      ? [...chars].filter(c => !AMBIGUOUS.has(c)).join('')
      : chars
  }
  if (opts.upper) add(UPPER)
  if (opts.lower) add(LOWER)
  if (opts.digits) add(DIGITS)
  if (opts.symbols) add(SYMBOLS)
  return pool
}

/** Shannon-style entropy in bits: length × log2(|charset|). */
export function passwordEntropyBits(opts: PasswordOptions): number {
  const pool = passwordCharset(opts)
  if (!pool || opts.length <= 0) return 0
  return opts.length * Math.log2(pool.length)
}

export type PasswordStrength = 'weak' | 'fair' | 'good' | 'strong'

export function passwordStrength(bits: number): PasswordStrength {
  if (bits < 40) return 'weak'
  if (bits < 60) return 'fair'
  if (bits < 80) return 'good'
  return 'strong'
}

/** Generate a random password. Throws when no charset is enabled or the
 *  length is outside 4..=128 — callers surface the message as `invalid:`. */
export function generatePassword(opts: PasswordOptions = PASSWORD_DEFAULTS): string {
  const pool = passwordCharset(opts)
  if (!pool) throw new Error('invalid: enable at least one character set')
  const length = Math.floor(opts.length)
  if (!Number.isFinite(length) || length < 4 || length > 128) {
    throw new Error('invalid: length must be between 4 and 128')
  }
  const chars: string[] = []
  // Rejection sampling over the full 32-bit range keeps the distribution
  // uniform (no modulo bias) without pulling in a bigint dependency.
  const limit = Math.floor(0x1_0000_0000 / pool.length) * pool.length
  while (chars.length < length) {
    const draw = crypto.getRandomValues(new Uint32Array(8))
    for (const n of draw) {
      if (n >= limit) continue
      chars.push(pool[n % pool.length])
      if (chars.length === length) break
    }
  }
  return chars.join('')
}

/** Strength label for the meter next to a generated/current password. */
export function strengthOf(password: string): { bits: number; strength: PasswordStrength } {
  // Entropy of a *generated* password — estimate from unique charset size
  // when the exact options are unknown (user-pasted values).
  if (!password) return { bits: 0, strength: 'weak' }
  const unique = new Set(password).size
  const bits = password.length * Math.log2(Math.max(unique, 2))
  return { bits, strength: passwordStrength(bits) }
}
