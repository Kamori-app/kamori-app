<script lang="ts">
  export let type = 'text';
  export let value = '';
  export let placeholder = '';
  export let required = false;
</script>

<input
  class="w-full rounded-xl border border-slate/20 bg-white px-3 py-2 text-sm text-slate outline-none ring-mint/40 focus:ring"
  {type}
  bind:value
  {placeholder}
  {required}
/>
