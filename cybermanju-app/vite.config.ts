// CyberManju OS — Vite.config (vite.config.ts).
// Role: module (see exports below).
// Exports: module root (see items below).
// Integrates: vite, @vitejs, path, ./….
// Contracts: none special (pure logic; errors bubble to caller).
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { resolve } from "path";
import wasmStub from "./vite-plugin-wasm-stub";
import { wasmPkgEntry } from "./wasm-pkg";

const host = process.env.TAURI_DEV_HOST;

// Real wasm-pack output, when present AND still newer than crates/ (a stale
// pkg is ignored — see wasm-pkg.ts). Otherwise the stub file below stands in
// — a real file (not a virtual module) so worker bundles resolve it
// deterministically. The entry file (not the directory) is aliased: worker
// pipelines don't apply package.json directory resolution the same way the
// main bundle does.
const wasmTarget =
  wasmPkgEntry(__dirname) ?? resolve(__dirname, "src/wasm-stub.js");

export default defineConfig(async () => ({
  plugins: [vue(), wasmStub()],
  // Worker bundles don't inherit config plugins — the db worker imports
  // 'cybermanju-os-wasm', so the stub must apply there too.
  worker: {
    format: 'es',
    plugins: () => [wasmStub()],
  },
  resolve: {
    alias: {
      "@": resolve(__dirname, "src"),
      "cybermanju-os-wasm": wasmTarget,
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    allowedHosts: [".manus.computer"],
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
