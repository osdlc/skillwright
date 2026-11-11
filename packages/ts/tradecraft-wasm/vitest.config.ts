import { fileURLToPath } from 'node:url'

import { defineConfig } from 'vitest/config'

const src = (path: string) => fileURLToPath(new URL(path, import.meta.url))

export default defineConfig({
  resolve: {
    // The source imports the glue as `#wasm` (package.json maps that to
    // dist/). Point it at wasm/, so the tests need `pnpm run build:wasm` but
    // not the TypeScript build.
    alias: [{ find: /^#wasm$/, replacement: src('./wasm/tradecraft_wasm.js') }],
  },
  test: {
    include: ['tests/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      include: ['src/**/*.ts'],
      reporter: ['text'],
    },
  },
})
