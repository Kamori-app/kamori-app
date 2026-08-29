<script lang="ts">
    import { locale } from "$lib/i18n";
    import { syncState } from "$lib/stores/sync";

    export let onSync: () => void = () => {};

    const relativeTime = (timestamp: number | null): string => {
        if (!timestamp) return $locale === "ru" ? "ещё не синхронизировано" : "not synced yet";
        const seconds = Math.max(0, Math.round((Date.now() - timestamp) / 1_000));
        if (seconds < 10) return $locale === "ru" ? "только что" : "just now";
        if (seconds < 60) return $locale === "ru" ? `${seconds} сек. назад` : `${seconds}s ago`;
        const minutes = Math.round(seconds / 60);
        return $locale === "ru" ? `${minutes} мин. назад` : `${minutes}m ago`;
    };

    $: label = $syncState.phase === "idle" && $syncState.lastSuccessAt === null
        ? ($locale === "ru" ? "Ожидает синхронизации" : "Waiting to sync")
        : $syncState.phase === "syncing"
        ? ($locale === "ru" ? "Синхронизация…" : "Syncing…")
        : $syncState.phase === "offline"
            ? ($locale === "ru" ? "Офлайн" : "Offline")
            : $syncState.phase === "error"
                ? ($locale === "ru" ? "Ошибка синхронизации" : "Sync failed")
                : `${$locale === "ru" ? "Синхронизировано" : "Synced"} ${relativeTime($syncState.lastSuccessAt)}`;
    $: compactLabel = $syncState.phase === "syncing"
        ? ($locale === "ru" ? "Синхр…" : "Syncing…")
        : $syncState.phase === "offline"
            ? ($locale === "ru" ? "Офлайн" : "Offline")
            : $syncState.phase === "error"
                ? ($locale === "ru" ? "Ошибка" : "Failed")
                : ($locale === "ru" ? "Синхр." : "Sync");
</script>

<button
    class="inline-flex min-h-9 shrink-0 items-center gap-1.5 border border-slate/20 bg-paper px-2 py-1.5 text-xs text-slate hover:bg-sand/40 sm:gap-2 sm:px-3"
    on:click={onSync}
    aria-label={$locale === "ru" ? "Синхронизировать сейчас" : "Sync now"}
    title={$syncState.error ?? label}
>
    <svg
        aria-hidden="true"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        class:animate-spin={$syncState.phase === "syncing"}
        class:text-moss={$syncState.phase === "idle"}
        class:text-gold={$syncState.phase === "syncing" || $syncState.phase === "offline"}
        class:text-coral={$syncState.phase === "error"}
        class="size-4 shrink-0"
    >
        <path d="M20 11a8.1 8.1 0 0 0-15.5-2M4 4v5h5" />
        <path d="M4 13a8.1 8.1 0 0 0 15.5 2M20 20v-5h-5" />
    </svg>
    <span class="sm:hidden">{compactLabel}</span>
    <span class="hidden sm:inline">{label}</span>
    {#if $syncState.pendingOperations > 0}
        <span class="rounded-full bg-sand px-1.5 py-0.5 font-semibold">
            {$syncState.pendingOperations > 99 ? "99+" : $syncState.pendingOperations}
        </span>
    {/if}
</button>
