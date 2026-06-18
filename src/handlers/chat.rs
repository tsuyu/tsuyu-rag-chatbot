//! Endpoint chat: soalan -> retrieval -> generation -> jawapan + sumber.
//!
//! Dua varian:
//! - `chat`        : pulang keseluruhan jawapan sekali gus (JSON).
//! - `chat_stream` : alirkan jawapan token demi token (SSE) untuk respons pantas.

use std::convert::Infallible;
use std::time::Instant;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::{pin_mut, Stream, StreamExt};

use crate::error::AppError;
use crate::models::{ChatRequest, ChatResponse, Message, MetaFilter, RetrievedChunk, Source};
use crate::services::{generate, memory, rerank, retrieve};
use crate::state::AppState;

/// Muat sejarah perbualan untuk sesi (jika memori dihidupkan & ada `session_id`).
async fn muat_sejarah(
    state: &AppState,
    session_id: Option<&str>,
) -> Result<Vec<Message>, AppError> {
    match (state.config.memory_enabled, session_id) {
        (true, Some(sid)) => memory::load_recent(state, sid, state.config.memory_turns).await,
        _ => Ok(Vec::new()),
    }
}

/// Cari konteks untuk soalan: carian (hybrid atau vektor), kemudian reranking (jika dihidupkan).
///
/// Carian: `HYBRID_ENABLED=true` → gabung vektor + kata kunci (RRF); jika tidak, vektor sahaja.
/// Tanpa rerank: ambil terus `top_k` chunk.
/// Dengan rerank: ambil `retrieve_n` (lebih besar) → rerank → pangkas ke `top_k`.
async fn cari_konteks(
    state: &AppState,
    question: &str,
    filter: &MetaFilter,
) -> Result<Vec<RetrievedChunk>, AppError> {
    let top_k = state.config.top_k.max(1) as usize;
    let mula = Instant::now();

    let hasil = if state.config.rerank_enabled {
        let n = state.config.retrieve_n.max(state.config.top_k);
        let candidates = retrieve::retrieve(state, question, n, filter).await?;
        rerank::rerank(state, question, candidates, top_k).await
    } else {
        retrieve::retrieve(state, question, state.config.top_k, filter).await
    };

    // Rekod masa retrieval (termasuk rerank) hanya jika berjaya.
    if hasil.is_ok() {
        state.metrics.observe_retrieval(mula.elapsed().as_millis() as u64);
    }
    hasil
}

/// Mesej penolakan piawai apabila tiada konteks cukup relevan dalam dokumen TSUYU.
const MESEJ_PENOLAKAN: &str =
    "Maaf, saya tidak menjumpai maklumat berkaitan soalan ini dalam dokumen TSUYU. \
     Sila cuba tanya dengan cara lain atau pastikan dokumen berkaitan telah dimuat naik.";

/// Guardrail: adakah chunk yang diambil cukup relevan untuk dijawab oleh LLM?
///
/// Pintasan yang membaca konfigurasi dari `state`; logik sebenar dalam `nilai_relevan`
/// (fungsi tulen, diuji secara berasingan).
fn cukup_relevan(state: &AppState, chunks: &[RetrievedChunk]) -> bool {
    nilai_relevan(
        chunks,
        state.config.relevance_enabled,
        state.config.relevance_min_rerank,
        state.config.relevance_max_distance,
    )
}

/// Logik tulen guardrail relevansi (tiada `AppState`, mudah diuji).
///
/// Bila reranking dihidupkan, guna `rerank_score` chunk terbaik (semakin tinggi semakin
/// relevan); jika tidak, guna jarak cosine terkecil (semakin kecil semakin relevan).
/// Jika `enabled == false`, sentiasa `true`.
fn nilai_relevan(
    chunks: &[RetrievedChunk],
    enabled: bool,
    min_rerank: f64,
    max_distance: f64,
) -> bool {
    if !enabled {
        return true;
    }
    // Tiada chunk langsung → tidak relevan.
    let Some(terbaik) = chunks.first() else {
        return false;
    };

    match terbaik.rerank_score {
        // Laluan reranker: skor mesti >= ambang minimum.
        Some(skor) => skor >= min_rerank,
        // Laluan vektor: jarak chunk terkecil mesti <= ambang maksimum.
        None => {
            let jarak_min = chunks
                .iter()
                .map(|c| c.distance)
                .fold(f64::INFINITY, f64::min);
            jarak_min <= max_distance
        }
    }
}

