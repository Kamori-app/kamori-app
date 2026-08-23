<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { route, navigate } from './lib/router';
  import {
    backendSettings,
    loginNotice,
    saveBackendSettings,
    saveWindowPreferences,
    session,
    windowPreferences,
  } from './lib/stores/app';
  import { api } from './lib/tauri';
  import LoginPage from './lib/pages/LoginPage.svelte';
  import DashboardPage from './lib/pages/DashboardPage.svelte';
  import CollectionsPage from './lib/pages/CollectionsPage.svelte';
  import TopNav from './lib/components/TopNav.svelte';
  import Modal from './lib/components/ui/Modal.svelte';
  import Input from './lib/components/ui/Input.svelte';
  import Button from './lib/components/ui/Button.svelte';
  import { locale } from './lib/i18n';

  const copies = {
    en: {
      openSettings: 'Open desktop settings', settings: 'Desktop settings', cloudUrl: 'Kamori service URL', cache: 'SQLite cache path', fixed: 'fixed',
      closeBehavior: 'When the window closes', hide: 'Keep running in background', minimize: 'Minimize the window', quit: 'Quit Kamori', tray: 'Show tray icon',
      save: 'Save settings', saved: 'Settings saved.', savedTray: 'Settings saved. The tray icon was enabled because background mode needs it.', failed: 'Failed to save settings.',
      unsaved: 'Unsaved changes', unsavedBody: 'You have unsaved settings changes. Closing now will discard them.', keep: 'Keep editing', discard: 'Discard changes',
    },
    ru: {
      openSettings: 'Открыть настройки приложения', settings: 'Настройки приложения', cloudUrl: 'Адрес сервиса Kamori', cache: 'Путь к кешу SQLite', fixed: 'неизменяемый',
      closeBehavior: 'При закрытии окна', hide: 'Продолжать работу в фоне', minimize: 'Свернуть окно', quit: 'Завершить Kamori', tray: 'Показывать значок в трее',
      save: 'Сохранить настройки', saved: 'Настройки сохранены.', savedTray: 'Настройки сохранены. Значок в трее включён для фонового режима.', failed: 'Не удалось сохранить настройки.',
      unsaved: 'Несохранённые изменения', unsavedBody: 'Настройки изменены. Если закрыть окно сейчас, изменения будут потеряны.', keep: 'Продолжить редактирование', discard: 'Отменить изменения',
    },
  } as const;
  $: copy = copies[$locale];

  const FIXED_SQLITE_CACHE_PATH = '.kamori/local-cache.sqlite3';

  let settingsOpen = false;
  let settingsCloudBaseUrl = '';
  let settingsCloseBehavior: 'quit' | 'hide' | 'minimize' = 'quit';
  let settingsShowTrayIcon = false;
  let settingsSaveError = '';
  let discardChangesOpen = false;
  let settingsNoticeTimer: ReturnType<typeof setTimeout> | null = null;
  let settingsInitial: {
    cloudBaseUrl: string;
    closeBehavior: 'quit' | 'hide' | 'minimize';
    showTrayIcon: boolean;
  } | null = null;

  const openSettings = () => {
    settingsCloudBaseUrl = $backendSettings.cloudBaseUrl;
    settingsCloseBehavior = $windowPreferences.closeBehavior;
    settingsShowTrayIcon = $windowPreferences.showTrayIcon;
    settingsSaveError = '';
    settingsInitial = {
      cloudBaseUrl: settingsCloudBaseUrl,
      closeBehavior: settingsCloseBehavior,
      showTrayIcon: settingsShowTrayIcon,
    };
    settingsOpen = true;
  };

  const hasUnsavedSettingsChanges = () =>
    settingsInitial !== null &&
    (settingsCloudBaseUrl !== settingsInitial.cloudBaseUrl ||
      settingsCloseBehavior !== settingsInitial.closeBehavior ||
      settingsShowTrayIcon !== settingsInitial.showTrayIcon);

  const closeSettingsImmediately = () => {
    settingsOpen = false;
    discardChangesOpen = false;
    settingsSaveError = '';
    settingsInitial = null;
  };

  const requestCloseSettings = () => {
    if (hasUnsavedSettingsChanges()) {
      settingsOpen = false;
      discardChangesOpen = true;
      return;
    }
    closeSettingsImmediately();
  };

  const keepEditingSettings = () => {
    discardChangesOpen = false;
    settingsOpen = true;
  };

  const discardSettingsChanges = () => {
    closeSettingsImmediately();
  };

  const onAppKeydown = (event: KeyboardEvent) => {
    if (event.key !== 'Escape' && event.key !== 'Esc') {
      return;
    }

    if (discardChangesOpen) {
      event.preventDefault();
      event.stopImmediatePropagation();
      discardSettingsChanges();
      return;
    }

    if (settingsOpen) {
      event.preventDefault();
      event.stopImmediatePropagation();
      requestCloseSettings();
    }
  };

  const saveSettings = async () => {
    settingsSaveError = '';
    const backendNext = {
      cloudBaseUrl: settingsCloudBaseUrl.trim() || 'https://api.kamori.app',
    };

    const windowNext = {
      closeBehavior: settingsCloseBehavior,
      showTrayIcon:
        settingsShowTrayIcon || settingsCloseBehavior !== 'hide' ? settingsShowTrayIcon : true,
    };

    try {
      await api.configureBackend(backendNext.cloudBaseUrl);
      await api.applyWindowPreferences(windowNext.closeBehavior, windowNext.showTrayIcon);
      saveBackendSettings(backendNext);
      saveWindowPreferences(windowNext);

      const trayAdjusted =
        settingsCloseBehavior === 'hide' && settingsShowTrayIcon !== windowNext.showTrayIcon;
      const savedMessage = trayAdjusted
        ? copy.savedTray
        : copy.saved;

      loginNotice.set(savedMessage);
      if (settingsNoticeTimer) {
        clearTimeout(settingsNoticeTimer);
      }
      settingsNoticeTimer = setTimeout(() => {
        loginNotice.update((current) => (current === savedMessage ? '' : current));
        settingsNoticeTimer = null;
      }, 5000);
      closeSettingsImmediately();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      settingsSaveError = message || copy.failed;
    }
  };

  onMount(async () => {
    try {
      await api.configureBackend($backendSettings.cloudBaseUrl);
      await api.restoreSession();
      await api.applyWindowPreferences(
        $windowPreferences.closeBehavior,
        $windowPreferences.showTrayIcon,
      );

      const snap = await api.dashboardSnapshot();
      session.set({
        hasSession: snap.has_access_token,
        serverRunning: snap.server.running,
        bindAddr: snap.server.bind_addr,
        collectionsTotal: snap.collections_total,
        syncedItemsTotal: snap.synced_items_total,
      });

      if (snap.has_access_token && $route === '/login') {
        navigate('/dashboard');
      }
    } catch {
      navigate('/login');
    }
  });

  onDestroy(() => {
    if (settingsNoticeTimer) {
      clearTimeout(settingsNoticeTimer);
      settingsNoticeTimer = null;
    }
  });
