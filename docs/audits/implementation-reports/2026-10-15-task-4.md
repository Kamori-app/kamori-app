# Task 4 report — desktop decode, recovery, startup refresh

Base: `78100e5a955a4dc6ecc63b5defe2dd926810c60a`.
Worktree: `/Users/yujin/.codex/worktrees/d868/kamori-app`.
Scope: C02/C03/C04 only; no production, pushes, database, private PKI, or real OS keychain access. Synthetic SQLCipher files and loopback HTTP fixtures only. No dependencies or lockfiles changed. Existing public runner method signatures and FRB bindings remain unchanged.

## Implemented behavior

- C02: direct HTTP and runner transport now share `decode_msgpack_bytes`, using human-readable MessagePack semantics. Real nonempty spaces responses retain UUID strings and binary payload bytes. Extracted the existing network/crypto reconciliation body so its production flow can run with a synthetic authorized device instead of invoking keychain APIs.
- C03: collection creation and reconciliation share HPKE `EncryptedGroupKey` wrapping addressed to the deterministic account recovery identity. A second device with only the account master/recovery identity decrypts the exact key. Tests assert version 1, 32-byte encapsulated key, ciphertext, and rejection with another account identity. Legacy malformed recovery data is rewritten only when the authorized device's current-epoch device package yields the actual key. A device package for an old epoch cannot repair the unavailable current key.
- C04: a narrow shared `RefreshCredentialStore` uses the same encrypted cache operations as the active runtime. Startup validates its stored account master-key provenance, then opens this store before refresh. New desktop credential state is explicitly attributed to normalized endpoint and account. Legacy state is adopted only with an exact trusted current/immediate-predecessor/pending-token anchor; ambiguous rows and explicit endpoint/account mismatches fail before network.
- Successful password and browser signin call the same fresh-session initializer. It atomically replaces only credential/scope/attempt rows, binds the newly authenticated account/endpoint, and retains all PIM data and cursors.
- Unattributed FRB/mobile caches keep their existing lifecycle. The desktop guard does not reclassify a fresh mobile login or backend switch. A runner opening an already attributed desktop cache preserves and validates its attribution.

## Accepted rulings and refresh state machine

The controller explicitly approved narrowly anchored legacy adoption using the endpoint-scoped keychain token plus matching stored account/vault identity as provenance; never claim a conflicting endpoint or unrelated token. The controller also approved fail-closed behavior for tokens older than the immediate predecessor, with an explicit atomic reset after fresh signin.

1. Read the trusted endpoint keychain credential and account, then validate the account vault identity.
2. Consult scoped encrypted cache. T0 with pending R resumes R. T0 with committed T1 selects T1. Unknown older T0 after T0→T1→T2 fails closed before any refresh.
3. Durably begin/resume the selected token's cache attempt. If no cache generation exists, adopt its keychain attempt K rather than inventing another retry id.
4. Write the selected token and that same attempt to the keychain **before** sending refresh. Thus a recovered T1 becomes the OS anchor before any T1→T2 rotation; OS-write failure prevents network.
5. Send refresh, validate returned account, then commit replacement credentials to SQLCipher before provisioning or the final keychain update. A dropped response retains the pending attempt. A crash after replacement commit leaves its immediately preceding OS anchor available.
6. Repeated crashes/lost responses repeat the same persisted attempt, preserving the replacement and unrelated sessions. Fresh signin can deliberately reset an obsolete lineage without deleting the PIM cache.

The original active runtime's immediate-predecessor recovery is deliberately not expanded into an unbounded lineage archive. After 2+ runtime rotations with a stale OS token beyond that bound, startup requests a fresh signin instead of trying the unknown old token and revoking the account's sessions.

## RED evidence

All commands ran from the worktree above. Each bug's regression failed before its fix, not just at compile time.

### C02 + C03

Command (exit 101):

```sh
PATH=/Users/yujin/.cargo/bin:$PATH CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197 rustup run 1.97.1 cargo test -p dav-bridge-desktop --lib commands:: -- --nocapture
```

Failure output:

```text
running 10 tests
thread 'commands::collections::tests::desktop_recovery_package_unwraps_on_second_device_with_account_identity_only' panicked:
HPKE recovery package schema: Syntax("missing field `version`")
test commands::collections::tests::desktop_recovery_package_unwraps_on_second_device_with_account_identity_only ... FAILED
thread 'commands::auth::tests::nonempty_cloud_spaces_decode_in_direct_and_runner_paths' panicked:
cloud UUID strings and MessagePack binary fields: "invalid type: string \"4757cc29-19a9-44b4-9923-ba37fd594ed6\", expected a 16 byte array"
test commands::auth::tests::nonempty_cloud_spaces_decode_in_direct_and_runner_paths ... FAILED
failures:
    commands::auth::tests::nonempty_cloud_spaces_decode_in_direct_and_runner_paths
    commands::collections::tests::desktop_recovery_package_unwraps_on_second_device_with_account_identity_only
test result: FAILED. 8 passed; 2 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.02s
```

