//! Operasi pengurusan dokumen (senarai & padam).
//!
//! Query di sini menggunakan makro `sqlx::query!` — SQL & jenis disahkan pada masa
//! kompil terhadap skema sebenar (lihat README §Ujian untuk nota DATABASE_URL/.sqlx).

use crate::error::AppError;
use crate::models::{DocumentInfo, DocumentMeta};
use crate::state::AppState;

/// Senaraikan semua dokumen beserta bilangan chunk setiap satu.
pub async fn list_documents(state: &AppState) -> Result<Vec<DocumentInfo>, AppError> {
    // Penanda `!` memaksa bukan-null untuk lajur terbitan (cast & agregat) yang sqlx
    // anggap nullable secara lalai.
    let rows = sqlx::query!(
        r#"
        SELECT
            d.id,
            d.filename,
            d.path,
            d.size_bytes,
            d.mtime_unix,
            d.category,
            d.department,
            d.year,
            d.security,
            d.ingested_at::text AS "ingested_at!",
            COUNT(c.id) AS "chunk_count!"
        FROM documents d
        LEFT JOIN chunks c ON c.document_id = d.id
        GROUP BY d.id
        ORDER BY d.ingested_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let docs = rows
        .into_iter()
        .map(|r| DocumentInfo {
            id: r.id,
            filename: r.filename,
            path: r.path,
            size_bytes: r.size_bytes,
            mtime_unix: r.mtime_unix,
            chunk_count: r.chunk_count,
            ingested_at: r.ingested_at,
            meta: DocumentMeta {
                category: r.category,
                department: r.department,
                year: r.year,
                security: r.security,
            },
        })
        .collect();
    Ok(docs)
}

/// Padam satu dokumen mengikut id. Chunk berkaitan dipadam secara automatik
/// melalui `ON DELETE CASCADE`. Pulang bilangan baris dokumen yang dipadam (0 atau 1).
pub async fn delete_document(state: &AppState, id: i64) -> Result<u64, AppError> {
    let res = sqlx::query!("DELETE FROM documents WHERE id = $1", id)
        .execute(&state.db)
        .await?;
    Ok(res.rows_affected())
}
