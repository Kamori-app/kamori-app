<script lang="ts">
    import { onMount } from "svelte";
    import { cloudApi } from "$lib/api/cloud";
    import { decode, encode } from "@msgpack/msgpack";
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
    import { appState, type CollectionEntry } from "$lib/stores/app";
    import {
        getActiveWebDevice,
        listQueuedOperationEnvelopes,
        loadMaterializedPimState,
        loadSpaceKey,
        queueOperationEnvelope,
        removeQueuedOperationEnvelope,
        storeSpaceKey,
        storeMaterializedPimState,
        withActiveMasterKey,
    } from "$lib/cryptoVault";
    import {
        decryptVaultBytes,
        encryptVaultBytes,
        openOperationEnvelope,
        sealOperationEnvelope,
        unwrapSpaceKeyForDevice,
        verifyOperationEnvelope,
        wrapSpaceKeyForDevice,
    } from "$lib/opaqueClient";
    import {
        decodePimOperation,
        decodePimSnapshot,
        encodePimOperation,
        type MaterializedPimItem,
        type MaterializedOperationState,
        type PimOperationV1,
        type PimResourceKind,
        type PimValue,
        type PimSnapshotV1,
    } from "$lib/pim";
    import Button from "$lib/components/ui/Button.svelte";
    import Card from "$lib/components/ui/Card.svelte";
    import Input from "$lib/components/ui/Input.svelte";
    import Modal from "$lib/components/ui/Modal.svelte";
    import { locale } from "$lib/i18n";

    const ruCopy: Record<string, string> = {
        "Confirm this code in your desktop app": "Сверьте этот код с кодом в приложении для компьютера",
        "Approve Desktop": "Подтвердить компьютер",
        "Dashboard": "Обзор", "Current user": "Текущий пользователь", "Not signed in": "Вход не выполнен",
        "Collections": "Пространства", "Synced events total": "Всего синхронизировано", "Last synced seq": "Последняя операция",
        "Syncing...": "Синхронизация…", "Sync Now": "Синхронизировать", "Collection name": "Название пространства",
        "Create Collection": "Создать пространство", "No collections yet.": "Пространств пока нет.", "Approval required on another device": "Нужно подтверждение на другом устройстве",
        "Delete": "Удалить", "Trash · retained for 30 days": "Корзина · хранение 30 дней", "Restore": "Восстановить",
        "Tasks": "Задачи", "New task": "Новая задача", "Saving...": "Сохранение…", "Add Task": "Добавить задачу",
        "Completed": "Выполнено", "Open": "Открыто", "Reopen": "Вернуть", "Complete": "Выполнить", "No tasks on this device yet.": "На этом устройстве задач пока нет.",
        "Calendar": "Календарь", "Event title": "Название события", "Add Event": "Добавить событие", "No events on this device yet.": "На этом устройстве событий пока нет.",
        "Contacts": "Контакты", "Full name": "Полное имя", "Email": "Email", "Phone": "Телефон", "Add Contact": "Добавить контакт", "No contacts on this device yet.": "На этом устройстве контактов пока нет.",
        "Invite Codes": "Коды приглашений", "Generate": "Создать", "No collections": "Нет пространств", "Editor": "Редактор", "Reader": "Чтение",
        "Optional encrypted note for recipient": "Необязательная зашифрованная заметка получателю", "Generating...": "Создание…", "Generate Invite Code": "Создать код приглашения",
        "Redeem": "Принять", "Redeeming...": "Принимаем…", "Redeem Code": "Принять код", "Invite Note": "Заметка приглашения",
        "Delete Collection": "Удалить пространство", "Cancel": "Отмена", "Move to Trash": "Переместить в корзину",
    };
    const t = (english: string) => $locale === "ru" ? (ruCopy[english] ?? english) : english;

    /**
     * Authenticated workspace:
     * dashboard counters, local collection state,
     * and invite-code issue/redeem flows.
     */
    let collectionName = "";
    let selectedCollectionId = "";
    let taskTitle = "";
    let eventTitle = "";
    let eventStart = "";
    let eventEnd = "";
    let contactName = "";
    let contactEmail = "";
    let contactPhone = "";
    let pimItems: MaterializedPimItem[] = [];
    let operationStates: MaterializedOperationState[] = [];
    let trashedCollections: CollectionEntry[] = [];

    let inviteTtlMinutes = "60";
    let inviteRole: "editor" | "reader" = "editor";
    let inviteCodeIssued = "";
    let inviteCodeToRedeem = "";
    let inviteNotePlaintext = "";
    let inviteRedeemedNote = "";

    let loadingAction = "";
    let deviceAuthorizationCode = "";

    let deleteModalOpen = false;
    let pendingDeleteCollectionId = "";

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

    const setNotice = (notice: string) => {
        appState.update((state) => ({ ...state, notice }));
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
            await withAccessRetry((accessToken) =>
                cloudApi.approveDeviceAuthorization(
                    $appState.cloudBaseUrl,
                    deviceAuthorizationCode,
                    accessToken,
                ),
            );
            setNotice("Desktop sign-in approved. You may return to Kamori Desktop.");
            clearDeviceAuthorizationQuery();
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Desktop authorization failed: ${message}`);
        } finally {
            clearLoading();
        }
    };

    const withAccessToken = (): string => {
        const token = tokenStore.getAccessToken();
        if (!token) {
            throw new Error("Sign in first.");
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
                    preauthToken: null,
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
            setNotice("Collection name is required.");
            return;
        }

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
                (masterKey) => encryptVaultBytes(masterKey, spaceKey),
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
            const entry: CollectionEntry = {
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
                notice: `Collection "${name}" created.`,
            }));

            selectedCollectionId = entry.id;
            collectionName = "";
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Space creation failed: ${message}`);
        } finally {
            clearLoading();
        }
    };

    const applyPimOperation = (
        spaceId: string,
        clientOpId: string,
        operation: PimOperationV1,
    ) => {
        if (operationStates.some((state) => state.clientOpId === clientOpId)) return;
        const parent = operation.dependencies
            .map((dependency) =>
                operationStates.find(
                    (state) =>
                        state.clientOpId === dependency &&
                        state.spaceId === spaceId &&
                        state.logicalResourceId === operation.resource_id &&
                        state.kind === operation.resource_kind,
                ),
            )
            .find((state) => state !== undefined);
        const canonical = pimItems.find(
            (item) =>
                item.spaceId === spaceId &&
                item.resourceId === operation.resource_id &&
                (item.projectionId ?? item.resourceId) === operation.resource_id,
        );
        const isSequential = Boolean(
            canonical && operation.dependencies.includes(canonical.headOperationId),
        );
        const projectionId =
            parent?.projectionId ??
            (isSequential || !canonical
                ? operation.resource_id
                : `${operation.resource_id}~conflict-${clientOpId.slice(0, 8)}`);
        const index = pimItems.findIndex(
            (item) =>
                item.spaceId === spaceId &&
                (item.projectionId ?? item.resourceId) === projectionId,
        );
        if (operation.operation === "delete") {
            if (index >= 0) {
                pimItems = pimItems.filter((_, itemIndex) => itemIndex !== index);
            }
            operationStates = [
                ...operationStates,
                {
                    spaceId,
                    clientOpId,
                    logicalResourceId: operation.resource_id,
                    projectionId,
                    kind: operation.resource_kind,
                    title: parent?.title ?? "",
                    completed: parent?.completed ?? false,
                    fields: parent?.fields ?? {},
                    deleted: true,
                },
            ];
            return;
        }
        const existing = index >= 0 ? pimItems[index] : undefined;
        const titleValue = operation.fields.title;
        const completedValue = operation.fields.completed;
        const fields = { ...(parent?.fields ?? {}), ...operation.fields };
        const next: MaterializedPimItem = {
            spaceId,
            resourceId: operation.resource_id,
            projectionId,
            kind: operation.resource_kind,
            title:
                titleValue?.type === "text"
                    ? titleValue.value
                    : (parent?.title ?? "Untitled"),
            completed:
                completedValue?.type === "boolean"
                    ? completedValue.value
                    : (parent?.completed ?? false),
            fields,
            headOperationId: clientOpId,
            conflict: projectionId !== operation.resource_id,
        };
        if (index >= 0) {
            pimItems = pimItems.map((item, itemIndex) =>
                itemIndex === index ? next : item,
            );
        } else {
            pimItems = [...pimItems, next];
        }
        operationStates = [
            ...operationStates,
            {
                spaceId,
                clientOpId,
                logicalResourceId: operation.resource_id,
                projectionId,
                kind: operation.resource_kind,
                title: next.title,
                completed: next.completed,
                fields,
                deleted: false,
            },
        ];
    };

    const applyPimSnapshot = (spaceId: string, snapshot: PimSnapshotV1) => {
        pimItems = pimItems.filter(
            (item) =>
                !(
                    item.spaceId === spaceId &&
                    item.resourceId === snapshot.resource_id
                ),
        );
        operationStates = operationStates.filter(
            (state) =>
                !(
                    state.spaceId === spaceId &&
                    state.logicalResourceId === snapshot.resource_id
                ),
        );
        if (snapshot.deleted) return;

        const projection = new TextDecoder().decode(
            snapshot.materialized_projection,
        );
        const lineValue = (names: string[]): string => {
            for (const line of projection.split(/\r?\n/)) {
                const separator = line.indexOf(":");
                if (separator < 0) continue;
                const property = line
                    .slice(0, separator)
                    .split(";", 1)[0]
                    .toUpperCase();
                if (names.includes(property)) return line.slice(separator + 1);
            }
            return "";
        };
        const title =
            lineValue(
                snapshot.resource_kind === "contact"
                    ? ["FN", "N"]
                    : ["SUMMARY"],
            ) || "Untitled";
        const completed = lineValue(["STATUS"]).toUpperCase() === "COMPLETED";
        const fields: Record<string, PimValue> = {
            title: { type: "text", value: title },
        };
        if (snapshot.resource_kind === "task") {
            fields.completed = { type: "boolean", value: completed };
        }
        pimItems = [
            ...pimItems,
            {
                spaceId,
                resourceId: snapshot.resource_id,
                projectionId: snapshot.projection_resource_id,
                kind: snapshot.resource_kind,
                title,
                completed,
                fields,
                headOperationId: snapshot.head_operation_id,
                conflict: false,
            },
        ];
        operationStates = [
            ...operationStates,
            {
                spaceId,
                clientOpId: snapshot.head_operation_id,
                logicalResourceId: snapshot.resource_id,
                projectionId: snapshot.projection_resource_id,
                kind: snapshot.resource_kind,
                title,
                completed,
                fields,
                deleted: false,
            },
        ];
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
                await withAccessRetry((accessToken) =>
                    cloudApi.appendOperation(
                        $appState.cloudBaseUrl,
                        envelope,
                        accessToken,
                    ),
                );
                await removeQueuedOperationEnvelope(envelope.client_op_id);
                flushed += 1;
            } catch {
                break;
            }
        }
        return flushed;
    };

    const commitPimOperation = async (
        operation: PimOperationV1,
    ): Promise<boolean> => {
        const collection = $appState.collections.find(
            (item) => item.id === selectedCollectionId,
        );
        if (!collection) {
            throw new Error("Choose a collection first.");
        }
        if (collection.role === "reader") {
            throw new Error("Reader access does not allow changes.");
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
        applyPimOperation(collection.id, clientOpId, operation);
        await storeMaterializedPimState(pimItems, operationStates);
        return (await flushOutbox()) > 0;
    };

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
            setNotice("Enter a task title.");
            return;
        }
        setLoading("task-create");
        try {
            const flushed = await commitPimOperation(
                makeUpsert("task", crypto.randomUUID(), {
                    title: { type: "text", value: title },
                    completed: { type: "boolean", value: false },
                }),
            );
            taskTitle = "";
            setNotice(
                flushed
                    ? "Task encrypted and synced."
                    : "Task saved to the encrypted offline outbox.",
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Task creation failed: ${message}`);
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
            setNotice(completed ? "Task completed." : "Task reopened.");
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Task update failed: ${message}`);
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
            setNotice("Item moved to encrypted tombstone history.");
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Delete failed: ${message}`);
        } finally {
            clearLoading();
        }
    };

    const createCalendarEvent = async () => {
        const title = eventTitle.trim();
        if (!title || !eventStart) {
            setNotice("Event title and start time are required.");
            return;
        }
        if (eventEnd && eventEnd < eventStart) {
            setNotice("Event end must not be earlier than its start.");
            return;
        }
        setLoading("event-create");
        try {
            const fields: Record<string, PimValue> = {
                title: { type: "text", value: title },
                starts_at: { type: "text", value: eventStart },
            };
            if (eventEnd) {
                fields.ends_at = { type: "text", value: eventEnd };
            }
            const flushed = await commitPimOperation(
                makeUpsert("calendar_event", crypto.randomUUID(), fields),
            );
            eventTitle = "";
            eventStart = "";
            eventEnd = "";
            setNotice(
                flushed
                    ? "Event encrypted and synced."
                    : "Event saved to the encrypted offline outbox.",
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Event creation failed: ${message}`);
        } finally {
            clearLoading();
        }
    };

    const createContact = async () => {
        const name = contactName.trim();
        const email = contactEmail.trim();
        const phone = contactPhone.trim();
        if (!name || (!email && !phone)) {
            setNotice("Contact name and an email or phone number are required.");
            return;
        }
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
                    ? "Contact encrypted and synced."
                    : "Contact saved to the encrypted offline outbox.",
            );
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Contact creation failed: ${message}`);
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
                notice: "Collection moved to trash for 30 days.",
            }));
            if (removed) {
                trashedCollections = [removed, ...trashedCollections];
            }
            pimItems = pimItems.filter((item) => item.spaceId !== collectionId);
            await storeMaterializedPimState(pimItems, operationStates);
            if (selectedCollectionId === collectionId) {
                selectedCollectionId = "";
            }
            pendingDeleteCollectionId = "";
            deleteModalOpen = false;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Collection deletion failed: ${message}`);
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
            setNotice("Collection restored. Syncing its encrypted history...");
            await syncNow();
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            setNotice(`Collection restore failed: ${message}`);
        } finally {
            clearLoading();
        }
    };

    /** Pulls accessible security spaces and unlocks packages for this device. */
    const syncNow = async () => {
        setLoading("sync-now");
        const previousPimItems = pimItems;
        const previousOperationStates = operationStates;
        try {
            const response = await withAccessRetry((accessToken) =>
                cloudApi.listSpaces($appState.cloudBaseUrl, accessToken),
            );
            const trashResponse = await withAccessRetry((accessToken) =>
                cloudApi.listTrashedSpaces(
                    $appState.cloudBaseUrl,
                    accessToken,
                ),
            );
            const device = getActiveWebDevice();
            const collections: CollectionEntry[] = [];
            let syncedOperationCount = 0;
            pimItems = [];
            operationStates = [];
            for (const space of response.spaces) {
                let key = await loadSpaceKey(space.space_id, space.key_epoch);
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
                if (key) {
                    const recoveryPackage = await withActiveMasterKey(
                        (masterKey) => encryptVaultBytes(masterKey, key),
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
                    const directory = await withAccessRetry((accessToken) =>
                        cloudApi.listSpaceDevices(
                            $appState.cloudBaseUrl,
                            space.space_id,
                            accessToken,
                        ),
                    );
                    const signingKeys = new Map(
                        directory.devices.map((item) => [
                            item.device_id,
                            item.signing_public_key,
                        ]),
                    );
                    let cursor = 0;
                    let spaceOperationCount = 0;
                    while (true) {
                        const page = await withAccessRetry((accessToken) =>
                            cloudApi.listOperations(
                                $appState.cloudBaseUrl,
                                space.space_id,
                                cursor,
                                accessToken,
                            ),
                        );
                        for (const stored of page.operations) {
                            const signingKey = signingKeys.get(
                                stored.envelope.author_device_id,
                            );
                            if (!signingKey) {
                                throw new Error(
                                    `Missing author key for operation ${stored.envelope.client_op_id}`,
                                );
                            }
                            await verifyOperationEnvelope(
                                stored.envelope,
                                signingKey,
                            );
                            const operationKey = await loadSpaceKey(
                                space.space_id,
                                stored.envelope.key_epoch,
                            );
                            if (!operationKey) {
                                continue;
                            }
                            const plaintext = await openOperationEnvelope(
                                stored.envelope,
                                operationKey,
                            );
                            if (stored.envelope.envelope_kind === "operation") {
                                applyPimOperation(
                                    space.space_id,
                                    stored.envelope.client_op_id,
                                    decodePimOperation(plaintext),
                                );
                            } else if (
                                stored.envelope.envelope_kind === "snapshot"
                            ) {
                                applyPimSnapshot(
                                    space.space_id,
                                    decodePimSnapshot(plaintext),
                                );
                            } else {
                                throw new Error(
                                    "Unsupported mandatory key-control envelope.",
                                );
                            }
                        }
                        spaceOperationCount += page.operations.length;
                        if (
                            page.operations.length === 0 ||
                            page.next_cursor <= cursor
                        ) {
                            break;
                        }
                        cursor = page.next_cursor;
                    }
                    syncedOperationCount += spaceOperationCount;
                    collections[collections.length - 1].syncedItems =
                        spaceOperationCount;
                }
            }

            const queued = await listQueuedOperationEnvelopes();
            for (const envelope of queued) {
                const key = await loadSpaceKey(envelope.space_id, envelope.key_epoch);
                if (!key) continue;
                const plaintext = await openOperationEnvelope(envelope, key);
                if (envelope.envelope_kind === "operation") {
                    applyPimOperation(
                        envelope.space_id,
                        envelope.client_op_id,
                        decodePimOperation(plaintext),
                    );
                } else if (envelope.envelope_kind === "snapshot") {
                    applyPimSnapshot(
                        envelope.space_id,
                        decodePimSnapshot(plaintext),
                    );
                }
            }
            const flushed = await flushOutbox();
            await storeMaterializedPimState(pimItems, operationStates);

            const trash: CollectionEntry[] = [];
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

            appState.update((state) => ({
                ...state,
                collections,
                syncedItemsTotal: syncedOperationCount,
                notice: `Sync completed: ${syncedOperationCount} operations, ${flushed} outbox items uploaded.`,
            }));
        } catch (error) {
            pimItems = previousPimItems;
            operationStates = previousOperationStates;
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Sync failed: ${message}`);
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
            setNotice("Create at least one collection first.");
            return;
        }

        const collection = $appState.collections.find(
            (item) => item.id === collectionId,
        );
        if (!collection) {
            setNotice("Selected collection not found.");
            return;
        }

        setLoading("invite-generate");
        try {
            inviteRedeemedNote = "";
            const inviteCode = generateInviteCode();
            const collectionKey = await loadSpaceKey(collection.id, collection.keyEpoch);
            if (!collectionKey) {
                throw new Error("This device has no key package for the selected space.");
            }
            const encryptedGroupKey = await wrapCollectionKeyWithInviteCode(collectionKey, inviteCode);
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
            setNotice("Invite code generated.");
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Invite generation failed: ${message}`);
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
            setNotice("Invite code is required.");
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
                (masterKey) => encryptVaultBytes(masterKey, collectionKey),
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
                        notice: `Invite redeemed for space ${redeemed.space_id.slice(0, 8)}.`,
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
                    notice: `Invite redeemed for space ${redeemed.space_id.slice(0, 8)}.`,
                };
            });

            inviteCodeToRedeem = "";
            inviteRedeemedNote = decryptedInviteNote;
        } catch (error) {
            const message =
                error instanceof Error ? error.message : String(error);
            setNotice(`Invite redemption failed: ${message}`);
        } finally {
            clearLoading();
        }
    };

    $: if (!selectedCollectionId && $appState.collections.length > 0) {
        selectedCollectionId = $appState.collections[0].id;
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
            } catch (error) {
                const message =
                    error instanceof Error ? error.message : String(error);
                setNotice(`Encrypted offline state could not be opened: ${message}`);
            }
        })();
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

