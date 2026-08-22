<script lang="ts">
    import { locale, setLocale } from "$lib/i18n";
</script>

<div class="locale-switch" role="group" aria-label="Language / Язык">
    <button
        type="button"
        class:active={$locale === "en"}
        aria-pressed={$locale === "en"}
        on:click={() => setLocale("en")}>EN</button
    >
    <span aria-hidden="true">/</span>
    <button
        type="button"
        class:active={$locale === "ru"}
        aria-pressed={$locale === "ru"}
        on:click={() => setLocale("ru")}>RU</button
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

    button {
        border: 0;
        padding: 0.3rem 0.1rem;
        background: transparent;
        color: inherit;
        cursor: pointer;
    }

    button:hover,
    button.active {
        color: var(--ink);
    }

    button.active {
        text-decoration: underline;
        text-decoration-color: var(--coral);
        text-decoration-thickness: 2px;
        text-underline-offset: 0.28rem;
    }
</style>
