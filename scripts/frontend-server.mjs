// adapter-node 6's documented custom-server entrypoint. Keep the deployment's
// runtime ORIGIN contract without disabling SvelteKit's request-origin checks.
import http from 'node:http';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

let origin;
if (process.env.ORIGIN !== undefined) {
  try {
    if (!/^https?:\/\/[^/?#\\\s]+\/?$/i.test(process.env.ORIGIN)) throw new Error();
    origin = new URL(process.env.ORIGIN);
    if (!['http:', 'https:'].includes(origin.protocol) || origin.username || origin.password ||
      origin.pathname !== '/' || /[?#]/.test(process.env.ORIGIN)) throw new Error();
  } catch {
    throw new Error('Invalid ORIGIN: expected an http(s) origin without credentials, path, query or fragment');
  }
  // Captured by the adapter when handler.js is evaluated, before listen().
  process.env.PROTOCOL_HEADER = 'x-kamori-origin-protocol';
  process.env.HOST_HEADER = 'x-kamori-origin-host';
  process.env.PORT_HEADER = '';
}
const { handler } = await import(pathToFileURL(resolve('build/handler.js')).href);

function timeout(name, fallback) {
  const value = process.env[name];
  if (!value) return fallback;
  if (!/^\d+$/.test(value) || !Number.isSafeInteger(Number(value))) {
    throw new Error(`Invalid ${name}: expected a non-negative integer in seconds`);
  }
  return Number(value);
}
const shutdownTimeout = timeout('SHUTDOWN_TIMEOUT', 30);
let draining = false;
let shutdownTimer;
const server = http.createServer((request, response) => {
  if (origin) {
    // Always overwrite client-supplied values, including on the first request.
    // The actual Origin header, body, cookies and forwarded client IP stay intact.
    request.headers['x-kamori-origin-protocol'] = origin.protocol.slice(0, -1);
    request.headers['x-kamori-origin-host'] = origin.host;
  }
  request.on('close', () => {
    if (draining) server.closeIdleConnections();
  });
  handler(request, response, () => {
    response.statusCode = 404;
    response.end();
  });
});
for (const [variable, property] of [['KEEP_ALIVE_TIMEOUT', 'keepAliveTimeout'], ['HEADERS_TIMEOUT', 'headersTimeout']]) {
  const seconds = timeout(variable);
  if (seconds !== undefined) server[property] = seconds * 1000;
}
function shutdown(reason) {
  if (draining) return;
  draining = true;
  server.closeIdleConnections();
  shutdownTimer = setTimeout(() => server.closeAllConnections(), shutdownTimeout * 1000);
  server.close(error => {
    clearTimeout(shutdownTimer);
    if (!error) process.emit('sveltekit:shutdown', reason);
  });
}
process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);
const host = process.env.HOST ?? '0.0.0.0';
server.listen(Number(process.env.PORT ?? 3000), host, () => {
  const { address, port } = server.address();
  console.log(`Listening on http://${address.includes(':') ? `[${address}]` : address}:${port}`);
});
