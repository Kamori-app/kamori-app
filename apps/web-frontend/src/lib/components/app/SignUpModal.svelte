<script lang="ts">
    import { cloudApi } from "$lib/api/cloud";
    import { appState } from "$lib/stores/app";
    import Button from "$lib/components/ui/Button.svelte";
    import Input from "$lib/components/ui/Input.svelte";
    import Modal from "$lib/components/ui/Modal.svelte";
    import {
        deriveAccountRecoveryKeypair,
        masterKeyToRecoveryPhrase,
        opaqueSignupFinish,
        opaqueSignupStart,
    } from "$lib/opaqueClient";
    import { encode } from "@msgpack/msgpack";
    import { deriveDataRecoveryVerifier } from "$lib/cryptoVault";
    import { wrapAccountMasterKey } from "$lib/opaqueClient";
    import { locale } from "$lib/i18n";
    import { notify } from "$lib/stores/notifications";

    const signupCopy = {
        en: {
            title: "Create account",
            required: "Username, password, and confirmation are required.",
            mismatch: "Password confirmation does not match.",
            saveKit: "Save the 24-word data recovery kit, then confirm its final word.",
            failed: "Account creation failed",
            finalMismatch: "The final recovery word does not match.",
            created: "Account created. Sign in to continue.",
            copied: "Data recovery kit copied. Store it offline.",
            copyFailed: "Copy failed. Select the words and copy them manually.",
            finishFirst: "Finish saving the recovery kit before closing account setup.",
            kitBody: "This is the only offline recovery secret for your encrypted data. Kamori support cannot recreate it. Write it down or print it, keep it away from your password, and never send it to anyone.",
            copyWords: "Copy 24 words",
            confirmWord: "Type word 24 to confirm",
            creating: "Creating…",
            saved: "I saved the kit — create account",
            username: "Username",
            password: "Password",
            confirmPassword: "Confirm password",
            create: "Create account",
            existing: "Already have an account?",
            signIn: "Sign in",
        },
        ru: {
            title: "Создать аккаунт",
            required: "Нужны имя пользователя, пароль и подтверждение пароля.",
            mismatch: "Пароли не совпадают.",
            saveKit: "Сохраните recovery kit из 24 слов и подтвердите последнее слово.",
            failed: "Не удалось создать аккаунт",
            finalMismatch: "Последнее слово recovery kit не совпадает.",
            created: "Аккаунт создан. Теперь войдите.",
            copied: "Recovery kit скопирован. Храните его офлайн.",
            copyFailed: "Не удалось скопировать. Выделите и скопируйте слова вручную.",
            finishFirst: "Сначала завершите сохранение recovery kit.",
            kitBody: "Это единственный офлайн-секрет для восстановления зашифрованных данных. Поддержка Kamori не сможет его воссоздать. Запишите или распечатайте слова, храните отдельно от пароля и никому не отправляйте.",
            copyWords: "Скопировать 24 слова",
            confirmWord: "Введите 24-е слово",
            creating: "Создаём…",
            saved: "Kit сохранён — создать аккаунт",
            username: "Имя пользователя",
            password: "Пароль",
            confirmPassword: "Повторите пароль",
            create: "Создать аккаунт",
            existing: "Уже есть аккаунт?",
            signIn: "Войти",
        },
    } as const;

    $: copy = signupCopy[$locale];

    /**
     * Sign-up modal for web-only registration via OPAQUE flow.
     */
    export let open = false;
    export let onClose: () => void = () => {};
    export let onOpenSignIn: () => void = () => {};
    export let embedded = false;

    let signupUsername = "";
    let signupPassword = "";
    let signupPasswordConfirm = "";
    let recoveryConfirmation = "";
    let recoveryWords: string[] = [];
    let loadingAction = "";
    let formNotice = "";
    let pendingSignup:
        | {
              username: string;
              signupRequestId: string;
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
        formNotice = notice;
        notify(notice, { source: copy.title });
    };

    /**
     * Creates account with OPAQUE start/finish exchange.
     */
    const prepareSignup = async () => {
        const username = signupUsername.trim();
        if (!username || !signupPassword || !signupPasswordConfirm) {
            setNotice(copy.required);
            return;
        }
        if (signupPassword !== signupPasswordConfirm) {
            setNotice(copy.mismatch);
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
            try {
                const encryptedMasterKey = await wrapAccountMasterKey(
                    finish.export_key,
                    masterKey,
                );
                const recoveryIdentity = await deriveAccountRecoveryKeypair(masterKey);
                let publicKeyBundle: Uint8Array;
                try {
                    publicKeyBundle = encode({
                        version: 2,
                        account_recovery_public_key: recoveryIdentity.public_key,
                    });
                } finally {
                    recoveryIdentity.private_key.fill(0);
                }

                pendingSignup = {
                    username,
                    signupRequestId: crypto.randomUUID(),
                    phrase: await masterKeyToRecoveryPhrase(masterKey),
                    opaqueFinishRequest: finish.opaque_finish_request,
                    encryptedMasterKey,
                    publicKeyBundle,
                    recoveryVerifier: await deriveDataRecoveryVerifier(masterKey),
                };
            } finally {
                masterKey.fill(0);
            }
            signupPassword = "";
            signupPasswordConfirm = "";
            setNotice(
                copy.saveKit,
            );
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`${copy.failed}: ${message}`);
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
            setNotice(copy.finalMismatch);
            return;
        }
        setLoading("signup-finish");
        try {
            await cloudApi.signupFinish($appState.cloudBaseUrl, {
                signup_request_id: pendingSignup.signupRequestId,
                username: pendingSignup.username,
                opaque_finish_request: pendingSignup.opaqueFinishRequest,
                encrypted_master_key: pendingSignup.encryptedMasterKey,
                public_key_bundle: pendingSignup.publicKeyBundle,
                recovery_verifier: pendingSignup.recoveryVerifier,
            });
            signupUsername = "";
            recoveryConfirmation = "";
            pendingSignup = undefined;
            onOpenSignIn();
            setNotice(copy.created);
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`${copy.failed}: ${message}`);
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
            setNotice(copy.copied);
        } catch {
            setNotice(copy.copyFailed);
        }
    };

    const requestClose = () => {
        if (pendingSignup) {
            setNotice(
                copy.finishFirst,
            );
            return;
        }
        onClose();
    };
