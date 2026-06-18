//! Cubaan semula (retry) dengan exponential backoff untuk panggilan HTTP ke Ollama
//! dan reranker, supaya ralat sementara (sibuk/timeout/5xx) tidak terus gagal.

use std::time::Duration;

use crate::error::AppError;
use crate::state::AppState;

/// Hantar permintaan HTTP dengan cubaan semula bagi ralat sementara.
///
/// `make` membina permintaan baharu setiap cubaan (kerana `RequestBuilder` digunakan
/// sekali sahaja). Respons dengan status berjaya dipulangkan; status 5xx/429 dan ralat
/// rangkaian (timeout/connect) dianggap sementara dan dicuba semula sehingga
/// `OLLAMA_MAX_RETRIES` kali dengan backoff `base * 2^percubaan`.
///
/// Ralat 4xx (selain 429) dianggap kekal — tidak dicuba semula.
pub async fn send_with_retry<F>(
    state: &AppState,
    label: &str,
    make: F,
) -> Result<reqwest::Response, AppError>
where
    F: Fn() -> reqwest::RequestBuilder,
{
    let max = state.config.ollama_max_retries;
    let base = state.config.ollama_retry_base_ms;
    let mut attempt = 0u32;

    loop {
        let result = make().send().await;

        // Tentukan sama ada ini ralat sementara yang patut dicuba semula.
        let transient_err: Option<String> = match &result {
            Ok(resp) => {
                let s = resp.status();
                if s.is_server_error() || s == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    Some(format!("status {s}"))
                } else {
                    None
                }
            }
            Err(e) if e.is_timeout() || e.is_connect() || e.is_request() => Some(e.to_string()),
            Err(_) => None, // ralat lain: jangan retry
        };

        match transient_err {
            // Berjaya atau ralat kekal: kembalikan terus (biar pemanggil urus error_for_status).
            None => return result.map_err(AppError::from),
            Some(reason) => {
                if attempt >= max {
                    // Cubaan habis: kembalikan hasil terakhir (Ok 5xx atau Err).
                    tracing::warn!("{label}: gagal selepas {} cubaan ({reason})", attempt + 1);
                    return result.map_err(AppError::from);
                }
                let delay = base.saturating_mul(1u64 << attempt);
                tracing::warn!(
                    "{label}: cubaan {} gagal ({reason}); cuba semula dalam {delay}ms",
                    attempt + 1
                );
                tokio::time::sleep(Duration::from_millis(delay)).await;
                attempt += 1;
            }
        }
    }
}
