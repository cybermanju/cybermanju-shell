// CyberManju OS — Vite.config.wasm (vite.config.wasm.ts).
// Role: module (see exports below).
// Exports: module root (see items below).
// Integrates: vite, @vitejs, path, ./….
// Contracts: none special (pure logic; errors bubble to caller).
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { resolve } from "path";
import wasmStub from "./vite-plugin-wasm-stub";
import { wasmPkgEntry } from "./wasm-pkg";

// Determine the base path:
// - GitHub Pages (cybermanju.github.io root site): /
// - Docker / standalone: / (served from root)
// Override with VITE_BASE env var if needed
const base = process.env.VITE_BASE || "/";

export default defineConfig({
  plugins: [vue(), wasmStub()],
  resolve: {
    alias: {
      "@": resolve(__dirname, "src"),
      // wasm-pack output (`wasm-pack build crates/os-wasm --target web
      // --out-dir crates/os-wasm/pkg`) — the integrated backend for the
      // static/GH-Pages bundle. Used only when present AND not older than
      // crates/**: a stale pkg falls back to the stub instead of shipping an
      // artifact that no longer matches the Rust (see wasm-pkg.ts).
      // NOTE: the entry FILE (not the directory) is aliased — worker
      // bundles don't apply package.json directory resolution, so a
      // directory alias EISDIRs the db-worker chunk. Without a usable pkg
      // (Docker frontend stage) the stub file stands in, same guarantee.
      "cybermanju-os-wasm":
        wasmPkgEntry(__dirname) ?? resolve(__dirname, "src/wasm-stub.js"),
    },
  },
  base,
  build: {
    outDir: "dist-wasm",
    emptyOutDir: true,
    // Chunk splitting for better caching in production
    rollupOptions: {
      output: {
        manualChunks: {
          "vendor-vue": ["vue", "pinia"],
          "vendor-map": ["maplibre-gl"],
          "vendor-icons": ["@iconify/vue"],
        },
      },
    },
    // Source maps for debugging (disabled in production for smaller bundles)
    sourcemap: process.env.NODE_ENV !== "production",
    // Minification settings
    minify: "esbuild",
    // Chunk size warning threshold (500KB)
    chunkSizeWarningLimit: 500,
  },
  // Ensure Tauri APIs are stubbed out for web/WASM builds
  define: {
    __TAURI__: "false",
    "window.__TAURI__": "false",
    "import.meta.env.TAURI": "false",
    "import.meta.env.VITE_TAURI": "false",
  },
  css: {
    devSourcemap: true,
  },
  // Development server for local WASM testing
  server: {
    port: 4174,
    strictPort: false,
    open: false,
  },
  // Preview server configuration
  preview: {
    port: 4175,
  },
});
