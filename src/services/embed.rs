//! Jana embedding melalui Ollama (`POST /api/embeddings`).

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::services::retry::send_with_retry;
use crate::state::AppState;

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
}

/// Hasilkan satu vektor embedding untuk satu teks.
pub async fn embed_text(state: &AppState, text: &str) -> Result<Vec<f32>, AppError> {
    let url = format!("{}/api/embeddings", state.config.ollama_url);

    let resp = send_with_retry(state, "embed_text", || {
        state.http.post(&url).json(&EmbedRequest {
            model: &state.config.embed_model,
            prompt: text,
        })
    })
    .await?
    .error_for_status()?
    .json::<EmbedResponse>()
    .await?;

    if resp.embedding.is_empty() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "Ollama pulang embedding kosong untuk model '{}'",
            state.config.embed_model
        )));
    }

    Ok(resp.embedding)
}

#[derive(Serialize)]
struct EmbedBatchRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct EmbedBatchResponse {
    embeddings: Vec<Vec<f32>>,
}

/// Hasilkan embedding untuk beberapa teks dalam SATU panggilan HTTP (endpoint
/// `/api/embed` Ollama). Jauh lebih pantas daripada memanggil `embed_text`
/// berulang kali semasa ingest.
///
/// Pulang vektor dalam susunan yang sama dengan `texts`.
pub async fn embed_batch(state: &AppState, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let url = format!("{}/api/embed", state.config.ollama_url);

    let resp = send_with_retry(state, "embed_batch", || {
        state.http.post(&url).json(&EmbedBatchRequest {
            model: &state.config.embed_model,
            input: texts,
        })
    })
    .await?
    .error_for_status()?
    .json::<EmbedBatchResponse>()
    .await?;

    if resp.embeddings.len() != texts.len() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "Ollama pulang {} embedding untuk {} input (model '{}')",
            resp.embeddings.len(),
            texts.len(),
            state.config.embed_model
        )));
    }

    Ok(resp.embeddings)
}
