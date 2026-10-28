import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { loadEnv } from 'vite';

// Club panel (C3). API base URL is environment-specific; local dev uses the
// Vite proxy so the httpOnly session cookie stays first-party (same origin),
// which also keeps the CSRF meta-tag flow simple.
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, '.', '');
  const isDemo = mode === 'demo' || env.VITE_APP_MODE === 'demo';
  return {
  define: {
    __APP_MODE__: JSON.stringify(isDemo ? 'demo' : 'production'),
  },

  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/v1/status': {
        target: 'http://localhost:8081',
        changeOrigin: false,
      },
      '/v1': {
        target: 'http://localhost:8081',
        changeOrigin: false,
      },
      // The test-stand badge reads the server's own answer (open signup /
      // mock payments) instead of guessing from the build mode; keep the probe
      // first-party in dev too.
      '/readyz': {
        target: 'http://localhost:8081',
        changeOrigin: false,
      },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
  },
  };
});
