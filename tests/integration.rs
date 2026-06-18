//! Ujian integrasi terhadap PostgreSQL sebenar, melalui **lib crate**
//! (`tsuyu_rag_chatbot`) — mengakses permukaan API awam seperti pengguna luar.
//!
//! Bergerbang oleh `TEST_DATABASE_URL`: jika env tidak ditetapkan, setiap ujian
//! pulang awal (dilangkau bersih) supaya `cargo test` lulus di mana-mana. Memerlukan
//! pgvector dipasang pada DB ujian (`CREATE EXTENSION vector`).
//!
//! **WAJIB jalan bersiri** (`--test-threads=1`): setiap `test_state()` mengosongkan
//! jadual yang dikongsi, jadi pelaksanaan selari akan berlanggar. Contoh:
//!
//! ```bash
//! TEST_DATABASE_URL=postgres://tsuyu:password@localhost/tsuyu_rag_test \
//!     cargo test --test integration -- --test-threads=1
//! ```

use tsuyu_rag_chatbot::config::Config;
use tsuyu_rag_chatbot::services::{documents, memory};
use tsuyu_rag_chatbot::state::AppState;
use tsuyu_rag_chatbot::db;

// ---------------------------------------------------------------------------
// Penyokong ujian (dulu `src/testutil.rs`) — kini sebahagian crate ujian.
// ---------------------------------------------------------------------------

/// Bina `Config` lalai untuk ujian dengan `database_url` diberi. Nilai lain ialah
/// lalai munasabah; ujian yang menyentuh DB sahaja tidak bergantung padanya.
fn test_config(database_url: String) -> Config {
    Config {
        database_url: secrecy::SecretString::new(database_url),
        ollama_url: "http://localhost:11434".to_string(),
        gen_model: "test-gen".to_string(),
        embed_model: "test-embed".to_string(),
        embed_dim: 8, // kecil — ujian DB tidak menjana embedding sebenar
        docs_dir: "./docs".to_string(),
        character_card_path: "character.json".to_string(),
        app_timezone: "Asia/Kuala_Lumpur".to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        api_key: None,
        admin_api_key: None,
        top_k: 5,
        retrieve_n: 30,
        chunk_tokens: 700,
        chunk_overlap: 100,
        embed_batch_size: 16,
        rerank_enabled: false,
        reranker_url: "http://localhost:8081".to_string(),
        reranker_model: "test-rerank".to_string(),
        think: Some(false),
        hybrid_enabled: true,
        rrf_k: 60.0,
        fts_config: "simple".to_string(),
        memory_enabled: true,
        memory_turns: 6,
        relevance_enabled: false,
        relevance_min_rerank: 0.0,
        relevance_max_distance: 1.0,
        ollama_max_retries: 0,
        ollama_retry_base_ms: 1,
        rate_limit_rpm: 0,
        max_body_bytes: 2 * 1024 * 1024,
    }
}

/// Sediakan `AppState` ujian yang bersambung ke `TEST_DATABASE_URL` dan menjalankan
/// skema. Pulang `None` jika env tidak ditetapkan (ujian patut dilangkau).
///
/// Setiap panggilan **mengosongkan** jadual `documents` & `messages` supaya ujian
/// bermula dari keadaan bersih (chunks dipadam melalui cascade documents).
async fn test_state() -> Option<AppState> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;

    let config = test_config(url);
    let pool = db::init_pool(
        secrecy::ExposeSecret::expose_secret(&config.database_url),
        &config.app_timezone,
    )
    .await
    .expect("sambung ke TEST_DATABASE_URL");
    db::run_migrations(&pool, config.embed_dim, &config.fts_config)
        .await
        .expect("jalankan migrasi ujian");

    // Bersihkan data ujian terdahulu.
    sqlx::query("DELETE FROM documents")
        .execute(&pool)
        .await
        .expect("kosongkan documents");
    sqlx::query("DELETE FROM messages")
        .execute(&pool)
        .await
        .expect("kosongkan messages");

    let http = reqwest::Client::new();
    let tokenizer = tiktoken_rs::cl100k_base().expect("muat tokenizer");
    Some(AppState::new(config, pool, http, tokenizer))
}

/// Sisip satu dokumen ujian terus ke DB (memintas ingest/embedding). Pulang id dokumen.
async fn insert_test_document(state: &AppState, filename: &str, path: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO documents (filename, path) VALUES ($1, $2) RETURNING id",
    )
    .bind(filename)
    .bind(path)
    .fetch_one(&state.db)
    .await
    .expect("sisip dokumen ujian")
}

