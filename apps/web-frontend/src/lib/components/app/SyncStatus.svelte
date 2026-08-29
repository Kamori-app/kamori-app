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
</script>

<button
    class="inline-flex min-h-9 items-center gap-2 border border-slate/20 bg-paper px-3 py-1.5 text-xs text-slate hover:bg-sand/40"
    on:click={onSync}
    aria-label={$locale === "ru" ? "Синхронизировать сейчас" : "Sync now"}
    title={$syncState.error ?? label}
>
    <span
        class:bg-moss={$syncState.phase === "idle"}
        class:bg-gold={$syncState.phase === "syncing" || $syncState.phase === "offline"}
        class:bg-coral={$syncState.phase === "error"}
        class="h-2 w-2 rounded-full"
    ></span>
    <span class="hidden sm:inline">{label}</span>
    {#if $syncState.pendingOperations > 0}
        <span class="rounded-full bg-sand px-1.5 py-0.5 font-semibold">
            {$syncState.pendingOperations}
        </span>
    {/if}
</button>
