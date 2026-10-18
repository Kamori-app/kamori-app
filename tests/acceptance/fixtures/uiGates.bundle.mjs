import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, resolve, extname } from "node:path";
const fixtures = dirname(new URL(import.meta.url).pathname);
const kind = process.argv[2];
const app = kind === "desktop" ? "dav-bridge-desktop" : kind === "admin" ? "admin-frontend" : "web-frontend";
const requireApp = createRequire(new URL(`../../../apps/${app}/package.json`, import.meta.url));
const { compile } = requireApp("svelte/compiler");
const lib = resolve(fixtures, `../../../apps/${app}/src/lib`);
const bundle = await Bun.build({
  entrypoints: [resolve(fixtures, `${kind}Ui.js`)], target: "browser", conditions: ["browser"],
  define: { "import.meta.env.VITE_KAMORI_API_BASE_URL": '"/"' },
  plugins: [{ name: "mounted-ui-gates", setup(build) {
    for (const [name, entry] of [["svelte", "src/index-client.js"], ["svelte/store", "src/store/index-client.js"]]) {
      build.onResolve({ filter: new RegExp(`^${name}$`) }, () => ({
        path: resolve(dirname(requireApp.resolve("svelte/package.json")), entry),
      }));
    }
    build.onResolve({ filter: /^@tauri-apps\/api\// }, ({ path }) => ({ path: requireApp.resolve(path) }));
    build.onResolve({ filter: /^\$lib\// }, ({ path }) => {
      let resolved = resolve(lib, path.slice(5));
      if (!extname(resolved)) resolved += ".ts";
      if (!existsSync(resolved) && resolved.endsWith(".js")) resolved = resolved.slice(0, -3) + ".ts";
      return { path: resolved };
    });
    build.onResolve({ filter: /^\$app\// }, ({ path }) => ({ path, namespace: "shell" }));
    build.onLoad({ filter: /.*/, namespace: "shell" }, ({ path }) => ({
      contents: path === "$app/environment" ? "export const browser = true;" :
        'import { readable } from "svelte/store"; export const page = readable({url: new URL(location.href)});',
      loader: "js", resolveDir: resolve(lib, ".."),
    }));
    build.onLoad({ filter: /\.svelte$/ }, async ({ path }) => ({
      contents: compile(await readFile(path, "utf8"), { filename: path, generate: "client" }).js.code,
      loader: "js", resolveDir: dirname(path),
    }));
  }}],
});
if (!bundle.success) throw new AggregateError(bundle.logs, "UI fixture build failed");
process.stdout.write(await bundle.outputs[0].text());
