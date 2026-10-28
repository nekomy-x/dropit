import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { loadEnv } from 'vite';

// Admin panel (C4). Same first-party cookie model as web-club: the dev proxy
// keeps the httpOnly session cookie same-origin, CSRF token comes from a
// meta tag rendered by the backend.
//
// Deployment: the panel is served from the same origin as the club panel,
// under http://HOST/admin/ (nginx location /admin/ → dist/). `base` emits
// every asset under /admin/assets/..., and BrowserRouter uses the matching
// basename so SPA routes stay under /admin/. API calls are absolute-path
// (/v1/...), so they still hit the origin root, which nginx proxies to the
// Rust gateway.
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, '.', '');
  const isDemo = mode === 'demo' || env.VITE_APP_MODE === 'demo';
  return {
  define: { __APP_MODE__: JSON.stringify(isDemo ? 'demo' : 'production') },

  base: '/admin/',

  plugins: [react()],
  server: {
    port: 5174,
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
