//! UI pengurusan dokumen yang dirender pelayan (Askama).
//!
//! - `GET  /admin`              : halaman penuh (terbuka — sama seperti `/`); data dimuat
//!   melalui fetch berpengesahan, jadi halaman sendiri tidak mendedahkan apa-apa.
//! - `GET  /admin/documents`    : fragmen HTML jadual dokumen (auth pengguna).
//! - `POST /admin/ingest`       : cetus ingest segerak, pulang ringkasan (auth admin).
//! - `DELETE /admin/documents/:id` : padam dokumen, pulang jadual dikemas kini (auth admin).
//! - `GET  /admin/character`    : kad watak semasa sebagai JSON (auth pengguna).
//! - `PUT  /admin/character`    : kemas kini kad watak (tulis fail + state) (auth admin).
//!
//! Tindakan tulis menggunakan endpoint berasingan ini supaya boleh memulangkan HTML
//! (untuk disuntik terus ke halaman) berbanding JSON pada `/ingest` & `/documents`.

use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::Html;
use axum::Json;

use crate::error::AppError;
use crate::handlers::ingest::IngestParams;
use crate::models::DocumentInfo;
use crate::services::character::CharacterCard;
use crate::services::{documents as doc_svc, ingest as ingest_svc};
use crate::state::AppState;

/// Halaman penuh pengurusan dokumen.
#[derive(Template)]
#[template(path = "admin.html")]
struct AdminTemplate {
    gen_model: String,
    embed_model: String,
    docs_dir: String,
    auth_required: bool,
}

/// Fragmen jadual dokumen.
#[derive(Template)]
#[template(path = "documents_table.html")]
struct DocumentsTableTemplate {
    docs: Vec<DocRow>,
    total_chunks: i64,
}

/// Satu baris dokumen, medan pra-format untuk paparan (templat kekal ringkas).
struct DocRow {
    id: i64,
    filename: String,
    path: String,
    category: String,
    department: String,
    year: String,
    security: String,
    is_sulit: bool,
    chunks: i64,
    size: String,
    ingested: String,
}

/// GET /admin — halaman penuh.
pub async fn admin_page(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let tpl = AdminTemplate {
        gen_model: state.config.gen_model.clone(),
        embed_model: state.config.embed_model.clone(),
        docs_dir: state.config.docs_dir.clone(),
        auth_required: state.config.api_key.is_some() || state.config.admin_api_key.is_some(),
    };
    render(&tpl)
}

/// GET /admin/documents — fragmen jadual.
pub async fn admin_documents(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let docs = doc_svc::list_documents(&state).await?;
    render_table(docs)
}

/// POST /admin/ingest — jalankan ingest segerak, pulang ringkasan teks biasa.
pub async fn admin_ingest(
    State(state): State<AppState>,
    Query(params): Query<IngestParams>,
) -> Result<String, AppError> {
    state.metrics.inc_ingest();
    let s = ingest_svc::ingest_dir(&state, params.force).await?;
    Ok(format!(
        "Ingest selesai: {} dokumen diproses, {} chunk disimpan, {} tidak berubah (dilangkau), {} gagal.",
        s.documents, s.chunks, s.unchanged, s.skipped
    ))
}

/// DELETE /admin/documents/:id — padam, pulang jadual dokumen dikemas kini.
pub async fn admin_delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Html<String>, AppError> {
    let affected = doc_svc::delete_document(&state, id).await?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("dokumen id {id} tidak dijumpai")));
    }
    let docs = doc_svc::list_documents(&state).await?;
    render_table(docs)
}

/// GET /admin/character — kad watak (persona) semasa sebagai JSON.
pub async fn get_character(State(state): State<AppState>) -> Json<CharacterCard> {
    let card = state.character.read().await;
    Json(card.clone())
}

/// PUT /admin/character — kemas kini persona: tulis ke fail + tukar dalam memori
/// (berkuat kuasa serta-merta untuk permintaan chat seterusnya).
pub async fn put_character(
    State(state): State<AppState>,
    Json(card): Json<CharacterCard>,
) -> Result<Json<CharacterCard>, AppError> {
    // Simpan dahulu ke fail; jika gagal, jangan ubah memori.
    card.save(&state.config.character_card_path)?;
    {
        let mut current = state.character.write().await;
        *current = card.clone();
    }
    tracing::info!("kad watak dikemas kini oleh admin");
    Ok(Json(card))
}

/// Bina fragmen jadual daripada senarai dokumen.
fn render_table(docs: Vec<DocumentInfo>) -> Result<Html<String>, AppError> {
    let total_chunks = docs.iter().map(|d| d.chunk_count).sum();
    let rows = docs.into_iter().map(to_row).collect();
    render(&DocumentsTableTemplate {
        docs: rows,
        total_chunks,
    })
}