</script>

<svelte:window on:keydown|capture={onAppKeydown} />

<main class="min-h-screen px-4 py-6 md:px-8">
  <div class="fixed right-4 top-4 z-40 flex items-center gap-2">
  <button
    class="inline-flex h-11 w-11 items-center justify-center border border-slate/20 bg-paper text-slate transition hover:bg-sand/50"
    on:click={openSettings}
    aria-label={copy.openSettings}
    title={copy.settings}
  >
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      class="h-5 w-5"
      stroke="currentColor"
      stroke-width="1.9"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path d="M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4Z" />
      <path d="m19.4 15-.3.6a1 1 0 0 0 .2 1.1l.1.1a1 1 0 0 1 0 1.4l-1.3 1.3a1 1 0 0 1-1.4 0l-.1-.1a1 1 0 0 0-1.1-.2l-.6.3a1 1 0 0 0-.6.9V21a1 1 0 0 1-1 1h-1.8a1 1 0 0 1-1-1v-.2a1 1 0 0 0-.6-.9l-.6-.3a1 1 0 0 0-1.1.2l-.1.1a1 1 0 0 1-1.4 0L4.6 18a1 1 0 0 1 0-1.4l.1-.1a1 1 0 0 0 .2-1.1l-.3-.6a1 1 0 0 0-.9-.6H3.5a1 1 0 0 1-1-1v-1.8a1 1 0 0 1 1-1h.2a1 1 0 0 0 .9-.6l.3-.6a1 1 0 0 0-.2-1.1l-.1-.1a1 1 0 0 1 0-1.4L5.9 4a1 1 0 0 1 1.4 0l.1.1a1 1 0 0 0 1.1.2l.6-.3a1 1 0 0 0 .6-.9V3a1 1 0 0 1 1-1h1.8a1 1 0 0 1 1 1v.2a1 1 0 0 0 .6.9l.6.3a1 1 0 0 0 1.1-.2l.1-.1a1 1 0 0 1 1.4 0l1.3 1.3a1 1 0 0 1 0 1.4l-.1.1a1 1 0 0 0-.2 1.1l.3.6a1 1 0 0 0 .9.6h.2a1 1 0 0 1 1 1v1.8a1 1 0 0 1-1 1h-.2a1 1 0 0 0-.9.6Z" />
    </svg>
  </button>
  </div>

  {#if $route === '/login'}
    <LoginPage />
  {:else}
    <div class="mx-auto max-w-6xl">
      <TopNav />
      {#if $route === '/dashboard'}
        <DashboardPage />
      {:else}
        <CollectionsPage />
      {/if}
    </div>
  {/if}
</main>

<Modal open={settingsOpen} title={copy.settings} onClose={requestCloseSettings}>
  <div class="space-y-3">
    <p class="text-xs font-semibold uppercase tracking-wide text-slate/70">{copy.cloudUrl}</p>
    <Input bind:value={settingsCloudBaseUrl} />

    <p class="text-xs font-semibold uppercase tracking-wide text-slate/70">{copy.cache}</p>
    <p class="rounded-xl border border-slate/15 bg-white/70 px-3 py-2 text-sm text-slate/80">
      {FIXED_SQLITE_CACHE_PATH} ({copy.fixed})
    </p>

    <div class="pt-2">
      <p class="text-xs font-semibold uppercase tracking-wide text-slate/70">
        {copy.closeBehavior}
      </p>
      <div class="mt-2 space-y-2 text-sm text-slate">
        <label class="flex cursor-pointer items-center gap-2">
          <input type="radio" name="closeBehavior" value="hide" bind:group={settingsCloseBehavior} />
          <span>{copy.hide}</span>
        </label>
        <label class="flex cursor-pointer items-center gap-2">
          <input
            type="radio"
            name="closeBehavior"
            value="minimize"
            bind:group={settingsCloseBehavior}
          />
          <span>{copy.minimize}</span>
        </label>
        <label class="flex cursor-pointer items-center gap-2">
          <input type="radio" name="closeBehavior" value="quit" bind:group={settingsCloseBehavior} />
          <span>{copy.quit}</span>
        </label>
      </div>
    </div>

    <label class="flex cursor-pointer items-center gap-2 pt-1 text-sm text-slate">
      <input type="checkbox" bind:checked={settingsShowTrayIcon} />
      <span>{copy.tray}</span>
    </label>

    {#if settingsSaveError}
      <p class="text-sm text-rose-700">{settingsSaveError}</p>
    {/if}

    <div class="pt-1">
      <Button on:click={saveSettings}>{copy.save}</Button>
    </div>
  </div>
</Modal>

<Modal open={discardChangesOpen} title={copy.unsaved} onClose={keepEditingSettings}>
  <div class="space-y-4">
    <p class="text-sm text-slate">
      {copy.unsavedBody}
    </p>
    <div class="flex gap-2">
      <Button variant="ghost" on:click={keepEditingSettings}>{copy.keep}</Button>
      <Button variant="secondary" on:click={discardSettingsChanges}>{copy.discard}</Button>
    </div>
  </div>
</Modal>
