# Dependabot remediation and integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Execute tasks with independent scoped reviews and a final branch review.

**Goal:** Resolve the reviewed14 Dependabot PRs, merge all compatible requested updates and required migrations into main, close superseded/unsupported proposals, and verify finalmain CI.
**Architecture:** One visible integration branch from main0db47ae combines compatible updates before merge. Preserve existing product behavior and data formats; repair dependency API breaks rather than disabling checks. Keep supported Node24 instead of EOL25.
**Tech Stack:** Rust1.99.0, Bun1.4.3, Flutter3.47.0, Go1.26.6, SvelteKit3.1/adapter-node6, existing GitHubActions and Docker.
**Spec:** User explicitly requests autonomous fixes and merges until the reviewed queue is resolved. Binding product constraints are AGENTS.md. Exact reviewed snapshots and diagnoses are retained in /private/tmp/kamori-dependabot-review-20261018; primary tagged upstream migration contracts govern library API changes.

## Global Constraints

- Use visible fix/dependabot-integration in this existing checkout; no worktree, codex prefix, force push or history rewrite. Controller owns branch switches, integration, pushes, PR creation/merge/closure. Workers may commit scoped tasks, no worker subagents.
- User explicitly authorizes updating and merging the reviewed queue. One consolidated mainPR may supersede12compatible PRs; close original requests only after equivalent changes land. Close103/104 as unsupported Node25; retain Node24LTS.
- No production deployment, release publication, real-user keychain/data access or live cloud mutation. Use synthetic isolated fixtures; inspect actual mounts and clean exact owned containers/volumes, including anonymous image volumes; preserve user resources/caches.
- Preserve web-only registration, native sign-in, invite-code TTL15minutes..7days, MessagePack/byte fields, signed encrypted log, desktop loopbackDAV, offlinePIM and explicit per-collection mobile projection. Cachepaths remain .kamori/local-cache.sqlite3 and .kamori/mobile-cache.sqlite3.
- Preserve existing encrypted data, recovery kits, OPAQUE/KDF choices and transport/session/security semantics. Do not implement the separate E2EEtarget design. Original16E2EEdocs and normative digest1fa996f69ff98a22561e84dfe5948811e61935836d35c546f97762b67cf9baa7 stay unchanged.
- No disabled checks, weakened assertions, production mocks, new advisory ignores or blanket update ignores. Existing RustSec exception policy stays unchanged. Preserve CRLF PIM fixtures and source-qualified historical evidence.
- Active tooling pins must agree on actual Rust1.99.0/Bun1.4.3. Put selected binaries first in PATH; verify rustc/cargo/clippy/Bun versions, never silently use a different compiler. Reuse caches, do not alter global default toolchains. Logs/targets/artifacts under /private/tmp/kamori-dependabot-fix-20261018.
- Apply only reviewed dependency updates, necessary compatibility repairs and active documentation/configuration. Do not rewrite historical audit logs/version-qualified evidence. No unrelated refactoring.
- Bugfixes require meaningful RED/GREEN from existing/new behavior tests. Pure version/config changes use real install/build/audit verification rather than implementation-shaped tests.
- Final PR must target main, exact reviewed head, net intended delta, current CI/security green; no admin bypass. Follow post-merge currentmain checks to completion and leave visiblemain clean.

## Review Focus

- Tooling versions must be effective, including root overrides, Docker ENV and nested mobile adapters.
- Framework migration must retain SSR, origin/CSRF boundaries, serviceworker/offline cache and page/store reactivity.
- XML migration must preserve escaped text/entities/CDATA and DAV sync/multiget identity, not merely compile.
- Crypto dependency bumps must retain previously issued recovery phrases and existing OPAQUE/HPKE data contracts.
- Green historicCI does not establish combined-head compatibility; finalmatrix/nativepackages and production-image acceptance are required.

### Task 1: Integrate Actions, Flutter and coherent Android toolchain

**Files:** Five workflows from115; mobilepubspec.yaml/lock from128; Androidsettings.gradle.kts and wrapperproperties from4/5/8. Include this plan as taskcontext if updated.
**Inputs:** pr-115/128/4/5/8 JSON/diffs and mobile-review.md in reviewartifactdirectory. Exactsourceheads are in initial-snapshot.json.
**Produces:** Updated actionSHA pins and seven Flutter dependencies, Gradle9.7.0 + AGP9.3.1 + Kotlin2.4.10 together; preserve builtInKotlin/newDsl compatibility flags, matching realWorkmanager adapter pins and productionFRB.

