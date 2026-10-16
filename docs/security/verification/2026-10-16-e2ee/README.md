# E2EE document V&V evidence — 2026-10-16

This bundle accompanies the [validation and verification report](../../design-reviews/2026-10-16-e2ee-validation-verification.md). It contains a standalone diagnostic experiment and document checks, not a new Kamori protocol implementation.

## Reproduce the diagnostic experiment

Run from the repository root with the pinned Rust toolchain and dependencies already available in the Cargo cache:

```sh
CARGO_TARGET_DIR=/private/tmp/kamori-e2ee-vv-target rustup run 1.97.1 cargo test --locked --offline --manifest-path docs/security/verification/2026-10-16-e2ee/bootstrap-probe/Cargo.toml --lib -- --nocapture
rustup run 1.97.1 cargo fmt --manifest-path docs/security/verification/2026-10-16-e2ee/bootstrap-probe/Cargo.toml -- --check
python3 docs/security/verification/2026-10-16-e2ee/check_documents.py
git diff --check
```

The nested Cargo manifest declares its own workspace and pins `hpke = "=0.14.0"`; its lockfile is retained. No root workspace dependency or product source is changed. The target directory stays outside the repository. An empty offline cache needs a separate dependency download before the offline command can succeed.

The six tests exercise:

1. HPKE Base accepts an arbitrary sender's chosen root at the genuine recipient.
2. A matching out-of-band PSK permits the honest return.
3. A mandatory PSK receiver rejects a public-key-only replacement.
4. A wrong PSK fails authentication.
5. Changed public transcript/AAD fails authentication.
6. The library permits an empty PSK bundle, so the application must enforce the specified length and entropy itself.

The constant test PSK and payloads are synthetic fixtures. They are not a source of production randomness. Tests 1 and 6 intentionally demonstrate permissive primitive behavior; the suite does not claim that production code already prevents it. Correct `setup_receiver` construction with a mismatched mode succeeds until authenticated `open` rejects the ciphertext.

The experiment does not import Kamori code. It does not verify UI fragment handling, pairing expiry, secret erasure, pending-state persistence, account/root verification, replay handling, recovery, log integrity, storage, or the hostile-API integration scenario P15. It is neither a protocol proof nor RED/GREEN evidence of a product fix.

## Recorded evidence

- [Diagnostic test output](bootstrap-probe.log): six passed, with the exact command and toolchain.
- [Document check output](doc-check.log): local file links, UTF-8/whitespace, P01–P16 and the normative document digest.
- [Check script](check_documents.py): read-only Python standard-library checks, runnable from any directory.

Reviewer reports retain the initial findings and subsequent scoped closure. Their line references and temporary draft paths identify the reviewed snapshots; the normative repository document is authoritative. The main specification's digest in `doc-check.log` identifies the final saved document.
