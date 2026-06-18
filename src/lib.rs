//! Pustaka TSUYU RAG Chatbot — semua logik aplikasi.
//!
//! Titik masuk sebenar ialah [`run`]; `src/main.rs` hanya pembalut nipis yang
//! memulakan runtime tokio dan memanggilnya. Struktur ini membolehkan ujian
//! integrasi mengakses permukaan API melalui pustaka.
//!
//! Binari menyokong beberapa perintah (lihat [`parse_args`]):
//!   - `serve` (lalai)   : hidupkan pelayan HTTP Axum.
//!   - `ingest [--force]` : ingest dokumen sekali lalu keluar.
//!   - `check`            : pemeriksaan praterbang (DB, Ollama, model, reranker).
//!   - `stats`            : gambaran ringkas pangkalan data.
//!   - `prune-sessions`   : padam memori perbualan lama.
//!   - `ask "<soalan>"`   : pertanyaan RAG sekali-jalan.
//!
//! Urutan start (semua perintah kongsi langkah 1-5):
//!   1. Muat .env (untuk pembangunan tempatan; systemd guna EnvironmentFile).
//!   2. Mulakan logging (tracing).
//!   3. Baca konfigurasi.
//!   4. Sambung DB + jalankan skema.
//!   5. Bina state (pool, klien HTTP, tokenizer).
//!   6. serve: hidupkan pelayan Axum · selainnya: jalankan perintah lalu keluar.

pub mod auth;
pub mod cli;
pub mod config;
pub mod db;
pub mod error;
pub mod handlers;
pub mod metrics;
pub mod models;
pub mod ratelimit;
pub mod services;
pub mod state;

use anyhow::Context;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::AppState;

/// Bilangan hari lalai untuk `prune-sessions` jika `--older-than` tidak diberi.
const PRUNE_DEFAULT_DAYS: i64 = 90;

/// Mod jalanan yang ditentukan oleh argumen baris arahan.
enum Command {
    /// Hidupkan pelayan HTTP (lalai).
    Serve,
    /// Jalankan ingest sekali lalu keluar. `force` = paksa proses semula semua fail.
    Ingest { force: bool },
    /// Pemeriksaan praterbang: DB, Ollama, model, reranker.
    Check,
    /// Gambaran ringkas pangkalan data (kiraan + saiz).
    Stats,
    /// Padam memori perbualan lebih lama daripada `days` hari.
    PruneSessions { days: i64 },
    /// Pertanyaan RAG sekali-jalan; cetak jawapan + sumber.
    Ask { question: String },
}

/// Titik masuk sebenar aplikasi: hurai argumen, sediakan state, jalankan perintah.
///
/// Dipanggil oleh `main` selepas runtime tokio dimulakan. Mengembalikan ralat
/// (bukan `exit`) untuk semua kegagalan setup supaya pemanggil/ujian boleh
/// menanganinya; hanya laluan bantuan/argumen salah memanggil `exit`.
pub async fn run() -> anyhow::Result<()> {
    let command = match parse_args(std::env::args().skip(1)) {
        Ok(cmd) => cmd,
        Err(ArgError::Help) => {
            print_usage();
            return Ok(());
        }
        Err(ArgError::Unknown(arg)) => {
            eprintln!("Argumen tidak dikenali: {arg}\n");
            print_usage();
            std::process::exit(2);
        }
    };

    load_env();
    init_tracing();

    let config = Config::from_env().context("gagal baca konfigurasi")?;

    let pool = db::init_pool(
        secrecy::ExposeSecret::expose_secret(&config.database_url),
        &config.app_timezone,
    )
    .await
    .context("gagal sambung ke pangkalan data")?;
    db::run_migrations(&pool, config.embed_dim, &config.fts_config)
        .await
        .context("gagal jalankan migrasi pangkalan data")?;
    tracing::info!("pangkalan data sedia (migrasi dijalankan)");

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("gagal bina klien HTTP")?;

    // Muat tokenizer BPE sekali semasa start (digunakan untuk chunking semasa ingest).
    let tokenizer = tiktoken_rs::cl100k_base().context("gagal muat tokenizer cl100k_base")?;
    tracing::info!("tokenizer chunking sedia");

    let bind_addr = config.bind_addr.clone();
    let state = AppState::new(config, pool, http, tokenizer);

    match command {
        Command::Serve => run_server(state, bind_addr).await,
        Command::Ingest { force } => cli::ingest(&state, force).await,
        Command::Check => cli::check(&state).await,
        Command::Stats => cli::stats(&state).await,
        Command::PruneSessions { days } => cli::prune_sessions(&state, days).await,
        Command::Ask { question } => cli::ask(&state, &question).await,
    }
}

