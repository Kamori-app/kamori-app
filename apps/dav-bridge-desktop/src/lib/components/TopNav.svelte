<script lang="ts">
  import { navigate, route, type Route } from '../router';
  import Button from './ui/Button.svelte';
  import { api } from '../tauri';
  import { loginNotice } from '../stores/app';

  const go = (next: Route) => navigate(next);

  const logout = async () => {
    const result = await api.logout();
    loginNotice.set(result.warning ?? 'Logged out and revoked this server session.');
    navigate('/login');
  };
</script>

<nav class="mb-6 flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-white/70 bg-white/70 p-3 shadow-panel">
  <div class="flex items-center gap-2">
    <button class={`rounded-lg px-3 py-2 text-sm ${$route === '/dashboard' ? 'bg-slate text-white' : 'bg-transparent text-slate'}`} on:click={() => go('/dashboard')}>Dashboard</button>
    <button class={`rounded-lg px-3 py-2 text-sm ${$route === '/collections' ? 'bg-slate text-white' : 'bg-transparent text-slate'}`} on:click={() => go('/collections')}>Collections</button>
  </div>

  <div class="flex items-center gap-3">
    <a
      class="text-xs text-slate/70 underline underline-offset-2"
      href="https://github.com/Kamori-app/kamori-app"
      target="_blank"
      rel="noreferrer"
      title="AGPL-3.0-only corresponding source; no warranty"
    >Source &amp; license</a>
    <Button variant="ghost" on:click={logout}>Log Out</Button>
  </div>
</nav>
