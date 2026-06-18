//! Struktur data yang dikongsi antara handler dan service.

use serde::{Deserialize, Serialize};

/// Metadata satu dokumen, dibaca dari fail sidecar `<dokumen>.meta.json`.
/// Semua medan pilihan — dokumen tanpa sidecar tetap boleh di-ingest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentMeta {
    /// Jenis dokumen, cth. "kontrak", "polisi", "perolehan", "hr".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Jabatan/bahagian TSUYU yang memiliki dokumen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    /// Tahun dokumen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    /// Tahap keselamatan, cth. "awam", "dalaman", "sulit".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<String>,
}

/// Penapis metadata untuk carian. Hanya medan yang ditetapkan akan menapis.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MetaFilter {
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub department: Option<String>,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub security: Option<String>,
}

/// Badan permintaan untuk POST /chat.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub question: String,
    /// ID sesi untuk memori perbualan. Jika `None`, memori tidak digunakan untuk
    /// permintaan ini (perbualan tanpa konteks lampau).
    #[serde(default)]
    pub session_id: Option<String>,
    /// Penapis metadata pilihan untuk mengehadkan carian (cth. kategori/tahun).
    #[serde(default)]
    pub filter: Option<MetaFilter>,
}

/// Satu mesej dalam sejarah perbualan.
#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Satu sumber rujukan yang digunakan untuk menjawab.
#[derive(Debug, Serialize)]
pub struct Source {
    pub document_id: i64,
    pub filename: String,
    pub chunk_index: i32,
    /// Nombor muka surat (1-asas) jika diketahui (cth. PDF). `None` untuk format
    /// tanpa konsep muka surat (TXT/MD/DOCX).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<i32>,
    /// Petikan ringkas kandungan chunk untuk paparan rujukan.
    pub snippet: String,
    /// Jarak cosine (semakin kecil semakin relevan).
    pub distance: f64,
    /// Skor reranker jika reranking dijalankan (semakin besar semakin relevan).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerank_score: Option<f64>,
    /// Metadata dokumen sumber (kategori, jabatan, tahun, keselamatan).
    pub meta: DocumentMeta,
}

/// Respons untuk POST /chat.
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub answer: String,
    pub sources: Vec<Source>,
}

/// Chunk yang diambil semasa retrieval (digunakan dalaman + jadi `Source`).
#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    /// ID baris chunk dalam DB (digunakan untuk dedup semasa gabungan hybrid/RRF).
    pub id: i64,
    pub document_id: i64,
    pub filename: String,
    pub chunk_index: i32,
    pub content: String,
    /// Nombor muka surat (1-asas) jika diketahui.
    pub page: Option<i32>,
    /// Jarak cosine dari carian vektor (semakin kecil semakin relevan).
    pub distance: f64,
    /// Skor relevansi dari reranker (semakin besar semakin relevan).
    /// `None` jika reranking tidak dijalankan.
    pub rerank_score: Option<f64>,
    /// Metadata dokumen sumber.
    pub meta: DocumentMeta,
}

/// Respons untuk POST /ingest.
#[derive(Debug, Serialize)]
pub struct IngestResponse {
    pub status: String,
    pub message: String,
}

/// Respons untuk GET /health.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub database: bool,
    pub ollama: bool,
    /// Status reranker. `None` jika reranking dimatikan.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reranker: Option<bool>,
    /// Status ketersediaan model di Ollama (GEN_MODEL & EMBED_MODEL).
    pub models: ModelHealth,
}

/// Ketersediaan model individu di Ollama.
#[derive(Debug, Serialize)]
pub struct ModelHealth {
    /// Adakah `GEN_MODEL` wujud dalam senarai model Ollama?
    pub gen: bool,
    /// Adakah `EMBED_MODEL` wujud dalam senarai model Ollama?
    pub embed: bool,
}

/// Maklumat satu dokumen untuk GET /documents.
#[derive(Debug, Serialize)]
pub struct DocumentInfo {
    pub id: i64,
    pub filename: String,
    pub path: String,
    pub size_bytes: Option<i64>,
    pub mtime_unix: Option<i64>,
    pub chunk_count: i64,
    pub ingested_at: String,
    /// Metadata dokumen.
    pub meta: DocumentMeta,
}

/// Respons untuk GET /documents.
#[derive(Debug, Serialize)]
pub struct DocumentListResponse {
    pub count: usize,
    pub documents: Vec<DocumentInfo>,
}

/// Respons untuk DELETE /documents/:id.
#[derive(Debug, Serialize)]
pub struct DeleteResponse {
    pub deleted: bool,
    pub id: i64,
}
