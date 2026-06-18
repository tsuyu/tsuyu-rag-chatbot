//! Carian chunk relevan: vektor (pgvector), kata kunci (tsvector/BM25), dan hybrid (RRF).
//!
//! Semua carian menyokong penapis metadata pilihan (`MetaFilter`) — hanya medan yang
//! ditetapkan akan menapis, menggunakan corak `($n IS NULL OR col = $n)` supaya tiada
//! SQL dinamik diperlukan.

use std::collections::HashMap;

use sqlx::Row;

use crate::db;
use crate::error::AppError;
use crate::models::{DocumentMeta, MetaFilter, RetrievedChunk};
use crate::services::embed;
use crate::state::AppState;

/// Carian vektor: `limit` chunk paling hampir dengan soalan (jarak cosine `<=>`).
pub async fn retrieve_vector(
    state: &AppState,
    question: &str,
    limit: i64,
    filter: &MetaFilter,
) -> Result<Vec<RetrievedChunk>, AppError> {
    let query_embedding = embed::embed_text(state, question).await?;
    let vector = db::vector_literal(&query_embedding);

    // $1=vector, $2=limit, $3..$6 = penapis metadata (NULL = tiada syarat).
    let rows = sqlx::query(
        r#"
        SELECT
            c.id, c.document_id, d.filename, c.chunk_index, c.content, c.page,
            d.category, d.department, d.year, d.security,
            (c.embedding <=> $1::vector) AS distance
        FROM chunks c
        JOIN documents d ON d.id = c.document_id
        WHERE ($3::text IS NULL OR d.category   = $3)
          AND ($4::text IS NULL OR d.department = $4)
          AND ($5::int  IS NULL OR d.year       = $5)
          AND ($6::text IS NULL OR d.security   = $6)
        ORDER BY c.embedding <=> $1::vector
        LIMIT $2
        "#,
    )
    .bind(&vector)
    .bind(limit)
    .bind(&filter.category)
    .bind(&filter.department)
    .bind(filter.year)
    .bind(&filter.security)
    .fetch_all(&state.db)
    .await?;

    rows_to_chunks(rows, false)
}

/// Carian kata kunci (full-text) menggunakan tsvector + `ts_rank`.
///
/// `websearch_to_tsquery` mengendalikan input pengguna secara selamat (frasa, OR, dll.)
/// dan tidak melontar ralat untuk sintaks pelik.
pub async fn retrieve_keyword(
    state: &AppState,
    question: &str,
    limit: i64,
    filter: &MetaFilter,
) -> Result<Vec<RetrievedChunk>, AppError> {
    let cfg = &state.config.fts_config;

    // `fts_config` telah disahkan semasa run_migrations; selamat disisip sebagai literal.
    // $1=question, $2=limit, $3..$6 = penapis metadata.
    let sql = format!(
        r#"
        SELECT
            c.id, c.document_id, d.filename, c.chunk_index, c.content, c.page,
            d.category, d.department, d.year, d.security,
            ts_rank(c.content_tsv, websearch_to_tsquery('{cfg}', $1)) AS rank
        FROM chunks c
        JOIN documents d ON d.id = c.document_id
        WHERE c.content_tsv @@ websearch_to_tsquery('{cfg}', $1)
          AND ($3::text IS NULL OR d.category   = $3)
          AND ($4::text IS NULL OR d.department = $4)
          AND ($5::int  IS NULL OR d.year       = $5)
          AND ($6::text IS NULL OR d.security   = $6)
        ORDER BY rank DESC
        LIMIT $2
        "#
    );

    let rows = sqlx::query(&sql)
        .bind(question)
        .bind(limit)
        .bind(&filter.category)
        .bind(&filter.department)
        .bind(filter.year)
        .bind(&filter.security)
        .fetch_all(&state.db)
        .await?;

    // Untuk hasil kata kunci, `distance` tidak bermakna; disimpan 0 sebagai placeholder.
    rows_to_chunks(rows, true)
}

/// Hybrid search: gabung kedudukan carian vektor + kata kunci menggunakan RRF.
pub async fn retrieve_hybrid(
    state: &AppState,
    question: &str,
    limit: i64,
    filter: &MetaFilter,
) -> Result<Vec<RetrievedChunk>, AppError> {
    // Ambil lebih banyak calon dari setiap sumber supaya RRF ada ruang menggabung.
    let per_source = (limit * 2).max(limit);
    let (vec_res, kw_res) = tokio::join!(
        retrieve_vector(state, question, per_source, filter),
        retrieve_keyword(state, question, per_source, filter),
    );
    let vec_res = vec_res?;
    let kw_res = kw_res?;

    Ok(rrf_merge(vec![vec_res, kw_res], state.config.rrf_k, limit as usize))
}

