import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const apiOrigin = new URL(
  process.env.VITE_KAMORI_API_BASE_URL ?? 'https://api.kamori.app',
).origin;

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({ precompress: true }),
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
  },
};

export default config;
