import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Порт и адрес должны совпадать с `build.devUrl` в tauri.conf.json,
// иначе окно не найдёт страницу в режиме разработки.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: 'localhost',
    watch: {
      // Следим только за исходниками: пересборка окна не нужна.
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    // Путь указывает на `build.frontendDist` в tauri.conf.json.
    outDir: '../dist',
    emptyOutDir: true,
    target: 'es2021',
    sourcemap: false,
  },
})
