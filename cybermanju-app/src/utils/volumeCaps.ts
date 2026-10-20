// CyberManju OS — Volume write caps (src/utils/volumeCaps.ts).
// Role: single source of truth for the 1 MiB single-write cap shared by the
// WASM volume, the static cybsh shell, and the mirror queue (ANALYSIS.md §2C
// follow-up for P1-16/P1-17: the triple-writer race needs one cap + one queue,
// not three divergent constants).
// Exports: VOLUME_WRITE_LIMIT, isOverWriteCap, writeCapError.
// Integrates: staticCybsh re-exports STATIC_WRITE_LIMIT from here;
// volumeWriteQueue enforces the cap before serializing; Rust twin is
// MAX_WRITE_BYTES in crates/os-wasm/src/os.rs (pinned by volume-caps.test.ts
// reading that file statically — no cargo needed).
// Contracts: pure, never throws; cap is exactly 1 MiB (1048576 bytes);
// over-cap answers are `too_large:` so the AGENT-1 matrix hints correctly.
export const VOLUME_WRITE_LIMIT = 1024 * 1024

/** `true` when `bytes` would exceed the single-write cap. */
export function isOverWriteCap(bytes: number): boolean {
  return bytes > VOLUME_WRITE_LIMIT
}

/** `too_large:` refusal for a write of `bytes` at `where` (path or stage). */
export function writeCapError(bytes: number, where: string): string {
  return `too_large: ${where} is ${bytes} bytes, volume write limit is ${VOLUME_WRITE_LIMIT}`
}
