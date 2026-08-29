<script lang="ts">
    import Button from "$lib/components/ui/Button.svelte";
    import Modal from "$lib/components/ui/Modal.svelte";
    import type { MaterializedPimItem, PimValue } from "$lib/pim";
    import { locale } from "$lib/i18n";

    type LabeledValue = { label: string; value: string; raw_head?: string };
    type AddressValue = { label: string; po_box?: string; extended?: string; street: string; locality: string; region: string; postal_code: string; country: string; raw_head?: string };
    type SortMode = "name" | "family" | "organization";

    export let spaces: { id: string; name: string }[] = [];
    export let selectedSpaceId = "";
    export let canWrite = false;
    export let items: MaterializedPimItem[] = [];
    export let busy = "";
    export let onSave: (item: MaterializedPimItem | undefined, fields: Record<string, PimValue>) => Promise<void>;
    export let onDelete: (item: MaterializedPimItem) => Promise<void>;

    let search = "";
    let sort: SortMode = "name";
    let favoritesOnly = false;
    let editorOpen = false;
    let editing: MaterializedPimItem | undefined;
    let displayName = "";
    let given = "";
    let family = "";
    let middle = "";
    let prefix = "";
    let suffix = "";
    let organization = "";
    let jobTitle = "";
    let birthday = "";
    let url = "";
    let notes = "";
    let categories = "";
    let favorite = false;
    let emails: LabeledValue[] = [{ label: "home", value: "" }];
    let phones: LabeledValue[] = [{ label: "mobile", value: "" }];
    let addresses: AddressValue[] = [];
    let formError = "";

    const tr = (english: string, russian: string) => $locale === "ru" ? russian : english;
    const textField = (item: MaterializedPimItem, field: string) => {
        const value = item.fields[field];
        return value?.type === "text" ? value.value : "";
    };
    const recordField = (item: MaterializedPimItem, field: string) => {
        const value = item.fields[field];
        return value?.type === "record" ? value.value : {};
    };
    const recordsField = (item: MaterializedPimItem, field: string): Record<string, string>[] => {
        const value = item.fields[field];
        return value?.type === "records" ? value.value : [];
    };
    const listField = (item: MaterializedPimItem, field: string) => {
        const value = item.fields[field];
        return value?.type === "text_list" ? value.value : [];
    };
    const boolField = (item: MaterializedPimItem, field: string) => {
        const value = item.fields[field];
        return value?.type === "boolean" && value.value;
    };
    const contactSearchText = (item: MaterializedPimItem) => [
        item.title,
        textField(item, "organization"),
        textField(item, "job_title"),
        ...recordsField(item, "emails").map((value) => value.value),
        ...recordsField(item, "phones").map((value) => value.value),
    ].join(" ").toLocaleLowerCase();
    const sortValue = (item: MaterializedPimItem) => {
        if (sort === "family") return recordField(item, "name").family || item.title;
        if (sort === "organization") return textField(item, "organization") || item.title;
        return item.title;
    };

    const reset = () => {
        editing = undefined;
        displayName = "";
        given = "";
        family = "";
        middle = "";
        prefix = "";
        suffix = "";
        organization = "";
        jobTitle = "";
        birthday = "";
        url = "";
        notes = "";
        categories = "";
        favorite = false;
        emails = [{ label: "home", value: "" }];
        phones = [{ label: "mobile", value: "" }];
        addresses = [];
        formError = "";
    };
    const openCreate = () => { reset(); editorOpen = true; };
    const openEdit = (item: MaterializedPimItem) => {
        reset();
        editing = item;
        displayName = item.title;
        const name = recordField(item, "name");
        given = name.given ?? "";
        family = name.family ?? "";
        middle = name.middle ?? "";
        prefix = name.prefix ?? "";
        suffix = name.suffix ?? "";
        organization = textField(item, "organization");
        jobTitle = textField(item, "job_title");
        birthday = textField(item, "birthday");
        url = textField(item, "url");
        notes = textField(item, "notes");
        categories = listField(item, "categories").join(", ");
        favorite = boolField(item, "favorite");
        const storedEmails = recordsField(item, "emails") as LabeledValue[];
        const storedPhones = recordsField(item, "phones") as LabeledValue[];
        emails = storedEmails.length > 0 ? storedEmails.map((entry) => ({ ...entry })) : [{ label: "home", value: textField(item, "email") }];
        phones = storedPhones.length > 0 ? storedPhones.map((entry) => ({ ...entry })) : [{ label: "mobile", value: textField(item, "phone") }];
        addresses = (recordsField(item, "addresses") as AddressValue[]).map((entry) => ({ ...entry }));
        editorOpen = true;
    };
    const removeEmail = (index: number) => { emails = emails.filter((_, itemIndex) => itemIndex !== index); };
    const removePhone = (index: number) => { phones = phones.filter((_, itemIndex) => itemIndex !== index); };
    const removeAddress = (index: number) => { addresses = addresses.filter((_, itemIndex) => itemIndex !== index); };

    const save = async () => {
        const title = displayName.trim() || [prefix, given, middle, family, suffix]
            .map((part) => part.trim())
            .filter(Boolean)
            .join(" ");
        if (!title) {
            formError = tr("Enter a display name or name parts.", "Введите отображаемое имя или части имени.");
            return;
        }
        try {
            await onSave(editing, {
                title: { type: "text", value: title },
                name: { type: "record", value: { family: family.trim(), given: given.trim(), middle: middle.trim(), prefix: prefix.trim(), suffix: suffix.trim() } },
                emails: { type: "records", value: emails.map((entry) => ({ ...entry, label: entry.label.trim(), value: entry.value.trim() })).filter((entry) => entry.value) },
                phones: { type: "records", value: phones.map((entry) => ({ ...entry, label: entry.label.trim(), value: entry.value.trim() })).filter((entry) => entry.value) },
                addresses: { type: "records", value: addresses
                    .map((entry) => Object.fromEntries(Object.entries(entry).map(([key, value]) => [key, value.trim()])))
                    .filter((entry) => ["po_box", "extended", "street", "locality", "region", "postal_code", "country"].some((key) => entry[key])) },
                organization: organization.trim() ? { type: "text", value: organization.trim() } : { type: "null" },
                job_title: jobTitle.trim() ? { type: "text", value: jobTitle.trim() } : { type: "null" },
                birthday: birthday ? { type: "text", value: birthday } : { type: "null" },
                url: url.trim() ? { type: "text", value: url.trim() } : { type: "null" },
                notes: notes.trim() ? { type: "text", value: notes.trim() } : { type: "null" },
                favorite: { type: "boolean", value: favorite },
                categories: { type: "text_list", value: categories.split(",").map((value) => value.trim()).filter(Boolean) },
            });
            editorOpen = false;
            reset();
        } catch (error) {
            formError = error instanceof Error ? error.message : String(error);
        }
    };

    $: visible = items
        .filter((item) => item.kind === "contact" && item.spaceId === selectedSpaceId)
        .filter((item) => !favoritesOnly || boolField(item, "favorite"))
        .filter((item) => contactSearchText(item).includes(search.trim().toLocaleLowerCase()))
        .sort((left, right) => sortValue(left).localeCompare(sortValue(right), $locale, { sensitivity: "base" }));