- [x] Apply only relevant originalPRdeltas against currentmain, resolve context without reverting prioraudit fixes.
- [x] Verify actual nativeadapter/interface lock consistency and no dropped application dependency; preserve explicit testadapterregistration.
- [x] Run Flutter pub resolution/analyze/test, focused scheduling paths and Androidtoolchain compatibility check. Save exactcommands/versions/outputs; finalcombinedAndroid/iOS nativepackage gates run in CI.
- [x] Review changed action upstream contracts relevant to current inputs; do not weaken permissions/security/release controls. Record unexecuted release-only paths honestly.
- [x] Self-review, scopedcommit and taskreport. No push/branchswitch.

### Task 2: Upgrade Rust graph/compiler and repair DAV XML compatibility

**Files:** Original129 manifests/Cargo.lock; active Rusttoolchain pins/Dockerbuilder ENV/workflows; coreDAV parser/tests and recoverytests; minimal newcompiler formatting/lint repairs if proved necessary.
**Inputs:** pr-129/pr-118 diffs, rust-review.md and exact CIerrors; existing removedunmaintainedpath repair stays intact.
**Produces:** All20requested Rustupdates under actual1.99.0, coherentbuilder/CI/localpins, correct quick-xml0.42 usage and retained oldrecoverydata compatibility.

- [x] Apply reviewed129 update set; upgrade activecompiler pins from1.97.1 to1.99.0 including DockerFROM/ENV and rust-toolchain/workflow fields. Install/reuse explicit native toolchain components without changing global defaults.
- [x] Observe original quick-xml compile failure; add/strengthen meaningful DAV REPORT tests for decoded entities/escapedhrefs/CDATA before parserrepair. Preserve parser behavior using tagged0.42 APIs.
- [x] Add fixed old-version recovery phrase/entropy compatibility vector, keep OPAQUE using opaque_ke::argon2 and existing HPKE suite/bytes unchanged.
- [x] Resolve minimal actual1.99 fmt/Clippy/compiler errors, retaining generatedbridge contract; no blind broad edits. Run affected core/DAV/recovery/FRB and strictRustchecks, unchangedRustSecaudit. Fullworkspace/backenddatabase CI remains finalgate.
- [x] Self-review, scopedcommit/report with RED/GREEN and exact effectiveversions; no push/switch.

### Task 3: Upgrade Go infrastructure dependencies

**Files:** infra/go.mod/go.sum from105; only proven required Go API/test adjustments.
**Inputs:** pr-105.diff, infrastructure-workflows-review.txt. Keep PulumiCLI/backend contract unchanged unless concretecompatibility requires a coordinated repair.
**Produces:** Five requested SDK/crypto updates and Go1.26.6 with currentinfrastructure code/tests.

- [x] Apply intended go.mod/sum changes without removing localproviderreplace or priorliteral-secret/backupfailurerepairs.
- [x] Use actualGo1.26.6; verify module resolution, formatting/vet and complete localinfrastructure tests including existing mocks. No livecloud/productionstack operations.
- [x] Evaluate pinnedPulumiCLI3.259.0 compatibility from actual SDKcontract; versiondifference alone is not a defect. Exercise offline/mock behavior as appropriate, record lackoflivepreview without claimingproductionchanges.
- [x] Self-review, scopedcommit/report; no push/switch.

### Task 4: Migrate frontend dependencies and Bun baseline coherently

**Files:**119 JSmanifests/lock, web/admin Svelte/Vite/TSconfig/serviceworker/pageconsumer migration, root overrides, activeBun pins/AGENTS/Dockerfiles/workflows from124/125. Desktopfrontend/tests only as necessary compatibility boundary.
**Inputs:** javascript-review.md and forthcoming tagged-source migration notes javascript-migration-design.txt in fixartifactdirectory. These are concrete compatibility requirements, not approval to redesign productflows.
**Produces:** SvelteKit3.1/adapter6 and all requestedcompatible JSupdates, TypeScript6 JSAPI plusupdated7nativealias, effectiveVite8.3.4/PostCSS8.5.30, Bun1.4.3 everywhere active.

