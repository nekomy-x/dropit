/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        // DROP IT club palette: dark stage + accent magenta (see dizayn-zadanie).
        brand: {
          50: '#fdf2f8',
          100: '#fce7f3',
          500: '#e11d74',
          600: '#c2185b',
          700: '#a01246',
        },
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', '-apple-system', 'sans-serif'],
      },
    },
  },
  plugins: [],
};
