<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import Card from '../components/ui/Card.svelte';
  import Button from '../components/ui/Button.svelte';
  import Badge from '../components/ui/Badge.svelte';
  import { api, type DavConnectionInfo } from '../tauri';
  import { session, loginNotice } from '../stores/app';

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
      loginNotice.set('Local DAV bridge started. Background encrypted sync is active.');
      await refresh();
    } catch (error) {
      operationError = errorMessage(error);
    }
  };

  const stopServer = async () => {
    operationError = '';
    try {
      await api.stopLocalServer();
      loginNotice.set('Local DAV bridge and background sync stopped.');
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
      loginNotice.set(`Sync complete: applied ${applied} encrypted operations.`);
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
      operationError = 'Stop the local bridge before rotating credentials.';
      return;
    }
    if (!window.confirm('Rotate the DAV password? Existing DAV clients will need the new password.')) {
      return;
    }
    try {
      davInfo = await api.rotateDavCredentials();
      loginNotice.set('DAV password rotated. Update every connected DAV client.');
    } catch (error) {
      operationError = errorMessage(error);
    }
  };

  const copyValue = async (value: string, label: string) => {
    try {
      await navigator.clipboard.writeText(value);
      loginNotice.set(`${label} copied.`);
    } catch {
      operationError = `Could not copy ${label.toLowerCase()}. Select it manually.`;
    }
  };

  onMount(refresh);
  onDestroy(hideDavSetup);
</script>

<section class="animate-fade-slide">
  <div class="mb-6 flex items-center justify-between">
    <div>
      <h1 class="font-heading text-3xl font-bold text-slate">Dashboard</h1>
      <p class="mt-1 text-sm text-slate/70">Monitor local bridge runtime and synchronization status.</p>
    </div>

    <Badge active={$session.serverRunning}>{$session.serverRunning ? 'Running' : 'Stopped'}</Badge>
  </div>

  <div class="grid gap-4 md:grid-cols-3">
    <Card>
      <p class="text-xs uppercase tracking-wide text-slate/60">Local Server</p>
      <p class="mt-2 text-2xl font-bold text-slate">{$session.serverRunning ? 'Running' : 'Stopped'}</p>
      <p class="text-sm text-slate/70">Port {$session.bindAddr}</p>
    </Card>

    <Card>
      <p class="text-xs uppercase tracking-wide text-slate/60">Collections</p>
      <p class="mt-2 text-2xl font-bold text-slate">{$session.collectionsTotal}</p>
      <p class="text-sm text-slate/70">Configured local vaults</p>
    </Card>

    <Card>
      <p class="text-xs uppercase tracking-wide text-slate/60">Synced Items</p>
      <p class="mt-2 text-2xl font-bold text-slate">{$session.syncedItemsTotal}</p>
      <p class="text-sm text-slate/70">Applied in local cache</p>
    </Card>
  </div>

  <div class="mt-6 flex flex-wrap gap-3">
    <Button disabled={$session.serverRunning} on:click={startServer}>Start DAV Bridge</Button>
    <Button variant="ghost" disabled={!$session.serverRunning} on:click={stopServer}>Stop DAV Bridge</Button>
    <Button variant="secondary" disabled={syncing || !$session.serverRunning} on:click={syncNow}>{syncing ? 'Syncing...' : 'Sync Now'}</Button>
    <Button variant="secondary" on:click={davInfo ? hideDavSetup : showDavSetup}>
      {davInfo ? 'Hide Setup Details' : 'Show DAV Setup'}
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
          <p class="text-lg font-semibold text-slate">DAV client setup</p>
          <p class="mt-1 text-sm text-slate/70">
            These details are shown for 60 seconds. The bridge accepts connections only from this computer.
          </p>
        </div>
        <Button variant="ghost" disabled={$session.serverRunning} on:click={rotateCredentials}>
          Rotate Password
        </Button>
      </div>

      <div class="mt-4 grid gap-3 md:grid-cols-2">
        <div class="rounded-xl border border-slate/15 bg-white/70 p-3">
          <p class="text-xs uppercase tracking-wide text-slate/60">Username</p>
          <code class="mt-1 block break-all text-sm">{davInfo.username}</code>
          <button class="mt-2 text-xs font-semibold text-teal-700" on:click={() => copyValue(davInfo?.username ?? '', 'Username')}>Copy</button>
        </div>
        <div class="rounded-xl border border-slate/15 bg-white/70 p-3">
          <p class="text-xs uppercase tracking-wide text-slate/60">Dedicated DAV password</p>
          <code class="mt-1 block break-all text-sm">{davInfo.password}</code>
          <button class="mt-2 text-xs font-semibold text-teal-700" on:click={() => copyValue(davInfo?.password ?? '', 'Password')}>Copy</button>
        </div>
      </div>

      {#if davInfo.collections.length === 0}
        <p class="mt-4 text-sm text-slate/70">Create a space before connecting a DAV client.</p>
      {:else}
        <div class="mt-4 space-y-4">
          {#each davInfo.collections as collection}
            <div class="rounded-xl border border-slate/15 bg-white/70 p-3">
              <p class="font-semibold text-slate">{collection.name}</p>
              <p class="mt-2 text-xs uppercase tracking-wide text-slate/60">Calendar URL</p>
              <code class="mt-1 block break-all text-xs">{collection.calendar_url}</code>
              <button class="mt-1 text-xs font-semibold text-teal-700" on:click={() => copyValue(collection.calendar_url, 'Calendar URL')}>Copy</button>
              <p class="mt-3 text-xs uppercase tracking-wide text-slate/60">Address book URL</p>
              <code class="mt-1 block break-all text-xs">{collection.address_book_url}</code>
              <button class="mt-1 text-xs font-semibold text-teal-700" on:click={() => copyValue(collection.address_book_url, 'Address book URL')}>Copy</button>
            </div>
          {/each}
        </div>
      {/if}

      <p class="mt-4 text-xs text-slate/60">
        Use the direct collection URL in a client that supports custom CalDAV/CardDAV URLs. Your Kamori account password is never used for DAV.
      </p>
    </Card>
    </div>
  {/if}

  {#if $loginNotice}
    <p class="mt-4 rounded-xl bg-sand/40 p-3 text-sm text-slate">{$loginNotice}</p>
  {/if}
</section>
