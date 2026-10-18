import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { check, inventory, recipe, artifactDirectory, outputs } from './web-crypto-wasm.mjs';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'kamori-wasm-guard-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const put = (path, data) => { mkdirSync(dirname(join(root, path)), { recursive: true }); writeFileSync(join(root, path), data); };
  put('Cargo.toml', '[workspace]\nmembers = ["packages/crypto-core-lib", "apps/cloud-server"]\n');
  put('Cargo.lock', '[[package]]\nname = "wasm-bindgen"\nversion = "0.2.127"\n');
  put('rust-toolchain.toml', '[toolchain]\nchannel = "1.99.0"\n');
  put('packages/crypto-core-lib/Cargo.toml', '[package]\nname = "crypto-core-lib"\n');
  put('apps/cloud-server/Cargo.toml', '[package]\nname = "cloud-server"\n');
  put('packages/crypto-core-lib/src/lib.rs', 'pub fn encrypt() {}');
  put('packages/crypto-core-lib/src/nested/recovery.rs', 'pub fn recover() {}');
  put('scripts/web-crypto-wasm.mjs', 'fixture generator source');
  for (const name of outputs) put(`${artifactDirectory}/${name}`, `generated ${name}`);
  const manifest = `${artifactDirectory}/manifest.json`;
  put(manifest, JSON.stringify({ schema: 1, recipe: recipe(root), inputs: inventory(root, 'inputs'), outputs: inventory(root, 'outputs') }));
  return { root, put, manifest, remove: path => rmSync(join(root, path)), read: path => readFileSync(join(root, path), 'utf8') };
}

test('valid generated inventory passes without compiler or network', t => { check(fixture(t).root); });
for (const [name, mutate] of [
  ['source changed', f => f.put('packages/crypto-core-lib/src/lib.rs', 'changed')],
  ['nested source added', f => f.put('packages/crypto-core-lib/src/new/module.rs', 'added')],
  ['source deleted', f => f.remove('packages/crypto-core-lib/src/nested/recovery.rs')],
  ['workspace manifest changed', f => f.put('apps/cloud-server/Cargo.toml', 'changed')],
  ['workspace member added', f => { f.put('Cargo.toml', '[workspace]\nmembers = ["packages/crypto-core-lib", "apps/cloud-server", "apps/new"]'); f.put('apps/new/Cargo.toml', 'new'); }],
  ['lock changed', f => f.put('Cargo.lock', f.read('Cargo.lock') + '# changed')],
  ['toolchain changed', f => f.put('rust-toolchain.toml', '[toolchain]\nchannel = "1.100.0"')],
  ['generator changed', f => f.put('scripts/web-crypto-wasm.mjs', 'changed generator')],
  ['optional build script added', f => f.put('packages/crypto-core-lib/build.rs', 'fn main() {}')],
  ['cargo config added', f => f.put('.cargo/config.toml', '[build]\n')],
  ['output tampered', f => f.put(`${artifactDirectory}/crypto_core_lib_bg.wasm`, 'tampered')],
  ['output missing', f => f.remove(`${artifactDirectory}/crypto_core_lib.js`)],
  ['output added', f => f.put(`${artifactDirectory}/crypto_core_lib_unexpected.js`, 'extra')],
  ['manifest missing', f => f.remove(f.manifest)],
  ['manifest corrupt', f => f.put(f.manifest, '{not json')],
  ['manifest recipe tampered', f => { const m = JSON.parse(f.read(f.manifest)); m.recipe.profile = 'debug'; f.put(f.manifest, JSON.stringify(m)); }],
]) test(`rejects ${name} with regeneration guidance`, t => {
  const f = fixture(t); mutate(f);
  assert.throws(() => check(f.root), /bun run wasm:build/);
});
