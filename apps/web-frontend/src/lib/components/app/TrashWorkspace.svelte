<script lang="ts">
    import Button from "$lib/components/ui/Button.svelte";
    import { locale } from "$lib/i18n";
    import type { DeletedPimItem } from "$lib/pimHistory";

    type Space = {
        id: string;
        name: string;
        role: "owner" | "editor" | "reader";
        keyAvailable: boolean;
    };
    type KindFilter = "all" | "task" | "calendar_event" | "contact";

    export let spaces: Space[] = [];
    export let items: DeletedPimItem[] = [];
    export let busy = "";
    export let onRestore: (item: DeletedPimItem) => Promise<void>;

    let selectedSpaceId = "all";
    let kind: KindFilter = "all";
    let search = "";

    const tr = (english: string, russian: string) =>
        $locale === "ru" ? russian : english;
    const spaceFor = (spaceId: string) =>
        spaces.find((space) => space.id === spaceId);
    const kindLabel = (value: DeletedPimItem["kind"]) =>
        value === "task"
            ? tr("Task", "Задача")
            : value === "calendar_event"
              ? tr("Calendar event", "Событие календаря")
              : tr("Contact", "Контакт");
    const titleFor = (item: DeletedPimItem) =>
        item.title || tr("Deleted item", "Удалённый элемент");
    const canRestore = (item: DeletedPimItem) => {
        const space = spaceFor(item.spaceId);
        return Boolean(
            item.restorableProjection &&
            space?.keyAvailable &&
            space.role !== "reader",
        );
    };

    $: normalizedSearch = search.trim().toLocaleLowerCase();
    $: visible = items.filter((item) =>
        (selectedSpaceId === "all" || item.spaceId === selectedSpaceId) &&
        (kind === "all" || item.kind === kind) &&
        (!normalizedSearch ||
            titleFor(item).toLocaleLowerCase().includes(normalizedSearch) ||
            (spaceFor(item.spaceId)?.name ?? "")
                .toLocaleLowerCase()
                .includes(normalizedSearch)),
    );
</script>

<section class="space-y-4">
    <header>
        <p class="text-xs font-semibold uppercase tracking-[0.18em] text-moss">
            {tr("Encrypted history", "Зашифрованная история")}
        </p>
        <h1 class="mt-1 font-heading text-2xl font-semibold text-slate">
            {tr("Trash", "Корзина")}
        </h1>
        <p class="mt-2 max-w-3xl text-sm text-slate/70">
            {tr(
                "Deleted tasks, calendar events, and contacts remain encrypted in the operation history. Restoring an item creates a new signed version; it does not rewrite past changes.",
                "Удалённые задачи, события календаря и контакты остаются в зашифрованной истории операций. Восстановление создаёт новую подписанную версию и не переписывает прошлые изменения.",
            )}
        </p>
    </header>

    <div class="grid gap-2 md:grid-cols-[minmax(12rem,18rem)_minmax(12rem,16rem)_1fr]">
        <select
            class="border border-slate/20 bg-paper px-3 py-2 text-sm"
            bind:value={selectedSpaceId}
            aria-label={tr("Filter by space", "Фильтр по пространству")}
        >
            <option value="all">{tr("All spaces", "Все пространства")}</option>
            {#each spaces as space}
                <option value={space.id}>{space.name}</option>
            {/each}
        </select>
        <select
            class="border border-slate/20 bg-paper px-3 py-2 text-sm"
            bind:value={kind}
            aria-label={tr("Filter by item type", "Фильтр по типу элемента")}
        >
            <option value="all">{tr("All item types", "Все типы")}</option>
            <option value="task">{tr("Tasks", "Задачи")}</option>
            <option value="calendar_event">{tr("Calendar events", "События календаря")}</option>
            <option value="contact">{tr("Contacts", "Контакты")}</option>
        </select>
        <input
            class="border border-slate/20 bg-paper px-3 py-2 text-sm outline-none focus:ring"
            bind:value={search}
            placeholder={tr("Search Trash", "Поиск в корзине")}
        />
    </div>

    {#if visible.length === 0}
        <div class="border border-dashed border-slate/25 bg-white/45 p-8 text-center">
            <p class="font-heading text-lg text-slate">
                {tr("Trash is empty", "Корзина пуста")}
            </p>
            <p class="mt-1 text-sm text-slate/65">
                {tr(
                    "Deleted items that are available to this device will appear here.",
                    "Здесь появятся удалённые элементы, история которых доступна этому устройству.",
                )}
            </p>
        </div>
    {:else}
        <div class="space-y-2">
            {#each visible as item (item.tombstoneOperationId)}
                <article class="flex flex-wrap items-center gap-3 border border-slate/15 bg-white/70 p-4">
                    <div class="min-w-0 flex-1">
                        <h2 class="truncate font-semibold text-slate">
                            {titleFor(item)}
                        </h2>
                        <p class="mt-1 text-xs text-slate/60">
                            {kindLabel(item.kind)} · {spaceFor(item.spaceId)?.name ?? tr("Unknown space", "Неизвестное пространство")}
                        </p>
                        {#if !item.restorableProjection}
                            <p class="mt-2 text-xs text-coral">
                                {tr(
                                    "The previous encrypted version is not available on this device, so this item cannot be restored here.",
                                    "Предыдущая зашифрованная версия недоступна этому устройству, поэтому восстановить элемент здесь нельзя.",
                                )}
                            </p>
                        {:else if spaceFor(item.spaceId)?.role === "reader"}
                            <p class="mt-2 text-xs text-slate/60">
                                {tr(
                                    "Read-only access does not allow restoration.",
                                    "Доступ только для чтения не позволяет восстановить элемент.",
                                )}
                            </p>
                        {/if}
                    </div>
                    <Button
                        variant="secondary"
                        disabled={!canRestore(item) || busy === `restore-${item.tombstoneOperationId}`}
                        on:click={() => onRestore(item)}
                    >
                        {busy === `restore-${item.tombstoneOperationId}`
                            ? tr("Restoring…", "Восстановление…")
                            : tr("Restore", "Восстановить")}
                    </Button>
                </article>
            {/each}
        </div>
    {/if}

    <p class="border-l-4 border-gold bg-sand/45 p-3 text-xs text-slate/70">
        {tr(
            "Permanent history erasure is not available yet. It requires a coordinated encrypted generation reset so offline devices cannot reintroduce erased history.",
            "Безвозвратное стирание истории пока недоступно: для него требуется согласованная смена поколения зашифрованных данных, чтобы офлайн-устройства не вернули стёртую историю.",
        )}
    </p>
</section>
