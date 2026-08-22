<script lang="ts">
  import { navigate, route, type Route } from '../router';
  import Button from './ui/Button.svelte';
  import { api } from '../tauri';
  import { loginNotice } from '../stores/app';
  import { locale } from '../i18n';
  import BrandMark from './BrandMark.svelte';
  import LocaleSwitch from './LocaleSwitch.svelte';

  const copies = {
    en: { dashboard: 'Dashboard', collections: 'Spaces', source: 'License', logout: 'Log out', loggedOut: 'Logged out and revoked this server session.' },
    ru: { dashboard: 'Обзор', collections: 'Пространства', source: 'Лицензия', logout: 'Выйти', loggedOut: 'Вы вышли, серверная сессия отозвана.' },
  } as const;
  $: copy = copies[$locale];

  const go = (next: Route) => navigate(next);

  const logout = async () => {
    const result = await api.logout();
    loginNotice.set(result.warning ?? copy.loggedOut);
    navigate('/login');
  };
</script>

<nav class="mb-6 flex flex-wrap items-center justify-between gap-3 border-y border-slate/20 bg-paper/90 px-1 py-3">
  <div class="flex items-center gap-2">
    <BrandMark size={30} />
    <button class={`px-3 py-2 text-sm ${$route === '/dashboard' ? 'bg-slate text-white' : 'bg-transparent text-slate'}`} on:click={() => go('/dashboard')}>{copy.dashboard}</button>
    <button class={`px-3 py-2 text-sm ${$route === '/collections' ? 'bg-slate text-white' : 'bg-transparent text-slate'}`} on:click={() => go('/collections')}>{copy.collections}</button>
  </div>

  <div class="flex items-center gap-3">
    <a
      class="text-xs text-slate/70 underline underline-offset-2"
      href="https://github.com/Kamori-app/kamori-app/blob/main/LICENSE.md"
      target="_blank"
      rel="noreferrer"
      title="AGPL-3.0-only corresponding source; no warranty"
    >{copy.source}</a>
    <LocaleSwitch />
    <Button variant="ghost" on:click={logout}>{copy.logout}</Button>
  </div>
</nav>
