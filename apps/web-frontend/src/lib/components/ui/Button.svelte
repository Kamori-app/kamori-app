<script lang="ts">
    /**
     * Shared button component with style variants used across landing/app views.
     */
    export let type: "button" | "submit" = "button";
    export let variant:
        | "primary"
        | "secondary"
        | "ghost"
        | "subtle"
        | "danger" = "primary";
    export let disabled = false;
    export let fullWidth = false;

    const variants = {
        primary: "bg-mint text-white hover:bg-emerald-700",
        secondary: "bg-slate text-white hover:bg-slate-800",
        ghost: "bg-white/70 text-slate border border-slate/20 hover:bg-white",
        subtle: "bg-slate/10 text-slate/75 border border-slate/15 hover:bg-slate/20",
        danger: "bg-coral text-white hover:bg-rose-600",
    };
</script>

<button
    {type}
    {disabled}
    on:click
    class={`inline-flex items-center justify-center border border-transparent px-4 py-2 text-sm font-semibold tracking-[0.01em] transition disabled:opacity-50 ${
        fullWidth ? "w-full" : ""
    } ${variants[variant]}`}
>
    <slot />
</button>
