import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import path from 'node:path';

export default defineConfig({
  // `wasm` + `topLevelAwait` let the compiler-wasm bundler-target
  // package (compiler-wasm/pkg/) load through a normal dynamic
  // `import()`. Without these Vite returns a 500 for the .wasm file
  // (ESM-integration proposal unsupported by default in Vite 5).
  plugins: [vue(), wasm(), topLevelAwait()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  // B.D c41 — the compiler Web Worker (src/compiler/worker.ts)
  // imports the same WASM module the main thread uses. Vite's
  // worker bundler doesn't inherit the main `plugins` array, so
  // we replay wasm + topLevelAwait inside the worker context too;
  // without this the worker import fails with the same
  // "ESM integration proposal" error as the main-thread case.
  worker: {
    format: 'es',
    plugins: () => [wasm(), topLevelAwait()],
  },
  server: {
    port: 5173,
    strictPort: false,
  },
  // Don't pre-bundle our WASM pkg — wasm-pack output already ships
  // ESM and the wasm plugin handles instantiation.
  optimizeDeps: {
    exclude: ['solflow_compiler_wasm'],
  },
});
