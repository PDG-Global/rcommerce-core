/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{astro,html,js,jsx,ts,tsx}'],
  theme: {
    extend: {
      fontFamily: {
        sans: ['"Work Sans"', 'system-ui', 'sans-serif'],
        serif: ['"Source Serif 4"', 'Georgia', 'serif'],
        mono: ['"JetBrains Mono"', 'monospace'],
      },
      colors: {
        rcommerce: {
          50: '#faf8f5',
          100: '#f0ede7',
          200: '#e5e2dc',
          300: '#c9c5bc',
          400: '#99958c',
          500: '#77736a',
          600: '#55524b',
          700: '#3a3833',
          800: '#232220',
          900: '#161616',
          // Brand palette — set per store via build-time env vars (all
          // optional; fallbacks are the generic demo-store values).
          primary: process.env.PUBLIC_BRAND_PRIMARY || '#FF5FA2',
          accent: process.env.PUBLIC_BRAND_ACCENT || '#ff4d00',
          'accent-dark': '#e64500',
          highlight: process.env.PUBLIC_BRAND_HIGHLIGHT || '#f4da40',
          success: '#10b981',
          danger: '#ef4444',
        },
      },
      letterSpacing: {
        button: '0.14em',
      },
    },
  },
  plugins: [],
};
