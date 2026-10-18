import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, loadEnv } from 'vite';

export default defineConfig(({ mode }) => {
  const env = { ...loadEnv(mode, import.meta.dirname, ''), ...process.env };
  const apiOrigin = new URL(env.VITE_KAMORI_API_BASE_URL ?? 'https://api.kamori.app').origin;
  return {
    plugins: [sveltekit({
      preprocess: vitePreprocess(),
      adapter: adapter({ precompress: true }),
      version: { pollInterval: 0 },
      // The cached /app shell also serves uncached deep links while offline.
      // Its assets and worker URL must not resolve relative to that new URL.
      paths: { relative: false },
      csp: {
        mode: 'auto',
        directives: {
          'default-src': ['self'],
          'connect-src': [
            'self',
            apiOrigin,
            'https://api.github.com',
          ],
          'script-src': ['self', 'wasm-unsafe-eval'],
          'style-src': ['self', 'unsafe-inline'],
          'img-src': ['self', 'data:'],
          'font-src': ['self'],
          'frame-ancestors': ['none'],
          'base-uri': ['self'],
          'form-action': ['self'],
        },
      },
    })],
    ssr: { noExternal: ['rrule'] },
    server: { port: 4173, strictPort: true },
  };
});
