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
    import { normalizeCloudBaseUrl } from "$lib/endpoint";
    import { appState } from "$lib/stores/app";
    import Button from "$lib/components/ui/Button.svelte";
    import Input from "$lib/components/ui/Input.svelte";
    import Modal from "$lib/components/ui/Modal.svelte";
    import LocaleSwitch from "$lib/components/LocaleSwitch.svelte";
    import { locale } from "$lib/i18n";
    import { notify } from "$lib/stores/notifications";

    const ruCopy: Record<string, string> = {
        "Web Settings": "Настройки веб-приложения", "Cloud Base URL": "Адрес сервиса Kamori", "Save Settings": "Сохранить настройки",
        "Ownership transfers": "Передача владения", "Refresh": "Обновить", "Accept ownership": "Принять владение", "Decline": "Отклонить",
        "Pending offers": "Ожидающие предложения", "Cancel offer": "Отменить предложение", "Offer ownership": "Предложить владение",
        "Privacy choices": "Настройки приватности", "Product analytics": "Аналитика продукта", "Crash reports": "Отчёты о сбоях", "Product email": "Новости продукта",
        "Save privacy choices": "Сохранить выбор", "Security: Devices": "Безопасность: устройства", "Approve encrypted access": "Разрешить доступ к шифрованным данным",
        "Revoke device": "Отозвать устройство", "Current device": "Текущее устройство",
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
    const localized = (english: string, russian: string) =>
        $locale === "ru" ? russian : english;

    /** Routed settings surface; it can still use the modal shell in legacy hosts. */
    export let open = false;
    export let onClose: () => void = () => {};
    export let embedded = false;
    export let section: "general" | "security" | "devices" | "privacy" | "account" | "advanced" = "general";

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
    let totpSetupFlowId = "";
    let totpManualEntryKey = "";
    let totpOtpAuthUri = "";
    let totpQrDataUrl = "";
    let totpSetupCode = "";
    let securityCurrentPassword = "";
    let securityCurrentTotp = "";
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
    let formNotice = "";
    let previousSection = section;

    const setNotice = (notice: string) => {
        formNotice = notice;
        notify(notice, { source: t("Web Settings") });
    };

    $: if (section !== previousSection) {
        formNotice = "";
        previousSection = section;
    }

    const clearTotpSetupDraft = () => {
        totpSetupFlowId = "";
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
                    totpContinuationToken: null,
                }));
            },
        });
    }

    const createSecurityReauth = async (): Promise<string> => {
        if (!securityCurrentPassword) {
            throw new Error(localized(
                "Enter your current password to change security settings.",
                "Введите текущий пароль, чтобы изменить настройки безопасности.",
            ));
        }

        const opaqueStart = await opaqueSigninStart(securityCurrentPassword);
        const started = await withAccessRetry((accessToken) =>
            cloudApi.reauthStart(
                $appState.cloudBaseUrl,
                opaqueStart.opaque_start_request,
                "security_settings",
                accessToken,
            ),
        );
        if (started.totp_required && !securityCurrentTotp.trim()) {
            throw new Error(localized(
                "Enter your current two-factor or backup code.",
                "Введите текущий двухфакторный или backup-код.",
            ));
        }
        const opaqueFinish = await opaqueSigninFinish(
            opaqueStart.flow_id,
            securityCurrentPassword,
            started.opaque_server_message,
        );
        const finished = await withAccessRetry((accessToken) =>
            cloudApi.reauthFinish(
                $appState.cloudBaseUrl,
                started.opaque_flow_id,
                opaqueFinish.opaque_finish_request,
                started.totp_required ? securityCurrentTotp.trim() : null,
                "security_settings",
                accessToken,
            ),
        );
        return finished.reauth_token;
    };

    const copyText = async (value: string, label: string) => {
        try {
            await navigator.clipboard.writeText(value);
            setNotice(localized(`${label} copied.`, `${label}: скопировано.`));
        } catch {
            setNotice(localized(
                `Unable to copy ${label.toLowerCase()}.`,
                `Не удалось скопировать: ${label.toLowerCase()}.`,
            ));
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
            setNotice(`${localized("Failed to load TOTP status", "Не удалось загрузить состояние TOTP")}: ${message}`);
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
            setNotice(`${localized("Failed to load privacy choices", "Не удалось загрузить настройки приватности")}: ${message}`);
        } finally {
            consentLoading = false;
        }
    };

    const saveConsents = async () => {
        if (!tokenStore.getAccessToken()) {
            setNotice(localized("Sign in to save privacy choices.", "Войдите, чтобы сохранить настройки приватности."));
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
            setNotice(localized(
                "Privacy choices saved. You can revoke them at any time.",
                "Настройки приватности сохранены. Вы можете отозвать согласие в любое время.",
            ));
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Failed to save privacy choices", "Не удалось сохранить настройки приватности")}: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const startTotpSetup = async () => {
        if (!tokenStore.getAccessToken()) {
            setNotice(localized("Sign in first.", "Сначала войдите."));
            return;
        }
        if (!totpAvailable) {
            setNotice(localized("TOTP is disabled on server.", "TOTP отключён на сервере."));
            return;
        }

        setBusyAction("totp-start");
        try {
            clearTotpRecoveryCodes();
            const reauthToken = await createSecurityReauth();
            const response = await withAccessRetry((accessToken) =>
                cloudApi.totpSetupStart(
                    $appState.cloudBaseUrl,
                    reauthToken,
                    accessToken,
                ),
            );
            const qrSvg = await generateQrSvg(response.otpauth_uri);
            totpSetupFlowId = response.flow_id;
            totpManualEntryKey = response.manual_entry_key;
            totpOtpAuthUri = response.otpauth_uri;
            totpQrDataUrl = `data:image/svg+xml;utf8,${encodeURIComponent(qrSvg)}`;
            totpSetupCode = "";
            setNotice(
                localized(
                    "TOTP setup started. Scan QR code (or use manual key), then confirm with code.",
                    "Настройка TOTP начата. Отсканируйте QR-код или введите ключ вручную, затем подтвердите кодом.",
                ),
            );
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Failed to start TOTP setup", "Не удалось начать настройку TOTP")}: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const finishTotpSetup = async () => {
        if (!totpSetupFlowId) {
            setNotice(localized("Start TOTP setup first.", "Сначала начните настройку TOTP."));
            return;
        }

        const code = totpSetupCode.trim();
        if (!code) {
            setNotice(localized("Enter TOTP code to confirm setup.", "Введите TOTP-код для подтверждения настройки."));
            return;
        }

        setBusyAction("totp-finish");
        try {
            const response = await withAccessRetry((accessToken) =>
                cloudApi.totpSetupFinish(
                    $appState.cloudBaseUrl,
                    {
                        flow_id: totpSetupFlowId,
                        code,
                    },
                    accessToken,
                ),
            );
            totpEnabled = Boolean(response.enabled);
            totpRecoveryCodes = response.recovery_codes ?? [];
            totpRecoveryCodesRemaining = totpRecoveryCodes.length;
            clearTotpSetupDraft();
            setNotice(localized(
                "TOTP enabled. Save the one-time TOTP backup codes now.",
                "TOTP включён. Сохраните одноразовые backup-коды прямо сейчас.",
            ));
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Failed to enable TOTP", "Не удалось включить TOTP")}: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const disableTotp = async () => {
        const code = securityCurrentTotp.trim();
        if (!code) {
            setNotice(localized("Enter current TOTP code to disable.", "Введите текущий TOTP-код для отключения."));
            return;
        }

        setBusyAction("totp-disable");
        try {
            const reauthToken = await createSecurityReauth();
            const response = await withAccessRetry((accessToken) =>
                cloudApi.totpDisable(
                    $appState.cloudBaseUrl,
                    { reauth_token: reauthToken, code },
                    accessToken,
                ),
            );
            totpEnabled = Boolean(response.enabled);
            securityCurrentPassword = "";
            securityCurrentTotp = "";
            clearTotpSetupDraft();
            setNotice(localized("TOTP disabled.", "TOTP отключён."));
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Failed to disable TOTP", "Не удалось отключить TOTP")}: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const changePassword = async () => {
        if (!tokenStore.getAccessToken()) {
            setNotice(localized("Sign in first.", "Сначала войдите."));
            return;
        }

        const nextPassword = passwordChangeNew;
        if (!passwordChangeCurrent || !nextPassword || !passwordChangeConfirm) {
            setNotice(localized(
                "Current password, new password, and confirmation are required.",
                "Введите текущий пароль, новый пароль и его подтверждение.",
            ));
            return;
        }
        if (nextPassword !== passwordChangeConfirm) {
            setNotice(localized("Password confirmation does not match.", "Пароли не совпадают."));
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
                throw new Error(localized("Current two-factor code is required.", "Введите текущий код двухфакторной аутентификации."));
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
                totpContinuationToken: null,
            }));
            setNotice(localized("Password changed. Sign in again.", "Пароль изменён. Войдите снова."));
            clearPasswordChangeDraft();
            clearTotpRecoveryCodes();
            clearTotpSetupDraft();
            totpRecoveryCodesRemaining = 0;
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Password change failed", "Не удалось изменить пароль")}: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const regenerateRecoveryCodes = async () => {
        setBusyAction("account-recovery-regenerate");
        try {
            const reauthToken = await createSecurityReauth();
            const response = await withAccessRetry((accessToken) =>
                cloudApi.accountRecoveryCodesRegenerate(
                    $appState.cloudBaseUrl,
                    reauthToken,
                    accessToken,
                ),
            );
            totpRecoveryCodes = response.recovery_codes ?? [];
            totpRecoveryCodesRemaining = totpRecoveryCodes.length;
            securityCurrentPassword = "";
            securityCurrentTotp = "";
            setNotice(
                localized(
                    "TOTP backup codes regenerated. Old unused codes are revoked.",
                    "Backup-коды TOTP созданы заново. Старые неиспользованные коды отозваны.",
                ),
            );
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(
                `${localized("Failed to regenerate TOTP backup codes", "Не удалось создать новые backup-коды TOTP")}: ${message}`,
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
                localized(
                    "Data recovery kit revealed. Keep it private and store it offline.",
                    "Recovery kit показан. Не передавайте его другим и храните офлайн.",
                ),
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Unable to reveal data recovery kit", "Не удалось показать recovery kit")}: ${message}`);
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
            setNotice(`${localized("Failed to load sessions", "Не удалось загрузить сессии")}: ${message}`);
        }
    };

    const revokeSession = async (sessionId: string) => {
        setBusyAction(`session-${sessionId}`);
        try {
            const reauthToken = await createSecurityReauth();
            await withAccessRetry((accessToken) =>
                cloudApi.revokeSession(
                    $appState.cloudBaseUrl,
                    sessionId,
                    reauthToken,
                    accessToken,
                ),
            );
            await loadSessions();
            setNotice(localized("Session revoked.", "Сессия отозвана."));
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Failed to revoke session", "Не удалось отозвать сессию")}: ${message}`);
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
            setNotice(`${localized("Failed to load devices", "Не удалось загрузить устройства")}: ${message}`);
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
                    ? localized(
                          `Approved ${entry.label} for ${approved} encrypted space(s).`,
                          `Устройство ${entry.label} получило доступ к зашифрованным пространствам: ${approved}.`,
                      )
                    : localized(
                          "No locally unlocked space keys were available.",
                          "На этом устройстве нет разблокированных ключей пространств.",
                      ),
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Device approval failed", "Не удалось одобрить устройство")}: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const revokeDevice = async (entry: (typeof deviceApprovals)[number]) => {
        const activeDeviceId = getActiveWebDevice().deviceId;
        if (entry.device.device_id === activeDeviceId) {
            setNotice(localized(
                "Revoke the current browser from another approved device.",
                "Отзовите текущий браузер с другого одобренного устройства.",
            ));
            return;
        }
        if (!window.confirm(localized(
            `Revoke ${entry.label}? Its sessions and encrypted key packages will be invalidated immediately.`,
            `Отозвать ${entry.label}? Его сессии и пакеты ключей будут немедленно аннулированы.`,
        ))) return;
        setBusyAction(`revoke-device-${entry.device.device_id}`);
        try {
            const reauthToken = await createSecurityReauth();
            await withAccessRetry((accessToken) =>
                cloudApi.revokeDevice(
                    $appState.cloudBaseUrl,
                    entry.device.device_id,
                    reauthToken,
                    accessToken,
                ),
            );
            await Promise.all([loadDevices(), loadSessions()]);
            setNotice(localized("Device revoked.", "Устройство отозвано."));
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Failed to revoke device", "Не удалось отозвать устройство")}: ${message}`);
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
            setNotice(`${localized("Failed to load ownership controls", "Не удалось загрузить управление владением")}: ${message}`);
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
                localized(
                    `Ownership offer sent to ${target.username}. It expires in 24 hours and only the recipient can accept it.`,
                    `Предложение владения отправлено пользователю ${target.username}. Оно действует 24 часа, принять его может только получатель.`,
                ),
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Ownership transfer failed", "Не удалось передать владение")}: ${message}`);
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
                    ? localized("Ownership transfer accepted.", "Передача владения принята.")
                    : localized("Ownership transfer declined.", "Передача владения отклонена."),
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Unable to resolve ownership offer", "Не удалось обработать предложение владения")}: ${message}`);
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
            setNotice(`${localized("Failed to check account deletion", "Не удалось проверить возможность удаления аккаунта")}: ${message}`);
        }
    };

    const deleteAccount = async () => {
        const username = $appState.currentUsername;
        if (!username || !accountDeletePassword) {
            setNotice(localized("Password is required to delete the account.", "Для удаления аккаунта нужен пароль."));
            return;
        }
        if (accountDeleteConfirmation !== `DELETE ${username}`) {
            setNotice(localized(
                `Type DELETE ${username} exactly to confirm.`,
                `Для подтверждения введите без изменений: DELETE ${username}`,
            ));
            return;
        }
        if (!deletionStatus?.can_delete) {
            setNotice(localized(
                "Transfer shared resources before deleting the account.",
                "Перед удалением аккаунта передайте владение общими ресурсами.",
            ));
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
                throw new Error(localized("Current two-factor code is required.", "Введите текущий код двухфакторной аутентификации."));
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
            await deleteWebVaultAccount($appState.cloudBaseUrl, username);
            tokenStore.clear();
            appState.update((state) => ({
                ...state,
                currentUsername: "",
                accessToken: null,
                totpContinuationToken: null,
                collections: [],
                syncedItemsTotal: 0,
                lastSyncedSeq: 0,
            }));
            setNotice(localized(
                "Account and local encrypted browser data deleted.",
                "Аккаунт и локальные зашифрованные данные браузера удалены.",
            ));
            accountDeletePassword = "";
            accountDeleteTotp = "";
            accountDeleteConfirmation = "";
            onClose();
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Account deletion failed", "Не удалось удалить аккаунт")}: ${message}`);
        } finally {
            clearBusyAction();
        }
    };

    const formatSessionTime = (unixMs?: number | null): string =>
        unixMs
            ? new Date(unixMs).toLocaleString($locale === "ru" ? "ru-RU" : "en-US")
            : localized("Never", "Никогда");

    /**
     * Persists normalized base URL and notifies user.
     */
    const saveCloudBaseUrl = async () => {
        let next: string;
        try {
            next = normalizeCloudBaseUrl(
                settingsCloudBaseUrl.trim() || "http://127.0.0.1:3000",
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Invalid service address", "Некорректный адрес сервиса")}: ${message}`);
            return;
        }
        if (tokenStore.getAccessToken()) {
            setNotice(localized(
                "Sign out before changing the Kamori service address.",
                "Выйдите из аккаунта перед изменением адреса сервиса Kamori.",
            ));
            return;
        }
        // A cookie-backed session can exist while the local vault is locked.
        // Revoke it on the old origin before switching; an unreachable old
        // origin must not trap the user in a broken configuration.
        await cloudApi.logout($appState.cloudBaseUrl, "").catch(() => undefined);
        tokenStore.clear();
        lockWebVault();
        appState.update((state) => ({
            ...state,
            cloudBaseUrl: next,
            accessToken: null,
            totpContinuationToken: null,
        }));
        settingsCloudBaseUrl = next;
        clearTotpSetupDraft();
        clearTotpRecoveryCodes();
        clearPasswordChangeDraft();
        totpRecoveryCodesRemaining = 0;
        securityCurrentPassword = "";
        securityCurrentTotp = "";
        onClose();
        setNotice(localized(
            "Cloud base URL updated. Sign in again.",
            "Адрес сервиса Kamori обновлён. Войдите снова.",
        ));
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
        securityCurrentPassword = "";
        securityCurrentTotp = "";
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

<Modal {open} title={t("Web Settings")} {onClose} {embedded}>
    <div class="space-y-4">
        {#if formNotice}
            <p class="border border-coral/30 bg-coral/10 p-3 text-sm text-slate" role="alert">
                {formNotice}
            </p>
        {/if}
        {#if section === "general"}
        <p class="text-xs font-semibold uppercase tracking-wide text-slate/70">
            {localized("Application language", "Язык приложения")}
        </p>
        <p class="text-sm text-slate/70">
            {localized(
                "Choose the language used by this browser. The preference stays on this device.",
                "Выберите язык для этого браузера. Настройка сохраняется только на этом устройстве.",
            )}
        </p>
        <LocaleSwitch />
        {/if}

        {#if section === "advanced"}
        <p class="text-xs font-semibold uppercase tracking-wide text-slate/70">
            {t("Cloud Base URL")}
        </p>
        <Input bind:value={settingsCloudBaseUrl} />
        <div class="pt-1">
            <Button on:click={saveCloudBaseUrl}>{t("Save Settings")}</Button>
        </div>
        <p class="text-xs text-slate/65">
            {localized(
                "The hosted app configures this automatically. Change it only when connecting to a self-hosted Kamori service.",
                "В hosted-версии адрес настроен автоматически. Меняйте его только для подключения к self-hosted Kamori.",
            )}
        </p>
        {/if}

        {#if section === "account"}
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
                {localized(
                    "Ownership never changes silently. An active member must accept the offer within 24 hours. Space storage is charged to the new owner only after acceptance.",
                    "Владелец никогда не меняется автоматически. Активный участник должен принять предложение в течение 24 часов. Хранилище пространства учитывается за новым владельцем только после принятия.",
                )}
            </p>
            {#if incomingOwnershipOffers.length > 0}
                <div class="mt-3 space-y-2">
                    {#each incomingOwnershipOffers as offer}
                        <div
                            class="rounded-xl border border-slate/15 bg-white/70 p-3"
                        >
                            <p class="text-xs font-semibold text-slate">
                                {offer.resource_kind === "security_space"
                                    ? localized("Encrypted space", "Зашифрованное пространство")
                                    : localized("Team workspace", "Командное рабочее пространство")}
                                · {offer.resource_id.slice(0, 8)}
                            </p>
                            <p class="mt-1 text-xs text-slate/65">
                                {localized("Offered by", "Предложил")}
                                {offer.current_owner_username} · {localized("expires", "истекает")}
                                {formatSessionTime(offer.expires_at_unix_ms)}
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
                                    ? localized("Encrypted space", "Зашифрованное пространство")
                                    : localized("Team workspace", "Командное рабочее пространство")}
                                {offer.resource_id.slice(0, 8)} · {localized("recipient", "получатель")}
                                {offer.target_user_id.slice(0, 8)} · {localized("expires", "истекает")}
                                {formatSessionTime(offer.expires_at_unix_ms)}
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
                                    {localized(
                                        "Invite a member before transferring ownership.",
                                        "Сначала пригласите участника, затем передайте владение.",
                                    )}
                                </p>
                            {:else}
                                <div class="mt-2 space-y-2">
                                    {#each resource.members as member}
                                        <div
                                            class="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-sand/50 p-2"
                                        >
                                            <span class="text-xs text-slate">
                                                {member.username} · {localized(
                                                    member.role,
                                                    member.role === "owner" ? "владелец" : member.role === "editor" ? "редактор" : "чтение",
                                                )}
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
        {/if}

        {#if section === "privacy"}
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
                    {localized(
                        "Sign in to review or change these choices.",
                        "Войдите, чтобы просмотреть или изменить этот выбор.",
                    )}
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
                                {localized(
                                    "Share privacy-filtered feature usage and performance counters.",
                                    "Передавать очищенные от личных данных сведения об использовании функций и производительности.",
                                )}
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
                                {localized(
                                    "Send redacted crash diagnostics without content, keys, or tokens.",
                                    "Отправлять обезличенную диагностику сбоев без содержимого, ключей и токенов.",
                                )}
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
                                {localized(
                                    "Receive optional product news. Security notices remain operational.",
                                    "Получать необязательные новости продукта. Уведомления безопасности остаются служебными.",
                                )}
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
        {/if}

        {#if section === "devices"}
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
                {localized(
                    "New devices authenticate first, then require explicit encrypted key approval from an already unlocked device.",
                    "Новое устройство сначала входит в аккаунт, а затем получает явное разрешение на зашифрованные ключи с уже разблокированного устройства.",
                )}
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
                                    ? localized("Encrypted access is current.", "Доступ к зашифрованным данным актуален.")
                                    : localized(
                                          `${entry.missingSpaces.length} space(s) await approval.`,
                                          `${entry.missingSpaces.length} пространств ожидают одобрения.`,
                                      )}
                            </p>
                            {#if entry.missingSpaces.length > 0}
                                <div class="mt-2 flex flex-wrap gap-2">
                                    <Button
                                        variant="secondary"
                                        on:click={() => approveDevice(entry)}
                                        disabled={totpBusyAction ===
                                            `device-${entry.device.device_id}`}
                                    >{t("Approve encrypted access")}</Button>
                                    {#if entry.device.device_id !== getActiveWebDevice().deviceId}
                                        <Button
                                            variant="ghost"
                                            on:click={() => revokeDevice(entry)}
                                            disabled={totpBusyAction === `revoke-device-${entry.device.device_id}`}
                                        >{t("Revoke device")}</Button>
                                    {/if}
                                </div>
                            {:else if entry.device.device_id !== getActiveWebDevice().deviceId}
                                <div class="mt-2">
                                    <Button
                                        variant="ghost"
                                        on:click={() => revokeDevice(entry)}
                                        disabled={totpBusyAction === `revoke-device-${entry.device.device_id}`}
                                    >{t("Revoke device")}</Button>
                                </div>
                            {:else}
                                <p class="mt-2 text-xs text-slate/55">{t("Current device")}</p>
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
                                {session.user_agent || localized("Unknown client", "Неизвестный клиент")}
                                {session.is_current ? ` · ${localized("Current", "Текущая")}` : ""}
                            </p>
                            <p class="mt-1 text-xs text-slate/65">
                                IP {session.ip_address || localized("unknown", "неизвестен")} · {localized("Created", "Создана")}
                                {formatSessionTime(session.created_at_unix_ms)} ·
                                {t("Last used")}
                                {formatSessionTime(session.last_used_at_unix_ms)}
                            </p>
                            {#if session.revoked_at_unix_ms}
                                <p class="mt-1 text-xs text-coral">
                                    {localized("Revoked", "Отозвана")} {formatSessionTime(session.revoked_at_unix_ms)}
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
        {/if}

        {#if section === "security"}
        <div class="border-t border-slate/15 pt-3">
            <p
                class="text-xs font-semibold uppercase tracking-wide text-slate/70"
            >
                {t("Security: Data Recovery Kit")}
            </p>
            <p class="mt-2 text-xs text-slate/70">
                {localized(
                    "These 24 words restore your encrypted data key. They are different from the one-time TOTP backup codes below. Kamori support cannot recreate them.",
                    "Эти 24 слова восстанавливают ключ зашифрованных данных. Это не одноразовые резервные коды TOTP ниже. Поддержка Kamori не сможет восстановить эту фразу.",
                )}
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
                <div class="mt-2 grid gap-2 sm:grid-cols-2">
                    <Input
                        bind:value={securityCurrentPassword}
                        type="password"
                        placeholder={localized(
                            "Current password for security changes",
                            "Текущий пароль для изменений безопасности",
                        )}
                    />
                    {#if totpEnabled}
                        <Input
                            bind:value={securityCurrentTotp}
                            placeholder={localized(
                                "Current TOTP or backup code",
                                "Текущий TOTP или backup-код",
                            )}
                        />
                    {/if}
                </div>
                <p class="mt-2 text-xs text-slate/70">
                    {localized(
                        "Changing two-factor settings requires a fresh password check. Credentials stay in this dialog and are never persisted.",
                        "Изменение двухфакторных настроек требует свежей проверки пароля. Данные остаются только в этом окне и не сохраняются.",
                    )}
                </p>
                <div class="mt-2 flex flex-wrap items-center gap-2 text-xs">
                    <span class="rounded-lg bg-sand/70 px-2 py-1 text-slate">
                        {totpLoading
                            ? localized("Status: loading…", "Статус: загрузка…")
                            : localized(
                                  `Status: ${totpEnabled ? "enabled" : "disabled"}`,
                                  `Статус: ${totpEnabled ? "включено" : "отключено"}`,
                              )}
                    </span>
                    <span class="rounded-lg bg-sand/70 px-2 py-1 text-slate">
                        {totpLoading
                            ? localized("TOTP backup codes: …", "Резервные коды TOTP: …")
                            : localized(
                                  `TOTP backup codes: ${totpRecoveryCodesRemaining} left`,
                                  `Резервных кодов TOTP осталось: ${totpRecoveryCodesRemaining}`,
                              )}
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
                            ? localized("Regenerating…", "Создаём новые…")
                            : localized("Regenerate TOTP Backup Codes", "Создать новые резервные коды TOTP")}
                    </Button>
                </div>

                {#if !totpLoading && !totpAvailable}
                    <p class="mt-2 text-xs text-slate/70">
                        {localized(
                            "TOTP is disabled by the service operator.",
                            "TOTP отключён оператором сервиса.",
                        )}
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
                                    ? localized("Preparing…", "Подготовка…")
                                    : localized("Start TOTP Setup", "Настроить TOTP")}
                            </Button>
                        </div>

                        {#if totpQrDataUrl}
                            <img
                                src={totpQrDataUrl}
                                alt={localized("TOTP QR code", "QR-код TOTP")}
                                class="mx-auto mt-2 h-56 w-56 rounded-lg border border-slate/15 bg-white p-2"
                            />
                            <p class="text-center text-xs text-slate/70">
                                {localized(
                                    "Scan the QR code with an authenticator app.",
                                    "Отсканируйте QR-код в приложении-аутентификаторе.",
                                )}
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
                                    ? localized("Verifying…", "Проверка…")
                                    : localized("Enable TOTP", "Включить TOTP")}
                            </Button>
                        {/if}
                    </div>
                {/if}

                {#if !totpLoading && totpAvailable && totpEnabled}
                    <div
                        class="mt-3 space-y-2 rounded-xl border border-slate/15 p-3"
                    >
                        <p class="text-xs text-slate/80">
                            {localized(
                                "Use the current password and authenticator code above to disable TOTP.",
                                "Чтобы отключить TOTP, введите выше текущий пароль и код из аутентификатора.",
                            )}
                        </p>
                        <Button
                            variant="danger"
                            on:click={disableTotp}
                            disabled={Boolean(totpBusyAction)}
                        >
                            {totpBusyAction === "totp-disable"
                                ? localized("Disabling…", "Отключение…")
                                : localized("Disable TOTP", "Отключить TOTP")}
                        </Button>
                    </div>
                {/if}

                {#if totpRecoveryCodes.length > 0}
                    <div
                        class="mt-3 space-y-2 rounded-xl border border-coral/30 bg-coral/10 p-3"
                    >
                        <p class="text-xs font-semibold text-slate">
                            {localized(
                                "Save these TOTP backup codes now. Each code can be used only once in place of a TOTP code.",
                                "Сохраните резервные коды TOTP сейчас. Каждый код можно использовать только один раз вместо кода TOTP.",
                            )}
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
                            {localized("Copy TOTP Backup Codes", "Копировать резервные коды TOTP")}
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
                            ? localized("Updating…", "Обновление…")
                            : localized("Change Password", "Изменить пароль")}
                    </Button>
                    <p class="text-xs text-slate/70">
                        {localized(
                            "After a password change, all refresh sessions are revoked and you must sign in again.",
                            "После смены пароля все сессии будут отозваны, и потребуется войти снова.",
                        )}
                    </p>
                </div>
            {/if}
        </div>
        {/if}

        {#if section === "account"}
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
                {localized(
                    "This permanently removes credentials, keys stored for your account, personal resources, and this browser's encrypted cache. Public device verification keys are retained only where needed to verify signed history shared with other members.",
                    "Это безвозвратно удалит учётные данные, сохранённые для аккаунта ключи, личные ресурсы и зашифрованный кеш этого браузера. Публичные ключи проверки устройств сохраняются только там, где они нужны для проверки подписанной истории, доступной другим участникам.",
                )}
            </p>
            {#if deletionStatus && !deletionStatus.can_delete}
                <p
                    class="mt-2 rounded-xl border border-coral/30 bg-coral/10 p-3 text-xs text-slate"
                >
                    {localized(
                        `Before deletion, transfer or remove ${deletionStatus.shared_workspaces_owned} shared workspace(s) and ${deletionStatus.shared_spaces_owned} shared encrypted space(s).`,
                        `Перед удалением передайте или удалите общие рабочие пространства (${deletionStatus.shared_workspaces_owned}) и зашифрованные пространства (${deletionStatus.shared_spaces_owned}).`,
                    )}
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
                        placeholder={localized(
                            `Type DELETE ${$appState.currentUsername}`,
                            `Введите DELETE ${$appState.currentUsername}`,
                        )}
                    />
                    <Button
                        variant="danger"
                        on:click={deleteAccount}
                        disabled={Boolean(totpBusyAction) ||
                            deletionStatus?.can_delete === false}
                    >
                        {totpBusyAction === "account-delete"
                            ? localized("Deleting…", "Удаление…")
                            : localized("Permanently delete account", "Удалить аккаунт безвозвратно")}
                    </Button>
                </div>
            {/if}
        </div>
        {/if}
    </div>
</Modal>
