import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

export default {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({ precompress: true }),
    csp: {
      mode: 'auto',
      directives: {
        'default-src': ['self'],
        'connect-src': ['self', 'https://api.kamori.app'],
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
