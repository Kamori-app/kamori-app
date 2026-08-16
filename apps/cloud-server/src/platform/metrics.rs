//! Low-cardinality operational HTTP metrics.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

/// Process-local counters intended for Prometheus scraping.
#[derive(Default)]
pub struct HttpMetrics {
    requests_total: AtomicU64,
    responses_4xx_total: AtomicU64,
    responses_5xx_total: AtomicU64,
    in_flight: AtomicU64,
    duration_microseconds_total: AtomicU64,
}

impl HttpMetrics {
    /// Renders the current aggregate snapshot in Prometheus text format.
    pub fn render(&self) -> String {
        format!(
            concat!(
                "# TYPE kamori_http_requests_total counter\n",
                "kamori_http_requests_total {}\n",
                "# TYPE kamori_http_responses_4xx_total counter\n",
                "kamori_http_responses_4xx_total {}\n",
                "# TYPE kamori_http_responses_5xx_total counter\n",
                "kamori_http_responses_5xx_total {}\n",
                "# TYPE kamori_http_requests_in_flight gauge\n",
                "kamori_http_requests_in_flight {}\n",
                "# TYPE kamori_http_request_duration_seconds_total counter\n",
                "kamori_http_request_duration_seconds_total {:.6}\n",
            ),
            self.requests_total.load(Ordering::Relaxed),
            self.responses_4xx_total.load(Ordering::Relaxed),
            self.responses_5xx_total.load(Ordering::Relaxed),
            self.in_flight.load(Ordering::Relaxed),
            self.duration_microseconds_total.load(Ordering::Relaxed) as f64 / 1_000_000.0,
        )
    }
}

/// Records one HTTP request without route, user, IP, or content labels.
pub async fn record_http(
    State(metrics): State<Arc<HttpMetrics>>,
    request: Request,
    next: Next,
) -> Response {
    metrics.requests_total.fetch_add(1, Ordering::Relaxed);
    metrics.in_flight.fetch_add(1, Ordering::Relaxed);
    let started = Instant::now();
    let response = next.run(request).await;
    metrics.in_flight.fetch_sub(1, Ordering::Relaxed);
    metrics.duration_microseconds_total.fetch_add(
        u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
    if response.status().is_client_error() {
        metrics.responses_4xx_total.fetch_add(1, Ordering::Relaxed);
    } else if response.status().is_server_error() {
        metrics.responses_5xx_total.fetch_add(1, Ordering::Relaxed);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_has_no_dynamic_labels() {
        let rendered = HttpMetrics::default().render();
        assert!(rendered.contains("kamori_http_requests_total 0"));
        assert!(!rendered.contains('{'));
    }
}
