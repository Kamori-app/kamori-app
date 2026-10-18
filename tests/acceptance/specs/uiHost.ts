import { spawnSync } from "node:child_process";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { readFileSync } from "node:fs";

// An ephemeral HTTP shell serves a freshly compiled production component tree.
export async function uiHost(kind: "desktop" | "admin" | "invite") {
  const compiled = spawnSync("bun", [new URL("../fixtures/uiGates.bundle.mjs", import.meta.url).pathname, kind], {
    encoding: "utf8", maxBuffer: 16 * 1024 * 1024,
  });
  if (compiled.status !== 0) throw new Error(compiled.stderr || compiled.stdout);
  const server = createServer((request, response) => {
    if (request.url === "/fixture.js") {
      response.writeHead(200, { "content-type": "text/javascript" }); response.end(compiled.stdout);
    } else if (request.url === "/crypto_core_lib_bg.wasm") {
      response.writeHead(200, { "content-type": "application/wasm" });
      response.end(readFileSync(new URL("../../../apps/web-frontend/src/lib/wasm/crypto-core-lib/crypto_core_lib_bg.wasm", import.meta.url)));
    } else {
      response.writeHead(200, { "content-type": "text/html" });
      response.end('<!doctype html><html><body><script type="module" src="/fixture.js"></script></body></html>');
    }
  });
  await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
  return {
    origin: `http://127.0.0.1:${(server.address() as AddressInfo).port}`,
    async close() {
      server.closeAllConnections();
      await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    },
  };
}
