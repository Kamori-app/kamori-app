<script lang="ts">
    import { onMount } from "svelte";
    import { browser } from "$app/environment";
    import { goto } from "$app/navigation";
    import { page } from "$app/stores";
    import { cloudApi } from "$lib/api/cloud";
    import { bestEffortLogoutWithRefresh } from "$lib/auth/session-flow.js";
    import { tokenStore } from "$lib/auth/tokenStore";
    import BrandMark from "$lib/components/BrandMark.svelte";
    import LocaleSwitch from "$lib/components/LocaleSwitch.svelte";
    import AccountRecovery from "$lib/components/app/AccountRecovery.svelte";
    import AppWorkspace from "$lib/components/app/AppWorkspace.svelte";
    import SettingsModal from "$lib/components/app/SettingsModal.svelte";
    import SignInModal from "$lib/components/app/SignInModal.svelte";
    import SignUpModal from "$lib/components/app/SignUpModal.svelte";
    import SyncStatus from "$lib/components/app/SyncStatus.svelte";
    import Badge from "$lib/components/ui/Badge.svelte";
    import Button from "$lib/components/ui/Button.svelte";
    import {
        lockWebVault,
        unlockWebVaultFromLocalUnlock,
    } from "$lib/cryptoVault";
    import { locale } from "$lib/i18n";
    import { appState } from "$lib/stores/app";
    import { notify } from "$lib/stores/notifications";
    import { requestManualSync, resetSyncState } from "$lib/stores/sync";

    const copy = {
        en: {
            today: "Today",
            tasks: "Tasks",
            calendar: "Calendar",
            contacts: "Contacts",
            spaces: "Spaces",
            settings: "Settings",
            signIn: "Sign in",
            signUp: "Create account",
            logout: "Log out",
            home: "Home",
            signedOut: "Not signed in",
            locked: "Your server session is available, but sign in again to unlock encrypted browser data.",
            restored: "Session restored.",
            loggedOut: "Logged out.",
            general: "General",
            security: "Security",
            devices: "Devices & sessions",
            privacy: "Privacy",
            account: "Account",
            advanced: "Advanced",
        },
        ru: {
            today: "Сегодня",
            tasks: "Задачи",
            calendar: "Календарь",
            contacts: "Контакты",
            spaces: "Пространства",
            settings: "Настройки",
            signIn: "Войти",
            signUp: "Создать аккаунт",
            logout: "Выйти",
            home: "Главная",
            signedOut: "Вход не выполнен",
            locked: "Серверная сессия доступна, но для расшифровки данных браузера нужно войти ещё раз.",
            restored: "Сессия восстановлена.",
            loggedOut: "Вы вышли из аккаунта.",
            general: "Общие",
            security: "Безопасность",
            devices: "Устройства и сессии",
            privacy: "Приватность",
            account: "Аккаунт",
            advanced: "Дополнительно",
        },
    } as const;

    let workspaceView:
        | "today"
        | "tasks"
        | "calendar"
        | "contacts"
        | "spaces"
        | "sharing" = "today";
    let settingsSection:
        | "general"
        | "security"
        | "devices"
        | "privacy"
        | "account"
        | "advanced" = "general";
    let previouslyAuthenticated = false;

    const normalizeSettingsSection = (
        value: string | undefined,
    ): typeof settingsSection => {
        if (
            value === "security" ||
            value === "devices" ||
            value === "privacy" ||
            value === "account" ||
            value === "advanced"
        ) {
            return value;
        }
        return "general";
    };

    $: text = copy[$locale];
    $: path = $page.url.pathname.replace(/^\/app\/?/, "");
    $: firstSegment = path.split("/")[0] || "today";
    $: authView = firstSegment === "sign-up"
        ? "sign-up"
        : firstSegment === "recovery"
            ? "recovery"
            : "sign-in";
    $: workspaceView = firstSegment === "tasks" ||
        firstSegment === "calendar" ||
        firstSegment === "contacts" ||
        firstSegment === "spaces" ||
        firstSegment === "sharing"
            ? firstSegment
            : "today";
    $: settingsSection = normalizeSettingsSection(path.split("/")[1]);
    $: settingsOpen = firstSegment === "settings";
    $: if (previouslyAuthenticated && !$appState.accessToken) {
        resetSyncState();
    }
    $: previouslyAuthenticated = Boolean($appState.accessToken);
    $: if (
        browser &&
        $appState.accessToken &&
        (firstSegment === "sign-in" ||
            firstSegment === "sign-up" ||
            firstSegment === "recovery")
    ) {
        void goto("/app", { replaceState: true });
    }

    const navItems = [
        { href: "/app", key: "today" },
        { href: "/app/tasks", key: "tasks" },
        { href: "/app/calendar", key: "calendar" },
        { href: "/app/contacts", key: "contacts" },
        { href: "/app/spaces", key: "spaces" },
    ] as const;

    const settingsItems = [
        { href: "/app/settings", key: "general" },
        { href: "/app/settings/security", key: "security" },
        { href: "/app/settings/devices", key: "devices" },
        { href: "/app/settings/privacy", key: "privacy" },
        { href: "/app/settings/account", key: "account" },
        { href: "/app/settings/advanced", key: "advanced" },
    ] as const;

    const openSignIn = () => void goto("/app/sign-in");
    const openSignUp = () => void goto("/app/sign-up");
    const openRecovery = () => void goto("/app/recovery");

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
        resetSyncState();
        appState.update((state) => ({
            ...state,
            currentUsername: "",
            accessToken: null,
            totpContinuationToken: null,
            collections: [],
        }));
        notify(text.loggedOut, { kind: "success", source: "Kamori" });
        await goto("/app/sign-in");
    };

    onMount(() => {
        if (!$appState.accessToken) {
            void (async () => {
                try {
                    const rotated = await cloudApi.refresh($appState.cloudBaseUrl);
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
                        }));
                        notify(text.locked, { kind: "warning", source: "Kamori" });
                        return;
                    }
                    tokenStore.setAccessToken(rotated.access_token);
                    appState.update((state) => ({
                        ...state,
                        currentUsername: username,
                        accessToken: rotated.access_token,
                    }));
                    notify(text.restored, { kind: "success", source: "Kamori" });
                    if (firstSegment === "sign-in") await goto("/app");
                } catch {
                    // No active cookie-backed session.
                }
            })();
        }

        if ($page.url.searchParams.get("start") === "signup") {
            void goto("/app/sign-up", { replaceState: true });
        }
    });
