-- Migrasi awal TSUYU RAG Chatbot.
--
-- Nota dimensi & FTS:
--   Lajur `chunks.embedding` dicipta dengan dimensi LALAI 1024 (bge-m3) dan
--   `content_tsv` guna konfigurasi 'simple'. Jika `EMBED_DIM`/`FTS_CONFIG` berbeza,
--   aplikasi menyelaraskannya semasa start (lihat src/db.rs::reconcile_schema) —
--   migrasi statik tidak boleh menerima parameter runtime.

CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS documents (
    id          BIGSERIAL PRIMARY KEY,
    filename    TEXT NOT NULL,
    path        TEXT NOT NULL UNIQUE,
    size_bytes  BIGINT,        -- untuk ingest tokokan (incremental)
    mtime_unix  BIGINT,        -- masa ubah suai fail (epoch saat)
    category    TEXT,          -- metadata sidecar: jenis dokumen
    department  TEXT,          -- metadata sidecar: jabatan/bahagian
    year        INT,           -- metadata sidecar: tahun dokumen
    security    TEXT,          -- metadata sidecar: tahap keselamatan
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS chunks (
    id          BIGSERIAL PRIMARY KEY,
    document_id BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    chunk_index INT NOT NULL,
    content     TEXT NOT NULL,
    page        INT,           -- nombor muka surat (1-asas) untuk PDF; NULL jika tiada
    embedding   vector(1024) NOT NULL,
    -- Lajur full-text untuk hybrid search (BM25/tsvector), dijana automatik.
    content_tsv tsvector GENERATED ALWAYS AS (to_tsvector('simple', content)) STORED
);

CREATE INDEX IF NOT EXISTS chunks_embedding_idx
    ON chunks USING hnsw (embedding vector_cosine_ops);

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
