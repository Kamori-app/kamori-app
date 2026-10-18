import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync, chmodSync, realpathSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
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

for (const explicitTarget of [undefined, 'explicit-cache']) {
  test(`generator binds fresh Cargo output with configured target-dir (${explicitTarget ?? 'default'})`, t => {
    const f = fixture(t);
    f.put('scripts/web-crypto-wasm.mjs', readFileSync(new URL('./web-crypto-wasm.mjs', import.meta.url)));
    f.put('.cargo/config.toml', '[build]\ntarget-dir = "configured-cache"\n');
    const wasmRelativePath = 'wasm32-unknown-unknown/release/crypto_core_lib.wasm';
    f.put(`target/${wasmRelativePath}`, 'stale default artifact');
    f.put(`configured-cache/${wasmRelativePath}`, 'stale configured artifact');
    f.put('packages/crypto-core-lib/src/lib.rs', 'fresh source bytes');
    // Subprocess command fixtures model Cargo's output selection and bindgen's
    // input consumption, while executing the real canonical generator CLI.
    const command = String.raw`#!/usr/bin/env node
const fs = require('node:fs');
const path = require('node:path');
const name = path.basename(process.argv[1]);
const args = process.argv.slice(2);
if (args[0] === '--version') {
  console.log(name + ' ' + (name === 'wasm-bindgen' ? '0.2.127' : '1.99.0'));
} else if (name === 'cargo') {
  const configured = fs.readFileSync('.cargo/config.toml', 'utf8').match(/target-dir = "([^"]+)"/)[1];
  const target = path.resolve(args.includes('--target-dir') ? args[args.indexOf('--target-dir') + 1] : process.env.CARGO_TARGET_DIR || configured);
  const output = path.join(target, 'wasm32-unknown-unknown/release/crypto_core_lib.wasm');
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, fs.readFileSync('packages/crypto-core-lib/src/lib.rs'));
  fs.writeFileSync('cargo-invocation.json', JSON.stringify({ args, output }));
} else if (name === 'wasm-bindgen') {
  const input = args.at(-1);
  const out = args[args.indexOf('--out-dir') + 1];
  for (const file of ['crypto_core_lib.js', 'crypto_core_lib.d.ts', 'crypto_core_lib_bg.wasm', 'crypto_core_lib_bg.wasm.d.ts']) {
    fs.writeFileSync(path.join(out, file), file.endsWith('.wasm') ? fs.readFileSync(input) : 'generated companion');
  }
  fs.writeFileSync('bindgen-input.txt', input);
} else process.exit(1);
`;
    for (const name of ['rustc', 'cargo', 'wasm-bindgen']) {
      f.put(`commands/${name}`, command);
      chmodSync(join(f.root, 'commands', name), 0o755);
    }
    const env = { ...process.env, PATH: `${join(f.root, 'commands')}:${process.env.PATH}` };
    delete env.CARGO_TARGET_DIR;
    if (explicitTarget) env.CARGO_TARGET_DIR = explicitTarget;
    const result = spawnSync(process.execPath, ['scripts/web-crypto-wasm.mjs', 'build'], { cwd: f.root, env, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    check(f.root);
    assert.equal(f.read(`${artifactDirectory}/crypto_core_lib_bg.wasm`), 'fresh source bytes');
    const cargo = JSON.parse(f.read('cargo-invocation.json'));
    const canonical = join(f.root, explicitTarget ?? 'target');
    assert.equal(f.read('bindgen-input.txt'), cargo.output);
    assert.equal(realpathSync(cargo.output), realpathSync(join(canonical, wasmRelativePath)));
    assert.equal(f.read(`configured-cache/${wasmRelativePath}`), 'stale configured artifact');
    if (explicitTarget) assert.equal(f.read(`target/${wasmRelativePath}`), 'stale default artifact');
  });
}
