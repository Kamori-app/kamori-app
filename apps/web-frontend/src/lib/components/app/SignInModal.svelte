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
        opaqueSignupFinish,
        opaqueSignupStart,
        recoveryPhraseToMasterKey,
        decryptVaultBytes,
        unwrapAccountMasterKey,
        wrapAccountMasterKey,
        encryptVaultBytes,
    } from "$lib/opaqueClient";
    import {
        deriveDataRecoveryVerifier,
        lockWebVault,
        rememberMasterKeyForLocalPasskey,
        storeSpaceKey,
        resetWebCredentialsAfterRecovery,
        unlockOrCreateWebVault,
        unlockWebVaultFromLocalPasskey,
    } from "$lib/cryptoVault";
    import {
        parseRequestOptions,
        serializeAssertionCredential,
        toUtf8Bytes,
    } from "$lib/webauthn";
    import { locale } from "$lib/i18n";

    const signinCopy = {
        en: {
            title: "Sign in",
            username: "Username",
            password: "Password",
            totp: "TOTP or one-time backup code (if enabled)",
            signingIn: "Signing in…",
            signIn: "Sign in",
            recovery: "Account recovery",
            kit: "24-word data recovery kit",
            newPassword: "New password",
            confirmPassword: "Confirm new password",
            recovering: "Recovering…",
            recover: "Recover account",
            recoveryBody: "The 24-word data recovery kit authorizes recovery and restores your account key. It resets the password, rewraps that key, revokes active sessions, and disables TOTP. TOTP backup codes are separate and are never required here.",
            passkey: "Passkey",
            signInPasskey: "Sign in with passkey",
            passkeyBody: "Passkey login uses discoverable authentication and does not require username input.",
            credentialsRequired: "Username and password are required.",
            signedIn: "Signed in with password.",
            totpRequired: "TOTP is required. Enter a code and submit sign in again.",
            passwordFailed: "Password sign-in failed.",
            signInFailed: "Sign-in failed",
            passkeyUnsupported: "Passkey login is not supported in this browser.",
            passkeyFailed: "Passkey sign-in failed",
            passkeyUnlocked: "Signed in with passkey and unlocked this approved browser.",
            passkeyApprove: "Passkey authentication succeeded. Enter your password once to approve and unlock this new browser.",
        },
        ru: {
            title: "Войти",
            username: "Имя пользователя",
            password: "Пароль",
            totp: "TOTP или одноразовый backup-код (если включено)",
            signingIn: "Входим…",
            signIn: "Войти",
            recovery: "Восстановление аккаунта",
            kit: "Recovery kit из 24 слов",
            newPassword: "Новый пароль",
            confirmPassword: "Повторите новый пароль",
            recovering: "Восстанавливаем…",
            recover: "Восстановить аккаунт",
            recoveryBody: "Recovery kit из 24 слов подтверждает восстановление и возвращает ключ аккаунта. Пароль сбрасывается, ключ оборачивается заново, активные сессии отзываются, а TOTP отключается. Backup-коды TOTP здесь не нужны.",
            passkey: "Passkey",
            signInPasskey: "Войти с passkey",
            passkeyBody: "Passkey использует discoverable-аутентификацию, поэтому имя пользователя вводить не нужно.",
            credentialsRequired: "Введите имя пользователя и пароль.",
            signedIn: "Вход по паролю выполнен.",
            totpRequired: "Нужен TOTP-код. Введите его и повторите вход.",
            passwordFailed: "Не удалось войти по паролю.",
            signInFailed: "Не удалось войти",
            passkeyUnsupported: "Этот браузер не поддерживает вход с passkey.",
            passkeyFailed: "Не удалось войти с passkey",
            passkeyUnlocked: "Вход с passkey выполнен, одобренный браузер разблокирован.",
            passkeyApprove: "Passkey подтверждён. Один раз введите пароль, чтобы одобрить и разблокировать новый браузер.",
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

    let loginUsername = "";
    let loginPassword = "";
    let loginTotpCode = "";
    let recoveryNewPassword = "";
    let recoveryPasswordConfirm = "";
    let recoveryPhrase = "";
    let loadingAction = "";
    let wasOpen = false;

    const setLoading = (value: string) => {
        loadingAction = value;
    };

    const clearLoading = () => {
        loadingAction = "";
    };

    const setNotice = (notice: string) => {
        appState.update((state) => ({ ...state, notice }));
    };

    /**
     * Runs OPAQUE signin against cloud start/finish endpoints.
     */
    const signinWithOpaque = async () => {
        const username = loginUsername.trim();
        if (!username || !loginPassword) {
            setNotice(copy.credentialsRequired);
            return;
        }

        setLoading("signin-opaque");
        try {
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
                    preauth_token:
                        $appState.preauthToken ??
                        startResponse.preauth_token ??
                        undefined,
                },
            );

            if (finishResponse.access_token) {
                const masterKey = await unwrapAccountMasterKey(
                    finish.export_key,
                    finishResponse.encrypted_master_key,
                );
                const device = await unlockOrCreateWebVault(
                    username,
                    masterKey,
                );
                await rememberMasterKeyForLocalPasskey(username, masterKey);
                const encryptedDeviceName = await encryptVaultBytes(
                    masterKey,
                    new TextEncoder().encode("Web browser"),
                );
                await cloudApi.registerDevice(
                    $appState.cloudBaseUrl,
                    {
                        device_id: device.deviceId,
                        signing_public_key:
                            device.identity.signing_public_key,
                        hpke_public_key: device.identity.hpke_public_key,
                        encrypted_name: encryptedDeviceName,
                        platform: "web",
                    },
                    finishResponse.access_token,
                );
                masterKey.fill(0);
                tokenStore.setAccessToken(finishResponse.access_token);
                appState.update((state) => ({
                    ...state,
                    currentUsername: username,
                    accessToken: finishResponse.access_token ?? null,
                    preauthToken: null,
                    notice: copy.signedIn,
                }));
                loginPassword = "";
                loginTotpCode = "";
                onClose();
                return;
            }

            if (finishResponse.preauth_token) {
                tokenStore.clear();
                appState.update((state) => ({
                    ...state,
                    currentUsername: username,
                    accessToken: null,
                    preauthToken: finishResponse.preauth_token ?? null,
                }));
                setNotice(
                    copy.totpRequired,
                );
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

    /**
     * Resets password using the data recovery kit and disables TOTP.
     */
    const recoverAccount = async () => {
        const username = loginUsername.trim();
        const nextPassword = recoveryNewPassword;

        if (
            !username ||
            !nextPassword ||
            !recoveryPasswordConfirm ||
            !recoveryPhrase.trim()
        ) {
            setNotice(
                "Username, 24-word data recovery kit, and new password are required.",
            );
            return;
        }
        if (nextPassword !== recoveryPasswordConfirm) {
            setNotice("Password confirmation does not match.");
            return;
        }

        let recoveredMasterKey: Uint8Array;
        try {
            recoveredMasterKey = await recoveryPhraseToMasterKey(
                recoveryPhrase,
            );
        } catch {
            setNotice("The 24-word data recovery kit is invalid.");
            return;
        }

        setLoading("account-recovery");
        try {
            const start = await opaqueSignupStart(nextPassword);
            const recoveryVerifier =
                await deriveDataRecoveryVerifier(recoveredMasterKey);
            const startResponse = await cloudApi.accountRecoveryStart(
                $appState.cloudBaseUrl,
                {
                    username,
                    recovery_verifier: recoveryVerifier,
                    opaque_start_request: start.opaque_start_request,
                },
            );

            const finish = await opaqueSignupFinish(
                start.flow_id,
                nextPassword,
                startResponse.opaque_server_message,
            );
            const encryptedMasterKey = await wrapAccountMasterKey(
                finish.export_key,
                recoveredMasterKey,
            );
            const recovered = await cloudApi.accountRecoveryFinish($appState.cloudBaseUrl, {
                recovery_token: startResponse.recovery_token,
                opaque_finish_request: finish.opaque_finish_request,
                encrypted_master_key: encryptedMasterKey,
            });
            await unlockOrCreateWebVault(username, recoveredMasterKey);
            for (const packageEntry of recovered.space_key_packages) {
                const spaceKey = await decryptVaultBytes(
                    recoveredMasterKey,
                    packageEntry.encrypted_key_package,
                );
                if (spaceKey.length !== 32) {
                    throw new Error("A recovered space key has an invalid length.");
                }
                await storeSpaceKey(
                    packageEntry.space_id,
                    packageEntry.key_epoch,
                    spaceKey,
                );
                spaceKey.fill(0);
            }
            await resetWebCredentialsAfterRecovery(username);
            lockWebVault();

            tokenStore.clear();
            appState.update((state) => ({
                ...state,
                accessToken: null,
                preauthToken: null,
                notice: "Account recovery completed: password reset and TOTP disabled. Sign in with your new password or passkey.",
            }));
            loginPassword = "";
            loginTotpCode = "";
            recoveryNewPassword = "";
            recoveryPasswordConfirm = "";
            recoveryPhrase = "";
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Account recovery failed: ${message}`);
        } finally {
            recoveredMasterKey.fill(0);
            clearLoading();
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
                throw new Error("Passkey request was cancelled.");
            }

            const payload = toUtf8Bytes(
                JSON.stringify(serializeAssertionCredential(credential)),
            );
            const finish = await cloudApi.passkeyLoginFinish(
                $appState.cloudBaseUrl,
                payload,
                start.flow_id,
            );

            const localDevice = await unlockWebVaultFromLocalPasskey(
                finish.username,
            );

            tokenStore.setAccessToken(finish.access_token);
            appState.update((state) => ({
                ...state,
                currentUsername: finish.username,
                accessToken: finish.access_token,
                preauthToken: null,
                notice: localDevice
                    ? copy.passkeyUnlocked
                    : copy.passkeyApprove,
            }));
            if (localDevice) {
                onClose();
            }
        } catch (error) {
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
        recoveryNewPassword = "";
        recoveryPasswordConfirm = "";
        recoveryPhrase = "";
        loadingAction = "";
        wasOpen = false;
    }
</script>

<Modal {open} title={copy.title} {onClose}>
    <div class="space-y-3">
        <Input bind:value={loginUsername} placeholder={copy.username} />
        <Input
            bind:value={loginPassword}
            type="password"
            placeholder={copy.password}
        />
        <Input
            bind:value={loginTotpCode}
            placeholder={copy.totp}
        />
        <Button
            on:click={signinWithOpaque}
            disabled={loadingAction === "signin-opaque"}
        >
            {loadingAction === "signin-opaque" ? copy.signingIn : copy.signIn}
        </Button>

        <div class="space-y-2 rounded-xl border border-slate/15 p-3">
            <p
                class="text-xs font-semibold uppercase tracking-wide text-slate/70"
            >
                {copy.recovery}
            </p>
            <Input
                bind:value={recoveryPhrase}
                autocomplete="off"
                placeholder={copy.kit}
            />
            <Input
                bind:value={recoveryNewPassword}
                type="password"
                placeholder={copy.newPassword}
            />
            <Input
                bind:value={recoveryPasswordConfirm}
                type="password"
                placeholder={copy.confirmPassword}
            />
            <Button
                variant="secondary"
                on:click={recoverAccount}
                disabled={loadingAction === "account-recovery"}
            >
                {loadingAction === "account-recovery"
                    ? copy.recovering
                    : copy.recover}
            </Button>
            <p class="text-xs text-slate/70">
                {copy.recoveryBody}
            </p>
        </div>

        <div class="pt-1">
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
    </div>
</Modal>
