<script lang="ts">
    import { decode } from "@msgpack/msgpack";
    import { cloudApi } from "$lib/api/cloud";
    import { tokenStore } from "$lib/auth/tokenStore";
    import {
        deriveDataRecoveryVerifier,
        lockWebVault,
        resetWebCredentialsAfterRecovery,
        storeSpaceKey,
        unlockWebVaultForRecovery,
    } from "$lib/cryptoVault";
    import { locale } from "$lib/i18n";
    import {
        opaqueSignupFinish,
        opaqueSignupStart,
        recoveryPhraseToMasterKey,
        unwrapSpaceKeyFromAccountRecovery,
        wrapAccountMasterKey,
    } from "$lib/opaqueClient";
    import { appState } from "$lib/stores/app";
    import { notify } from "$lib/stores/notifications";
    import Button from "$lib/components/ui/Button.svelte";
    import Input from "$lib/components/ui/Input.svelte";

    export let onComplete: () => void = () => {};
    export let onCancel: () => void = () => {};

    let username = "";
    let phrase = "";
    let newPassword = "";
    let passwordConfirmation = "";
    let busy = false;
    let formError = "";

    const copy = {
        en: {
            title: "Recover account",
            body: "Use your 24-word Data Recovery Kit only when you can no longer sign in. Recovery resets the password, revokes every session, passkey, and enrolled device, and disables TOTP.",
            warning: "You will need to sign in again and approve your devices after recovery. TOTP backup codes cannot replace the Data Recovery Kit.",
            username: "Username",
            phrase: "24-word Data Recovery Kit",
            password: "New password",
            confirmation: "Confirm new password",
            action: "Recover account",
            working: "Recovering…",
            cancel: "Back to sign in",
            required: "Username, the 24-word Data Recovery Kit, and both password fields are required.",
            mismatch: "Password confirmation does not match.",
            invalid: "The Data Recovery Kit is invalid.",
            invalidKey: "A recovered space key has an invalid length.",
            failed: "Account recovery failed",
            completed: "Account recovered. Sign in with the new password.",
        },
        ru: {
            title: "Восстановить аккаунт",
            body: "Используйте Data Recovery Kit из 24 слов, только если больше не можете войти. Восстановление меняет пароль, отзывает все сессии, passkey и устройства и отключает TOTP.",
            warning: "После восстановления потребуется снова войти и одобрить устройства. Backup-коды TOTP не заменяют Data Recovery Kit.",
            username: "Имя пользователя",
            phrase: "Data Recovery Kit из 24 слов",
            password: "Новый пароль",
            confirmation: "Повторите новый пароль",
            action: "Восстановить аккаунт",
            working: "Восстанавливаем…",
            cancel: "Вернуться ко входу",
            required: "Введите имя пользователя, Data Recovery Kit из 24 слов и новый пароль дважды.",
            mismatch: "Пароли не совпадают.",
            invalid: "Data Recovery Kit недействителен.",
            invalidKey: "Восстановленный ключ пространства имеет неверную длину.",
            failed: "Не удалось восстановить аккаунт",
            completed: "Аккаунт восстановлен. Войдите с новым паролем.",
        },
    } as const;

    $: text = copy[$locale];

    const fail = (message: string) => {
        formError = message;
        notify(message, { kind: "error", source: text.title });
    };

    const recover = async () => {
        const normalizedUsername = username.trim();
        if (!normalizedUsername || !phrase.trim() || !newPassword || !passwordConfirmation) {
            fail(text.required);
            return;
        }
        if (newPassword !== passwordConfirmation) {
            fail(text.mismatch);
            return;
        }

        let masterKey: Uint8Array;
        try {
            masterKey = await recoveryPhraseToMasterKey(phrase);
        } catch {
            fail(text.invalid);
            return;
        }

        busy = true;
        formError = "";
        try {
            const start = await opaqueSignupStart(newPassword);
            const startResponse = await cloudApi.accountRecoveryStart(
                $appState.cloudBaseUrl,
                {
                    username: normalizedUsername,
                    recovery_verifier: await deriveDataRecoveryVerifier(masterKey),
                    opaque_start_request: start.opaque_start_request,
                },
            );
            const finish = await opaqueSignupFinish(
                start.flow_id,
                newPassword,
                startResponse.opaque_server_message,
            );
            const recovered = await cloudApi.accountRecoveryFinish(
                $appState.cloudBaseUrl,
                {
                    recovery_token: startResponse.recovery_token,
                    opaque_finish_request: finish.opaque_finish_request,
                    encrypted_master_key: await wrapAccountMasterKey(
                        finish.export_key,
                        masterKey,
                    ),
                },
            );
            await unlockWebVaultForRecovery(
                $appState.cloudBaseUrl,
                normalizedUsername,
                masterKey,
            );
            for (const packageEntry of recovered.space_key_packages) {
                const spaceKey = await unwrapSpaceKeyFromAccountRecovery(
                    masterKey,
                    decode(packageEntry.encrypted_key_package),
                );
                if (spaceKey.length !== 32) throw new Error(text.invalidKey);
                await storeSpaceKey(packageEntry.space_id, packageEntry.key_epoch, spaceKey);
                spaceKey.fill(0);
            }
            await resetWebCredentialsAfterRecovery(
                $appState.cloudBaseUrl,
                normalizedUsername,
            );
            tokenStore.clear();
            lockWebVault();
            appState.update((state) => ({
                ...state,
                currentUsername: normalizedUsername,
                accessToken: null,
                totpContinuationToken: null,
            }));
            phrase = "";
            newPassword = "";
            passwordConfirmation = "";
            notify(text.completed, { kind: "success", source: text.title });
            onComplete();
        } catch (error) {
            fail(`${text.failed}: ${error instanceof Error ? error.message : String(error)}`);
        } finally {
            masterKey.fill(0);
            lockWebVault();
            busy = false;
        }
    };
</script>

<section
    class="border border-slate/20 bg-paper p-5 shadow-[6px_6px_0_rgba(23,63,55,0.10)] md:p-7"
    aria-label={text.title}
>
    <p class="text-xs font-semibold uppercase tracking-[0.18em] text-coral">Security reset</p>
    <h1 class="mt-2 font-heading text-2xl font-semibold text-slate">{text.title}</h1>
    <p class="mt-3 text-sm leading-6 text-slate/80">{text.body}</p>
    <p class="mt-3 border-l-4 border-gold bg-sand/45 p-3 text-sm text-slate">{text.warning}</p>

    {#if formError}
        <p class="mt-4 border border-coral/30 bg-coral/10 p-3 text-sm text-slate" role="alert">
            {formError}
        </p>
    {/if}

    <div class="mt-5 space-y-3">
        <Input bind:value={username} autocomplete="username" placeholder={text.username} />
        <textarea
            class="min-h-28 w-full border border-slate/20 bg-white px-3 py-2 text-sm text-slate outline-none focus:border-slate/55"
            bind:value={phrase}
            autocomplete="off"
            placeholder={text.phrase}
        ></textarea>
        <Input bind:value={newPassword} type="password" autocomplete="new-password" placeholder={text.password} />
        <Input bind:value={passwordConfirmation} type="password" autocomplete="new-password" placeholder={text.confirmation} />
        <div class="flex flex-wrap gap-2">
            <Button on:click={recover} disabled={busy}>{busy ? text.working : text.action}</Button>
            <Button variant="ghost" on:click={onCancel}>{text.cancel}</Button>
        </div>
    </div>
</section>
