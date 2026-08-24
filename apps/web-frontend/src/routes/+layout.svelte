<script lang="ts">
    /** Loads global app styles for all routes. */
    import "@fontsource-variable/ibm-plex-sans/wght.css";
    import "@fontsource/ibm-plex-serif/400.css";
    import "@fontsource/ibm-plex-serif/600.css";
    import "../app.css";
    import NotificationHost from "$lib/components/app/NotificationHost.svelte";
</script>

<slot />
<NotificationHost />
