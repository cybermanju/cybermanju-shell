// CyberManju OS — Vitest.config (vitest.config.ts).
// Vitest config (AGENT-4 item 19). Node environment on purpose: the suite
// exercises transport/pure logic, not rendered DOM. AGENT-5 adds component
// tests on top — put them under src/**/*.{test,spec}.ts and they are picked
// up by the include patterns below.
import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts', 'src/**/*.{test,spec}.ts'],
  },
})
