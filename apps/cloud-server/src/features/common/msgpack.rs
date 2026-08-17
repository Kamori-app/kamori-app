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

        let mut deserializer = rmp_serde::Deserializer::new(bytes.as_ref()).with_human_readable();
        let value = T::deserialize(&mut deserializer).map_err(|_error| {
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
        let mut body = Vec::new();
        let result = self.0.serialize(
            &mut rmp_serde::Serializer::new(&mut body)
                .with_struct_map()
                .with_human_readable(),
        );
        match result {
            Ok(()) => (
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

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct BrowserContract {
        id: Uuid,
        #[serde(with = "serde_bytes")]
        public_key: Vec<u8>,
    }

    #[derive(Deserialize)]
    struct BrowserWireContract {
        id: String,
        #[serde(with = "serde_bytes")]
        public_key: Vec<u8>,
    }

    #[test]
    fn human_readable_msgpack_uses_uuid_strings_and_binary_bytes() {
        let expected = BrowserContract {
            id: Uuid::parse_str("f26f4239-754e-4653-81b8-5b6112514a16").expect("uuid"),
            public_key: vec![1, 2, 3, 4],
        };
        let mut encoded = Vec::new();
        expected
            .serialize(
                &mut rmp_serde::Serializer::new(&mut encoded)
                    .with_struct_map()
                    .with_human_readable(),
            )
            .expect("encode browser contract");

        let wire: BrowserWireContract =
            rmp_serde::from_slice(&encoded).expect("decode browser wire contract");
        assert_eq!(wire.id, "f26f4239-754e-4653-81b8-5b6112514a16");
        assert_eq!(wire.public_key, [1, 2, 3, 4]);

        let mut deserializer =
            rmp_serde::Deserializer::new(encoded.as_slice()).with_human_readable();
        let decoded = BrowserContract::deserialize(&mut deserializer).expect("decode contract");
        assert_eq!(decoded, expected);
    }
}
