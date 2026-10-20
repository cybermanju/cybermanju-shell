// CyberManju OS — swarm symbol framing (src/utils/p2pSwarm.ts).
// Role: frontend half of erasure-coded relay swarming — layout math,
// symbol envelope builders/validators, and backend-injected RS math. The
// Reed–Solomon computation itself runs natively (`swarm_encode`/
// `swarm_decode` Tauri commands) and on Pages (wasm `swarm_encode_json`/
// `swarm_decode_json`); this module never reimplements the codec, so the
// math stays single-sourced in `cybermanju-erasure` on every transport.
// Exports: SwarmSymbol, SWARM_KIND, swarmLayout, buildSwarmPlan,
// validateSwarmSymbol, parseSwarmSymbols, swarmBackendFromWasm,
// encodeSwarm, decodeSwarm, type SwarmBackend, FountainSymbol,
// FOUNTAIN_KIND, validateFountainSymbol, parseFountainSymbols,
// buildFountainPlan, fountainBackendFromWasm, fountainBackendFromTauri,
// encodeFountain, decodeFountain.
// Integrates: bundle payload spray (`kind: swarm-symbol` relay messages);
// DevicesPanel swarm demo; Rust twins are `sync::swarm` + `os-wasm/src/
// swarm.rs` (same JSON, same caps, pinned by p2p-swarm.test.ts).
// Contracts: pure builders throw only `invalid:`/`too_large:` (AGENT-1);
// without an injected backend the math answers `unsupported:` naming the
// exact wiring (Tauri IPC vs wasm export) — never a guessed decode.

export interface SwarmSymbol {
  object: string
  dataShards: number
  parityShards: number
  index: number
  shardLen: number
  blake3: string
  bytesBase64: string
}

/** Relay `kind` for swarm symbols (mirrors Rust `swarm::SWARM_KIND`). */
export const SWARM_KIND = 'swarm-symbol'

/** Target shard size (mirrors Rust `SWARM_TARGET_SHARD`). */
export const SWARM_TARGET_SHARD = 64 * 1024

/** Max relay message on the wire (mirrors Rust `RELAY_MAX_BYTES`). */
export const SWARM_MAX_MESSAGE = 256 * 1024

/** Max swarm object (mirrors `BUNDLE_MAX_PAYLOAD_TOTAL`). */
export const SWARM_MAX_OBJECT = 50 * 1024 * 1024

/** RS math backend (native IPC twins or wasm exports — injected, never imported). */
export interface SwarmBackend {
  encode(payload: Uint8Array, k: number, m: number): Promise<SwarmSymbol[]>
  decode(symbols: SwarmSymbol[]): Promise<Uint8Array>
}

/**
 * Default `(k, m)` layout for a payload length — same heuristic as Rust
 * `swarm::swarm_layout` (shards near 64 KiB, `m ≈ k/2` in 1..=8).
 */
export function swarmLayout(payloadLen: number): { k: number; m: number } {
  const framed = Math.max(0, Math.floor(payloadLen) || 0) + 12
  const k = Math.max(1, Math.min(32, Math.ceil(framed / SWARM_TARGET_SHARD)))
  const m = Math.max(1, Math.min(8, Math.ceil(k / 2)))
  return { k, m }
}

/** Offline dial plan for a swarm send (layout + symbol budget, no math). */
export function buildSwarmPlan(payloadLen: number, k?: number, m?: number): string[] {
  const len = Math.max(0, Math.floor(payloadLen) || 0)
  if (len > SWARM_MAX_OBJECT) {
    throw new Error(
      `too_large: swarm object is ${len} bytes (cap ${SWARM_MAX_OBJECT} — ship it via the sync backends)`,
    )
  }
  const layout = k != null && m != null ? { k, m } : swarmLayout(len)
  if (!Number.isInteger(layout.k) || !Number.isInteger(layout.m) || layout.k < 1 || layout.m < 1 || layout.k + layout.m > 256) {
    throw new Error('invalid: swarm layout needs k>=1, m>=1, k+m<=256')
  }
  const shardLen = Math.max(1, Math.ceil((len + 12) / layout.k))
  return [
    `object: ${len} bytes → k=${layout.k} data + m=${layout.m} parity (shard ~${shardLen} B)`,
    `spray: POST /api/relay/drop { kind: ${SWARM_KIND} } × ${layout.k + layout.m} symbols (any peer, any order)`,
    `collect: POST /api/relay/poll until ≥${layout.k} distinct indices of one object hash`,
    `decode: any ${layout.k} symbols reconstruct byte-identically (dropped messages are non-events)`,
  ]
}

