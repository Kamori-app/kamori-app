import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, writeFileSync, mkdirSync, mkdtempSync, copyFileSync, renameSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

export const artifactDirectory = 'apps/web-frontend/src/lib/wasm/crypto-core-lib';
export const outputs = ['crypto_core_lib.d.ts', 'crypto_core_lib.js', 'crypto_core_lib_bg.wasm', 'crypto_core_lib_bg.wasm.d.ts'];
const buildArgs = ['build', '-p', 'crypto-core-lib', '--locked', '--release', '--target', 'wasm32-unknown-unknown', '--features', 'wasm', '--no-default-features'];
const guidance = 'Run bun run wasm:build from the repository root, then commit all generated files and manifest.';
const read = (root, path) => readFileSync(join(root, path), 'utf8');

function walk(root, path) {
  return readdirSync(join(root, path), { withFileTypes: true }).flatMap(entry => {
    const child = `${path}/${entry.name}`;
    return entry.isDirectory() ? walk(root, child) : [child];
  });
}

export function recipe(root) {
  const toolchain = read(root, 'rust-toolchain.toml').match(/^channel\s*=\s*"([^"]+)"/m)?.[1];
  const bindgen = read(root, 'Cargo.lock').split('[[package]]').find(section => /^name = "wasm-bindgen"$/m.test(section))?.match(/^version = "([^"]+)"/m)?.[1];
  if (!toolchain || !bindgen) throw new Error('Cannot resolve Rust toolchain or wasm-bindgen version');
  return { rust: toolchain, wasmBindgen: bindgen, profile: 'release', target: 'wasm32-unknown-unknown', cargoArgs: buildArgs, bindgenArgs: ['--target', 'web'] };
}

export function inventory(root, kind) {
  let paths;
  if (kind === 'inputs') {
    const members = read(root, 'Cargo.toml').match(/^members\s*=\s*\[([^\]]*)\]/m)?.[1];
    if (!members) throw new Error('Cannot resolve Cargo workspace members');
    const manifests = [...members.matchAll(/"([^"]+)"/g)].map(([, member]) => {
      // The current workspace uses explicit members. Fail closed on a future
      // glob rather than silently leaving a new manifest out of provenance.
      if (/[*?\[\]]/.test(member)) throw new Error(`Unsupported workspace member pattern: ${member}`);
      return `${member}/Cargo.toml`;
    });
    paths = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'scripts/web-crypto-wasm.mjs', ...manifests,
      ...walk(root, 'packages/crypto-core-lib/src').filter(path => path.endsWith('.rs'))];
    for (const optional of ['packages/crypto-core-lib/build.rs', '.cargo/config', '.cargo/config.toml']) {
      if (existsSync(join(root, optional))) paths.push(optional);
    }
  } else {
    paths = readdirSync(join(root, artifactDirectory)).filter(name => name.startsWith('crypto_core_lib')).map(name => `${artifactDirectory}/${name}`);
    for (const name of outputs) if (!paths.includes(`${artifactDirectory}/${name}`)) throw new Error(`Missing generated output: ${name}`);
  }
  return Object.fromEntries([...new Set(paths)].sort().map(path => [path, createHash('sha256').update(readFileSync(join(root, path))).digest('hex')]));
}

export function check(root) {
  try {
    const manifest = JSON.parse(read(root, `${artifactDirectory}/manifest.json`));
    if (manifest.schema !== 1) throw new Error('Unsupported manifest schema');
    if (JSON.stringify(manifest.recipe) !== JSON.stringify(recipe(root))) throw new Error('Build recipe changed');
    for (const kind of ['inputs', 'outputs']) {
      const actual = inventory(root, kind);
      const recorded = manifest[kind];
      if (!recorded || typeof recorded !== 'object' || Array.isArray(recorded)) throw new Error(`Invalid ${kind} inventory`);
      const changed = [...new Set([...Object.keys(actual), ...Object.keys(recorded)])].filter(path => actual[path] !== recorded[path]);
      if (changed.length) throw new Error(`Changed, added or missing ${kind}: ${changed.join(', ')}`);
    }
  } catch (error) {
    throw new Error(`Web crypto WASM is stale or unverifiable: ${error.message}. ${guidance}`);
  }
}

export function build(root) {
  const expected = recipe(root);
  for (const [command, version] of [['rustc', expected.rust], ['cargo', expected.rust], ['wasm-bindgen', expected.wasmBindgen]]) {
    const actual = execFileSync(command, ['--version'], { cwd: root, encoding: 'utf8' }).trim();
    if (actual.split(/\s+/)[1] !== version) throw new Error(`${command} must be ${version}; found ${actual}`);
    console.log(actual);
  }
  // Capture inputs before generation as well: concurrent source edits must not
  // bless an artifact built from a different source snapshot.
  const inputs = inventory(root, 'inputs');
  const staging = mkdtempSync(join(tmpdir(), 'kamori-wasm-build-'));
  try {
    execFileSync('cargo', buildArgs, { cwd: root, stdio: 'inherit' });
    const target = resolve(root, process.env.CARGO_TARGET_DIR || 'target');
    execFileSync('wasm-bindgen', ['--target', 'web', '--out-dir', staging, join(target, expected.target, 'release/crypto_core_lib.wasm')], { cwd: root, stdio: 'inherit' });
    if (JSON.stringify(inventory(root, 'inputs')) !== JSON.stringify(inputs)) throw new Error('Inputs changed during generation; rerun build');
    for (const name of outputs) {
      if (!existsSync(join(staging, name))) throw new Error(`Generation omitted ${name}; existing artifacts retained`);
    }
    const destination = join(root, artifactDirectory);
    mkdirSync(destination, { recursive: true });
    for (const name of outputs) copyFileSync(join(staging, name), join(destination, name));
    const manifest = { schema: 1, recipe: expected, inputs, outputs: inventory(root, 'outputs') };
    const temporary = join(destination, 'manifest.json.tmp');
    writeFileSync(temporary, `${JSON.stringify(manifest, null, 2)}\n`);
    renameSync(temporary, join(destination, 'manifest.json'));
    check(root);
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
    if (process.argv[2] === 'build') build(root);
    else if (process.argv[2] === 'check') check(root);
    else throw new Error('Usage: node scripts/web-crypto-wasm.mjs <check|build>');
    console.log(`Web crypto WASM ${process.argv[2]} passed`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
