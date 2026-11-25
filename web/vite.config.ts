import { fileURLToPath } from 'node:url'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// The kit package is served from wherever a host puts it, so every asset path is relative.
export default defineConfig({
  base: './',
  plugins: [react()],
  resolve: {
    // One React for the page (also when KITION_LINK=1 links the kition-sdk and Tessera checkouts).
    dedupe: ['react', 'react-dom'],
  },
  // @kition/sdk stays unbundled, so the editor's engine worker is found next to it; the CommonJS
  // module its components import is bundled for the page.
  optimizeDeps: { exclude: ['@kition/sdk'], include: ['@kition/sdk > use-sync-external-store/shim'] },
  worker: { format: 'es' },
  server: {
    port: 5180,
    fs: { allow: [fileURLToPath(new URL('..', import.meta.url)), fileURLToPath(new URL('../../kition-sdk', import.meta.url)), fileURLToPath(new URL('../../kition-tessera', import.meta.url))] },
  },
  build: {
    outDir: 'dist',
    rollupOptions: { input: { index: fileURLToPath(new URL('index.html', import.meta.url)), dev: fileURLToPath(new URL('dev.html', import.meta.url)) } },
  },
})
