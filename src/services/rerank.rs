//! Reranking chunk menggunakan model cross-encoder (cth. bge-reranker-v2-m3).
//!
//! Ollama tidak menyajikan reranker; jadi kita panggil servis berasingan yang
//! mendedahkan endpoint `/rerank` serasi HuggingFace TEI (text-embeddings-inference):
//!
//!   POST {RERANKER_URL}/rerank
//!   { "query": "...", "texts": ["...", "..."] }
//!   -> [ { "index": 0, "score": 0.97 }, { "index": 3, "score": 0.61 }, ... ]
//!
//! Reranker menilai pasangan (soalan, chunk) secara langsung, memberi ketepatan
//! relevansi yang jauh lebih baik daripada carian vektor semata-mata.

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::models::RetrievedChunk;
use crate::services::retry::send_with_retry;
use crate::state::AppState;

#[derive(Serialize)]
struct RerankRequest<'a> {
    query: &'a str,
    texts: Vec<&'a str>,
    /// TEI menerima medan `model` (diabaikan oleh sesetengah pelaksanaan single-model).
    model: &'a str,
}

#[derive(Deserialize)]
struct RerankResult {
    index: usize,
    score: f64,
}

/// Susun semula `chunks` mengikut relevansi dengan `question` menggunakan reranker,
/// kemudian pulang `top_k` teratas. Skor reranker dilekatkan pada setiap chunk.
///
/// Jika `chunks` kosong, pulang kosong tanpa memanggil servis.
pub async fn rerank(
    state: &AppState,
    question: &str,
    mut chunks: Vec<RetrievedChunk>,
    top_k: usize,
) -> Result<Vec<RetrievedChunk>, AppError> {
    if chunks.is_empty() {
        return Ok(chunks);
    }

    let url = format!("{}/rerank", state.config.reranker_url.trim_end_matches('/'));

    let results = send_with_retry(state, "rerank", || {
        // Bina semula badan setiap cubaan (closure Fn). `texts` meminjam `chunks`
        // yang hidup lebih lama daripada closure ini.
        let texts: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
        state.http.post(&url).json(&RerankRequest {
            query: question,
            texts,
            model: &state.config.reranker_model,
        })
    })
    .await?
    .error_for_status()?
    .json::<Vec<RerankResult>>()
    .await?;

    // Lekatkan skor pada chunk mengikut indeks yang dipulangkan reranker.
    for r in &results {
        if let Some(c) = chunks.get_mut(r.index) {
            c.rerank_score = Some(r.score);
        }
    }

    // Susun ikut skor reranker menurun. Chunk tanpa skor (jika ada) diletak di belakang.
    chunks.sort_by(|a, b| {
        let sa = a.rerank_score.unwrap_or(f64::NEG_INFINITY);
        let sb = b.rerank_score.unwrap_or(f64::NEG_INFINITY);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });

    chunks.truncate(top_k);
    Ok(chunks)
}
