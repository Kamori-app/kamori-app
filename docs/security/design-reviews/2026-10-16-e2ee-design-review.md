# E2EE design review — 2026-10-16

**Status: CHANGES REQUIRED before treating this draft as the approved implementation specification.**

Reviewed `/private/tmp/kamori-e2ee-20261016/design-draft.md` and the relevant identity/log/lifecycle supporting proposals. References below are to the draft's original line numbers. This is an architectural review, not a cryptographic proof or implementation assessment. No product files, commits, tests, services, or credentials were changed or exercised.

## Blocking findings

### B1. Membership proof is not enough without an activation position in the authenticated log

**Exact paragraphs:** A, line 44: “Owner-signed membership manifest фиксирует ... предыдущий manifest hash ...”; D, lines 76–78: append binds “hash проверенного membership state”, and the client checks “его роли в соответствующем membership state”.

These requirements permit an implementation to accept an authentic but obsolete membership manifest at a newer log position. A server can attach an old proof permitting a revoked writer to a new append. Chaining the membership manifests separately does not establish which manifest governs a particular operation. This is a missing security invariant, not a request for detailed wire encoding.

**Required correction:** Normatively order membership, device-authorization, epoch and snapshot transitions in the same authenticated log. A transition at position N must be authorized by the state immediately before N and activate the new state at N+1. Every normal record binds the membership head computed at its predecessor; clients must not accept a manifest chosen independently by the server. Rotation commits the prior log head, old/new membership heads, recipient/package commitments and snapshot manifest atomically. Retain historical verification keys and activation/revocation boundaries. Withheld transitions on an isolated branch retain the stated freshness limitation.

The supporting log proposal already contains this rule in its “Epoch membership and directory dependency” section; promote it into the normative draft. Add an explicit negative test: old authentic membership cannot authorize a post-revocation position even when all supplied signatures validate.

### B2. “Confirmed absence” and retry identity are underspecified across resealing and compaction

**Exact paragraphs:** C, lines 64–66: “согласовать ... по operation UUID и digest исходного envelope” and “Если отсутствие записи подтверждено в проверенном состоянии ... переупаковать ...”; D, line 76: “смена позиции ... не создаёт вторую PIM-операцию”; D, line 78 permits payload shortening while retaining commitments.

A receipt response saying `absent` is not an authenticated non-inclusion proof. A verified head only establishes a prefix, not global absence. After ciphertext is purged, an envelope hash alone does not provide the operation-ID inventory needed to resolve old lost acknowledgements. Multiple reseal attempts also produce multiple legitimate exact-envelope digests for one logical ID; retaining only the original digest cannot reconcile an accepted replacement after another crash/rotation.

**Required correction:** Define absence as absence through a particular verified head H, using either the retained authenticated operation-ID/digest inventory or an authenticated non-inclusion proof committed at H. Reconcile every locally persisted attempted envelope digest for that logical operation. Unknown/different bytes remain a conflict. Resealing requires a verified later epoch/control transition that makes the old attempt ineligible for a new append on that chain. Bind replacement append to H using CAS; if the head changes, verify the extension and reconcile before rebasing. Persist exact replacement bytes before upload and use a fresh nonce. Enforce and retain logical-ID uniqueness, inclusion receipts, and the authenticated ID/digest mapping across payload compaction; clients also reject duplicate logical IDs in a verified chain. Failure to obtain the required evidence leaves `awaiting-ack-reconciliation`; it is not permission to generate a new logical operation or assume absence. This guarantees idempotency on the verified branch, not globally across undetected forks.

### B3. Server generation enforcement is presented as a malicious-server guarantee

**Exact paragraphs:** Threat model, line 22 explicitly permits a malicious API/DB; E, line 96 says “конкурентный владелец не может после перехода опубликовать пакет для прежнего recovery key”; E, line 94 says “старый kit после commit больше не авторизует новый recovery”.

An isolated owner can receive an old authentic account identity and wrap a fresh space key to the compromised old recovery key. A malicious server can retain that uploaded ciphertext regardless of its transaction/generation checks. Rejecting the upload after encryption is not a confidentiality barrier. Similarly, a malicious server can ignore its recovery verifier or replay its old account database. Independent recovery authorization prevents a root-only attacker from satisfying an honest verifier; it does not force a malicious API to enforce authorization.

**Required correction:** Separate the guarantees explicitly. Server-side generation CAS, kit/session invalidation and transactional exclusion are honest-server enforcement guarantees. Client cryptographic checks reject identities older than their trusted pins and accept replacement roots only through the specified independent verification; encryption to a known obsolete identity must fail before upload. A previously isolated client that has not learned the root change cannot guarantee immediate revocation/future confidentiality. Mark foreign-space remediation complete only after the owner independently verifies the new root and commits the new recipient manifest/epoch, with participating clients adopting that checkpoint; account commit alone is insufficient. State that malicious withholding can maintain an old branch and cause further exposure there until trusted checkpoint/identity exchange. Old-kit holders cannot derive independently random new roots or new keys from old secrets alone; do not claim that malicious servers necessarily reject their API calls.

### B4. Root rotation preparation omits retained historical-key coverage

