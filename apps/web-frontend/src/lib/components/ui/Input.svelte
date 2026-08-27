<script lang="ts">
    import type { HTMLInputAttributes } from "svelte/elements";

    /**
     * Small styled input wrapper for consistent form controls.
     */
    export let type = "text";
    export let value = "";
    export let placeholder = "";
    export let required = false;
    export let disabled = false;
    export let autocomplete: HTMLInputAttributes["autocomplete"] = undefined;
    export let maxlength: number | undefined = undefined;
</script>

<input
    class="w-full border border-slate/25 bg-paper px-3 py-2 text-sm text-slate outline-none ring-mint/40 focus:ring disabled:opacity-70"
    {type}
    bind:value
    {placeholder}
    {required}
    {disabled}
    {autocomplete}
    {maxlength}
/>
