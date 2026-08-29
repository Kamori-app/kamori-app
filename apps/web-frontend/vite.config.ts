import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  // rrule publishes both ESM and CommonJS entry points without an exports map.
  // Bundle it for SSR so Node never resolves the CommonJS entry as native ESM.
  ssr: {
    noExternal: ['rrule'],
  },
  server: {
    port: 4173,
    strictPort: true,
  },
});