/// Hidupkan pelayan HTTP Axum dengan graceful shutdown.
async fn run_server(state: AppState, bind_addr: String) -> anyhow::Result<()> {
    tracing::info!("konfigurasi dimuat; bind ke {bind_addr}");

    if state.config.api_key.is_none() {
        tracing::warn!(
            "API_KEY tidak ditetapkan — endpoint /chat, /chat/stream dan /ingest TIDAK dilindungi"
        );
    }

    let app = handlers::router(state);

    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("gagal bind ke {bind_addr}"))?;
    tracing::info!("pelayan mendengar di http://{bind_addr}");

    // `ConnectInfo` diperlukan oleh middleware had kadar untuk dapatkan IP klien.
    let make_service = app.into_make_service_with_connect_info::<std::net::SocketAddr>();

    axum::serve(listener, make_service)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("pelayan terhenti dengan ralat")?;

    tracing::info!("pelayan ditutup dengan kemas");
    Ok(())
}

/// Ralat penghuraian argumen.
enum ArgError {
    /// Pengguna minta bantuan (`-h`/`--help`).
    Help,
    /// Argumen tidak dikenali.
    Unknown(String),
}

/// Hurai argumen baris arahan kepada `Command`. Tanpa argumen = `Serve`.
fn parse_args(args: impl Iterator<Item = String>) -> Result<Command, ArgError> {
    let args: Vec<String> = args.collect();

    // Bantuan dikesan di mana-mana, KECUALI selepas `ask` (di mana ia sebahagian soalan).
    let is_ask = args.first().map(|a| a == "ask").unwrap_or(false);
    if !is_ask && args.iter().any(|a| a == "-h" || a == "--help" || a == "help") {
        return Err(ArgError::Help);
    }

    let mut it = args.iter();
    let Some(sub) = it.next() else {
        return Ok(Command::Serve);
    };

    match sub.as_str() {
        "serve" => Ok(Command::Serve),
        "ingest" => {
            let force = it.any(|a| a == "--force" || a == "-f");
            Ok(Command::Ingest { force })
        }
        "check" => Ok(Command::Check),
        "stats" => Ok(Command::Stats),
        "prune-sessions" => {
            let mut days = PRUNE_DEFAULT_DAYS;
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--older-than" => {
                        let val = it.next().ok_or_else(|| {
                            ArgError::Unknown("--older-than perlukan nilai (hari)".to_string())
                        })?;
                        days = val.parse().map_err(|_| {
                            ArgError::Unknown(format!("bilangan hari tidak sah: {val}"))
                        })?;
                    }
                    other => return Err(ArgError::Unknown(other.to_string())),
                }
            }
            Ok(Command::PruneSessions { days })
        }
        "ask" => {
            // Semua token selepas `ask` digabung sebagai soalan (sokong tak-petik & petik).
            let question = it.cloned().collect::<Vec<_>>().join(" ");
            if question.trim().is_empty() {
                return Err(ArgError::Unknown("ask perlukan soalan".to_string()));
            }
            Ok(Command::Ask { question })
        }
        other => Err(ArgError::Unknown(other.to_string())),
    }
}

fn print_usage() {
    println!(
        "TSUYU RAG Chatbot

PENGGUNAAN:
    tsuyu-rag-chatbot [PERINTAH] [PILIHAN]

PERINTAH:
    serve                          Hidupkan pelayan HTTP (lalai jika tiada perintah).
    ingest [--force]               Ingest dokumen sekali lalu keluar.
                                   --force (-f): proses semula walau tidak berubah.
    check                          Pemeriksaan praterbang: DB, Ollama, model, reranker.
    stats                          Gambaran pangkalan data (kiraan dokumen/chunk/mesej + saiz).
    prune-sessions [--older-than N]  Padam memori perbualan > N hari (lalai 90).
    ask \"<soalan>\"                 Pertanyaan RAG sekali-jalan; cetak jawapan + sumber.

PILIHAN:
    -h, --help                     Papar bantuan ini.

CONTOH:
    tsuyu-rag-chatbot                          # hidupkan pelayan
    tsuyu-rag-chatbot ingest                   # ingest tokokan
    tsuyu-rag-chatbot ingest --force           # ingest semula semua fail
    tsuyu-rag-chatbot check                    # sahkan infra sebelum deploy
    tsuyu-rag-chatbot stats                    # statistik pangkalan data
    tsuyu-rag-chatbot prune-sessions --older-than 30
    tsuyu-rag-chatbot ask \"Apa polisi cuti tahunan?\"

NOTA:
    Semua perintah membaca konfigurasi dari .env (DATABASE_URL, DOCS_DIR, dsb.)."
    );
}

