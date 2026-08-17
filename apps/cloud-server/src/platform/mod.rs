//! Platform and infrastructure modules shared across features.

pub mod config;
pub mod db;
pub mod jwt;
pub mod maintenance;
pub mod metrics;
pub mod object_storage;
pub mod rate_limit;
pub mod secret_box;
pub mod security;
pub mod state;
pub mod state_store;

#[cfg(test)]
pub mod test_support;
