<script lang="ts">
    import { cloudApi } from "$lib/api/cloud";
    import { tokenStore } from "$lib/auth/tokenStore";
    import { appState } from "$lib/stores/app";
    import Button from "$lib/components/ui/Button.svelte";
    import Input from "$lib/components/ui/Input.svelte";
    import Modal from "$lib/components/ui/Modal.svelte";
    import {
        opaqueSigninFinish,
        opaqueSigninStart,
        unwrapAccountMasterKey,
        encryptVaultBytes,
    } from "$lib/opaqueClient";
    import {
        forgetMasterKeyForLocalUnlock,
        lockWebVault,
        rememberMasterKeyForLocalUnlock,
        unlockOrCreateWebVault,
        unlockWebVaultFromLocalUnlock,
        withActiveMasterKey,
    } from "$lib/cryptoVault";
    import {
        parseRequestOptions,
        serializeAssertionCredential,
        toUtf8Bytes,
    } from "$lib/webauthn";
    import { locale } from "$lib/i18n";
    import { notify } from "$lib/stores/notifications";

    const signinCopy = {
        en: {
            title: "Sign in",
            username: "Username",
            password: "Password",
            totp: "TOTP or one-time backup code (if enabled)",
            signingIn: "Signing in…",
            signIn: "Sign in",
            recovery: "Recover account",
            passkey: "Passkey",
            signInPasskey: "Sign in with passkey",
            passkeyBody: "Passkey login uses discoverable authentication and does not require username input.",
            credentialsRequired: "Username and password are required.",
            signedIn: "Signed in with password.",
            totpRequired: "TOTP is required. Enter a code to complete this sign-in.",
            passwordFailed: "Password sign-in failed.",
            signInFailed: "Sign-in failed",
            passkeyUnsupported: "Passkey login is not supported in this browser.",
            passkeyFailed: "Passkey sign-in failed",
            passkeyUnlocked: "Signed in with passkey and unlocked this approved browser.",
            passkeyApprove: "Passkey authentication succeeded. Enter your password once to approve and unlock this new browser.",
            passkeyCancelled: "Passkey request was cancelled.",
            localUnlock: "Allow passkey sign-in to unlock data on this browser",
            localUnlockBody: "Off by default. Kamori stores the account key encrypted by a non-exportable browser key. This protects copied storage, but code running as app.kamori.app in this browser profile can request decryption.",
        },
        ru: {
            title: "Войти",
            username: "Имя пользователя",
            password: "Пароль",
            totp: "TOTP или одноразовый backup-код (если включено)",
            signingIn: "Входим…",
            signIn: "Войти",
            recovery: "Восстановить аккаунт",
            passkey: "Passkey",
            signInPasskey: "Войти с passkey",
            passkeyBody: "Passkey использует discoverable-аутентификацию, поэтому имя пользователя вводить не нужно.",
            credentialsRequired: "Введите имя пользователя и пароль.",
            signedIn: "Вход по паролю выполнен.",
            totpRequired: "Нужен TOTP-код. Введите его, чтобы завершить этот вход.",
            passwordFailed: "Не удалось войти по паролю.",
            signInFailed: "Не удалось войти",
            passkeyUnsupported: "Этот браузер не поддерживает вход с passkey.",
            passkeyFailed: "Не удалось войти с passkey",
            passkeyUnlocked: "Вход с passkey выполнен, одобренный браузер разблокирован.",
            passkeyApprove: "Passkey подтверждён. Один раз введите пароль, чтобы одобрить и разблокировать новый браузер.",
            passkeyCancelled: "Запрос passkey отменён.",
            localUnlock: "Разрешить passkey-входу разблокировать данные в этом браузере",
            localUnlockBody: "По умолчанию выключено. Ключ аккаунта хранится зашифрованным неизвлекаемым ключом браузера. Это защищает скопированное хранилище, но код app.kamori.app в этом профиле может запросить расшифровку.",
        },
    } as const;

    $: copy = signinCopy[$locale];

    /**
     * Sign-in modal supporting:
     * - OPAQUE password flow
     * - passkey assertion flow
     */
    export let open = false;
    export let onClose: () => void = () => {};
    export let onOpenRecovery: () => void = () => {};
    export let embedded = false;

    let loginUsername = "";
    let loginPassword = "";
    let loginTotpCode = "";
    let loadingAction = "";
    let wasOpen = false;
    let pendingOpaqueExportKey: Uint8Array | null = null;
    let allowLocalUnlock = false;
    let formNotice = "";

    const setLoading = (value: string) => {
        loadingAction = value;
    };

    const clearLoading = () => {
        loadingAction = "";
    };

    const setNotice = (notice: string) => {
        formNotice = notice;
        notify(notice, { source: copy.title });
    };

    /**
     * Runs OPAQUE signin against cloud start/finish endpoints.
     */
    const signinWithOpaque = async () => {
        const username = loginUsername.trim();
        const continuationToken = $appState.totpContinuationToken;
        if (!username || (!continuationToken && !loginPassword)) {
            setNotice(copy.credentialsRequired);
            return;
        }

        setLoading("signin-opaque");
        try {
            if (continuationToken) {
                const code = loginTotpCode.trim();
                if (!code || !pendingOpaqueExportKey) {
                    setNotice(copy.totpRequired);
                    return;
                }
                const response = await cloudApi.signinTotp(
                    $appState.cloudBaseUrl,
                    {
                        continuation_token: continuationToken,
                        totp_code: code,
                    },
                );
                await completePasswordSignin(
                    username,
                    response,
                    pendingOpaqueExportKey,
                );
                return;
            }

            const start = await opaqueSigninStart(loginPassword);
            const startResponse = await cloudApi.signinStart(
                $appState.cloudBaseUrl,
                {
                    username,
                    opaque_start_request: start.opaque_start_request,
                },
            );

            const finish = await opaqueSigninFinish(
                start.flow_id,
                loginPassword,
                startResponse.opaque_server_message,
            );

            const totp = loginTotpCode.trim() || undefined;
            const finishResponse = await cloudApi.signinFinish(
                $appState.cloudBaseUrl,
                {
                    username,
                    opaque_flow_id: startResponse.opaque_flow_id,
                    opaque_finish_request: finish.opaque_finish_request,
                    totp_code: totp,
                },
            );

            if (finishResponse.access_token) {
                await completePasswordSignin(
                    username,
                    finishResponse,
                    finish.export_key,
                );
                return;
            }

            if (finishResponse.totp_continuation_token) {
                pendingOpaqueExportKey?.fill(0);
                pendingOpaqueExportKey = new Uint8Array(finish.export_key);
                tokenStore.clear();
                appState.update((state) => ({
                    ...state,
                    currentUsername: username,
                    accessToken: null,
                    totpContinuationToken:
                        finishResponse.totp_continuation_token ?? null,
                }));
                setNotice(copy.totpRequired);
                return;
            }

            setNotice(copy.passwordFailed);
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`${copy.signInFailed}: ${message}`);
        } finally {
            clearLoading();
        }
    };

    const completePasswordSignin = async (
        username: string,
        response: Awaited<ReturnType<typeof cloudApi.signinFinish>>,
        exportKey: Uint8Array,
    ) => {
        if (!response.access_token) {
            throw new Error(copy.passwordFailed);
        }
        if (!response.device_enrollment_token) {
            throw new Error("Device enrollment capability is missing.");
        }
        const masterKey = await unwrapAccountMasterKey(
            exportKey,
            response.encrypted_master_key,
        );
        try {
            const device = await unlockOrCreateWebVault(
                $appState.cloudBaseUrl,
                username,
                masterKey,
            );
            if (allowLocalUnlock) {
                await rememberMasterKeyForLocalUnlock(
                    $appState.cloudBaseUrl,
                    username,
                    masterKey,
                );
            } else {
                await forgetMasterKeyForLocalUnlock(
                    $appState.cloudBaseUrl,
                    username,
                );
            }
            const encryptedDeviceName = await encryptVaultBytes(
                masterKey,
                new TextEncoder().encode("Web browser"),
            );
            await cloudApi.registerDevice(
                $appState.cloudBaseUrl,
                {
                    enrollment_token: response.device_enrollment_token,
                    device_id: device.deviceId,
                    signing_public_key: device.identity.signing_public_key,
                    hpke_public_key: device.identity.hpke_public_key,
                    encrypted_name: encryptedDeviceName,
                    platform: "web",
                },
                response.access_token,
            );
            tokenStore.setAccessToken(response.access_token);
            appState.update((state) => ({
                ...state,
                currentUsername: username,
                accessToken: response.access_token ?? null,
                totpContinuationToken: null,
            }));
            notify(copy.signedIn, { kind: "success", source: copy.title });
            pendingOpaqueExportKey?.fill(0);
            pendingOpaqueExportKey = null;
            loginPassword = "";
            loginTotpCode = "";
            onClose();
        } catch (error) {
            lockWebVault();
            tokenStore.clear();
            appState.update((state) => ({
                ...state,
                accessToken: null,
                totpContinuationToken: null,
            }));
            try {
                await cloudApi.logout(
                    $appState.cloudBaseUrl,
                    response.access_token,
                );
            } catch {
                // Preserve the enrollment error; the access token is never
                // installed locally and the refresh cookie will be handled by
                // the next explicit authentication attempt.
            }
            throw error;
        } finally {
            masterKey.fill(0);
        }
    };

    /**
     * Runs browser WebAuthn assertion and completes passkey signin.
     */
    const signinWithPasskey = async () => {
        if (!("PublicKeyCredential" in window) || !navigator.credentials) {
            setNotice(copy.passkeyUnsupported);
            return;
        }

        setLoading("signin-passkey");
        try {
            const start = await cloudApi.passkeyLoginStart(
                $appState.cloudBaseUrl,
            );
            const requestOptions = parseRequestOptions(
                start.public_key_credential_request_options,
            );
            const credential = (await navigator.credentials.get({
                publicKey: requestOptions,
            })) as PublicKeyCredential | null;

            if (!credential) {
                throw new Error(copy.passkeyCancelled);
            }

            const payload = toUtf8Bytes(
                JSON.stringify(serializeAssertionCredential(credential)),
            );
            const finish = await cloudApi.passkeyLoginFinish(
                $appState.cloudBaseUrl,
                payload,
                start.flow_id,
            );

            const localDevice = await unlockWebVaultFromLocalUnlock(
                $appState.cloudBaseUrl,
                finish.username,
            );
            if (!localDevice) {
                // A passkey proves the account identity, but a clean browser
                // still has no E2EE master key or registered device. Do not
                // present the enrollment-only session as an unlocked login.
                try {
                    await cloudApi.logout(
                        $appState.cloudBaseUrl,
                        finish.access_token,
                    );
                } catch {
                    // The access token is intentionally not retained. A
                    // cookie that could not be revoked remains unable to use
                    // normal endpoints because it is not device-bound.
                }
                tokenStore.clear();
                appState.update((state) => ({
                    ...state,
                    currentUsername: finish.username,
                    accessToken: null,
                    totpContinuationToken: null,
                }));
                setNotice(copy.passkeyApprove);
                allowLocalUnlock = true;
                return;
            }

            const encryptedDeviceName = await withActiveMasterKey((masterKey) =>
                encryptVaultBytes(
                    masterKey,
                    new TextEncoder().encode("Web browser"),
                ),
            );
            try {
                await cloudApi.registerDevice(
                    $appState.cloudBaseUrl,
                    {
                        enrollment_token: finish.device_enrollment_token,
                        device_id: localDevice.deviceId,
                        signing_public_key:
                            localDevice.identity.signing_public_key,
                        hpke_public_key: localDevice.identity.hpke_public_key,
                        encrypted_name: encryptedDeviceName,
                        platform: "web",
                    },
                    finish.access_token,
                );
            } catch (error) {
                try {
                    await cloudApi.logout(
                        $appState.cloudBaseUrl,
                        finish.access_token,
                    );
                } catch {
                    // The original enrollment error is the actionable one.
                }
                throw error;
            }
            tokenStore.setAccessToken(finish.access_token);
            appState.update((state) => ({
                ...state,
                currentUsername: finish.username,
                accessToken: finish.access_token,
                totpContinuationToken: null,
            }));
            notify(copy.passkeyUnlocked, { kind: "success", source: copy.title });
            onClose();
        } catch (error) {
            lockWebVault();
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`${copy.passkeyFailed}: ${message}`);
        } finally {
            clearLoading();
        }
    };

    $: if (open && !wasOpen) {
        if ($appState.currentUsername) {
            loginUsername = $appState.currentUsername;
        }
        wasOpen = true;
    }

    $: if (!open && wasOpen) {
        loginPassword = "";
        loginTotpCode = "";
        formNotice = "";
        loadingAction = "";
        pendingOpaqueExportKey?.fill(0);
        pendingOpaqueExportKey = null;
        allowLocalUnlock = false;
        appState.update((state) => ({
            ...state,
            totpContinuationToken: null,
        }));
        wasOpen = false;
    }
