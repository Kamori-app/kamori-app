<script lang="ts">
    import Button from "$lib/components/ui/Button.svelte";
    import Modal from "$lib/components/ui/Modal.svelte";
    import type { MaterializedPimItem, PimValue } from "$lib/pim";
    import { localDateTimeToTemporal, recurringIntervalOccursOnDay, temporalToDate, temporalToInputValue } from "$lib/pim";
    import { locale } from "$lib/i18n";

    export let spaces: { id: string; name: string }[] = [];
    export let selectedSpaceId = "";
    export let canWrite = false;
    export let items: MaterializedPimItem[] = [];
    export let busy = "";
    export let onSave: (item: MaterializedPimItem | undefined, fields: Record<string, PimValue>) => Promise<void>;
    export let onDelete: (item: MaterializedPimItem) => Promise<void>;

    type CalendarMode = "month" | "week" | "list";
    let mode: CalendarMode = "month";
    let anchor = new Date(new Date().getFullYear(), new Date().getMonth(), 1);
    let editorOpen = false;
    let editing: MaterializedPimItem | undefined;
    let title = "";
    let allDay = false;
    let start = "";
    let end = "";
    let startDate = "";
    let endDate = "";
    let timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
    let endWasMissing = false;
    let endTouched = false;
    let location = "";
    let notes = "";
    let recurrence = "";
    let reminderMinutes = "";
    let categories = "";
    let formError = "";

    const tr = (english: string, russian: string) => $locale === "ru" ? russian : english;
    const textField = (item: MaterializedPimItem, field: string) => {
        const value = item.fields[field];
        return value?.type === "text" ? value.value : "";
    };
    const listField = (item: MaterializedPimItem, field: string) => {
        const value = item.fields[field];
        return value?.type === "text_list" ? value.value : [];
    };
    const isAllDay = (item: MaterializedPimItem) =>
        item.fields.starts_at?.type === "record" && item.fields.starts_at.value.kind === "date";
    const addDays = (value: Date, days: number) => new Date(value.getFullYear(), value.getMonth(), value.getDate() + days);
    const isoDate = (value: Date) => `${value.getFullYear().toString().padStart(4, "0")}-${(value.getMonth() + 1).toString().padStart(2, "0")}-${value.getDate().toString().padStart(2, "0")}`;
    const parseIsoDate = (value: string) => {
        const [year, month, day] = value.split("-").map(Number);
        return new Date(year, month - 1, day);
    };
    const eventStart = (item: MaterializedPimItem) => temporalToDate(item.fields.starts_at);
    const explicitEventEnd = (item: MaterializedPimItem) => temporalToDate(item.fields.ends_at);
    const eventEnd = (item: MaterializedPimItem) => {
        const explicit = explicitEventEnd(item);
        if (explicit) return explicit;
        const startsAt = eventStart(item);
        if (!startsAt) return undefined;
        return isAllDay(item) ? addDays(startsAt, 1) : new Date(startsAt.getTime() + 1);
    };
    const eventTime = (item: MaterializedPimItem) => {
        if (isAllDay(item)) return tr("All day", "Весь день");
        const startsAt = eventStart(item);
        const endsAt = explicitEventEnd(item);
        if (!startsAt) return "";
        const date = new Intl.DateTimeFormat($locale, { dateStyle: "medium" }).format(startsAt);
        const startTime = new Intl.DateTimeFormat($locale, { timeStyle: "short" }).format(startsAt);
        const endTime = endsAt ? new Intl.DateTimeFormat($locale, { timeStyle: "short" }).format(endsAt) : "";
        return `${date} · ${startTime}${endTime ? `–${endTime}` : ""}`;
    };
    const occursOn = (item: MaterializedPimItem, day: Date) => {
        return recurringIntervalOccursOnDay(
            item.fields.starts_at,
            item.fields.ends_at,
            textField(item, "recurrence_rule"),
            day,
        );
    };
    const calendarDays = (month: Date) => {
        const first = new Date(month.getFullYear(), month.getMonth(), 1);
        const mondayOffset = (first.getDay() + 6) % 7;
        const gridStart = addDays(first, -mondayOffset);
        return Array.from({ length: 42 }, (_, index) => addDays(gridStart, index));
    };
    const weekDays = (date: Date) => {
        const monday = addDays(date, -((date.getDay() + 6) % 7));
        return Array.from({ length: 7 }, (_, index) => addDays(monday, index));
    };
    const sameDay = (left: Date, right: Date) => isoDate(left) === isoDate(right);
    const go = (offset: number) => {
        anchor = mode === "month"
            ? new Date(anchor.getFullYear(), anchor.getMonth() + offset, 1)
            : addDays(anchor, offset * 7);
    };
    const reset = () => {
        editing = undefined;
        title = "";
        allDay = false;
        start = "";
        end = "";
        startDate = isoDate(new Date());
        endDate = startDate;
        location = "";
        notes = "";
        recurrence = "";
        reminderMinutes = "";
        categories = "";
        timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
        endWasMissing = false;
        endTouched = false;
        formError = "";
    };
    const openCreate = (day?: Date) => {
        reset();
        if (day) {
            allDay = true;
            startDate = isoDate(day);
            endDate = startDate;
        }
        editorOpen = true;
    };
    const openEdit = (item: MaterializedPimItem) => {
        editing = item;
        title = item.title;
        allDay = isAllDay(item);
        const storedStart = item.fields.starts_at;
        if (storedStart?.type === "record" && storedStart.value.kind === "zoned_datetime") {
            timezone = storedStart.value.timezone || timezone;
        }
        endWasMissing = !item.fields.ends_at || item.fields.ends_at.type === "null";
        endTouched = false;
        if (allDay) {
            const startsAt = eventStart(item);
            const endsAt = explicitEventEnd(item);
            startDate = startsAt ? isoDate(startsAt) : "";
            endDate = endsAt ? isoDate(addDays(endsAt, -1)) : startDate;
        } else {
            start = temporalToInputValue(item.fields.starts_at);
            end = temporalToInputValue(item.fields.ends_at);
        }
        location = textField(item, "location");
        notes = textField(item, "notes");
        recurrence = textField(item, "recurrence_rule");
        const reminder = item.fields.reminder_minutes;
        reminderMinutes = reminder?.type === "integer" ? String(reminder.value) : "";
        categories = listField(item, "categories").join(", ");
        formError = "";
        editorOpen = true;
    };
    const save = async () => {
        if (!title.trim()) {
            formError = tr("Enter an event title.", "Введите название события.");
            return;
        }
        try {
            let startsAt: PimValue;
            let endsAt: PimValue;
            if (allDay) {
                if (!startDate || !endDate || endDate < startDate) throw new Error(tr("Choose a valid date range.", "Выберите корректный диапазон дат."));
                startsAt = { type: "record", value: { kind: "date", date: startDate } };
                endsAt = editing && endWasMissing && !endTouched
                    ? { type: "null" }
                    : { type: "record", value: { kind: "date", date: isoDate(addDays(parseIsoDate(endDate), 1)) } };
            } else {
                if (!start || (!end && !(editing && endWasMissing && !endTouched)) || (end && end <= start)) throw new Error(tr("End time must be later than start time.", "Время окончания должно быть позже начала."));
                const originalStart = editing?.fields.starts_at;
                const originalEnd = editing?.fields.ends_at;
                startsAt = editing && temporalToInputValue(originalStart) === start &&
                        (!(originalStart?.type === "record") || originalStart.value.kind !== "zoned_datetime" || originalStart.value.timezone === timezone)
                    ? originalStart ?? { type: "record", value: localDateTimeToTemporal(start, timezone) }
                    : { type: "record", value: localDateTimeToTemporal(start, timezone) };
                endsAt = editing && endWasMissing && !endTouched
                    ? { type: "null" }
                    : editing && temporalToInputValue(originalEnd) === end &&
                            (!(originalEnd?.type === "record") || originalEnd.value.kind !== "zoned_datetime" || originalEnd.value.timezone === timezone)
                        ? originalEnd ?? { type: "record", value: localDateTimeToTemporal(end, timezone) }
                        : { type: "record", value: localDateTimeToTemporal(end, timezone) };
            }
            await onSave(editing, {
                title: { type: "text", value: title.trim() },
                starts_at: startsAt,
                ends_at: endsAt,
                location: location.trim() ? { type: "text", value: location.trim() } : { type: "null" },
                notes: notes.trim() ? { type: "text", value: notes.trim() } : { type: "null" },
                recurrence_rule: recurrence ? { type: "text", value: recurrence } : { type: "null" },
                reminder_minutes: reminderMinutes ? { type: "integer", value: Number(reminderMinutes) } : { type: "null" },
                categories: { type: "text_list", value: categories.split(",").map((value) => value.trim()).filter(Boolean) },
            });
            editorOpen = false;
            reset();
        } catch (error) {
            formError = error instanceof Error ? error.message : String(error);
        }
    };

    $: visible = items
        .filter((item) => item.kind === "calendar_event" && item.spaceId === selectedSpaceId)
        .sort((left, right) => (eventStart(left)?.getTime() ?? 0) - (eventStart(right)?.getTime() ?? 0));
    $: monthDays = calendarDays(anchor);
    $: visibleWeekDays = weekDays(anchor);
    $: calendarHeading = mode === "week"
        ? `${new Intl.DateTimeFormat($locale, { month: "short", day: "numeric" }).format(visibleWeekDays[0])} – ${new Intl.DateTimeFormat($locale, { month: "short", day: "numeric", year: "numeric" }).format(visibleWeekDays[6])}`
        : new Intl.DateTimeFormat($locale, { month: "long", year: "numeric" }).format(anchor);
    $: recurrenceOptions = ["", "FREQ=DAILY", "FREQ=WEEKLY", "FREQ=MONTHLY", "FREQ=YEARLY"].includes(recurrence)
        ? ["", "FREQ=DAILY", "FREQ=WEEKLY", "FREQ=MONTHLY", "FREQ=YEARLY"]
        : ["", "FREQ=DAILY", "FREQ=WEEKLY", "FREQ=MONTHLY", "FREQ=YEARLY", recurrence];
    $: reminderOptions = ["", "0", "10", "60", "1440"].includes(reminderMinutes)
        ? ["", "0", "10", "60", "1440"]
        : ["", "0", "10", "60", "1440", reminderMinutes];
