### Spec Compliance

- ❌ Issues found: Task 4's account-switch isolation requirement is not satisfied for a queued operation that outlives its originating workspace/account. The committed-state reload at `apps/web-frontend/src/lib/components/app/AppWorkspace.svelte:194` runs against whichever vault is active when the lock becomes available, then executes the old operation. A focused Chromium reproduction placed Alice's task into Bob's durable materialization and outbox.
- ✅ Remaining scoped requirements are represented: actual child-process SIGKILL/reopen against SQLCipher; pending rotation identity and unsupported-token network checks; genuine shared-context Web Locks/cookies/IndexedDB; lost versus partial response generations; acknowledged offline edits across page close/reopen; and idempotent append retry. See `packages/crypto-core-lib/src/local_bridge_runner/durability_tests.rs:1`, `tests/acceptance/specs/browser-durability.spec.ts:1`, and the checks below.
- ⚠️ Broader preservation of all thirty prior fixes, deployed-image acceptance, native OS keychain behavior, whole-browser crash, and physical power-loss durability cannot be certified by this task diff. The report appropriately excludes those claims. Root Turbo database environment/cache follow-up remains separate under ruling 9.

### Strengths

- `apps/web-frontend/src/lib/components/app/AppWorkspace.svelte:194`: reloading all three durable structures inside the existing exclusive lock fixes the demonstrated same-account stale-tab overwrite without changing operation contents or dependency identities. The local promise tail and Web Lock remain intact.
- `packages/crypto-core-lib/src/local_bridge_runner/durability_tests.rs:1`: the parent waits for a flushed post-commit marker, kills the actual child process, checks SIGKILL on Unix, proves the file is encrypted, and reopens through production credential/cache APIs. The unsupported T0 phase verifies both unchanged request count and absence of queued TCP connections. The helper test is truthfully distinguished from the substantive scenario.
- `tests/acceptance/fixtures/browserDurability.js:1` and `tests/acceptance/fixtures/browserDurability.bundle.mjs:1`: test-only shell imports production AppWorkspace, vault, auth client, real stores and WASM. Browser storage and Web Locks are not replaced.
- `tests/acceptance/specs/browser-durability.spec.ts:1`: tests observe actual pending Web Locks, exact refresh identities/counts, actual HttpOnly cookie generations, retained/cleared durable attempts, operation IDs and state across new pages. The truncated-body peer distinguishes no-cookie loss from delivered-cookie partial response. Synthetic-server idempotency is not misrepresented as new server implementation coverage.
- `tests/acceptance/specs/browser-durability.spec.ts:274`: append acknowledgement recovery checks exact repeated client operation ID, one logical server record, empty outbox, one materialized operation, sequence/cursor 1 and no third append after another completed sync.

### Issues

#### Critical (Must Fix)

- None identified within this task scope.

#### Important (Should Fix)

1. **Queued operations survive account switch and write another account's content into the new account.** `apps/web-frontend/src/lib/components/app/AppWorkspace.svelte:194` loads the current global vault after an arbitrary lock wait, then invokes a captured operation without checking its originating lifetime. The unchanged caller at `AppWorkspace.svelte:906` subsequently uses active device/key/store APIs; `AppWorkspace.svelte:2096` stops scheduling on destruction but does not cancel already queued work. `apps/web-frontend/src/lib/cryptoVault.ts:772` resolves the current account scope, and the active scope is replaced by the normal lock/unlock sequence (`cryptoVault.ts:296`, `cryptoVault.ts:836`).

   **Observed schedule:** start Alice and finish hydration; hold the actual data-plane Web Lock in a second page; submit Alice's real task editor; observe one pending lock request; invoke the existing fixture's unmount/lock/unlock/mount sequence for Bob; release the lock; wait for Bob's initial sync to finish; inspect production vault; switch back to Alice. Bob contains Alice's literal title and one queued signed envelope; Alice contains no item or queue entry. This is durable account contamination, not merely stale UI.

   **Required fix:** bind admitted operations to their originating account/workspace lifetime and reject obsolete queued work before reloading or writing the new account. Preserve the existing coordination and operation dependency identity; do not introduce a per-session runtime architecture. Add an assertion-driven regression that switches while work is pending, and consider invalidation during the new asynchronous load as well as the lock wait. Existing account tests settle their saves before switching, so they miss this boundary. The pending-lifetime weakness predates part of this patch; the task explicitly promises account-switch isolation and the new reload has no guard for it.

#### Minor (Nice to Have)

1. **Verification output is not pristine.** `/private/tmp/kamori-readiness-20261015/durability-browser-final.log:5` reports NO_COLOR/FORCE_COLOR conflict; `/private/tmp/kamori-readiness-20261015/durability-web-build.log:127` and `:128` report empty generated chunks. These are disclosed non-failing environment/build messages, not evidence of a functional regression. Remove the conflicting color setting where controlled; retain explicit disclosure of the adapter messages if not actionable within this task.

### Checks and evidence