/** Validate one symbol envelope (shape + ranges, not the hash — the backend checks bytes). */
export function validateSwarmSymbol(value: unknown): SwarmSymbol {
  if (!value || typeof value !== 'object') throw new Error('invalid: swarm symbol must be an object')
  const s = value as Record<string, unknown>
  const symbol: SwarmSymbol = {
    object: String(s.object ?? ''),
    dataShards: Number(s.dataShards ?? 0),
    parityShards: Number(s.parityShards ?? 0),
    index: Number(s.index ?? -1),
    shardLen: Number(s.shardLen ?? 0),
    blake3: String(s.blake3 ?? ''),
    bytesBase64: String(s.bytesBase64 ?? ''),
  }
  if (!/^[0-9a-f]{64}$/.test(symbol.object)) throw new Error('invalid: swarm symbol needs a 64-hex object hash')
  if (!Number.isInteger(symbol.dataShards) || !Number.isInteger(symbol.parityShards) || symbol.dataShards < 1 || symbol.parityShards < 1 || symbol.dataShards + symbol.parityShards > 256) {
    throw new Error('invalid: swarm symbol layout needs k>=1, m>=1, k+m<=256')
  }
  if (!Number.isInteger(symbol.index) || symbol.index < 0 || symbol.index >= symbol.dataShards + symbol.parityShards) {
    throw new Error('invalid: swarm symbol index is out of range')
  }
  if (!Number.isInteger(symbol.shardLen) || symbol.shardLen < 1) {
    throw new Error('invalid: swarm symbol needs a shard length')
  }
  if (!symbol.bytesBase64.trim()) throw new Error('invalid: swarm symbol carries no bytes')
  return symbol
}

/** Parse + validate a symbols array (JSON string or parsed value). */
export function parseSwarmSymbols(input: unknown): SwarmSymbol[] {
  let arr: unknown = input
  if (typeof input === 'string') {
    try {
      arr = JSON.parse(input)
    } catch {
      throw new Error('invalid: symbols are not JSON')
    }
  }
  if (!Array.isArray(arr)) throw new Error('invalid: swarm decode needs an array of symbols')
  return arr.map(validateSwarmSymbol)
}

function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64.trim())
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

function bytesToBase64(bytes: Uint8Array): string {
  let bin = ''
  const CHUNK = 0x8000
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK))
  }
  return btoa(bin)
}

/**
 * Build a backend from wasm exports (`swarm_encode_json`/
 * `swarm_decode_json` string in/out — the `os-wasm/src/swarm.rs` shape).
 * The loader is injected (usually `wasmModuleExports`) so this module
 * imports no composable and creates no cycle.
 */
export function swarmBackendFromWasm(
  loadWasm: () => Promise<{
    swarm_encode_json: (payload: Uint8Array, k: number, m: number) => string
    swarm_decode_json: (symbolsJson: string) => Uint8Array
    swarm_layout_json?: (len: number) => string
  }>,
): SwarmBackend {
  return {
    async encode(payload: Uint8Array, k: number, m: number): Promise<SwarmSymbol[]> {
      const w = await loadWasm().catch((e) => {
        throw new Error(
          `unsupported: swarm encode needs the rebuilt wasm crypto bundle — reconnect after the next Pages deploy (${e instanceof Error ? e.message : String(e)})`,
        )
      })
      const json = w.swarm_encode_json(payload, k, m)
      return parseSwarmSymbols(json)
    },
    async decode(symbols: SwarmSymbol[]): Promise<Uint8Array> {
      const w = await loadWasm().catch((e) => {
        throw new Error(
          `unsupported: swarm decode needs the rebuilt wasm crypto bundle — reconnect after the next Pages deploy (${e instanceof Error ? e.message : String(e)})`,
        )
      })
      return w.swarm_decode_json(JSON.stringify(symbols))
    },
  }
}

