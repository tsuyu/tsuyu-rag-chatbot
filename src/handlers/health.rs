//! GET /health — semak DB, Ollama (+ model), dan reranker.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::db;
use crate::models::{HealthResponse, ModelHealth};
use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let (all_ok, body) = gather_health(&state).await;

    let status_code = if all_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(body))
}

/// Jalankan semua semakan kesihatan dan pulang `(all_ok, HealthResponse)`.
///
/// Diasingkan daripada handler HTTP supaya perintah CLI `check` boleh guna semula
/// logik yang sama tanpa pelayan.
pub async fn gather_health(state: &AppState) -> (bool, HealthResponse) {
    let db_ok = db::ping(&state.db).await.is_ok();

    // Ambil senarai model Ollama sekali; guna untuk semak "hidup" + ketersediaan model.
    let tags = fetch_ollama_tags(state).await;
    let ollama_ok = tags.is_some();
    let available = tags.unwrap_or_default();

    let models = ModelHealth {
        gen: model_present(&available, &state.config.gen_model),
        embed: model_present(&available, &state.config.embed_model),
    };

    // Reranker hanya diperiksa jika dihidupkan.
    let reranker = if state.config.rerank_enabled {
        Some(check_reranker(state).await)
    } else {
        None
    };

    // "ok" hanya jika semua komponen relevan hidup DAN model tersedia.
    let all_ok = db_ok && ollama_ok && models.gen && models.embed && reranker.unwrap_or(true);

    let body = HealthResponse {
        status: if all_ok { "ok" } else { "degraded" }.to_string(),
        database: db_ok,
        ollama: ollama_ok,
        reranker,
        models,
    };

    (all_ok, body)
}

#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagModel>,
}

#[derive(Deserialize)]
struct TagModel {
    #[serde(default)]
    name: String,
}

/// Ambil senarai nama model dari Ollama (`GET /api/tags`).
/// Pulang `None` jika Ollama tidak boleh dihubungi atau respons tidak sah.
async fn fetch_ollama_tags(state: &AppState) -> Option<Vec<String>> {
    let url = format!("{}/api/tags", state.config.ollama_url);
    let resp = state.http.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed = resp.json::<TagsResponse>().await.ok()?;
    Some(parsed.models.into_iter().map(|m| m.name).collect())
}

/// Adakah `wanted` hadir dalam senarai model Ollama?
///
/// Ollama melampirkan tag (cth. `:latest`). Kita padan secara fleksibel: nama penuh
/// sama, ATAU bahagian sebelum ':' sama (cth. config `bge-m3` ≈ tag `bge-m3:latest`).
fn model_present(available: &[String], wanted: &str) -> bool {
    let wanted_base = wanted.split(':').next().unwrap_or(wanted);
    available.iter().any(|m| {
        m == wanted || m.split(':').next().unwrap_or(m.as_str()) == wanted_base
    })
}

/// Ping servis reranker pada laluan akar (`GET {RERANKER_URL}/`).
async fn check_reranker(state: &AppState) -> bool {
    let url = format!("{}/", state.config.reranker_url.trim_end_matches('/'));
    matches!(
        state.http.get(&url).send().await,
        Ok(resp) if resp.status().is_success()
    )
}

#[cfg(test)]
mod tests {
    use super::model_present;

    #[test]
    fn padanan_nama_penuh() {
        let avail = vec!["qwen3:14b".to_string(), "bge-m3:latest".to_string()];
        assert!(model_present(&avail, "qwen3:14b"));
    }

    #[test]
    fn padanan_abai_tag_latest() {
        let avail = vec!["bge-m3:latest".to_string()];
        // Config tanpa tag patut padan dengan tag :latest Ollama.
        assert!(model_present(&avail, "bge-m3"));
    }

    #[test]
    fn tiada_padanan() {
        let avail = vec!["llama3.2:3b".to_string()];
        assert!(!model_present(&avail, "qwen3:14b"));
    }

    #[test]
    fn senarai_kosong() {
        assert!(!model_present(&[], "qwen3:14b"));
    }
}
