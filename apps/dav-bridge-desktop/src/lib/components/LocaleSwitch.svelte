<script lang="ts">
  import { locale, setLocale, type AppLocale } from '../i18n';
</script>

<div class="inline-flex border border-slate/20 bg-paper" aria-label="Language">
  {#each ['en', 'ru'] as option}
    <button
      type="button"
      class={`px-2.5 py-1.5 text-[11px] font-semibold tracking-[0.12em] transition ${$locale === option ? 'bg-slate text-white' : 'text-slate hover:bg-sand/50'}`}
      aria-pressed={$locale === option}
      on:click={() => setLocale(option as AppLocale)}
    >{option.toUpperCase()}</button>
  {/each}
</div>