- Read the complete supplied 825-line `review-69e377d..084d054.diff` once in two non-overlapping ranges. Compared all six changed files to the task brief and report. No git rederivation, broad crawl, source/index/ref mutation or subagent delegation.
- **Named risk: changed lock-wrapper contract versus operation/account lifetime.** Focused checks examined only unchanged AppWorkspace callers, save/dependency capture and mount/destruction code around lines 860–1090, 1490–1580, 1630–1735 and 1990–2120. This is the permitted shared-state/call-site check, not a second review of the changed wrapper.
- **Named risk: active vault ownership across asynchronous refresh and account replacement.** Focused unchanged-code check inspected IndexedDB transaction helpers, vault activation/device access, materialization load/store and lock behavior in `cryptoVault.ts`. Reads do not capture a durable operation lifetime; current active account globals remain reachable from obsolete work.
- Read existing validation summary lines: RED browser 1 failed / 5 passed; GREEN 6 passed; final browser 7 passed; FRB 77 unit + 14 integration passed; Clippy finished; Svelte check 0 errors / 0 warnings. No package or browser suite was repeated. Other reported verification remains controller-owned evidence rather than a new reviewer run.
- **One focused unchanged-production-source browser reproduction**, authorized by controller after naming the lifetime risk. It reuses the task's exact in-memory fixture/server helpers and compiles the production browser bundle unchanged. Test registration is suppressed in memory so none of the seven existing cases run. Actual Chromium uses a fresh context and real Web Locks/storage; no repository test file is written. Reading the spec for execution is not a second changed-file review.
- Initial harness setup attempts did not execute product behavior: Bun cannot import the long data URL; a second attempt was stopped after sandbox loopback bind denial. The authorized escalated attempt executed the complete schedule and produced the observations below. Its final exit code was 1 solely because Bun's `closeAllConnections(); close()` teardown returned `ERR_SERVER_NOT_RUNNING` after all observations. This observation run is not presented as an assertion-driven RED test; the implementer should turn it into one with correct teardown.
- Reproduction tool output reference: exec chunk `0fb337`. Exact relevant output:

```text
BOUNDARY: Alice save queued behind actual Web Lock
BOUNDARY: Alice unmounted, Bob vault active and Bob component mounted
BOB_AFTER_RELEASE {"titles":["Alice queued before account switch"],"queue":[{"id":"227e5426-a332-4956-9b97-a6cbeed43417","device":"c7c1af27-b965-48de-ae90-5427b6a5dfdd","space":"00000000-0000-0000-0000-000000000041"}]}
ALICE_AFTER_RETURN {"titles":[],"queue":[]}
[teardown only] ERR_SERVER_NOT_RUNNING
```

### Assessment

**Task quality:** Needs fixes.

**Reasoning:** The durable refresh under lock and boundary tests are well targeted, with accurate network/generation and process-versus-power-loss limits. The observed cross-account queued-operation write blocks the account-isolation claim and must be fixed with a narrow lifetime guard and genuine pending-switch regression before task approval.

### Exact focused reproduction command

Working directory: `/Users/yujin/Development/kamori-app/kamori-app/kamori-app/tests/acceptance`. Requires authorized loopback listener and isolated Chromium launch. This command records the successful observation run, including its disclosed Bun teardown incompatibility; it writes no repository source.

```sh
PATH=/Users/yujin/.bun/install/cache/@oven/bun-darwin-aarch64@1.3.14@@@1/bin:$PATH bun - <<'JS'
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
const spec = '/Users/yujin/Development/kamori-app/kamori-app/kamori-app/tests/acceptance/specs/browser-durability.spec.ts';
let source = readFileSync(spec, 'utf8').replace(/^import .*from "@playwright\/test";/m, 'import { expect, chromium } from "@playwright/test"; let setup, reset, teardown; const test = Object.assign(() => {}, {beforeAll: f => setup=f, beforeEach: f => reset=f, afterAll: f => teardown=f, afterEach: () => {}});').replaceAll('import.meta.url', JSON.stringify(pathToFileURL(spec).href));
source += `
await setup(); reset();
const browser = await chromium.launch({headless: true});
try {
 const context = await browser.newContext(); const page = await context.newPage();
 await open(page); await start(page);
 const blocker = await context.newPage(); await open(blocker);
 await blocker.evaluate(() => { window.blockHeld = false; navigator.locks.request('kamori-web-data-plane-v1', async () => { window.blockHeld = true; await new Promise(resolve => window.releaseBlock = resolve); }); });
 await expect.poll(() => blocker.evaluate(() => window.blockHeld)).toBe(true);
 await page.getByRole('button', {name:'New task', exact:true}).click();
 const editor = page.getByRole('dialog', {name:'New task'});
 await editor.getByRole('textbox', {name:'Title'}).fill('Alice queued before account switch');
 await editor.getByRole('button', {name:'Save task'}).click();
 await expect.poll(() => blocker.evaluate(async () => (await navigator.locks.query()).pending?.length)).toBe(1);
 console.log('BOUNDARY: Alice save queued behind actual Web Lock');
 await page.evaluate(args => window.durability.start(args), {endpoint:origin+'/pim',account:'bob',space});
 console.log('BOUNDARY: Alice unmounted, Bob vault active and Bob component mounted');
 await blocker.evaluate(() => window.releaseBlock());
 await expect.poll(() => page.evaluate(() => window.durability.sync.phase)).toBe('error');
 const bob = await snapshot(page);
 console.log('BOB_AFTER_RELEASE', JSON.stringify({titles:bob.state.items.map(item=>item.title),queue:bob.queue.map(op=>({id:op.client_op_id,device:op.author_device_id,space:op.space_id}))}));
 await start(page, 'alice'); const alice = await snapshot(page);
 console.log('ALICE_AFTER_RETURN',JSON.stringify({titles:alice.state.items.map(item=>item.title),queue:alice.queue.map(op=>op.client_op_id)}));
} finally { await browser.close(); await teardown(); }
`;
const js = new Bun.Transpiler({loader:'ts',target:'bun'}).transformSync(source).replace(/^import \{([^}]+)\} from "([^"]+)";/gm, 'const {$1} = require("$2");');
await new (Object.getPrototypeOf(async function(){}).constructor)('require',js)(require);
JS
```
