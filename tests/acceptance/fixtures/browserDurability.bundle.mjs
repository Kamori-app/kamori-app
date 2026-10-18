import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, resolve, extname } from "node:path";
const requireWeb = createRequire(new URL("../../../apps/web-frontend/package.json", import.meta.url));
const { compile } = requireWeb("svelte/compiler");
const fixtures = dirname(new URL(import.meta.url).pathname);
const lib = resolve(fixtures, "../../../apps/web-frontend/src/lib");
const bundle = await Bun.build({
  entrypoints: [resolve(fixtures, "browserDurability.js")], target: "browser", conditions: ["browser"],
  define: { "import.meta.env.VITE_KAMORI_API_BASE_URL": '"http://127.0.0.1:3000"' },
  plugins: [{ name: "browser-durability-fixture", setup(build) {
    build.onResolve({ filter: /^svelte\/store$/ }, () => ({
      path: resolve(dirname(requireWeb.resolve("svelte/package.json")), "src/store/index-client.js"),
    }));
    build.onResolve({ filter: /^svelte$/ }, () => ({
      path: resolve(dirname(requireWeb.resolve("svelte/package.json")), "src/index-client.js"),
    }));
    build.onResolve({ filter: /^(?:\$lib|#lib)\// }, ({ path }) => {
      let resolved = resolve(lib, path.slice(5));
      if (!extname(resolved)) resolved += ".ts";
      if (!existsSync(resolved) && resolved.endsWith(".js")) resolved = resolved.slice(0, -3) + ".ts";
      return { path: resolved };
    });
    build.onResolve({ filter: /^\$app\// }, ({ path }) => ({ path, namespace: "shell" }));
    build.onLoad({ filter: /.*/, namespace: "shell" }, ({ path }) => ({
      contents: ["$app/environment", "$app/env"].includes(path) ? "export const browser = true;" :
        path === "$app/state" ? "export const page = {url: new URL(location.href)};" :
        'import { readable } from "svelte/store"; export const page = readable({url: new URL(location.href)});',
      loader: "js", resolveDir: resolve(lib, ".."),
    }));
    build.onLoad({ filter: /\.svelte$/ }, async ({ path }) => ({
      contents: compile(await readFile(path, "utf8"), { filename: path, generate: "client" }).js.code,
      loader: "js", resolveDir: dirname(path),
    }));
  }}],
});
if (!bundle.success) throw new AggregateError(bundle.logs, "Durability fixture build failed");
process.stdout.write(await bundle.outputs[0].text());
