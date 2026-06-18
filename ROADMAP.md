# ROADMAP — Penambahbaikan TSUYU RAG Chatbot

Checklist penambahbaikan. Item **Selesai** ditanda; item **Belum dibuat** sebagai kotak
semak untuk dilaksanakan kemudian. Disusun ikut kategori, dengan anggaran impak/usaha.

---

## ✅ Selesai

- [x] **Streaming jawapan (SSE)** — `/chat/stream`, token demi token. _(#1)_
- [x] **Batch embedding semasa ingest** — `/api/embed`, `EMBED_BATCH_SIZE`. _(#7)_
- [x] **Ingest tokokan (incremental)** — langkau fail tak berubah (saiz + mtime), `?force=true`. _(#8)_
- [x] **Pengesahan asas (API key)** — `Authorization: Bearer`, middleware, constant-time. _(#12)_
- [x] **Pengurusan dokumen** — `GET /documents`, `DELETE /documents/:id`. _(#19)_
- [x] **Reranking (cross-encoder)** — `bge-reranker-v2-m3`, retrieve-N → rerank → top-k. _(#2)_
- [x] **Dimensi embedding boleh konfig** — `EMBED_DIM`, skema diselaras automatik. _(#23)_
- [x] **Naik taraf stack model** — Qwen3 14B + bge-m3 + penapis mod thinking.
- [x] **Hybrid search** — vektor + kata kunci (`tsvector`/GIN) digabung RRF, satu DB. _(#3)_
- [x] **Memori perbualan (multi-turn)** — sejarah sesi (`session_id`) dalam PostgreSQL. _(#5)_
- [x] **Metadata chunk + penapisan** — sidecar `.meta.json` (kategori/jabatan/tahun/keselamatan),
  tapis carian via `filter`, papar dalam `sources[].meta`.
- [x] **Tokenizer sebenar untuk chunking** — token BPE (`cl100k_base`/tiktoken-rs), terbenam. _(#4)_
- [x] **Petikan lebih kaya** — `sources[]` sertakan `page` (PDF per-muka-surat) + `snippet`. _(#6)_
- [x] **Ingest rekursif** — `DOCS_DIR` dijelajah termasuk subfolder (stack eksplisit). _(#9)_
- [x] **Retry + backoff Ollama** — cubaan semula ralat sementara (embed/generate/rerank). _(#11)_
- [x] **Had kadar & saiz permintaan** — fixed-window per-IP (`RATE_LIMIT_RPM`) + `DefaultBodyLimit`. _(#13)_
- [x] **Graceful shutdown** — tangani SIGTERM (systemd) & Ctrl-C. _(#17)_
- [x] **Health check khusus model** — `/health` sahkan `GEN_MODEL` & `EMBED_MODEL` wujud. _(#20)_
- [x] **Ujian integrasi** — memori/dokumen/skema, bergerbang `TEST_DATABASE_URL`. _(#21)_
- [x] **Migrasi DB berstruktur** — migrasi sqlx terbenam + penyelarasan runtime dim/FTS. _(#16)_
- [x] **Peranan/akses berperingkat** — key pengguna (`API_KEY`) vs admin (`ADMIN_API_KEY`). _(#14)_
- [x] **Mitigasi prompt injection** — neutralkan penanda palsu + arahan "DATA bukan arahan". _(#15)_
- [x] **Metrik & pemerhatian** — `/metrics` Prometheus (kiraan chat/ingest, masa retrieval/jana). _(#18)_
- [x] **Frontend lebih lengkap** — riwayat perbualan (bubble), penunjuk menaip, Enter-hantar. _(#24)_
- [x] **Guardrail anti-halusinasi** — ambang relevansi pra-LLM (rerank/cosine), tolak awal tanpa LLM.
- [x] **Query compile-time (separa)** _(#22)_ — query CRUD selamat (documents/memory/sessions/stats)
      ditukar ke makro `sqlx::query!` (semakan SQL + jenis masa kompil). Cache offline `.sqlx`
      di-commit supaya build tetap berfungsi tanpa DB. Query retrieval (vektor/kata kunci/hybrid)
      kekal `query()` masa-jalan kerana dibina dinamik (`format!`) + lajur `vector`/`tsvector`.

---

## ⬜ Belum dibuat

### ⚡ Prestasi & skala

- [ ] **Penalaan index HNSW** _(#10)_ — tetapkan `m`/`ef_construction` & `ef_search` ikut
      saiz data untuk imbangan kelajuan/ketepatan. _(impak: sederhana · usaha: rendah)_

---

## Cadangan urutan seterusnya

1. **Penalaan index HNSW (#10)** — apabila data membesar.

Item berbaki bersifat opsyenal/bergantung-skala — teras projek sudah lengkap
untuk pengeluaran.

> Nombor `(#n)` merujuk senarai cadangan asal yang dibincangkan dalam sesi pembangunan.
