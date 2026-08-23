#![allow(clippy::result_large_err)]

#[cfg(all(feature = "frb", not(frb_expand)))]
mod frb_generated;

pub mod account_keys;
mod crypto;
pub mod operation_envelope;
pub mod pim;
pub mod recovery;
pub mod secret_vault;

pub use crypto::{
    CipherAlgorithm, CryptoEngine, CryptoError, EncryptedDeviceBootstrap, EncryptedGroupKey,
    EncryptedPayload, Keypair, SymmetricKey,
};
#[cfg(any(feature = "local-bridge", feature = "sync-runtime"))]
pub mod local_bridge_runner;

#[cfg(feature = "wasm")]
mod wasm_bindings;

#[cfg(feature = "frb")]
#[flutter_rust_bridge::frb]
mod frb_api {
    include!("frb_api.rs");
}

#[cfg(test)]
mod tests;
