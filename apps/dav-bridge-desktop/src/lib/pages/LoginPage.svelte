<script lang="ts">
    import { navigate } from "../router";
    import { api } from "../tauri";
    import { backendSettings, loginNotice, session } from "../stores/app";
    import Card from "../components/ui/Card.svelte";
    import Button from "../components/ui/Button.svelte";
    import Input from "../components/ui/Input.svelte";

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
            loginNotice.set("Enter your username to continue.");
            return;
        }
        if (!password) {
            loginNotice.set("Enter your password to continue.");
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
            await completeLogin("Signed in with password.");
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            loginNotice.set(`Sign-in failed: ${message}`);
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
                `Approve code ${start.user_code} in the browser. Keep this window open.`,
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
                    await completeLogin("Signed in through your browser.");
                    return;
                }
            }
            throw new Error("Browser authorization expired. Start again.");
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            loginNotice.set(`Sign-in failed: ${message}`);
        } finally {
            loading = false;
            loadingMode = null;
        }
    };
</script>

<section class="mx-auto max-w-3xl px-4 py-12 animate-fade-slide">
    <div class="mb-8 text-center">
        <h1 class="font-heading text-4xl font-bold text-slate">
            Kamori Desktop Bridge
        </h1>
        <p class="mx-auto mt-3 max-w-2xl text-sm text-slate/70">
            Sign in to start the local DAV bridge. Account registration is
            handled in the web portal.
        </p>
    </div>

    <Card>
        <h2 class="mb-4 font-heading text-xl font-semibold">Sign In</h2>

        <form class="space-y-3" on:submit|preventDefault={signInPassword}>
            <div class="space-y-1">
                <p
                    class="block text-xs font-semibold uppercase tracking-wide text-slate/70"
                >
                    Username
                </p>
                <Input bind:value={username} required />
            </div>

            <div class="space-y-1">
                <p
                    class="block text-xs font-semibold uppercase tracking-wide text-slate/70"
                >
                    Password
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
                    TOTP (optional)
                </p>
                <Input bind:value={totpCode} placeholder="123456" />
            </div>

            <div class="pt-2">
                <div class="flex flex-wrap items-center gap-3">
                    <Button type="submit" disabled={loading}>
                        {loadingMode === "password" ? "Logging In..." : "Login"}
                    </Button>
                    <Button
                        type="button"
                        variant="secondary"
                        disabled={loading}
                        on:click={signInBrowser}
                    >
                        {loadingMode === "passkey"
                            ? "Signing In..."
                            : "Continue In Browser"}
                    </Button>
                </div>
                <p class="mt-2 text-xs text-slate/70">
                    The system browser handles passkeys on the trusted Kamori
                    web origin. First-time devices must be unlocked once with
                    the password before browser-only sign-in can restore keys.
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
