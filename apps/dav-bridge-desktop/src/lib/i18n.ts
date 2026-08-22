import { writable } from 'svelte/store';

export type AppLocale = 'en' | 'ru';

const storageKey = 'kamori.desktop.locale';

const initialLocale = (): AppLocale => {
  if (typeof localStorage === 'undefined') return 'en';
  return localStorage.getItem(storageKey) === 'ru' ? 'ru' : 'en';
};

export const locale = writable<AppLocale>(initialLocale());

export const setLocale = (next: AppLocale) => {
  locale.set(next);
  if (typeof localStorage !== 'undefined') localStorage.setItem(storageKey, next);
  if (typeof document !== 'undefined') document.documentElement.lang = next;
};

locale.subscribe((next) => {
  if (typeof document !== 'undefined') document.documentElement.lang = next;
});
