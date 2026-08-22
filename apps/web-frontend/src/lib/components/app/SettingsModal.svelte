<script lang="ts">
    import { encode } from "@msgpack/msgpack";
    import {
        cloudApi,
        type DeletionStatusResponse,
        type DeviceSummary,
        type OwnershipResourceKind,
        type OwnershipTransferOffer,
        type SessionSummary,
        type SpaceMemberSummary,
        type SpaceSummary,
        type WorkspaceMember,
    } from "$lib/api/cloud";
    import { runWithAccessRefreshRetry } from "$lib/auth/session-flow.js";
    import {
        generateQrSvg,
        masterKeyToRecoveryPhrase,
        opaqueSigninFinish,
        opaqueSigninStart,
        opaqueSignupFinish,
        opaqueSignupStart,
        wrapAccountMasterKey,
        wrapSpaceKeyForDevice,
        decryptVaultBytes,
    } from "$lib/opaqueClient";
    import {
        getActiveWebDevice,
        deleteWebVaultAccount,
        loadSpaceKey,
        lockWebVault,
        withActiveMasterKey,
    } from "$lib/cryptoVault";
    import { tokenStore } from "$lib/auth/tokenStore";
    import { appState } from "$lib/stores/app";
    import Button from "$lib/components/ui/Button.svelte";
    import Input from "$lib/components/ui/Input.svelte";
    import Modal from "$lib/components/ui/Modal.svelte";
    import { locale } from "$lib/i18n";

    const ruCopy: Record<string, string> = {
        "Web Settings": "Настройки веб-приложения", "Cloud Base URL": "Адрес сервиса Kamori", "Save Settings": "Сохранить настройки",
        "Ownership transfers": "Передача владения", "Refresh": "Обновить", "Accept ownership": "Принять владение", "Decline": "Отклонить",
        "Pending offers": "Ожидающие предложения", "Cancel offer": "Отменить предложение", "Offer ownership": "Предложить владение",
        "Privacy choices": "Настройки приватности", "Product analytics": "Аналитика продукта", "Crash reports": "Отчёты о сбоях", "Product email": "Новости продукта",
        "Save privacy choices": "Сохранить выбор", "Security: Devices": "Безопасность: устройства", "Approve encrypted access": "Разрешить доступ к шифрованным данным",
        "Security: Sessions": "Безопасность: сессии", "Sign in to review sessions.": "Войдите, чтобы просмотреть сессии.", "No sessions found.": "Сессий нет.",
        "Last used": "Последнее использование", "Revoke": "Отозвать", "Security: Data Recovery Kit": "Безопасность: recovery kit данных",
        "Reveal 24-word kit": "Показать набор из 24 слов", "Copy kit": "Копировать набор", "Sign in to reveal it.": "Войдите, чтобы показать его.",
        "Security: TOTP": "Безопасность: TOTP", "Sign in to manage TOTP.": "Войдите для управления TOTP.", "Manual Entry Key": "Ключ для ручного ввода",
        "Copy Manual Key": "Копировать ключ", "Copy URI": "Копировать URI", "Security: Password": "Безопасность: пароль",
        "Sign in to change password.": "Войдите, чтобы изменить пароль.", "Current password": "Текущий пароль", "New password": "Новый пароль",
        "Confirm new password": "Повторите новый пароль", "Current TOTP or backup code": "Текущий TOTP или backup-код", "Delete account": "Удалить аккаунт",
        "Refresh status": "Обновить статус", "Current TOTP code, if enabled": "Текущий TOTP-код, если включён",
    };
    const t = (english: string) => $locale === "ru" ? (ruCopy[english] ?? english) : english;

    /**
     * Minimal settings modal used to override cloud API base URL.
     */
    export let open = false;
    export let onClose: () => void = () => {};

    let settingsCloudBaseUrl = "";
    let wasOpen = false;
    let consentLoading = false;
    let consentProductAnalytics = false;
    let consentCrashReports = false;
    let consentMarketing = false;
    let totpLoading = false;
    let totpBusyAction = "";
    let totpAvailable = false;
    let totpEnabled = false;
    let totpRecoveryCodesRemaining = 0;
    let totpRecoveryCodes: string[] = [];
    let totpManualEntryKey = "";
    let totpOtpAuthUri = "";
    let totpQrDataUrl = "";
    let totpSetupCode = "";
    let totpDisableCode = "";
    let passwordChangeNew = "";
    let passwordChangeConfirm = "";
    let passwordChangeCurrent = "";
    let passwordChangeTotp = "";
    let dataRecoveryKit = "";
    let sessions: SessionSummary[] = [];
    let deviceApprovals: Array<{
        device: DeviceSummary;
        label: string;
        missingSpaces: SpaceSummary[];
    }> = [];
    let incomingOwnershipOffers: OwnershipTransferOffer[] = [];
    let outgoingOwnershipOffers: OwnershipTransferOffer[] = [];
    let ownedResources: Array<{
        kind: OwnershipResourceKind;
        id: string;
        label: string;
        members: Array<SpaceMemberSummary | WorkspaceMember>;
    }> = [];
    let deletionStatus: DeletionStatusResponse | null = null;
    let accountDeletePassword = "";
    let accountDeleteTotp = "";
    let accountDeleteConfirmation = "";

    const setNotice = (notice: string) => {
        appState.update((state) => ({ ...state, notice }));
    };

    const clearTotpSetupDraft = () => {
        totpManualEntryKey = "";
        totpOtpAuthUri = "";
        totpQrDataUrl = "";
        totpSetupCode = "";
    };

    const clearTotpRecoveryCodes = () => {
        totpRecoveryCodes = [];
    };

    const clearPasswordChangeDraft = () => {
        passwordChangeNew = "";
        passwordChangeConfirm = "";
        passwordChangeCurrent = "";
        passwordChangeTotp = "";
    };

    const setBusyAction = (value: string) => {
        totpBusyAction = value;
    };

    const clearBusyAction = () => {
        totpBusyAction = "";
    };

    const resetConsents = () => {
        consentProductAnalytics = false;
        consentCrashReports = false;
        consentMarketing = false;
    };

    const persistRotatedAccessToken = (accessToken: string) => {
        tokenStore.setAccessToken(accessToken);
        appState.update((state) => ({
            ...state,
            accessToken,
        }));
    };

    async function withAccessRetry<T>(
        operation: (accessToken: string) => Promise<T>,
    ): Promise<T> {
        return runWithAccessRefreshRetry({
            getAccessToken: () => tokenStore.getAccessToken(),
            operation,
            refresh: () => cloudApi.refresh($appState.cloudBaseUrl),
            onAccessTokenRotated: persistRotatedAccessToken,
            onRefreshUnauthorized: () => {
                tokenStore.clear();
                appState.update((state) => ({
                    ...state,
                    accessToken: null,
                    preauthToken: null,
                }));
            },
        });
    }

    const copyText = async (value: string, label: string) => {
        try {
            await navigator.clipboard.writeText(value);
            setNotice(`${label} copied.`);
        } catch {
            setNotice(`Unable to copy ${label.toLowerCase()}.`);
        }
    };

    const loadTotpStatus = async () => {
        if (!tokenStore.getAccessToken()) {
            totpAvailable = false;
            totpEnabled = false;
            totpRecoveryCodesRemaining = 0;
            clearTotpSetupDraft();
            clearTotpRecoveryCodes();
            return;
        }

        totpLoading = true;
        try {
            const status = await withAccessRetry((accessToken) =>
                cloudApi.totpStatus($appState.cloudBaseUrl, accessToken),
            );
            totpAvailable = Boolean(status.available);
            totpEnabled = Boolean(status.enabled);
            totpRecoveryCodesRemaining = Number(
                status.recovery_codes_remaining || 0,
            );
            if (totpEnabled) {
                clearTotpSetupDraft();
            }
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Failed to load TOTP status: ${message}`);
        } finally {
            totpLoading = false;
        }
    };

    const loadConsents = async () => {
        if (!tokenStore.getAccessToken()) {
            resetConsents();
            return;
        }
        consentLoading = true;
        try {
            const settings = await withAccessRetry((accessToken) =>
                cloudApi.getConsentSettings(
                    $appState.cloudBaseUrl,
                    accessToken,
                ),
            );
            consentProductAnalytics = settings.product_analytics;
            consentCrashReports = settings.crash_reports;
            consentMarketing = settings.marketing;
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Failed to load privacy choices: ${message}`);
        } finally {
            consentLoading = false;
        }
    };

    const saveConsents = async () => {
        if (!tokenStore.getAccessToken()) {
            setNotice("Sign in to save privacy choices.");
            return;
        }
        setBusyAction("consents");
        try {
            const settings = await withAccessRetry((accessToken) =>
                cloudApi.updateConsentSettings(
                    $appState.cloudBaseUrl,
                    {
                        product_analytics: consentProductAnalytics,
                        crash_reports: consentCrashReports,
                        marketing: consentMarketing,
                    },
                    accessToken,
                ),
            );
            consentProductAnalytics = settings.product_analytics;
            consentCrashReports = settings.crash_reports;
            consentMarketing = settings.marketing;
            setNotice("Privacy choices saved. You can revoke them at any time.");
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Failed to save privacy choices: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const startTotpSetup = async () => {
        if (!tokenStore.getAccessToken()) {
            setNotice("Sign in first.");
            return;
        }
        if (!totpAvailable) {
            setNotice("TOTP is disabled on server.");
            return;
        }

        setBusyAction("totp-start");
        try {
            clearTotpRecoveryCodes();
            const response = await withAccessRetry((accessToken) =>
                cloudApi.totpSetupStart($appState.cloudBaseUrl, accessToken),
            );
            const qrSvg = await generateQrSvg(response.otpauth_uri);
            totpManualEntryKey = response.manual_entry_key;
            totpOtpAuthUri = response.otpauth_uri;
            totpQrDataUrl = `data:image/svg+xml;utf8,${encodeURIComponent(qrSvg)}`;
            totpSetupCode = "";
            setNotice(
                "TOTP setup started. Scan QR code (or use manual key), then confirm with code.",
            );
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Failed to start TOTP setup: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const finishTotpSetup = async () => {
        if (!totpManualEntryKey) {
            setNotice("Start TOTP setup first.");
            return;
        }

        const code = totpSetupCode.trim();
        if (!code) {
            setNotice("Enter TOTP code to confirm setup.");
            return;
        }

        setBusyAction("totp-finish");
        try {
            const response = await withAccessRetry((accessToken) =>
                cloudApi.totpSetupFinish(
                    $appState.cloudBaseUrl,
                    {
                        manual_entry_key: totpManualEntryKey,
                        code,
                    },
                    accessToken,
                ),
            );
            totpEnabled = Boolean(response.enabled);
            totpRecoveryCodes = response.recovery_codes ?? [];
            totpRecoveryCodesRemaining = totpRecoveryCodes.length;
            clearTotpSetupDraft();
            setNotice("TOTP enabled. Save the one-time TOTP backup codes now.");
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Failed to enable TOTP: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const disableTotp = async () => {
        const code = totpDisableCode.trim();
        if (!code) {
            setNotice("Enter current TOTP code to disable.");
            return;
        }

        setBusyAction("totp-disable");
        try {
            const response = await withAccessRetry((accessToken) =>
                cloudApi.totpDisable(
                    $appState.cloudBaseUrl,
                    { code },
                    accessToken,
                ),
            );
            totpEnabled = Boolean(response.enabled);
            totpDisableCode = "";
            clearTotpSetupDraft();
            setNotice("TOTP disabled.");
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Failed to disable TOTP: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const changePassword = async () => {
        if (!tokenStore.getAccessToken()) {
            setNotice("Sign in first.");
            return;
        }

        const nextPassword = passwordChangeNew;
        if (!passwordChangeCurrent || !nextPassword || !passwordChangeConfirm) {
            setNotice("Current password, new password, and confirmation are required.");
            return;
        }
        if (nextPassword !== passwordChangeConfirm) {
            setNotice("Password confirmation does not match.");
            return;
        }

        setBusyAction("password-change");
        try {
            const reauthStart = await opaqueSigninStart(passwordChangeCurrent);
            const reauthStartResponse = await withAccessRetry((accessToken) =>
                cloudApi.reauthStart(
                    $appState.cloudBaseUrl,
                    reauthStart.opaque_start_request,
                    "change_password",
                    accessToken,
                ),
            );
            if (reauthStartResponse.totp_required && !passwordChangeTotp.trim()) {
                throw new Error("Current two-factor code is required.");
            }
            const reauthFinish = await opaqueSigninFinish(
                reauthStart.flow_id,
                passwordChangeCurrent,
                reauthStartResponse.opaque_server_message,
            );
            const reauth = await withAccessRetry((accessToken) =>
                cloudApi.reauthFinish(
                    $appState.cloudBaseUrl,
                    reauthStartResponse.opaque_flow_id,
                    reauthFinish.opaque_finish_request,
                    reauthStartResponse.totp_required
                        ? passwordChangeTotp.trim()
                        : null,
                    "change_password",
                    accessToken,
                ),
            );
            const start = await opaqueSignupStart(nextPassword);
            const startResponse = await withAccessRetry((accessToken) =>
                cloudApi.passwordChangeStart(
                    $appState.cloudBaseUrl,
                    { opaque_start_request: start.opaque_start_request },
                    accessToken,
                ),
            );
            const finish = await opaqueSignupFinish(
                start.flow_id,
                nextPassword,
                startResponse.opaque_server_message,
            );
            const encryptedMasterKey = await withActiveMasterKey(
                (masterKey) => wrapAccountMasterKey(finish.export_key, masterKey),
            );
            await withAccessRetry((accessToken) =>
                cloudApi.passwordChangeFinish(
                    $appState.cloudBaseUrl,
                    {
                        reauth_token: reauth.reauth_token,
                        opaque_finish_request: finish.opaque_finish_request,
                        encrypted_master_key: encryptedMasterKey,
                    },
                    accessToken,
                ),
            );

            const oldAccess = tokenStore.getAccessToken();
            if (oldAccess) {
                try {
                    await cloudApi.logout($appState.cloudBaseUrl, oldAccess);
                } catch {
                    // best effort cookie cleanup
                }
            }

            tokenStore.clear();
            lockWebVault();
            appState.update((state) => ({
                ...state,
                accessToken: null,
                preauthToken: null,
                notice: "Password changed. Sign in again.",
            }));
            clearPasswordChangeDraft();
            clearTotpRecoveryCodes();
            clearTotpSetupDraft();
            totpRecoveryCodesRemaining = 0;
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Password change failed: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const regenerateRecoveryCodes = async () => {
        setBusyAction("account-recovery-regenerate");
        try {
            const response = await withAccessRetry((accessToken) =>
                cloudApi.accountRecoveryCodesRegenerate(
                    $appState.cloudBaseUrl,
                    accessToken,
                ),
            );
            totpRecoveryCodes = response.recovery_codes ?? [];
            totpRecoveryCodesRemaining = totpRecoveryCodes.length;
            setNotice(
                "TOTP backup codes regenerated. Old unused codes are revoked.",
            );
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(
                `Failed to regenerate TOTP backup codes: ${message}`,
            );
        } finally {
            clearBusyAction();
        }
    };

    const revealDataRecoveryKit = async () => {
        try {
            dataRecoveryKit = await withActiveMasterKey(
                (masterKey) => masterKeyToRecoveryPhrase(masterKey),
            );
            setNotice(
                "Data recovery kit revealed. Keep it private and store it offline.",
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Unable to reveal data recovery kit: ${message}`);
        }
    };

    const loadSessions = async () => {
        if (!tokenStore.getAccessToken()) {
            sessions = [];
            return;
        }
        try {
            const response = await withAccessRetry((accessToken) =>
                cloudApi.listSessions($appState.cloudBaseUrl, accessToken),
            );
            sessions = response.sessions;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Failed to load sessions: ${message}`);
        }
    };

    const revokeSession = async (sessionId: string) => {
        setBusyAction(`session-${sessionId}`);
        try {
            await withAccessRetry((accessToken) =>
                cloudApi.revokeSession(
                    $appState.cloudBaseUrl,
                    sessionId,
                    accessToken,
                ),
            );
            await loadSessions();
            setNotice("Session revoked.");
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Failed to revoke session: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const loadDevices = async () => {
        if (!tokenStore.getAccessToken()) {
            deviceApprovals = [];
            return;
        }
        try {
            const [deviceResponse, spaceResponse] = await Promise.all([
                withAccessRetry((accessToken) =>
                    cloudApi.listDevices($appState.cloudBaseUrl, accessToken),
                ),
                withAccessRetry((accessToken) =>
                    cloudApi.listSpaces($appState.cloudBaseUrl, accessToken),
                ),
            ]);
            const activeDeviceId = getActiveWebDevice().deviceId;
            deviceApprovals = await Promise.all(
                deviceResponse.devices.map(async (device) => {
                    let label = `${device.platform} · ${device.device_id.slice(0, 8)}`;
                    try {
                        label = new TextDecoder().decode(
                            await withActiveMasterKey((masterKey) =>
                                decryptVaultBytes(
                                    masterKey,
                                    device.encrypted_name,
                                ),
                            ),
                        );
                    } catch {
                        // Keep non-sensitive platform/id fallback.
                    }
                    const missingSpaces =
                        device.device_id === activeDeviceId
                            ? []
                            : spaceResponse.spaces.filter(
                                  (space) =>
                                      !space.device_key_packages.some(
                                          (item) =>
                                              item.device_id ===
                                                  device.device_id &&
                                              item.key_epoch === space.key_epoch,
                                      ),
                              );
                    return { device, label, missingSpaces };
                }),
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Failed to load devices: ${message}`);
        }
    };

    const approveDevice = async (entry: (typeof deviceApprovals)[number]) => {
        setBusyAction(`device-${entry.device.device_id}`);
        try {
            let approved = 0;
            for (const space of entry.missingSpaces) {
                const key = await loadSpaceKey(space.space_id, space.key_epoch);
                if (!key) continue;
                const encryptedKeyPackage = encode(
                    await wrapSpaceKeyForDevice(
                        key,
                        entry.device.hpke_public_key,
                    ),
                );
                await withAccessRetry((accessToken) =>
                    cloudApi.putDeviceKeyPackage(
                        $appState.cloudBaseUrl,
                        space.space_id,
                        {
                            device_id: entry.device.device_id,
                            key_epoch: space.key_epoch,
                            encrypted_key_package: encryptedKeyPackage,
                        },
                        accessToken,
                    ),
                );
                approved += 1;
            }
            await loadDevices();
            setNotice(
                approved > 0
                    ? `Approved ${entry.label} for ${approved} encrypted space(s).`
                    : "No locally unlocked space keys were available.",
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Device approval failed: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const loadOwnership = async () => {
        if (!tokenStore.getAccessToken()) {
            incomingOwnershipOffers = [];
            outgoingOwnershipOffers = [];
            ownedResources = [];
            return;
        }
        try {
            const [incoming, outgoing, spaces, workspaces] = await Promise.all([
                withAccessRetry((accessToken) =>
                    cloudApi.listIncomingOwnershipTransfers(
                        $appState.cloudBaseUrl,
                        accessToken,
                    ),
                ),
                withAccessRetry((accessToken) =>
                    cloudApi.listOutgoingOwnershipTransfers(
                        $appState.cloudBaseUrl,
                        accessToken,
                    ),
                ),
                withAccessRetry((accessToken) =>
                    cloudApi.listSpaces($appState.cloudBaseUrl, accessToken),
                ),
                withAccessRetry((accessToken) =>
                    cloudApi.listWorkspaces(
                        $appState.cloudBaseUrl,
                        accessToken,
                    ),
                ),
            ]);
            incomingOwnershipOffers = incoming.offers;
            outgoingOwnershipOffers = outgoing.offers;
            const resources: typeof ownedResources = [];
            for (const space of spaces.spaces.filter(
                (item) => item.role === "owner",
            )) {
                const response = await withAccessRetry((accessToken) =>
                    cloudApi.listSpaceMembers(
                        $appState.cloudBaseUrl,
                        space.space_id,
                        accessToken,
                    ),
                );
                resources.push({
                    kind: "security_space",
                    id: space.space_id,
                    label:
                        $appState.collections.find(
                            (collection) => collection.id === space.space_id,
                        )?.name ?? `Encrypted space ${space.space_id.slice(0, 8)}`,
                    members: response.members.filter(
                        (member) => member.role !== "owner",
                    ),
                });
            }
            for (const workspace of workspaces.workspaces.filter(
                (item) => item.kind === "team" && item.role === "owner",
            )) {
                const response = await withAccessRetry((accessToken) =>
                    cloudApi.listWorkspaceMembers(
                        $appState.cloudBaseUrl,
                        workspace.workspace_id,
                        accessToken,
                    ),
                );
                resources.push({
                    kind: "workspace",
                    id: workspace.workspace_id,
                    label: `Team workspace ${workspace.workspace_id.slice(0, 8)}`,
                    members: response.members.filter(
                        (member) => member.role !== "owner",
                    ),
                });
            }
            ownedResources = resources;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Failed to load ownership controls: ${message}`);
        }
    };

    const offerOwnership = async (
        resource: (typeof ownedResources)[number],
        target: SpaceMemberSummary | WorkspaceMember,
    ) => {
        setBusyAction(`ownership-${resource.id}-${target.user_id}`);
        try {
            await withAccessRetry((accessToken) =>
                cloudApi.createOwnershipTransfer(
                    $appState.cloudBaseUrl,
                    resource.kind,
                    resource.id,
                    target.user_id,
                    accessToken,
                ),
            );
            await loadOwnership();
            setNotice(
                `Ownership offer sent to ${target.username}. It expires in 24 hours and only the recipient can accept it.`,
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Ownership transfer failed: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const resolveOwnershipOffer = async (
        offer: OwnershipTransferOffer,
        accept: boolean,
    ) => {
        setBusyAction(`ownership-offer-${offer.transfer_id}`);
        try {
            await withAccessRetry((accessToken) =>
                accept
                    ? cloudApi.acceptOwnershipTransfer(
                          $appState.cloudBaseUrl,
                          offer.transfer_id,
                          accessToken,
                      )
                    : cloudApi.cancelOwnershipTransfer(
                          $appState.cloudBaseUrl,
                          offer.transfer_id,
                          accessToken,
                      ),
            );
            await Promise.all([loadOwnership(), loadDeletionStatus()]);
            setNotice(
                accept
                    ? "Ownership transfer accepted."
                    : "Ownership transfer declined.",
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Unable to resolve ownership offer: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const loadDeletionStatus = async () => {
        if (!tokenStore.getAccessToken()) {
            deletionStatus = null;
            return;
        }
        try {
            deletionStatus = await withAccessRetry((accessToken) =>
                cloudApi.getDeletionStatus(
                    $appState.cloudBaseUrl,
                    accessToken,
                ),
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Failed to check account deletion: ${message}`);
        }
    };

    const deleteAccount = async () => {
        const username = $appState.currentUsername;
        if (!username || !accountDeletePassword) {
            setNotice("Password is required to delete the account.");
            return;
        }
        if (accountDeleteConfirmation !== `DELETE ${username}`) {
            setNotice(`Type DELETE ${username} exactly to confirm.`);
            return;
        }
        if (!deletionStatus?.can_delete) {
            setNotice("Transfer shared resources before deleting the account.");
            return;
        }
        setBusyAction("account-delete");
        try {
            const start = await opaqueSigninStart(accountDeletePassword);
            const startResponse = await withAccessRetry((accessToken) =>
                cloudApi.reauthStart(
                    $appState.cloudBaseUrl,
                    start.opaque_start_request,
                    "delete_account",
                    accessToken,
                ),
            );
            if (startResponse.totp_required && !accountDeleteTotp.trim()) {
                throw new Error("Current two-factor code is required.");
            }
            const finish = await opaqueSigninFinish(
                start.flow_id,
                accountDeletePassword,
                startResponse.opaque_server_message,
            );
            const reauth = await withAccessRetry((accessToken) =>
                cloudApi.reauthFinish(
                    $appState.cloudBaseUrl,
                    startResponse.opaque_flow_id,
                    finish.opaque_finish_request,
                    startResponse.totp_required
                        ? accountDeleteTotp.trim()
                        : null,
                    "delete_account",
                    accessToken,
                ),
            );
            await withAccessRetry((accessToken) =>
                cloudApi.deleteAccount(
                    $appState.cloudBaseUrl,
                    reauth.reauth_token,
                    accountDeleteConfirmation,
                    accessToken,
                ),
            );
            await deleteWebVaultAccount(username);
            tokenStore.clear();
            appState.update((state) => ({
                ...state,
                currentUsername: "",
                accessToken: null,
                preauthToken: null,
                collections: [],
                syncedItemsTotal: 0,
                lastSyncedSeq: 0,
                notice: "Account and local encrypted browser data deleted.",
            }));
            accountDeletePassword = "";
            accountDeleteTotp = "";
            accountDeleteConfirmation = "";
            onClose();
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Account deletion failed: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const formatSessionTime = (unixMs?: number | null): string =>
        unixMs ? new Date(unixMs).toLocaleString() : "Never";

    /**
     * Persists normalized base URL and notifies user.
     */
    const saveCloudBaseUrl = () => {
        const next = settingsCloudBaseUrl.trim() || "http://127.0.0.1:3000";
        tokenStore.clear();
        lockWebVault();
        appState.update((state) => ({
            ...state,
            cloudBaseUrl: next,
            accessToken: null,
            preauthToken: null,
        }));
        settingsCloudBaseUrl = next;
        clearTotpSetupDraft();
        clearTotpRecoveryCodes();
        clearPasswordChangeDraft();
        totpRecoveryCodesRemaining = 0;
        totpDisableCode = "";
        onClose();
        setNotice("Cloud base URL updated. Sign in again.");
    };

    $: if (open && !wasOpen) {
        settingsCloudBaseUrl = $appState.cloudBaseUrl;
        void loadTotpStatus();
        void loadSessions();
        void loadDevices();
        void loadConsents();
        void loadOwnership();
        void loadDeletionStatus();
        wasOpen = true;
    }

    $: if (!open && wasOpen) {
        clearTotpSetupDraft();
        clearTotpRecoveryCodes();
        clearPasswordChangeDraft();
        totpRecoveryCodesRemaining = 0;
        totpDisableCode = "";
        clearBusyAction();
        dataRecoveryKit = "";
        sessions = [];
        deviceApprovals = [];
        resetConsents();
        incomingOwnershipOffers = [];
        outgoingOwnershipOffers = [];
        ownedResources = [];
        deletionStatus = null;
        accountDeletePassword = "";
        accountDeleteTotp = "";
        accountDeleteConfirmation = "";
        wasOpen = false;
    }
</script>

<Modal {open} title={t("Web Settings")} {onClose}>
    <div class="space-y-4">
        <p class="text-xs font-semibold uppercase tracking-wide text-slate/70">
            {t("Cloud Base URL")}
        </p>
        <Input bind:value={settingsCloudBaseUrl} />
        <div class="pt-1">
            <Button on:click={saveCloudBaseUrl}>{t("Save Settings")}</Button>
        </div>

        <div class="border-t border-slate/15 pt-3">
            <div class="flex items-center justify-between gap-2">
                <p
                    class="text-xs font-semibold uppercase tracking-wide text-slate/70"
                >
                    {t("Ownership transfers")}
                </p>
                {#if $appState.accessToken}
                    <Button variant="ghost" on:click={loadOwnership}>{t("Refresh")}</Button>
                {/if}
            </div>
            <p class="mt-2 text-xs text-slate/70">
                Ownership never changes silently. An active member must accept
                the offer within 24 hours. Space storage is charged to the new
                owner only after acceptance.
            </p>
            {#if incomingOwnershipOffers.length > 0}
                <div class="mt-3 space-y-2">
                    {#each incomingOwnershipOffers as offer}
                        <div
                            class="rounded-xl border border-slate/15 bg-white/70 p-3"
                        >
                            <p class="text-xs font-semibold text-slate">
                                {offer.resource_kind === "security_space"
                                    ? "Encrypted space"
                                    : "Team workspace"}
                                · {offer.resource_id.slice(0, 8)}
                            </p>
                            <p class="mt-1 text-xs text-slate/65">
                                Offered by {offer.current_owner_username} · expires
                                {new Date(
                                    offer.expires_at_unix_ms,
                                ).toLocaleString()}
                            </p>
                            <div class="mt-2 flex flex-wrap gap-2">
                                <Button
                                    variant="secondary"
                                    on:click={() =>
                                        resolveOwnershipOffer(offer, true)}
                                    disabled={totpBusyAction ===
                                        `ownership-offer-${offer.transfer_id}`}
                                >{t("Accept ownership")}</Button>
                                <Button
                                    variant="ghost"
                                    on:click={() =>
                                        resolveOwnershipOffer(offer, false)}
                                    disabled={totpBusyAction ===
                                        `ownership-offer-${offer.transfer_id}`}
                                >{t("Decline")}</Button>
                            </div>
                        </div>
                    {/each}
                </div>
            {/if}
            {#if outgoingOwnershipOffers.length > 0}
                <div class="mt-3 space-y-2">
                    <p class="text-xs font-semibold text-slate">{t("Pending offers")}</p>
                    {#each outgoingOwnershipOffers as offer}
                        <div
                            class="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-slate/15 bg-white/70 p-3"
                        >
                            <span class="text-xs text-slate">
                                {offer.resource_kind === "security_space"
                                    ? "Encrypted space"
                                    : "Team workspace"}
                                {offer.resource_id.slice(0, 8)} · recipient
                                {offer.target_user_id.slice(0, 8)} · expires
                                {new Date(
                                    offer.expires_at_unix_ms,
                                ).toLocaleString()}
                            </span>
                            <Button
                                variant="ghost"
                                on:click={() =>
                                    resolveOwnershipOffer(offer, false)}
                                disabled={totpBusyAction ===
                                    `ownership-offer-${offer.transfer_id}`}
                            >{t("Cancel offer")}</Button>
                        </div>
                    {/each}
                </div>
            {/if}
            {#if ownedResources.length > 0}
                <div class="mt-3 space-y-2">
                    {#each ownedResources as resource}
                        <div
                            class="rounded-xl border border-slate/15 bg-white/70 p-3"
                        >
                            <p class="text-xs font-semibold text-slate">
                                {resource.label}
                            </p>
                            {#if resource.members.length === 0}
                                <p class="mt-1 text-xs text-slate/65">
                                    Invite a member before transferring ownership.
                                </p>
                            {:else}
                                <div class="mt-2 space-y-2">
                                    {#each resource.members as member}
                                        <div
                                            class="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-sand/50 p-2"
                                        >
                                            <span class="text-xs text-slate">
                                                {member.username} · {member.role}
                                            </span>
                                            <Button
                                                variant="ghost"
                                                on:click={() =>
                                                    offerOwnership(
                                                        resource,
                                                        member,
                                                    )}
                                                disabled={totpBusyAction ===
                                                    `ownership-${resource.id}-${member.user_id}`}
                                            >{t("Offer ownership")}</Button>
                                        </div>
                                    {/each}
                                </div>
                            {/if}
                        </div>
                    {/each}
                </div>
            {/if}
        </div>

        <div class="border-t border-slate/15 pt-3">
            <p
                class="text-xs font-semibold uppercase tracking-wide text-slate/70"
            >
                {t("Privacy choices")}
            </p>
            <p class="mt-2 text-xs text-slate/70">
                {$locale === "ru" ? "Всё отключено по умолчанию. До явного согласия SDK аналитики и отчётов о сбоях не загружаются, а данные не накапливаются. Каждое согласие можно отозвать независимо." : "Everything is off by default. No analytics or crash-reporting SDK is loaded and nothing is queued before you explicitly opt in. These choices are independent and can be revoked at any time."}
            </p>
            {#if !$appState.accessToken}
                <p class="mt-2 text-xs text-slate/70">
                    Sign in to review or change these choices.
                </p>
            {:else}
                <div class="mt-3 space-y-3">
                    <label class="flex items-start gap-3 text-sm text-slate">
                        <input
                            class="mt-1"
                            type="checkbox"
                            bind:checked={consentProductAnalytics}
                            disabled={consentLoading}
                        />
                        <span>
                            <strong>{t("Product analytics")}</strong><br />
                            <span class="text-xs text-slate/70">
                                Share privacy-filtered feature usage and
                                performance counters.
                            </span>
                        </span>
                    </label>
                    <label class="flex items-start gap-3 text-sm text-slate">
                        <input
                            class="mt-1"
                            type="checkbox"
                            bind:checked={consentCrashReports}
                            disabled={consentLoading}
                        />
                        <span>
                            <strong>{t("Crash reports")}</strong><br />
                            <span class="text-xs text-slate/70">
                                Send redacted crash diagnostics without content,
                                keys, or tokens.
                            </span>
                        </span>
                    </label>
                    <label class="flex items-start gap-3 text-sm text-slate">
                        <input
                            class="mt-1"
                            type="checkbox"
                            bind:checked={consentMarketing}
                            disabled={consentLoading}
                        />
                        <span>
                            <strong>{t("Product email")}</strong><br />
                            <span class="text-xs text-slate/70">
                                Receive optional product news. Security notices
                                remain operational.
                            </span>
                        </span>
                    </label>
                    <Button
                        variant="secondary"
                        on:click={saveConsents}
                        disabled={consentLoading ||
                            totpBusyAction === "consents"}
                    >{t("Save privacy choices")}</Button>
                </div>
            {/if}
        </div>

        <div class="border-t border-slate/15 pt-3">
            <div class="flex items-center justify-between gap-2">
                <p
                    class="text-xs font-semibold uppercase tracking-wide text-slate/70"
                >
                    {t("Security: Devices")}
                </p>
                {#if $appState.accessToken}
                    <Button variant="ghost" on:click={loadDevices}>{t("Refresh")}</Button>
                {/if}
            </div>
            <p class="mt-2 text-xs text-slate/70">
                New devices authenticate first, then require explicit encrypted
                key approval from an already unlocked device.
            </p>
            {#if deviceApprovals.length > 0}
                <div class="mt-2 space-y-2">
                    {#each deviceApprovals as entry}
                        <div
                            class="rounded-xl border border-slate/15 bg-white/70 p-3"
                        >
                            <p class="text-xs font-semibold text-slate">
                                {entry.label}
                            </p>
                            <p class="mt-1 text-xs text-slate/65">
                                {entry.missingSpaces.length === 0
                                    ? "Encrypted access is current."
                                    : `${entry.missingSpaces.length} space(s) await approval.`}
                            </p>
                            {#if entry.missingSpaces.length > 0}
                                <div class="mt-2">
                                    <Button
                                        variant="secondary"
                                        on:click={() => approveDevice(entry)}
                                        disabled={totpBusyAction ===
                                            `device-${entry.device.device_id}`}
                                    >{t("Approve encrypted access")}</Button>
                                </div>
                            {/if}
                        </div>
                    {/each}
                </div>
            {/if}
        </div>

        <div class="border-t border-slate/15 pt-3">
            <div class="flex items-center justify-between gap-2">
                <p
                    class="text-xs font-semibold uppercase tracking-wide text-slate/70"
                >
                    {t("Security: Sessions")}
                </p>
                {#if $appState.accessToken}
                    <Button variant="ghost" on:click={loadSessions}>{t("Refresh")}</Button>
                {/if}
            </div>
            {#if !$appState.accessToken}
                <p class="mt-2 text-xs text-slate/70">
                    {t("Sign in to review sessions.")}
                </p>
            {:else if sessions.length === 0}
                <p class="mt-2 text-xs text-slate/70">{t("No sessions found.")}</p>
            {:else}
                <div class="mt-2 space-y-2">
                    {#each sessions as session}
                        <div
                            class="rounded-xl border border-slate/15 bg-white/70 p-3"
                        >
                            <p class="break-all text-xs font-semibold text-slate">
                                {session.user_agent || "Unknown client"}
                            </p>
                            <p class="mt-1 text-xs text-slate/65">
                                IP {session.ip_address || "unknown"} · Created
                                {formatSessionTime(session.created_at_unix_ms)} ·
                                {t("Last used")}
                                {formatSessionTime(session.last_used_at_unix_ms)}
                            </p>
                            {#if session.revoked_at_unix_ms}
                                <p class="mt-1 text-xs text-coral">
                                    Revoked {formatSessionTime(session.revoked_at_unix_ms)}
                                </p>
                            {:else}
                                <div class="mt-2">
                                    <Button
                                        variant="danger"
                                        on:click={() =>
                                            revokeSession(
                                                session.refresh_token_id,
                                            )}
                                        disabled={totpBusyAction ===
                                            `session-${session.refresh_token_id}`}
                                    >{t("Revoke")}</Button>
                                </div>
                            {/if}
                        </div>
                    {/each}
                </div>
            {/if}
        </div>

        <div class="border-t border-slate/15 pt-3">
            <p
                class="text-xs font-semibold uppercase tracking-wide text-slate/70"
            >
                {t("Security: Data Recovery Kit")}
            </p>
            <p class="mt-2 text-xs text-slate/70">
                These 24 words restore your encrypted data key. They are
                different from the one-time TOTP backup codes below.
                Kamori support cannot recreate them.
            </p>
            {#if $appState.accessToken}
                <div class="mt-2 space-y-2">
                    <Button variant="ghost" on:click={revealDataRecoveryKit}>
                        {t("Reveal 24-word kit")}
                    </Button>
                    {#if dataRecoveryKit}
                        <p
                            class="rounded-xl border border-coral/30 bg-coral/10 p-3 font-mono text-sm text-slate"
                        >
                            {dataRecoveryKit}
                        </p>
                        <Button
                            variant="secondary"
                            on:click={() =>
                                copyText(dataRecoveryKit, "Data recovery kit")}
                        >{t("Copy kit")}</Button>
                    {/if}
                </div>
            {:else}
                <p class="mt-2 text-xs text-slate/70">{t("Sign in to reveal it.")}</p>
            {/if}
        </div>

        <div class="border-t border-slate/15 pt-3">
            <p
                class="text-xs font-semibold uppercase tracking-wide text-slate/70"
            >
                {t("Security: TOTP")}
            </p>

            {#if !$appState.accessToken}
                <p class="mt-2 text-xs text-slate/70">
                    {t("Sign in to manage TOTP.")}
                </p>
            {:else}
                <div class="mt-2 flex flex-wrap items-center gap-2 text-xs">
                    <span class="rounded-lg bg-sand/70 px-2 py-1 text-slate">
                        {totpLoading
                            ? "Status: loading..."
                            : `Status: ${totpEnabled ? "enabled" : "disabled"}`}
                    </span>
                    <span class="rounded-lg bg-sand/70 px-2 py-1 text-slate">
                        {totpLoading
                            ? "TOTP backup codes: ..."
                            : `TOTP backup codes: ${totpRecoveryCodesRemaining} left`}
                    </span>
                    <Button
                        variant="ghost"
                        on:click={loadTotpStatus}
                        disabled={Boolean(totpBusyAction)}
                    >
                        {t("Refresh")}
                    </Button>
                    <Button
                        variant="secondary"
                        on:click={regenerateRecoveryCodes}
                        disabled={Boolean(totpBusyAction)}
                    >
                        {totpBusyAction === "account-recovery-regenerate"
                            ? "Regenerating..."
                            : "Regenerate TOTP Backup Codes"}
                    </Button>
                </div>

                {#if !totpLoading && !totpAvailable}
                    <p class="mt-2 text-xs text-slate/70">
                        TOTP is disabled in server config
                        (`KAMORI_ENABLE_TOTP`).
                    </p>
                {/if}

                {#if !totpLoading && totpAvailable && !totpEnabled}
                    <div
                        class="mt-3 space-y-2 rounded-xl border border-slate/15 p-3"
                    >
                        <div class="flex flex-wrap gap-2">
                            <Button
                                on:click={startTotpSetup}
                                disabled={Boolean(totpBusyAction)}
                            >
                                {totpBusyAction === "totp-start"
                                    ? "Preparing..."
                                    : "Start TOTP Setup"}
                            </Button>
                        </div>

                        {#if totpQrDataUrl}
                            <img
                                src={totpQrDataUrl}
                                alt="TOTP QR code"
                                class="mx-auto mt-2 h-56 w-56 rounded-lg border border-slate/15 bg-white p-2"
                            />
                            <p class="text-center text-xs text-slate/70">
                                Scan QR code with authenticator app.
                            </p>

                            <div class="space-y-2 rounded-lg bg-sand/50 p-2">
                                <p
                                    class="text-[11px] font-semibold uppercase tracking-wide text-slate/70"
                                >
                                    {t("Manual Entry Key")}
                                </p>
                                <p
                                    class="break-all font-mono text-xs text-slate"
                                >
                                    {totpManualEntryKey}
                                </p>
                                <Button
                                    variant="ghost"
                                    on:click={() =>
                                        copyText(
                                            totpManualEntryKey,
                                            "Manual entry key",
                                        )}
                                >
                                    {t("Copy Manual Key")}
                                </Button>
                            </div>

                            <div class="space-y-2 rounded-lg bg-sand/50 p-2">
                                <p
                                    class="text-[11px] font-semibold uppercase tracking-wide text-slate/70"
                                >
                                    OTPAuth URI
                                </p>
                                <p
                                    class="break-all font-mono text-xs text-slate"
                                >
                                    {totpOtpAuthUri}
                                </p>
                                <Button
                                    variant="ghost"
                                    on:click={() =>
                                        copyText(totpOtpAuthUri, "OTPAuth URI")}
                                >
                                    {t("Copy URI")}
                                </Button>
                            </div>

                            <Input
                                bind:value={totpSetupCode}
                                placeholder={$locale === "ru" ? "Введите текущий 6-значный TOTP-код" : "Enter current 6-digit TOTP code"}
                            />
                            <Button
                                on:click={finishTotpSetup}
                                disabled={Boolean(totpBusyAction)}
                            >
                                {totpBusyAction === "totp-finish"
                                    ? "Verifying..."
                                    : "Enable TOTP"}
                            </Button>
                        {/if}
                    </div>
                {/if}

                {#if !totpLoading && totpAvailable && totpEnabled}
                    <div
                        class="mt-3 space-y-2 rounded-xl border border-slate/15 p-3"
                    >
                        <p class="text-xs text-slate/80">
                            To disable TOTP, confirm with a current code from
                            your authenticator.
                        </p>
                        <Input
                            bind:value={totpDisableCode}
                            placeholder={$locale === "ru" ? "Введите текущий 6-значный TOTP-код" : "Enter current 6-digit TOTP code"}
                        />
                        <Button
                            variant="danger"
                            on:click={disableTotp}
                            disabled={Boolean(totpBusyAction)}
                        >
                            {totpBusyAction === "totp-disable"
                                ? "Disabling..."
                                : "Disable TOTP"}
                        </Button>
                    </div>
                {/if}

                {#if totpRecoveryCodes.length > 0}
                    <div
                        class="mt-3 space-y-2 rounded-xl border border-coral/30 bg-coral/10 p-3"
                    >
                        <p class="text-xs font-semibold text-slate">
                            Save these TOTP backup codes now. Each code can be
                            used only once in place of a TOTP code.
                        </p>
                        <div class="grid gap-1 sm:grid-cols-2">
                            {#each totpRecoveryCodes as recoveryCode}
                                <p
                                    class="rounded-md bg-white px-2 py-1 font-mono text-xs text-slate"
                                >
                                    {recoveryCode}
                                </p>
                            {/each}
                        </div>
                        <Button
                            variant="ghost"
                            on:click={() =>
                                copyText(
                                    totpRecoveryCodes.join("\n"),
                                    "TOTP backup codes",
                                )}
                        >
                            Copy TOTP Backup Codes
                        </Button>
                    </div>
                {/if}
            {/if}
        </div>

        <div class="border-t border-slate/15 pt-3">
            <p
                class="text-xs font-semibold uppercase tracking-wide text-slate/70"
            >
                {t("Security: Password")}
            </p>

            {#if !$appState.accessToken}
                <p class="mt-2 text-xs text-slate/70">
                    {t("Sign in to change password.")}
                </p>
            {:else}
                <div
                    class="mt-3 space-y-2 rounded-xl border border-slate/15 p-3"
                >
                    <Input
                        bind:value={passwordChangeCurrent}
                        type="password"
                        placeholder={t("Current password")}
                    />
                    <Input
                        bind:value={passwordChangeNew}
                        type="password"
                        placeholder={t("New password")}
                    />
                    <Input
                        bind:value={passwordChangeConfirm}
                        type="password"
                        placeholder={t("Confirm new password")}
                    />
                    {#if totpEnabled}
                        <Input
                            bind:value={passwordChangeTotp}
                            placeholder={t("Current TOTP or backup code")}
                        />
                    {/if}
                    <Button
                        variant="secondary"
                        on:click={changePassword}
                        disabled={Boolean(totpBusyAction)}
                    >
                        {totpBusyAction === "password-change"
                            ? "Updating..."
                            : "Change Password"}
                    </Button>
                    <p class="text-xs text-slate/70">
                        After password change, all refresh sessions are revoked
                        and you will need to sign in again.
                    </p>
                </div>
            {/if}
        </div>

        <div class="border-t border-coral/30 pt-3">
            <div class="flex items-center justify-between gap-2">
                <p
                    class="text-xs font-semibold uppercase tracking-wide text-coral"
                >
                    {t("Delete account")}
                </p>
                {#if $appState.accessToken}
                    <Button variant="ghost" on:click={loadDeletionStatus}
                        >{t("Refresh status")}</Button
                    >
                {/if}
            </div>
            <p class="mt-2 text-xs text-slate/70">
                This permanently removes credentials, keys stored for your
                account, personal resources, and this browser's encrypted
                cache. Public device verification keys are retained only where
                needed to verify signed history shared with other members.
            </p>
            {#if deletionStatus && !deletionStatus.can_delete}
                <p
                    class="mt-2 rounded-xl border border-coral/30 bg-coral/10 p-3 text-xs text-slate"
                >
                    Before deletion, transfer or remove
                    {deletionStatus.shared_workspaces_owned} shared workspace(s)
                    and {deletionStatus.shared_spaces_owned} shared encrypted
                    space(s).
                </p>
            {/if}
            {#if $appState.accessToken}
                <div
                    class="mt-3 space-y-2 rounded-xl border border-coral/30 bg-coral/10 p-3"
                >
                    <Input
                        bind:value={accountDeletePassword}
                        type="password"
                        placeholder={t("Current password")}
                    />
                    <Input
                        bind:value={accountDeleteTotp}
                        placeholder={t("Current TOTP code, if enabled")}
                    />
                    <Input
                        bind:value={accountDeleteConfirmation}
                        placeholder={`Type DELETE ${$appState.currentUsername}`}
                    />
                    <Button
                        variant="danger"
                        on:click={deleteAccount}
                        disabled={Boolean(totpBusyAction) ||
                            deletionStatus?.can_delete === false}
                    >
                        {totpBusyAction === "account-delete"
                            ? "Deleting..."
                            : "Permanently delete account"}
                    </Button>
                </div>
            {/if}
        </div>
    </div>
</Modal>
