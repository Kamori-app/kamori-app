<script lang="ts">
    import { navigate } from "../router";
    import { api } from "../tauri";
    import { backendSettings, loginNotice, session } from "../stores/app";
    import Card from "../components/ui/Card.svelte";
    import Button from "../components/ui/Button.svelte";
    import Input from "../components/ui/Input.svelte";
    import BrandMark from "../components/BrandMark.svelte";
    import LocaleSwitch from "../components/LocaleSwitch.svelte";
    import { locale } from "../i18n";

    const copies = {
        en: {
            title: "Kamori desktop",
            intro: "Your encrypted control center and optional CalDAV/CardDAV bridge. Registration stays in the web app.",
            signIn: "Sign in", username: "Username", password: "Password", totp: "TOTP (optional)",
            loggingIn: "Signing in…", login: "Sign in", browserBusy: "Opening browser…", browser: "Continue in browser",
            passkey: "The trusted Kamori web origin handles passkeys. A new device needs your password once before browser-only sign-in can restore its keys.",
            usernameRequired: "Enter your username to continue.", passwordRequired: "Enter your password to continue.",
            signedPassword: "Signed in with password.", approve: (code: string) => `Approve code ${code} in the browser. Keep this window open.`,
            signedBrowser: "Signed in through your browser.", expired: "Browser authorization expired. Start again.", failed: "Sign-in failed",
        },
        ru: {
            title: "Kamori для компьютера",
            intro: "Центр управления зашифрованными данными и необязательный мост CalDAV/CardDAV. Регистрация доступна в веб-приложении.",
            signIn: "Вход", username: "Имя пользователя", password: "Пароль", totp: "TOTP (необязательно)",
            loggingIn: "Входим…", login: "Войти", browserBusy: "Открываем браузер…", browser: "Продолжить в браузере",
            passkey: "Passkey обрабатывается на доверенном веб-домене Kamori. На новом устройстве пароль нужен один раз, чтобы затем восстанавливать ключи через браузер.",
            usernameRequired: "Введите имя пользователя.", passwordRequired: "Введите пароль.",
            signedPassword: "Вход по паролю выполнен.", approve: (code: string) => `Подтвердите код ${code} в браузере и не закрывайте это окно.`,
            signedBrowser: "Вход через браузер выполнен.", expired: "Время подтверждения истекло. Начните заново.", failed: "Не удалось войти",
        },
    } as const;
    $: copy = copies[$locale];

    let username = "";
    let password = "";
    let totpCode = "";
    let loading = false;
    let loadingMode: "password" | "passkey" | null = null;

    const configureBackend = async () =>
        api.configureBackend($backendSettings.cloudBaseUrl);

    const completeLogin = async (notice: string) => {
        const snap = await api.dashboardSnapshot();
        session.set({
            hasSession: snap.has_access_token,
            serverRunning: snap.server.running,
            bindAddr: snap.server.bind_addr,
            collectionsTotal: snap.collections_total,
            syncedItemsTotal: snap.synced_items_total,
        });

        loginNotice.set(notice);
        navigate("/dashboard");
    };

    const signInPassword = async () => {
        const user = username.trim();

        if (!user) {
            loginNotice.set(copy.usernameRequired);
            return;
        }
        if (!password) {
            loginNotice.set(copy.passwordRequired);
            return;
        }

        loading = true;
        loadingMode = "password";

        try {
            await configureBackend();
            await api.passwordLogin(
                user,
                password,
                totpCode.trim() || undefined,
            );
            await completeLogin(copy.signedPassword);
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            loginNotice.set(`${copy.failed}: ${message}`);
        } finally {
            loading = false;
            loadingMode = null;
        }
    };

    const signInBrowser = async () => {
        loading = true;
        loadingMode = "passkey";

        try {
            await configureBackend();

            const start = await api.browserLoginStart();
            loginNotice.set(
                copy.approve(start.user_code),
            );
            const deadline = Date.now() + start.expires_in_seconds * 1000;
            while (Date.now() < deadline) {
                await new Promise((resolve) =>
                    window.setTimeout(
                        resolve,
                        Math.max(start.poll_interval_seconds, 1) * 1000,
                    ),
                );
                const result = await api.browserLoginPoll(
                    start.flow_id,
                    start.device_secret,
                );
                if (result.status === "approved") {
                    await completeLogin(copy.signedBrowser);
                    return;
                }
            }
            throw new Error(copy.expired);
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            loginNotice.set(`${copy.failed}: ${message}`);
        } finally {
            loading = false;
            loadingMode = null;
        }
    };
</script>

<section class="mx-auto max-w-3xl px-4 py-12 animate-fade-slide">
    <div class="mb-8">
        <div class="mb-8 flex items-center justify-between border-b border-slate/20 pb-4">
            <div class="flex items-center gap-3"><BrandMark size={42} /><span class="text-xs font-semibold tracking-[0.18em]">KAMORI</span></div>
            <LocaleSwitch />
        </div>
        <h1 class="font-heading text-4xl font-bold text-slate">
            {copy.title}
        </h1>
        <p class="mx-auto mt-3 max-w-2xl text-sm text-slate/70">
            {copy.intro}
        </p>
    </div>

    <Card>
        <h2 class="mb-4 font-heading text-xl font-semibold">{copy.signIn}</h2>

        <form class="space-y-3" on:submit|preventDefault={signInPassword}>
            <div class="space-y-1">
                <p
                    class="block text-xs font-semibold uppercase tracking-wide text-slate/70"
                >
                    {copy.username}
                </p>
                <Input bind:value={username} required />
            </div>

            <div class="space-y-1">
                <p
                    class="block text-xs font-semibold uppercase tracking-wide text-slate/70"
                >
                    {copy.password}
                </p>
                <Input
                    bind:value={password}
                    required
                    type="password"
                    placeholder="********"
                />
            </div>

            <div class="space-y-1">
                <p
                    class="block text-xs font-semibold uppercase tracking-wide text-slate/70"
                >
                    {copy.totp}
                </p>
                <Input bind:value={totpCode} placeholder="123456" />
            </div>

            <div class="pt-2">
                <div class="flex flex-wrap items-center gap-3">
                    <Button type="submit" disabled={loading}>
                        {loadingMode === "password" ? copy.loggingIn : copy.login}
                    </Button>
                    <Button
                        type="button"
                        variant="secondary"
                        disabled={loading}
                        on:click={signInBrowser}
                    >
                        {loadingMode === "passkey"
                            ? copy.browserBusy
                            : copy.browser}
                    </Button>
                </div>
                <p class="mt-2 text-xs text-slate/70">
                    {copy.passkey}
                </p>
            </div>
        </form>

        {#if $loginNotice}
            <p class="mt-4 rounded-xl bg-sand/50 p-3 text-sm text-slate">
                {$loginNotice}
            </p>
        {/if}
    </Card>
</section>
