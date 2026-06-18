//! Titik masuk binari TSUYU RAG Chatbot.
//!
//! Sengaja nipis: hanya memulakan runtime tokio dan menyerahkan kepada
//! [`tsuyu_rag_chatbot::run`]. Semua logik aplikasi berada dalam pustaka (`lib.rs`).

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tsuyu_rag_chatbot::run().await
}
