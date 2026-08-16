//! Shared API transport helpers used across features.

mod authz;
mod errors;
mod msgpack;

pub use authz::{Principal, authorize_principal, authorize_session};
pub use errors::{
    ApiError, ErrorResponse, bad_request, conflict, internal_error, not_found, quota_exceeded,
    unauthenticated, unauthorized,
};
pub use msgpack::MsgPack;
