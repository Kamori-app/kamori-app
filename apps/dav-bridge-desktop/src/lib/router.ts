import { writable } from 'svelte/store';

export type Route = '/login' | '/dashboard' | '/collections';

const normalize = (path: string): Route => {
  if (path === '/dashboard' || path === '/collections' || path === '/login') {
    return path;
  }
  return '/login';
};

export const route = writable<Route>(normalize(window.location.pathname));

export const navigate = (next: Route) => {
  window.history.pushState({}, '', next);
  route.set(next);
};

window.addEventListener('popstate', () => {
  route.set(normalize(window.location.pathname));
});