</script>

<section class="space-y-4">
    <header class="flex flex-wrap items-end justify-between gap-3"><div><p class="text-xs font-semibold uppercase tracking-[0.18em] text-moss">{tr("People", "Люди")}</p><h1 class="mt-1 font-heading text-2xl font-semibold text-slate">{tr("Contacts", "Контакты")}</h1></div><Button on:click={openCreate} disabled={!canWrite}>{tr("New contact", "Новый контакт")}</Button></header>
    <div class="grid gap-2 lg:grid-cols-[minmax(12rem,18rem)_1fr_auto_auto]">
        <select class="border border-slate/20 bg-paper px-3 py-2 text-sm" bind:value={selectedSpaceId} aria-label={tr("Space", "Пространство")}>{#each spaces as space}<option value={space.id}>{space.name}</option>{/each}</select>
        <input class="border border-slate/20 bg-paper px-3 py-2 text-sm outline-none focus:ring" bind:value={search} placeholder={tr("Search names, companies, email, or phone", "Поиск по имени, компании, email или телефону")} />
        <select class="border border-slate/20 bg-paper px-3 py-2 text-sm" bind:value={sort} aria-label={tr("Sort contacts", "Сортировка контактов")}><option value="name">{tr("Sort by name", "По имени")}</option><option value="family">{tr("Sort by family name", "По фамилии")}</option><option value="organization">{tr("Sort by company", "По компании")}</option></select>
        <button class:favorites-active={favoritesOnly} class="border border-slate/20 bg-white px-3 py-2 text-sm font-semibold" on:click={() => (favoritesOnly = !favoritesOnly)}>★ {tr("Favorites", "Избранные")}</button>
    </div>
    {#if !canWrite}<p class="border-l-4 border-gold bg-sand/45 p-3 text-sm text-slate">{tr("This space is read-only or its key is unavailable on this device.", "Пространство доступно только для чтения или его ключ недоступен на устройстве.")}</p>{/if}
    <div class="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
        {#each visible as item}
            <article class="flex min-h-44 flex-col border border-slate/15 bg-white/70 p-4">
                <div class="flex items-start gap-3"><div class="grid size-10 shrink-0 place-items-center rounded-full bg-mint/15 font-heading font-semibold text-moss">{item.title.slice(0, 1).toLocaleUpperCase()}</div><div class="min-w-0 flex-1"><h2 class="truncate font-semibold text-slate">{item.title} {boolField(item, "favorite") ? "★" : ""}</h2>{#if textField(item, "organization")}<p class="truncate text-sm text-slate/60">{textField(item, "job_title")}{textField(item, "job_title") && textField(item, "organization") ? " · " : ""}{textField(item, "organization")}</p>{/if}</div></div>
                <div class="mt-3 space-y-1 text-sm text-slate/70">{#each recordsField(item, "emails").slice(0, 2) as email}<a class="block truncate underline-offset-2 hover:underline" href={`mailto:${email.value}`}>{email.value} <span class="text-xs text-slate/45">{email.label}</span></a>{/each}{#each recordsField(item, "phones").slice(0, 2) as phone}<a class="block truncate underline-offset-2 hover:underline" href={`tel:${phone.value}`}>{phone.value} <span class="text-xs text-slate/45">{phone.label}</span></a>{/each}</div>
                <div class="mt-auto flex gap-2 pt-4"><Button variant="ghost" on:click={() => openEdit(item)}>{tr("Details", "Подробнее")}</Button><Button variant="danger" on:click={() => onDelete(item)} disabled={!canWrite}>{tr("Delete", "Удалить")}</Button></div>
            </article>
        {:else}
            <div class="border border-dashed border-slate/25 p-8 text-center text-sm text-slate/65 md:col-span-2 xl:col-span-3">{tr("No matching contacts.", "Подходящих контактов нет.")}</div>
        {/each}
    </div>
</section>

<Modal open={editorOpen} width="3xl" title={editing ? tr("Edit contact", "Изменить контакт") : tr("New contact", "Новый контакт")} onClose={() => (editorOpen = false)}>
        <form class="space-y-5" on:submit|preventDefault={save}>
            <label class="block text-sm font-semibold text-slate">{tr("Display name", "Отображаемое имя")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={displayName} maxlength="500" /></label>
            <fieldset><legend class="mb-2 text-sm font-semibold text-slate">{tr("Structured name", "Составное имя")}</legend><div class="grid gap-2 sm:grid-cols-2 lg:grid-cols-5"><input class="border border-slate/20 bg-white px-3 py-2 text-sm" bind:value={prefix} placeholder={tr("Prefix", "Обращение")} /><input class="border border-slate/20 bg-white px-3 py-2 text-sm" bind:value={given} placeholder={tr("Given name", "Имя")} /><input class="border border-slate/20 bg-white px-3 py-2 text-sm" bind:value={middle} placeholder={tr("Middle name", "Отчество")} /><input class="border border-slate/20 bg-white px-3 py-2 text-sm" bind:value={family} placeholder={tr("Family name", "Фамилия")} /><input class="border border-slate/20 bg-white px-3 py-2 text-sm" bind:value={suffix} placeholder={tr("Suffix", "Суффикс")} /></div></fieldset>
            <div class="grid gap-3 sm:grid-cols-2"><label class="text-sm font-semibold text-slate">{tr("Organization", "Организация")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={organization} /></label><label class="text-sm font-semibold text-slate">{tr("Job title", "Должность")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={jobTitle} /></label></div>
            <fieldset><div class="mb-2 flex items-center justify-between"><legend class="text-sm font-semibold text-slate">{tr("Email addresses", "Email-адреса")}</legend><Button variant="ghost" on:click={() => (emails = [...emails, { label: "work", value: "" }])}>+ {tr("Email", "Email")}</Button></div><div class="space-y-2">{#each emails as email, index}<div class="grid grid-cols-[8rem_1fr_auto] gap-2"><input class="border border-slate/20 bg-white px-3 py-2 text-sm" bind:value={email.label} placeholder={tr("Label", "Метка")} /><input class="border border-slate/20 bg-white px-3 py-2 text-sm" type="email" bind:value={email.value} placeholder="name@example.com" /><button type="button" class="px-2 text-coral" aria-label={tr("Remove email", "Удалить email")} on:click={() => removeEmail(index)}>×</button></div>{/each}</div></fieldset>
            <fieldset><div class="mb-2 flex items-center justify-between"><legend class="text-sm font-semibold text-slate">{tr("Phone numbers", "Телефоны")}</legend><Button variant="ghost" on:click={() => (phones = [...phones, { label: "mobile", value: "" }])}>+ {tr("Phone", "Телефон")}</Button></div><div class="space-y-2">{#each phones as phone, index}<div class="grid grid-cols-[8rem_1fr_auto] gap-2"><input class="border border-slate/20 bg-white px-3 py-2 text-sm" bind:value={phone.label} placeholder={tr("Label", "Метка")} /><input class="border border-slate/20 bg-white px-3 py-2 text-sm" type="tel" bind:value={phone.value} /><button type="button" class="px-2 text-coral" aria-label={tr("Remove phone", "Удалить телефон")} on:click={() => removePhone(index)}>×</button></div>{/each}</div></fieldset>
            <fieldset><div class="mb-2 flex items-center justify-between"><legend class="text-sm font-semibold text-slate">{tr("Addresses", "Адреса")}</legend><Button variant="ghost" on:click={() => (addresses = [...addresses, { label: "home", street: "", locality: "", region: "", postal_code: "", country: "" }])}>+ {tr("Address", "Адрес")}</Button></div><div class="space-y-3">{#each addresses as address, index}<div class="grid gap-2 border border-slate/10 bg-white/50 p-3 sm:grid-cols-2"><input class="border border-slate/20 bg-white px-3 py-2 text-sm" bind:value={address.label} placeholder={tr("Label", "Метка")} /><input class="border border-slate/20 bg-white px-3 py-2 text-sm" bind:value={address.street} placeholder={tr("Street", "Улица")} /><input class="border border-slate/20 bg-white px-3 py-2 text-sm" bind:value={address.locality} placeholder={tr("City", "Город")} /><input class="border border-slate/20 bg-white px-3 py-2 text-sm" bind:value={address.region} placeholder={tr("Region", "Регион")} /><input class="border border-slate/20 bg-white px-3 py-2 text-sm" bind:value={address.postal_code} placeholder={tr("Postal code", "Индекс")} /><input class="border border-slate/20 bg-white px-3 py-2 text-sm" bind:value={address.country} placeholder={tr("Country", "Страна")} /><button type="button" class="justify-self-start text-sm text-coral" on:click={() => removeAddress(index)}>{tr("Remove address", "Удалить адрес")}</button></div>{/each}</div></fieldset>
            <div class="grid gap-3 sm:grid-cols-2"><label class="text-sm font-semibold text-slate">{tr("Birthday", "День рождения")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" type="text" inputmode="numeric" placeholder={tr("YYYY-MM-DD or --MM-DD", "ГГГГ-ММ-ДД или --ММ-ДД")} bind:value={birthday} /></label><label class="text-sm font-semibold text-slate">{tr("Website", "Сайт")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" type="url" bind:value={url} /></label></div>
            <label class="block text-sm font-semibold text-slate">{tr("Notes", "Заметки")}<textarea class="mt-1 min-h-24 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={notes}></textarea></label>
            <label class="block text-sm font-semibold text-slate">{tr("Categories (comma-separated)", "Категории через запятую")}<input class="mt-1 w-full border border-slate/20 bg-white px-3 py-2 font-normal" bind:value={categories} /></label>
            <label class="flex items-center gap-2 text-sm font-semibold text-slate"><input type="checkbox" bind:checked={favorite} />★ {tr("Favorite contact", "Избранный контакт")}</label>
            {#if formError}<p class="text-sm text-coral" role="alert">{formError}</p>{/if}
            <div class="flex justify-end gap-2"><Button variant="ghost" on:click={() => (editorOpen = false)}>{tr("Cancel", "Отмена")}</Button><Button type="submit" disabled={!canWrite || busy === "pim-save"}>{busy === "pim-save" ? tr("Saving…", "Сохранение…") : tr("Save contact", "Сохранить контакт")}</Button></div>
        </form>
</Modal>

<style>.favorites-active { background: rgb(29 108 85); color: white; }</style>