/// POST /chat — jawapan penuh dalam satu respons JSON.
pub async fn chat(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, AppError> {
    state.metrics.inc_chat();
    let hasil = chat_inner(&state, req).await;
    if hasil.is_err() {
        state.metrics.inc_chat_error();
    }
    hasil.map(Json)
}

/// Logik teras /chat, diasingkan supaya pengiraan metrik (jaya/gagal) berpusat.
async fn chat_inner(state: &AppState, req: ChatRequest) -> Result<ChatResponse, AppError> {
    let question = req.question.trim();
    if question.is_empty() {
        return Err(AppError::BadRequest("soalan tidak boleh kosong".to_string()));
    }

    let history = muat_sejarah(state, req.session_id.as_deref()).await?;
    let filter = req.filter.clone().unwrap_or_default();
    let chunks = cari_konteks(state, question, &filter).await?;

    // Guardrail: jika konteks tidak cukup relevan, tolak terus tanpa panggil LLM
    // (elak halusinasi / jawapan luar dokumen TSUYU).
    if !cukup_relevan(state, &chunks) {
        return Ok(ChatResponse {
            answer: MESEJ_PENOLAKAN.to_string(),
            sources: Vec::new(),
        });
    }

    let mula = Instant::now();
    let answer = generate::generate_answer(state, question, &chunks, &history).await?;
    state.metrics.observe_generate(mula.elapsed().as_millis() as u64);

    let sources = to_sources(&chunks);

    // Simpan giliran ini untuk memori sesi (jika ada session_id & memori dihidupkan).
    if state.config.memory_enabled {
        if let Some(sid) = req.session_id.as_deref() {
            memory::save_turn(state, sid, question, &answer).await?;
        }
    }

    Ok(ChatResponse { answer, sources })
}

/// Jawab satu soalan sekali-jalan (untuk perintah CLI `ask`): retrieval → guardrail →
/// generation. Tiada memori sesi atau metrik. Pulang mesej penolakan + sumber kosong
/// jika guardrail menolak.
pub async fn jawab_soalan(
    state: &AppState,
    question: &str,
    filter: &MetaFilter,
) -> Result<ChatResponse, AppError> {
    let question = question.trim();
    if question.is_empty() {
        return Err(AppError::BadRequest("soalan tidak boleh kosong".to_string()));
    }

    let chunks = cari_konteks(state, question, filter).await?;
    if !cukup_relevan(state, &chunks) {
        return Ok(ChatResponse {
            answer: MESEJ_PENOLAKAN.to_string(),
            sources: Vec::new(),
        });
    }

    let answer = generate::generate_answer(state, question, &chunks, &[]).await?;
    Ok(ChatResponse {
        answer,
        sources: to_sources(&chunks),
    })
}

/// POST /chat/stream — alirkan jawapan sebagai Server-Sent Events.
///
/// Jujukan event:
///   1. `sources` — JSON senarai sumber rujukan (dihantar sebaik retrieval siap).
///   2. `token`   — banyak event; setiap satu satu potongan teks (dipetik JSON).
///   3. `done`    — penanda tamat.
///   4. `error`   — jika berlaku ralat semasa penjanaan.
pub async fn chat_stream(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    state.metrics.inc_chat();

    let question = req.question.trim().to_string();
    if question.is_empty() {
        state.metrics.inc_chat_error();
        return Err(AppError::BadRequest("soalan tidak boleh kosong".to_string()));
    }

    // Retrieval + rerank dibuat dahulu supaya ralat (cth. DB/reranker mati) pulang
    // sebagai status HTTP, bukan tersembunyi dalam aliran SSE.
    let history = muat_sejarah(&state, req.session_id.as_deref()).await?;
    let filter = req.filter.clone().unwrap_or_default();
    let chunks = match cari_konteks(&state, &question, &filter).await {
        Ok(c) => c,
        Err(e) => {
            state.metrics.inc_chat_error();
            return Err(e);
        }
    };
    // Guardrail: jika konteks tidak cukup relevan, jangan panggil LLM.
    let relevan = cukup_relevan(&state, &chunks);
    // Bila ditolak, jangan dedahkan sumber (konteks tidak digunakan untuk menjawab).
    let sources = if relevan { to_sources(&chunks) } else { Vec::new() };
    let session_id = req.session_id.clone();
    let gen_mula = Instant::now();

    let stream = async_stream::stream! {
        // 1. Hantar senarai sumber dahulu.
        match serde_json::to_string(&sources) {
            Ok(json) => yield Ok(Event::default().event("sources").data(json)),
            Err(e) => {
                yield Ok(Event::default().event("error").data(json_str(&e.to_string())));
                return;
            }
        }

        // Guardrail: konteks tidak relevan → pulang mesej penolakan, langkau LLM.
        if !relevan {
            yield Ok(Event::default().event("token").data(json_str(MESEJ_PENOLAKAN)));
            yield Ok(Event::default().event("done").data("[DONE]"));
            return;
        }

        // 2. Alirkan token jawapan dari Ollama (kumpul untuk disimpan ke memori).
        let mut jawapan_penuh = String::new();
        match generate::generate_answer_stream(&state, &question, &chunks, &history).await {
            Ok(tokens) => {
                pin_mut!(tokens);
                while let Some(item) = tokens.next().await {
                    match item {
                        Ok(tok) => {
                            jawapan_penuh.push_str(&tok);
                            yield Ok(Event::default().event("token").data(json_str(&tok)));
                        }
                        Err(e) => {
                            state.metrics.inc_chat_error();
                            yield Ok(Event::default().event("error").data(json_str(&e.to_string())));
                            return;
                        }
                    }
                }
                // Rekod masa penjanaan penuh (sejak retrieval siap hingga token terakhir).
                state.metrics.observe_generate(gen_mula.elapsed().as_millis() as u64);
                // 3. Simpan giliran ke memori (jika ada session_id & memori dihidupkan).
                if state.config.memory_enabled {
                    if let Some(sid) = session_id.as_deref() {
                        if let Err(e) = memory::save_turn(&state, sid, &question, &jawapan_penuh).await {
                            tracing::warn!("gagal simpan memori sesi: {e}");
                        }
                    }
                }
                // 4. Tamat.
                yield Ok(Event::default().event("done").data("[DONE]"));
            }
            Err(e) => {
                state.metrics.inc_chat_error();
                yield Ok(Event::default().event("error").data(json_str(&e.to_string())));
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Tukar chunk yang diambil kepada senarai `Source` untuk klien.
fn to_sources(chunks: &[RetrievedChunk]) -> Vec<Source> {
    chunks
        .iter()
        .map(|c| Source {
            document_id: c.document_id,
            filename: c.filename.clone(),
            chunk_index: c.chunk_index,
            page: c.page,
            snippet: snippet(&c.content),
            distance: c.distance,
            rerank_score: c.rerank_score,
            meta: c.meta.clone(),
        })
        .collect()
}

/// Petik string sebagai JSON supaya selamat dihantar dalam satu medan `data:` SSE
/// (mengelak masalah aksara baris baru di dalam token).
fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// Hasilkan petikan ringkas dari kandungan chunk untuk paparan rujukan.
/// Ruang putih dimampatkan; dipotong pada sempadan aksara (~240 aksara).
fn snippet(content: &str) -> String {
    const MAX: usize = 240;
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= MAX {
        return normalized;
    }
    let truncated: String = normalized.chars().take(MAX).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_mampat_ruang_putih() {
        assert_eq!(snippet("Cuti   tahunan\n\n20 hari"), "Cuti tahunan 20 hari");
    }

    #[test]
    fn snippet_pendek_kekal_penuh() {
        let s = "Teks pendek.";
        assert_eq!(snippet(s), s);
        assert!(!snippet(s).ends_with('…'));
    }

    #[test]
    fn snippet_panjang_dipotong() {
        let teks = "a ".repeat(500); // jauh melebihi 240 aksara
        let out = snippet(&teks);
        assert!(out.ends_with('…'));
        assert!(out.chars().count() <= 241); // 240 + elipsis
    }

    // --- Guardrail relevansi ---

    fn chunk_rerank(skor: Option<f64>, jarak: f64) -> RetrievedChunk {
        RetrievedChunk {
            id: 1,
            document_id: 1,
            filename: "f.txt".to_string(),
            chunk_index: 0,
            content: "x".to_string(),
            page: None,
            distance: jarak,
            rerank_score: skor,
            meta: crate::models::DocumentMeta::default(),
        }
    }

    #[test]
    fn relevansi_dimatikan_sentiasa_benar() {
        // enabled=false → benar walaupun tiada chunk.
        assert!(nilai_relevan(&[], false, 0.0, 1.0));
    }

    #[test]
    fn relevansi_tiada_chunk_ditolak() {
        assert!(!nilai_relevan(&[], true, 0.0, 1.0));
    }

    #[test]
    fn relevansi_rerank_atas_ambang_diterima() {
        let c = vec![chunk_rerank(Some(2.5), 0.0)];
        assert!(nilai_relevan(&c, true, 0.0, 1.0));
    }

    #[test]
    fn relevansi_rerank_bawah_ambang_ditolak() {
        let c = vec![chunk_rerank(Some(-1.0), 0.0)];
        assert!(!nilai_relevan(&c, true, 0.0, 1.0));
    }

    #[test]
    fn relevansi_vektor_jarak_dekat_diterima() {
        // Tiada rerank_score → guna jarak; 0.3 <= 1.0 ambang.
        let c = vec![chunk_rerank(None, 0.3)];
        assert!(nilai_relevan(&c, true, 0.0, 1.0));
    }

    #[test]
    fn relevansi_vektor_jarak_jauh_ditolak() {
        let c = vec![chunk_rerank(None, 1.8)];
        assert!(!nilai_relevan(&c, true, 0.0, 1.0));
    }
}
