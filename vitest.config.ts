import { defineConfig } from 'vitest/config';
import vue from '@vitejs/plugin-vue';
import path from 'node:path';

// Vitest needs Vue's SFC compiler (some imported modules transitively
// touch .vue files). The alias mirrors vite.config.ts so `@/`
// resolves the same way in tests.
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  test: {
    // Tests live next to source. `api/` holds the Vercel serverless
    // functions; their tests live in `api/**/__tests__/*.test.ts`
    // and are picked up by the same vitest run.
    include: ['src/**/*.test.ts', 'api/**/*.test.ts'],
    environment: 'node',
    globals: false,
  },
});
