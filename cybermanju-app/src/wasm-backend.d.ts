// Types for the `cybermanju-os-wasm` build artifact.
//
// In the static/WASM bundle this resolves to `crates/os-wasm/pkg`
// (vite.config.wasm.ts alias); in desktop builds the virtual-module stub
// (vite-plugin-wasm-stub) stands in. Declared here so `vue-tsc` is
// deterministic whether or not `pkg/` has been built.
declare module 'cybermanju-os-wasm' {
  const init: () => Promise<void>;
  export default init;
  export function os_dispatch(cmd: string, argsJson: string): string;
  export function db_open(): Promise<string>;
  export function db_dispatch(op: string, argsJson: string): string;
  export function db_snapshot(): unknown;
  export function db_restore(data: Uint8Array): string;
  /** Provider presets from the shared Rust core (pure, no network). */
  export function agent_catalog(): string;
  /** ONE provider turn over browser `fetch`; JSON in, JSON out. */
  export function agent_prompt(reqJson: string): Promise<string>;
}
