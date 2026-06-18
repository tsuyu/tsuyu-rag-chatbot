//! Pipeline ingest: baca folder dokumen -> ekstrak teks -> chunk -> embed -> simpan.

use std::path::{Path, PathBuf};

use sqlx::Row;

use crate::db;
use crate::error::AppError;
use crate::services::{chunk, embed, metadata};
use crate::state::AppState;

/// Ringkasan hasil ingest.
#[derive(Debug, Default)]
pub struct IngestSummary {
    pub documents: usize,
    pub chunks: usize,
    pub unchanged: usize,
    pub skipped: usize,
}

/// Hasil memproses satu fail.
enum FileOutcome {
    /// Fail diproses; mengandungi bilangan chunk yang disimpan.
    Ingested(usize),
    /// Fail tidak berubah sejak ingest terakhir; dilangkau.
    Unchanged,
}

/// Proses semua fail yang disokong dalam `DOCS_DIR`.
///
/// `force = true` memaksa ingest semula walaupun fail tidak berubah.
pub async fn ingest_dir(state: &AppState, force: bool) -> Result<IngestSummary, AppError> {
    let dir = PathBuf::from(&state.config.docs_dir);
    let files = list_supported_files(&dir).await?;

    let mut summary = IngestSummary::default();

    for path in files {
        match ingest_file(state, &path, force).await {
            Ok(FileOutcome::Ingested(n_chunks)) => {
                summary.documents += 1;
                summary.chunks += n_chunks;
                tracing::info!("ingest selesai: {} ({} chunk)", path.display(), n_chunks);
            }
            Ok(FileOutcome::Unchanged) => {
                summary.unchanged += 1;
                tracing::debug!("tidak berubah, dilangkau: {}", path.display());
            }
            Err(e) => {
                // Satu fail gagal tidak boleh menghentikan keseluruhan ingest.
                summary.skipped += 1;
                tracing::warn!("langkau {}: {e}", path.display());
            }
        }
    }

    Ok(summary)
}

/// Ambil mtime fail sebagai saat epoch (jika ada).
fn mtime_secs(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

/// Ingest satu fail jika ia berubah (atau jika `force`). Pulang hasilnya.
async fn ingest_file(
    state: &AppState,
    path: &Path,
    force: bool,
) -> Result<FileOutcome, AppError> {
    let path_str = path.to_string_lossy().to_string();

    // Semakan tokokan (incremental): bandingkan saiz + masa ubah suai (mtime) dengan
    // rekod sedia ada. Jika sepadan dan bukan `force`, langkau tanpa membaca fail.
    let meta = tokio::fs::metadata(path).await?;
    let size = meta.len() as i64;
    let doc_mtime = mtime_secs(&meta);

    // Ambil kira juga mtime sidecar metadata: jika .meta.json diubah, re-ingest supaya
    // metadata baharu digunakan walaupun dokumen itu sendiri tidak berubah.
    let sidecar_mtime = match tokio::fs::metadata(metadata::sidecar_path(path)).await {
        Ok(m) => mtime_secs(&m),
        Err(_) => None,
    };
    let mtime = match (doc_mtime, sidecar_mtime) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };

    if !force {
        let existing = sqlx::query("SELECT size_bytes, mtime_unix FROM documents WHERE path = $1")
            .bind(&path_str)
            .fetch_optional(&state.db)
            .await?;

        if let Some(row) = existing {
            let prev_size: Option<i64> = row.try_get("size_bytes")?;
            let prev_mtime: Option<i64> = row.try_get("mtime_unix")?;
            if prev_size == Some(size) && mtime.is_some() && prev_mtime == mtime {
                return Ok(FileOutcome::Unchanged);
            }
        }
    }

    // Ekstrak teks per-muka-surat (PDF) atau satu blok (format lain). Setiap chunk
    // mewarisi nombor muka surat halamannya supaya rujukan boleh menunjuk muka surat.
    let pages = extract_pages(path).await?;

    // Pecah setiap muka surat jadi chunk; kekalkan nombor muka surat untuk setiap chunk.
    let mut pieces: Vec<(Option<i32>, String)> = Vec::new();
    for (page, page_text) in &pages {
        for piece in chunk::chunk_text(
            &state.tokenizer,
            page_text,
            state.config.chunk_tokens,
            state.config.chunk_overlap,
        ) {
            pieces.push((*page, piece));
        }
    }

    if pieces.is_empty() {
        return Err(AppError::BadRequest(format!(
            "fail '{}' tiada teks boleh dibaca",
            path.display()
        )));
    }

    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "tanpa-nama".to_string());

    // Muat metadata dari sidecar <dokumen>.meta.json (kosong jika tiada).
    let meta = metadata::load_for(path).await;

    // Satu transaksi setiap dokumen: upsert dokumen, buang chunk lama, masuk chunk baru.
    let mut tx = state.db.begin().await?;

    let doc_id: i64 = sqlx::query(
        r#"
        INSERT INTO documents (filename, path, size_bytes, mtime_unix, category, department, year, security)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (path) DO UPDATE SET
            filename = EXCLUDED.filename,
            size_bytes = EXCLUDED.size_bytes,
            mtime_unix = EXCLUDED.mtime_unix,
            category = EXCLUDED.category,
            department = EXCLUDED.department,
            year = EXCLUDED.year,
            security = EXCLUDED.security,
            ingested_at = now()
        RETURNING id
        "#,
    )
    .bind(&filename)
    .bind(&path_str)
    .bind(size)
    .bind(mtime)
    .bind(&meta.category)
    .bind(&meta.department)
    .bind(meta.year)
    .bind(&meta.security)
    .fetch_one(&mut *tx)
    .await?
    .try_get("id")?;

    // Buang chunk lama supaya re-ingest tidak menghasilkan pendua.
    sqlx::query("DELETE FROM chunks WHERE document_id = $1")
        .bind(doc_id)
        .execute(&mut *tx)
        .await?;

    // Proses chunk dalam kelompok: satu panggilan embedding + satu INSERT berbilang
    // baris setiap kelompok. Ini mengurangkan round-trip HTTP dan overhead DB.
    let batch_size = state.config.embed_batch_size.max(1);
    let mut stored = 0usize;

    for (batch_no, batch) in pieces.chunks(batch_size).enumerate() {
        // Embedding dijana atas teks chunk sahaja (komponen kedua tuple).
        let texts: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
        let embeddings = embed::embed_batch(state, &texts).await?;
        let base = batch_no * batch_size;

        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
            "INSERT INTO chunks (document_id, chunk_index, content, page, embedding) ",
        );

        qb.push_values(
            batch.iter().zip(embeddings.iter()).enumerate(),
            |mut row, (i, ((page, content), embedding))| {
                row.push_bind(doc_id)
                    .push_bind((base + i) as i32)
                    .push_bind(content.as_str())
                    .push_bind(*page)
                    .push_bind(db::vector_literal(embedding))
                    .push_unseparated("::vector");
            },
        );

        qb.build().execute(&mut *tx).await?;
        stored += batch.len();
    }

    tx.commit().await?;
    Ok(FileOutcome::Ingested(stored))
}