### C04

Added the production startup orchestration seam initially preserving its broken keychain-only selection, with fake OS/network boundaries and real encrypted cache state. Command (exit 101):

```sh
PATH=/Users/yujin/.cargo/bin:$PATH CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197 rustup run 1.97.1 cargo test -p dav-bridge-desktop --lib startup_refresh_preserves -- --nocapture
```

Failure output:

```text
running 1 test
thread 'commands::auth::tests::startup_refresh_preserves_exact_retry_and_unrelated_sessions_at_crash_boundaries' panicked:
response-lost: startup must recover runtime credentials: Some("refresh_token_reuse")
test commands::auth::tests::startup_refresh_preserves_exact_retry_and_unrelated_sessions_at_crash_boundaries ... FAILED
failures:
    commands::auth::tests::startup_refresh_preserves_exact_retry_and_unrelated_sessions_at_crash_boundaries
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 11 filtered out; finished in 0.26s
```

Routine intermediate compile fixes: async closures needed `move`; the cursor accessor in a new test is `load_last_seq`, not `last_seq`. These compile errors were corrected and are not counted as RED business-logic evidence.

## GREEN verification

Commands, all exit 0:

```sh
PATH=/Users/yujin/.cargo/bin:$PATH CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197 rustup run 1.97.1 cargo test -p dav-bridge-desktop
PATH=/Users/yujin/.cargo/bin:$PATH CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197 rustup run 1.97.1 cargo test -p crypto-core-lib --features frb
PATH=/Users/yujin/.cargo/bin:$PATH CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197 rustup run 1.97.1 cargo test -p dav-bridge-desktop -p crypto-core-lib --features crypto-core-lib/frb
PATH=/Users/yujin/.cargo/bin:$PATH CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197 rustup run 1.97.1 cargo clippy -p dav-bridge-desktop -p crypto-core-lib --features crypto-core-lib/frb --all-targets -- -D warnings
PATH=/Users/yujin/.cargo/bin:$PATH CARGO_TARGET_DIR=/private/tmp/kamori-audit-target-197 rustup run 1.97.1 cargo fmt --all -- --check
git diff --check
```

Final combined test output is reproduced below in full (build output included). Final counts: 65 core/FRB unit tests, 14 DAV integration tests, 18 desktop unit tests; no failures, ignores or filtered tests. Clippy's final output: `Checking dav-bridge-desktop ... Finished dev profile ... in 0.96s`; formatting and diff checks produced no output.

