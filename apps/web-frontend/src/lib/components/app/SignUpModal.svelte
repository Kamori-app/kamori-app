<script lang="ts">
    import { cloudApi } from "$lib/api/cloud";
    import { appState } from "$lib/stores/app";
    import Button from "$lib/components/ui/Button.svelte";
    import Input from "$lib/components/ui/Input.svelte";
    import Modal from "$lib/components/ui/Modal.svelte";
    import {
        masterKeyToRecoveryPhrase,
        opaqueSignupFinish,
        opaqueSignupStart,
    } from "$lib/opaqueClient";
    import { encode } from "@msgpack/msgpack";
    import {
        deriveDataRecoveryVerifier,
        lockWebVault,
        unlockOrCreateWebVault,
    } from "$lib/cryptoVault";
    import { wrapAccountMasterKey } from "$lib/opaqueClient";

    /**
     * Sign-up modal for web-only registration via OPAQUE flow.
     */
    export let open = false;
    export let onClose: () => void = () => {};
    export let onOpenSignIn: () => void = () => {};

    let signupUsername = "";
    let signupPassword = "";
    let signupPasswordConfirm = "";
    let recoveryConfirmation = "";
    let recoveryWords: string[] = [];
    let loadingAction = "";
    let pendingSignup:
        | {
              username: string;
              phrase: string;
              opaqueFinishRequest: Uint8Array;
              encryptedMasterKey: Uint8Array;
              publicKeyBundle: Uint8Array;
              recoveryVerifier: Uint8Array;
          }
        | undefined;

    $: recoveryWords = pendingSignup?.phrase.split(" ") ?? [];

    const randomBytes = (length: number): Uint8Array => {
        const out = new Uint8Array(length);
        crypto.getRandomValues(out);
        return out;
    };

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
     * Creates account with OPAQUE start/finish exchange.
     */
    const prepareSignup = async () => {
        const username = signupUsername.trim();
        if (!username || !signupPassword || !signupPasswordConfirm) {
            setNotice("Username, password, and confirmation are required.");
            return;
        }
        if (signupPassword !== signupPasswordConfirm) {
            setNotice("Password confirmation does not match.");
            return;
        }

        setLoading("signup-opaque");
        try {
            const start = await opaqueSignupStart(signupPassword);
            const startResponse = await cloudApi.signupStart(
                $appState.cloudBaseUrl,
                {
                    username,
                    opaque_start_request: start.opaque_start_request,
                },
            );

            const finish = await opaqueSignupFinish(
                start.flow_id,
                signupPassword,
                startResponse.opaque_server_message,
            );

            const masterKey = randomBytes(32);
            const encryptedMasterKey = await wrapAccountMasterKey(
                finish.export_key,
                masterKey,
            );
            const device = await unlockOrCreateWebVault(username, masterKey);
            const publicKeyBundle = encode({
                version: 1,
                device_id: device.deviceId,
                signing_public_key: device.identity.signing_public_key,
                hpke_public_key: device.identity.hpke_public_key,
            });

            pendingSignup = {
                username,
                phrase: await masterKeyToRecoveryPhrase(masterKey),
                opaqueFinishRequest: finish.opaque_finish_request,
                encryptedMasterKey,
                publicKeyBundle,
                recoveryVerifier: await deriveDataRecoveryVerifier(masterKey),
            };
            masterKey.fill(0);
            signupPassword = "";
            signupPasswordConfirm = "";
            setNotice(
                "Save the 24-word data recovery kit, then confirm its final word.",
            );
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Sign-up failed: ${message}`);
        } finally {
            clearLoading();
        }
    };

    const finalizeSignup = async () => {
        if (!pendingSignup) {
            return;
        }
        const expected = recoveryWords.at(-1) ?? "";
        if (recoveryConfirmation.trim().toLowerCase() !== expected) {
            setNotice("The final recovery word does not match.");
            return;
        }
        setLoading("signup-finish");
        try {
            await cloudApi.signupFinish($appState.cloudBaseUrl, {
                username: pendingSignup.username,
                opaque_finish_request: pendingSignup.opaqueFinishRequest,
                encrypted_master_key: pendingSignup.encryptedMasterKey,
                public_key_bundle: pendingSignup.publicKeyBundle,
                recovery_verifier: pendingSignup.recoveryVerifier,
            });
            signupUsername = "";
            recoveryConfirmation = "";
            pendingSignup = undefined;
            lockWebVault();
            onOpenSignIn();
            setNotice("Account created. Sign in to continue.");
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Sign-up failed: ${message}`);
        } finally {
            clearLoading();
        }
    };

    const copyRecoveryKit = async () => {
        if (!pendingSignup) {
            return;
        }
        try {
            await navigator.clipboard.writeText(pendingSignup.phrase);
            setNotice("Data recovery kit copied. Store it offline.");
        } catch {
            setNotice("Copy failed. Select the words and copy them manually.");
        }
    };

    const requestClose = () => {
        if (pendingSignup) {
            setNotice(
                "Finish saving the recovery kit before closing account setup.",
            );
            return;
        }
        onClose();
    };
</script>

<Modal {open} title="Sign Up" onClose={requestClose}>
    {#if pendingSignup}
        <div class="space-y-3">
            <p class="text-sm text-slate">
                This is the only offline recovery secret for your encrypted
                data. Kamori support cannot recreate it. Write it down or print
                it, keep it away from your password, and never send it to
                anyone.
            </p>
            <ol
                class="grid grid-cols-2 gap-x-4 gap-y-1 rounded-xl border border-slate/15 bg-white/70 p-3 font-mono text-sm text-slate sm:grid-cols-3"
            >
                {#each recoveryWords as word, index}
                    <li>{index + 1}. {word}</li>
                {/each}
            </ol>
            <Button variant="secondary" on:click={copyRecoveryKit}>
                Copy 24 words
            </Button>
            <Input
                bind:value={recoveryConfirmation}
                autocomplete="off"
                placeholder="Type word 24 to confirm"
            />
            <Button
                on:click={finalizeSignup}
                disabled={loadingAction === "signup-finish"}
            >
                {loadingAction === "signup-finish"
                    ? "Creating..."
                    : "I saved the kit — create account"}
            </Button>
        </div>
    {:else}
        <div class="space-y-3">
            <Input bind:value={signupUsername} placeholder="Username" />
            <Input
                bind:value={signupPassword}
                type="password"
                placeholder="Password"
            />
            <Input
                bind:value={signupPasswordConfirm}
                type="password"
                placeholder="Confirm password"
            />
            <Button
                on:click={prepareSignup}
                disabled={loadingAction === "signup-opaque"}
            >
                {loadingAction === "signup-opaque"
                    ? "Creating..."
                    : "Create Account"}
            </Button>
            <p class="text-xs text-slate/70">
                Already signed up?
                <button
                    class="underline underline-offset-2 hover:text-slate"
                    on:click={onOpenSignIn}
                >
                    Click here to Sign In
                </button>
            </p>
        </div>
    {/if}
</Modal>
