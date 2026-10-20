// CyberManju OS — human pairing codes (src/utils/pairing.ts).
// Role: wormhole-style rendezvous without a wormhole server — short
// `NNN-word-word` codes two humans read aloud (`482-oak-river`); the sender
// drops to the code-derived mailbox (`pair:<code>`), the receiver polls it.
// Codes are *rendezvous*, not PAKE (guessing is bounded by the 4.1M code
// space + the relay's JWT gate + mailbox depth caps); attestation still
// comes from ML-DSA envelopes (`auth`), never from the code.
// Exports: PAIRING_WORDS, PAIRING_PREFIX, makePairingCode, parsePairingCode,
// pairingTarget, isPairingTarget, dropPairingEnvelope, claimPairingEnvelopes.
// Integrates: `pair` cybsh verb; DevicesPanel pair card; the relay mailbox
// (`dropRelayMessage`/`pollRelayMailbox`/`ackRelayMessages`); Rust twin is
// `peers::{pairing_code, normalize_pairing_code, pairing_recipient}`
// (same grammar, same wordlist, pinned by p2p-pairing.test.ts).
// Contracts: pure builders throw only `invalid:` (AGENT-1); randomness is
// WebCrypto (`crypto.getRandomValues`) — never `Math.random`; nothing here
// opens a socket (delivery rides the injected relay fetch deps).

import {
  ackRelayMessages,
  dropRelayMessage,
  pollRelayMailbox,
  type RelayFetchDeps,
  type RelayMessage,
} from './p2pPresence'

/** Code wordlist — byte-identical to Rust `peers::PAIRING_WORDS`. */
export const PAIRING_WORDS: readonly string[] = [
  'oak', 'river', 'stone', 'birch', 'comet', 'harbor', 'meadow', 'ridge', 'willow', 'flint',
  'grove', 'summit', 'brook', 'cedar', 'dune', 'ember', 'fjord', 'glacier', 'heath', 'inlet',
  'juniper', 'kelp', 'lagoon', 'maple', 'north', 'otter', 'pine', 'quarry', 'reef', 'spruce',
  'tundra', 'umber', 'vale', 'wolf', 'yonder', 'zinc', 'acorn', 'basalt', 'cliff', 'drift',
  'elm', 'fern', 'granite', 'holly', 'iris', 'jade', 'kestrel', 'lichen', 'moss', 'nimbus',
  'onyx', 'prairie', 'quartz', 'robin', 'sage', 'thistle', 'upland', 'vetch', 'wren', 'yarrow',
  'alder', 'beacon', 'cinder', 'dawn',
]

/** Mailbox prefix for code-derived recipients. */
export const PAIRING_PREFIX = 'pair:'

function randomU32(): number {
  const g = (globalThis as { crypto?: { getRandomValues?: (a: Uint32Array) => void } }).crypto
  if (!g?.getRandomValues) {
    throw new Error('unsupported: pairing codes need WebCrypto (browser or Node 18+)')
  }
  const buf = new Uint32Array(1)
  g.getRandomValues(buf)
  return buf[0]
}

/** Mint a fresh `NNN-word-word` code (WebCrypto entropy, lowercase). */
export function makePairingCode(): string {
  const n = randomU32()
  const num = String(n % 1000).padStart(3, '0')
  const w1 = PAIRING_WORDS[(n >>> 10) % PAIRING_WORDS.length]
  const w2 = PAIRING_WORDS[(n >>> 16) % PAIRING_WORDS.length]
  return `${num}-${w1}-${w2}`
}

/** Validate + normalize a code (case-insensitive in, lowercase out). */
export function parsePairingCode(code: string): string {
  const low = code.trim().toLowerCase()
  const parts = low.split('-')
  if (parts.length !== 3) {
    throw new Error('invalid: pairing code is NNN-word-word (e.g. 482-oak-river)')
  }
  if (parts[0].length !== 3 || !/^\d{3}$/.test(parts[0])) {
    throw new Error('invalid: pairing code starts with 3 digits (e.g. 482-oak-river)')
  }
  for (const w of [parts[1], parts[2]]) {
    if (!PAIRING_WORDS.includes(w)) {
      throw new Error(`invalid: pairing word '${w}' is not in the code wordlist`)
    }
  }
  return low
}

/** Mailbox recipient for a code (`pair:482-oak-river`). */
export function pairingTarget(code: string): string {
  return `${PAIRING_PREFIX}${parsePairingCode(code)}`
}

/** True when a mailbox id is a code-derived recipient (never a device id). */
export function isPairingTarget(to: string): boolean {
  const code = to.trim().startsWith(PAIRING_PREFIX) ? to.trim().slice(PAIRING_PREFIX.length) : null
  if (!code) return false
  try {
    parsePairingCode(code)
    return true
  } catch {
    return false
  }
}

/** Drop one envelope to a pairing code's mailbox (sender side). */
export async function dropPairingEnvelope(
  relayBase: string,
  from: string,
  code: string,
  kind: string,
  payload: unknown,
  deps: RelayFetchDeps,
): Promise<{ accepted: boolean; id: string }> {
  return dropRelayMessage(relayBase, from, pairingTarget(code), kind, payload, deps)
}

/**
 * Claim a pairing code's mailbox (receiver side): poll, then ack what was
 * read so a dropped poll never loses mail but a delivered one never
 * replays. Returns the messages in mailbox order.
 */
export async function claimPairingEnvelopes(
  relayBase: string,
  code: string,
  deps: RelayFetchDeps,
): Promise<RelayMessage[]> {
  const target = pairingTarget(code)
  const messages = await pollRelayMailbox(relayBase, target, deps)
  const ids = messages.map((m) => m.id).filter((id) => id.trim())
  if (ids.length > 0) {
    await ackRelayMessages(relayBase, target, ids, deps).catch(() => 0)
  }
  return messages
}
