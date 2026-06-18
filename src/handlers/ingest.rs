//! POST /ingest — cetuskan ingest dokumen sebagai background task.

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::models::IngestResponse;
use crate::services::ingest as ingest_svc;
use crate::state::AppState;

/// Parameter query untuk /ingest. `?force=true` memaksa ingest semula semua fail
/// walaupun tidak berubah.
#[derive(Debug, Deserialize)]
pub struct IngestParams {
    #[serde(default)]
    pub force: bool,
}

pub async fn ingest(
    State(state): State<AppState>,
    Query(params): Query<IngestParams>,
) -> Json<IngestResponse> {
    let force = params.force;
    state.metrics.inc_ingest();

    // Ingest boleh ambil masa (banyak panggilan embedding), jadi kita larikan
    // sebagai tugas latar belakang dan terus pulang respons kepada pemanggil.
    tokio::spawn(async move {
        match ingest_svc::ingest_dir(&state, force).await {
            Ok(s) => tracing::info!(
                "ingest selesai: {} dokumen, {} chunk, {} tidak berubah, {} dilangkau (ralat)",
                s.documents,
                s.chunks,
                s.unchanged,
                s.skipped
            ),
            Err(e) => tracing::error!("ingest gagal: {e}"),
        }
    });

    let message = if force {
        "Ingest (paksa) dimulakan di latar belakang. Semak log untuk kemajuan."
    } else {
        "Ingest dimulakan di latar belakang. Fail tidak berubah akan dilangkau. Semak log untuk kemajuan."
    };

    Json(IngestResponse {
        status: "accepted".to_string(),
        message: message.to_string(),
    })
}
