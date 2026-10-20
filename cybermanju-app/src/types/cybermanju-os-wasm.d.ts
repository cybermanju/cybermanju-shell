// Type declaration for the wasm-pack bundle of `crates/os-wasm`
// (resolved at build time via the `cybermanju-os-wasm` alias in
// vite.config.wasm.ts). Mirrors the `#[wasm_bindgen]` exports in
// `crates/os-wasm/src` — keep it in sync when an entry point lands
// (pkg/ itself is a gitignored CI artifact and is never committed).
declare module 'cybermanju-os-wasm' {
  /** One entry point for every transport: dispatches an os/* command. */
  export function os_dispatch(cmd: string, args_json: string): string
  /** redb-in-OPFS handshake; resolves to the worker's storage environment. */
  export function db_open(): Promise<string>
  /** One database op against the worker-backed redb store. */
  export function db_dispatch(op: string, argsJson: string): string
  /** Serialized database image (for `.cybermanju` export). */
  export function db_snapshot(): unknown
  /** Restore a previously exported database image. */
  export function db_restore(data: Uint8Array): string
  /** Provider presets from the shared Rust core (pure, no network). */
  export function agent_catalog(): string
  /** ONE provider turn over browser `fetch`; JSON in, JSON out. */
  export function agent_prompt(reqJson: string): Promise<string>
  /** wasm-pack `--target web` init hook. */
  export default function init(): Promise<void>
}
