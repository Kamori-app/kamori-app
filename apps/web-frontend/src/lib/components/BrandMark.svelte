<script lang="ts">
    export let size = 42;
    export let labelled = false;
</script>

<svg
    width={size}
    height={size}
    viewBox="0 0 48 48"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    role={labelled ? "img" : undefined}
    aria-label={labelled ? "Kamori" : undefined}
    aria-hidden={labelled ? undefined : "true"}
>
    <path
        d="M8 4h22l10 10v30H8V4Z"
        fill="#173F37"
    />
    <path d="M30 4v10h10L30 4Z" fill="#F1B95B" />
    <path d="M17 14v21" stroke="#F6F0E4" stroke-width="4.5" stroke-linecap="round" />
    <path d="m18 25 11-11" stroke="#F6F0E4" stroke-width="4.5" stroke-linecap="round" />
    <path d="m18 25 13 12" stroke="#E76F58" stroke-width="4.5" stroke-linecap="round" />
</svg>
