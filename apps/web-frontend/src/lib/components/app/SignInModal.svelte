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
            setNotice("Username and password are required.");
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
                    notice: "Signed in with password.",
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
                    "TOTP is required. Enter a TOTP code and submit Sign In again.",
                );
                return;
            }

            setNotice("Password sign-in failed.");
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Sign-in failed: ${message}`);
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
            setNotice("Passkey login is not supported in this browser.");
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
                    ? "Signed in with passkey and unlocked this approved browser."
                    : "Passkey authentication succeeded. Enter your password once to approve and unlock this new browser.",
            }));
            if (localDevice) {
                onClose();
            }
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Passkey sign-in failed: ${message}`);
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

<Modal {open} title="Sign In" {onClose}>
    <div class="space-y-3">
        <Input bind:value={loginUsername} placeholder="Username" />
        <Input
            bind:value={loginPassword}
            type="password"
            placeholder="Password"
        />
        <Input
            bind:value={loginTotpCode}
            placeholder="TOTP or one-time backup code (if enabled)"
        />
        <Button
            on:click={signinWithOpaque}
            disabled={loadingAction === "signin-opaque"}
        >
            {loadingAction === "signin-opaque" ? "Signing In..." : "Sign In"}
        </Button>

        <div class="space-y-2 rounded-xl border border-slate/15 p-3">
            <p
                class="text-xs font-semibold uppercase tracking-wide text-slate/70"
            >
                Account Recovery
            </p>
            <Input
                bind:value={recoveryPhrase}
                autocomplete="off"
                placeholder="24-word data recovery kit"
            />
            <Input
                bind:value={recoveryNewPassword}
                type="password"
                placeholder="New password"
            />
            <Input
                bind:value={recoveryPasswordConfirm}
                type="password"
                placeholder="Confirm new password"
            />
            <Button
                variant="secondary"
                on:click={recoverAccount}
                disabled={loadingAction === "account-recovery"}
            >
                {loadingAction === "account-recovery"
                    ? "Recovering..."
                    : "Recover Account"}
            </Button>
            <p class="text-xs text-slate/70">
                The 24-word data recovery kit authorizes recovery and restores
                your account key. It resets the password, rewraps that key,
                revokes active sessions, and disables TOTP. TOTP backup codes
                are separate and are never required here.
            </p>
        </div>

        <div class="pt-1">
            <p
                class="text-xs font-semibold uppercase tracking-wide text-slate/70"
            >
                Passkey
            </p>
            <div class="mt-2">
                <Button
                    variant="secondary"
                    on:click={signinWithPasskey}
                    disabled={loadingAction === "signin-passkey"}
                >
                    {loadingAction === "signin-passkey"
                        ? "Signing In..."
                        : "Sign in with Passkey"}
                </Button>
            </div>
            <p class="mt-2 text-xs text-slate/70">
                Passkey login uses discoverable authentication and does not
                require username input.
            </p>
        </div>
    </div>
</Modal>
