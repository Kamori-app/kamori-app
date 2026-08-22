<script lang="ts">
  export let open = false;
  export let title = '';
  export let onClose: () => void = () => {};
  import { locale } from '../../i18n';

  const onWindowKeydown = (event: KeyboardEvent) => {
    if (!open) {
      return;
    }
    if (event.key === 'Escape' || event.key === 'Esc') {
      event.preventDefault();
      onClose();
    }
  };

  const onBackdropClick = (event: MouseEvent) => {
    if (event.target === event.currentTarget) {
      onClose();
    }
  };

  const onBackdropKeydown = (event: KeyboardEvent) => {
    if (event.key === 'Escape') {
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
    <div class="w-full max-w-lg border border-slate/25 bg-paper p-5 shadow-[10px_10px_0_rgba(23,53,47,0.18)]">
      <div class="mb-4 flex items-center justify-between">
        <h3 class="font-heading text-lg font-semibold text-slate">{title}</h3>
        <button class="text-sm text-slate/60 hover:text-slate" on:click={onClose}>{$locale === 'ru' ? 'Закрыть' : 'Close'}</button>
      </div>
      <slot />
    </div>
  </div>
{/if}
