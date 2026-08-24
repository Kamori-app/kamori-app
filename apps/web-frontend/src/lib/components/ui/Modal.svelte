<script lang="ts">
    /**
     * Reusable modal shell with:
     * - click-outside close
     * - keyboard close (`Esc`, `Enter`, `Space` on backdrop)
     */
    export let open = false;
    export let title = "";
    export let onClose: () => void = () => {};
    export let embedded = false;
    import { onDestroy } from "svelte";
    import { locale } from "$lib/i18n";

    let bodyOverflowBeforeOpen: string | null = null;

    const lockPageScroll = () => {
        if (typeof document === "undefined" || bodyOverflowBeforeOpen !== null) {
            return;
        }
        bodyOverflowBeforeOpen = document.body.style.overflow;
        document.body.style.overflow = "hidden";
    };

    const unlockPageScroll = () => {
        if (typeof document === "undefined" || bodyOverflowBeforeOpen === null) {
            return;
        }
        document.body.style.overflow = bodyOverflowBeforeOpen;
        bodyOverflowBeforeOpen = null;
    };

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

    $: if (open && !embedded) {
        lockPageScroll();
    } else {
        unlockPageScroll();
    }

    onDestroy(unlockPageScroll);
</script>

<svelte:window on:keydown|capture={onWindowKeydown} />

{#if open && embedded}
    <section class="border border-slate/20 bg-paper p-4 shadow-[6px_6px_0_rgba(23,63,55,0.10)] md:p-6">
        <h1 class="mb-5 font-heading text-2xl font-semibold text-slate">{title}</h1>
        <slot />
    </section>
{:else if open}
    <div
        class="modal-backdrop fixed inset-0 z-50 grid place-items-center overflow-y-auto overscroll-contain bg-slate/45"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabindex="0"
        on:click={onBackdropClick}
        on:keydown={onBackdropKeydown}
    >
        <div class="modal-panel flex w-full max-w-lg flex-col border border-slate/25 bg-paper p-5 shadow-[10px_10px_0_rgba(23,63,55,0.18)]">
            <div class="mb-4 flex shrink-0 items-center justify-between">
                <h3 class="font-heading text-lg font-semibold text-slate">
                    {title}
                </h3>
                <button
                    class="text-sm text-slate/60 hover:text-slate"
                    on:click={onClose}>{$locale === "ru" ? "Закрыть" : "Close"}</button
                >
            </div>
            <div class="min-h-0 overflow-y-auto overscroll-contain">
                <slot />
            </div>
        </div>
    </div>
{/if}

<style>
    .modal-backdrop {
        padding: max(1rem, env(safe-area-inset-top))
            max(1rem, env(safe-area-inset-right))
            max(1rem, env(safe-area-inset-bottom))
            max(1rem, env(safe-area-inset-left));
    }

    .modal-panel {
        max-height: calc(100vh - 2rem);
        max-height: calc(
            100dvh - max(1rem, env(safe-area-inset-top)) -
                max(1rem, env(safe-area-inset-bottom))
        );
    }
</style>
