<script lang="ts">
  export let type: 'button' | 'submit' = 'button';
  export let variant: 'primary' | 'secondary' | 'ghost' = 'primary';
  export let disabled = false;

  const variants = {
    primary: 'bg-mint text-white hover:bg-emerald-700',
    secondary: 'bg-slate text-white hover:bg-slate-800',
    ghost: 'bg-white/70 text-slate border border-slate/20 hover:bg-white',
  };
</script>

<button
  {type}
  {disabled}
  on:click
  class={`border border-transparent px-4 py-2 text-sm font-semibold transition disabled:opacity-50 ${variants[variant]}`}
>
  <slot />
</button>
