<script lang="ts">
    import { onMount } from "svelte";
    import { cloudApi } from "$lib/api/cloud";
    import { bestEffortLogoutWithRefresh } from "$lib/auth/session-flow.js";
    import { tokenStore } from "$lib/auth/tokenStore";
    import { appState } from "$lib/stores/app";
    import {
        lockWebVault,
        unlockWebVaultFromLocalPasskey,
    } from "$lib/cryptoVault";
    import Badge from "$lib/components/ui/Badge.svelte";
    import Button from "$lib/components/ui/Button.svelte";
    import Card from "$lib/components/ui/Card.svelte";
    import AppWorkspace from "$lib/components/app/AppWorkspace.svelte";
    import SettingsModal from "$lib/components/app/SettingsModal.svelte";
    import SignInModal from "$lib/components/app/SignInModal.svelte";
    import SignUpModal from "$lib/components/app/SignUpModal.svelte";

    /**
     * Web app shell:
     * - top navigation and auth gate
     * - settings/sign-in/sign-up modal orchestration
     * - workspace mount for authenticated users
     */
    let settingsOpen = false;
    let signInOpen = false;
    let signUpOpen = false;

    const openSettings = () => {
        settingsOpen = true;
    };

    const openSignIn = () => {
        signUpOpen = false;
        signInOpen = true;
    };

    const openSignUp = () => {
        signInOpen = false;
        signUpOpen = true;
    };

    /**
     * Clears session-related state from local store.
     */
    const logout = async () => {
        await bestEffortLogoutWithRefresh({
            accessToken: tokenStore.getAccessToken(),
            logout: (accessToken: string) =>
                cloudApi.logout($appState.cloudBaseUrl, accessToken),
            refresh: () => cloudApi.refresh($appState.cloudBaseUrl),
            onAccessTokenRotated: (accessToken: string) =>
                tokenStore.setAccessToken(accessToken),
        });
        tokenStore.clear();
        lockWebVault();
        appState.update((state) => ({
            ...state,
            currentUsername: "",
            accessToken: null,
            preauthToken: null,
            notice: "Logged out.",
        }));
    };

    onMount(() => {
        // Attempt silent restore from refresh cookie on page load.
        if (!$appState.accessToken) {
            void (async () => {
                try {
                    const rotated = await cloudApi.refresh(
                        $appState.cloudBaseUrl,
                    );
                    const username = $appState.currentUsername.trim();
                    const unlocked = username
                        ? await unlockWebVaultFromLocalPasskey(username)
                        : null;
                    if (!unlocked) {
                        appState.update((state) => ({
                            ...state,
                            accessToken: null,
                            notice:
                                "Your server session is available, but sign in again to unlock encrypted browser data.",
                        }));
                        return;
                    }
                    tokenStore.setAccessToken(rotated.access_token);
                    appState.update((state) => ({
                        ...state,
                        accessToken: rotated.access_token,
                        notice: state.notice || "Session restored.",
                    }));
                } catch {
                    // no active cookie-backed session
                }
            })();
        }

        // `?start=signup` enables deep-link onboarding from landing CTA.
        const url = new URL(window.location.href);
        if (url.searchParams.get("start") === "signup") {
            if (!$appState.accessToken) {
                signUpOpen = true;
                appState.update((state) => ({
                    ...state,
                    notice: "Get started: 1) Create account, 2) Install bridge app on your device, 3) Connect your DAV client to the local endpoint.",
                }));
            }
            // Treat onboarding deep links as one-shot commands. Keeping the
            // parameter would reopen Sign Up after authentication or reload.
            url.searchParams.delete("start");
            window.history.replaceState(
                window.history.state,
                "",
                `${url.pathname}${url.search}${url.hash}`,
            );
        }
    });
</script>

