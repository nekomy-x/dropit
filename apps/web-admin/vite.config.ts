import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Admin panel (C4). Same first-party cookie model as web-club: the dev proxy
// keeps the httpOnly session cookie same-origin, CSRF token comes from a
// meta tag rendered by the backend.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5174,
    proxy: {
      '/v1': {
        target: 'http://localhost:8080',
        changeOrigin: false,
      },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
  },
});
