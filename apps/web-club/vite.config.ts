import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Club panel (C3). API base URL is environment-specific; local dev uses the
// Vite proxy so the httpOnly session cookie stays first-party (same origin),
// which also keeps the CSRF meta-tag flow simple.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/v1': {
        target: 'http://localhost:8081',
        changeOrigin: false,
      },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
  },
});
