// CyberManju OS — Shamir (src/utils/shamir.ts).
// Role: recovery / key escrow primitive (ANALYSIS.md §3) — Shamir m-of-n
// secret sharing over GF(256) so the master passphrase can split into N
// recovery shares, any M of which restore it. Pure TypeScript; the Rust
// keystore stays the at-rest holder, this is the escrow UX the ANALYSIS
// asks for ("extend to Shamir m-of-n recovery of the master key").
// Exports: splitSecret, combineShares, splitText, combineText, ShamirShare.
// Integrates: standalone; ShieldPanel RECOVERY tab; never persisted, shares
// live only in the UI the user prints/stores offline.
// Contracts: pure except randomness (crypto.getRandomValues); m < 2 or
// m > n or n > 255 throws; duplicate share indexes throw; fewer than m
// shares produce garbage (documented — never silently "succeed").
export interface ShamirShare {
  /** Share index (1–255), the public x coordinate. */
  x: number
  /** Share bytes as lowercase hex. */
  hex: string
}

const MAX_SHARES = 255

// ── GF(256) with 0x11b ──────────────────────────────────────────────
// NB: 0x02 is NOT primitive in this field, so the exp/log tables use 0x03
// as generator (same convention as AES); 0x02 would leave holes in LOG.
const EXP = new Array<number>(512)
const LOG = new Array<number>(256)
function xtime(x: number): number {
  return ((x << 1) ^ (x & 0x80 ? 0x11b : 0)) & 0xff
}
{
  let x = 1
  for (let i = 0; i < 255; i++) {
    EXP[i] = x
    LOG[x] = i
    x = xtime(x) ^ x // multiply by 0x03
  }
  for (let i = 255; i < 512; i++) EXP[i] = EXP[i - 255]
}

function gfMul(a: number, b: number): number {
  if (a === 0 || b === 0) return 0
  return EXP[LOG[a] + LOG[b]]
}

function gfDiv(a: number, b: number): number {
  if (b === 0) throw new Error('shamir: division by zero')
  if (a === 0) return 0
  return EXP[(LOG[a] - LOG[b] + 255) % 255]
}

function randomByte(): number {
  const buf = new Uint8Array(1)
  crypto.getRandomValues(buf)
  return buf[0]
}

/** Split bytes into n shares, any m of which restore the secret. */
export function splitSecret(secret: Uint8Array, n: number, m: number): ShamirShare[] {
  if (!Number.isInteger(n) || !Number.isInteger(m)) throw new Error('shamir: n and m must be integers')
  if (m < 2) throw new Error('shamir: threshold m must be >= 2')
  if (m > n) throw new Error('shamir: threshold m must be <= n')
  if (n > MAX_SHARES) throw new Error('shamir: n must be <= 255')
  if (secret.length === 0) throw new Error('shamir: secret must not be empty')
  // coefficients[m-1][byte]: random polynomial per byte, a0 = secret.
  const xs: number[] = []
  const used = new Set<number>()
  while (xs.length < n) {
    const x = 1 + (randomByte() % 255)
    if (!used.has(x)) {
      used.add(x)
      xs.push(x)
    }
  }
  const ys: Uint8Array[] = xs.map(() => new Uint8Array(secret.length))
  for (let b = 0; b < secret.length; b++) {
    const coeff = new Array<number>(m)
    coeff[0] = secret[b]
    for (let i = 1; i < m; i++) coeff[i] = randomByte()
    for (let s = 0; s < n; s++) {
      // Horner: y = ((a_{m-1} * x + a_{m-2}) * x + … + a_0)
      let y = coeff[m - 1]
      for (let i = m - 2; i >= 0; i--) y = coeff[i] ^ gfMul(y, xs[s])
      ys[s][b] = y
    }
  }
  return xs.map((x, i) => ({ x, hex: [...ys[i]].map((v) => v.toString(16).padStart(2, '0')).join('') }))
}

/** Restore the secret from ≥m shares (Lagrange at x=0). */
export function combineShares(shares: ShamirShare[]): Uint8Array {
  if (shares.length < 2) throw new Error('shamir: need at least 2 shares')
  const xs = shares.map((s) => s.x)
  if (new Set(xs).size !== xs.length) throw new Error('shamir: duplicate share index')
  for (const x of xs) {
    if (!Number.isInteger(x) || x < 1 || x > 255) throw new Error('shamir: share index out of range')
  }
  const yHex = shares.map((s) => s.hex)
  const len = yHex[0].length
  if (len === 0 || len % 2 !== 0) throw new Error('shamir: malformed share data')
  const yBytes = yHex.map((h) => {
    if (!/^[0-9a-f]*$/.test(h) || h.length !== len) throw new Error('shamir: malformed share data')
    const out = new Uint8Array(h.length / 2)
    for (let i = 0; i < out.length; i++) out[i] = parseInt(h.slice(i * 2, i * 2 + 2), 16)
    return out
  })
  const secret = new Uint8Array(len / 2)
  for (let b = 0; b < secret.length; b++) {
    let acc = 0
    for (let j = 0; j < shares.length; j++) {
      let num = 1
      let den = 1
      for (let k = 0; k < shares.length; k++) {
        if (k === j) continue
        num = gfMul(num, xs[k])
        den = gfMul(den, xs[k] ^ xs[j])
      }
      acc ^= gfMul(yBytes[j][b], gfDiv(num, den))
    }
    secret[b] = acc
  }
  return secret
}

const encoder = new TextEncoder()
const decoder = new TextDecoder()

export function splitText(secret: string, n: number, m: number): ShamirShare[] {
  const bytes = encoder.encode(secret)
  if (bytes.length === 0) throw new Error('shamir: secret must not be empty')
  return splitSecret(bytes, n, m)
}

export function combineText(shares: ShamirShare[]): string {
  return decoder.decode(combineShares(shares))
}