/** Encode via an injected backend (Tauri IPC twins or wasm exports). */
export async function encodeSwarm(
  payload: Uint8Array,
  backend: SwarmBackend | null,
  k?: number,
  m?: number,
): Promise<SwarmSymbol[]> {
  if (!backend) {
    throw new Error(
      'unsupported: swarm encode needs a backend — desktop/mobile use the `swarm_encode` IPC twin, Pages uses the wasm `swarm_encode_json` export',
    )
  }
  const layout = k != null && m != null ? { k, m } : swarmLayout(payload.length)
  return backend.encode(payload, layout.k, layout.m)
}

/** Decode via an injected backend (any k distinct symbols suffice). */
export async function decodeSwarm(
  symbols: unknown,
  backend: SwarmBackend | null,
): Promise<Uint8Array> {
  const parsed = parseSwarmSymbols(symbols)
  if (!backend) {
    throw new Error(
      'unsupported: swarm decode needs a backend — desktop/mobile use the `swarm_decode` IPC twin, Pages uses the wasm `swarm_decode_json` export',
    )
  }
  return backend.decode(parsed)
}

export { base64ToBytes as swarmBase64ToBytes, bytesToBase64 as swarmBytesToBase64 }

// ─── RaptorQ fountain symbols (RFC 6330 twin of native `swarm::fountain_*`) ──

export interface FountainSymbol {
  object: string
  otiBase64: string
  mtu: number
  blake3: string
  bytesBase64: string
}

/** Relay `kind` for fountain symbols (mirrors Rust `swarm::FOUNTAIN_KIND`). */
export const FOUNTAIN_KIND = 'fountain-symbol'

/** Default fountain MTU (mirrors Rust `swarm::FOUNTAIN_DEFAULT_MTU`). */
export const FOUNTAIN_DEFAULT_MTU = 32768

/** RaptorQ math backend (native IPC twins or wasm exports — injected). */
export interface FountainBackend {
  encode(payload: Uint8Array, mtu: number, repair: number): Promise<FountainSymbol[]>
  decode(symbols: FountainSymbol[]): Promise<Uint8Array>
}

/** Validate one fountain symbol envelope (shape, not the bytes — the backend checks those). */
export function validateFountainSymbol(value: unknown): FountainSymbol {
  if (!value || typeof value !== 'object') throw new Error('invalid: fountain symbol must be an object')
  const s = value as Record<string, unknown>
  const symbol: FountainSymbol = {
    object: String(s.object ?? ''),
    otiBase64: String(s.otiBase64 ?? ''),
    mtu: Number(s.mtu ?? 0),
    blake3: String(s.blake3 ?? ''),
    bytesBase64: String(s.bytesBase64 ?? ''),
  }
  if (!/^[0-9a-f]{64}$/.test(symbol.object)) throw new Error('invalid: fountain symbol needs a 64-hex object hash')
  if (!symbol.otiBase64.trim()) throw new Error('invalid: fountain symbol needs its OTI (self-describing subsets)')
  if (!Number.isInteger(symbol.mtu) || symbol.mtu < 1 || symbol.mtu > 65535) {
    throw new Error('invalid: fountain symbol MTU must be 1..=65535')
  }
  if (!symbol.bytesBase64.trim()) throw new Error('invalid: fountain symbol carries no bytes')
  return symbol
}

/** Parse + validate a fountain symbols array (JSON string or parsed value). */
export function parseFountainSymbols(input: unknown): FountainSymbol[] {
  let arr: unknown = input
  if (typeof input === 'string') {
    try {
      arr = JSON.parse(input)
    } catch {
      throw new Error('invalid: symbols are not JSON')
    }
  }
  if (!Array.isArray(arr)) throw new Error('invalid: fountain decode needs an array of symbols')
  return arr.map(validateFountainSymbol)
}

