<script lang="ts">
    /**
     * Reusable modal shell with:
     * - click-outside close
     * - keyboard close (`Esc`, `Enter`, `Space` on backdrop)
     */
    export let open = false;
    export let title = "";
    export let onClose: () => void = () => {};

    const onBackdropClick = (event: MouseEvent) => {
        if (event.target === event.currentTarget) {
            onClose();
        }
    };

    const onBackdropKeydown = (event: KeyboardEvent) => {
        if (event.target !== event.currentTarget) {
            return;
        }
        if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onClose();
        }
    };

    const onWindowKeydown = (event: KeyboardEvent) => {
        if (!open) {
            return;
        }
        if (event.key === "Escape" || event.key === "Esc") {
            event.preventDefault();
            onClose();
        }
    };
</script>

<svelte:window on:keydown|capture={onWindowKeydown} />

{#if open}
    <div
        class="fixed inset-0 z-50 grid place-items-center bg-slate/45 p-4"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabindex="0"
        on:click={onBackdropClick}
        on:keydown={onBackdropKeydown}
    >
        <div class="w-full max-w-lg rounded-2xl bg-white p-5 shadow-panel">
            <div class="mb-4 flex items-center justify-between">
                <h3 class="font-heading text-lg font-semibold text-slate">
                    {title}
                </h3>
                <button
                    class="text-sm text-slate/60 hover:text-slate"
                    on:click={onClose}>Close</button
                >
            </div>
            <slot />
        </div>
    </div>
{/if}