</script>

{#if !$appState.accessToken}
    <main class="min-h-screen px-4 py-6 md:px-8">
        <div class="mx-auto max-w-xl">
            <nav class="mb-8 flex items-center justify-between border-y border-slate/20 py-3">
                <a href="/" class="inline-flex items-center gap-2" aria-label="Kamori home">
                    <BrandMark size={32} />
                    <span class="font-heading font-semibold tracking-[0.12em] text-slate">KAMORI</span>
                </a>
                <div class="flex items-center gap-2">
                    <LocaleSwitch />
                    <a class="px-2 py-1 text-sm text-slate" href="/">{text.home}</a>
                </div>
            </nav>

            {#if authView === "recovery"}
                <AccountRecovery onComplete={openSignIn} onCancel={openSignIn} />
            {:else if authView === "sign-up"}
                <SignUpModal
                    open
                    embedded
                    onClose={openSignIn}
                    onOpenSignIn={openSignIn}
                />
            {:else}
                <SignInModal
                    open
                    embedded
                    onClose={() => void goto("/app")}
                    onOpenRecovery={openRecovery}
                />
                <p class="mt-4 text-center text-sm text-slate/70">
                    {text.signedOut} ·
                    <button class="font-semibold underline underline-offset-2" on:click={openSignUp}>
                        {text.signUp}
                    </button>
                </p>
            {/if}
        </div>
    </main>
{:else}
    <div class="min-h-screen bg-surface md:grid md:grid-cols-[15rem_minmax(0,1fr)]">
        <aside class="sticky top-0 hidden h-screen border-r border-slate/15 bg-paper px-4 py-5 md:flex md:flex-col">
            <a href="/" class="inline-flex items-center gap-2 px-2" aria-label="Kamori home">
                <BrandMark size={34} />
                <span class="font-heading font-semibold tracking-[0.12em] text-slate">KAMORI</span>
            </a>
            <nav class="mt-9 space-y-1" aria-label="Application">
                {#each navItems as item}
                    <a
                        href={item.href}
                        class:bg-slate={workspaceView === item.key && !settingsOpen}
                        class:text-white={workspaceView === item.key && !settingsOpen}
                        class="block px-3 py-2.5 text-sm font-medium text-slate hover:bg-sand/60"
                    >{text[item.key]}</a>
                {/each}
            </nav>
            <div class="mt-auto space-y-3 border-t border-slate/15 pt-4">
                <a
                    href="/app/settings"
                    class:bg-sand={settingsOpen}
                    class="block px-3 py-2 text-sm font-medium text-slate hover:bg-sand/60"
                >{text.settings}</a>
                <Button variant="ghost" on:click={logout}>{text.logout}</Button>
            </div>
        </aside>

        <div class="min-w-0 pb-20 md:pb-0">
            <header class="sticky top-0 z-30 flex min-h-16 min-w-0 items-center gap-3 border-b border-slate/15 bg-paper/95 px-4 backdrop-blur md:px-7">
                <a href="/app" class="inline-flex shrink-0 items-center gap-2 md:hidden" aria-label="Kamori">
                    <BrandMark size={28} />
                    <span class="hidden font-heading text-sm font-semibold tracking-[0.12em] text-slate min-[360px]:inline">KAMORI</span>
                </a>
                <div class="hidden md:block">
                    <Badge active>{$appState.currentUsername}</Badge>
                </div>
                <div class="ml-auto flex min-w-0 shrink-0 items-center gap-2">
                    <SyncStatus onSync={requestManualSync} />
                    <LocaleSwitch />
                    <a class="px-2 py-2 text-sm text-slate md:hidden" href="/app/settings" aria-label={text.settings}>⚙</a>
                </div>
            </header>

            <main class="mx-auto max-w-5xl px-4 py-5 md:px-7 md:py-7">
                <div class:hidden={settingsOpen} aria-hidden={settingsOpen}>
                    <AppWorkspace view={workspaceView} />
                </div>
                {#if settingsOpen}
                    <div class="grid gap-4 lg:grid-cols-[12rem_minmax(0,1fr)]">
                        <nav class="flex gap-1 overflow-x-auto border-b border-slate/15 pb-3 lg:block lg:space-y-1 lg:border-b-0 lg:border-r lg:pb-0 lg:pr-4" aria-label={text.settings}>
                            {#each settingsItems as item}
                                <a
                                    href={item.href}
                                    class:bg-slate={settingsSection === item.key}
                                    class:text-white={settingsSection === item.key}
                                    class="block shrink-0 px-3 py-2 text-sm text-slate hover:bg-sand/60"
                                >{text[item.key]}</a>
                            {/each}
                        </nav>
                        <SettingsModal
                            open
                            embedded
                            section={settingsSection}
                            onClose={() => void goto("/app")}
                        />
                    </div>
                {/if}
            </main>
        </div>

        <nav class="fixed inset-x-0 bottom-0 z-40 grid grid-cols-5 border-t border-slate/20 bg-paper md:hidden" aria-label="Application">
            {#each navItems as item}
                <a
                    href={item.href}
                    class:bg-sand={workspaceView === item.key && !settingsOpen}
                    class="min-w-0 px-1 py-3 text-center text-[11px] font-semibold text-slate"
                >{text[item.key]}</a>
            {/each}
        </nav>
    </div>
{/if}

<slot />
