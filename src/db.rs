//! Sambungan pangkalan data (connection pool) + penyediaan skema.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Cipta connection pool ke PostgreSQL, dengan zon waktu sesi ditetapkan ke `timezone`
/// (cth. "Asia/Kuala_Lumpur"; lihat `APP_TIMEZONE` dalam config).
///
/// Zon waktu ditetapkan pada **setiap** sambungan baharu dalam pool supaya nilai
/// `TIMESTAMPTZ` (cth. `ingested_at`, `created_at`) dirender dalam zon itu apabila
/// dipaparkan (`::text`), bukan UTC/zon-OS pelayan. Nilai disimpan dalam DB kekal UTC
/// dalaman — hanya tafsiran/paparan sesi berubah. Guna `set_config(...)` berparameter
/// (selamat untuk nilai dari konfigurasi).
pub async fn init_pool(database_url: &str, timezone: &str) -> anyhow::Result<PgPool> {
    let tz = timezone.to_string();
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .after_connect(move |conn, _meta| {
            let tz = tz.clone();
            Box::pin(async move {
                sqlx::query("SELECT set_config('TimeZone', $1, false)")
                    .bind(tz)
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await?;
    Ok(pool)
}

/// Jalankan migrasi skema (sqlx) + penyelarasan runtime.
///
/// 1. `sqlx::migrate!` menjalankan fail dalam `./migrations` secara teratur dan
///    menjejakinya dalam jadual `_sqlx_migrations` (idempotent merentas restart).
/// 2. Migrasi statik guna lalai stack (vector(1024), FTS 'simple'). Jika `EMBED_DIM`
///    atau `FTS_CONFIG` berbeza, kita selaraskan skema pada masa runtime — kerana
///    migrasi statik tidak boleh menerima parameter.
pub async fn run_migrations(pool: &PgPool, embed_dim: usize, fts_config: &str) -> anyhow::Result<()> {
    // Sanitasi nama konfigurasi FTS (hanya aksara mudah) — ia disisip terus ke SQL.
    if !fts_config.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        anyhow::bail!("FTS_CONFIG tidak sah: '{fts_config}' (hanya huruf/nombor/garis bawah)");
    }

    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| anyhow::anyhow!("gagal jalankan migrasi: {e}"))?;

    // Selaraskan dimensi embedding & konfigurasi FTS jika berbeza daripada lalai migrasi.
    reconcile_embedding_dim(pool, embed_dim).await?;
    reconcile_fts_config(pool, fts_config).await?;

    Ok(())
}

/// Pastikan lajur `chunks.embedding` menggunakan dimensi `embed_dim`. Jika berbeza,
/// kosongkan chunk dan tukar jenis lajur. Tiada tindakan jika sudah sepadan.
async fn reconcile_embedding_dim(pool: &PgPool, embed_dim: usize) -> anyhow::Result<()> {
    // atttypmod bagi jenis `vector(n)` ialah n (tiada offset seperti varchar).
    let current: Option<i32> = sqlx::query_scalar(
        r#"
        SELECT a.atttypmod
        FROM pg_attribute a
        JOIN pg_class c ON c.oid = a.attrelid
        WHERE c.relname = 'chunks' AND a.attname = 'embedding'
        "#,
    )
    .fetch_optional(pool)
    .await?;

    if let Some(dim) = current {
        if dim != embed_dim as i32 {
            tracing::warn!(
                "dimensi embedding berubah ({dim} -> {embed_dim}); membina semula lajur \
                 dan mengosongkan chunk. Sila jalankan POST /ingest?force=true selepas ini."
            );
            // Buang index dahulu (bergantung pada lajur), kosongkan data, tukar jenis.
            sqlx::query("DROP INDEX IF EXISTS chunks_embedding_idx")
                .execute(pool)
                .await?;
            sqlx::query("TRUNCATE chunks").execute(pool).await?;
            sqlx::query(&format!(
                "ALTER TABLE chunks ALTER COLUMN embedding TYPE vector({embed_dim})"
            ))
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

/// Pastikan lajur dijana `chunks.content_tsv` menggunakan `fts_config`. Migrasi lalai
/// menggunakan 'simple'; jika `FTS_CONFIG` berbeza, bina semula lajur + index.
async fn reconcile_fts_config(pool: &PgPool, fts_config: &str) -> anyhow::Result<()> {
    // Ungkapan penjanaan lajur disimpan dalam pg_get_expr; ia merujuk konfigurasi
    // melalui OID regconfig, cth. "to_tsvector('simple'::regconfig, content)".
    let expr: Option<String> = sqlx::query_scalar(
        r#"
        SELECT pg_get_expr(d.adbin, d.adrelid)
        FROM pg_attrdef d
        JOIN pg_attribute a ON a.attrelid = d.adrelid AND a.attnum = d.adnum
        JOIN pg_class c ON c.oid = d.adrelid
        WHERE c.relname = 'chunks' AND a.attname = 'content_tsv'
        "#,
    )
    .fetch_optional(pool)
    .await?;

    let needs_rebuild = match expr {
        // Jika ungkapan tidak menyebut konfigurasi semasa, bina semula.
        Some(e) => !e.contains(&format!("'{fts_config}'")),
        None => false, // lajur tiada (sepatutnya tidak berlaku selepas migrasi)
    };

    if needs_rebuild {
        tracing::warn!(
            "FTS_CONFIG berubah kepada '{fts_config}'; membina semula lajur content_tsv + index."
        );
        sqlx::query("DROP INDEX IF EXISTS chunks_content_tsv_idx")
            .execute(pool)
            .await?;
        sqlx::query("ALTER TABLE chunks DROP COLUMN IF EXISTS content_tsv")
            .execute(pool)
            .await?;
        sqlx::query(&format!(
            "ALTER TABLE chunks ADD COLUMN content_tsv tsvector \
             GENERATED ALWAYS AS (to_tsvector('{fts_config}', content)) STORED"
        ))
        .execute(pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS chunks_content_tsv_idx ON chunks USING gin (content_tsv)",
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Semakan ringkas untuk /health: pastikan DB boleh dihubungi.
pub async fn ping(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await.map(|_| ())
}

/// Tukar embedding kepada literal teks pgvector, cth. `[0.1,0.2,0.3]`.
///
/// Kita guna pendekatan teks (bukan crate `pgvector`) supaya kebergantungan
/// kekal ringan. Literal ini di-cast kepada `::vector` di dalam SQL.
pub fn vector_literal(embedding: &[f32]) -> String {
    let mut s = String::with_capacity(embedding.len() * 8 + 2);
    s.push('[');
    for (i, v) in embedding.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&v.to_string());
    }
    s.push(']');
    s
}
