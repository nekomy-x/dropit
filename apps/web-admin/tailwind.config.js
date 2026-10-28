/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        // Every colour resolves through the panel token layer, so existing
        // utility classes flip with `data-theme` instead of pinning dark
        // values. docs/ui/PANEL-DESIGN.md is the contract.
        brand: {
          50: 'var(--c-accent-tint)',
          100: 'var(--c-accent-tint-strong)',
          300: 'var(--c-accent)',
          400: 'var(--c-accent)',
          500: 'var(--c-accent)',
          600: 'var(--c-accent-hover)',
          700: 'var(--c-accent-hover)',
        },
        // Legacy neutral scale used across the admin pages before the redesign.
        // Aliased onto the token layer so old markup is themed, not invisible.
        neutral: {
          50: 'var(--c-text)',
          100: 'var(--c-text)',
          200: 'var(--c-text)',
          300: 'var(--c-text-2)',
          400: 'var(--c-text-2)',
          500: 'var(--c-text-muted)',
          600: 'var(--c-text-muted-sm)',
          700: 'var(--c-hairline-strong)',
          800: 'var(--c-hairline)',
          900: 'var(--c-raised)',
          950: 'var(--c-canvas)',
        },
        ink: 'var(--c-text)',
        surface: 'var(--c-surface)',
        raised: 'var(--c-raised)',
        inset: 'var(--c-inset)',
        hairline: 'var(--c-hairline)',
        muted: 'var(--c-text-muted)',
        secondary: 'var(--c-text-2)',
        canvas: 'var(--c-canvas)',
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', '-apple-system', 'BlinkMacSystemFont', '"Segoe UI"', 'sans-serif'],
        mono: ['ui-monospace', 'SFMono-Regular', 'Menlo', 'monospace'],
      },
      borderRadius: {
        sm: '10px',
        DEFAULT: '10px',
        md: '10px',
        lg: '12px',
        xl: '12px',
        '2xl': '16px',
        '3xl': '16px',
        card: '16px',
        field: '10px',
      },
      boxShadow: {
        card: 'var(--c-panel-shadow)',
      },
    },
  },
  plugins: [],
};
