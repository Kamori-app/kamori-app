<script lang="ts">
    import { page } from "$app/stores";
    import { replaceState } from "$app/navigation";
    import { locale, setLocale, type AppLocale } from "$lib/i18n";

    export let value: AppLocale | undefined = undefined;
    export let onSelect: ((value: AppLocale) => void) | undefined = undefined;

    $: selected = value ?? $locale;

    const hrefFor = (next: AppLocale): string => {
        const url = new URL($page.url);
        url.searchParams.set("lang", next);
        return `${url.pathname}${url.search}${url.hash}`;
    };

    const select = (event: MouseEvent, next: AppLocale) => {
        event.preventDefault();
        if (onSelect) {
            onSelect(next);
        } else {
            setLocale(next);
            const url = new URL(window.location.href);
            url.searchParams.set("lang", next);
            replaceState(
                `${url.pathname}${url.search}${url.hash}`,
                {},
            );
        }
    };
</script>

<div class="locale-switch" role="group" aria-label="Language / Язык">
    <a
        href={hrefFor("en")}
        class:active={selected === "en"}
        aria-current={selected === "en" ? "true" : undefined}
        on:click={(event) => select(event, "en")}>EN</a
    >
    <span aria-hidden="true">/</span>
    <a
        href={hrefFor("ru")}
        class:active={selected === "ru"}
        aria-current={selected === "ru" ? "true" : undefined}
        on:click={(event) => select(event, "ru")}>RU</a
    >
</div>

<style>
    .locale-switch {
        display: inline-flex;
        align-items: center;
        gap: 0.28rem;
        color: color-mix(in srgb, var(--ink) 45%, transparent);
        font-family: var(--font-sans);
        font-size: 0.72rem;
        font-weight: 650;
        letter-spacing: 0.08em;
    }

    a {
        padding: 0.3rem 0.1rem;
        color: inherit;
        cursor: pointer;
        text-decoration: none;
    }

    a:hover,
    a.active {
        color: var(--ink);
    }

    a.active {
        text-decoration: underline;
        text-decoration-color: var(--coral);
        text-decoration-thickness: 2px;
        text-underline-offset: 0.28rem;
    }
</style>
