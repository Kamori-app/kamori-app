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
      cloudBaseUrl: settingsCloudBaseUrl.trim() || 'http://127.0.0.1:3000',
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
        ? 'Settings saved. Tray icon was enabled because close behavior is set to hide.'
        : 'Settings saved successfully.';

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
      settingsSaveError = message || 'Failed to save settings.';
    }
  };

  onMount(async () => {
    try {
      await api.configureBackend($backendSettings.cloudBaseUrl);
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
  <button
    class="fixed right-4 top-4 z-40 inline-flex h-11 w-11 items-center justify-center rounded-full border border-white/70 bg-white/85 text-slate shadow-panel transition hover:bg-white"
    on:click={openSettings}
    aria-label="Open backend settings"
    title="Backend settings"
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

<Modal open={settingsOpen} title="Desktop Settings" onClose={requestCloseSettings}>
  <div class="space-y-3">
    <p class="text-xs font-semibold uppercase tracking-wide text-slate/70">Cloud Base URL</p>
    <Input bind:value={settingsCloudBaseUrl} />

    <p class="text-xs font-semibold uppercase tracking-wide text-slate/70">SQLite Cache Path</p>
    <p class="rounded-xl border border-slate/15 bg-white/70 px-3 py-2 text-sm text-slate/80">
      {FIXED_SQLITE_CACHE_PATH} (fixed)
    </p>

    <div class="pt-2">
      <p class="text-xs font-semibold uppercase tracking-wide text-slate/70">
        Window Behavior On Close
      </p>
      <div class="mt-2 space-y-2 text-sm text-slate">
        <label class="flex cursor-pointer items-center gap-2">
          <input type="radio" name="closeBehavior" value="hide" bind:group={settingsCloseBehavior} />
          <span>Hide on close</span>
        </label>
        <label class="flex cursor-pointer items-center gap-2">
          <input
            type="radio"
            name="closeBehavior"
            value="minimize"
            bind:group={settingsCloseBehavior}
          />
          <span>Minimize on close</span>
        </label>
        <label class="flex cursor-pointer items-center gap-2">
          <input type="radio" name="closeBehavior" value="quit" bind:group={settingsCloseBehavior} />
          <span>Quit on close</span>
        </label>
      </div>
    </div>

    <label class="flex cursor-pointer items-center gap-2 pt-1 text-sm text-slate">
      <input type="checkbox" bind:checked={settingsShowTrayIcon} />
      <span>Show tray icon</span>
    </label>

    {#if settingsSaveError}
      <p class="text-sm text-rose-700">{settingsSaveError}</p>
    {/if}

    <div class="pt-1">
      <Button on:click={saveSettings}>Save Settings</Button>
    </div>
  </div>
</Modal>

<Modal open={discardChangesOpen} title="Unsaved Changes" onClose={keepEditingSettings}>
  <div class="space-y-4">
    <p class="text-sm text-slate">
      You have unsaved settings changes. If you close now, all unsaved changes will be lost.
    </p>
    <div class="flex gap-2">
      <Button variant="ghost" on:click={keepEditingSettings}>Keep Editing</Button>
      <Button variant="secondary" on:click={discardSettingsChanges}>Discard Changes</Button>
    </div>
  </div>
</Modal>
