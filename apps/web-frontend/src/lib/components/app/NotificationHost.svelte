<script lang="ts">
    import {
        dismissNotification,
        notificationStore,
    } from "$lib/stores/notifications";

    const tone = {
        success: "border-moss/40 bg-[#eef5ea]",
        info: "border-slate/30 bg-paper",
        warning: "border-gold/50 bg-[#fff8df]",
        error: "border-coral/50 bg-[#fff0eb]",
    } as const;
</script>

<div
    class="pointer-events-none fixed inset-x-3 top-3 z-[100] flex flex-col items-end gap-2 md:left-auto md:right-5 md:top-5 md:w-[24rem]"
    aria-live="polite"
    aria-relevant="additions"
>
    {#each $notificationStore as notification (notification.id)}
        <div
            class={`pointer-events-auto w-full border p-3 shadow-[5px_5px_0_rgba(23,63,55,0.14)] ${tone[notification.kind]}`}
            role={notification.kind === "error" ? "alert" : "status"}
        >
            <div class="flex items-start justify-between gap-3">
                <div class="min-w-0">
                    {#if notification.source}
                        <p class="text-[11px] font-semibold uppercase tracking-wide text-slate/55">
                            {notification.source}
                        </p>
                    {/if}
                    <p class="break-words text-sm text-slate">{notification.message}</p>
                    {#if notification.actionLabel && notification.onAction}
                        <button
                            class="mt-2 text-xs font-semibold text-slate underline underline-offset-2"
                            on:click={notification.onAction}
                        >{notification.actionLabel}</button>
                    {/if}
                </div>
                <button
                    class="shrink-0 text-lg leading-none text-slate/55 hover:text-slate"
                    aria-label="Dismiss notification"
                    on:click={() => dismissNotification(notification.id)}
                >×</button>
            </div>
        </div>
    {/each}
</div>
