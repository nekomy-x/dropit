/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        // Authenticated product surfaces use green for actions; Connect keeps its own accent.
        // Exactly one green (§4.5): the accent, its hover, and alpha tints of
        // the same hue. The old 50/100/300/400/700 shades were extra greens and
        // are gone; the class names stay so existing call sites keep resolving.
        brand: {
          100: 'rgba(0, 255, 135, 0.82)',
          300: '#00FF87',
          400: '#00FF87',
          500: '#00FF87',
          600: '#00D873',
        },
        // "Connect" flow palette — graphite surfaces with the DROP IT green accent.
        drop: {
          bg: '#000000',
          accent: '#00FF87',
          text: '#ECECEC',
          muted: '#B4B4B4',
          dim: '#777777',
          surface: '#191919',
          surface2: '#242424',
          rail: '#080808',
          ink: '#080808',
          line: '#FFFFFF18',
          tint: '#00FF8714',
        },
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', '-apple-system', 'BlinkMacSystemFont', '"Segoe UI"', 'sans-serif'],
        display: ['system-ui', '-apple-system', 'BlinkMacSystemFont', '"Segoe UI"', 'sans-serif'],
        mono: ['ui-monospace', 'SFMono-Regular', 'Menlo', 'monospace'],
      },
      // One radius scale for the whole club panel: 16 surfaces, 12 tiles,
      // 10 controls, 999 chips. Legacy 5/6/7/8px utilities are remapped here so
      // no `rounded-md`/`rounded-lg` can reintroduce an off-scale corner.
      borderRadius: {
        none: '0',
        sm: '10px',
        DEFAULT: '10px',
        md: '10px',
        lg: '12px',
        xl: '12px',
        '2xl': '16px',
        '3xl': '16px',
        full: '999px',
        card: '16px',
        field: '10px',
      },
      keyframes: {
        marquee: {
          '0%': { transform: 'translate3d(0, 0, 0)' },
          '100%': { transform: 'translate3d(-50%, 0, 0)' },
        },
        'reveal-up': {
          '0%': { opacity: '0', transform: 'translate3d(0, 18px, 0)' },
          '100%': { opacity: '1', transform: 'translate3d(0, 0, 0)' },
        },
        'fade-in': {
          '0%': { opacity: '0' },
          '100%': { opacity: '1' },
        },
        'step-in': {
          '0%': { opacity: '0', transform: 'translate3d(24px, 0, 0)' },
          '100%': { opacity: '1', transform: 'translate3d(0, 0, 0)' },
        },
        'pop-in': {
          '0%': { opacity: '0', transform: 'scale(0.82)' },
          '60%': { opacity: '1', transform: 'scale(1.06)' },
          '100%': { opacity: '1', transform: 'scale(1)' },
        },
        'pulse-dot': {
          '0%, 100%': { opacity: '1', transform: 'scale(1)' },
          '50%': { opacity: '0.45', transform: 'scale(0.82)' },
        },
        float: {
          '0%, 100%': { transform: 'translate3d(0, -6px, 0)' },
          '50%': { transform: 'translate3d(0, 8px, 0)' },
        },
        'check-draw': {
          '0%': { strokeDashoffset: '32' },
          '100%': { strokeDashoffset: '0' },
        },
        'ring-out': {
          '0%': { opacity: '0.55', transform: 'scale(0.6)' },
          '100%': { opacity: '0', transform: 'scale(2.1)' },
        },
        'bar-grow': {
          '0%': { transform: 'scaleY(0.02)' },
          '100%': { transform: 'scaleY(1)' },
        },
      },
      animation: {
        marquee: 'marquee 34s linear infinite',
        'reveal-up': 'reveal-up 620ms cubic-bezier(0.22, 1, 0.36, 1) both',
        'fade-in': 'fade-in 480ms ease-out both',
        'step-in': 'step-in 380ms cubic-bezier(0.22, 1, 0.36, 1) both',
        'pop-in': 'pop-in 420ms cubic-bezier(0.22, 1, 0.36, 1) both',
        'pulse-dot': 'pulse-dot 1.8s ease-in-out infinite',
        float: 'float 7s ease-in-out infinite',
        'check-draw': 'check-draw 520ms ease-out 160ms forwards',
        'ring-out': 'ring-out 900ms ease-out infinite',
        'bar-grow': 'bar-grow 700ms cubic-bezier(0.22, 1, 0.36, 1) both',
      },
    },
  },
  plugins: [],
};