/// Tukar `DocumentInfo` kepada baris paparan dengan medan pra-format.
fn to_row(d: DocumentInfo) -> DocRow {
    let security = d.meta.security.clone().unwrap_or_else(|| "—".to_string());
    let is_sulit = d
        .meta
        .security
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("sulit"))
        .unwrap_or(false);
    DocRow {
        id: d.id,
        filename: d.filename,
        path: d.path,
        category: d.meta.category.unwrap_or_else(|| "—".to_string()),
        department: d.meta.department.unwrap_or_else(|| "—".to_string()),
        year: d.meta.year.map(|y| y.to_string()).unwrap_or_else(|| "—".to_string()),
        security,
        is_sulit,
        chunks: d.chunk_count,
        size: format_size(d.size_bytes),
        ingested: format_masa(&d.ingested_at),
    }
}

/// Format saiz bait kepada teks ringkas (KB/MB). `None` → "—".
fn format_size(bytes: Option<i64>) -> String {
    match bytes {
        None => "—".to_string(),
        Some(b) if b < 1024 => format!("{b} B"),
        Some(b) if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        Some(b) => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

/// Pangkas cap masa `timestamptz::text` kepada "YYYY-MM-DD HH:MM".
fn format_masa(ts: &str) -> String {
    if ts.len() >= 16 {
        ts[..16].to_string()
    } else {
        ts.to_string()
    }
}

/// Render mana-mana templat Askama kepada `Html<String>`, petakan ralat ke 500.
fn render<T: Template>(tpl: &T) -> Result<Html<String>, AppError> {
    tpl.render()
        .map(Html)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("gagal render templat: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saiz_diformat() {
        assert_eq!(format_size(None), "—");
        assert_eq!(format_size(Some(512)), "512 B");
        assert_eq!(format_size(Some(2048)), "2.0 KB");
        assert_eq!(format_size(Some(3 * 1024 * 1024)), "3.0 MB");
    }

    #[test]
    fn templat_admin_render() {
        let html = AdminTemplate {
            gen_model: "qwen3:14b".to_string(),
            embed_model: "bge-m3".to_string(),
            docs_dir: "/opt/tsuyu-rag/docs".to_string(),
            auth_required: true,
        }
        .render()
        .expect("render admin");
        assert!(html.contains("Pengurusan Dokumen"));
        assert!(html.contains("qwen3:14b"));
    }

    #[test]
    fn templat_jadual_kosong_dan_isi() {
        // Kosong
        let kosong = DocumentsTableTemplate {
            docs: vec![],
            total_chunks: 0,
        }
        .render()
        .expect("render kosong");
        assert!(kosong.contains("Tiada dokumen"));

        // Berisi
        let isi = DocumentsTableTemplate {
            docs: vec![DocRow {
                id: 7,
                filename: "polisi.pdf".to_string(),
                path: "/docs/polisi.pdf".to_string(),
                category: "polisi".to_string(),
                department: "—".to_string(),
                year: "2024".to_string(),
                security: "sulit".to_string(),
                is_sulit: true,
                chunks: 12,
                size: "1.2 MB".to_string(),
                ingested: "2026-06-02 10:11".to_string(),
            }],
            total_chunks: 12,
        }
        .render()
        .expect("render isi");
        assert!(isi.contains("polisi.pdf"));
        assert!(isi.contains("data-id=\"7\""));
        assert!(isi.contains("pil sulit"));
    }

    #[test]
    fn masa_dipangkas() {
        assert_eq!(format_masa("2026-06-02 10:11:12.345+00"), "2026-06-02 10:11");
        assert_eq!(format_masa("pendek"), "pendek");
    }

    #[test]
    fn baris_guna_lalai_untuk_meta_kosong() {
        let info = DocumentInfo {
            id: 1,
            filename: "a.pdf".to_string(),
            path: "/docs/a.pdf".to_string(),
            size_bytes: None,
            mtime_unix: None,
            chunk_count: 3,
            ingested_at: "2026-01-01 00:00:00+00".to_string(),
            meta: crate::models::DocumentMeta {
                category: None,
                department: None,
                year: None,
                security: None,
            },
        };
        let row = to_row(info);
        assert_eq!(row.category, "—");
        assert_eq!(row.year, "—");
        assert!(!row.is_sulit);
    }

    #[test]
    fn tag_sulit_dikesan() {
        let mut meta = crate::models::DocumentMeta {
            category: None,
            department: None,
            year: None,
            security: Some("Sulit".to_string()),
        };
        let info = DocumentInfo {
            id: 1,
            filename: "x".to_string(),
            path: "x".to_string(),
            size_bytes: None,
            mtime_unix: None,
            chunk_count: 0,
            ingested_at: String::new(),
            meta: meta.clone(),
        };
        assert!(to_row(info).is_sulit);

        meta.security = Some("terbuka".to_string());
        let info2 = DocumentInfo {
            id: 2,
            filename: "y".to_string(),
            path: "y".to_string(),
            size_bytes: None,
            mtime_unix: None,
            chunk_count: 0,
            ingested_at: String::new(),
            meta,
        };
        assert!(!to_row(info2).is_sulit);
    }
}
