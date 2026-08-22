<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import Card from '../components/ui/Card.svelte';
  import Button from '../components/ui/Button.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import { api, type DavConnectionInfo } from '../tauri';
  import { session, loginNotice } from '../stores/app';
  import { locale } from '../i18n';

  const copies = {
    en: {
      title: 'Desktop control center', intro: 'Encrypted sync is primary. The local DAV bridge is an optional compatibility layer.', running: 'Running', stopped: 'Stopped',
      server: 'Local DAV bridge', port: 'Address', spaces: 'Spaces', vaults: 'Encrypted spaces', items: 'Synced items', cache: 'Applied to local cache',
      start: 'Start DAV bridge', stop: 'Stop DAV bridge', syncing: 'Syncing…', sync: 'Sync now', hide: 'Hide DAV details', show: 'Set up a DAV app',
      started: 'Local DAV bridge started. Background encrypted sync is active.', stoppedNotice: 'Local DAV bridge and background sync stopped.',
      syncDone: (count: number) => `Sync complete: applied ${count} encrypted operations.`, stopRotate: 'Stop the local bridge before rotating credentials.',
      rotateConfirm: 'Rotate the DAV password? Existing DAV apps will need the new password.', rotated: 'DAV password rotated. Update every connected DAV app.',
      copied: (label: string) => `${label} copied.`, copyFailed: (label: string) => `Could not copy ${label.toLowerCase()}. Select it manually.`,
      setup: 'Optional DAV app setup', setupBody: 'Details remain visible for 60 seconds. The bridge accepts connections only from this computer.', rotate: 'Rotate password',
      username: 'Username', password: 'Dedicated DAV password', copy: 'Copy', noSpaces: 'Create a space before connecting a DAV app.', calendar: 'Calendar URL', addressBook: 'Address book URL',
      davFootnote: 'Use a direct collection URL in an app that supports custom CalDAV/CardDAV servers. Your Kamori account password is never exposed to DAV.',
    },
    ru: {
      title: 'Центр управления', intro: 'Основа продукта — зашифрованная синхронизация. Локальный DAV-мост остаётся необязательным слоем совместимости.', running: 'Работает', stopped: 'Остановлен',
      server: 'Локальный DAV-мост', port: 'Адрес', spaces: 'Пространства', vaults: 'Зашифрованные пространства', items: 'Синхронизировано', cache: 'Применено к локальному кешу',
      start: 'Запустить DAV-мост', stop: 'Остановить DAV-мост', syncing: 'Синхронизация…', sync: 'Синхронизировать', hide: 'Скрыть данные DAV', show: 'Настроить DAV-приложение',
      started: 'Локальный DAV-мост запущен. Фоновая зашифрованная синхронизация активна.', stoppedNotice: 'Локальный DAV-мост и фоновая синхронизация остановлены.',
      syncDone: (count: number) => `Синхронизация завершена: применено операций — ${count}.`, stopRotate: 'Перед сменой реквизитов остановите локальный мост.',
      rotateConfirm: 'Сменить пароль DAV? Его потребуется обновить во всех подключённых приложениях.', rotated: 'Пароль DAV изменён. Обновите его во всех подключённых приложениях.',
      copied: (label: string) => `${label}: скопировано.`, copyFailed: (label: string) => `Не удалось скопировать «${label}». Выделите значение вручную.`,
      setup: 'Настройка необязательного DAV-приложения', setupBody: 'Данные показываются 60 секунд. Мост принимает подключения только с этого компьютера.', rotate: 'Сменить пароль',
      username: 'Имя пользователя', password: 'Отдельный пароль DAV', copy: 'Копировать', noSpaces: 'Создайте пространство перед подключением DAV-приложения.', calendar: 'URL календаря', addressBook: 'URL адресной книги',
      davFootnote: 'Используйте прямой URL коллекции в приложении с поддержкой собственных CalDAV/CardDAV-серверов. Пароль аккаунта Kamori никогда не передаётся в DAV.',
    },
  } as const;
  $: copy = copies[$locale];

  let syncing = false;
  let operationError = '';
  let davInfo: DavConnectionInfo | null = null;
  let hideCredentialsTimer: ReturnType<typeof setTimeout> | null = null;

  const errorMessage = (error: unknown) =>
    error instanceof Error ? error.message : String(error);

  const refresh = async () => {
    const snap = await api.dashboardSnapshot();
    session.set({
      hasSession: snap.has_access_token,
      serverRunning: snap.server.running,
      bindAddr: snap.server.bind_addr,
      collectionsTotal: snap.collections_total,
      syncedItemsTotal: snap.synced_items_total,
    });
  };

  const startServer = async () => {
    operationError = '';
    try {
      await api.startLocalServer();
      loginNotice.set(copy.started);
      await refresh();
    } catch (error) {
      operationError = errorMessage(error);
    }
  };

  const stopServer = async () => {
    operationError = '';
    try {
      await api.stopLocalServer();
      loginNotice.set(copy.stoppedNotice);
      await refresh();
    } catch (error) {
      operationError = errorMessage(error);
    }
  };

  const syncNow = async () => {
    syncing = true;
    operationError = '';
    try {
      const applied = await api.syncNow();
      loginNotice.set(copy.syncDone(applied));
      await refresh();
    } catch (error) {
      operationError = errorMessage(error);
    } finally {
      syncing = false;
    }
  };

  const showDavSetup = async () => {
    operationError = '';
    try {
      davInfo = await api.davConnectionInfo();
      if (hideCredentialsTimer) clearTimeout(hideCredentialsTimer);
      hideCredentialsTimer = setTimeout(() => {
        davInfo = null;
        hideCredentialsTimer = null;
      }, 60_000);
    } catch (error) {
      operationError = errorMessage(error);
    }
  };

  const hideDavSetup = () => {
    davInfo = null;
    if (hideCredentialsTimer) {
      clearTimeout(hideCredentialsTimer);
      hideCredentialsTimer = null;
    }
  };

  const rotateCredentials = async () => {
    operationError = '';
    if ($session.serverRunning) {
      operationError = copy.stopRotate;
      return;
    }
    if (!window.confirm(copy.rotateConfirm)) {
      return;
    }
    try {
      davInfo = await api.rotateDavCredentials();
      loginNotice.set(copy.rotated);
    } catch (error) {
      operationError = errorMessage(error);
    }
  };

  const copyValue = async (value: string, label: string) => {
    try {
      await navigator.clipboard.writeText(value);
      loginNotice.set(copy.copied(label));
    } catch {
      operationError = copy.copyFailed(label);
    }
  };

  onMount(refresh);
  onDestroy(hideDavSetup);
