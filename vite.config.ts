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