<main class="min-h-screen px-4 py-6 md:px-8">
    <button
        class="fixed right-4 top-4 z-40 inline-flex h-11 w-11 items-center justify-center rounded-full border border-white/70 bg-white/85 text-slate shadow-panel transition hover:bg-white"
        on:click={openSettings}
        aria-label="Open web settings"
        title="Web settings"
    >
        <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 24 24"
            fill="none"
            class="h-5 w-5"
            stroke="currentColor"
            stroke-width="1.9"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <path d="M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4Z" />
            <path
                d="m19.4 15-.3.6a1 1 0 0 0 .2 1.1l.1.1a1 1 0 0 1 0 1.4l-1.3 1.3a1 1 0 0 1-1.4 0l-.1-.1a1 1 0 0 0-1.1-.2l-.6.3a1 1 0 0 0-.6.9V21a1 1 0 0 1-1 1h-1.8a1 1 0 0 1-1-1v-.2a1 1 0 0 0-.6-.9l-.6-.3a1 1 0 0 0-1.1.2l-.1.1a1 1 0 0 1-1.4 0L4.6 18a1 1 0 0 1 0-1.4l.1-.1a1 1 0 0 0 .2-1.1l-.3-.6a1 1 0 0 0-.9-.6H3.5a1 1 0 0 1-1-1v-1.8a1 1 0 0 1 1-1h.2a1 1 0 0 0 .9-.6l.3-.6a1 1 0 0 0-.2-1.1l-.1-.1a1 1 0 0 1 0-1.4L5.9 4a1 1 0 0 1 1.4 0l.1.1a1 1 0 0 0 1.1.2l.6-.3a1 1 0 0 0 .6-.9V3a1 1 0 0 1 1-1h1.8a1 1 0 0 1 1 1v.2a1 1 0 0 0 .6.9l.6.3a1 1 0 0 0 1.1-.2l.1-.1a1 1 0 0 1 1.4 0l1.3 1.3a1 1 0 0 1 0 1.4l-.1.1a1 1 0 0 0-.2 1.1l.3.6a1 1 0 0 0 .9.6h.2a1 1 0 0 1 1 1v1.8a1 1 0 0 1-1 1h-.2a1 1 0 0 0-.9.6Z"
            />
        </svg>
    </button>

    <div class="mx-auto max-w-6xl animate-fade-slide">
        <nav
            class="mb-6 flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-white/70 bg-white/70 p-3 shadow-panel"
        >
            <div class="flex items-center gap-2">
                <a
                    class="rounded-lg bg-slate px-3 py-2 text-sm text-white"
                    href="/app">Web App</a
                >
                <a class="rounded-lg px-3 py-2 text-sm text-slate" href="/"
                    >Landing</a
                >
            </div>

            <div class="flex items-center gap-2">
                <Badge active={Boolean($appState.accessToken)}>
                    {$appState.accessToken ? "Authenticated" : "Not Signed In"}
                </Badge>
                {#if $appState.accessToken}
                    <Button variant="ghost" on:click={logout}>Log Out</Button>
                {/if}
            </div>
        </nav>

        {#if !$appState.accessToken}
            <div class="mx-auto mt-14 max-w-lg">
                <Card>
                    <h2
                        class="text-center font-heading text-xl font-semibold text-slate"
                    >
                        Authentication Required
                    </h2>
                    <p class="mt-2 text-center text-sm text-slate/80">
                        Open one of the dialogs to sign in or create a new
                        account.
                    </p>
                    <div class="mt-4 flex flex-wrap justify-center gap-2">
                        <Button variant="secondary" on:click={openSignIn}
                            >Sign In</Button
                        >
                        <Button variant="ghost" on:click={openSignUp}
                            >Sign Up</Button
                        >
                    </div>
                </Card>
            </div>
        {:else}
            <AppWorkspace />
        {/if}

        {#if $appState.notice}
            <p class="mt-4 rounded-xl bg-sand/50 p-3 text-sm text-slate">
                {$appState.notice}
            </p>
        {/if}
    </div>
</main>

<SettingsModal open={settingsOpen} onClose={() => (settingsOpen = false)} />
<SignInModal open={signInOpen} onClose={() => (signInOpen = false)} />
<SignUpModal
    open={signUpOpen}
    onClose={() => (signUpOpen = false)}
    onOpenSignIn={openSignIn}
/>
