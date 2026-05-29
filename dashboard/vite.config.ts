import { defineConfig } from 'vite'
import preact from '@preact/preset-vite'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [preact(), tailwindcss()],
  build: {
    outDir: '../crates/reestream-server/static',
    emptyOutDir: true,
  },
  optimizeDeps: {
    include: ['flv.js'],
  },
})