<div class="grid gap-4 md:grid-cols-2">
    <Card>
        <h2 class="font-heading text-xl font-semibold text-slate">{t("Dashboard")}</h2>
        <div class="mt-3 space-y-2 text-sm text-slate/80">
            <p>{t("Current user")}: {$appState.currentUsername || t("Not signed in")}</p>
            <p>{t("Collections")}: {$appState.collections.length}</p>
            <p>{t("Synced events total")}: {$appState.syncedItemsTotal}</p>
            <p>{t("Last synced seq")}: {$appState.lastSyncedSeq}</p>
        </div>
        <div class="mt-3">
            <Button on:click={syncNow} disabled={loadingAction === "sync-now"}>
                {loadingAction === "sync-now" ? t("Syncing...") : t("Sync Now")}
            </Button>
        </div>
    </Card>

    <Card>
        <h2 class="font-heading text-xl font-semibold text-slate">
            {t("Collections")}
        </h2>
        <div class="mt-3 space-y-2">
            <Input bind:value={collectionName} placeholder={t("Collection name")} />
            <Button on:click={createCollection}>{t("Create Collection")}</Button>
        </div>

        <div class="mt-4 space-y-2">
            {#if $appState.collections.length === 0}
                <p class="text-sm text-slate/70">{t("No collections yet.")}</p>
            {:else}
                {#each $appState.collections as collection}
                    <div
                        class="rounded-xl border border-slate/15 bg-white/70 p-3"
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
                        <div class="mt-2">
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
            <Input bind:value={taskTitle} placeholder={t("New task")} />
            <Button
                on:click={createTask}
                disabled={loadingAction === "task-create"}
            >
                {loadingAction === "task-create" ? t("Saving...") : t("Add Task")}
            </Button>
        </div>
        <div class="mt-4 space-y-2">
            {#each pimItems.filter((item) => item.kind === "task") as item}
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
                            disabled={loadingAction === `task-${item.resourceId}`}
                        >
                            {item.completed ? t("Reopen") : t("Complete")}
                        </Button>
                        <Button
                            variant="danger"
                            on:click={() => deletePimItem(item)}
                            disabled={loadingAction === `delete-${item.resourceId}`}
                        >{t("Delete")}</Button>
                    </div>
                </div>
            {:else}
                <p class="text-sm text-slate/70">{t("No tasks on this device yet.")}</p>
            {/each}
        </div>
    </Card>

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
            <Input bind:value={eventTitle} placeholder={t("Event title")} />
            <Input bind:value={eventStart} type="datetime-local" />
            <Input bind:value={eventEnd} type="datetime-local" />
            <Button
                on:click={createCalendarEvent}
                disabled={loadingAction === "event-create"}
            >
                {loadingAction === "event-create" ? t("Saving...") : t("Add Event")}
            </Button>
        </div>
        <div class="mt-4 space-y-2">
            {#each pimItems.filter((item) => item.kind === "calendar_event") as item}
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
                            disabled={loadingAction === `delete-${item.resourceId}`}
                        >{t("Delete")}</Button>
                    </div>
                </div>
            {:else}
                <p class="text-sm text-slate/70">{t("No events on this device yet.")}</p>
            {/each}
        </div>
    </Card>

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
            <Input bind:value={contactName} placeholder={t("Full name")} />
            <Input bind:value={contactEmail} type="email" placeholder={t("Email")} />
            <Input bind:value={contactPhone} type="tel" placeholder={t("Phone")} />
            <Button
                on:click={createContact}
                disabled={loadingAction === "contact-create"}
            >
                {loadingAction === "contact-create" ? t("Saving...") : t("Add Contact")}
            </Button>
        </div>
        <div class="mt-4 space-y-2">
            {#each pimItems.filter((item) => item.kind === "contact") as item}
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
                            disabled={loadingAction === `delete-${item.resourceId}`}
                        >{t("Delete")}</Button>
                    </div>
                </div>
            {:else}
                <p class="text-sm text-slate/70">{t("No contacts on this device yet.")}</p>
            {/each}
        </div>
    </Card>

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
                    <option value="">{t("No collections")}</option>
                {:else}
                    {#each $appState.collections as collection}
                        <option value={collection.id}>{collection.name}</option>
                    {/each}
                {/if}
            </select>

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
                disabled={loadingAction === "invite-generate"}
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
</div>

<Modal
    open={deleteModalOpen}
    title={t("Delete Collection")}
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
