import { browser } from "$app/environment";
import { derived, writable } from "svelte/store";

export type AppLocale = "en" | "ru";

const STORAGE_KEY = "kamori.locale";

export function normalizeLocale(value: string | null | undefined): AppLocale {
    return value?.toLowerCase() === "ru" ? "ru" : "en";
}

const initialLocale = browser
    ? normalizeLocale(window.localStorage.getItem(STORAGE_KEY))
    : "en";

export const locale = writable<AppLocale>(initialLocale);

if (browser) {
    locale.subscribe((value) => {
        window.localStorage.setItem(STORAGE_KEY, value);
        document.documentElement.lang = value;
    });
}

export const isRussian = derived(locale, ($locale) => $locale === "ru");

export function setLocale(value: AppLocale): void {
    locale.set(value);
}
