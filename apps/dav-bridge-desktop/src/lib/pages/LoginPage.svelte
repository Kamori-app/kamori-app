<script lang="ts">
    import { navigate } from "../router";
    import { api } from "../tauri";
    import { backendSettings, loginNotice, session } from "../stores/app";
    import {
        parseRequestOptions,
        serializeAssertionCredential,
        toUtf8Bytes,
    } from "../webauthn";
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

    const signInPasskey = async () => {
        if (!("PublicKeyCredential" in window) || !navigator.credentials) {
            loginNotice.set(
                "Passkey login is not supported in this environment.",
            );
            return;
        }

        loading = true;
        loadingMode = "passkey";

        try {
            await configureBackend();

            const start = await api.passkeyLoginStart();
            const requestOptions = parseRequestOptions(
                start.public_key_credential_request_options,
            );

            const credential = (await navigator.credentials.get({
                publicKey: requestOptions,
            })) as PublicKeyCredential | null;

            if (!credential) {
                throw new Error("Passkey request was cancelled.");
            }

            const payload = serializeAssertionCredential(credential);
            if (!start.flow_id) {
                throw new Error("Missing flow_id for passkey login.");
            }
            await api.passkeyLoginFinish(
                toUtf8Bytes(JSON.stringify(payload)),
                start.flow_id,
            );
            await completeLogin("Signed in with passkey.");
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
                        on:click={signInPasskey}
                    >
                        {loadingMode === "passkey"
                            ? "Signing In..."
                            : "Sign In With Passkey"}
                    </Button>
                </div>
                <p class="mt-2 text-xs text-slate/70">
                    Passkey login uses discoverable authentication and does not
                    require username input.
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
