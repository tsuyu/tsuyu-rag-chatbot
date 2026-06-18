# CHANGELOG — TSUYU RAG Chatbot

Semua perubahan ketara pada projek ini direkod di sini. Format berdasarkan
[Keep a Changelog](https://keepachangelog.com/), disesuaikan ke Bahasa Malaysia.

Jenis perubahan: **Ditambah** (ciri baru), **Diubah** (kelakuan sedia ada),
**Dibaiki** (pembetulan), **Keselamatan** (berkaitan keselamatan).

---

## [Belum dikeluarkan]

- **Keselamatan** Rahsia config (`DATABASE_URL`, `API_KEY`, `ADMIN_API_KEY`) dibungkus
  `secrecy::SecretString` — tidak dicetak oleh `Debug`, memori di-zero-kan apabila
  digugurkan, dan diakses hanya melalui `.expose_secret()` (di sempadan: `init_pool`
  & perbandingan key auth). Mengurangkan risiko kebocoran rahsia melalui log/dump.
- **Dibaiki** Zon waktu — connection pool menetapkan zon waktu sesi (via `set_config`
  berparameter) pada setiap sambungan, jadi `TIMESTAMPTZ` (`ingested_at`, `created_at`)
  dipaparkan dalam zon yang dipilih tanpa kira konfigurasi OS/PG pelayan (pelayan
  pengeluaran lazimnya UTC). Nilai disimpan kekal UTC dalaman. Cap masa eksport pada
  frontend ialah "bila dieksport" (waktu tempatan pelayar pengguna).
- **Ditambah** Zon waktu boleh dikonfig — `APP_TIMEZONE` (nama zon IANA, lalai
  `Asia/Kuala_Lumpur`). Dipakai pada sesi DB dan disahkan boleh ditukar (cth. `UTC`).
- **Ditambah** Kad Watak (Character Card) — persona pembantu yang ditala admin
  (nama, peranan, nada, bahasa, panjang jawapan, emoji, peraturan khas). Disimpan
  sebagai fail JSON (`CHARACTER_CARD_PATH`), disuntik ke *system prompt*, dan boleh
  diedit melalui UI `/admin` (berkuat kuasa serta-merta tanpa restart via `RwLock`).
  Endpoint: `GET /admin/character` (pengguna), `PUT /admin/character` (admin). Peraturan
  keras anti-halusinasi/anti-injection dirantai SELEPAS persona supaya tidak boleh
  diatasi. Lalai munasabah digunakan jika fail tiada.
- **Dibaiki** Susunan memori perbualan — `load_recent` kini susun ikut `id` (BIGSERIAL),
  bukan `created_at`. Kedua-dua mesej satu giliran disimpan dalam satu transaksi &
  berkongsi `now()` yang sama, jadi `created_at` tidak boleh menjamin susunan
  user→assistant. Terdedah oleh ujian integrasi DB sebenar (kali pertama dijalankan).
- **Ditambah** Semakan SQL masa-kompil separa _(#22)_ — query CRUD selamat
  (documents, memory, sessions, stats) ditukar ke makro `sqlx::query!` (SQL + jenis
  disahkan terhadap skema semasa build). Cache offline `.sqlx/` dijana (`cargo sqlx prepare`)
  & di-commit supaya `cargo build` tidak memerlukan DB. Query retrieval (vektor/kata kunci/
  hybrid) kekal `query()` masa-jalan kerana dibina dinamik + lajur `vector`/`tsvector`.
  Build offline: `SQLX_OFFLINE=true cargo build`.
- **Diubah** Struktur projek kepada pustaka + binari: `src/main.rs` kini pembalut nipis
  (`#[tokio::main]` → `tsuyu_rag_chatbot::run()`); semua logik (parse argumen, setup, router,
  dispatch perintah) berpindah ke `src/lib.rs` dengan `pub async fn run()`. Modul dijadikan
  `pub` supaya permukaan API boleh diakses ujian integrasi melalui lib crate.
- **Diubah** Ujian integrasi DB dipindahkan dari modul `#[cfg(test)]` dalam-sumber
  (`src/testutil.rs`, `src/integration_tests.rs`) ke crate ujian sebenar
  [tests/integration.rs](tests/integration.rs) yang `use tsuyu_rag_chatbot::…`. Jalankan
  dengan `cargo test --test integration` (masih bergerbang `TEST_DATABASE_URL`).
- **Ditambah** Eksport/cetak jawapan pada frontend chat — butang **Salin** (📋) pada
  setiap jawapan, **Cetak** (🖨️, dengan CSS cetak kemas), dan **Eksport** perbualan ke
  fail `.md` atau `.txt` (soalan + jawapan + rujukan). Semua sisi-klien (JavaScript) —
  tiada perubahan backend, tiada data ke luar.
- **Ditambah** UI pengurusan dokumen di `GET /admin` — dirender pelayan dengan templat
  **Askama** (dependency baharu `askama`, terbenam dalam binari, tiada fail/CDN luar).
  Memaparkan senarai dokumen + metadata + bilangan chunk, mencetus ingest (biasa/paksa),
  dan memadam dokumen. Halaman terbuka; tindakan menghantar `ADMIN_API_KEY` sebagai
  header Bearer. Laluan sokongan HTML: `GET /admin/documents`, `POST /admin/ingest`,
  `DELETE /admin/documents/:id`. Pautan ditambah pada halaman chat utama.
- **Ditambah** Perintah CLI — binari kini menyokong beberapa perintah selain `serve`
  (lalai), semua membaca `.env` yang sama & berjalan sekali tanpa pelayan/API key:
  - `ingest [--force]` — saluran ingest sama seperti `POST /ingest`; cetak ringkasan,
    kod keluar bukan-sifar jika ada fail gagal.
  - `check` — pemeriksaan praterbang (DB, Ollama, model, reranker); guna semula logik
    `/health`. Kod keluar bukan-sifar jika tidak sihat.
  - `stats` — kiraan dokumen/chunk/mesej + saiz DB.
  - `prune-sessions [--older-than N]` — padam memori perbualan > N hari (lalai 90;
    menguatkuasakan pengekalan PDPA).
  - `ask "<soalan>"` — pertanyaan RAG sekali-jalan; cetak jawapan + sumber.
  - `--help` papar penggunaan.
- **Ditambah** Unit systemd `tsuyu-rag-ingest.service` + `.timer` untuk ingest berjadual
  (lihat [deploy/](deploy/) & [RUNBOOK.md](RUNBOOK.md) §4b).
- **Diubah** Logik `/health` diekstrak ke `gather_health` supaya dikongsi antara handler
  HTTP & perintah CLI `check`. Tambah `memory::prune_older_than` & `chat::jawab_soalan`.

Dokumentasi disusun semula & dilengkapkan:
- **Ditambah** [MODEL.md](MODEL.md) — penerangan setiap model (Qwen3 14B, bge-m3,
  reranker, tokenizer) & panduan menukar model.
- **Ditambah** [RUNBOOK.md](RUNBOOK.md) — panduan operasi: log, pemantauan, sandaran &
  pemulihan (DR), senario kegagalan.
- **Ditambah** [KESELAMATAN.md](KESELAMATAN.md) — model ancaman, pengelasan data,
  senarai semak hardening, pengekalan data (PDPA), tindak balas insiden.
- **Ditambah** [PANDUAN-PENGGUNA.md](PANDUAN-PENGGUNA.md) — panduan pengguna akhir.
- **Ditambah** [PANDUAN-DOKUMEN.md](PANDUAN-DOKUMEN.md) — penyediaan & muat naik dokumen.
- **Ditambah** seksyen "Dokumen berkaitan" dalam [README.md](README.md).

---

## Sejarah pembangunan (ikut ciri)

Projek dibina secara berperingkat. Berikut ringkasan ciri utama mengikut tonggak
pembangunan (rujuk [ROADMAP.md](ROADMAP.md) untuk nombor cadangan `(#n)`).

### Guardrail anti-halusinasi
- **Ditambah** semakan relevansi pra-LLM: jika konteks tak cukup relevan (skor reranker
  atau jarak cosine), sistem menolak tanpa memanggil LLM — mengurangkan jawapan
  direka-reka. Ambang konfig: `RELEVANCE_ENABLED`, `RELEVANCE_MIN_RERANK`,
  `RELEVANCE_MAX_DISTANCE` (lalai sengaja longgar, untuk ditala selepas data sebenar).

### Frontend perbualan
- **Ditambah** antara muka chat gaya gelembung (bubble), penunjuk menaip, hantar dengan
  Enter, pengurusan dokumen & penapis metadata. _(#24)_

### Keselamatan & ketahanan
- **Keselamatan** Peranan berperingkat — `API_KEY` (pengguna) vs `ADMIN_API_KEY` (admin),
  perbandingan masa-tetap. _(#14)_
- **Keselamatan** Mitigasi prompt injection — neutralkan penanda palsu + arahan
  "DATA bukan arahan". _(#15)_
- **Ditambah** Had kadar per-IP (`RATE_LIMIT_RPM`) + had saiz badan (`MAX_BODY_BYTES`). _(#13)_
- **Ditambah** Retry + backoff untuk panggilan Ollama (embed/generate/rerank). _(#11)_
- **Ditambah** Graceful shutdown (tangani SIGTERM systemd & Ctrl-C). _(#17)_

### Pemerhatian & operasi
- **Ditambah** Health check khusus — `/health` sahkan `GEN_MODEL` & `EMBED_MODEL` wujud
  dalam Ollama. _(#20)_
- **Ditambah** Metrik Prometheus `/metrics` — kiraan chat/ingest, masa retrieval/jana. _(#18)_
- **Ditambah** Migrasi DB berstruktur (sqlx terbenam) + penyelarasan runtime dim/FTS. _(#16)_
- **Ditambah** Ujian integrasi bergerbang `TEST_DATABASE_URL`. _(#21)_

### Kualiti retrieval
- **Ditambah** Reranker cross-encoder (`bge-reranker-v2-m3` via TEI): retrieve-N →
  rerank → top-k. _(#2)_
- **Ditambah** Hybrid search — vektor (pgvector) + kata kunci (`tsvector`/GIN) digabung
  Reciprocal Rank Fusion (RRF). _(#3)_
- **Ditambah** Metadata chunk + penapisan — sidecar `.meta.json`
  (kategori/jabatan/tahun/keselamatan), tapis & papar dalam `sources[].meta`.
- **Ditambah** Tokenizer sebenar untuk chunking — BPE `cl100k_base` (tiktoken-rs),
  terbenam. _(#4)_
- **Ditambah** Petikan lebih kaya — `sources[]` sertakan nombor muka surat (PDF
  per-muka-surat) + snippet. _(#6)_
- **Ditambah** Memori perbualan multi-turn — sejarah sesi (`session_id`) dalam
  PostgreSQL. _(#5)_

### Naik taraf stack model
- **Diubah** Stack model ke **Qwen3 14B** (jana) + **bge-m3** (embed, 1024-dim) +
  reranker, dengan penapis mod *thinking* Qwen3.
- **Ditambah** Dimensi embedding boleh konfig (`EMBED_DIM`), skema diselaras automatik. _(#23)_

### Ingest
- **Ditambah** Ingest rekursif — `DOCS_DIR` diselak termasuk subfolder. _(#9)_
- **Ditambah** Ingest tokokan (incremental) — langkau fail tak berubah (saiz + mtime),
  `?force=true` untuk paksa. _(#8)_
- **Ditambah** Batch embedding semasa ingest (`/api/embed`, `EMBED_BATCH_SIZE`). _(#7)_
- **Ditambah** Pengurusan dokumen — `GET /documents`, `DELETE /documents/:id`. _(#19)_

### Asas
- **Ditambah** Streaming jawapan token-demi-token (SSE) — `/chat/stream`. _(#1)_
- **Ditambah** Pengesahan asas API key (`Authorization: Bearer`, middleware). _(#12)_
- **Ditambah** Versi awal: Axum + tokio + sqlx + PostgreSQL/pgvector + Ollama, saluran
  RAG asas (ingest → embed → retrieve → generate), Bahasa Malaysia, deploy systemd.

---

> Mulai keluaran rasmi pertama, gunakan nombor versi semantik (cth. `## [1.0.0] - 2026-xx-xx`)
> dan pindahkan entri "Belum dikeluarkan" ke bawahnya.