```text
   Compiling dav-bridge-desktop v0.1.0 (/Users/yujin/.codex/worktrees/d868/kamori-app/apps/dav-bridge-desktop/src-tauri)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.46s
     Running unittests src/lib.rs (/private/tmp/kamori-audit-target-197/debug/deps/crypto_core_lib-4009b496715df6d1)

running 65 tests
test frb_api::crypto_exports::tests::exported_crypto_errors_do_not_panic_the_mobile_runtime ... ok
test account_keys::tests::opaque_export_key_wrap_roundtrips ... ok
test frb_api::transport::tls_tests::mobile_factory_verifies_synthetic_ca_and_hostname ... ok
test local_bridge_runner::cache::tests::advancing_sync_cursor_never_moves_it_backwards ... ok
test local_bridge_runner::cache::tests::operation_state_and_resource_head_are_committed_together ... ok
test local_bridge_runner::cache::tests::authoritative_upsert_replaces_unconfirmed_wall_clock_version ... ok
test local_bridge_runner::cache::tests::lww_prefers_newer_payload ... ok
test local_bridge_runner::cache::tests::dav_change_journal_records_updates_and_tombstones_atomically ... ok
test local_bridge_runner::cache::tests::graph_reconciliation_does_not_emit_noop_dav_changes ... ok
test local_bridge_runner::cache::tests::operation_acknowledgement_preserves_highest_server_sequence ... ok
test local_bridge_runner::cache::tests::legacy_operation_state_key_is_migrated_to_space_scope ... ok
test local_bridge_runner::dav::tests::if_match_parser_uses_strong_comparison_and_supports_lists ... ok
test local_bridge_runner::cloud::tls_tests::cloud_factory_verifies_synthetic_ca_and_hostname ... ok
test local_bridge_runner::dav::tests::parses_discovery_and_resource_paths ... ok
test local_bridge_runner::dav::tests::rejects_doctype_and_oversized_reports ... ok
test local_bridge_runner::dav::tests::rejects_invalid_encoded_path_segments ... ok
test local_bridge_runner::dav::tests::report_parser_is_namespace_prefix_independent ... ok
test local_bridge_runner::dav::tests::resource_hrefs_round_trip_reserved_and_unicode_characters ... ok
test local_bridge_runner::dav::tests::sync_tokens_are_bound_to_collection_and_kind ... ok
test local_bridge_runner::dav::tests::limits_multiget_fanout ... ok
test local_bridge_runner::first_party_pim_tests::cloud_url_is_a_secure_path_free_origin ... ok
test local_bridge_runner::cache::tests::legacy_outbox_is_preserved_and_scoped_per_space ... ok
test local_bridge_runner::first_party_pim_tests::partial_task_edit_preserves_existing_and_extension_properties ... ok
test native_http::tests::android_bundled_roots_build_without_jvm_and_reject_private_ca ... ok
test native_http::tests::shared_native_factory_verifies_synthetic_ca_and_hostname ... ok
test operation_envelope::tests::canonical_bytes_are_independent_of_signature ... ok
test operation_envelope::tests::sealed_envelope_authenticates_public_context ... ok
test local_bridge_runner::cache::tests::outbox_preserves_insertion_order_when_timestamps_match ... ok
test operation_envelope::tests::signature_covers_every_client_field_and_ciphertext ... ok
test pim::tests::branch_assignment_rejects_incomplete_or_cyclic_graphs ... ok
test pim::tests::branch_assignment_is_arrival_order_independent ... ok
test pim::tests::contact_edit_preserves_parameters_groups_folding_and_unknown_properties ... ok
test pim::tests::contact_edit_preserves_grouped_parameters_and_address_components ... ok
test pim::tests::delete_preserves_adapter_resource_identity ... ok
test pim::tests::event_edit_changes_master_without_overwriting_recurrence_exception ... ok
test pim::tests::property_reader_unfolds_and_accepts_parameters ... ok
test pim::tests::field_operation_msgpack_roundtrip_is_stable ... ok
test pim::tests::rejects_multi_parent_v1_operation ... ok
test pim::tests::rejects_non_utf8_raw_projection ... ok
test pim::tests::rich_contact_replaces_managed_values_but_preserves_unknown_properties ... ok
test pim::tests::snapshot_msgpack_roundtrip_is_stable ... ok
test pim::tests::rich_event_roundtrips_typed_time_recurrence_and_managed_alarm ... ok
test pim::tests::snapshot_preserves_all_conflict_branches ... ok
test pim::tests::snapshot_rejects_duplicate_branch_identity ... ok
test pim::tests::v2_validation_rejects_injection_and_impossible_dates ... ok
test pim::tests::task_reminders_are_relative_to_due_time ... ok
test pim::tests::validator_rejects_mismatched_or_missing_uids ... ok
test recovery::tests::recovery_kit_rejects_short_or_invalid_phrases ... ok
test recovery::tests::recovery_kit_roundtrips_master_key ... ok
test secret_vault::tests::secret_vault_rejects_wrong_key ... ok
test tests::aes_gcm_roundtrip ... ok
test tests::account_recovery_identity_is_deterministic_and_unwraps_space_keys ... ok
test tests::group_key_wrap_unwrap ... ok
test tests::device_bootstrap_is_bound_to_recipient_and_authorization_flow ... ok
test tests::xchacha_roundtrip ... ok
test local_bridge_runner::cache::tests::plaintext_cache_is_rejected_without_creating_a_backup_copy ... ok
test local_bridge_runner::cache::tests::sync_cursor_is_scoped_per_workspace ... ok
test local_bridge_runner::cache::tests::quarantined_envelope_is_deduplicated_per_space_and_operation ... ok
test local_bridge_runner::first_party_pim_tests::first_party_write_is_materialized_and_queued_offline ... ok
test local_bridge_runner::dav_identity_tests::conditional_http_conflict_edits_and_deletes_replay_in_the_original_stream ... ok
test local_bridge_runner::cache::tests::refresh_rotation_attempt_is_random_stable_and_token_scoped ... ok
test local_bridge_runner::cache::tests::rotated_credentials_recover_only_from_the_current_or_previous_token ... ok
test local_bridge_runner::cache::tests::unattributed_frb_runtime_keeps_existing_fresh_login_and_endpoint_switch_behavior ... ok
test local_bridge_runner::cache::tests::fresh_session_replaces_credentials_atomically_and_preserves_pim_and_cursors ... ok
test local_bridge_runner::cache::tests::legacy_refresh_scope_requires_exact_anchor_and_never_overwrites_endpoint ... ok

test result: ok. 65 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.99s

     Running tests/dav_conformance.rs (/private/tmp/kamori-audit-target-197/debug/deps/dav_conformance-12a9f0caa21ce799)

running 14 tests
test discovers_principal_homes_and_registered_collections ... ok
test enforces_loopback_authentication_and_supported_methods ... ok
test projects_carddav_resources_and_reports ... ok
test rejects_unselected_sibling_missing_parent_on_put_without_persistence ... ok
test rejects_unselected_sibling_cycle_on_put_without_persistence ... ok
test rejects_unselected_sibling_missing_parent_on_delete_without_persistence ... ok
test rejects_unselected_sibling_cycle_on_delete_without_persistence ... ok
test rejects_unselected_sibling_seed_misuse_on_delete_without_persistence ... ok
test recreates_a_deleted_first_party_href_in_its_existing_stream ... ok
test rejects_unselected_sibling_seed_misuse_on_put_without_persistence ... ok
test serializes_concurrent_writes_against_the_same_etag ... ok
test applies_calendar_preconditions_reports_and_sync_tombstones ... ok
test preserves_first_party_streams_and_kinds_through_conditional_dav_writes ... ok
test rejects_broken_heads_without_persisting_invalid_outbox_operations ... ok

test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s

     Running unittests src/lib.rs (/private/tmp/kamori-audit-target-197/debug/deps/dav_bridge_desktop-5b737ee42ae6427e)

running 18 tests
test commands::common::tests::endpoint_join_is_stable ... ok
test commands::common::tests::legacy_refresh_token_receives_a_random_attempt_id ... ok
test commands::common::tests::refresh_token_account_is_stable_for_base_url ... ok
test commands::common::tests::ui_error_conversion_is_non_empty ... ok
test commands::common::tests::refresh_credential_roundtrips_token_and_random_attempt_id ... ok
test commands::window::tests::normalize_window_preferences_forces_tray_for_hide ... ok
test commands::window::tests::cloud_base_url_requires_https_except_loopback ... ok
test commands::window::tests::normalize_window_preferences_keeps_non_hide_values ... ok
test state::tests::sync_counts_are_recorded_for_the_matching_collection ... ok
test commands::collections::tests::desktop_recovery_package_unwraps_on_second_device_with_account_identity_only ... ok
test commands::auth::tests::nonempty_cloud_spaces_decode_in_direct_and_runner_paths ... ok
test commands::auth::tests::runner_reconciliation_repairs_legacy_recovery_only_with_current_device_key ... ok
test commands::auth::tests::startup_rejects_response_for_another_account_without_committing_it ... ok
test commands::auth::tests::startup_keychain_only_attempt_is_durably_adopted_before_network ... ok
test commands::auth::tests::startup_keychain_failure_stops_network_and_preserves_retry ... ok
test commands::auth::tests::runtime_dropped_refresh_response_is_exactly_retried_by_startup ... ok
test commands::auth::tests::startup_refuses_older_than_predecessor_until_explicit_fresh_signin ... ok
test commands::auth::tests::startup_refresh_preserves_exact_retry_and_unrelated_sessions_at_crash_boundaries ... ok

test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.49s

     Running unittests src/main.rs (/private/tmp/kamori-audit-target-197/debug/deps/dav_bridge_desktop-c66a517863369050)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests crypto_core_lib

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests dav_bridge_desktop

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```

