// CyberManju OS — Vite.config.wiki (vite.config.wiki.ts).
// Role: static build for the Three.js wiki SPA into dist-wasm/wiki/.
// Exports: default vite config (root wiki-src, base <VITE_BASE>wiki/).
// Integrates: `npm run wiki` chains generate-wiki.mjs then this build;
//   emptyOutDir stays OFF so the generator's .txt/.json/.html survive.
// Contracts: fully static output (no server, no env secrets baked in).
import { defineConfig } from "vite";
import { resolve } from "path";

// Same branch-aware base as the main bundle, plus the wiki subpath:
// main pushes serve /wiki/, develop pushes /develop/wiki/.
const base = `${process.env.VITE_BASE || "/"}wiki/`;

export default defineConfig({
  root: resolve(__dirname, "wiki-src"),
  base,
  build: {
    outDir: resolve(__dirname, "dist-wasm/wiki"),
    emptyOutDir: false,
    minify: "esbuild",
    sourcemap: false,
    chunkSizeWarningLimit: 900,
  },
  server: {
    port: 4176,
    strictPort: false,
    open: false,
  },
});
