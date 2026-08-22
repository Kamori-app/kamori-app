import type { Config } from 'tailwindcss';

const config: Config = {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {
      fontFamily: {
        heading: ['IBM Plex Serif', 'serif'],
        body: ['IBM Plex Sans Variable', 'IBM Plex Sans', 'sans-serif'],
      },
      colors: {
        surface: '#f4f7f5',
        slate: '#1f2a31',
        mint: '#0f8b6d',
        coral: '#f45b69',
        sand: '#ffd6a5',
        paper: '#f3eee2',
      },
      boxShadow: {
        panel: '0 24px 60px -32px rgba(31, 42, 49, 0.45)',
      },
    },
  },
  plugins: [],
};

export default config;
