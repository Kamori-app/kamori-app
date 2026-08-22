import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

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
          'https://api.kamori.app',
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