- [x] Reproduce original checker/Kitconfig errors with actualnewdependencies. Preserve TS6 installation required by checker and7alias; do not satisfy update by deleting checks.
- [x] Implement taggedKit3 flat plugin configuration, explicitlegacy$lib alias if retainingimports, correctpage-store/reactivity/tsconfig/serviceworker replacement. Preserve SSR/routing/offline output and authentication flows.
- [x] Preserve deployment/acceptance runtimeorigin and CSRF semantics under adapter6; use documented configuration contract and meaningfulactualHTTP/browser tests with differing configuredorigin and hostileOrigin negativecontrols.
- [x] Reconcile cookieoverride to compatibleaudited2.x; devalue at least5.9.3 and satisfyingKitrequiredrange; preserve source-map-js1.2.2. Make requested Vite/PostCSS versions effective. No arbitrary override removal or auditignore.
- [x] Upgrade bothfrontendBunbuilderimages, packageManager, activeCI/release/devinstructions to1.4.3. Retain productionNode24LTS. Installtask-localBun, frozeninstall and unchangedproductionaudit.
- [x] Run allthreefrontend checks/unit/mountedUI/builds, relevant serviceworker/origin/SSR and actualproductionimage/browser checks. Add meaningful regression tests only at changed behavior seams, retain oldoffline/auth tests.
- [x] Self-review, scopedcommit/report with effectiveversionmanifest and exactevidence; no push/switch.

### Task 5: Upgrade edge image and prevent EOL Node proposals

**Files:** deploy/edge/Dockerfile from117, narrow .github/dependabot.yml rules for Node25 only in web/admin dockerentries; focused existingedge acceptance controls if meaningful.
**Produces:** Caddy2.11.7 with validated config/proxy behavior; Node24 retained and unsupported25proposals excluded without blocking future supportedmajors/securityupdates.

- [x] Apply Caddy imagebump. Validate realnewimage with currentCaddyfile using synthetics, test meaningfulHTTP/header/WebSocket contract in isolatedownedfixture. No productionACME/DNS/networkmutation.
- [x] Verify officialDependabotversion-ignore syntax and narrowly exclude Node25.x only, preserving futureNodeLTSupdates and securityalerts. No broadmajorversionignore.
- [x] Inspect actualtaskcontainers/mounts and remove exact ownedresources using rm-v plus absencecheck; preserve preexistingvolumes/images/caches.
- [x] Self-review, scopedcommit/report; no push/switch/remotePRclosure.

### Task 6: Final integration verification and audit record

**Files:** docs/audits/2026-10-18-dependabot-remediation.md and thisplancheckboxes; sourcefixes only after provenintegrationfailure, via boundedreviewloop.
**Produces:** Complete14PRdisposition/effectiveversion map and exact current-source localverification record.

- [ ] Inspect finaldiff/root graph and coherentactivepins. Run appropriate complete Rustworkspace/FRB/fmt/strictClippy withsyntheticDB, Flutteranalyze/tests, frontend checks/tests/builds and bothunchangedsecurityaudits. Reuse completedresults only if actualrelevantinputs identical; document overlap and sourcebounds.
- [ ] Build freshfinal productionapplicationimages and execute full46acceptance suite with zero retry masking; record imageIDs and exact9deployedstack/37mountedcomponent populations. Restorefocusedcontrols if changedtooling affects them; do not claim historicalfullPITRrerun.
- [ ] Verify original16E2EEdocs unchanged; run existingstructuralchecker and pinnedstandaloneprobe if relevant to effectiveRustchange, no protocoleffectclaim.
- [ ] Write concise committedaudit linking14originalPRs, originalfailures/fixes, effectiveversions, actualcommands/outcomes and unexecutedphysical/release/providerbounds. Report CI publicationpending accurately.
- [ ] Self-review, commitreport/plan and taskreport for finalwholebranchreview.

## Controller integration

After everytaskreview gate, continue sequentially; parallelagents mayresearch read-onlyonly. Finalreview on baseline0db47ae..HEAD, mostcapablemodel; resolve findings in one scopedfixwave. PublishmainPR with explicitbase/main and original12PRsupersession map. Await actualfinalhead all9CIjobs and bothsecurityjobs, fix new failures untilgreen. Merge onlyexpectedreviewedhead; update localmain fastforward, verify content/effectivepins and actualpostmergeallmatrix/fullacceptance. Close original12 as superseded aftermerge and103/104 asunsupported, with reviewablePRbodyreason/link; confirm finalqueue state. No deployment. Archiveownedplanworkspace/evidence and finish cleanmain.
