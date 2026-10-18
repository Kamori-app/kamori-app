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
      csp: {
        mode: 'auto',
        directives: {
          'default-src': ['self'],
          'connect-src': ['self', apiOrigin],
          'script-src': ['self'],
          'style-src': ['self', 'unsafe-inline'],
          'img-src': ['self', 'data:'],
          'font-src': ['self'],
          'frame-ancestors': ['none'],
          'base-uri': ['none'],
          'form-action': ['self'],
        },
      },
    })],
    server: { port: 4174, strictPort: true },
  };
});
