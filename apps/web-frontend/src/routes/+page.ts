import type { AppLocale } from "$lib/i18n";

export const load = ({ url }: { url: URL }) => {
    const requested = url.searchParams.get("lang");
    const requestedLocale: AppLocale | null =
        requested === "en" || requested === "ru" ? requested : null;

    return { requestedLocale };
};
