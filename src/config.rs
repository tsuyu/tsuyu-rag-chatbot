//! Pemuatan konfigurasi dari persekitaran (.env atau systemd EnvironmentFile).

use anyhow::Context;
use secrecy::SecretString;

/// Konfigurasi aplikasi. Dimuat sekali semasa start dan dikongsi melalui `Arc<Config>`.
///
/// Medan sensitif (`database_url`, `api_key`, `admin_api_key`) dibungkus `SecretString`:
/// tidak dicetak oleh `Debug`, dan memorinya di-zero-kan apabila digugurkan. Akses nilai
/// sebenar hanya melalui `.expose_secret()`.
#[derive(Debug)]
pub struct Config {
    /// URL sambungan DB (mengandungi kata laluan) — sensitif.
    pub database_url: SecretString,
    pub ollama_url: String,
    pub gen_model: String,
    pub embed_model: String,
    /// Dimensi vektor embedding. bge-m3 = 1024; nomic-embed-text = 768.
    pub embed_dim: usize,
    pub docs_dir: String,
    /// Laluan fail JSON kad watak (persona) yang ditala admin.
    pub character_card_path: String,
    /// Zon waktu sesi DB untuk paparan TIMESTAMPTZ (cth. "Asia/Kuala_Lumpur").
    pub app_timezone: String,
    pub bind_addr: String,
    /// API key pengguna untuk endpoint biasa (chat, senarai dokumen) — sensitif.
    /// `None` = pengesahan dimatikan.
    pub api_key: Option<SecretString>,
    /// API key admin untuk operasi menulis/memusnah (ingest, padam dokumen/sesi) — sensitif.
    /// `None` = jatuh balik ke `api_key` (mod satu-key).
    pub admin_api_key: Option<SecretString>,
    /// Bilangan chunk akhir yang dihantar ke LLM sebagai konteks.
    pub top_k: i64,
    /// Bilangan chunk yang diambil dari pgvector SEBELUM reranking.
    /// Bila rerank dimatikan, `top_k` digunakan terus.
    pub retrieve_n: i64,
    pub chunk_tokens: usize,
    pub chunk_overlap: usize,
    pub embed_batch_size: usize,
    /// Hidupkan reranking (cross-encoder) selepas carian vektor.
    pub rerank_enabled: bool,
    /// URL servis reranker (cth. HuggingFace TEI dengan endpoint /rerank).
    pub reranker_url: String,
    pub reranker_model: String,
    /// Kawal mod "thinking" Qwen3 melalui parameter Ollama `think`.
    /// `Some(false)` matikan (disyorkan untuk RAG); `None` = jangan hantar parameter.
    pub think: Option<bool>,
    /// Hidupkan hybrid search: gabung carian vektor + kata kunci (BM25/tsvector) via RRF.
    pub hybrid_enabled: bool,
    /// Pemalar k dalam formula Reciprocal Rank Fusion (lazimnya 60).
    pub rrf_k: f64,
    /// Konfigurasi teks penuh PostgreSQL untuk tsvector (cth. "simple", "english").
    /// "simple" sesuai untuk Bahasa Malaysia (tiada stemming bahasa Inggeris).
    pub fts_config: String,
    /// Hidupkan memori perbualan (sejarah sesi disuntik ke prompt).
    pub memory_enabled: bool,
    /// Bilangan mesej terkini (giliran) untuk dimuat sebagai konteks perbualan.
    pub memory_turns: i64,
    /// Guardrail: tolak soalan (tanpa panggil LLM) jika konteks tidak cukup relevan.
    pub relevance_enabled: bool,
    /// Ambang minimum skor reranker (semakin tinggi semakin relevan). Digunakan bila
    /// reranking dihidupkan. Lalai konservatif supaya jarang tersilap tolak.
    pub relevance_min_rerank: f64,
    /// Ambang maksimum jarak cosine (semakin kecil semakin relevan). Digunakan bila
    /// reranking dimatikan (carian vektor sahaja).
    pub relevance_max_distance: f64,
    /// Bilangan cubaan semula untuk panggilan Ollama yang gagal sementara.
    /// 0 = tiada retry (sekali cubaan sahaja).
    pub ollama_max_retries: u32,
    /// Tempoh asas backoff (milisaat) — digandakan setiap cubaan (exponential).
    pub ollama_retry_base_ms: u64,
    /// Had kadar: bilangan permintaan dibenarkan per IP setiap minit. 0 = dimatikan.
    pub rate_limit_rpm: u32,
    /// Had saiz badan permintaan (bait). Lalai 2 MiB.
    pub max_body_bytes: usize,
}

