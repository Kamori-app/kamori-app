import { expect, test } from '@playwright/test';
import { createCollection, openAppSection, signup, signIn, uniqueAccount } from './helpers';

// Real production router/SSR/service-worker, with the isolated acceptance backend.
// Snapshotting page.url instead of tracking it must break these assertions.
test('@smoke @full Kit navigation updates auth surfaces and locale URLs without reloading', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('/app/sign-in?space=one#anchor');
  await page.evaluate(() => { (window as any).__navigationMarker = 'same-document'; });
  await page.getByRole('button', { name: 'Create account', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Create account' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'RU', exact: true })).toHaveAttribute('href', '/app/sign-up?lang=ru');
  await page.goBack();
  await expect(page.getByRole('region', { name: 'Sign in' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'RU', exact: true })).toHaveAttribute('href', '/app/sign-in?space=one&lang=ru#anchor');
  const historyLength = await page.evaluate(() => history.length);
  await page.getByRole('link', { name: 'RU', exact: true }).click();
  await expect(page).toHaveURL(/space=one&lang=ru#anchor$/);
  await expect(page.getByRole('link', { name: 'EN', exact: true })).toHaveAttribute('href', '/app/sign-in?space=one&lang=en#anchor');
  expect(await page.evaluate(() => history.length)).toBe(historyLength);
  expect(await page.evaluate(() => (window as any).__navigationMarker)).toBe('same-document');
  expect(errors).toEqual([]);
});

test('@smoke @full Kit route state follows workspace, settings, history and sharing query', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  const account = await signup(page, uniqueAccount('kitroute'));
  await signIn(page, account);
  await page.evaluate(() => { (window as any).__navigationMarker = 'same-document'; });
  const first = await createCollection(page, 'First migration space');
  const second = await createCollection(page, 'Second migration space');
  for (const [section, button] of [['Tasks', 'New task'], ['Calendar', 'New event'], ['Contacts', 'New contact']] as const) {
    await openAppSection(page, section);
    await expect(page.getByRole('button', { name: button, exact: true })).toBeVisible();
  }
  await page.getByRole('link', { name: 'Settings', exact: true }).filter({ visible: true }).click();
  await expect(page.getByRole('navigation', { name: 'Settings', exact: true })).toBeVisible();
  await page.goBack();
  await expect(page.getByRole('button', { name: 'New contact', exact: true })).toBeVisible();
  await page.goForward();
  await expect(page.getByRole('navigation', { name: 'Settings', exact: true })).toBeVisible();
  for (const id of [first, second]) {
    await openAppSection(page, 'Spaces');
    await page.locator(`[data-space-id="${id}"]`).getByRole('link', { name: 'Members and access' }).click();
    await expect(page).toHaveURL(new RegExp(`/app/sharing\\?space=${id}$`));
    await expect(page.locator('select').filter({ has: page.locator(`option[value="${id}"]`) })).toHaveValue(id);
    await expect(page.getByRole('link', { name: 'RU', exact: true })).toHaveAttribute('href', `/app/sharing?space=${id}&lang=ru`);
  }
  expect(await page.evaluate(() => (window as any).__navigationMarker)).toBe('same-document');
  expect(errors).toEqual([]);
});

test('@smoke @full built module worker precaches assets, restores offline shell and cleans only old shell caches', async ({ page, context }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error' && !/401|Unauthorized/.test(message.text())) errors.push(message.text()); });
  await page.goto('/app');
  await page.evaluate(async () => {
    await navigator.serviceWorker.ready;
    if (!navigator.serviceWorker.controller) await new Promise<void>(resolve => navigator.serviceWorker.addEventListener('controllerchange', () => resolve(), { once: true }));
  });
  const shell = await page.evaluate(async () => {
    const name = (await caches.keys()).find(name => name.startsWith('kamori-shell-'))!;
    return { name, paths: (await (await caches.open(name)).keys()).map(request => new URL(request.url).pathname),
      script: navigator.serviceWorker.controller!.scriptURL };
  });
  await test.info().attach('worker-cache.json', { body: JSON.stringify(shell, null, 2), contentType: 'application/json' });
  expect(shell.script).toMatch(/\/service-worker\.js$/);
  expect(shell.paths.some(path => /_app\/immutable\/.*\.js$/.test(path))).toBe(true);
  expect(shell.paths.some(path => path.endsWith('.css'))).toBe(true);
  expect(shell.paths.some(path => path.endsWith('.woff2'))).toBe(true);
  expect(shell.paths.some(path => path.endsWith('.wasm'))).toBe(true);
  // Controlled navigation puts /app into the network-first navigation cache.
  await page.reload();
  await context.setOffline(true);
  await page.reload();
  await expect(page.getByRole('region', { name: 'Sign in' })).toBeVisible();
  await page.goto('/app/offline-unvisited');
  await expect(page.getByRole('region', { name: 'Sign in' })).toBeVisible();
  await page.getByRole('button', { name: 'Create account', exact: true }).click();
  await expect(page.getByRole('region', { name: 'Create account' })).toBeVisible();
  await context.setOffline(false);
  await page.goto('/app');
  // Reinstall the actual built worker to run its install/activate lifecycle with
  // stale version and unrelated caches present, without changing generated JS.
  await page.evaluate(async () => {
    await caches.open('kamori-shell-obsolete-version');
    await caches.open('unrelated-application-cache');
    // A distinct script URL creates a fresh worker installation even while the
    // current document remains controlled by the previous registration.
    await navigator.serviceWorker.register('/service-worker.js?migration-update=1', { type: 'module' });
  });
  await expect.poll(() => page.evaluate(() => caches.keys())).not.toContain('kamori-shell-obsolete-version');
  expect(await page.evaluate(() => caches.keys())).toContain('unrelated-application-cache');
  expect(await page.evaluate(() => caches.keys())).toContain(shell.name);
  // Offline reload can log expected failed network requests; CSP/JS failures cannot.
  expect(errors.filter(error => !/ERR_INTERNET_DISCONNECTED|Failed to fetch|net::ERR_FAILED/.test(error))).toEqual([]);
});
