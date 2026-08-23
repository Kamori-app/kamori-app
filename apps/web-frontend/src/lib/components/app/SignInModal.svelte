<script lang="ts">
    import { decode } from "@msgpack/msgpack";
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
        unwrapSpaceKeyFromAccountRecovery,
        unwrapAccountMasterKey,
        wrapAccountMasterKey,
        encryptVaultBytes,
    } from "$lib/opaqueClient";
    import {
        deriveDataRecoveryVerifier,
        forgetMasterKeyForLocalUnlock,
        lockWebVault,
        rememberMasterKeyForLocalUnlock,
        storeSpaceKey,
        resetWebCredentialsAfterRecovery,
        unlockOrCreateWebVault,
        unlockWebVaultForRecovery,
        unlockWebVaultFromLocalUnlock,
        withActiveMasterKey,
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
            totpRequired: "TOTP is required. Enter a code to complete this sign-in.",
            passwordFailed: "Password sign-in failed.",
            signInFailed: "Sign-in failed",
            passkeyUnsupported: "Passkey login is not supported in this browser.",
            passkeyFailed: "Passkey sign-in failed",
            passkeyUnlocked: "Signed in with passkey and unlocked this approved browser.",
            passkeyApprove: "Passkey authentication succeeded. Enter your password once to approve and unlock this new browser.",
            recoveryRequired: "Username, 24-word data recovery kit, and new password are required.",
            passwordMismatch: "Password confirmation does not match.",
            invalidRecoveryKit: "The 24-word data recovery kit is invalid.",
            invalidRecoveredSpaceKey: "A recovered space key has an invalid length.",
            recoveryCompleted: "Account recovery completed. Your password was reset, TOTP was disabled, and existing sessions, passkeys, and devices were revoked. Sign in with your new password.",
            recoveryFailed: "Account recovery failed",
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
            totpRequired: "Нужен TOTP-код. Введите его, чтобы завершить этот вход.",
            passwordFailed: "Не удалось войти по паролю.",
            signInFailed: "Не удалось войти",
            passkeyUnsupported: "Этот браузер не поддерживает вход с passkey.",
            passkeyFailed: "Не удалось войти с passkey",
            passkeyUnlocked: "Вход с passkey выполнен, одобренный браузер разблокирован.",
            passkeyApprove: "Passkey подтверждён. Один раз введите пароль, чтобы одобрить и разблокировать новый браузер.",
            recoveryRequired: "Введите имя пользователя, recovery kit из 24 слов и новый пароль.",
            passwordMismatch: "Пароли не совпадают.",
            invalidRecoveryKit: "Recovery kit из 24 слов недействителен.",
            invalidRecoveredSpaceKey: "Восстановленный ключ пространства имеет неверную длину.",
            recoveryCompleted: "Аккаунт восстановлен. Пароль сброшен, TOTP отключён, а прежние сессии, passkey и устройства отозваны. Войдите с новым паролем.",
            recoveryFailed: "Не удалось восстановить аккаунт",
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

    let loginUsername = "";
    let loginPassword = "";
    let loginTotpCode = "";
    let recoveryNewPassword = "";
    let recoveryPasswordConfirm = "";
    let recoveryPhrase = "";
    let loadingAction = "";
    let wasOpen = false;
    let pendingOpaqueExportKey: Uint8Array | null = null;
    let allowLocalUnlock = false;

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
                notice: copy.signedIn,
            }));
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
            setNotice(copy.recoveryRequired);
            return;
        }
        if (nextPassword !== recoveryPasswordConfirm) {
            setNotice(copy.passwordMismatch);
            return;
        }

        let recoveredMasterKey: Uint8Array;
        try {
            recoveredMasterKey = await recoveryPhraseToMasterKey(
                recoveryPhrase,
            );
        } catch {
            setNotice(copy.invalidRecoveryKit);
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
            await unlockWebVaultForRecovery(
                $appState.cloudBaseUrl,
                username,
                recoveredMasterKey,
            );
            for (const packageEntry of recovered.space_key_packages) {
                const spaceKey = await unwrapSpaceKeyFromAccountRecovery(
                    recoveredMasterKey,
                    decode(packageEntry.encrypted_key_package),
                );
                if (spaceKey.length !== 32) {
                    throw new Error(copy.invalidRecoveredSpaceKey);
                }
                await storeSpaceKey(
                    packageEntry.space_id,
                    packageEntry.key_epoch,
                    spaceKey,
                );
                spaceKey.fill(0);
            }
            await resetWebCredentialsAfterRecovery(
                $appState.cloudBaseUrl,
                username,
            );
            lockWebVault();

            tokenStore.clear();
            appState.update((state) => ({
                ...state,
                accessToken: null,
                totpContinuationToken: null,
                notice: copy.recoveryCompleted,
            }));
            loginPassword = "";
            loginTotpCode = "";
            recoveryNewPassword = "";
            recoveryPasswordConfirm = "";
            recoveryPhrase = "";
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`${copy.recoveryFailed}: ${message}`);
        } finally {
            recoveredMasterKey.fill(0);
            lockWebVault();
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
                    notice: copy.passkeyApprove,
                }));
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
                notice: copy.passkeyUnlocked,
            }));
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
        recoveryNewPassword = "";
        recoveryPasswordConfirm = "";
        recoveryPhrase = "";
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

<Modal {open} title={copy.title} {onClose}>
    <div class="space-y-3">
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
