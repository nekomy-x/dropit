/// <reference types="vite/client" />

// Vite injects `import.meta.env.BASE_URL` from `base` in vite.config.ts.
// The admin panel uses it to derive the BrowserRouter basename so the same
// bundle works at /admin/ (production) and / (plain `vite dev`).
