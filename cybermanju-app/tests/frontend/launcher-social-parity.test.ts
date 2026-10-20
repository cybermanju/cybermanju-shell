// CyberManju OS — Launcher social parity (tests/frontend/launcher-social-parity.test.ts).
// Role: Vitest regression suite (node env, no deps).
// Exports: module root (see items below).
// Integrates: node:fs, vitest, crates/launcher/src/lib.rs, scripts/android-configure.sh.
// Contracts: the generated Kotlin notification gate (CybermanjuMessages.kt,
//   written by scripts/android-configure.sh §4) mirrors the Rust source of
//   truth `is_social_package` — Kotlin SOCIAL ⊇ Rust SOCIAL_PACKAGES,
//   Kotlin TOKENS ⊇ Rust SOCIAL_TOKENS, Kotlin SEGMENTS == Rust
//   SOCIAL_SEGMENTS, and the isSocial() rule chain includes the dot-segment
//   rule. A silently-shrunken Kotlin list means notifications from those
//   apps are never mirrored into the hub while docs claim parity.
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

const RUST_SRC = readFileSync('crates/launcher/src/lib.rs', 'utf8')
const SCRIPT_SRC = readFileSync('scripts/android-configure.sh', 'utf8')

/** String array of a Rust `const NAME: &[&str] = &[ ... ];` literal. */
function rustArray(name: string): string[] {
  const m = RUST_SRC.match(
    new RegExp(`(?:pub\\s+)?const\\s+${name}\\s*:\\s*&\\[&str\\]\\s*=\\s*&\\[([\\s\\S]*?)\\];`),
  )
  if (!m) {
    throw new Error(
      `Rust ${name}: &[&str] literal not found in crates/launcher/src/lib.rs — update the extraction regex here if the const was renamed/moved`,
    )
  }
  return [...m[1].matchAll(/"([^"]*)"/g)].map((d) => d[1])
}

/** String array of the embedded Kotlin `private val NAME = setOf/listOf( ... )`. */
function kotlinArray(name: string): string[] {
  const m = SCRIPT_SRC.match(
    new RegExp(`private\\s+val\\s+${name}\\s*=\\s*(?:setOf|listOf)\\(([\\s\\S]*?)\\)`),
  )
  if (!m) {
    throw new Error(
      `Kotlin ${name} literal not found in the embedded CybermanjuMessages.kt of scripts/android-configure.sh — update the extraction regex here if the val was renamed/moved`,
    )
  }
  return [...m[1].matchAll(/"([^"]*)"/g)].map((d) => d[1])
}

/** Body of the embedded Kotlin `private fun isSocial(pkg, n)` (up to its closing brace). */
function kotlinIsSocialBody(): string {
  const idx = SCRIPT_SRC.indexOf('private fun isSocial')
  if (idx < 0) {
    throw new Error(
      'Kotlin isSocial() not found in the embedded CybermanjuMessages.kt of scripts/android-configure.sh',
    )
  }
  const end = SCRIPT_SRC.indexOf('\n    }', idx)
  if (end < 0) {
    throw new Error('Kotlin isSocial() closing brace not found — extraction regex needs updating')
  }
  return SCRIPT_SRC.slice(idx, end)
}

describe('launcher social parity (Rust is_social_package vs generated Kotlin gate)', () => {
  const rustPackages = rustArray('SOCIAL_PACKAGES')
  const rustTokens = rustArray('SOCIAL_TOKENS')
  const rustSegments = rustArray('SOCIAL_SEGMENTS')
  const kotlinSocial = kotlinArray('SOCIAL')
  const kotlinTokens = kotlinArray('TOKENS')
  const kotlinSegments = kotlinArray('SEGMENTS')

  it('extracts non-trivial arrays from both sources', () => {
    expect(rustPackages.length, 'SOCIAL_PACKAGES in crates/launcher/src/lib.rs looks empty').toBeGreaterThan(30)
    expect(rustTokens.length, 'SOCIAL_TOKENS in crates/launcher/src/lib.rs looks empty').toBeGreaterThan(20)
    expect(rustSegments, 'SOCIAL_SEGMENTS in crates/launcher/src/lib.rs changed shape').not.toEqual([])
    expect(kotlinSocial.length, 'Kotlin SOCIAL set in android-configure.sh looks empty').toBeGreaterThan(30)
    expect(kotlinTokens.length, 'Kotlin TOKENS list in android-configure.sh looks empty').toBeGreaterThan(20)
    expect(kotlinSegments, 'Kotlin SEGMENTS list in android-configure.sh looks empty').not.toEqual([])
  })

  it('Kotlin SOCIAL contains every Rust SOCIAL_PACKAGES entry', () => {
    const missing = rustPackages.filter((p) => !kotlinSocial.includes(p))
    expect(
      missing,
      `Kotlin SOCIAL set (scripts/android-configure.sh CybermanjuMessages.kt) is missing Rust SOCIAL_PACKAGES entries — notifications from these apps would never mirror into the hub: ${missing.join(', ')}`,
    ).toEqual([])
  })

  it('Kotlin TOKENS contains every Rust SOCIAL_TOKENS entry', () => {
    const missing = rustTokens.filter((t) => !kotlinTokens.includes(t))
    expect(
      missing,
      `Kotlin TOKENS list (scripts/android-configure.sh CybermanjuMessages.kt) is missing Rust SOCIAL_TOKENS entries — packages matched only via these tokens would never mirror into the hub: ${missing.join(', ')}`,
    ).toEqual([])
  })

  it('Kotlin SEGMENTS equals Rust SOCIAL_SEGMENTS', () => {
    expect(
      [...kotlinSegments].sort(),
      'Kotlin SEGMENTS list must equal Rust SOCIAL_SEGMENTS (line/mail/sms/mms/talk/chat) — a divergent list silently changes which dot-segments count as social',
    ).toEqual([...rustSegments].sort())
  })

  it('Kotlin isSocial() applies the same rule chain as Rust is_social_package()', () => {
    const body = kotlinIsSocialBody()
    expect(body, 'isSocial() must lowercase + trim before matching (Rust: package.trim().to_lowercase())').toContain('pkg.trim().lowercase()')
    expect(body, 'isSocial() must test the empty package like Rust is_social_package').toContain('if (p.isEmpty()) return false')
    expect(body, 'isSocial() must keep the exact-package check against SOCIAL (Rust step 1)').toContain('if (p in SOCIAL) return true')
    expect(body, 'isSocial() must keep the token-contains check against TOKENS (Rust step 2)').toContain('for (t in TOKENS) if (p.contains(t)) return true')
    expect(body, "isSocial() must keep the dot-segment check — Rust splits on '.' (Rust step 3)").toContain("p.split('.')")
    expect(body, 'isSocial() must match whole dot-segments against SEGMENTS (Rust step 3)').toContain('it in SEGMENTS')
  })
})
