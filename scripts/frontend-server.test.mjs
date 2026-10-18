// Run after building both frontends: node --test scripts/frontend-server.test.mjs
// These are real Node HTTP + built SvelteKit checks, not a handler mock.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, readdir, writeFile, symlink, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { once } from 'node:events';
import { request } from 'node:http';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
async function start(app, origin, extra = {}) {
  const child = spawn(process.execPath, [process.env.KAMORI_SERVER_ENTRY ?? resolve(root, 'scripts/frontend-server.mjs')], {
    cwd: resolve(root, 'apps', app),
    env: { ...process.env, HOST: '127.0.0.1', PORT: '0', ORIGIN: origin,
      SHUTDOWN_TIMEOUT: '1', ...extra }, stdio: ['ignore', 'pipe', 'pipe'],
  });
  let output = '';
  child.stdout.on('data', chunk => { output += chunk; });
  child.stderr.on('data', chunk => { output += chunk; });
  const exited = once(child, 'exit');
  const deadline = Date.now() + 15_000;
  while (!output.includes('Listening on ') && child.exitCode === null && Date.now() < deadline) {
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  const match = output.match(/Listening on http:\/\/127\.0\.0\.1:(\d+)/);
  return { child, output: () => output, port: match?.[1], exited, async stop(signal = 'SIGTERM') {
    if (child.exitCode === null) child.kill(signal);
    const timer = setTimeout(() => child.kill('SIGKILL'), 4000);
    const result = await exited; clearTimeout(timer);
    assert.deepEqual(result, [0, null], output);
  } };
}
function send(port, path, headers = {}, method = 'GET', body = '') {
  return new Promise((resolve, reject) => {
    const req = request({ hostname: '127.0.0.1', port, path, method, headers }, response => {
      const chunks = []; response.on('data', c => chunks.push(c));
      response.on('end', () => resolve({ status: response.statusCode, headers: response.headers,
        body: Buffer.concat(chunks).toString() }));
    });
    req.on('error', reject); req.end(body);
  });
}
for (const app of ['web-frontend', 'admin-frontend']) {
  for (const origin of ['https://canonical.kamori.test', 'http://localhost:19473']) {
    test(`${app} preserves runtime ${origin} despite hostile proxy headers`, async () => {
      const server = await start(app, origin);
      try {
        assert.ok(server.port, server.output());
        const path = app === 'web-frontend' ? '/app/sign-in?lang=ru&space=one' : '/';
        const headers = { host: 'attacker.invalid', 'x-forwarded-host': 'attacker.invalid',
          'x-forwarded-proto': 'https', 'x-forwarded-port': '444',
          'x-kamori-origin-host': 'attacker.invalid', 'x-kamori-origin-protocol': 'https' };
        const page = await send(server.port, path, headers);
        assert.equal(page.status, 200); assert.match(page.headers['content-security-policy'], /form-action 'self'/);
        if (app === 'web-frontend') {
          assert.match(page.body, /\/app\/sign-in\?lang=en&amp;space=one/);
          assert.match(page.body, /Sign in|Войти/);
        }
        const good = await send(server.port, path, { ...headers, origin,
          'content-type': 'application/x-www-form-urlencoded' }, 'POST', 'x=1');
        assert.equal(good.status, 405, good.body);
        const evil = await send(server.port, path, { ...headers, origin: 'https://attacker.invalid',
          'content-type': 'application/x-www-form-urlencoded' }, 'POST', 'x=1');
        assert.equal(evil.status, 403, evil.body);
      } finally { await server.stop(); }
    });
  }
  test(`${app} rejects malformed runtime origins before listening`, async () => {
    for (const origin of ['null', 'https://user:pass@example.com', 'https://example.com/path',
      'https://example.com?query', 'https://example.com#hash', 'ftp://example.com',
      'https:example.com', ' https://example.com']) {
      const server = await start(app, origin);
      if (server.port) { await server.stop(); assert.fail(`Accepted ${origin}`); }
      const [code] = await server.exited;
      assert.notEqual(code, 0); assert.match(server.output(), /Invalid ORIGIN/);
    }
  });
}

// A temporary real Kit endpoint consumes bodies and exposes client address so
// lifecycle/body-limit contracts can be tested without adding production routes.
test('custom launcher retains adapter body limits, proxy address and request draining', async () => {
  const fixture = await mkdtemp(resolve(tmpdir(), 'kamori-server-contract-'));
  try {
    await mkdir(resolve(fixture, 'node_modules'));
    const dependencies = resolve(root, 'apps/web-frontend/node_modules');
    for (const name of await readdir(dependencies)) {
      if (name !== '$app' && name !== '.bin') await symlink(resolve(dependencies, name), resolve(fixture, 'node_modules', name));
    }
    await mkdir(resolve(fixture, 'src/routes'), { recursive: true });
    await writeFile(resolve(fixture, 'package.json'), '{"type":"module"}');
    await writeFile(resolve(fixture, 'vite.config.js'), `
      import { sveltekit } from '@sveltejs/kit/vite';
      import adapter from '@sveltejs/adapter-node';
      export default { plugins: [sveltekit({ adapter: adapter() })] };
    `);
    await writeFile(resolve(fixture, 'src/app.html'), '<html><head>%sveltekit.head%</head><body><div>%sveltekit.body%</div></body></html>');
    await writeFile(resolve(fixture, 'src/routes/+server.js'), `
      process.on('sveltekit:shutdown', reason => console.log('shutdown:' + reason));
      export async function POST({ request }) { console.log("body-started"); return new Response(await request.text()); }
      export async function GET({ url, getClientAddress }) {
        console.log('request-started');
        await new Promise(resolve => setTimeout(resolve, Number(url.searchParams.get('delay') || 0)));
        return new Response(getClientAddress());
      }
    `);
    const build = spawnSync(process.execPath, [resolve(dependencies, 'vite/bin/vite.js'), 'build'], {
      cwd: fixture, encoding: 'utf8', env: process.env,
    });
    assert.equal(build.status, 0, build.stdout + build.stderr);
    for (const [limit, accepted, rejected] of [[undefined, 512 * 1024, 512 * 1024 + 1], ['1M', 600 * 1024, 1024 * 1024 + 1]]) {
      const server = await start(fixture, 'http://localhost:19473', { BODY_SIZE_LIMIT: limit });
      try {
        assert.ok(server.port, server.output());
        const headers = { 'content-type': 'application/octet-stream' };
        assert.equal((await send(server.port, '/', headers, 'POST', 'x'.repeat(accepted))).body.length, accepted);
        assert.equal((await send(server.port, '/', headers, 'POST', 'x'.repeat(rejected))).status, 413);
      } finally { await server.stop(); }
    }
    // No ORIGIN falls back to the existing explicit proxy contract.
    const proxy = await start(fixture, undefined, { PROTOCOL_HEADER: 'x-forwarded-proto', HOST_HEADER: 'x-forwarded-host',
      PORT_HEADER: 'x-forwarded-port', ADDRESS_HEADER: 'x-forwarded-for', XFF_DEPTH: '2', KEEP_ALIVE_TIMEOUT: '2', HEADERS_TIMEOUT: '3' });
    try {
      assert.ok(proxy.port, proxy.output());
      const headers = { 'x-forwarded-proto': 'http', 'x-forwarded-host': 'localhost', 'x-forwarded-port': '19473',
        'x-forwarded-for': '198.51.100.12, 192.0.2.2', origin: 'http://localhost:19473',
        'content-type': 'application/x-www-form-urlencoded' };
      assert.equal((await send(proxy.port, '/', headers, 'POST', 'x=1')).status, 200);
      const active = send(proxy.port, '/?delay=200', headers);
      const deadline = Date.now() + 5000;
      while (!proxy.output().includes('request-started') && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 10));
      assert.match(proxy.output(), /request-started/);
      const stopping = proxy.stop('SIGINT');
      const response = await active;
      assert.equal(response.status, 200); assert.equal(response.body, '198.51.100.12');
      await stopping; assert.match(proxy.output(), /shutdown:SIGINT/);
    } finally { if (proxy.child.exitCode === null) await proxy.stop(); }
    const stalled = await start(fixture, 'http://localhost:19473', { SHUTDOWN_TIMEOUT: '0' });
    try {
      assert.ok(stalled.port, stalled.output());
      const interrupted = new Promise(resolve => {
        const req = request({ hostname: '127.0.0.1', port: stalled.port, path: '/', method: 'POST',
          headers: { 'content-type': 'application/octet-stream', 'content-length': '100' } });
        req.on('error', error => resolve(error.code));
        req.write('incomplete');
      });
      const deadline = Date.now() + 5000;
      while (!stalled.output().includes('body-started') && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 10));
      assert.match(stalled.output(), /body-started/);
      const stopping = stalled.stop();
      assert.equal(await interrupted, 'ECONNRESET');
      await stopping; assert.match(stalled.output(), /shutdown:SIGTERM/);
    } finally { if (stalled.child.exitCode === null) await stalled.stop(); }

  } finally { await rm(fixture, { recursive: true, force: true }); }
});