/// Makro kecil: dapatkan state ujian atau `return` (langkau) jika DB tiada.
macro_rules! state_or_skip {
    () => {
        match test_state().await {
            Some(s) => s,
            None => {
                eprintln!("LANGKAU: TEST_DATABASE_URL tidak ditetapkan");
                return;
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Memori perbualan
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memori_simpan_dan_muat_kronologi() {
    let state = state_or_skip!();
    let sid = "sesi-ujian-1";

    memory::save_turn(&state, sid, "soalan pertama", "jawapan pertama")
        .await
        .unwrap();
    memory::save_turn(&state, sid, "soalan kedua", "jawapan kedua")
        .await
        .unwrap();

    let msgs = memory::load_recent(&state, sid, 10).await.unwrap();
    // 2 giliran = 4 mesej (user/assistant × 2), susunan kronologi.
    assert_eq!(msgs.len(), 4);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].content, "soalan pertama");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[3].content, "jawapan kedua");
}

#[tokio::test]
async fn memori_hadkan_bilangan_giliran() {
    let state = state_or_skip!();
    let sid = "sesi-ujian-had";

    for i in 0..5 {
        memory::save_turn(&state, sid, &format!("q{i}"), &format!("a{i}"))
            .await
            .unwrap();
    }

    // Minta 2 mesej terkini sahaja.
    let msgs = memory::load_recent(&state, sid, 2).await.unwrap();
    assert_eq!(msgs.len(), 2);
    // Mesej terkini ialah jawapan giliran terakhir.
    assert_eq!(msgs[1].content, "a4");
}

#[tokio::test]
async fn memori_clear_padam_sesi_itu_sahaja() {
    let state = state_or_skip!();

    memory::save_turn(&state, "sesi-a", "qa", "aa").await.unwrap();
    memory::save_turn(&state, "sesi-b", "qb", "ab").await.unwrap();

    let dipadam = memory::clear_session(&state, "sesi-a").await.unwrap();
    assert_eq!(dipadam, 2); // user + assistant

    assert!(memory::load_recent(&state, "sesi-a", 10).await.unwrap().is_empty());
    // Sesi lain tidak terjejas.
    assert_eq!(memory::load_recent(&state, "sesi-b", 10).await.unwrap().len(), 2);
}

#[tokio::test]
async fn memori_sesi_baharu_kosong() {
    let state = state_or_skip!();
    let msgs = memory::load_recent(&state, "sesi-tak-wujud", 10).await.unwrap();
    assert!(msgs.is_empty());
}

#[tokio::test]
async fn memori_prune_buang_yang_lama_sahaja() {
    let state = state_or_skip!();

    memory::save_turn(&state, "sesi-prune", "q", "a").await.unwrap();
    // Tiada mesej lebih lama dari 1 hari → tiada yang dipadam.
    let dipadam = memory::prune_older_than(&state, 1).await.unwrap();
    assert_eq!(dipadam, 0);
    assert_eq!(memory::load_recent(&state, "sesi-prune", 10).await.unwrap().len(), 2);
}

// ---------------------------------------------------------------------------
// Pengurusan dokumen
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dokumen_senarai_dan_padam() {
    let state = state_or_skip!();

    let id1 = insert_test_document(&state, "a.pdf", "/docs/a.pdf").await;
    let _id2 = insert_test_document(&state, "b.txt", "/docs/b.txt").await;

    let docs = documents::list_documents(&state).await.unwrap();
    assert_eq!(docs.len(), 2);

    // Padam satu; pulang 1 baris terjejas.
    let affected = documents::delete_document(&state, id1).await.unwrap();
    assert_eq!(affected, 1);

    let docs = documents::list_documents(&state).await.unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].filename, "b.txt");
}

#[tokio::test]
async fn dokumen_padam_tak_wujud_pulang_sifar() {
    let state = state_or_skip!();
    let affected = documents::delete_document(&state, 999_999).await.unwrap();
    assert_eq!(affected, 0);
}

#[tokio::test]
async fn dokumen_padam_cascade_chunk() {
    let state = state_or_skip!();

    let doc_id = insert_test_document(&state, "c.pdf", "/docs/c.pdf").await;

    // Sisip satu chunk dengan embedding bersaiz EMBED_DIM (8) terus ke DB.
    let vektor = "[0,0,0,0,0,0,0,0]";
    sqlx::query(
        "INSERT INTO chunks (document_id, chunk_index, content, page, embedding) \
         VALUES ($1, 0, 'kandungan ujian', 1, $2::vector)",
    )
    .bind(doc_id)
    .bind(vektor)
    .execute(&state.db)
    .await
    .unwrap();

    let kiraan: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE document_id = $1")
        .bind(doc_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(kiraan, 1);

    // Padam dokumen — chunk patut hilang melalui ON DELETE CASCADE.
    documents::delete_document(&state, doc_id).await.unwrap();
    let kiraan: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE document_id = $1")
        .bind(doc_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(kiraan, 0);
}

// ---------------------------------------------------------------------------
// Skema
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migrasi_idempoten_boleh_jalan_berulang() {
    let state = state_or_skip!();
    // run_migrations sudah dipanggil sekali dalam test_state; jalankan sekali lagi.
    // sqlx menjejak migrasi yang telah dijalankan, jadi panggilan kedua tidak gagal.
    db::run_migrations(&state.db, state.config.embed_dim, &state.config.fts_config)
        .await
        .expect("migrasi boleh dijalankan berulang tanpa ralat");
}
