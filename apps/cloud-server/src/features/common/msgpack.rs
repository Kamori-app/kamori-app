//! MessagePack request/response extractors and responders.

use axum::{
    Json,
    body::Bytes,
    extract::{FromRequest, Request},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Serialize, de::DeserializeOwned};

use super::{ApiError, ErrorResponse};

/// MessagePack request/response wrapper used by binary-heavy endpoints.
pub struct MsgPack<T>(pub T);

impl<S, T> FromRequest<S> for MsgPack<T>
where
    S: Send + Sync,
    Bytes: FromRequest<S>,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state).await.map_err(|_error| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "invalid_request",
                    "invalid msgpack request body",
                )),
            )
        })?;

        let value = rmp_serde::from_slice(&bytes).map_err(|_error| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "invalid_request",
                    "failed to decode msgpack body",
                )),
            )
        })?;
        Ok(Self(value))
    }
}

impl<T> IntoResponse for MsgPack<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        match rmp_serde::to_vec_named(&self.0) {
            Ok(body) => (
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/msgpack"),
                )],
                body,
            )
                .into_response(),
            Err(error) => {
                let response =
                    ErrorResponse::new("internal_error", "failed to encode server response");
                tracing::error!(request_id = %response.request_id, %error, "msgpack response encoding failed");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(response)).into_response()
            }
        }
    }
}