/// Gabung beberapa senarai terkedudukan menggunakan Reciprocal Rank Fusion.
///
/// skor(chunk) = Σ 1 / (k + kedudukan + 1) merentas setiap senarai. Chunk yang
/// dikenali melalui `id`; pulang sehingga `limit` chunk dengan skor tertinggi.
/// Fungsi tulen — mudah diuji tanpa DB.
fn rrf_merge(lists: Vec<Vec<RetrievedChunk>>, k: f64, limit: usize) -> Vec<RetrievedChunk> {
    let mut scores: HashMap<i64, f64> = HashMap::new();
    let mut by_id: HashMap<i64, RetrievedChunk> = HashMap::new();

    for list in lists {
        for (rank, c) in list.into_iter().enumerate() {
            *scores.entry(c.id).or_insert(0.0) += 1.0 / (k + (rank as f64) + 1.0);
            by_id.entry(c.id).or_insert(c);
        }
    }

    let mut ranked: Vec<(i64, f64)> = scores.into_iter().collect();
    // Susun ikut skor menurun; pecah seri ikut id menaik untuk hasil deterministik.
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });

    let mut out = Vec::with_capacity(ranked.len().min(limit));
    for (id, _) in ranked.into_iter().take(limit) {
        if let Some(c) = by_id.remove(&id) {
            out.push(c);
        }
    }
    out
}

/// Pintu masuk retrieval: pilih hybrid atau vektor sahaja mengikut konfigurasi.
pub async fn retrieve(
    state: &AppState,
    question: &str,
    limit: i64,
    filter: &MetaFilter,
) -> Result<Vec<RetrievedChunk>, AppError> {
    if state.config.hybrid_enabled {
        retrieve_hybrid(state, question, limit, filter).await
    } else {
        retrieve_vector(state, question, limit, filter).await
    }
}

/// Tukar baris SQL kepada `RetrievedChunk`. `keyword` menentukan placeholder distance.
fn rows_to_chunks(
    rows: Vec<sqlx::postgres::PgRow>,
    keyword: bool,
) -> Result<Vec<RetrievedChunk>, AppError> {
    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let distance = if keyword { 0.0 } else { row.try_get("distance")? };
        results.push(RetrievedChunk {
            id: row.try_get("id")?,
            document_id: row.try_get("document_id")?,
            filename: row.try_get("filename")?,
            chunk_index: row.try_get("chunk_index")?,
            content: row.try_get("content")?,
            page: row.try_get("page")?,
            distance,
            rerank_score: None,
            meta: DocumentMeta {
                category: row.try_get("category")?,
                department: row.try_get("department")?,
                year: row.try_get("year")?,
                security: row.try_get("security")?,
            },
        });
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(id: i64) -> RetrievedChunk {
        RetrievedChunk {
            id,
            document_id: 1,
            filename: "f.txt".to_string(),
            chunk_index: 0,
            content: format!("chunk {id}"),
            page: None,
            distance: 0.0,
            rerank_score: None,
            meta: DocumentMeta::default(),
        }
    }

    #[test]
    fn rrf_chunk_dalam_kedua_senarai_paling_atas() {
        // id 2 muncul tinggi dalam kedua-dua senarai -> patut menang.
        let vektor = vec![ch(1), ch(2), ch(3)];
        let kunci = vec![ch(2), ch(4), ch(5)];
        let hasil = rrf_merge(vec![vektor, kunci], 60.0, 10);
        assert_eq!(hasil[0].id, 2);
    }

    #[test]
    fn rrf_dedup_tiada_pendua() {
        let vektor = vec![ch(1), ch(2)];
        let kunci = vec![ch(2), ch(1)];
        let hasil = rrf_merge(vec![vektor, kunci], 60.0, 10);
        assert_eq!(hasil.len(), 2);
    }

    #[test]
    fn rrf_hormati_limit() {
        let vektor = vec![ch(1), ch(2), ch(3), ch(4)];
        let kunci = vec![ch(5), ch(6)];
        let hasil = rrf_merge(vec![vektor, kunci], 60.0, 3);
        assert_eq!(hasil.len(), 3);
    }

    #[test]
    fn rrf_senarai_kosong_pulang_kosong() {
        let hasil = rrf_merge(vec![vec![], vec![]], 60.0, 5);
        assert!(hasil.is_empty());
    }
}
