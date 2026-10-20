// CyberManju OS — Env.d (env.d.ts).
/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_BASE?: string
  readonly VITE_TRANSPORT?: 'tauri' | 'rest' | 'wasm'
  readonly VITE_API_URL?: string
  /** Supabase OAuth broker, baked at build time from GH Secrets (Pages) or `.env` (local). */
  readonly VITE_SUPABASE_URL?: string
  readonly VITE_SUPABASE_ANON_KEY?: string
  /** Legacy alias, read when `VITE_SUPABASE_ANON_KEY` is unset. */
  readonly VITE_SUPABASE_KEY?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<{}, {}, any>
  export default component
}

declare module '*?raw' {
  const content: string
  export default content
}
