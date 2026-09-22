import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'node:path'

export default defineConfig({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: { '@': path.resolve(__dirname, './src') },
  },

  server: {
    port: 5180,
    strictPort: true,
    // The node serves this bundle in production, so the app always calls
    // same-origin `/api/...`. In development the proxy stands in for that,
    // which keeps the client code free of environment branching.
    proxy: {
      '/api': {
        target: process.env.GIAMS_NODE_URL ?? 'http://127.0.0.1:8787',
        changeOrigin: true,
      },
    },
  },

  build: {
    // Served by giams-node and bundled into the Tauri installer.
    outDir: 'dist',
    sourcemap: true,
  },
})
