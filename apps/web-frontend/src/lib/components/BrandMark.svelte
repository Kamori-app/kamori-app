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
        d="M8 8.5h13.5c10.2 0 18.5 8.3 18.5 18.5v12.5H26.5C16.3 39.5 8 31.2 8 21V8.5Z"
        fill="#173F37"
    />
    <path
        d="M13.5 13.5h8.2c7.4 0 13.3 6 13.3 13.3v7.7h-8.2c-7.4 0-13.3-6-13.3-13.3v-7.7Z"
        fill="#F1B95B"
    />
    <path d="M13.5 34.5 34.8 13.2" stroke="#F6F0E4" stroke-width="4.5" />
    <path d="m23 25 12 12" stroke="#E76F58" stroke-width="4.5" />
</svg>