</script>

<section class="space-y-4">
    <header class="flex flex-wrap items-end justify-between gap-3">
        <div><p class="text-xs font-semibold uppercase tracking-[0.18em] text-moss">{tr("Schedule", "Расписание")}</p><h1 class="mt-1 font-heading text-2xl font-semibold text-slate">{tr("Calendar", "Календарь")}</h1></div>
        <Button on:click={() => openCreate()} disabled={!canWrite}>{tr("New event", "Новое событие")}</Button>
    </header>
    <div class="flex flex-wrap items-center gap-2">
        <select class="min-w-48 border border-slate/20 bg-paper px-3 py-2 text-sm" bind:value={selectedSpaceId} aria-label={tr("Space", "Пространство")}>{#each spaces as space}<option value={space.id}>{space.name}</option>{/each}</select>
        <div class="flex border border-slate/20 bg-white">
            {#each [["month", tr("Month", "Месяц")], ["week", tr("Week", "Неделя")], ["list", tr("Agenda", "Список")]] as option}
                <button class:active-mode={mode === option[0]} class="px-3 py-2 text-sm" on:click={() => (mode = option[0] as CalendarMode)}>{option[1]}</button>
            {/each}
        </div>
        {#if mode !== "list"}<div class="ml-auto flex items-center gap-2"><button class="border border-slate/20 bg-white px-3 py-2" aria-label={tr("Previous", "Назад")} on:click={() => go(-1)}>←</button><button class="border border-slate/20 bg-white px-3 py-2 text-sm font-semibold" on:click={() => (anchor = new Date())}>{tr("Today", "Сегодня")}</button><button class="border border-slate/20 bg-white px-3 py-2" aria-label={tr("Next", "Вперёд")} on:click={() => go(1)}>→</button></div>{/if}
    </div>
    {#if !canWrite}<p class="border-l-4 border-gold bg-sand/45 p-3 text-sm text-slate">{tr("This space is read-only or its key is unavailable on this device.", "Пространство доступно только для чтения или его ключ недоступен на устройстве.")}</p>{/if}

    {#if mode === "month"}
        <div class="border border-slate/15 bg-white/60">
            <h2 class="border-b border-slate/15 px-4 py-3 font-heading text-lg font-semibold capitalize text-slate">{calendarHeading}</h2>
            <div class="calendar-grid border-b border-slate/15 text-center text-xs font-semibold uppercase tracking-wide text-slate/55">{#each [tr("Mon", "Пн"), tr("Tue", "Вт"), tr("Wed", "Ср"), tr("Thu", "Чт"), tr("Fri", "Пт"), tr("Sat", "Сб"), tr("Sun", "Вс")] as day}<div class="p-2">{day}</div>{/each}</div>
            <div class="calendar-grid">
                {#each monthDays as day}
                    <div class:outside={day.getMonth() !== anchor.getMonth()} class:today={sameDay(day, new Date())} class="calendar-day min-h-24 border-b border-r border-slate/10 p-2 text-left">
                        <button class="day-number text-xs font-semibold" aria-label={`${tr("Create event on", "Создать событие на")} ${isoDate(day)}`} disabled={!canWrite} on:click={() => openCreate(day)}>{day.getDate()}</button>
                        <span class="mt-1 block space-y-1">
                            {#each visible.filter((item) => occursOn(item, day)).slice(0, 3) as item}
                                <button class="block w-full truncate bg-mint/15 px-1.5 py-1 text-left text-xs text-moss" on:click={() => openEdit(item)}>{isAllDay(item) ? "" : new Intl.DateTimeFormat($locale, { timeStyle: "short" }).format(eventStart(item))} {item.title}</button>
                            {/each}
                            {#if visible.filter((item) => occursOn(item, day)).length > 3}<span class="block text-xs text-slate/55">+{visible.filter((item) => occursOn(item, day)).length - 3}</span>{/if}
                        </span>
                    </div>
                {/each}
            </div>
        </div>
    {:else if mode === "week"}
        <div class="border border-slate/15 bg-white/60"><h2 class="border-b border-slate/15 px-4 py-3 font-heading text-lg font-semibold text-slate">{calendarHeading}</h2><div class="calendar-grid">{#each visibleWeekDays as day}<div class:today={sameDay(day, new Date())} class="min-h-64 border-r border-slate/10 p-2"><button class="mb-2 text-sm font-semibold text-slate" on:click={() => canWrite && openCreate(day)}>{new Intl.DateTimeFormat($locale, { weekday: "short", day: "numeric" }).format(day)}</button>{#each visible.filter((item) => occursOn(item, day)) as item}<button class="mb-1 block w-full bg-mint/15 p-2 text-left text-xs text-moss" on:click={() => openEdit(item)}><strong class="block">{item.title}</strong>{eventTime(item)}</button>{/each}</div>{/each}</div></div>
    {:else}
        <div class="space-y-2">{#each visible as item}<article class="flex flex-wrap items-center gap-3 border border-slate/15 bg-white/70 p-4"><div class="min-w-0 flex-1"><h2 class="font-semibold text-slate">{item.title}</h2><p class="text-sm text-slate/65">{eventTime(item)}{textField(item, "location") ? ` · ${textField(item, "location")}` : ""}</p>{#if textField(item, "notes")}<p class="mt-1 line-clamp-2 text-sm text-slate/60">{textField(item, "notes")}</p>{/if}</div><Button variant="ghost" on:click={() => openEdit(item)} disabled={!canWrite}>{tr("Edit", "Изменить")}</Button><Button variant="danger" on:click={() => onDelete(item)} disabled={!canWrite}>{tr("Delete", "Удалить")}</Button></article>{:else}<div class="border border-dashed border-slate/25 p-8 text-center text-sm text-slate/65">{tr("No events yet.", "Событий пока нет.")}</div>{/each}</div>
    {/if}
</section>

<Modal open={editorOpen} width="2xl" title={editing ? tr("Edit event", "Изменить событие") : tr("New event", "Новое событие")} onClose={() => (editorOpen = false)}>
        <form class="space-y-4" on:submit|preventDefault={save}>
            <label class="block text-sm font-semibold text-slate">{tr("Title", "Название")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={title} maxlength="500" /></label>
            <label class="flex items-center gap-2 text-sm font-semibold text-slate"><input type="checkbox" bind:checked={allDay} on:change={() => (endTouched = true)} />{tr("All-day event", "Событие на весь день")}</label>
            <div class="grid gap-3 sm:grid-cols-2">
                {#if allDay}
                    <label class="text-sm font-semibold text-slate">{tr("Start date", "Дата начала")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" type="date" bind:value={startDate} /></label>
                    <label class="text-sm font-semibold text-slate">{tr("End date (inclusive)", "Дата окончания (включительно)")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" type="date" bind:value={endDate} on:input={() => (endTouched = true)} /></label>
                {:else}
                    <label class="text-sm font-semibold text-slate">{tr("Starts", "Начало")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" type="datetime-local" bind:value={start} /></label>
                    <label class="text-sm font-semibold text-slate">{tr("Ends", "Окончание")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" type="datetime-local" bind:value={end} on:input={() => (endTouched = true)} /></label>
                    <label class="text-sm font-semibold text-slate sm:col-span-2">{tr("Timezone", "Часовой пояс")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={timezone} placeholder="Europe/Tbilisi" /></label>
                {/if}
                <label class="text-sm font-semibold text-slate">{tr("Location", "Место")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={location} /></label>
                <label class="text-sm font-semibold text-slate">{tr("Reminder for DAV/system apps", "Напоминание для DAV/системных приложений")}<select class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={reminderMinutes}>{#each reminderOptions as option}<option value={option}>{option === "" ? tr("None", "Нет") : option === "0" ? tr("At start", "В момент начала") : option === "10" ? `10 ${tr("minutes before", "минут до")}` : option === "60" ? `1 ${tr("hour before", "час до")}` : option === "1440" ? `1 ${tr("day before", "день до")}` : `${option} ${tr("minutes before", "минут до")}`}</option>{/each}</select><span class="mt-1 block font-normal text-xs text-slate/60">{tr("Delivered by a compatible DAV client or an enabled mobile system projection; browser notifications are not scheduled yet.", "Доставляется совместимым DAV-клиентом или включённой системной проекцией на телефоне; браузерные уведомления пока не планируются.")}</span></label>
                <label class="text-sm font-semibold text-slate">{tr("Repeat", "Повтор")}<select class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={recurrence}>{#each recurrenceOptions as option}<option value={option}>{option === "" ? tr("Never", "Никогда") : option === "FREQ=DAILY" ? tr("Daily", "Ежедневно") : option === "FREQ=WEEKLY" ? tr("Weekly", "Еженедельно") : option === "FREQ=MONTHLY" ? tr("Monthly", "Ежемесячно") : option === "FREQ=YEARLY" ? tr("Yearly", "Ежегодно") : `${tr("Imported rule", "Импортированное правило")}: ${option}`}</option>{/each}</select></label>
            </div>
            <label class="block text-sm font-semibold text-slate">{tr("Notes", "Заметки")}<textarea class="mt-1 min-h-24 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={notes}></textarea></label>
            <label class="block text-sm font-semibold text-slate">{tr("Categories (comma-separated)", "Категории через запятую")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={categories} /></label>
            {#if formError}<p class="text-sm text-coral" role="alert">{formError}</p>{/if}
            <div class="flex justify-between gap-2">{#if editing}<Button variant="danger" disabled={!canWrite} on:click={() => { void onDelete(editing!); editorOpen = false; }}>{tr("Delete", "Удалить")}</Button>{:else}<span></span>{/if}<div class="flex gap-2"><Button variant="ghost" on:click={() => (editorOpen = false)}>{tr("Cancel", "Отмена")}</Button><Button type="submit" disabled={!canWrite || busy === "pim-save"}>{busy === "pim-save" ? tr("Saving…", "Сохранение…") : tr("Save event", "Сохранить событие")}</Button></div></div>
        </form>
</Modal>

<style>
    .calendar-grid { display: grid; grid-template-columns: repeat(7, minmax(0, 1fr)); }
    .calendar-day.outside { color: rgb(28 55 49 / 0.38); background: rgb(250 247 239 / 0.5); }
    .calendar-day.today .day-number, .today > button:first-child { display: inline-grid; min-width: 1.6rem; height: 1.6rem; place-items: center; background: rgb(29 108 85); color: white; border-radius: 999px; }
    .active-mode { background: rgb(28 55 49); color: white; }
    @media (max-width: 640px) { .calendar-day { min-height: 4.5rem; padding: .35rem; } .calendar-day span button { font-size: 0; min-height: .45rem; padding: 0; } }
</style>