/** Offline dial plan for a fountain send (spray budget, no math). */
export function buildFountainPlan(payloadLen: number, mtu = FOUNTAIN_DEFAULT_MTU, repair = 64): string[] {
  const len = Math.max(0, Math.floor(payloadLen) || 0)
  if (len === 0) throw new Error('invalid: fountain encode needs a non-empty payload')
  if (len > SWARM_MAX_OBJECT) {
    throw new Error(
      `too_large: fountain object is ${len} bytes (cap ${SWARM_MAX_OBJECT} — ship it via the sync backends)`,
    )
  }
  if (!Number.isInteger(mtu) || mtu < 1 || mtu > 65535) {
    throw new Error('invalid: fountain MTU must be 1..=65535')
  }
  if (!Number.isInteger(repair) || repair < 0) {
    throw new Error('invalid: fountain repair count must be >= 0')
  }
  return [
    `object: ${len} bytes → RaptorQ symbols (~${mtu} B each) + ${repair} repair packets`,
    `spray: POST /api/relay/drop { kind: ${FOUNTAIN_KIND} } (any peers, any order — fountains never run out)`,
    `collect: POST /api/relay/poll until decode completes (any K+h symbols suffice)`,
    `decode: first complete decode wins byte-identically (dropped messages are non-events)`,
  ]
}

/**
 * Build a fountain backend from wasm exports (`fountain_encode_json`/
 * `fountain_decode_json` string in/out). Loader injected (no cycles).
 */
export function fountainBackendFromWasm(
  loadWasm: () => Promise<{
    fountain_encode_json: (payload: Uint8Array, mtu: number, repair: number) => string
    fountain_decode_json: (symbolsJson: string) => Uint8Array
  }>,
): FountainBackend {
  return {
    async encode(payload: Uint8Array, mtu: number, repair: number): Promise<FountainSymbol[]> {
      const w = await loadWasm().catch((e) => {
        throw new Error(
          `unsupported: fountain encode needs the rebuilt wasm bundle — reconnect after the next Pages deploy (${e instanceof Error ? e.message : String(e)})`,
        )
      })
      return parseFountainSymbols(w.fountain_encode_json(payload, mtu, repair))
    },
    async decode(symbols: FountainSymbol[]): Promise<Uint8Array> {
      const w = await loadWasm().catch((e) => {
        throw new Error(
          `unsupported: fountain decode needs the rebuilt wasm bundle — reconnect after the next Pages deploy (${e instanceof Error ? e.message : String(e)})`,
        )
      })
      return w.fountain_decode_json(JSON.stringify(symbols))
    },
  }
}

/**
 * Build a fountain backend from a Tauri `invoke` function (native
 * `fountain_encode`/`fountain_decode` twins). Invoke injected (no cycles).
 */
export function fountainBackendFromTauri(
  invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>,
): FountainBackend {
  const toB64 = (bytes: Uint8Array): string => bytesToBase64(bytes)
  const fromB64 = (b64: string): Uint8Array => base64ToBytes(b64)
  return {
    async encode(payload: Uint8Array, mtu: number, repair: number): Promise<FountainSymbol[]> {
      const raw = (await invoke('fountain_encode', {
        bytesBase64: toB64(payload),
        mtu,
        repair,
        // Tauri binds by exact param name — send both cases.
        bytes_base64: toB64(payload),
      })) as unknown
      return parseFountainSymbols(raw)
    },
    async decode(symbols: FountainSymbol[]): Promise<Uint8Array> {
      const raw = (await invoke('fountain_decode', {
        symbolsJson: JSON.stringify(symbols),
        symbols_json: JSON.stringify(symbols),
      })) as string
      return fromB64(String(raw ?? ''))
    },
  }
}

/** Encode via an injected fountain backend (Tauri IPC or wasm export). */
export async function encodeFountain(
  payload: Uint8Array,
  backend: FountainBackend | null,
  mtu = FOUNTAIN_DEFAULT_MTU,
  repair = 64,
): Promise<FountainSymbol[]> {
  if (!backend) {
    throw new Error(
      'unsupported: fountain encode needs a backend — desktop/mobile use the `fountain_encode` IPC twin, Pages uses the wasm `fountain_encode_json` export',
    )
  }
  return backend.encode(payload, mtu, repair)
}

/** Decode via an injected fountain backend (any sufficient subset suffices). */
export async function decodeFountain(
  symbols: unknown,
  backend: FountainBackend | null,
): Promise<Uint8Array> {
  const parsed = parseFountainSymbols(symbols)
  if (!backend) {
    throw new Error(
      'unsupported: fountain decode needs a backend — desktop/mobile use the `fountain_decode` IPC twin, Pages uses the wasm `fountain_decode_json` export',
    )
  }
  return backend.decode(parsed)
}
