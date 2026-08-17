/* tslint:disable */
/* eslint-disable */

export function decrypt_group_key_from_peer(encrypted: any, recipient_private_key: Uint8Array): Uint8Array;

export function decrypt_payload(encrypted: any, key: Uint8Array, aad: Uint8Array): Uint8Array;

export function decrypt_vault_bytes(master_key: Uint8Array, encrypted: Uint8Array): Uint8Array;

export function encrypt_group_key_for_peer(cmk: Uint8Array, peer_public_key: Uint8Array): any;

export function encrypt_payload(algorithm: any, key: Uint8Array, nonce: Uint8Array, plaintext: Uint8Array, aad: Uint8Array): any;

export function encrypt_vault_bytes(master_key: Uint8Array, plaintext: Uint8Array): Uint8Array;

export function generate_qr_svg(payload: string): string;

export function generate_web_device_identity(): any;

export function generate_x25519_keypair(): any;

/**
 * Encodes the 256-bit account master key as a checksummed 24-word BIP-39 kit.
 */
export function master_key_to_recovery_phrase(master_key: Uint8Array): string;

export function opaque_signin_finish(flow_id: string, password: Uint8Array, opaque_server_message: Uint8Array): any;

export function opaque_signin_start(password: Uint8Array): any;

export function opaque_signup_finish(flow_id: string, password: Uint8Array, opaque_server_message: Uint8Array): any;

export function opaque_signup_start(password: Uint8Array): any;

export function open_operation_envelope(envelope: any, space_key: Uint8Array): Uint8Array;

/**
 * Validates a 24-word BIP-39 kit and restores its exact account master key.
 */
export function recovery_phrase_to_master_key(phrase: string): Uint8Array;

export function seal_operation_envelope(space_id: string, stream_id: string, client_op_id: string, author_device_id: string, key_epoch: number, envelope_kind: string, plaintext: Uint8Array, space_key: Uint8Array, signing_private_key: Uint8Array): any;

export function unwrap_account_master_key(export_key: Uint8Array, encrypted: Uint8Array): Uint8Array;

export function verify_operation_envelope(envelope: any, signing_public_key: Uint8Array): void;

export function wrap_account_master_key(export_key: Uint8Array, master_key: Uint8Array): Uint8Array;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly decrypt_group_key_from_peer: (a: any, b: number, c: number) => [number, number];
    readonly decrypt_payload: (a: any, b: number, c: number, d: number, e: number) => [number, number];
    readonly decrypt_vault_bytes: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly encrypt_group_key_for_peer: (a: number, b: number, c: number, d: number) => any;
    readonly encrypt_payload: (a: any, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => any;
    readonly encrypt_vault_bytes: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly generate_qr_svg: (a: number, b: number) => [number, number, number, number];
    readonly generate_web_device_identity: () => [number, number, number];
    readonly generate_x25519_keypair: () => any;
    readonly master_key_to_recovery_phrase: (a: number, b: number) => [number, number, number, number];
    readonly opaque_signin_finish: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
    readonly opaque_signin_start: (a: number, b: number) => [number, number, number];
    readonly opaque_signup_finish: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
    readonly opaque_signup_start: (a: number, b: number) => [number, number, number];
    readonly open_operation_envelope: (a: any, b: number, c: number) => [number, number, number, number];
    readonly recovery_phrase_to_master_key: (a: number, b: number) => [number, number, number, number];
    readonly seal_operation_envelope: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number) => [number, number, number];
    readonly unwrap_account_master_key: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly verify_operation_envelope: (a: any, b: number, c: number) => [number, number];
    readonly wrap_account_master_key: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
