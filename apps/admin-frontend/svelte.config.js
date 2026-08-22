import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const apiOrigin = new URL(
  process.env.VITE_KAMORI_API_BASE_URL ?? 'https://api.kamori.app',
).origin;

export default {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({ precompress: true }),
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
  },
};