/// Tunggu isyarat penamatan: SIGTERM (systemd) atau Ctrl-C (SIGINT).
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("gagal pasang pengendali Ctrl-C: {e}");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => tracing::error!("gagal pasang pengendali SIGTERM: {e}"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("menerima Ctrl-C; memulakan penutupan"),
        _ = terminate => tracing::info!("menerima SIGTERM; memulakan penutupan"),
    }
}

/// Muat .env. Jika `APP_ENV_FILE` ditetapkan, gunakan laluan mutlak itu
/// (berguna untuk systemd jika anda mahu memuat .env secara eksplisit).
fn load_env() {
    if let Ok(path) = std::env::var("APP_ENV_FILE") {
        let _ = dotenvy::from_path(path);
    } else {
        // Abaikan jika tiada (cth. dalam systemd yang guna EnvironmentFile).
        let _ = dotenvy::dotenv();
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, ArgError> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn tiada_argumen_lalai_serve() {
        assert!(matches!(parse(&[]), Ok(Command::Serve)));
    }

    #[test]
    fn perintah_serve_eksplisit() {
        assert!(matches!(parse(&["serve"]), Ok(Command::Serve)));
    }

    #[test]
    fn ingest_tanpa_force() {
        assert!(matches!(parse(&["ingest"]), Ok(Command::Ingest { force: false })));
    }

    #[test]
    fn ingest_dengan_force() {
        assert!(matches!(parse(&["ingest", "--force"]), Ok(Command::Ingest { force: true })));
        assert!(matches!(parse(&["ingest", "-f"]), Ok(Command::Ingest { force: true })));
    }

    #[test]
    fn bantuan_dikesan() {
        assert!(matches!(parse(&["--help"]), Err(ArgError::Help)));
        assert!(matches!(parse(&["-h"]), Err(ArgError::Help)));
    }

    #[test]
    fn argumen_tak_dikenali_ralat() {
        assert!(matches!(parse(&["bogus"]), Err(ArgError::Unknown(_))));
    }

    #[test]
    fn check_dan_stats() {
        assert!(matches!(parse(&["check"]), Ok(Command::Check)));
        assert!(matches!(parse(&["stats"]), Ok(Command::Stats)));
    }

    #[test]
    fn prune_lalai_90_hari() {
        assert!(matches!(
            parse(&["prune-sessions"]),
            Ok(Command::PruneSessions { days: 90 })
        ));
    }

    #[test]
    fn prune_dengan_older_than() {
        assert!(matches!(
            parse(&["prune-sessions", "--older-than", "30"]),
            Ok(Command::PruneSessions { days: 30 })
        ));
    }

    #[test]
    fn prune_nilai_tak_sah_ralat() {
        assert!(matches!(
            parse(&["prune-sessions", "--older-than", "abc"]),
            Err(ArgError::Unknown(_))
        ));
        assert!(matches!(
            parse(&["prune-sessions", "--older-than"]),
            Err(ArgError::Unknown(_))
        ));
    }

    #[test]
    fn ask_gabung_token_jadi_soalan() {
        match parse(&["ask", "apa", "polisi", "cuti"]) {
            Ok(Command::Ask { question }) => assert_eq!(question, "apa polisi cuti"),
            other => panic!("dijangka Ask, dapat {:?}", matches!(other, Ok(Command::Ask { .. }))),
        }
    }

    #[test]
    fn ask_tanpa_soalan_ralat() {
        assert!(matches!(parse(&["ask"]), Err(ArgError::Unknown(_))));
    }

    #[test]
    fn ask_dengan_help_kekal_soalan() {
        // `--help` selepas `ask` ialah sebahagian soalan, bukan permintaan bantuan.
        assert!(matches!(parse(&["ask", "--help"]), Ok(Command::Ask { .. })));
    }
}
