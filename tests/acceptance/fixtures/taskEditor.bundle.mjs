import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";

const requireWeb = createRequire(new URL("../../../apps/web-frontend/package.json", import.meta.url));
const { compile } = requireWeb("svelte/compiler");
const fixtures = dirname(new URL(import.meta.url).pathname);
const lib = resolve(fixtures, "../../../apps/web-frontend/src/lib");
const bundle = await Bun.build({
  entrypoints: [resolve(fixtures, "taskEditor.js")],
  target: "browser",
  conditions: ["browser"],
  plugins: [{
    name: "task-editor-fixture",
    setup(build) {
      // The fixture lives in the acceptance workspace; resolve its mount function
      // from the installed web client's browser entry, alongside the real editor.
      build.onResolve({ filter: /^svelte$/ }, () => ({
        path: resolve(dirname(requireWeb.resolve("svelte/package.json")), "src/index-client.js"),
      }));
      build.onResolve({ filter: /^\$lib\// }, ({ path }) => ({
        path: resolve(lib, path.slice(5)) + (path.endsWith(".svelte") ? "" : ".ts"),
      }));
      build.onResolve({ filter: /^\$app\/environment$/ }, () => ({ path: "environment", namespace: "environment" }));
      build.onLoad({ filter: /.*/, namespace: "environment" }, () => ({ contents: "export const browser = false;", loader: "js" }));
      build.onLoad({ filter: /\.svelte$/ }, async ({ path }) => ({
        contents: compile(await readFile(path, "utf8"), { filename: path, generate: "client" }).js.code,
        loader: "js",
        resolveDir: dirname(path),
      }));
    },
  }],
});
if (!bundle.success) throw new AggregateError(bundle.logs, "Editor fixture build failed");
process.stdout.write(await bundle.outputs[0].text());
