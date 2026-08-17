//! Health feature handlers.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::features::health::dto::HealthResponse;
use crate::platform::state::AppState;

/// Process liveness check; it intentionally does not touch dependencies.
pub async fn live() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

/// Dependency readiness check used by the load balancer.
pub async fn ready(State(state): State<AppState>) -> Response {
    let postgres_ready = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .is_ok();
    let valkey_ready = state.state_store.get("health:ready").await.is_ok();
    let ready = postgres_ready && valkey_ready;
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(HealthResponse {
            status: if ready { "ok" } else { "unavailable" }.to_string(),
        }),
    )
        .into_response()
}

/// Authenticated Prometheus exposition with no per-user or content labels.
pub async fn metrics(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let supplied = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if Sha256::digest(supplied.as_bytes())
        != Sha256::digest(state.config.metrics_bearer_token.as_bytes())
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let budget_row = sqlx::query(
        r#"
        SELECT
            (SELECT count(*)::bigint FROM users WHERE deleted_at IS NULL) AS active_accounts,
            (SELECT COALESCE(sum(size_padded), 0)::bigint FROM space_blobs WHERE status = 'ready') AS stored_blob_bytes,
            (SELECT count(*)::bigint FROM space_blobs WHERE status = 'pending') AS pending_blobs,
            (SELECT COALESCE(sum(bytes_reserved), 0)::bigint
             FROM blob_egress_reservations
             WHERE reserved_at >= date_trunc('month', now())) AS monthly_blob_egress,
            (SELECT COALESCE(extract(epoch FROM last_succeeded_at), 0)::bigint
             FROM operator_job_heartbeats WHERE job_name = 'object_cleanup') AS cleanup_success_epoch,
            (SELECT COALESCE(extract(epoch FROM last_succeeded_at), 0)::bigint
             FROM operator_job_heartbeats WHERE job_name = 'postgres_backup') AS postgres_backup_success_epoch,
            (SELECT COALESCE(extract(epoch FROM last_succeeded_at), 0)::bigint
             FROM operator_job_heartbeats WHERE job_name = 'blob_replication') AS blob_replication_success_epoch
        "#,
    )
    .fetch_one(&state.pool)
    .await;
    let row = match budget_row {
        Ok(row) => row,
        Err(error) => {
            tracing::error!(%error, "operational metrics query failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let beta_account_limit = match crate::features::admin::services::effective_u64(
        &state,
        "beta_account_limit",
        state.config.beta_account_limit,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(?error, "effective beta limit query failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let nonessential_stop = match crate::features::admin::services::effective_u64(
        &state,
        "global_nonessential_egress_stop_bytes",
        state.config.global_nonessential_egress_stop_bytes,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(?error, "effective egress stop query failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let emergency_breaker = match crate::features::admin::services::effective_u64(
        &state,
        "global_emergency_egress_breaker_bytes",
        state.config.global_emergency_egress_breaker_bytes,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(?error, "effective emergency breaker query failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let mut output = state.http_metrics.render();
    output.push_str(&format!(
        concat!(
            "# TYPE kamori_active_accounts gauge\n",
            "kamori_active_accounts {}\n",
            "# TYPE kamori_beta_account_limit gauge\n",
            "kamori_beta_account_limit {}\n",
            "# TYPE kamori_stored_blob_bytes gauge\n",
            "kamori_stored_blob_bytes {}\n",
            "# TYPE kamori_pending_blobs gauge\n",
            "kamori_pending_blobs {}\n",
            "# TYPE kamori_blob_egress_month_bytes gauge\n",
            "kamori_blob_egress_month_bytes {}\n",
            "# TYPE kamori_blob_egress_nonessential_stop_bytes gauge\n",
            "kamori_blob_egress_nonessential_stop_bytes {}\n",
            "# TYPE kamori_blob_egress_emergency_breaker_bytes gauge\n",
            "kamori_blob_egress_emergency_breaker_bytes {}\n",
            "# TYPE kamori_object_cleanup_last_success_timestamp_seconds gauge\n",
            "kamori_object_cleanup_last_success_timestamp_seconds {}\n",
            "# TYPE kamori_postgres_backup_last_success_timestamp_seconds gauge\n",
            "kamori_postgres_backup_last_success_timestamp_seconds {}\n",
            "# TYPE kamori_blob_replication_last_success_timestamp_seconds gauge\n",
            "kamori_blob_replication_last_success_timestamp_seconds {}\n",
        ),
        row.try_get::<i64, _>("active_accounts").unwrap_or_default(),
        beta_account_limit,
        row.try_get::<i64, _>("stored_blob_bytes")
            .unwrap_or_default(),
        row.try_get::<i64, _>("pending_blobs").unwrap_or_default(),
        row.try_get::<i64, _>("monthly_blob_egress")
            .unwrap_or_default(),
        nonessential_stop,
        emergency_breaker,
        row.try_get::<i64, _>("cleanup_success_epoch")
            .unwrap_or_default(),
        row.try_get::<i64, _>("postgres_backup_success_epoch")
            .unwrap_or_default(),
        row.try_get::<i64, _>("blob_replication_success_epoch")
            .unwrap_or_default(),
    ));
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        output,
    )
        .into_response()
}
