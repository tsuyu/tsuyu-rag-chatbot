//! Jenis ralat domain + penukaran kepada respons HTTP.
//!
//! Peraturan projek: TIADA unwrap()/expect() dalam kod produksi. Semua ralat
//! dikendalikan melalui `?` dan jenis `AppError` ini.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("ralat pangkalan data: {0}")]
    Database(#[from] sqlx::Error),

    #[error("ralat sambungan ke Ollama: {0}")]
    Http(#[from] reqwest::Error),

    #[error("ralat I/O fail: {0}")]
    Io(#[from] std::io::Error),

    /// Permintaan pengguna tidak sah (cth. soalan kosong).
    #[error("{0}")]
    BadRequest(String),

    /// Pengesahan gagal — API key tidak sah atau tiada.
    #[error("tidak dibenarkan: API key tidak sah atau tiada")]
    Unauthorized,

    /// Sumber tidak dijumpai (cth. dokumen tiada).
    #[error("tidak dijumpai: {0}")]
    NotFound(String),

    /// Terlalu banyak permintaan — had kadar dilampaui.
    #[error("terlalu banyak permintaan; sila cuba sebentar lagi")]
    TooManyRequests,

    /// Ralat dalaman umum (dibungkus dari anyhow di lapisan service).
    #[error("ralat dalaman: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        // Log ralat penuh di pelayan, pulang mesej ringkas kepada klien.
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!("{self}");
        }

        let body = Json(json!({ "error": self.to_string() }));
        (status, body).into_response()
    }
}