</script>

<section class="animate-fade-slide">
  <div class="mb-6 flex items-center justify-between">
    <div>
      <h1 class="font-heading text-3xl font-bold text-slate">{copy.title}</h1>
      <p class="mt-1 text-sm text-slate/70">{copy.intro}</p>
    </div>

    <Badge active={$session.serverRunning}>{$session.serverRunning ? copy.running : copy.stopped}</Badge>
  </div>

  <div class="grid gap-4 md:grid-cols-3">
    <Card>
      <p class="text-xs uppercase tracking-wide text-slate/60">{copy.server}</p>
      <p class="mt-2 text-2xl font-bold text-slate">{$session.serverRunning ? copy.running : copy.stopped}</p>
      <p class="text-sm text-slate/70">{copy.port}: {$session.bindAddr}</p>
    </Card>

    <Card>
      <p class="text-xs uppercase tracking-wide text-slate/60">{copy.spaces}</p>
      <p class="mt-2 text-2xl font-bold text-slate">{$session.collectionsTotal}</p>
      <p class="text-sm text-slate/70">{copy.vaults}</p>
    </Card>

    <Card>
      <p class="text-xs uppercase tracking-wide text-slate/60">{copy.items}</p>
      <p class="mt-2 text-2xl font-bold text-slate">{$session.syncedItemsTotal}</p>
      <p class="text-sm text-slate/70">{copy.cache}</p>
    </Card>
  </div>

  <div class="mt-6 flex flex-wrap gap-3">
    <Button disabled={$session.serverRunning} on:click={startServer}>{copy.start}</Button>
    <Button variant="ghost" disabled={!$session.serverRunning} on:click={stopServer}>{copy.stop}</Button>
    <Button variant="secondary" disabled={syncing || !$session.serverRunning} on:click={syncNow}>{syncing ? copy.syncing : copy.sync}</Button>
    <Button variant="secondary" on:click={davInfo ? hideDavSetup : showDavSetup}>
      {davInfo ? copy.hide : copy.show}
    </Button>
  </div>

  {#if operationError}
    <p class="mt-4 rounded-xl bg-red-50 p-3 text-sm text-red-800">{operationError}</p>
  {/if}

  {#if davInfo}
    <div class="mt-6">
    <Card>
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p class="text-lg font-semibold text-slate">{copy.setup}</p>
          <p class="mt-1 text-sm text-slate/70">
            {copy.setupBody}
          </p>
        </div>
        <Button variant="ghost" disabled={$session.serverRunning} on:click={rotateCredentials}>
          {copy.rotate}
        </Button>
      </div>

      <div class="mt-4 grid gap-3 md:grid-cols-2">
        <div class="rounded-xl border border-slate/15 bg-white/70 p-3">
          <p class="text-xs uppercase tracking-wide text-slate/60">{copy.username}</p>
          <code class="mt-1 block break-all text-sm">{davInfo.username}</code>
          <button class="mt-2 text-xs font-semibold text-teal-700" on:click={() => copyValue(davInfo?.username ?? '', copy.username)}>{copy.copy}</button>
        </div>
        <div class="rounded-xl border border-slate/15 bg-white/70 p-3">
          <p class="text-xs uppercase tracking-wide text-slate/60">{copy.password}</p>
          <code class="mt-1 block break-all text-sm">{davInfo.password}</code>
          <button class="mt-2 text-xs font-semibold text-teal-700" on:click={() => copyValue(davInfo?.password ?? '', copy.password)}>{copy.copy}</button>
        </div>
      </div>

      {#if davInfo.collections.length === 0}
        <p class="mt-4 text-sm text-slate/70">{copy.noSpaces}</p>
      {:else}
        <div class="mt-4 space-y-4">
          {#each davInfo.collections as collection}
            <div class="rounded-xl border border-slate/15 bg-white/70 p-3">
              <p class="font-semibold text-slate">{collection.name}</p>
              <p class="mt-2 text-xs uppercase tracking-wide text-slate/60">{copy.calendar}</p>
              <code class="mt-1 block break-all text-xs">{collection.calendar_url}</code>
              <button class="mt-1 text-xs font-semibold text-teal-700" on:click={() => copyValue(collection.calendar_url, copy.calendar)}>{copy.copy}</button>
              <p class="mt-3 text-xs uppercase tracking-wide text-slate/60">{copy.addressBook}</p>
              <code class="mt-1 block break-all text-xs">{collection.address_book_url}</code>
              <button class="mt-1 text-xs font-semibold text-teal-700" on:click={() => copyValue(collection.address_book_url, copy.addressBook)}>{copy.copy}</button>
            </div>
          {/each}
        </div>
      {/if}

      <p class="mt-4 text-xs text-slate/60">
        {copy.davFootnote}
      </p>
    </Card>
    </div>
  {/if}

  {#if $loginNotice}
    <p class="mt-4 rounded-xl bg-sand/40 p-3 text-sm text-slate">{$loginNotice}</p>
  {/if}
</section>
