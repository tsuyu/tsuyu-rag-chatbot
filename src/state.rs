//! Keadaan aplikasi yang dikongsi antara semua handler.

use std::sync::Arc;

use sqlx::PgPool;
use tiktoken_rs::CoreBPE;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::metrics::Metrics;
use crate::ratelimit::RateLimiter;
use crate::services::character::CharacterCard;

/// Dikongsi melalui `axum::extract::State`. Murah untuk diklon (Arc dalaman).
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub http: reqwest::Client,
    /// Tokenizer BPE untuk chunking (dimuat sekali semasa start).
    pub tokenizer: Arc<CoreBPE>,
    /// Had kadar per-IP (dikongsi).
    pub rate_limiter: Arc<RateLimiter>,
    /// Kaunter metrik untuk /metrics (dikongsi).
    pub metrics: Arc<Metrics>,
    /// Kad watak (persona) — boleh dikemas kini semasa berjalan via endpoint admin.
    pub character: Arc<RwLock<CharacterCard>>,
}

impl AppState {
    pub fn new(config: Config, db: PgPool, http: reqwest::Client, tokenizer: CoreBPE) -> Self {
        // Muat kad watak dari fail (lalai jika tiada/rosak).
        let character = CharacterCard::load(&config.character_card_path);
        Self {
            config: Arc::new(config),
            db,
            http,
            tokenizer: Arc::new(tokenizer),
            rate_limiter: Arc::new(RateLimiter::new()),
            metrics: Arc::new(Metrics::new()),
            character: Arc::new(RwLock::new(character)),
        }
    }
}