</script>

<Modal {open} title={copy.title} onClose={requestClose} {embedded}>
    {#if formNotice}
        <p class="mb-3 border border-coral/30 bg-coral/10 p-3 text-sm text-slate" role="alert">
            {formNotice}
        </p>
    {/if}
    {#if pendingSignup}
        <div class="space-y-3">
            <p class="text-sm text-slate">
                {copy.kitBody}
            </p>
            <ol
                class="grid grid-cols-2 gap-x-4 gap-y-1 rounded-xl border border-slate/15 bg-white/70 p-3 font-mono text-sm text-slate sm:grid-cols-3"
            >
                {#each recoveryWords as word, index}
                    <li>{index + 1}. {word}</li>
                {/each}
            </ol>
            <Button variant="secondary" on:click={copyRecoveryKit}>
                {copy.copyWords}
            </Button>
            <Input
                bind:value={recoveryConfirmation}
                autocomplete="off"
                placeholder={copy.confirmWord}
            />
            <Button
                on:click={finalizeSignup}
                disabled={loadingAction === "signup-finish"}
            >
                {loadingAction === "signup-finish"
                    ? copy.creating
                    : copy.saved}
            </Button>
        </div>
    {:else}
        <div class="space-y-3">
            <Input bind:value={signupUsername} placeholder={copy.username} />
            <Input
                bind:value={signupPassword}
                type="password"
                placeholder={copy.password}
            />
            <Input
                bind:value={signupPasswordConfirm}
                type="password"
                placeholder={copy.confirmPassword}
            />
            <Button
                on:click={prepareSignup}
                disabled={loadingAction === "signup-opaque"}
            >
                {loadingAction === "signup-opaque"
                    ? copy.creating
                    : copy.create}
            </Button>
            <p class="text-xs text-slate/70">
                {copy.existing}
                <button
                    class="underline underline-offset-2 hover:text-slate"
                    on:click={onOpenSignIn}
                >
                    {copy.signIn}
                </button>
            </p>
        </div>
    {/if}
</Modal>
