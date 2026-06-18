//! Perintah baris arahan (CLI) selain `serve`.
//!
//! Setiap perintah berkongsi setup yang sama (config + DB + state) seperti pelayan,
//! tetapi berjalan sekali lalu keluar. Berguna untuk automasi/cron & troubleshooting
//! tanpa perlu pelayan HTTP atau API key.

use anyhow::Context;

use crate::handlers;
use crate::models::MetaFilter;
use crate::services::{ingest as ingest_svc, memory};
use crate::state::AppState;

/// `ingest [--force]` — jalankan ingest dokumen sekali, cetak ringkasan.
/// Keluar dengan kod bukan-sifar jika ada fail gagal diproses (sesuai untuk cron).
pub async fn ingest(state: &AppState, force: bool) -> anyhow::Result<()> {
    tracing::info!(
        "ingest CLI bermula (force={force}); membaca dokumen dari {}",
        state.config.docs_dir
    );

    let s = ingest_svc::ingest_dir(state, force)
        .await
        .context("ingest gagal")?;

    println!(
        "Ingest selesai: {} dokumen diproses, {} chunk disimpan, {} tidak berubah (dilangkau), {} gagal.",
        s.documents, s.chunks, s.unchanged, s.skipped
    );

    if s.skipped > 0 {
        std::process::exit(1);
    }
    Ok(())
}

/// `check` — pemeriksaan praterbang: DB, Ollama, model, reranker.
/// Keluar dengan kod bukan-sifar jika mana-mana komponen tidak sihat.
pub async fn check(state: &AppState) -> anyhow::Result<()> {
    let (all_ok, h) = handlers::health::gather_health(state).await;

    let tanda = |ok: bool| if ok { "✓" } else { "✗" };
    println!("Pemeriksaan kesihatan TSUYU RAG:");
    println!("  {} Pangkalan data", tanda(h.database));
    println!("  {} Ollama ({})", tanda(h.ollama), state.config.ollama_url);
    println!(
        "  {} Model jana   ({})",
        tanda(h.models.gen),
        state.config.gen_model
    );
    println!(
        "  {} Model embed  ({})",
        tanda(h.models.embed),
        state.config.embed_model
    );
    match h.reranker {
        Some(ok) => println!(
            "  {} Reranker ({})",
            tanda(ok),
            state.config.reranker_url
        ),
        None => println!("  - Reranker (dimatikan)"),
    }

    println!("\nStatus: {}", if all_ok { "ok" } else { "DEGRADED" });
    if !all_ok {
        std::process::exit(1);
    }
    Ok(())
}

/// `stats` — gambaran ringkas pangkalan data (kiraan + saiz).
pub async fn stats(state: &AppState) -> anyhow::Result<()> {
    // Penanda `!` memaksa bukan-null (sqlx anggap subquery agregat & pg_size_pretty nullable).
    let row = sqlx::query!(
        r#"
        SELECT
            (SELECT count(*) FROM documents) AS "docs!",
            (SELECT count(*) FROM chunks)    AS "chunks!",
            (SELECT count(*) FROM messages)  AS "msgs!",
            pg_size_pretty(pg_database_size(current_database())) AS "db_size!"
        "#,
    )
    .fetch_one(&state.db)
    .await
    .context("gagal baca statistik pangkalan data")?;

    println!("Statistik TSUYU RAG:");
    println!("  Dokumen        : {}", row.docs);
    println!("  Chunk          : {}", row.chunks);
    println!("  Mesej (memori) : {}", row.msgs);
    println!("  Saiz DB        : {}", row.db_size);
    Ok(())
}

/// `prune-sessions --older-than <hari>` — padam memori perbualan lama (dasar PDPA).
pub async fn prune_sessions(state: &AppState, days: i64) -> anyhow::Result<()> {
    if days < 0 {
        anyhow::bail!("bilangan hari tidak boleh negatif");
    }
    let dipadam = memory::prune_older_than(state, days)
        .await
        .context("gagal padam mesej lama")?;
    println!("Dipadam {dipadam} mesej lebih lama daripada {days} hari.");
    Ok(())
}

/// `ask "<soalan>"` — pertanyaan RAG sekali-jalan; cetak jawapan + sumber.
pub async fn ask(state: &AppState, question: &str) -> anyhow::Result<()> {
    let resp = handlers::chat::jawab_soalan(state, question, &MetaFilter::default())
        .await
        .context("gagal menjawab soalan")?;

    println!("{}\n", resp.answer);
    if resp.sources.is_empty() {
        println!("(tiada sumber)");
    } else {
        println!("Sumber:");
        for s in &resp.sources {
            let muka = s.page.map(|p| format!(", ms {p}")).unwrap_or_default();
            println!("  - {} (chunk {}{muka})", s.filename, s.chunk_index);
        }
    }
    Ok(())
}
