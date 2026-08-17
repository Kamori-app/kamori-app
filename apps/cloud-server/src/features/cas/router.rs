//! Space-scoped encrypted blob routes.

use axum::{Router, extract::DefaultBodyLimit, routing::post};

use crate::platform::state::AppState;

use super::handlers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/spaces/{space_id}/blobs", post(handlers::cas_upload))
        .route(
            "/spaces/{space_id}/blobs/{blob_id}",
            axum::routing::get(handlers::cas_download),
        )
        .layer(DefaultBodyLimit::max(26 * 1024 * 1024))
}
