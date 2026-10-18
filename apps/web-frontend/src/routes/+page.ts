import type { AppLocale } from "#lib/i18n.ts";

import type { PageLoad } from "./$types";

export const load: PageLoad = ({ url }) => {
    const requested = url.searchParams.get("lang");
    const requestedLocale: AppLocale | null =
        requested === "en" || requested === "ru" ? requested : null;

    return { requestedLocale };
};
