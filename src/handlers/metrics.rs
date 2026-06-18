//! GET /metrics — eksposisi metrik format Prometheus.

use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;

use crate::state::AppState;

/// Pulang metrik dalam format teks Prometheus (terbuka, untuk pengikis dalaman).
pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let body = state.metrics.render_prometheus();
    ([(CONTENT_TYPE, "text/plain; version=0.0.4")], body)
}
