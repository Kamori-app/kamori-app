<script lang="ts">
  import { onMount } from 'svelte';
  import Card from '../components/ui/Card.svelte';
  import Button from '../components/ui/Button.svelte';
  import Input from '../components/ui/Input.svelte';
  import { api, type CollectionSummary } from '../tauri';
  import { session } from '../stores/app';
  import { locale } from '../i18n';

  const copies = {
    en: { title: 'Spaces', intro: 'Create encrypted spaces for calendars, tasks, and contacts. DAV can project compatible data locally.', name: 'Space name', placeholder: 'Personal', create: 'Create space', synced: 'Synced items' },
    ru: { title: 'Пространства', intro: 'Создавайте зашифрованные пространства для календарей, задач и контактов. Совместимые данные можно локально проецировать через DAV.', name: 'Название пространства', placeholder: 'Личное', create: 'Создать пространство', synced: 'Синхронизировано' },
  } as const;
  $: copy = copies[$locale];

  let collectionName = '';
  let collections: CollectionSummary[] = [];

  const refresh = async () => {
    collections = await api.listCollections();
    const snap = await api.dashboardSnapshot();
    session.set({
      hasSession: snap.has_access_token,
      serverRunning: snap.server.running,
      bindAddr: snap.server.bind_addr,
      collectionsTotal: snap.collections_total,
      syncedItemsTotal: snap.synced_items_total,
    });
  };

  const createCollection = async () => {
    if (!collectionName.trim()) return;
    await api.createCollection(collectionName.trim());
    collectionName = '';
    await refresh();
  };

  onMount(refresh);
</script>

<section class="animate-fade-slide">
  <div class="mb-6 flex flex-wrap items-center justify-between gap-3">
    <div>
      <h1 class="font-heading text-3xl font-bold text-slate">{copy.title}</h1>
      <p class="mt-1 text-sm text-slate/70">{copy.intro}</p>
    </div>
  </div>

  <Card>
    <div class="flex flex-wrap items-end gap-3">
      <div class="min-w-[220px] flex-1">
        <p class="mb-1 block text-xs font-semibold uppercase tracking-wide text-slate/70">{copy.name}</p>
        <Input bind:value={collectionName} placeholder={copy.placeholder} />
      </div>
      <Button on:click={createCollection}>{copy.create}</Button>
    </div>
  </Card>

  <div class="mt-5 grid gap-4 md:grid-cols-2">
    {#each collections as collection}
      <Card>
        <p class="font-heading text-lg font-semibold text-slate">{collection.name}</p>
        <p class="mt-1 text-xs text-slate/70">id: {collection.id}</p>
        <p class="mt-1 text-sm text-slate/80">{copy.synced}: {collection.synced_items}</p>
      </Card>
    {/each}
  </div>
</section>
