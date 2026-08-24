<script lang="ts">
    import { onDestroy, onMount } from "svelte";
    import { page } from "$app/stores";
    import { cloudApi, type SpaceMemberSummary } from "$lib/api/cloud";
    import { decode, encode } from "@msgpack/msgpack";
    import { normalizeByteArray } from "$lib/binary";
    import { tokenStore } from "$lib/auth/tokenStore";
    import { runWithAccessRefreshRetry } from "$lib/auth/session-flow.js";
    import {
        generateInviteCode,
        hashInviteCode,
        unwrapBytesWithInviteCode,
        unwrapCollectionKeyWithInviteCode,
        wrapBytesWithInviteCode,
        wrapCollectionKeyWithInviteCode,
    } from "$lib/inviteCodeCrypto";
    import { appState, type SpaceEntry } from "$lib/stores/app";
    import {
        getActiveWebDevice,
        listQueuedOperationEnvelopes,
        loadMaterializedPimState,
        listQuarantinedOperationRecords,
        loadSpaceKey,
        queueOperationEnvelope,
        quarantineOperationEnvelope,
        removeQuarantinedOperationEnvelope,
        removeQueuedOperationEnvelope,
        storeSpaceKey,
        storeMaterializedPimState,
        withActiveMasterKey,
    } from "$lib/cryptoVault";
    import {
        decryptVaultBytes,
        encryptAccountMasterKeyForDevice,
        encryptVaultBytes,
        assignPimBranchGraph,
        openOperationEnvelope,
        materializePimOperation,
        sealOperationEnvelope,
        unwrapSpaceKeyFromAccountRecovery,
        unwrapSpaceKeyForDevice,
        verifyOperationEnvelope,
        wrapSpaceKeyForDevice,
        wrapSpaceKeyForAccountRecovery,
    } from "$lib/opaqueClient";
    import {
        decodePimOperation,
        decodePimSnapshot,
        dateToIcalendarUtc,
        encodePimOperation,
        encodePimSnapshot,
        localDateTimeToIcalendarUtc,
        projectionFields,
        type MaterializedPimItem,
        type MaterializedOperationState,
        type PimOperationV1,
        type PimResourceKind,
        type PimValue,
        type PimSnapshotBranchV2,
        type PimSnapshotV2,
    } from "$lib/pim";
    import Button from "$lib/components/ui/Button.svelte";
    import Card from "$lib/components/ui/Card.svelte";
    import Input from "$lib/components/ui/Input.svelte";
    import Modal from "$lib/components/ui/Modal.svelte";
    import { locale } from "$lib/i18n";
    import { notify } from "$lib/stores/notifications";
    import {
        markSyncFailure,
        markSyncSuccess,
        markSyncing,
        registerManualSync,
        setPendingOperations,
        syncState,
    } from "$lib/stores/sync";
    import {
        AutoSyncCoordinator,
        type SyncReason,
    } from "$lib/sync/autoSyncCoordinator";

    const ruCopy: Record<string, string> = {
        "Confirm this code in your desktop app": "Сверьте этот код с кодом в приложении для компьютера",
        "Approve Desktop": "Подтвердить компьютер",
        "Dashboard": "Обзор", "Current user": "Текущий пользователь", "Not signed in": "Вход не выполнен",
        "Spaces": "Пространства", "Operation states on device": "Состояний операций на устройстве", "Last synced seq": "Последняя операция",
        "Syncing...": "Синхронизация…", "Sync Now": "Синхронизировать", "Space name": "Название пространства",
        "Create Space": "Создать пространство", "No spaces yet.": "Пространств пока нет.", "Approval required on another device": "Нужно подтверждение на другом устройстве",
        "Delete": "Удалить", "Trash · retained for 30 days": "Корзина · хранение 30 дней", "Restore": "Восстановить",
        "Tasks": "Задачи", "New task": "Новая задача", "Saving...": "Сохранение…", "Add Task": "Добавить задачу",
        "Completed": "Выполнено", "Open": "Открыто", "Reopen": "Вернуть", "Complete": "Выполнить", "No tasks on this device yet.": "На этом устройстве задач пока нет.",
        "Calendar": "Календарь", "Event title": "Название события", "Add Event": "Добавить событие", "No events on this device yet.": "На этом устройстве событий пока нет.",
        "Contacts": "Контакты", "Full name": "Полное имя", "Email": "Email", "Phone": "Телефон", "Add Contact": "Добавить контакт", "No contacts on this device yet.": "На этом устройстве контактов пока нет.",
        "Invite Codes": "Коды приглашений", "Generate": "Создать", "No spaces": "Нет пространств", "Editor": "Редактор", "Reader": "Чтение",
        "Optional encrypted note for recipient": "Необязательная зашифрованная заметка получателю", "Generating...": "Создание…", "Generate Invite Code": "Создать код приглашения",
        "Redeem": "Принять", "Redeeming...": "Принимаем…", "Redeem Code": "Принять код", "Invite Note": "Заметка приглашения",
        "Delete Space": "Удалить пространство", "Cancel": "Отмена", "Move to Trash": "Переместить в корзину",
    };
    const t = (english: string) => $locale === "ru" ? (ruCopy[english] ?? english) : english;
    const localized = (english: string, russian: string) =>
        $locale === "ru" ? russian : english;

    export let view:
        | "today"
        | "tasks"
        | "calendar"
        | "contacts"
        | "spaces"
        | "sharing" = "today";

    /** Persistent encrypted data-plane controller and focused routed views. */
    let collectionName = "";
    let selectedCollectionId = "";
    let selectedCollection: SpaceEntry | undefined;
    let canWriteSelectedCollection = false;
    let requestedSpaceId = "";
    let taskTitle = "";
    let taskFormError = "";
    let eventTitle = "";
    let eventStart = "";
    let eventEnd = "";
    let eventFormError = "";
    let contactName = "";
    let contactEmail = "";
    let contactPhone = "";
    let contactFormError = "";
    let pimItems: MaterializedPimItem[] = [];
    let operationStates: MaterializedOperationState[] = [];
    let spaceMembers: Record<string, SpaceMemberSummary[]> = {};
    const signingKeysBySpace = new Map<string, Map<string, Uint8Array>>();
    let syncCursors: Record<string, number> = {};
    let trashedCollections: SpaceEntry[] = [];

    let inviteTtlMinutes = "60";
    let inviteRole: "editor" | "reader" = "editor";
    let inviteCodeIssued = "";
    let inviteCodeToRedeem = "";
    let inviteNotePlaintext = "";
    let inviteRedeemedNote = "";

    let loadingAction = "";
    let deviceAuthorizationCode = "";
    let dataPlaneTail: Promise<void> = Promise.resolve();

    let deleteModalOpen = false;
    let pendingDeleteCollectionId = "";
    let collectionFormError = "";
    let suggestedPersonalSpace = false;
    let lastMetadataSyncAt = 0;
    let stopManualSyncRegistration = () => {};

    const textEncoder = new TextEncoder();
    const textDecoder = new TextDecoder();

    const inviteTtlOptions = [
        { value: 15, en: "15 minutes", ru: "15 минут" },
        { value: 30, en: "30 minutes", ru: "30 минут" },
        { value: 60, en: "1 hour", ru: "1 час" },
        { value: 180, en: "3 hours", ru: "3 часа" },
        { value: 720, en: "12 hours", ru: "12 часов" },
        { value: 1440, en: "24 hours", ru: "24 часа" },
        { value: 4320, en: "3 days", ru: "3 дня" },
        { value: 10080, en: "7 days", ru: "7 дней" },
    ];

    /**
     * Generates cryptographically strong random bytes in browser context.
     */
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

    async function runDataPlaneExclusive<T>(
        operation: () => Promise<T>,
    ): Promise<T> {
        const previous = dataPlaneTail;
        let release = () => {};
        dataPlaneTail = new Promise<void>((resolve) => {
            release = resolve;
        });
        await previous;
        try {
            if (navigator.locks) {
                return await navigator.locks.request(
                    "kamori-web-data-plane-v1",
                    operation,
                );
            }
            return await operation();
        } finally {
            release();
        }
    }

    const setNotice = (notice: string) => {
        notify(notice, { source: localized("Web app", "Веб-приложение") });
    };

    const clearDeviceAuthorizationQuery = () => {
        const url = new URL(window.location.href);
        url.searchParams.delete("device_code");
        window.history.replaceState({}, "", url);
        deviceAuthorizationCode = "";
    };

    const approveDeviceAuthorization = async () => {
        if (!deviceAuthorizationCode) return;
        setLoading("device-authorization");
        try {
            const challenge = await withAccessRetry((accessToken) =>
                cloudApi.inspectDeviceAuthorization(
                    $appState.cloudBaseUrl,
                    deviceAuthorizationCode,
                    accessToken,
                ),
            );
            const recipientKey = normalizeByteArray(
                challenge.hpke_public_key,
                32,
                "Desktop authorization public key",
            );
            const encryptedMasterKeyPackage = await withActiveMasterKey(
                (masterKey) =>
                    encryptAccountMasterKeyForDevice(
                        masterKey,
                        recipientKey,
                        challenge.flow_id,
                    ),
            );
            await withAccessRetry((accessToken) =>
                cloudApi.approveDeviceAuthorization(
                    $appState.cloudBaseUrl,
                    deviceAuthorizationCode,
                    encryptedMasterKeyPackage,
                    accessToken,
                ),
            );
            setNotice(localized(
                "Desktop sign-in approved. You may return to Kamori Desktop.",
                "Вход на компьютере подтверждён. Вернитесь в Kamori Desktop.",
            ));
            clearDeviceAuthorizationQuery();
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Desktop authorization failed", "Не удалось подтвердить вход на компьютере")}: ${message}`);
        } finally {
            clearLoading();
        }
    };

    const withAccessToken = (): string => {
        const token = tokenStore.getAccessToken();
        if (!token) {
            throw new Error(localized("Sign in first.", "Сначала войдите."));
        }
        return token;
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
            getAccessToken: withAccessToken,
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

    /**
     * Creates local-only collection entry with generated 32-byte CMK.
     */
    const createCollection = async () => {
        const name = collectionName.trim();
        if (!name) {
            collectionFormError = localized(
                "Space name is required.",
                "Введите название пространства.",
            );
            return;
        }

        collectionFormError = "";
        setLoading("space-create");
        try {
            const spaceId = crypto.randomUUID();
            const spaceKey = randomBytes(32);
            const device = getActiveWebDevice();
            const encryptedKeyPackage = encode(
                await wrapSpaceKeyForDevice(
                    spaceKey,
                    device.identity.hpke_public_key,
                ),
            );
            const encryptedMetadata = await encryptVaultBytes(
                spaceKey,
                encode({ version: 1, kind: "pim", name }),
            );
            const encryptedRecoveryKeyPackage = await withActiveMasterKey(
                async (masterKey) =>
                    encode(await wrapSpaceKeyForAccountRecovery(masterKey, spaceKey)),
            );
            await withAccessRetry((accessToken) =>
                cloudApi.createSpace(
                    $appState.cloudBaseUrl,
                    {
                        space_id: spaceId,
                        encrypted_metadata: encryptedMetadata,
                        device_key_packages: [
                            {
                                device_id: device.deviceId,
                                key_epoch: 1,
                                encrypted_key_package: encryptedKeyPackage,
                            },
                        ],
                        encrypted_recovery_key_package:
                            encryptedRecoveryKeyPackage,
                    },
                    accessToken,
                ),
            );
            await storeSpaceKey(spaceId, 1, spaceKey);
            const entry: SpaceEntry = {
                id: spaceId,
                name,
                keyAvailable: true,
                keyEpoch: 1,
                role: "owner",
                syncedItems: 0,
            };

            appState.update((state) => ({
                ...state,
                collections: [...state.collections, entry],
            }));
            notify(
                localized(
                    `Space "${name}" created.`,
                    `Пространство «${name}» создано.`,
                ),
                { kind: "success", source: localized("Spaces", "Пространства") },
            );

            selectedCollectionId = entry.id;
            collectionName = "";
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            collectionFormError = `${localized("Space creation failed", "Не удалось создать пространство")}: ${message}`;
            notify(collectionFormError, {
                kind: "error",
                source: localized("Spaces", "Пространства"),
            });
        } finally {
            clearLoading();
        }
    };

    const reconcilePimStream = async (
        spaceId: string,
        logicalResourceId: string,
        kind: PimResourceKind,
    ): Promise<void> => {
        const streamStates = operationStates.filter(
            (state) =>
                state.spaceId === spaceId &&
                state.logicalResourceId === logicalResourceId &&
                state.kind === kind,
        );
        const assignments = await assignPimBranchGraph(
            logicalResourceId,
            streamStates.map((state) => ({
                operation_id: state.clientOpId,
                parent_operation_id: state.parentOperationId ?? null,
                seed_projection_resource_id: state.seedProjectionId ?? null,
            })),
        );
        const byId = new Map(
            assignments.map((assignment) => [assignment.operation_id, assignment]),
        );
        const hasConflict = assignments.filter((assignment) => assignment.head).length > 1;
        operationStates = operationStates.map((state) => {
            if (
                state.spaceId !== spaceId ||
                state.logicalResourceId !== logicalResourceId ||
                state.kind !== kind
            ) {
                return state;
            }
            const assignment = byId.get(state.clientOpId);
            if (!assignment) throw new Error("PIM branch assignment is incomplete.");
            return { ...state, projectionId: assignment.projection_resource_id };
        });
        pimItems = pimItems.filter(
            (item) =>
                item.spaceId !== spaceId ||
                item.resourceId !== logicalResourceId ||
                item.kind !== kind,
        );
        for (const state of operationStates) {
            const assignment = byId.get(state.clientOpId);
            if (
                state.spaceId !== spaceId ||
                state.logicalResourceId !== logicalResourceId ||
                state.kind !== kind ||
                !assignment?.head ||
                state.deleted
            ) {
                continue;
            }
            pimItems = [
                ...pimItems,
                {
                    spaceId,
                    resourceId: logicalResourceId,
                    projectionId: assignment.projection_resource_id,
                    kind,
                    title: state.title,
                    completed: state.completed,
                    fields: state.fields,
                    headOperationId: state.clientOpId,
                    conflict: hasConflict,
                },
            ];
        }
    };

    const applyPimOperation = async (
        spaceId: string,
        streamId: string,
        clientOpId: string,
        operation: PimOperationV1,
        spaceSeq = 0,
    ): Promise<void> => {
        if (operation.resource_id !== streamId) {
            throw new Error("PIM operation stream does not match its resource.");
        }
        const existingOperation = operationStates.find(
            (state) =>
                state.spaceId === spaceId && state.clientOpId === clientOpId,
        );
        if (existingOperation) {
            if (spaceSeq > existingOperation.spaceSeq) {
                operationStates = operationStates.map((state) =>
                    state.spaceId === spaceId && state.clientOpId === clientOpId
                        ? { ...state, spaceSeq }
                        : state,
                );
            }
            return;
        }
        const parent = operation.dependencies.length === 0
            ? undefined
            : operationStates.find(
                  (state) =>
                      state.clientOpId === operation.dependencies[0] &&
                      state.spaceId === spaceId &&
                      state.logicalResourceId === operation.resource_id &&
                      state.kind === operation.resource_kind,
              );
        if (operation.dependencies.length > 0 && !parent) {
            throw new Error("PIM operation dependency is missing.");
        }
        if (operation.operation === "delete") {
            operationStates = [
                ...operationStates,
                {
                    spaceId,
                    clientOpId,
                    spaceSeq,
                    logicalResourceId: operation.resource_id,
                    projectionId: operation.resource_id,
                    parentOperationId: operation.dependencies[0],
                    kind: operation.resource_kind,
                    title: parent?.title ?? "",
                    completed: parent?.completed ?? false,
                    fields: parent?.fields ?? {},
                    deleted: true,
                    materializedProjection: "",
                },
            ];
            await reconcilePimStream(
                spaceId,
                operation.resource_id,
                operation.resource_kind,
            );
            return;
        }
        const materializedProjection = await materializePimOperation(
            operation,
            parent?.materializedProjection || undefined,
        );
        const titleValue = operation.fields.title;
        const completedValue = operation.fields.completed;
        const projectedFields = projectionFields(
            materializedProjection,
            operation.resource_kind,
        );
        const fields = {
            ...(parent?.fields ?? {}),
            ...projectedFields,
            ...operation.fields,
        };
        const projectedTitle = projectedFields.title;
        const projectedCompleted = projectedFields.completed;
        const title =
                titleValue?.type === "text"
                    ? titleValue.value
                    : projectedTitle?.type === "text"
                      ? projectedTitle.value
                      : (parent?.title ?? "Untitled");
        const completed =
                completedValue?.type === "boolean"
                    ? completedValue.value
                    : projectedCompleted?.type === "boolean"
                      ? projectedCompleted.value
                      : (parent?.completed ?? false);
        operationStates = [
            ...operationStates,
            {
                spaceId,
                clientOpId,
                spaceSeq,
                logicalResourceId: operation.resource_id,
                projectionId: operation.resource_id,
                parentOperationId: operation.dependencies[0],
                kind: operation.resource_kind,
                title,
                completed,
                fields,
                deleted: false,
                materializedProjection,
            },
        ];
        await reconcilePimStream(
            spaceId,
            operation.resource_id,
            operation.resource_kind,
        );
    };

    const applyPimSnapshot = async (
        spaceId: string,
        streamId: string,
        snapshot: PimSnapshotV2,
        transportSeq?: number,
    ): Promise<void> => {
        if (snapshot.resource_id !== streamId) {
            throw new Error("PIM snapshot stream does not match its resource.");
        }
        if (
            transportSeq !== undefined &&
            snapshot.covers_through_space_seq > transportSeq
        ) {
            throw new Error("PIM snapshot claims data beyond its transport position.");
        }
        for (const branch of snapshot.branches) {
            const existingIndex = operationStates.findIndex(
                (state) =>
                    state.spaceId === spaceId &&
                    state.clientOpId === branch.head_operation_id,
            );
            if (existingIndex >= 0) {
                operationStates = operationStates.map((state, index) =>
                    index === existingIndex
                        ? {
                              ...state,
                              spaceSeq: Math.max(
                                  state.spaceSeq,
                                  snapshot.covers_through_space_seq,
                              ),
                          }
                        : state,
                );
                continue;
            }
            const hasNewerProjection = operationStates.some(
                (state) =>
                    state.spaceId === spaceId &&
                    state.logicalResourceId === snapshot.resource_id &&
                    state.projectionId === branch.projection_resource_id &&
                    (state.spaceSeq === 0 ||
                        state.spaceSeq > snapshot.covers_through_space_seq),
            );
            if (hasNewerProjection) continue;

            const projection = branch.deleted
                ? ""
                : new TextDecoder("utf-8", { fatal: true }).decode(
                      branch.materialized_projection,
                  );
            const fields = branch.deleted
                ? {}
                : projectionFields(projection, snapshot.resource_kind);
            const title =
                fields.title?.type === "text"
                    ? fields.title.value
                    : "Untitled";
            const completed =
                fields.completed?.type === "boolean"
                    ? fields.completed.value
                    : false;
            operationStates = [
                ...operationStates,
                {
                    spaceId,
                    clientOpId: branch.head_operation_id,
                    spaceSeq: snapshot.covers_through_space_seq,
                    logicalResourceId: snapshot.resource_id,
                    projectionId: branch.projection_resource_id,
                    seedProjectionId: branch.projection_resource_id,
                    kind: snapshot.resource_kind,
                    title: branch.deleted ? "" : title,
                    completed: branch.deleted ? false : completed,
                    fields: branch.deleted ? {} : fields,
                    deleted: branch.deleted,
                    materializedProjection: projection,
                },
            ];
        }
        await reconcilePimStream(
            spaceId,
            snapshot.resource_id,
            snapshot.resource_kind,
        );
    };

    const retryUnresolvedPimOperations = async (
        spaceId: string,
        signingKeys: Map<string, Uint8Array>,
    ): Promise<number> => {
        let applied = 0;
        while (true) {
            const pending = (await listQuarantinedOperationRecords(spaceId))
                .filter((record) => record.reason_code === "unresolved_pim_graph")
                .sort((left, right) => left.space_seq - right.space_seq);
            if (pending.length === 0) return applied;
            let madeProgress = false;
            for (const record of pending) {
                const envelope = record.envelope;
                const signingKey = signingKeys.get(envelope.author_device_id);
                const operationKey = await loadSpaceKey(spaceId, envelope.key_epoch);
                if (!signingKey || !operationKey || envelope.envelope_kind !== "operation") {
                    continue;
                }
                try {
                    await verifyOperationEnvelope(envelope, signingKey);
                    const operation = decodePimOperation(
                        await openOperationEnvelope(envelope, operationKey),
                    );
                    if (operation.resource_id !== envelope.stream_id) continue;
                    await applyPimOperation(
                        spaceId,
                        envelope.stream_id,
                        envelope.client_op_id,
                        operation,
                        record.space_seq,
                    );
                    await removeQuarantinedOperationEnvelope(
                        spaceId,
                        envelope.client_op_id,
                    );
                    applied += 1;
                    madeProgress = true;
                } catch {
                    // The graph may still be incomplete. A future page or sync
                    // retries the encrypted, authenticated envelope.
                }
            }
            if (!madeProgress) return applied;
        }
    };

    const loadSigningKeys = async (
        spaceId: string,
        refresh = false,
    ): Promise<Map<string, Uint8Array>> => {
        const cached = signingKeysBySpace.get(spaceId);
        if (cached && !refresh) return cached;
        const directory = await withAccessRetry((accessToken) =>
            cloudApi.listSpaceDevices(
                $appState.cloudBaseUrl,
                spaceId,
                accessToken,
            ),
        );
        const signingKeys = new Map(
            directory.devices.map((item) => [
                item.device_id,
                item.signing_public_key,
            ]),
        );
        signingKeysBySpace.set(spaceId, signingKeys);
        return signingKeys;
    };

    /** Pulls only the signed operation delta for one already-unlocked space. */
    const syncSpaceOperations = async (
        spaceId: string,
        minimumCursor = 0,
        refreshSigningKeys = false,
    ): Promise<number> => {
        let signingKeys = await loadSigningKeys(spaceId, refreshSigningKeys);
        await retryUnresolvedPimOperations(spaceId, signingKeys);
        let cursor = Math.max(syncCursors[spaceId] ?? 0, minimumCursor);
        if ((syncCursors[spaceId] ?? 0) < cursor) {
            syncCursors = { ...syncCursors, [spaceId]: cursor };
        }
        let synced = 0;
        while (true) {
            const page = await withAccessRetry((accessToken) =>
                cloudApi.listOperations(
                    $appState.cloudBaseUrl,
                    spaceId,
                    cursor,
                    accessToken,
                ),
            );
            for (const stored of page.operations) {
                let signingKey = signingKeys.get(
                    stored.envelope.author_device_id,
                );
                if (!signingKey) {
                    signingKeys = await loadSigningKeys(spaceId, true);
                    signingKey = signingKeys.get(stored.envelope.author_device_id);
                    if (!signingKey) {
                        throw new Error(
                            `Missing author key for operation ${stored.envelope.client_op_id}`,
                        );
                    }
                }
                await verifyOperationEnvelope(stored.envelope, signingKey);
                const operationKey = await loadSpaceKey(
                    spaceId,
                    stored.envelope.key_epoch,
                );
                if (!operationKey) {
                    throw new Error(
                        `Missing key epoch ${stored.envelope.key_epoch} for operation ${stored.envelope.client_op_id}`,
                    );
                }
                let plaintext: Uint8Array;
                try {
                    plaintext = await openOperationEnvelope(
                        stored.envelope,
                        operationKey,
                    );
                } catch {
                    await quarantineOperationEnvelope(
                        stored.envelope,
                        stored.space_seq,
                        "invalid_ciphertext",
                    );
                    continue;
                }
                if (stored.envelope.envelope_kind === "operation") {
                    let operation: PimOperationV1;
                    try {
                        operation = decodePimOperation(plaintext);
                        if (operation.resource_id !== stored.envelope.stream_id) {
                            throw new Error("stream mismatch");
                        }
                    } catch {
                        await quarantineOperationEnvelope(
                            stored.envelope,
                            stored.space_seq,
                            "invalid_operation",
                        );
                        continue;
                    }
                    try {
                        await applyPimOperation(
                            spaceId,
                            stored.envelope.stream_id,
                            stored.envelope.client_op_id,
                            operation,
                            stored.space_seq,
                        );
                    } catch {
                        await quarantineOperationEnvelope(
                            stored.envelope,
                            stored.space_seq,
                            "unresolved_pim_graph",
                        );
                        continue;
                    }
                } else if (stored.envelope.envelope_kind === "snapshot") {
                    let snapshot: PimSnapshotV2;
                    try {
                        snapshot = decodePimSnapshot(plaintext);
                        if (
                            snapshot.resource_id !== stored.envelope.stream_id ||
                            snapshot.covers_through_space_seq > stored.space_seq
                        ) {
                            throw new Error("snapshot context mismatch");
                        }
                    } catch {
                        await quarantineOperationEnvelope(
                            stored.envelope,
                            stored.space_seq,
                            "invalid_snapshot",
                        );
                        continue;
                    }
                    await applyPimSnapshot(
                        spaceId,
                        stored.envelope.stream_id,
                        snapshot,
                        stored.space_seq,
                    );
                } else {
                    throw new Error("Unsupported mandatory key-control envelope.");
                }
            }
            await retryUnresolvedPimOperations(spaceId, signingKeys);
            synced += page.operations.length;
            if (page.next_cursor <= cursor) break;
            cursor = page.next_cursor;
            syncCursors = { ...syncCursors, [spaceId]: cursor };
            await storeMaterializedPimState(
                pimItems,
                operationStates,
                syncCursors,
            );
            if (page.operations.length === 0) break;
        }
        return synced;
    };

    const textField = (item: MaterializedPimItem, name: string): string => {
        const value = item.fields[name];
        return value?.type === "text" ? value.value : "";
    };

    const flushOutbox = async (): Promise<number> => {
        const queued = await listQueuedOperationEnvelopes();
        let flushed = 0;
        for (const envelope of queued) {
            try {
                const appended = await withAccessRetry((accessToken) =>
                    cloudApi.appendOperation(
                        $appState.cloudBaseUrl,
                        envelope,
                        accessToken,
                    ),
                );
                operationStates = operationStates.map((state) =>
                    state.spaceId === envelope.space_id &&
                    state.clientOpId === envelope.client_op_id
                        ? {
                              ...state,
                              spaceSeq: Math.max(
                                  state.spaceSeq,
                                  appended.space_seq,
                              ),
                          }
                        : state,
                );
                await storeMaterializedPimState(
                    pimItems,
                    operationStates,
                    syncCursors,
                );
                await removeQueuedOperationEnvelope(
                    envelope.space_id,
                    envelope.client_op_id,
                );
                flushed += 1;
            } catch {
                break;
            }
        }
        return flushed;
    };

    const commitPimOperation = async (
        operation: PimOperationV1,
    ): Promise<boolean> => runDataPlaneExclusive(async () => {
        const collection = $appState.collections.find(
            (item) => item.id === selectedCollectionId,
        );
        if (!collection) {
            throw new Error(localized("Choose a collection first.", "Сначала выберите пространство."));
        }
        if (collection.role === "reader") {
            throw new Error(localized(
                "Reader access does not allow changes.",
                "Доступ только для чтения не разрешает изменения.",
            ));
        }
        const spaceKey = await loadSpaceKey(collection.id, collection.keyEpoch);
        if (!spaceKey) {
            throw new Error(
                "This device has no key for the selected collection.",
            );
        }
        const device = getActiveWebDevice();
        const clientOpId = crypto.randomUUID();
        const envelope = await sealOperationEnvelope({
            spaceId: collection.id,
            streamId: operation.resource_id,
            clientOpId,
            authorDeviceId: device.deviceId,
            keyEpoch: collection.keyEpoch,
            envelopeKind: "operation",
            plaintext: encodePimOperation(operation),
            spaceKey,
            signingPrivateKey: device.identity.signing_private_key,
        });
        await queueOperationEnvelope(envelope);
        await applyPimOperation(
            collection.id,
            operation.resource_id,
            clientOpId,
            operation,
        );
        await storeMaterializedPimState(pimItems, operationStates, syncCursors);
        const flushed = (await flushOutbox()) > 0;
        const pending = (await listQueuedOperationEnvelopes()).length;
        setPendingOperations(pending);
        autoSync.request("local-change");
        return flushed;
    });

    const makeUpsert = (
        kind: PimResourceKind,
        resourceId: string,
        fields: Record<string, PimValue>,
        dependencies: string[] = [],
    ): PimOperationV1 => ({
        operation: "upsert",
        resource_kind: kind,
        resource_id: resourceId,
        dependencies,
        fields,
        raw_projection: new Uint8Array(),
    });

    const createTask = async () => {
        const title = taskTitle.trim();
        if (!title) {
            taskFormError = localized("Enter a task title.", "Введите название задачи.");
            return;
        }
        taskFormError = "";
        setLoading("task-create");
        try {
            const flushed = await commitPimOperation(
                makeUpsert("task", crypto.randomUUID(), {
                    title: { type: "text", value: title },
                    completed: { type: "boolean", value: false },
                    dtstamp: { type: "text", value: dateToIcalendarUtc(new Date()) },
                }),
            );
            taskTitle = "";
            setNotice(
                flushed
                    ? localized("Task encrypted and synced.", "Задача зашифрована и синхронизирована.")
                    : localized("Task saved to the encrypted offline outbox.", "Задача сохранена в зашифрованной офлайн-очереди."),
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            taskFormError = `${localized("Task creation failed", "Не удалось создать задачу")}: ${message}`;
            notify(taskFormError, { kind: "error", source: t("Tasks") });
        } finally {
            clearLoading();
        }
    };

    const setTaskCompleted = async (
        item: MaterializedPimItem,
        completed: boolean,
    ) => {
        selectedCollectionId = item.spaceId;
        setLoading(`task-${item.resourceId}`);
        try {
            await commitPimOperation(
                makeUpsert(
                    "task",
                    item.resourceId,
                    { completed: { type: "boolean", value: completed } },
                    [item.headOperationId],
                ),
            );
            setNotice(completed
                ? localized("Task completed.", "Задача выполнена.")
                : localized("Task reopened.", "Задача снова открыта."));
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Task update failed", "Не удалось обновить задачу")}: ${message}`);
        } finally {
            clearLoading();
        }
    };

    const deletePimItem = async (item: MaterializedPimItem) => {
        selectedCollectionId = item.spaceId;
        setLoading(`delete-${item.resourceId}`);
        try {
            await commitPimOperation({
                operation: "delete",
                resource_kind: item.kind,
                resource_id: item.resourceId,
                dependencies: [item.headOperationId],
                projection_resource_id: null,
            });
            setNotice(localized(
                "Item moved to encrypted tombstone history.",
                "Элемент перемещён в зашифрованную историю удалений.",
            ));
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Delete failed", "Не удалось удалить")}: ${message}`);
        } finally {
            clearLoading();
        }
    };

    const createCalendarEvent = async () => {
        const title = eventTitle.trim();
        if (!title || !eventStart) {
            eventFormError = localized(
                "Event title and start time are required.",
                "Введите название и время начала события.",
            );
            return;
        }
        if (eventEnd && eventEnd < eventStart) {
            eventFormError = localized(
                "Event end must not be earlier than its start.",
                "Время окончания события не может быть раньше начала.",
            );
            return;
        }
        eventFormError = "";
        setLoading("event-create");
        try {
            const startsAt = localDateTimeToIcalendarUtc(eventStart);
            const endsAt = eventEnd
                ? localDateTimeToIcalendarUtc(eventEnd)
                : undefined;
            if (endsAt && endsAt < startsAt) {
                throw new Error(localized(
                    "Event end must not be earlier than its start.",
                    "Время окончания события не может быть раньше начала.",
                ));
            }
            const fields: Record<string, PimValue> = {
                title: { type: "text", value: title },
                dtstamp: { type: "text", value: dateToIcalendarUtc(new Date()) },
                starts_at: { type: "text", value: startsAt },
            };
            if (endsAt) {
                fields.ends_at = { type: "text", value: endsAt };
            }
            const flushed = await commitPimOperation(
                makeUpsert("calendar_event", crypto.randomUUID(), fields),
            );
            eventTitle = "";
            eventStart = "";
            eventEnd = "";
            setNotice(
                flushed
                    ? localized("Event encrypted and synced.", "Событие зашифровано и синхронизировано.")
                    : localized("Event saved to the encrypted offline outbox.", "Событие сохранено в зашифрованной офлайн-очереди."),
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            eventFormError = `${localized("Event creation failed", "Не удалось создать событие")}: ${message}`;
            notify(eventFormError, { kind: "error", source: t("Calendar") });
        } finally {
            clearLoading();
        }
    };

    const createContact = async () => {
        const name = contactName.trim();
        const email = contactEmail.trim();
        const phone = contactPhone.trim();
        if (!name || (!email && !phone)) {
            contactFormError = localized(
                "Contact name and an email or phone number are required.",
                "Введите имя контакта и email или номер телефона.",
            );
            return;
        }
        contactFormError = "";
        setLoading("contact-create");
        try {
            const fields: Record<string, PimValue> = {
                title: { type: "text", value: name },
            };
            if (email) {
                fields.email = { type: "text", value: email };
            }
            if (phone) {
                fields.phone = { type: "text", value: phone };
            }
            const flushed = await commitPimOperation(
                makeUpsert("contact", crypto.randomUUID(), fields),
            );
            contactName = "";
            contactEmail = "";
            contactPhone = "";
            setNotice(
                flushed
                    ? localized("Contact encrypted and synced.", "Контакт зашифрован и синхронизирован.")
                    : localized("Contact saved to the encrypted offline outbox.", "Контакт сохранён в зашифрованной офлайн-очереди."),
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            contactFormError = `${localized("Contact creation failed", "Не удалось создать контакт")}: ${message}`;
            notify(contactFormError, { kind: "error", source: t("Contacts") });
        } finally {
            clearLoading();
        }
    };

    const requestCollectionDelete = (collectionId: string) => {
        pendingDeleteCollectionId = collectionId;
        deleteModalOpen = true;
    };

    const deleteCollection = async () => {
        if (!pendingDeleteCollectionId) {
            return;
        }
        const collectionId = pendingDeleteCollectionId;
        setLoading("space-delete");
        try {
            await withAccessRetry((accessToken) =>
                cloudApi.moveSpaceToTrash(
                    $appState.cloudBaseUrl,
                    collectionId,
                    accessToken,
                ),
            );
            const removed = $appState.collections.find(
                (item) => item.id === collectionId,
            );
            appState.update((state) => ({
                ...state,
                collections: state.collections.filter(
                    (item) => item.id !== collectionId,
                ),
            }));
            notify(
                localized(
                    "Space moved to trash for 30 days.",
                    "Пространство перемещено в корзину на 30 дней.",
                ),
                { kind: "success", source: localized("Spaces", "Пространства") },
            );
            if (removed) {
                trashedCollections = [removed, ...trashedCollections];
            }
            pimItems = pimItems.filter((item) => item.spaceId !== collectionId);
            operationStates = operationStates.filter(
                (state) => state.spaceId !== collectionId,
            );
            const remainingCursors = { ...syncCursors };
            delete remainingCursors[collectionId];
            syncCursors = remainingCursors;
            signingKeysBySpace.delete(collectionId);
            await storeMaterializedPimState(
                pimItems,
                operationStates,
                syncCursors,
            );
            if (selectedCollectionId === collectionId) {
                selectedCollectionId = "";
            }
            pendingDeleteCollectionId = "";
            deleteModalOpen = false;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Space deletion failed", "Не удалось удалить пространство")}: ${message}`);
        } finally {
            clearLoading();
        }
    };

    const restoreCollection = async (collectionId: string) => {
        setLoading(`space-restore-${collectionId}`);
        try {
            await withAccessRetry((accessToken) =>
                cloudApi.restoreSpace(
                    $appState.cloudBaseUrl,
                    collectionId,
                    accessToken,
                ),
            );
            trashedCollections = trashedCollections.filter(
                (item) => item.id !== collectionId,
            );
            setNotice(localized(
                "Space restored. Syncing its encrypted history...",
                "Пространство восстановлено. Синхронизируем зашифрованную историю…",
            ));
            await syncNow();
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Space restore failed", "Не удалось восстановить пространство")}: ${message}`);
        } finally {
            clearLoading();
        }
    };

    interface SyncRunOptions {
        announce?: boolean;
        showLoading?: boolean;
    }

    /** Pulls accessible spaces, key metadata, and operation deltas. */
    const syncNowUnlocked = async (
        options: SyncRunOptions = {},
    ): Promise<boolean> => {
        if (!navigator.onLine) {
            markSyncFailure(localized("You are offline.", "Нет подключения к сети."), true);
            return false;
        }
        if (options.showLoading) setLoading("sync-now");
        markSyncing();
        try {
            const response = await withAccessRetry((accessToken) =>
                cloudApi.listSpaces($appState.cloudBaseUrl, accessToken),
            );
            const recoveryResponse = await withAccessRetry((accessToken) =>
                cloudApi.listRecoveryKeyPackages(
                    $appState.cloudBaseUrl,
                    accessToken,
                ),
            );
            const recoveryPackages = new Map(
                recoveryResponse.packages.map((item) => [
                    `${item.space_id}:${item.key_epoch}`,
                    item.encrypted_key_package,
                ]),
            );
            const trashResponse = await withAccessRetry((accessToken) =>
                cloudApi.listTrashedSpaces(
                    $appState.cloudBaseUrl,
                    accessToken,
                ),
            );
            const device = getActiveWebDevice();
            const collections: SpaceEntry[] = [];
            const nextSpaceMembers: Record<string, SpaceMemberSummary[]> = {};
            let syncedOperationCount = 0;
            const accessibleSpaceIds = new Set(
                response.spaces.map((space) => space.space_id),
            );
            pimItems = pimItems.filter((item) =>
                accessibleSpaceIds.has(item.spaceId),
            );
            operationStates = operationStates.filter((state) =>
                accessibleSpaceIds.has(state.spaceId),
            );
            syncCursors = Object.fromEntries(
                Object.entries(syncCursors).filter(([spaceId]) =>
                    accessibleSpaceIds.has(spaceId),
                ),
            );
            for (const space of response.spaces) {
                if (space.role === "owner") {
                    const memberDirectory = await withAccessRetry((accessToken) =>
                        cloudApi.listSpaceMembers(
                            $appState.cloudBaseUrl,
                            space.space_id,
                            accessToken,
                        ),
                    );
                    nextSpaceMembers[space.space_id] = memberDirectory.members;
                }
                let key = await loadSpaceKey(space.space_id, space.key_epoch);
                if (!key) {
                    const recoveryPackage = recoveryPackages.get(
                        `${space.space_id}:${space.key_epoch}`,
                    );
                    if (recoveryPackage) {
                        key = await withActiveMasterKey((masterKey) =>
                            unwrapSpaceKeyFromAccountRecovery(
                                masterKey,
                                decode(recoveryPackage),
                            ),
                        );
                        if (key.length !== 32) {
                            throw new Error(
                                `Invalid recovery key package for space ${space.space_id}`,
                            );
                        }
                        await storeSpaceKey(space.space_id, space.key_epoch, key);
                    }
                }
                if (!key) {
                    const packageForDevice = space.device_key_packages.find(
                        (item) =>
                            item.device_id === device.deviceId &&
                            item.key_epoch === space.key_epoch,
                    );
                    if (packageForDevice) {
                        key = await unwrapSpaceKeyForDevice(
                            decode(packageForDevice.encrypted_key_package),
                            device.identity.hpke_private_key,
                        );
                        await storeSpaceKey(space.space_id, space.key_epoch, key);
                    }
                }
                const packageForThisDevice = space.device_key_packages.find(
                    (item) =>
                        item.device_id === device.deviceId &&
                        item.key_epoch === space.key_epoch,
                );
                if (key && !packageForThisDevice) {
                    const encryptedDevicePackage = encode(
                        await wrapSpaceKeyForDevice(
                            key,
                            device.identity.hpke_public_key,
                        ),
                    );
                    await withAccessRetry((accessToken) =>
                        cloudApi.putDeviceKeyPackage(
                            $appState.cloudBaseUrl,
                            space.space_id,
                            {
                                device_id: device.deviceId,
                                key_epoch: space.key_epoch,
                                encrypted_key_package: encryptedDevicePackage,
                            },
                            accessToken,
                        ),
                    );
                }
                if (
                    key &&
                    !recoveryPackages.has(`${space.space_id}:${space.key_epoch}`)
                ) {
                    const recoveryPackage = await withActiveMasterKey(
                        async (masterKey) =>
                            encode(await wrapSpaceKeyForAccountRecovery(masterKey, key)),
                    );
                    await withAccessRetry((accessToken) =>
                        cloudApi.putRecoveryKeyPackage(
                            $appState.cloudBaseUrl,
                            space.space_id,
                            space.key_epoch,
                            recoveryPackage,
                            accessToken,
                        ),
                    );
                }
                let name = `Space ${space.space_id.slice(0, 8)}`;
                if (key) {
                    try {
                        const metadata = decode(
                            await decryptVaultBytes(key, space.encrypted_metadata),
                        ) as { name?: string };
                        name = metadata.name?.trim() || name;
                    } catch {
                        // Preserve an opaque fallback name for corrupt/old metadata.
                    }
                }
                collections.push({
                    id: space.space_id,
                    name,
                    keyAvailable: Boolean(key),
                    keyEpoch: space.key_epoch,
                    role: space.role,
                    syncedItems: 0,
                });
                if (key) {
                    syncedOperationCount += await syncSpaceOperations(
                        space.space_id,
                        Math.max(
                            space.history_start_seq,
                            space.current_state_start_seq,
                        ),
                        true,
                    );
                }
            }

            const queued = await listQueuedOperationEnvelopes();
            for (const envelope of queued) {
                const key = await loadSpaceKey(envelope.space_id, envelope.key_epoch);
                if (!key) continue;
                const plaintext = await openOperationEnvelope(envelope, key);
                if (envelope.envelope_kind === "operation") {
                    await applyPimOperation(
                        envelope.space_id,
                        envelope.stream_id,
                        envelope.client_op_id,
                        decodePimOperation(plaintext),
                    );
                } else if (envelope.envelope_kind === "snapshot") {
                    await applyPimSnapshot(
                        envelope.space_id,
                        envelope.stream_id,
                        decodePimSnapshot(plaintext),
                    );
                }
            }
            const flushed = await flushOutbox();
            await storeMaterializedPimState(
                pimItems,
                operationStates,
                syncCursors,
            );

            const trash: SpaceEntry[] = [];
            for (const space of trashResponse.spaces) {
                const key = await loadSpaceKey(space.space_id, space.key_epoch);
                let name = `Space ${space.space_id.slice(0, 8)}`;
                if (key) {
                    try {
                        const metadata = decode(
                            await decryptVaultBytes(key, space.encrypted_metadata),
                        ) as { name?: string };
                        name = metadata.name?.trim() || name;
                    } catch {
                        // Keep opaque fallback when local key or metadata is stale.
                    }
                }
                trash.push({
                    id: space.space_id,
                    name,
                    keyAvailable: Boolean(key),
                    keyEpoch: space.key_epoch,
                    role: space.role,
                    syncedItems: 0,
                });
            }
            trashedCollections = trash;
            spaceMembers = nextSpaceMembers;
            for (const collection of collections) {
                collection.syncedItems = operationStates.filter(
                    (operation) => operation.spaceId === collection.id,
                ).length;
            }

            appState.update((state) => ({
                ...state,
                collections,
                syncedItemsTotal: operationStates.length,
                lastSyncedSeq: Math.max(0, ...Object.values(syncCursors)),
            }));
            const pending = (await listQueuedOperationEnvelopes()).length;
            markSyncSuccess(pending);
            lastMetadataSyncAt = Date.now();
            if (options.announce) {
                notify(
                    localized(
                        `Sync completed: ${syncedOperationCount} operations, ${flushed} outbox items uploaded.`,
                        `Синхронизация завершена: операций — ${syncedOperationCount}, отправлено из очереди — ${flushed}.`,
                    ),
                    { kind: "success", source: localized("Sync", "Синхронизация") },
                );
            }
            return true;
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            markSyncFailure(message, !navigator.onLine);
            if (options.announce) {
                notify(`${localized("Sync failed", "Ошибка синхронизации")}: ${message}`, {
                    kind: "error",
                    source: localized("Sync", "Синхронизация"),
                    persistent: true,
                });
            }
            return false;
        } finally {
            if (options.showLoading) clearLoading();
        }
    };

    /** Fast path used by automatic sync after space keys are already known. */
    const syncOperationDeltasUnlocked = async (): Promise<boolean> => {
        if (!navigator.onLine) {
            markSyncFailure(localized("You are offline.", "Нет подключения к сети."), true);
            return false;
        }
        markSyncing();
        try {
            const flushed = await flushOutbox();
            for (const collection of $appState.collections) {
                if (!collection.keyAvailable) continue;
                await syncSpaceOperations(collection.id);
            }
            await storeMaterializedPimState(
                pimItems,
                operationStates,
                syncCursors,
            );
            const pending = (await listQueuedOperationEnvelopes()).length;
            appState.update((state) => ({
                ...state,
                collections: state.collections.map((collection) => ({
                    ...collection,
                    syncedItems: operationStates.filter(
                        (operation) => operation.spaceId === collection.id,
                    ).length,
                })),
                syncedItemsTotal: operationStates.length,
                lastSyncedSeq: Math.max(0, ...Object.values(syncCursors)),
            }));
            markSyncSuccess(pending);
            if (flushed > 0) setPendingOperations(pending);
            return true;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            markSyncFailure(message, !navigator.onLine);
            return false;
        }
    };

    const syncNow = (): Promise<boolean> =>
        runDataPlaneExclusive(() =>
            syncNowUnlocked({ announce: true, showLoading: true }),
        );

    const runAutomaticSyncInThisTab = (reason: SyncReason): Promise<boolean> =>
        runDataPlaneExclusive(() =>
            reason === "initial" ||
            $appState.collections.length === 0 ||
            Date.now() - lastMetadataSyncAt >= 5 * 60_000
                ? syncNowUnlocked()
                : syncOperationDeltasUnlocked(),
        );

    const runAutomaticSync = async (reason: SyncReason): Promise<boolean> => {
        if (!navigator.locks) return runAutomaticSyncInThisTab(reason);
        return navigator.locks.request(
            "kamori-web-auto-sync-v1",
            { ifAvailable: true },
            (lock) => lock ? runAutomaticSyncInThisTab(reason) : true,
        );
    };

    const autoSync = new AutoSyncCoordinator({
        run: runAutomaticSync,
        ready: () => Boolean(tokenStore.getAccessToken()),
    });

    const accountRecoveryPublicKey = (
        member: SpaceMemberSummary,
    ): Uint8Array => {
        const bundle = decode(member.public_key_bundle) as {
            version?: number;
            account_recovery_public_key?: unknown;
        };
        if (bundle.version !== 2) {
            throw new Error(
                `Member ${member.username} has an invalid recovery public key.`,
            );
        }
        try {
            return normalizeByteArray(
                bundle.account_recovery_public_key,
                32,
                `Member ${member.username} recovery public key`,
            );
        } catch {
            throw new Error(
                `Member ${member.username} has an invalid recovery public key.`,
            );
        }
    };

    const buildRotationSnapshots = async (
        spaceId: string,
        newKeyEpoch: number,
        newSpaceKey: Uint8Array,
        baseSpaceSeq: number,
    ) => {
        const latestBranches = new Map<string, MaterializedOperationState>();
        for (const state of operationStates.filter(
            (candidate) => candidate.spaceId === spaceId,
        )) {
            if (state.spaceSeq === 0) {
                throw new Error(
                    "Every local operation must be uploaded before access can be revoked.",
                );
            }
            const branchKey = `${state.logicalResourceId}:${state.projectionId}`;
            const previous = latestBranches.get(branchKey);
            if (
                !previous ||
                state.spaceSeq > previous.spaceSeq ||
                (state.spaceSeq === previous.spaceSeq &&
                    state.clientOpId > previous.clientOpId)
            ) {
                latestBranches.set(branchKey, state);
            }
        }
        const streams = new Map<string, MaterializedOperationState[]>();
        for (const state of latestBranches.values()) {
            const branches = streams.get(state.logicalResourceId) ?? [];
            branches.push(state);
            streams.set(state.logicalResourceId, branches);
        }
        const device = getActiveWebDevice();
        return Promise.all(
            [...streams.entries()]
                .sort(([left], [right]) => left.localeCompare(right))
                .map(async ([streamId, states]) => {
                    states.sort((left, right) =>
                        left.projectionId.localeCompare(right.projectionId),
                    );
                    const resourceKind = states[0].kind;
                    if (states.some((state) => state.kind !== resourceKind)) {
                        throw new Error("A PIM stream contains mixed resource kinds.");
                    }
                    const branches: PimSnapshotBranchV2[] = states.map((state) => ({
                        projection_resource_id: state.projectionId,
                        head_operation_id: state.clientOpId,
                        deleted: state.deleted,
                        materialized_projection: state.deleted
                            ? new Uint8Array()
                            : textEncoder.encode(state.materializedProjection),
                    }));
                    const snapshot: PimSnapshotV2 = {
                        schema_version: 2,
                        covers_through_space_seq: baseSpaceSeq,
                        resource_kind: resourceKind,
                        resource_id: streamId,
                        branches,
                    };
                    return sealOperationEnvelope({
                        spaceId,
                        streamId,
                        clientOpId: crypto.randomUUID(),
                        authorDeviceId: device.deviceId,
                        keyEpoch: newKeyEpoch,
                        envelopeKind: "snapshot",
                        plaintext: encodePimSnapshot(snapshot),
                        spaceKey: newSpaceKey,
                        signingPrivateKey: device.identity.signing_private_key,
                    });
                }),
        );
    };

    const rotateSpaceKeyForMembershipChange = async (
        collection: SpaceEntry,
        target?: SpaceMemberSummary,
    ): Promise<{ keyEpoch: number; rotationId: string; spaceKey: Uint8Array }> =>
    runDataPlaneExclusive(async () => {
        if (!(await syncNowUnlocked())) {
            throw new Error("A complete sync is required before key rotation.");
        }
        const refreshedCollection = $appState.collections.find(
            (item) => item.id === collection.id,
        );
        if (!refreshedCollection || refreshedCollection.role !== "owner") {
            throw new Error("Only the current space owner can rotate membership keys.");
        }
        const queued = (await listQueuedOperationEnvelopes()).filter(
            (envelope) => envelope.space_id === collection.id,
        );
        if (queued.length > 0) {
            throw new Error("The encrypted outbox must be empty before key rotation.");
        }
        const baseSpaceSeq = syncCursors[collection.id] ?? 0;
        const [membersResponse, devicesResponse] = await Promise.all([
            withAccessRetry((accessToken) =>
                cloudApi.listSpaceMembers(
                    $appState.cloudBaseUrl,
                    collection.id,
                    accessToken,
                ),
            ),
            withAccessRetry((accessToken) =>
                cloudApi.listSpaceDevices(
                    $appState.cloudBaseUrl,
                    collection.id,
                    accessToken,
                ),
            ),
        ]);
        if (
            target &&
            !membersResponse.members.some(
                (member) =>
                    member.user_id === target.user_id && member.role !== "owner",
            )
        ) {
            throw new Error("The member is no longer active in this space.");
        }
        const newKeyEpoch = refreshedCollection.keyEpoch + 1;
        const newSpaceKey = crypto.getRandomValues(new Uint8Array(32));
        const remainingMembers = membersResponse.members.filter(
            (member) => !target || member.user_id !== target.user_id,
        );
        const remainingDevices = devicesResponse.devices.filter(
            (device) =>
                device.active && (!target || device.user_id !== target.user_id),
        );
        const remainingDevicePackages = await Promise.all(
            remainingDevices.map(async (device) => ({
                device_id: device.device_id,
                key_epoch: newKeyEpoch,
                encrypted_key_package: encode(
                    await wrapSpaceKeyForDevice(
                        newSpaceKey,
                        device.hpke_public_key,
                    ),
                ),
            })),
        );
        const remainingRecoveryPackages = await Promise.all(
            remainingMembers.map(async (member) => ({
                user_id: member.user_id,
                key_epoch: newKeyEpoch,
                encrypted_key_package: encode(
                    await wrapSpaceKeyForDevice(
                        newSpaceKey,
                        accountRecoveryPublicKey(member),
                    ),
                ),
            })),
        );
        const snapshots = await buildRotationSnapshots(
            collection.id,
            newKeyEpoch,
            newSpaceKey,
            baseSpaceSeq,
        );
        const snapshotStreams = new Set(
            snapshots.map((snapshot) => snapshot.stream_id),
        );
        const quarantinedStreams = [
            ...new Set(
                (await listQuarantinedOperationRecords(collection.id))
                    .filter((record) => record.space_seq <= baseSpaceSeq)
                    .map((record) => record.envelope.stream_id)
                    .filter((streamId) => !snapshotStreams.has(streamId)),
            ),
        ].sort();
        const rotationId = crypto.randomUUID();
        const payload = {
            rotation_id: rotationId,
            expected_key_epoch: refreshedCollection.keyEpoch,
            new_key_epoch: newKeyEpoch,
            base_space_seq: baseSpaceSeq,
            new_encrypted_metadata: await encryptVaultBytes(
                newSpaceKey,
                encode({ name: refreshedCollection.name }),
            ),
            remaining_device_packages: remainingDevicePackages,
            remaining_recovery_packages: remainingRecoveryPackages,
            snapshots,
            quarantined_streams: quarantinedStreams,
        };
        if (target) {
            await withAccessRetry((accessToken) =>
                cloudApi.revokeSpaceMember(
                    $appState.cloudBaseUrl,
                    collection.id,
                    target.user_id,
                    payload,
                    accessToken,
                ),
            );
        } else {
            await withAccessRetry((accessToken) =>
                cloudApi.rotateSpaceKey(
                    $appState.cloudBaseUrl,
                    collection.id,
                    payload,
                    accessToken,
                ),
            );
        }
        await storeSpaceKey(collection.id, newKeyEpoch, newSpaceKey);
        if (!(await syncNowUnlocked())) {
            throw new Error(
                "The key rotated successfully, but refreshing local state failed. Sync again to recover it.",
            );
        }
        return { keyEpoch: newKeyEpoch, rotationId, spaceKey: newSpaceKey };
    });

    const revokeSpaceMember = async (
        collection: SpaceEntry,
        target: SpaceMemberSummary,
    ) => {
        if (
            !window.confirm(
                localized(
                    `Remove ${target.username} from ${collection.name}? Their future access will be revoked and the space key will rotate.`,
                    `Удалить ${target.username} из «${collection.name}»? Доступ к будущим изменениям будет отозван, а ключ пространства — сменён.`,
                ),
            )
        ) {
            return;
        }
        setLoading(`member-revoke-${collection.id}-${target.user_id}`);
        try {
            await rotateSpaceKeyForMembershipChange(collection, target);
            setNotice(
                localized(
                    `${target.username} was removed and the space key was rotated.`,
                    `${target.username} удалён, ключ пространства сменён.`,
                ),
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Member removal failed", "Не удалось удалить участника")}: ${message}`);
        } finally {
            clearLoading();
        }
    };

    /**
     * Creates invite code client-side and uploads hash + encrypted payloads.
     */
    const issueInviteCode = async () => {
        const collectionId =
            selectedCollectionId || $appState.collections[0]?.id;
        if (!collectionId) {
            setNotice(localized(
                "Create at least one space first.",
                "Сначала создайте хотя бы одно пространство.",
            ));
            return;
        }

        const collection = $appState.collections.find(
            (item) => item.id === collectionId,
        );
        if (!collection) {
            setNotice(localized("Selected space not found.", "Выбранное пространство не найдено."));
            return;
        }
        if (collection.role !== "owner") {
            setNotice(localized(
                "Only the space owner can create membership invites.",
                "Только владелец пространства может создавать приглашения.",
            ));
            return;
        }

        setLoading("invite-generate");
        try {
            inviteRedeemedNote = "";
            const inviteCode = generateInviteCode();
            const rotated = await rotateSpaceKeyForMembershipChange(collection);
            setLoading("invite-generate");
            const encryptedGroupKey = await wrapCollectionKeyWithInviteCode(
                rotated.spaceKey,
                inviteCode,
            );
            const encryptedNote = inviteNotePlaintext.trim()
                ? await wrapBytesWithInviteCode(
                      textEncoder.encode(inviteNotePlaintext.trim()),
                      inviteCode,
                  )
                : undefined;
            const inviteCodeHash = await hashInviteCode(inviteCode);

            await withAccessRetry((accessToken) =>
                cloudApi.createInviteCode(
                    $appState.cloudBaseUrl,
                    {
                        space_id: collection.id,
                        rotation_id: rotated.rotationId,
                        role: inviteRole,
                        invite_code_hash: inviteCodeHash,
                        encrypted_key_package: encryptedGroupKey,
                        encrypted_note: encryptedNote,
                        ttl_minutes: Number(inviteTtlMinutes),
                    },
                    accessToken,
                ),
            );

            inviteCodeIssued = inviteCode;
            inviteNotePlaintext = "";
            setNotice(localized("Invite code generated.", "Код приглашения создан."));
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Invite generation failed", "Не удалось создать приглашение")}: ${message}`);
        } finally {
            clearLoading();
        }
    };

    /**
     * Resolves invite code against cloud and decrypts wrapped collection key locally.
     */
    const redeemInviteCode = async () => {
        const code = inviteCodeToRedeem.trim();
        if (!code) {
            setNotice(localized("Invite code is required.", "Введите код приглашения."));
            return;
        }

        setLoading("invite-redeem");
        try {
            inviteRedeemedNote = "";
            const inviteCodeHash = await hashInviteCode(code);
            const redeemed = await withAccessRetry((accessToken) =>
                cloudApi.redeemInviteCode(
                    $appState.cloudBaseUrl,
                    inviteCodeHash,
                    accessToken,
                ),
            );
            const collectionKey = await unwrapCollectionKeyWithInviteCode(
                redeemed.encrypted_key_package,
                code,
            );
            await storeSpaceKey(redeemed.space_id, redeemed.key_epoch, collectionKey);
            const device = getActiveWebDevice();
            const devicePackage = encode(
                await wrapSpaceKeyForDevice(
                    collectionKey,
                    device.identity.hpke_public_key,
                ),
            );
            await withAccessRetry((accessToken) =>
                cloudApi.putDeviceKeyPackage(
                    $appState.cloudBaseUrl,
                    redeemed.space_id,
                    {
                        device_id: device.deviceId,
                        key_epoch: redeemed.key_epoch,
                        encrypted_key_package: devicePackage,
                    },
                    accessToken,
                ),
            );
            const recoveryPackage = await withActiveMasterKey(
                async (masterKey) =>
                    encode(
                        await wrapSpaceKeyForAccountRecovery(
                            masterKey,
                            collectionKey,
                        ),
                    ),
            );
            await withAccessRetry((accessToken) =>
                cloudApi.putRecoveryKeyPackage(
                    $appState.cloudBaseUrl,
                    redeemed.space_id,
                    redeemed.key_epoch,
                    recoveryPackage,
                    accessToken,
                ),
            );
            let decryptedInviteNote = "";
            if (redeemed.encrypted_note && redeemed.encrypted_note.length > 0) {
                const decryptedBytes = await unwrapBytesWithInviteCode(
                    redeemed.encrypted_note,
                    code,
                );
                decryptedInviteNote = textDecoder.decode(decryptedBytes);
            }

            appState.update((state) => {
                const existingIndex = state.collections.findIndex(
                    (item) => item.id === redeemed.space_id,
                );
                if (existingIndex >= 0) {
                    const updated = [...state.collections];
                    updated[existingIndex] = {
                        ...updated[existingIndex],
                        keyAvailable: true,
                        keyEpoch: redeemed.key_epoch,
                        role: redeemed.role,
                    };
                    return {
                        ...state,
                        collections: updated,
                    };
                }

                return {
                    ...state,
                    collections: [
                        ...state.collections,
                        {
                            id: redeemed.space_id,
                            name: `Shared ${redeemed.space_id.slice(0, 8)}`,
                            keyAvailable: true,
                            keyEpoch: redeemed.key_epoch,
                            role: redeemed.role,
                            syncedItems: 0,
                        },
                    ],
                };
            });
            notify(
                localized(
                    `Invite redeemed for space ${redeemed.space_id.slice(0, 8)}.`,
                    `Приглашение в пространство ${redeemed.space_id.slice(0, 8)} принято.`,
                ),
                { kind: "success", source: localized("Sharing", "Доступ") },
            );

            inviteCodeToRedeem = "";
            inviteRedeemedNote = decryptedInviteNote;
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`${localized("Invite redemption failed", "Не удалось принять приглашение")}: ${message}`);
        } finally {
            clearLoading();
        }
    };

    $: if (!selectedCollectionId && $appState.collections.length > 0) {
        selectedCollectionId = $appState.collections[0].id;
    }
    $: selectedCollection = $appState.collections.find(
        (collection) => collection.id === selectedCollectionId,
    );
    $: canWriteSelectedCollection = Boolean(
        selectedCollection &&
        selectedCollection.keyAvailable &&
        selectedCollection.role !== "reader",
    );
    $: requestedSpaceId = $page.url.searchParams.get("space") ?? "";
    $: if (
        view === "spaces" &&
        $appState.collections.length === 0 &&
        !suggestedPersonalSpace
    ) {
        collectionName = localized("Personal", "Личное");
        suggestedPersonalSpace = true;
    }
    $: if (
        view === "sharing" &&
        requestedSpaceId &&
        $appState.collections.some((collection) => collection.id === requestedSpaceId)
    ) {
        selectedCollectionId = requestedSpaceId;
    }

    onMount(() => {
        deviceAuthorizationCode =
            new URLSearchParams(window.location.search)
                .get("device_code")
                ?.trim()
                .toUpperCase() ?? "";
        void (async () => {
            try {
                const materialized = await loadMaterializedPimState();
                pimItems = materialized.items;
                operationStates = materialized.operations;
                syncCursors = materialized.cursors;
            } catch (error) {
                const message =
                    error instanceof Error ? error.message : String(error);
                setNotice(`${localized("Encrypted offline state could not be opened", "Не удалось открыть зашифрованные офлайн-данные")}: ${message}`);
            } finally {
                stopManualSyncRegistration = registerManualSync(() => {
                    void syncNow();
                });
                autoSync.start(window, document);
            }
        })();
    });

    onDestroy(() => {
        autoSync.stop();
        stopManualSyncRegistration();
    });
</script>

{#if deviceAuthorizationCode}
    <Card>
        <h2 class="font-heading text-xl font-semibold text-slate">
            {$locale === "ru" ? "Подтвердить Kamori на компьютере" : "Authorize Kamori Desktop"}
        </h2>
        <p class="mt-2 text-sm text-slate/80">
            {$locale === "ru" ? "Убедитесь, что этот же код показан в приложении для компьютера:" : "Confirm that this code is also shown in your desktop app:"}
            <strong>{deviceAuthorizationCode}</strong>.
        </p>
        <div class="mt-3 flex gap-2">
            <Button
                on:click={approveDeviceAuthorization}
                disabled={loadingAction === "device-authorization"}
            >
                {loadingAction === "device-authorization"
                    ? ($locale === "ru" ? "Подтверждаем…" : "Approving...")
                    : t("Approve Desktop")}
            </Button>
            <Button variant="secondary" on:click={clearDeviceAuthorizationQuery}>
                {t("Cancel")}
            </Button>
        </div>
    </Card>
{/if}

{#if $appState.collections.length === 0 && $syncState.lastSuccessAt === null}
    <Card>
        <p class="text-xs font-semibold uppercase tracking-[0.18em] text-moss">
            {$locale === "ru" ? "Зашифрованные данные" : "Encrypted data"}
        </p>
        <h1 class="mt-2 font-heading text-2xl font-semibold text-slate">
            {$syncState.phase === "offline"
                ? ($locale === "ru" ? "Нет подключения к сети" : "You are offline")
                : $syncState.phase === "error"
                    ? ($locale === "ru" ? "Не удалось загрузить пространства" : "Spaces could not be loaded")
                    : ($locale === "ru" ? "Загружаем пространства…" : "Loading your spaces…")}
        </h1>
        {#if $syncState.error}
            <p class="mt-3 text-sm text-coral" role="alert">{$syncState.error}</p>
        {/if}
        {#if $syncState.phase === "offline" || $syncState.phase === "error"}
            <div class="mt-4">
                <Button on:click={syncNow}>{$locale === "ru" ? "Повторить" : "Retry"}</Button>
            </div>
        {/if}
    </Card>
{:else if $appState.collections.length === 0 && view !== "spaces" && view !== "sharing"}
    <Card>
        <p class="text-xs font-semibold uppercase tracking-[0.18em] text-moss">
            {$locale === "ru" ? "Первый шаг" : "First step"}
        </p>
        <h1 class="mt-2 font-heading text-2xl font-semibold text-slate">
            {$locale === "ru" ? "Создайте первое пространство" : "Create your first space"}
        </h1>
        <p class="mt-3 max-w-2xl text-sm leading-6 text-slate/75">
            {$locale === "ru"
                ? "Пространство хранит ваши задачи, события и контакты и определяет, кому разрешён доступ. Только после его создания можно добавлять данные."
                : "A space stores your tasks, events, and contacts and defines who may access them. Create one before adding data."}
        </p>
        <a
            class="mt-5 inline-flex bg-slate px-4 py-2.5 text-sm font-semibold text-white hover:bg-moss"
            href="/app/spaces"
        >{$locale === "ru" ? "Создать личное пространство →" : "Create personal space →"}</a>
    </Card>
{:else}
<div class="space-y-4">
    {#if view === "today"}
    <Card>
        <p class="text-xs font-semibold uppercase tracking-[0.18em] text-moss">
            {$locale === "ru" ? "Сегодня" : "Today"}
        </p>
        <h1 class="mt-2 font-heading text-2xl font-semibold text-slate">
            {$locale === "ru"
                ? `Здравствуйте, ${$appState.currentUsername}`
                : `Hello, ${$appState.currentUsername}`}
        </h1>
        <div class="mt-5 grid gap-3 sm:grid-cols-3">
            <a class="border border-slate/15 bg-white/70 p-4 hover:bg-sand/40" href="/app/tasks">
                <strong class="block text-2xl text-slate">{pimItems.filter((item) => item.kind === "task" && !item.completed).length}</strong>
                <span class="text-sm text-slate/65">{$locale === "ru" ? "открытых задач" : "open tasks"}</span>
            </a>
            <a class="border border-slate/15 bg-white/70 p-4 hover:bg-sand/40" href="/app/calendar">
                <strong class="block text-2xl text-slate">{pimItems.filter((item) => item.kind === "calendar_event").length}</strong>
                <span class="text-sm text-slate/65">{$locale === "ru" ? "событий" : "events"}</span>
            </a>
            <a class="border border-slate/15 bg-white/70 p-4 hover:bg-sand/40" href="/app/contacts">
                <strong class="block text-2xl text-slate">{pimItems.filter((item) => item.kind === "contact").length}</strong>
                <span class="text-sm text-slate/65">{$locale === "ru" ? "контактов" : "contacts"}</span>
            </a>
        </div>
        <div class="mt-4">
            <Button on:click={syncNow} disabled={loadingAction === "sync-now"}>
                {loadingAction === "sync-now" ? t("Syncing...") : t("Sync Now")}
            </Button>
        </div>
    </Card>
    {/if}

    {#if view === "spaces"}
    <Card>
        <h2 class="font-heading text-xl font-semibold text-slate">
            {t("Spaces")}
        </h2>
        <p class="mt-2 max-w-2xl text-sm text-slate/70">
            {$locale === "ru"
                ? "Пространство — независимая граница шифрования и совместного доступа для задач, календаря и контактов."
                : "A space is an independent encryption and sharing boundary for tasks, calendar events, and contacts."}
        </p>
        <div class="mt-3 space-y-2">
            <Input bind:value={collectionName} placeholder={t("Space name")} />
            <Button on:click={createCollection}>{t("Create Space")}</Button>
            {#if collectionFormError}
                <p class="text-sm text-coral" role="alert">{collectionFormError}</p>
            {/if}
        </div>

        <div class="mt-4 space-y-2">
            {#if $appState.collections.length === 0}
                <p class="text-sm text-slate/70">{t("No spaces yet.")}</p>
            {:else}
                {#each $appState.collections as collection}
                    <div
                        class="rounded-xl border border-slate/15 bg-white/70 p-3"
                        data-space-id={collection.id}
                    >
                        <p class="font-semibold text-slate">
                            {collection.name}
                        </p>
                        <p class="text-xs text-slate/65 break-all">
                            {collection.id}
                        </p>
                        <p class="mt-1 text-xs text-slate/65">
                            {collection.keyAvailable
                                ? `Key epoch ${collection.keyEpoch}`
                                : t("Approval required on another device")}
                        </p>
                        {#if collection.role === "owner"}
                        <div class="mt-2 flex flex-wrap gap-2">
                            <a
                                class="inline-flex border border-slate/20 px-3 py-2 text-xs font-semibold text-slate hover:bg-sand/50"
                                href={`/app/sharing?space=${encodeURIComponent(collection.id)}`}
                            >{$locale === "ru" ? "Участники и доступ" : "Members and access"}</a>
                            <Button
                                variant="danger"
                                on:click={() =>
                                    requestCollectionDelete(collection.id)}
                            >
                                {t("Delete")}
                            </Button>
                        </div>
                        {/if}
                    </div>
                {/each}
            {/if}
        </div>

        {#if trashedCollections.length > 0}
            <div class="mt-5 border-t border-slate/15 pt-4">
                <p class="text-xs font-semibold uppercase tracking-wide text-slate/70">
                    {t("Trash · retained for 30 days")}
                </p>
                <div class="mt-2 space-y-2">
                    {#each trashedCollections as collection}
                        <div class="rounded-xl border border-slate/15 bg-white/70 p-3">
                            <p class="font-semibold text-slate">{collection.name}</p>
                            <Button
                                variant="ghost"
                                on:click={() => restoreCollection(collection.id)}
                                disabled={loadingAction === `space-restore-${collection.id}`}
                            >{t("Restore")}</Button>
                        </div>
                    {/each}
                </div>
            </div>
        {/if}
    </Card>
    {/if}

    {#if view === "tasks"}
    <Card>
        <h2 class="font-heading text-xl font-semibold text-slate">{t("Tasks")}</h2>
        <p class="mt-2 text-xs text-slate/70">
            {$locale === "ru" ? "Изменения шифруются и подписываются локально. Офлайн-записи остаются в зашифрованной очереди до успешной синхронизации." : "Edits are encrypted and signed locally. Offline writes stay in the encrypted outbox until the next successful sync."}
        </p>
        <div class="mt-3 space-y-2">
            <select
                class="w-full rounded-xl border border-slate/20 bg-white px-3 py-2 text-sm text-slate outline-none"
                bind:value={selectedCollectionId}
            >
                {#each $appState.collections as collection}
                    <option value={collection.id}>{collection.name}</option>
                {/each}
            </select>

            {#if !canWriteSelectedCollection}
                <p class="border-l-4 border-gold bg-sand/45 p-3 text-sm text-slate">
                    {$locale === "ru" ? "Выбранное пространство доступно только для чтения или ожидает ключ устройства." : "The selected space is read-only or awaits this device key."}
                </p>
            {/if}
            <Input bind:value={taskTitle} placeholder={t("New task")} disabled={!canWriteSelectedCollection} />
            <Button
                on:click={createTask}
                disabled={loadingAction === "task-create" || !canWriteSelectedCollection}
            >
                {loadingAction === "task-create" ? t("Saving...") : t("Add Task")}
            </Button>
            {#if taskFormError}
                <p class="text-sm text-coral" role="alert">{taskFormError}</p>
            {/if}
        </div>
        <div class="mt-4 space-y-2">
            {#each pimItems.filter((item) => item.kind === "task" && item.spaceId === selectedCollectionId) as item}
                <div class="rounded-xl border border-slate/15 bg-white/70 p-3">
                    <p class="font-semibold text-slate">{item.title}</p>
                    <p class="text-xs text-slate/65">
                        {item.completed ? t("Completed") : t("Open")}
                        {item.conflict ? ($locale === "ru" ? " · проверьте параллельное изменение" : " · concurrent edit needs review") : ""}
                    </p>
                    <div class="mt-2 flex flex-wrap gap-2">
                        <Button
                            variant="ghost"
                            on:click={() =>
                                setTaskCompleted(item, !item.completed)}
                            disabled={loadingAction === `task-${item.resourceId}` || !canWriteSelectedCollection}
                        >
                            {item.completed ? t("Reopen") : t("Complete")}
                        </Button>
                        <Button
                            variant="danger"
                            on:click={() => deletePimItem(item)}
                            disabled={loadingAction === `delete-${item.resourceId}` || !canWriteSelectedCollection}
                        >{t("Delete")}</Button>
                    </div>
                </div>
            {:else}
                <p class="text-sm text-slate/70">{t("No tasks on this device yet.")}</p>
            {/each}
        </div>
    </Card>
    {/if}

    {#if view === "calendar"}
    <Card>
        <h2 class="font-heading text-xl font-semibold text-slate">{t("Calendar")}</h2>
        <div class="mt-3 space-y-2">
            <select
                class="w-full rounded-xl border border-slate/20 bg-white px-3 py-2 text-sm text-slate outline-none"
                bind:value={selectedCollectionId}
            >
                {#each $appState.collections as collection}
                    <option value={collection.id}>{collection.name}</option>
                {/each}
            </select>
            {#if !canWriteSelectedCollection}
                <p class="border-l-4 border-gold bg-sand/45 p-3 text-sm text-slate">
                    {$locale === "ru" ? "Выбранное пространство доступно только для чтения или ожидает ключ устройства." : "The selected space is read-only or awaits this device key."}
                </p>
            {/if}
            <Input bind:value={eventTitle} placeholder={t("Event title")} disabled={!canWriteSelectedCollection} />
            <Input bind:value={eventStart} type="datetime-local" disabled={!canWriteSelectedCollection} />
            <Input bind:value={eventEnd} type="datetime-local" disabled={!canWriteSelectedCollection} />
            <Button
                on:click={createCalendarEvent}
                disabled={loadingAction === "event-create" || !canWriteSelectedCollection}
            >
                {loadingAction === "event-create" ? t("Saving...") : t("Add Event")}
            </Button>
            {#if eventFormError}
                <p class="text-sm text-coral" role="alert">{eventFormError}</p>
            {/if}
        </div>
        <div class="mt-4 space-y-2">
            {#each pimItems.filter((item) => item.kind === "calendar_event" && item.spaceId === selectedCollectionId) as item}
                <div class="rounded-xl border border-slate/15 bg-white/70 p-3">
                    <p class="font-semibold text-slate">{item.title}</p>
                    <p class="text-xs text-slate/65">
                        {textField(item, "starts_at")}
                        {textField(item, "ends_at")
                            ? ` – ${textField(item, "ends_at")}`
                            : ""}
                    </p>
                    <div class="mt-2">
                        <Button
                            variant="danger"
                            on:click={() => deletePimItem(item)}
                            disabled={loadingAction === `delete-${item.resourceId}` || !canWriteSelectedCollection}
                        >{t("Delete")}</Button>
                    </div>
                </div>
            {:else}
                <p class="text-sm text-slate/70">{t("No events on this device yet.")}</p>
            {/each}
        </div>
    </Card>
    {/if}

    {#if view === "contacts"}
    <Card>
        <h2 class="font-heading text-xl font-semibold text-slate">{t("Contacts")}</h2>
        <div class="mt-3 space-y-2">
            <select
                class="w-full rounded-xl border border-slate/20 bg-white px-3 py-2 text-sm text-slate outline-none"
                bind:value={selectedCollectionId}
            >
                {#each $appState.collections as collection}
                    <option value={collection.id}>{collection.name}</option>
                {/each}
            </select>
            {#if !canWriteSelectedCollection}
                <p class="border-l-4 border-gold bg-sand/45 p-3 text-sm text-slate">
                    {$locale === "ru" ? "Выбранное пространство доступно только для чтения или ожидает ключ устройства." : "The selected space is read-only or awaits this device key."}
                </p>
            {/if}
            <Input bind:value={contactName} placeholder={t("Full name")} disabled={!canWriteSelectedCollection} />
            <Input bind:value={contactEmail} type="email" placeholder={t("Email")} disabled={!canWriteSelectedCollection} />
            <Input bind:value={contactPhone} type="tel" placeholder={t("Phone")} disabled={!canWriteSelectedCollection} />
            <Button
                on:click={createContact}
                disabled={loadingAction === "contact-create" || !canWriteSelectedCollection}
            >
                {loadingAction === "contact-create" ? t("Saving...") : t("Add Contact")}
            </Button>
            {#if contactFormError}
                <p class="text-sm text-coral" role="alert">{contactFormError}</p>
            {/if}
        </div>
        <div class="mt-4 space-y-2">
            {#each pimItems.filter((item) => item.kind === "contact" && item.spaceId === selectedCollectionId) as item}
                <div class="rounded-xl border border-slate/15 bg-white/70 p-3">
                    <p class="font-semibold text-slate">{item.title}</p>
                    {#if textField(item, "email")}
                        <p class="text-xs text-slate/65">{textField(item, "email")}</p>
                    {/if}
                    {#if textField(item, "phone")}
                        <p class="text-xs text-slate/65">{textField(item, "phone")}</p>
                    {/if}
                    <div class="mt-2">
                        <Button
                            variant="danger"
                            on:click={() => deletePimItem(item)}
                            disabled={loadingAction === `delete-${item.resourceId}` || !canWriteSelectedCollection}
                        >{t("Delete")}</Button>
                    </div>
                </div>
            {:else}
                <p class="text-sm text-slate/70">{t("No contacts on this device yet.")}</p>
            {/each}
        </div>
    </Card>
    {/if}

    {#if view === "sharing"}
    <Card>
        <h2 class="font-heading text-xl font-semibold text-slate">
            {t("Invite Codes")}
        </h2>
        <p class="mt-2 text-xs text-slate/70">
            {$locale === "ru" ? "Принять код могут только зарегистрированные пользователи Kamori." : "Only users already registered on Kamori can redeem invite codes."}
        </p>
        <p class="mt-1 text-xs text-slate/70">
            {$locale === "ru" ? "Ключ пространства шифруется и расшифровывается на клиенте; сервер хранит только непрозрачные байты." : "The space key is wrapped and unwrapped on the client; the server stores opaque bytes only."}
        </p>
        <div class="mt-3 rounded-xl border border-slate/15 bg-white/70 p-3">
            <p
                class="text-xs font-semibold uppercase tracking-wide text-slate/70"
            >
                {$locale === "ru" ? "Как работают коды" : "How invite codes work"}
            </p>
            <ol class="mt-2 space-y-1 text-xs text-slate/80">
                <li>1. {$locale === "ru" ? "Код создаётся локально и шифрует ключ пространства в браузере." : "The code is generated locally and encrypts the space key in your browser."}</li>
                <li>2. {$locale === "ru" ? "Сервер хранит хеш кода, зашифрованные данные, необязательную заметку и срок действия." : "The server stores the code hash, encrypted payload, optional note, and expiry."}</li>
                <li>3. {$locale === "ru" ? "Получатель вводит код и локально восстанавливает ключ пространства." : "The recipient enters the code and restores the space key locally."}</li>
                <li>4. {$locale === "ru" ? "Приглашение одноразовое и действует только до истечения срока." : "The invitation is single-use and expires."}</li>
            </ol>
        </div>

        <div class="mt-4 space-y-3">
            <p
                class="text-xs font-semibold uppercase tracking-wide text-slate/70"
            >
                {t("Generate")}
            </p>

            <select
                class="w-full rounded-xl border border-slate/20 bg-white px-3 py-2 text-sm text-slate outline-none"
                bind:value={selectedCollectionId}
            >
                {#if $appState.collections.length === 0}
                    <option value="">{t("No spaces")}</option>
                {:else}
                    {#each $appState.collections as collection}
                        <option value={collection.id}>{collection.name}</option>
                    {/each}
                {/if}
            </select>

            {#if !selectedCollection}
                <p class="border-l-4 border-gold bg-sand/45 p-3 text-sm text-slate">
                    {$locale === "ru" ? "Создайте пространство, чтобы приглашать участников. Принять чужой код можно ниже." : "Create a space before inviting members. You can still redeem someone else's code below."}
                </p>
            {:else if selectedCollection.role !== "owner"}
                <p class="border-l-4 border-gold bg-sand/45 p-3 text-sm text-slate">
                    {$locale === "ru" ? "Только владелец пространства может создавать приглашения." : "Only the space owner can create invitations."}
                </p>
            {/if}

            <select
                class="w-full rounded-xl border border-slate/20 bg-white px-3 py-2 text-sm text-slate outline-none"
                aria-label="Invite role"
                bind:value={inviteRole}
            >
                <option value="editor">{t("Editor")}</option>
                <option value="reader">{t("Reader")}</option>
            </select>

            <select
                class="w-full rounded-xl border border-slate/20 bg-white px-3 py-2 text-sm text-slate outline-none"
                aria-label="Invite expiry"
                bind:value={inviteTtlMinutes}
            >
                {#each inviteTtlOptions as option}
                    <option value={String(option.value)}>{option[$locale]}</option>
                {/each}
            </select>

            <Input
                bind:value={inviteNotePlaintext}
                placeholder={t("Optional encrypted note for recipient")}
            />

            <Button
                on:click={issueInviteCode}
                disabled={loadingAction === "invite-generate" || selectedCollection?.role !== "owner"}
            >
                {loadingAction === "invite-generate"
                    ? t("Generating...")
                    : t("Generate Invite Code")}
            </Button>

            {#if inviteCodeIssued}
                <p
                    class="rounded-xl border border-slate/15 bg-surface px-3 py-2 text-sm font-semibold text-slate"
                >
                    {inviteCodeIssued}
                </p>
            {/if}

            {#if selectedCollectionId && (spaceMembers[selectedCollectionId] ?? []).some((member) => member.role !== "owner")}
                <div class="border-t border-slate/15 pt-3">
                    <p class="text-xs font-semibold uppercase tracking-wide text-slate/70">
                        {localized("Current access", "Текущий доступ")}
                    </p>
                    <div class="mt-2 space-y-2">
                        {#each (spaceMembers[selectedCollectionId] ?? []).filter((member) => member.role !== "owner") as member}
                            <div class="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-sand/50 p-2">
                                <span class="text-xs text-slate">
                                    {member.username} · {t(member.role === "editor" ? "Editor" : "Reader")}
                                </span>
                                <Button
                                    variant="danger"
                                    on:click={() => {
                                        const collection = $appState.collections.find((item) => item.id === selectedCollectionId);
                                        if (collection) void revokeSpaceMember(collection, member);
                                    }}
                                    disabled={loadingAction === `member-revoke-${selectedCollectionId}-${member.user_id}`}
                                >
                                    {localized("Remove access", "Отозвать доступ")}
                                </Button>
                            </div>
                        {/each}
                    </div>
                </div>
            {/if}

            <p
                class="pt-3 text-xs font-semibold uppercase tracking-wide text-slate/70"
            >
                {t("Redeem")}
            </p>
            <Input
                bind:value={inviteCodeToRedeem}
                placeholder="ABCD-EFGH-JKLM-NPQR"
            />
            <Button
                variant="secondary"
                on:click={redeemInviteCode}
                disabled={loadingAction === "invite-redeem"}
            >
                {loadingAction === "invite-redeem"
                    ? t("Redeeming...")
                    : t("Redeem Code")}
            </Button>

            {#if inviteRedeemedNote}
                <div
                    class="rounded-xl border border-slate/15 bg-white/70 p-3 text-sm text-slate/85"
                >
                    <p
                        class="text-xs font-semibold uppercase tracking-wide text-slate/70"
                    >
                        {t("Invite Note")}
                    </p>
                    <p class="mt-1 whitespace-pre-wrap">{inviteRedeemedNote}</p>
                </div>
            {/if}
        </div>
    </Card>
    {/if}
</div>
{/if}

<Modal
    open={deleteModalOpen}
    title={t("Delete Space")}
    onClose={() => (deleteModalOpen = false)}
>
    <div class="space-y-3">
        <p class="text-sm text-slate">
            {$locale === "ru" ? "Переместить пространство в корзину? Зашифрованную историю можно восстановить в течение 30 дней." : "Move this space to trash? Its encrypted history can be restored for 30 days."}
        </p>
        <div class="flex gap-2">
            <Button variant="ghost" on:click={() => (deleteModalOpen = false)}
                >{t("Cancel")}</Button
            >
            <Button
                variant="danger"
                on:click={deleteCollection}
                disabled={loadingAction === "space-delete"}
            >{t("Move to Trash")}</Button>
        </div>
    </div>
</Modal>