</script>

<Modal {open} title={copy.title} {onClose} {embedded}>
    <div class="space-y-3">
        {#if formNotice}
            <p class="border border-coral/30 bg-coral/10 p-3 text-sm text-slate" role="alert">
                {formNotice}
            </p>
        {/if}
        <Input bind:value={loginUsername} placeholder={copy.username} />
        <Input
            bind:value={loginPassword}
            type="password"
            placeholder={copy.password}
        />
        <label class="flex items-start gap-2 text-xs text-slate/80">
            <input
                class="mt-0.5"
                type="checkbox"
                bind:checked={allowLocalUnlock}
            />
            <span>
                <span class="block font-medium text-ink">{copy.localUnlock}</span>
                <span class="mt-1 block text-slate/65">{copy.localUnlockBody}</span>
            </span>
        </label>
        {#if $appState.totpContinuationToken}
            <Input bind:value={loginTotpCode} placeholder={copy.totp} />
        {/if}
        <Button
            on:click={signinWithOpaque}
            disabled={loadingAction === "signin-opaque"}
        >
            {loadingAction === "signin-opaque" ? copy.signingIn : copy.signIn}
        </Button>

        <div class="border-t border-slate/15 pt-4">
            <p
                class="text-xs font-semibold uppercase tracking-wide text-slate/70"
            >
                {copy.passkey}
            </p>
            <div class="mt-2">
                <Button
                    variant="secondary"
                    on:click={signinWithPasskey}
                    disabled={loadingAction === "signin-passkey"}
                >
                    {loadingAction === "signin-passkey"
                        ? copy.signingIn
                        : copy.signInPasskey}
                </Button>
            </div>
            <p class="mt-2 text-xs text-slate/70">
                {copy.passkeyBody}
            </p>
        </div>

        <div class="border-t border-slate/15 pt-3 text-sm text-slate/70">
            <button
                class="font-semibold text-slate underline underline-offset-2"
                on:click={onOpenRecovery}
            >{copy.recovery}</button>
        </div>
    </div>
</Modal>