**Exact paragraph:** E, line 92: “До commit клиент получает все доступные текущие ключи ... меняются ... пакетный набор”. Compare migration, line 114: “Миграция не удаляет ... исторические keys”.

Replacing the account package set based on currently available current keys can strand retained old-epoch history, offline drafts, or packages temporarily omitted by the server. Keeping local files is not sufficient if their only usable decryption path is removed. The supporting lifecycle proposal requires all retained entitled historical packages and blocks on missing coverage; the authoritative draft lost that requirement.

**Required correction:** Prepare and validate coverage for all retained, entitled current and historical keys plus local draft/cache dependencies, using the client's known inventory and the server inventory under the stated CAS. Preserve verified historical access through new-root packages or an explicitly preserved historical-key archive protected by the new root/device-local key; never wrap the new root under the old one. Missing/corrupt known entries block finalization and identify repair needs; omission is not implicit abandonment. Verify each staged package/wrapper locally before commit. State the unavoidable limit: keys/history unknown to every trusted client cannot be proven complete from a malicious directory alone. Preserve the replacement-device bootstrap and retry/status capability across the commit that revokes old sessions/devices.

## Required implementation-plan clarifications (not additional architectural blockers)

- **Legacy purge boundary (D line 74, migration line 114):** The initial cursor-hardening stage must distinguish an existing legitimate history floor/current-state bootstrap from an unexpected gap within an established session. Identify a legacy floor as server-trusted until authenticated migration; never silently reset a pin to satisfy contiguity. V2 payload purge retains signed headers and requires an authenticated checkpoint authorizing the replaced prefix. The supporting log proposal already specifies this.
- **Multi-space lock/CAS participation (E line 96):** The lock order is sensible, but the implementation tasks must require every membership/package insertion/removal to bump the account inventory generation, including newly discovered spaces. Discover the inventory, acquire space locks in deterministic order, then account/device locks, and recheck inventory coverage; restart if it changed. No helper may acquire a space lock while holding the account lock.

## Bounded positive assessment

The draft correctly avoids treating TOFU as verified identity; separates pre-registration identity from the later server UUID routing binding; binds bootstrap to a locally supplied recipient commitment; uses an independent offline recovery-authorization seed; forbids old-root encryption of the new root and its resume artifacts; preserves revoked-author drafts; distinguishes password change from root replacement; and states that hidden tails, isolated forks, compromised web code and previously exposed plaintext remain outside its guarantees.

Subject to B1–B4, the approach is coherent as a client-verified extension of trusted anchors with authenticated membership and explicit recovery. This review does not establish that any cryptographic protocol or implementation is proven secure. Approval of the revised specification and later source/test review remain separate steps.

## Scoped re-review of revised draft — 2026-10-16

**Current verdict: APPROVED for the reviewed architectural scope. B1–B4 are closed; both requested clarifications are addressed.** This verdict supersedes the initial changes-required status for the revised draft; the original report above is retained as the review history. Only the four findings and two clarifications were re-reviewed, without broadening the audit.

- **B1 closed — A line 44, D lines 76–78:** Control transitions now share the authenticated log, activate at N+1 under prior-state authorization, and bind predecessor-computed membership. Rotation atomically binds log/membership heads, recipients/packages and snapshots; historical authorization boundaries are retained.
- **B2 closed — C line 66, D lines 76–78:** Absence is limited to verified H with authenticated UUID/digest evidence; all attempted-envelope digests are reconciled. Later-epoch eligibility, CAS/reconciliation on head changes, fresh nonce, persisted exact retries, retained authenticated inventory, and client/server logical-ID uniqueness are explicit. Missing evidence blocks resealing; undetected forks remain outside the guarantee.
- **B3 closed — E lines 94–96:** Server verifier/generation enforcement is explicitly conditional on a correct server. Pre-encryption rejection uses trusted client pins; isolated clients may still expose keys on a withheld old branch. Foreign-space remediation requires independently verified replacement identity and adoption of the new epoch/checkpoint. Old secrets do not derive fresh random roots, but malicious API enforcement is not promised.
- **B4 closed — E lines 92–94:** Preparation covers known retained/entitled historical keys and local dependencies, validates staged packages, blocks missing/corrupt known entries, and preserves historical access under new-root/device-local protection. Unknown hidden history is explicitly outside completeness claims. Replacement-device bootstrap and bound retry/status capability survive session revocation.
- **Legacy purge clarification closed — D line 78:** Legacy bootstrap floors are explicitly server-trusted, unexpected gaps cannot reset established pins, and v2 purge needs an authenticated replacement checkpoint with retained continuity/inventory.
- **Lock/inventory clarification closed — E line 96:** Deterministic space → account → device locking, inventory generation participation for membership/package insertions and removals including new spaces, and post-lock CAS/coverage checks with restart are explicit. Implementation must apply the same invariant to equivalent in-place inventory mutations and helpers.

Approval is bounded to the described design: verified continuation from trusted anchors, branch-scoped idempotency, and independently adopted identity transitions. It does not establish global freshness, immediate revocation for isolated clients, erasure of prior disclosure, correctness of executing web code, or implementation security. User approval of the specification remains a separate step before product implementation. No tests, product edits, commits, or service actions were performed; only this review artifact was appended.