/// Senaraikan fail bersokongan dalam direktori secara **rekursif** (termasuk subfolder).
///
/// Menggunakan stack direktori eksplisit (bukan rekursi async) untuk mengelak masalah
/// `Future` bersaiz tak diketahui. Subfolder tersembunyi (bermula dengan '.') dilangkau.
async fn list_supported_files(dir: &Path) -> Result<Vec<PathBuf>, AppError> {
    let mut files = Vec::new();
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&current).await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "gagal baca folder dokumen '{}': {e}",
                current.display()
            ))
        })?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                // Langkau folder tersembunyi (cth. .git, .trash).
                let hidden = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with('.'))
                    .unwrap_or(false);
                if !hidden {
                    stack.push(path);
                }
            } else if path.is_file() && is_supported(&path) {
                files.push(path);
            }
        }
    }

    Ok(files)
}

fn is_supported(path: &Path) -> bool {
    matches!(
        ext_lower(path).as_deref(),
        Some("pdf") | Some("docx") | Some("txt") | Some("md")
    )
}

fn ext_lower(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().to_lowercase())
}

/// Ekstrak teks sebagai senarai (nombor_muka_surat, teks) mengikut jenis fail.
///
/// - PDF  : satu entri setiap muka surat, dengan nombor `Some(n)` (1-asas).
/// - Lain : satu entri tunggal dengan `None` (tiada konsep muka surat).
///
/// Operasi blocking (PDF/DOCX) dijalankan dalam spawn_blocking.
async fn extract_pages(path: &Path) -> Result<Vec<(Option<i32>, String)>, AppError> {
    let ext = ext_lower(path).unwrap_or_default();
    let path = path.to_path_buf();

    match ext.as_str() {
        "txt" | "md" => {
            let text = tokio::fs::read_to_string(&path).await?;
            Ok(vec![(None, text)])
        }
        "pdf" => run_blocking(move || extract_pdf_pages(&path)).await,
        "docx" => {
            let text = run_blocking(move || extract_docx(&path)).await?;
            Ok(vec![(None, text)])
        }
        other => Err(AppError::BadRequest(format!(
            "jenis fail '.{other}' tidak disokong"
        ))),
    }
}

/// Bantu jalankan kerja blocking pada thread pool tokio.
async fn run_blocking<F, T>(f: F) -> Result<T, AppError>
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("tugas blocking gagal: {e}")))?
}

/// Ekstrak PDF per-muka-surat. Muka surat kosong (selepas trim) dilangkau.
fn extract_pdf_pages(path: &Path) -> Result<Vec<(Option<i32>, String)>, AppError> {
    let pages = pdf_extract::extract_text_by_pages(path)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("gagal baca PDF: {e}")))?;

    let mut out = Vec::new();
    for (i, text) in pages.into_iter().enumerate() {
        if !text.trim().is_empty() {
            out.push((Some((i + 1) as i32), text));
        }
    }
    Ok(out)
}

/// DOCX ialah arkib ZIP; teks utama ada dalam `word/document.xml`.
/// Kita baca XML itu dan ambil hanya kandungan teks (buang tag).
fn extract_docx(path: &Path) -> Result<String, AppError> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;
    use std::io::Read;

    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("DOCX bukan ZIP sah: {e}")))?;

    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|e| AppError::Internal(anyhow::anyhow!("DOCX tiada word/document.xml: {e}")))?
        .read_to_string(&mut xml)?;

    let mut reader = Reader::from_str(&xml);
    let mut out = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(e)) => {
                let t = e
                    .unescape()
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("ralat XML DOCX: {e}")))?;
                out.push_str(&t);
            }
            // <w:p> menandakan perenggan -> tambah baris baru.
            Ok(Event::End(e)) if e.name().as_ref() == b"w:p" => out.push('\n'),
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(AppError::Internal(anyhow::anyhow!(
                    "gagal hurai DOCX XML: {e}"
                )))
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(out)
}
