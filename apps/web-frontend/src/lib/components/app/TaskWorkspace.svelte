<script lang="ts">
    import Button from "$lib/components/ui/Button.svelte";
    import Modal from "$lib/components/ui/Modal.svelte";
    import type { MaterializedPimItem, PimValue } from "$lib/pim";
    import { localDateTimeToTemporal, temporalToDate, temporalToInputValue } from "$lib/pim";
    import { locale } from "$lib/i18n";

    export let spaces: { id: string; name: string }[] = [];
    export let selectedSpaceId = "";
    export let canWrite = false;
    export let items: MaterializedPimItem[] = [];
    export let busy = "";
    export let onSave: (
        item: MaterializedPimItem | undefined,
        fields: Record<string, PimValue>,
    ) => Promise<void>;
    export let onToggle: (item: MaterializedPimItem, completed: boolean) => Promise<void>;
    export let onDelete: (item: MaterializedPimItem) => Promise<void>;

    let editorOpen = false;
    let editing: MaterializedPimItem | undefined;
    let title = "";
    let notes = "";
    let dueAt = "";
    let priority = "0";
    let reminderMinutes = "";
    let categories = "";
    let showCompleted = false;
    let search = "";
    let formError = "";

    const tr = (english: string, russian: string) => $locale === "ru" ? russian : english;
    const textField = (item: MaterializedPimItem, field: string) => {
        const value = item.fields[field];
        return value?.type === "text" ? value.value : "";
    };
    const integerField = (item: MaterializedPimItem, field: string) => {
        const value = item.fields[field];
        return value?.type === "integer" ? value.value : 0;
    };
    const listField = (item: MaterializedPimItem, field: string) => {
        const value = item.fields[field];
        return value?.type === "text_list" ? value.value : [];
    };
    const dueDate = (item: MaterializedPimItem) => temporalToDate(item.fields.due_at);
    const formatDue = (item: MaterializedPimItem) => {
        const date = dueDate(item);
        return date ? new Intl.DateTimeFormat($locale, { dateStyle: "medium", timeStyle: "short" }).format(date) : "";
    };
    const isOverdue = (item: MaterializedPimItem) => {
        const due = dueDate(item);
        return !item.completed && due !== undefined && due.getTime() < Date.now();
    };
    const priorityLabel = (value: number) => {
        if (value === 0) return "";
        if (value <= 3) return tr("High priority", "Высокий приоритет");
        if (value <= 6) return tr("Medium priority", "Средний приоритет");
        return tr("Low priority", "Низкий приоритет");
    };
    const prioritySortKey = (item: MaterializedPimItem) => {
        const value = integerField(item, "priority");
        return value === 0 ? 10 : value;
    };

    const reset = () => {
        editing = undefined;
        title = "";
        notes = "";
        dueAt = "";
        priority = "0";
        reminderMinutes = "";
        categories = "";
        formError = "";
    };

    const openCreate = () => {
        reset();
        editorOpen = true;
    };

    const openEdit = (item: MaterializedPimItem) => {
        editing = item;
        title = item.title;
        notes = textField(item, "notes");
        dueAt = temporalToInputValue(item.fields.due_at);
        priority = String(integerField(item, "priority"));
        const reminder = item.fields.reminder_minutes;
        reminderMinutes = reminder?.type === "integer" ? String(reminder.value) : "";
        categories = listField(item, "categories").join(", ");
        formError = "";
        editorOpen = true;
    };

    const save = async () => {
        const normalizedTitle = title.trim();
        if (!normalizedTitle) {
            formError = tr("Enter a task title.", "Введите название задачи.");
            return;
        }
        try {
            const fields: Record<string, PimValue> = {
                title: { type: "text", value: normalizedTitle },
                notes: notes.trim() ? { type: "text", value: notes.trim() } : { type: "null" },
                due_at: dueAt
                    ? { type: "record", value: localDateTimeToTemporal(dueAt) }
                    : { type: "null" },
                priority: { type: "integer", value: Number(priority) },
                reminder_minutes: reminderMinutes
                    ? { type: "integer", value: Number(reminderMinutes) }
                    : { type: "null" },
                categories: {
                    type: "text_list",
                    value: categories.split(",").map((value) => value.trim()).filter(Boolean),
                },
            };
            if (!editing) fields.completed = { type: "boolean", value: false };
            await onSave(editing, fields);
            editorOpen = false;
            reset();
        } catch (error) {
            formError = error instanceof Error ? error.message : String(error);
        }
    };

    $: filtered = items
        .filter((item) => item.kind === "task" && item.spaceId === selectedSpaceId)
        .filter((item) => item.title.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase()))
        .sort((left, right) => {
            const leftDue = dueDate(left)?.getTime() ?? Number.MAX_SAFE_INTEGER;
            const rightDue = dueDate(right)?.getTime() ?? Number.MAX_SAFE_INTEGER;
            return leftDue - rightDue || prioritySortKey(left) - prioritySortKey(right) || left.title.localeCompare(right.title);
        });
    $: openItems = filtered.filter((item) => !item.completed);
    $: completedItems = filtered.filter((item) => item.completed);
    $: priorityOptions = [0, 1, 5, 9].includes(Number(priority))
        ? [0, 1, 5, 9]
        : [0, 1, 5, 9, Number(priority)].sort((left, right) => left - right);
    $: reminderOptions = ["", "0", "10", "60", "1440"].includes(reminderMinutes)
        ? ["", "0", "10", "60", "1440"]
        : ["", "0", "10", "60", "1440", reminderMinutes];
