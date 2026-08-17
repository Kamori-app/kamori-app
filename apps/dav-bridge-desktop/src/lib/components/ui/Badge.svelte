<script lang="ts">
  export let active = false;
</script>

<span class={`inline-flex rounded-full px-3 py-1 text-xs font-bold ${active ? 'bg-mint text-white' : 'bg-slate/10 text-slate'}`}>
  <slot />
</span>