impl Config {
    /// Baca konfigurasi dari pemboleh ubah persekitaran.
    ///
    /// Pemboleh ubah wajib akan menyebabkan ralat jika tiada; selebihnya ada nilai lalai.
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: SecretString::new(require("DATABASE_URL")?),
            ollama_url: optional("OLLAMA_URL", "http://localhost:11434"),
            gen_model: optional("GEN_MODEL", "qwen3:14b"),
            embed_model: optional("EMBED_MODEL", "bge-m3"),
            embed_dim: parse_or("EMBED_DIM", 1024),
            docs_dir: optional("DOCS_DIR", "./docs"),
            character_card_path: optional("CHARACTER_CARD_PATH", "character.json"),
            app_timezone: optional("APP_TIMEZONE", "Asia/Kuala_Lumpur"),
            bind_addr: optional("BIND_ADDR", "127.0.0.1:8080"),
            api_key: optional_some("API_KEY").map(SecretString::new),
            admin_api_key: optional_some("ADMIN_API_KEY").map(SecretString::new),
            top_k: parse_or("TOP_K", 5),
            retrieve_n: parse_or("RETRIEVE_N", 30),
            chunk_tokens: parse_or("CHUNK_TOKENS", 700),
            chunk_overlap: parse_or("CHUNK_OVERLAP", 100),
            embed_batch_size: parse_or("EMBED_BATCH_SIZE", 16),
            rerank_enabled: parse_or("RERANK_ENABLED", true),
            reranker_url: optional("RERANKER_URL", "http://localhost:8081"),
            reranker_model: optional("RERANKER_MODEL", "bge-reranker-v2-m3"),
            think: match optional("GEN_THINK", "false").to_lowercase().as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None, // "default" / lain-lain = jangan hantar parameter
            },
            hybrid_enabled: parse_or("HYBRID_ENABLED", true),
            rrf_k: parse_or("RRF_K", 60.0),
            fts_config: optional("FTS_CONFIG", "simple"),
            memory_enabled: parse_or("MEMORY_ENABLED", true),
            memory_turns: parse_or("MEMORY_TURNS", 6),
            relevance_enabled: parse_or("RELEVANCE_ENABLED", true),
            // bge-reranker-v2-m3 keluarkan skor logit; ~0.0 ialah ambang longgar
            // (kebanyakan padanan relevan jauh lebih tinggi, yang tak relevan negatif).
            relevance_min_rerank: parse_or("RELEVANCE_MIN_RERANK", 0.0),
            // Jarak cosine: 1.0 longgar (0=sama, 2=bertentangan). Hanya tolak yang sangat jauh.
            relevance_max_distance: parse_or("RELEVANCE_MAX_DISTANCE", 1.0),
            ollama_max_retries: parse_or("OLLAMA_MAX_RETRIES", 2),
            ollama_retry_base_ms: parse_or("OLLAMA_RETRY_BASE_MS", 500),
            rate_limit_rpm: parse_or("RATE_LIMIT_RPM", 120),
            max_body_bytes: parse_or("MAX_BODY_BYTES", 2 * 1024 * 1024),
        })
    }
}

fn require(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("pemboleh ubah persekitaran wajib '{key}' tiada"))
}

fn optional(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Baca pemboleh ubah pilihan: `Some` jika ditetapkan & bukan kosong, jika tidak `None`.
fn optional_some(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn parse_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
