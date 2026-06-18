-- Skema pangkalan data TSUYU RAG Chatbot — RUJUKAN SAHAJA.
--
-- Sumber sebenar skema ialah fail migrasi dalam `migrations/` (dijalankan automatik
-- semasa start melalui sqlx; lihat src/db.rs::run_migrations). Fail ini disediakan
-- untuk bacaan/setup manual; ia mungkin ketinggalan jika migrasi baharu ditambah.

CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS documents (
    id          BIGSERIAL PRIMARY KEY,
    filename    TEXT NOT NULL,
    path        TEXT NOT NULL UNIQUE,
    size_bytes  BIGINT,        -- untuk ingest tokokan (incremental)
    mtime_unix  BIGINT,        -- masa ubah suai fail (epoch saat)
    -- Metadata dari sidecar <dokumen>.meta.json (semua pilihan)
    category    TEXT,          -- jenis: kontrak/polisi/perolehan/hr/...
    department  TEXT,          -- jabatan/bahagian
    year        INT,           -- tahun dokumen
    security    TEXT,          -- tahap keselamatan: awam/dalaman/sulit
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Nota: dimensi vektor mesti sepadan dengan model embedding (EMBED_DIM).
--   bge-m3            = 1024  (lalai stack semasa)
--   nomic-embed-text  = 768
-- Aplikasi menyelaraskan dimensi ini secara automatik semasa start (lihat src/db.rs).
CREATE TABLE IF NOT EXISTS chunks (
    id          BIGSERIAL PRIMARY KEY,
    document_id BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    chunk_index INT NOT NULL,
    content     TEXT NOT NULL,
    page        INT,           -- nombor muka surat (1-asas) untuk PDF; NULL jika tiada
    embedding   vector(1024) NOT NULL,
    -- Lajur full-text untuk hybrid search (BM25/tsvector), dijana automatik dari content.
    -- 'simple' = tiada stemming bahasa (sesuai untuk Bahasa Malaysia); padan FTS_CONFIG.
    content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('simple', content)) STORED
);

-- Index HNSW untuk carian similarity cosine (cipta selepas ada data untuk hasil terbaik).
CREATE INDEX IF NOT EXISTS chunks_embedding_idx
    ON chunks USING hnsw (embedding vector_cosine_ops);

-- Index GIN untuk carian kata kunci (full-text / BM25).
CREATE INDEX IF NOT EXISTS chunks_content_tsv_idx
    ON chunks USING gin (content_tsv);

-- Mesej untuk memori perbualan (sejarah sesi).
CREATE TABLE IF NOT EXISTS messages (
    id         BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    role       TEXT NOT NULL,   -- 'user' atau 'assistant'
    content    TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS messages_session_idx
    ON messages (session_id, created_at);