## Self-review and limits

- Verified that startup's production orchestration, not only preexisting cache unit helpers, selects the runtime's pending R. A real runner receives a synthetic HTTP 401, sends refresh, loses its response, and startup repeats the observed request id. Fake server reuse semantics assert both replacement and unrelated sessions stay active.
- Verified response-loss, cache-commit/keychain-stale, keychain-current, a second response loss, OS anchor-write failure, keychain-only fallback, account-response mismatch, legacy anchored adoption, explicit account/endpoint conflicts, and unanchored legacy refusal. The 2+ rotation stale-anchor regression uses the real runner persistence API and the production startup store-opening guard.
- Verified fresh signin can replace account/backend attribution without losing cached PIM resources or the monotonic cursor. Runtime openings cannot erase known desktop account attribution. Unattributed FRB fresh-login/backend switching remains covered and unchanged.
- Self-review caught and corrected an intermediate cached-access shortcut: cached A1 can be expired. Final behavior advances the keychain anchor first, then refreshes to get fresh access; it does not assume cached access is current.
- No live platform keychain, packaged desktop UI, external DAV client, production server or hardware power-loss test was performed. OS and server boundaries are synthetic, while selection, persistence, HPKE and reconciliation code are production paths. No recovery is claimed for keys absent from legitimate current authorized material.
- Accepted operational limit: a keychain generation older than the immediate predecessor requires fresh signin. It fails closed without issuing a token-reuse request. A crash during fresh signin before the new OS credential is stored can similarly require repeating signin; the atomic credential reset prevents an old anchor from seeing an empty-cache fallback gap.
