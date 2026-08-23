<script lang="ts">
    import { onMount } from "svelte";
    import { cloudApi } from "$lib/api/cloud";
    import { bestEffortLogoutWithRefresh } from "$lib/auth/session-flow.js";
    import { tokenStore } from "$lib/auth/tokenStore";
    import { appState } from "$lib/stores/app";
    import {
        lockWebVault,
        unlockWebVaultFromLocalUnlock,
    } from "$lib/cryptoVault";
    import Badge from "$lib/components/ui/Badge.svelte";
    import Button from "$lib/components/ui/Button.svelte";
    import Card from "$lib/components/ui/Card.svelte";
    import AppWorkspace from "$lib/components/app/AppWorkspace.svelte";
    import SettingsModal from "$lib/components/app/SettingsModal.svelte";
    import SignInModal from "$lib/components/app/SignInModal.svelte";
    import SignUpModal from "$lib/components/app/SignUpModal.svelte";
    import BrandMark from "$lib/components/BrandMark.svelte";
    import LocaleSwitch from "$lib/components/LocaleSwitch.svelte";
    import { locale } from "$lib/i18n";

    const shellCopy = {
        en: {
            settings: "Web settings",
            webApp: "Web app",
            home: "Home",
            authenticated: "Authenticated",
            signedOut: "Not signed in",
            logout: "Log out",
            authRequired: "Authentication required",
            authBody: "Sign in to unlock encrypted browser data, or create a new account here on the web.",
            signIn: "Sign in",
            signUp: "Create account",
            loggedOut: "Logged out.",
            lockedSession: "Your server session is available, but sign in again to unlock encrypted browser data.",
            restored: "Session restored.",
            onboarding: "Create your account, save the recovery kit, then sign in on your other devices.",
        },
        ru: {
            settings: "Настройки веб-приложения",
            webApp: "Веб-приложение",
            home: "Главная",
            authenticated: "Вход выполнен",
            signedOut: "Вход не выполнен",
            logout: "Выйти",
            authRequired: "Нужно войти",
            authBody: "Войдите, чтобы открыть зашифрованные данные в браузере, или создайте новый аккаунт в вебе.",
            signIn: "Войти",
            signUp: "Создать аккаунт",
            loggedOut: "Вы вышли из аккаунта.",
            lockedSession: "Серверная сессия доступна, но для расшифровки данных браузера нужно войти ещё раз.",
            restored: "Сессия восстановлена.",
            onboarding: "Создайте аккаунт, сохраните recovery kit, затем войдите на других устройствах.",
        },
    } as const;

    $: copy = shellCopy[$locale];

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
            totpContinuationToken: null,
            notice: copy.loggedOut,
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
                    const username = rotated.username.trim();
                    const unlocked = username
                        ? await unlockWebVaultFromLocalUnlock(
                              $appState.cloudBaseUrl,
                              username,
                          )
                        : null;
                    if (!unlocked) {
                        appState.update((state) => ({
                            ...state,
                            currentUsername: username,
                            accessToken: null,
                            notice: copy.lockedSession,
                        }));
                        return;
                    }
                    tokenStore.setAccessToken(rotated.access_token);
                    appState.update((state) => ({
                        ...state,
                        currentUsername: username,
                        accessToken: rotated.access_token,
                        notice: state.notice || copy.restored,
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
                    notice: copy.onboarding,
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
        class="fixed right-4 top-4 z-40 inline-flex h-11 w-11 items-center justify-center border border-slate/25 bg-paper text-slate transition hover:bg-sand/50"
        on:click={openSettings}
        aria-label={copy.settings}
        title={copy.settings}
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
            class="mb-6 flex flex-wrap items-center justify-between gap-3 border-y border-slate/20 bg-paper/90 px-1 py-3"
        >
            <div class="flex items-center gap-2">
                <a href="/" class="mr-1 inline-flex items-center" aria-label="Kamori">
                    <BrandMark size={30} />
                </a>
                <a
                    class="bg-slate px-3 py-2 text-sm text-white"
                    href="/app">{copy.webApp}</a
                >
                <a class="px-3 py-2 text-sm text-slate" href="/"
                    >{copy.home}</a
                >
            </div>

            <div class="flex items-center gap-2">
                <LocaleSwitch />
                <Badge active={Boolean($appState.accessToken)}>
                    {$appState.accessToken ? copy.authenticated : copy.signedOut}
                </Badge>
                {#if $appState.accessToken}
                    <Button variant="ghost" on:click={logout}>{copy.logout}</Button>
                {/if}
            </div>
        </nav>

        {#if !$appState.accessToken}
            <div class="mx-auto mt-14 max-w-lg">
                <Card>
                    <h2
                        class="text-center font-heading text-xl font-semibold text-slate"
                    >
                        {copy.authRequired}
                    </h2>
                    <p class="mt-2 text-center text-sm text-slate/80">
                        {copy.authBody}
                    </p>
                    <div class="mt-4 flex flex-wrap justify-center gap-2">
                        <Button variant="secondary" on:click={openSignIn}
                            >{copy.signIn}</Button
                        >
                        <Button variant="ghost" on:click={openSignUp}
                            >{copy.signUp}</Button
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