</script>

<section class="space-y-4">
    <header class="flex flex-wrap items-end justify-between gap-3">
        <div>
            <p class="text-xs font-semibold uppercase tracking-[0.18em] text-moss">{tr("Work queue", "Рабочая очередь")}</p>
            <h1 class="mt-1 font-heading text-2xl font-semibold text-slate">{tr("Tasks", "Задачи")}</h1>
        </div>
        <Button on:click={openCreate} disabled={!canWrite}>{tr("New task", "Новая задача")}</Button>
    </header>

    <div class="grid gap-2 md:grid-cols-[minmax(12rem,18rem)_1fr_auto]">
        <select class="border border-slate/20 bg-paper px-3 py-2 text-sm" bind:value={selectedSpaceId} aria-label={tr("Space", "Пространство")}>
            {#each spaces as space}<option value={space.id}>{space.name}</option>{/each}
        </select>
        <input class="border border-slate/20 bg-paper px-3 py-2 text-sm outline-none focus:ring" bind:value={search} placeholder={tr("Search tasks", "Поиск задач")} />
        <button class="border border-slate/20 bg-white px-3 py-2 text-sm font-semibold text-slate" on:click={() => (showCompleted = !showCompleted)}>
            {showCompleted ? tr("Hide completed", "Скрыть выполненные") : `${tr("Completed", "Выполненные")} (${completedItems.length})`}
        </button>
    </div>

    {#if !canWrite}
        <p class="border-l-4 border-gold bg-sand/45 p-3 text-sm text-slate">{tr("This space is read-only or its key is unavailable on this device.", "Пространство доступно только для чтения или его ключ недоступен на устройстве.")}</p>
    {/if}

    {#if openItems.length === 0}
        <div class="border border-dashed border-slate/25 bg-white/45 p-8 text-center">
            <p class="font-heading text-lg text-slate">{tr("Nothing pending", "Нет открытых задач")}</p>
            <p class="mt-1 text-sm text-slate/65">{tr("Create a task and optionally add a due time, reminder, or priority.", "Создайте задачу и при необходимости добавьте срок, напоминание или приоритет.")}</p>
        </div>
    {:else}
        <div class="space-y-2">
            {#each openItems as item}
                <article class="grid gap-3 border border-slate/15 bg-white/70 p-4 sm:grid-cols-[auto_1fr_auto]">
                    <input class="mt-1 size-5 accent-emerald-700" type="checkbox" checked={false} aria-label={tr("Complete task", "Выполнить задачу")} disabled={!canWrite || busy === `task-${item.resourceId}`} on:change={() => onToggle(item, true)} />
                    <div class="min-w-0">
                        <h2 class="font-semibold text-slate">{item.title}</h2>
                        {#if textField(item, "notes")}<p class="mt-1 whitespace-pre-wrap text-sm text-slate/70">{textField(item, "notes")}</p>{/if}
                        <div class="mt-2 flex flex-wrap gap-2 text-xs">
                            {#if formatDue(item)}<span class:is-overdue={isOverdue(item)} class="border border-slate/15 bg-paper px-2 py-1">{isOverdue(item) ? tr("Overdue", "Просрочено") : tr("Due", "Срок")}: {formatDue(item)}</span>{/if}
                            {#if priorityLabel(integerField(item, "priority"))}<span class="border border-slate/15 bg-paper px-2 py-1">{priorityLabel(integerField(item, "priority"))}</span>{/if}
                            {#each listField(item, "categories") as category}<span class="bg-mint/10 px-2 py-1 text-moss">{category}</span>{/each}
                            {#if item.conflict}<span class="bg-coral/10 px-2 py-1 text-coral">{tr("Concurrent edit", "Параллельное изменение")}</span>{/if}
                        </div>
                    </div>
                    <div class="flex gap-2 sm:flex-col">
                        <Button variant="ghost" on:click={() => openEdit(item)} disabled={!canWrite}>{tr("Edit", "Изменить")}</Button>
                        <Button variant="danger" on:click={() => onDelete(item)} disabled={!canWrite}>{tr("Delete", "Удалить")}</Button>
                    </div>
                </article>
            {/each}
        </div>
    {/if}

    {#if showCompleted}
        <section class="border-t border-slate/15 pt-4">
            <h2 class="font-heading text-lg font-semibold text-slate">{tr("Completed", "Выполненные")}</h2>
            <div class="mt-2 space-y-2">
                {#each completedItems as item}
                    <article class="flex items-center gap-3 border border-slate/10 bg-sand/35 p-3">
                        <input class="size-5 accent-emerald-700" type="checkbox" checked aria-label={tr("Reopen task", "Вернуть задачу")} disabled={!canWrite} on:change={() => onToggle(item, false)} />
                        <div class="min-w-0 flex-1"><p class="truncate text-slate/60 line-through">{item.title}</p></div>
                        <Button variant="ghost" on:click={() => openEdit(item)} disabled={!canWrite}>{tr("Edit", "Изменить")}</Button>
                        <Button variant="danger" on:click={() => onDelete(item)} disabled={!canWrite}>{tr("Delete", "Удалить")}</Button>
                    </article>
                {/each}
                {#if completedItems.length === 0}<p class="text-sm text-slate/60">{tr("No completed tasks.", "Выполненных задач нет.")}</p>{/if}
            </div>
        </section>
    {/if}
</section>

<Modal open={editorOpen} width="2xl" title={editing ? tr("Edit task", "Изменить задачу") : tr("New task", "Новая задача")} onClose={() => (editorOpen = false)}>
        <form class="space-y-4" on:submit|preventDefault={save}>
            <label class="block text-sm font-semibold text-slate">{tr("Title", "Название")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={title} maxlength="500" /></label>
            <label class="block text-sm font-semibold text-slate">{tr("Notes", "Заметки")}<textarea class="mt-1 min-h-24 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={notes}></textarea></label>
            <div class="grid gap-3 sm:grid-cols-2">
                <label class="text-sm font-semibold text-slate">{tr("Due date and time", "Срок") }<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" type="datetime-local" bind:value={dueAt} /></label>
                <label class="text-sm font-semibold text-slate">{tr("Priority", "Приоритет")}<select class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={priority}>{#each priorityOptions as option}<option value={String(option)}>{option === 0 ? tr("None", "Нет") : option === 1 ? tr("High", "Высокий") : option === 5 ? tr("Medium", "Средний") : option === 9 ? tr("Low", "Низкий") : `${tr("Imported priority", "Импортированный приоритет")}: ${option}`}</option>{/each}</select></label>
                <label class="text-sm font-semibold text-slate">{tr("Reminder for DAV apps", "Напоминание для DAV-приложений")}<select class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={reminderMinutes}>{#each reminderOptions as option}<option value={option}>{option === "" ? tr("None", "Нет") : option === "0" ? tr("At due time", "В срок") : option === "10" ? `10 ${tr("minutes before", "минут до")}` : option === "60" ? `1 ${tr("hour before", "час до")}` : option === "1440" ? `1 ${tr("day before", "день до")}` : `${option} ${tr("minutes before", "минут до")}`}</option>{/each}</select><span class="mt-1 block font-normal text-xs text-slate/60">{tr("Kamori stores this alarm for compatible DAV clients; the web app does not deliver background notifications yet.", "Kamori сохраняет alarm для совместимых DAV-клиентов; веб-приложение пока не отправляет фоновые уведомления.")}</span></label>
            </div>
            <label class="block text-sm font-semibold text-slate">{tr("Categories (comma-separated)", "Категории через запятую")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={categories} /></label>
            {#if formError}<p class="text-sm text-coral" role="alert">{formError}</p>{/if}
            <div class="flex justify-end gap-2"><Button variant="ghost" on:click={() => (editorOpen = false)}>{tr("Cancel", "Отмена")}</Button><Button type="submit" disabled={!canWrite || busy === "pim-save"}>{busy === "pim-save" ? tr("Saving…", "Сохранение…") : tr("Save task", "Сохранить задачу")}</Button></div>
        </form>
</Modal>

<style>
    .is-overdue { border-color: rgb(225 89 80 / 0.45); color: rgb(177 55 48); background: rgb(225 89 80 / 0.08); }
</style>
