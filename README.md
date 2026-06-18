# TSUYU RAG Chatbot

Chatbot RAG (Retrieval-Augmented Generation) dalaman untuk **TSUYU**. Dibina supaya
**semua data kekal on-premise** (tiada API luar) dan berinteraksi dalam **Bahasa Malaysia**.

Pengguna bertanya soalan → sistem cari petikan dokumen paling relevan → bina prompt
dengan konteks tersebut → hantar ke model LLM tempatan (Ollama) → pulang jawapan
beserta senarai dokumen rujukan.

---

## Kandungan

- [Senibina](#senibina)
- [Stack teknologi](#stack-teknologi)
- [Keperluan sistem](#keperluan-sistem)
- [Cadangan perkakasan (hardware)](#cadangan-perkakasan-hardware)
- [Pemasangan & setup](#pemasangan--setup)
- [Konfigurasi (.env)](#konfigurasi-env)
- [Menjalankan aplikasi](#menjalankan-aplikasi)
- [Pengesahan (authentication)](#pengesahan-authentication)
- [API endpoint](#api-endpoint)
- [Aliran kerja biasa](#aliran-kerja-biasa)
- [Metadata dokumen](#metadata-dokumen)
- [Guardrail anti-halusinasi](#guardrail-anti-halusinasi)
- [Kad Watak (persona)](#kad-watak-persona)
- [Struktur projek](#struktur-projek)
- [Ujian](#ujian)
- [Deploy ke Ubuntu (systemd)](#deploy-ke-ubuntu-systemd)
- [Penyelesaian masalah](#penyelesaian-masalah)
- [Nota reka bentuk](#nota-reka-bentuk)
- [Dokumen berkaitan](#dokumen-berkaitan)

---

## Dokumen berkaitan

| Dokumen | Untuk siapa | Kandungan |
|---|---|---|
| [MODEL.md](MODEL.md) | Pembangun / penilai | Penerangan setiap model (Qwen3 14B, bge-m3, reranker, tokenizer) & cara menukar |
| [RUNBOOK.md](RUNBOOK.md) | Staf IT / operasi | Operasi harian, log, pemantauan, sandaran & pemulihan, senario kegagalan |
| [KESELAMATAN.md](KESELAMATAN.md) | Keselamatan ICT / audit | Model ancaman, pengelasan data, hardening, pengekalan data (PDPA), insiden |
| [PANDUAN-PENGGUNA.md](PANDUAN-PENGGUNA.md) | Pengguna akhir | Cara bertanya, membaca jawapan & sumber, had sistem, FAQ |
| [PANDUAN-DOKUMEN.md](PANDUAN-DOKUMEN.md) | Staf muat naik dokumen | Format disokong, kualiti PDF/OCR, metadata sidecar, cara ingest |
| [SETUP-DEV-TEMPATAN.md](SETUP-DEV-TEMPATAN.md) | Pembangun | Setup dev CPU-sahaja (model kecil) — langkah pasang + pengesahan |
| [CHANGELOG.md](CHANGELOG.md) | Semua | Sejarah perubahan & ciri ikut keluaran |
| [ROADMAP.md](ROADMAP.md) | Pembangun | Status ciri & cadangan penambahbaikan |

---

## Senibina

```
                    ┌──────────────┐
   Pengguna ───────▶│  Frontend    │  (htmx ringkas, GET /)
                    │  HTML        │
                    └──────┬───────┘
                           │ POST /chat { question }
                           ▼
   ┌───────────────────────────────────────────────────────────┐
   │                  Rust + Axum (API)                        │
   │                                                           │
   │   /chat ─▶ embed soalan ─▶ HYBRID retrieve N:             │
   │                              vektor + kata kunci → RRF    │
   │                                │                          │
   │                                ▼                          │
   │                          RERANK (top-N → top-k)           │
   │                                │                          │
   │                                ▼                          │
   │                       bina prompt (BM) ─▶ generate        │
   │   /ingest ─▶ baca dokumen ─▶ chunk ─▶ embed (batch) ─▶ DB │
   │   /health ─▶ ping DB + Ollama + reranker                  │
   └──────┬──────────────────┬─────────────────────┬───────────┘
          │                  │                     │
          ▼                  ▼                     ▼
   ┌───────────────┐  ┌───────────────┐   ┌────────────────┐
   │  PostgreSQL   │  │    Ollama     │   │   Reranker     │
   │ pgvector+tsv  │  │ Qwen3 + bge-m3│   │ bge-reranker   │
   └───────────────┘  └───────────────┘   └────────────────┘
```

**Aliran RAG (POST /chat):**
1. Jana embedding untuk soalan pengguna (Ollama `bge-m3`, 1024 dimensi).
2. **Hybrid search** — cari `RETRIEVE_N` calon secara selari:
   - **Vektor**: jarak cosine pgvector (`<=>`) — relevansi semantik.
   - **Kata kunci**: full-text PostgreSQL (`tsvector`/`ts_rank`) — padanan istilah tepat.
   - Gabung kedua-dua kedudukan dengan **Reciprocal Rank Fusion (RRF)**.
3. **Rerank** calon dengan cross-encoder (`bge-reranker-v2-m3`) → ambil `TOP_K` terbaik.
4. Bina prompt Bahasa Malaysia yang menyuntik konteks chunk.
5. Hantar prompt ke model jana (`qwen3:14b`) melalui Ollama (mod thinking dimatikan).
6. Pulang `{ answer, sources[] }` (token demi token jika guna `/chat/stream`).

> **Boleh dimatikan secara berasingan:** `HYBRID_ENABLED=false` (vektor sahaja, tanpa
> kata kunci) dan `RERANK_ENABLED=false` (langkau langkah rerank). Kedua-dua boleh
> dimatikan untuk pipeline paling ringkas (vektor → jana).

---

## Stack teknologi

| Lapisan         | Pilihan                          | Nota |
|-----------------|----------------------------------|------|
| API             | Rust + Axum + tokio              | async, systemd service |
| DB driver       | sqlx 0.8 (PostgreSQL)            | connection pool |
| Vector store    | PostgreSQL 16 + pgvector         | satu DB untuk metadata + vector |
| LLM runtime     | Ollama                           | port 11434 |
| Model jana      | `qwen3:14b`                      | Q4_K_M, mod thinking dimatikan |
| Model embedding | `bge-m3`                         | 1024 dimensi, multilingual (BM) |
| Reranker        | `bge-reranker-v2-m3` (TEI/Infinity)| cross-encoder, endpoint `/rerank` |
| Baca dokumen    | `pdf-extract`, `zip`, `quick-xml`| PDF / DOCX / TXT / MD |
| Frontend        | HTML + JS (vanilla)              | sembang bubble, streaming, penunjuk menaip |

---

## Keperluan sistem

- **Rust** (toolchain stabil; projek diuji pada rustc 1.92)
- **PostgreSQL 16** dengan sambungan **pgvector**
- **Ollama** dengan model embedding & jana yang telah di-`pull`

> Nota versi: projek ini **tidak** menggunakan crate `pgvector`. Embedding disimpan
> sebagai literal teks `'[...]'::vector` supaya kekal serasi dengan sqlx 0.8 dan
> rustc 1.92. (Crate `pgvector` terkini menarik masuk sqlx 0.9 yang memerlukan rustc 1.94.)

---

## Cadangan perkakasan (hardware)

Faktor penentu utama ialah **model LLM Ollama** — ia yang paling banyak makan RAM/VRAM.
Aplikasi Rust (Axum) dan PostgreSQL agak ringan berbanding model.

### Cadangan ikut skala penggunaan

Stack semasa (Qwen3 14B + bge-m3 + reranker) menjalankan **tiga model** serentak,
jadi keperluan VRAM lebih tinggi daripada stack ringkas.

| Skala                         | CPU            | RAM     | GPU (digalakkan)            | Stack model | Cakera (SSD) |
|-------------------------------|----------------|---------|------------------------------|-------------|--------------|
| **Minimum** (ujian / demo)    | 8 teras        | 16 GB   | NVIDIA 12 GB VRAM            | `qwen3:8b` + bge-m3 (rerank off) | 40 GB |
| **Disyorkan** (pejabat kecil) | 12 teras       | 32 GB   | NVIDIA 16 GB VRAM            | `qwen3:14b` + bge-m3 + reranker  | 80 GB |
| **Optimum** (ramai pengguna)  | 16+ teras      | 64 GB   | NVIDIA 24 GB VRAM (cth. RTX 4090/A5000) | `qwen3:14b` + bge-m3 + reranker | 150 GB+ |

> **Petua:** tanpa GPU, model masih boleh jalan atas CPU tetapi jawapan akan **lebih
> perlahan** (terutama Qwen3 14B). GPU NVIDIA dengan VRAM mencukupi memberi peningkatan
> kelajuan paling ketara. Jika VRAM terhad, kekalkan **bge-m3** (ringan, faedah BM besar)
> tetapi turunkan model jana ke `qwen3:8b` dan pertimbang `RERANK_ENABLED=false`.

### Anggaran penggunaan memori model (kuantisasi Q4_K_M)

| Model                  | Saiz fail | RAM/VRAM minimum semasa jalan |
|------------------------|-----------|-------------------------------|
| `qwen3:8b`             | ~5 GB     | ~7–9 GB                       |
| `qwen3:14b`            | ~9 GB     | ~12–16 GB                     |
| `bge-m3` (embedding)   | ~1.2 GB   | ~2 GB                         |
| `bge-reranker-v2-m3`   | ~1.1 GB   | ~2 GB (servis berasingan)     |

RAM/VRAM diperlukan = jumlah saiz semua model aktif + ruang untuk konteks (KV cache).
Untuk stack penuh (Qwen3 14B + bge-m3 + reranker), sasarkan **≥16 GB VRAM**.

### Nota GPU

- **NVIDIA (CUDA)** ialah sokongan paling matang untuk Ollama. Kad seperti RTX 3060 12GB,
  RTX 4060 Ti 16GB, atau RTX 4090 sesuai mengikut belanjawan.
- **AMD (ROCm)** disokong pada GPU tertentu, tetapi pengesahan keserasian lebih rumit.
- **Apple Silicon (Mac)** berfungsi baik untuk pembangunan, tetapi pelayan TSUYU dijangka
  Ubuntu — utamakan GPU NVIDIA untuk pengeluaran.
- Pastikan **VRAM ≥ saiz model**. Jika model lebih besar daripada VRAM, Ollama akan
  "offload" sebahagian ke RAM/CPU dan menjadi jauh lebih perlahan.

### Storan

- **Cakera model**: setiap model disimpan dalam `~/.ollama/models` (lihat saiz di atas).
- **Pangkalan data**: embedding 1024-dimensi (bge-m3) ≈ **4 KB seunit chunk**. Sebagai
  panduan kasar, ~1 juta chunk ≈ beberapa GB (vektor + teks + index HNSW). Saiz dokumen
  sumber asal tidak disimpan dalam DB (hanya teks chunk + metadata).
- Gunakan **SSD/NVMe** untuk PostgreSQL bagi prestasi carian vektor yang baik.

### Rumusan ringkas

> Untuk stack penuh TSUYU, sasaran yang seimbang:
> **12-teras CPU, 32 GB RAM, GPU NVIDIA 16 GB VRAM, SSD 80 GB**, menjalankan
> `qwen3:14b` + `bge-m3` + `bge-reranker-v2-m3`. Jika VRAM ≤12 GB, guna `qwen3:8b`
> dan/atau matikan reranker.

---

## Pemasangan & setup

### 1. Pasang kebergantungan sistem

```bash
# PostgreSQL + pgvector
sudo apt update
sudo apt install -y postgresql postgresql-16-pgvector

# Ollama
curl -fsSL https://ollama.com/install.sh | sh
```

### 2. Sediakan model Ollama

```bash
ollama pull qwen3:14b
ollama pull bge-m3
sudo systemctl status ollama   # Ollama jadi systemd service selepas install
```

### 3. Sediakan servis reranker

Reranker ialah cross-encoder yang **tidak** disajikan oleh Ollama, jadi ia berjalan
sebagai servis berasingan dengan endpoint `/rerank` (serasi HuggingFace TEI). Contoh
guna Docker (`text-embeddings-inference`):

```bash
docker run --gpus all -p 8081:80 \
  ghcr.io/huggingface/text-embeddings-inference:latest \
  --model-id BAAI/bge-reranker-v2-m3
```

Servis ini sepatutnya mendedahkan `POST /rerank` dengan badan
`{ "query": "...", "texts": ["...", ...] }`. Tetapkan `RERANKER_URL` ke alamatnya
(lalai `http://localhost:8081`).

> Tiada GPU/servis reranker? Tetapkan `RERANK_ENABLED=false` dalam `.env` — sistem akan
> guna carian vektor sahaja (kualiti sedikit kurang tetapi tetap berfungsi).

### 4. Sediakan pangkalan data

```bash
sudo -u postgres psql <<'SQL'
CREATE DATABASE tsuyu_rag;
CREATE USER tsuyu WITH PASSWORD 'password';
GRANT ALL PRIVILEGES ON DATABASE tsuyu_rag TO tsuyu;
\c tsuyu_rag
CREATE EXTENSION IF NOT EXISTS vector;
SQL
```

> Skema diuruskan melalui **migrasi sqlx** dalam [migrations/](migrations/), dijalankan
> **automatik** semasa start dan dijejak dalam jadual `_sqlx_migrations` (idempotent).
> Fail [schema.sql](schema.sql) hanya rujukan. Untuk menambah perubahan skema, cipta fail
> migrasi baharu (cth. `migrations/0002_xxx.sql`) — jangan ubah migrasi yang telah dihantar.

### 5. Konfigurasi aplikasi

```bash
cp .env.example .env
# Edit .env ikut persekitaran anda (lihat bahagian seterusnya)
```

---

## Konfigurasi (.env)

| Pemboleh ubah     | Wajib | Lalai                     | Keterangan |
|-------------------|:-----:|---------------------------|------------|
| `DATABASE_URL`    | ✅    | —                         | URL sambungan PostgreSQL |
| `OLLAMA_URL`      | ❌    | `http://localhost:11434`  | Alamat Ollama |
| `GEN_MODEL`       | ❌    | `qwen3:14b`               | Model menjana jawapan |
| `EMBED_MODEL`     | ❌    | `bge-m3`                  | Model embedding |
| `EMBED_DIM`       | ❌    | `1024`                    | Dimensi vektor — mesti sepadan model (bge-m3=1024, nomic=768) |
| `GEN_THINK`       | ❌    | `false`                   | Mod thinking Qwen3: `false`/`true`/`default` |
| `DOCS_DIR`        | ❌    | `./docs`                  | Folder dokumen untuk ingest |
| `CHARACTER_CARD_PATH` | ❌ | `character.json`          | Fail JSON persona (kad watak); lalai jika tiada |
| `APP_TIMEZONE`    | ❌    | `Asia/Kuala_Lumpur`       | Zon waktu (IANA) untuk paparan TIMESTAMPTZ |
| `BIND_ADDR`       | ❌    | `127.0.0.1:8080`          | Alamat pelayan |
| `API_KEY`         | ❌    | _(kosong)_                | Key pengguna: `/chat`, `/chat/stream`, `GET /documents` |
| `ADMIN_API_KEY`   | ❌    | _(kosong)_                | Key admin: `/ingest`, `DELETE /documents/:id`, `DELETE /sessions/:id` |
| `RERANK_ENABLED`  | ❌    | `true`                    | Hidupkan reranking selepas carian vektor |
| `RERANKER_URL`    | ❌    | `http://localhost:8081`   | Alamat servis reranker (endpoint `/rerank`) |
| `RERANKER_MODEL`  | ❌    | `bge-reranker-v2-m3`      | Nama model reranker |
| `TOP_K`           | ❌    | `5`                       | Bilangan chunk akhir dihantar ke LLM |
| `RETRIEVE_N`      | ❌    | `30`                      | Bilangan calon dari pgvector sebelum rerank (> `TOP_K`) |
| `HYBRID_ENABLED`  | ❌    | `true`                    | Gabung carian vektor + kata kunci (BM25) via RRF |
| `RRF_K`           | ❌    | `60`                      | Pemalar k dalam Reciprocal Rank Fusion |
| `FTS_CONFIG`      | ❌    | `simple`                  | Konfigurasi full-text PostgreSQL (`simple`/`english`) |
| `MEMORY_ENABLED`  | ❌    | `true`                    | Ingat sejarah perbualan untuk permintaan dengan `session_id` |
| `MEMORY_TURNS`    | ❌    | `6`                       | Bilangan mesej terkini dimuat sebagai konteks perbualan |
| `RELEVANCE_ENABLED`| ❌   | `true`                    | Guardrail: tolak soalan tanpa LLM jika konteks tak cukup relevan |
| `RELEVANCE_MIN_RERANK`| ❌| `0.0`                     | Ambang minimum skor reranker (bila rerank hidup) |
| `RELEVANCE_MAX_DISTANCE`| ❌| `1.0`                   | Ambang maksimum jarak cosine (bila rerank mati) |
| `CHUNK_TOKENS`    | ❌    | `700`                     | Saiz sasaran setiap chunk (token sebenar, BPE) |
| `CHUNK_OVERLAP`   | ❌    | `100`                     | Pertindihan antara chunk (token) |
| `EMBED_BATCH_SIZE`| ❌    | `16`                      | Bilangan chunk per panggilan embedding semasa ingest |
| `OLLAMA_MAX_RETRIES`| ❌  | `2`                       | Cubaan semula panggilan Ollama/reranker yang gagal sementara |
| `OLLAMA_RETRY_BASE_MS`| ❌| `500`                     | Tempoh asas backoff (ms), digandakan setiap cubaan |
| `RATE_LIMIT_RPM`  | ❌    | `120`                     | Permintaan dibenarkan per IP setiap minit. 0 = dimatikan |
| `MAX_BODY_BYTES`  | ❌    | `2097152`                 | Had saiz badan permintaan (bait, lalai 2 MiB) |
| `RUST_LOG`        | ❌    | `info`                    | Tahap log (cth. `debug`, `tsuyu_rag_chatbot=debug`) |

Contoh `.env`:

```env
DATABASE_URL=postgres://tsuyu:password@localhost/tsuyu_rag
OLLAMA_URL=http://localhost:11434
GEN_MODEL=qwen3:14b
EMBED_MODEL=bge-m3
EMBED_DIM=1024
GEN_THINK=false
DOCS_DIR=/opt/tsuyu-rag/docs
BIND_ADDR=127.0.0.1:8080
API_KEY=
ADMIN_API_KEY=
RERANK_ENABLED=true
RERANKER_URL=http://localhost:8081
RERANKER_MODEL=bge-reranker-v2-m3
TOP_K=5
RETRIEVE_N=30
HYBRID_ENABLED=true
RRF_K=60
FTS_CONFIG=simple
MEMORY_ENABLED=true
MEMORY_TURNS=6
RELEVANCE_ENABLED=true
RELEVANCE_MIN_RERANK=0.0
RELEVANCE_MAX_DISTANCE=1.0
CHUNK_TOKENS=700
CHUNK_OVERLAP=100
EMBED_BATCH_SIZE=16
OLLAMA_MAX_RETRIES=2
OLLAMA_RETRY_BASE_MS=500
RATE_LIMIT_RPM=120
MAX_BODY_BYTES=2097152
RUST_LOG=info
```

---

## Menjalankan aplikasi

```bash
# Mod pembangunan
cargo run

# Build optimum (untuk deploy)
cargo build --release
./target/release/tsuyu-rag-chatbot
```

> **Nota build (query masa-kompil):** sebahagian query (documents/memory/sessions/stats)
> guna makro `sqlx::query!` yang disahkan pada masa kompil. Build menggunakan cache
> **`.sqlx/`** yang di-commit, jadi **tiada DB diperlukan untuk build biasa**. Jika anda
> mengubah mana-mana query `query!`, jana semula cache: `cargo sqlx prepare` (dengan
> `DATABASE_URL` ditetapkan), kemudian commit folder `.sqlx/`. Untuk paksa mod offline:
> `SQLX_OFFLINE=true cargo build`. (Pasang alat: `cargo install sqlx-cli --no-default-features --features postgres`.)

Selepas start, buka pelayar ke `http://127.0.0.1:8080` untuk frontend chat. Setiap
jawapan ada butang **Salin** (📋); perbualan boleh **dicetak** (🖨️) atau **dieksport**
ke fail `.md`/`.txt` (soalan + jawapan + rujukan) — semua sisi-klien, tiada data ke luar. UI
**pengurusan dokumen** (lihat senarai, cetus ingest, padam dokumen) ada di
`http://127.0.0.1:8080/admin` — dirender pelayan dengan templat Askama. Masukkan
`ADMIN_API_KEY` pada halaman itu untuk membenarkan tindakan ingest/padam.

### Perintah CLI

Binari yang sama menyokong beberapa perintah selain menghidupkan pelayan. Semua perintah
membaca konfigurasi `.env` yang sama (`DATABASE_URL`, `DOCS_DIR`, dsb.) dan berjalan sekali
lalu keluar — **tiada pelayan atau API key diperlukan** — jadi sesuai untuk cron, skrip
deploy, dan troubleshooting.

```bash
tsuyu-rag-chatbot                  # (lalai) hidupkan pelayan HTTP — sama seperti `serve`
tsuyu-rag-chatbot serve            # hidupkan pelayan HTTP secara eksplisit
tsuyu-rag-chatbot ingest           # ingest dokumen sekali (tokokan)
tsuyu-rag-chatbot ingest --force   # ingest semula semua fail walaupun tidak berubah
tsuyu-rag-chatbot check            # pemeriksaan praterbang: DB, Ollama, model, reranker
tsuyu-rag-chatbot stats            # gambaran DB: kiraan dokumen/chunk/mesej + saiz
tsuyu-rag-chatbot prune-sessions --older-than 30   # padam memori perbualan > 30 hari
tsuyu-rag-chatbot ask "Apa polisi cuti tahunan?"   # pertanyaan RAG sekali-jalan
tsuyu-rag-chatbot --help           # papar bantuan
```

| Perintah | Guna | Kod keluar |
|---|---|---|
| `ingest [--force]` | Saluran ingest sama seperti `POST /ingest`. Cetak ringkasan. | `1` jika ada fail gagal |
| `check` | Sahkan DB + Ollama + model + reranker boleh dicapai (sebelum deploy). | `1` jika tidak sihat |
| `stats` | Kiraan dokumen/chunk/mesej + saiz DB. Read-only. | `0` |
| `prune-sessions [--older-than N]` | Padam memori perbualan > N hari (lalai 90; dasar PDPA). | `0` |
| `ask "<soalan>"` | Saluran RAG penuh; cetak jawapan + sumber. Ujian asap. | `0` |

Lihat [PANDUAN-DOKUMEN.md](PANDUAN-DOKUMEN.md) §5 dan [RUNBOOK.md](RUNBOOK.md) untuk
penjadualan automatik & penggunaan operasi.

---

## Pengesahan (authentication)

Pengesahan **dua peringkat** menggunakan API key melalui header `Authorization: Bearer <key>`.

| Peranan | Pemboleh ubah | Endpoint |
|---------|---------------|----------|
| **Pengguna** | `API_KEY` | `POST /chat`, `POST /chat/stream`, `GET /documents` |
| **Admin** | `ADMIN_API_KEY` | `POST /ingest`, `DELETE /documents/:id`, `DELETE /sessions/:id` |

Peraturan:
- Key **admin juga boleh** mengakses endpoint pengguna (admin ⊇ pengguna).
- Jika `ADMIN_API_KEY` **tidak ditetapkan**, endpoint admin **jatuh balik** ke `API_KEY`
  (mod satu-key — sama seperti versi sebelum ini).
- Jika `API_KEY` **kosong**, pengesahan pengguna dimatikan; jika kedua-dua kosong, semua
  endpoint terbuka (pembangunan tempatan) — amaran dicatat dalam log semasa start.
- Endpoint `GET /health`, frontend `GET /`, dan halaman UI `GET /admin` sentiasa
  **terbuka** — tetapi `/admin` hanya rangka halaman; data & tindakannya
  (`/admin/documents`, `/admin/ingest`, …) tetap memerlukan kunci yang sah.
- Key salah/tiada → **401 Unauthorized**.

Contoh:

```bash
# Pengguna biasa boleh bertanya
curl -X POST http://localhost:8080/chat \
  -H 'Authorization: Bearer <API_KEY>' \
  -H 'Content-Type: application/json' \
  -d '{ "question": "Soalan?" }'

# Hanya admin boleh cetus ingest / padam
curl -X POST http://localhost:8080/ingest \
  -H 'Authorization: Bearer <ADMIN_API_KEY>'
```

**Frontend**: medan "API key" disediakan di halaman utama; nilainya disimpan dalam
`localStorage` pelayar dan dihantar automatik dengan setiap permintaan. (Untuk operasi
admin melalui UI, masukkan `ADMIN_API_KEY` dalam medan itu.)

> **Amalan baik:**
> - Guna key yang panjang & rawak (cth. `openssl rand -hex 32`), berbeza untuk pengguna vs admin.
> - Hantar melalui HTTPS sahaja (letak Nginx/TLS di depan — lihat bahagian deploy).
> - Perbandingan key dibuat secara **masa-tetap** (constant-time) untuk elak serangan masa.
> - Untuk per-pengguna penuh atau SSO, ini boleh dinaik taraf pada masa hadapan.

---

## API endpoint

> Jika key ditetapkan, contoh `curl` di bawah perlu tambah header
> `-H 'Authorization: Bearer <key>'` — guna `ADMIN_API_KEY` untuk endpoint admin
> (`/ingest`, `DELETE …`) dan `API_KEY` untuk selebihnya.

### `GET /health`
Semak kesihatan DB, Ollama, ketersediaan model, dan reranker (jika dihidupkan).

```bash
curl http://localhost:8080/health
```
```json
{
  "status": "ok",
  "database": true,
  "ollama": true,
  "reranker": true,
  "models": { "gen": true, "embed": true }
}
```
Pulang **200** jika semua komponen relevan hidup, **503** jika ada yang gagal
(`status: "degraded"`). Medan:
- `models.gen` / `models.embed` — sama ada `GEN_MODEL` / `EMBED_MODEL` benar-benar wujud
  di Ollama (disemak dari `/api/tags`; padanan abai tag `:latest`). Jika `false`, jalankan
  `ollama pull <model>`.
- `reranker` — tiada jika `RERANK_ENABLED=false`.

---

### `GET /metrics`
Metrik dalam format teks **Prometheus** (terbuka, untuk pengikis dalaman).

```bash
curl http://localhost:8080/metrics
```
```
tsuyu_chat_requests_total 128
tsuyu_chat_errors_total 3
tsuyu_ingest_runs_total 5
tsuyu_retrieval_duration_ms_sum 9100
tsuyu_retrieval_duration_ms_count 125
tsuyu_generate_duration_ms_sum 412000
tsuyu_generate_duration_ms_count 125
```
Purata masa dikira di Prometheus: `…_duration_ms_sum / …_duration_ms_count`. Sesuai
disambung ke Grafana untuk papan pemuka latensi retrieval/generation & kadar ralat.

---

### `POST /ingest`
Cetus proses ingest semua fail bersokongan (PDF, DOCX, TXT, MD) dalam `DOCS_DIR`
**secara rekursif** (termasuk subfolder; folder tersembunyi `.` dilangkau).
Berjalan sebagai **background task** — respons pulang serta-merta; semak log untuk kemajuan.

```bash
curl -X POST http://localhost:8080/ingest
```
```json
{ "status": "accepted", "message": "Ingest dimulakan di latar belakang. Fail tidak berubah akan dilangkau. Semak log untuk kemajuan." }
```

**Ingest tokokan (incremental):** secara lalai, fail yang **tidak berubah** sejak ingest
terakhir akan **dilangkau** tanpa dibaca atau di-embed semula. Pengesanan perubahan
berdasarkan **saiz fail + masa ubah suai (mtime)** yang disimpan dalam jadual `documents`
(`size_bytes`, `mtime_unix`). Ini menjadikan ingest berulang sangat pantas apabila hanya
sebahagian kecil dokumen berubah.

Untuk **memaksa** ingest semula semua fail (cth. selepas tukar model embedding atau
saiz chunk), gunakan `?force=true`:

```bash
curl -X POST 'http://localhost:8080/ingest?force=true'
```

Log akan menunjukkan ringkasan, cth.:
```
ingest selesai: 3 dokumen, 57 chunk, 12 tidak berubah, 0 dilangkau (ralat)
```

> Re-ingest fail yang berubah adalah selamat: chunk lama untuk dokumen itu dibuang dahulu
> (kunci pada laluan fail), jadi tiada pendua.

**Batch embedding:** semasa ingest, chunk diproses dalam kelompok (`EMBED_BATCH_SIZE`,
lalai 16) — setiap kelompok dijana embeddingnya dalam **satu** panggilan ke endpoint
`/api/embed` Ollama, dan dimasukkan ke DB dengan **satu** INSERT berbilang baris. Ini
mengurangkan round-trip HTTP dan overhead DB secara drastik berbanding memproses chunk
satu demi satu. Naikkan `EMBED_BATCH_SIZE` untuk ingest lebih pantas jika RAM/Ollama
mengizinkan; turunkan jika berlaku timeout pada kelompok besar.

> Memerlukan versi Ollama yang menyokong endpoint `/api/embed` (kebanyakan pemasangan
> terkini). Laluan soalan (`/chat`) masih guna `/api/embeddings` untuk embedding tunggal.

---

### `POST /chat`
Tanya soalan dan dapatkan jawapan + rujukan.

```bash
curl -X POST http://localhost:8080/chat \
  -H 'Content-Type: application/json' \
  -d '{ "question": "Berapa hari cuti tahunan kakitangan?" }'
```
```json
{
  "answer": "Mengikut dokumen, cuti tahunan ialah 20 hari ...",
  "sources": [
    {
      "document_id": 3, "filename": "polisi-cuti.pdf", "chunk_index": 5,
      "page": 4,
      "snippet": "Kakitangan tetap layak mendapat cuti tahunan sebanyak 20 hari setahun …",
      "distance": 0.18,
      "meta": { "category": "hr", "department": "Sumber Manusia", "year": 2024, "security": "dalaman" }
    }
  ]
}
```
Medan `sources[]`:
- `page` — nombor muka surat (1-asas) untuk PDF; tiada untuk TXT/MD/DOCX.
- `snippet` — petikan ringkas kandungan chunk (ruang putih dimampatkan, ~240 aksara).
- `distance` — jarak cosine, semakin **kecil** semakin relevan.
- `meta` — metadata dokumen sumber (lihat [Metadata dokumen](#metadata-dokumen)).

**Memori perbualan (pilihan):** sertakan `session_id` untuk membolehkan soalan susulan.
Sistem akan memuat beberapa giliran terakhir sesi itu sebagai konteks, dan menyimpan
giliran baharu selepas menjawab.

```bash
# Soalan pertama
curl -X POST http://localhost:8080/chat -H 'Content-Type: application/json' \
  -d '{ "question": "Berapa hari cuti tahunan?", "session_id": "sesi-ali-123" }'
# Soalan susulan — faham "kontrak" merujuk cuti tahunan
curl -X POST http://localhost:8080/chat -H 'Content-Type: application/json' \
  -d '{ "question": "Untuk staf kontrak pula?", "session_id": "sesi-ali-123" }'
```

> Tanpa `session_id`, setiap soalan dilayan tanpa konteks lampau. Memori boleh
> dimatikan global dengan `MEMORY_ENABLED=false`.

**Penapis metadata (pilihan):** hadkan carian kepada dokumen yang sepadan. Hanya medan
yang ditetapkan menapis; selebihnya diabaikan.

```bash
curl -X POST http://localhost:8080/chat -H 'Content-Type: application/json' \
  -d '{
        "question": "Apakah syarat perolehan?",
        "filter": { "category": "perolehan", "year": 2024 }
      }'
```

---

### `POST /chat/stream`
Sama seperti `/chat`, tetapi **alirkan jawapan token demi token** menggunakan
Server-Sent Events (SSE) — jawapan terus dipapar ketika model menjananya, tanpa
menunggu jawapan penuh. Inilah endpoint yang digunakan oleh frontend.

```bash
curl -N -X POST http://localhost:8080/chat/stream \
  -H 'Content-Type: application/json' \
  -d '{ "question": "Berapa hari cuti tahunan kakitangan?" }'
```

Jujukan event SSE yang dipulangkan:

| Event     | `data`                          | Keterangan |
|-----------|---------------------------------|------------|
| `sources` | JSON array `Source[]`           | Dihantar dahulu, sebaik retrieval siap |
| `token`   | string dipetik JSON, cth. `"cu"`| Banyak event; setiap satu satu potongan teks |
| `done`    | `[DONE]`                        | Penanda tamat |
| `error`   | string dipetik JSON             | Jika berlaku ralat semasa penjanaan |

Contoh aliran mentah:
```
event: sources
data: [{"document_id":3,"filename":"polisi-cuti.pdf","chunk_index":5,"distance":0.18}]

event: token
data: "Mengikut "

event: token
data: "dokumen, cuti "

event: done
data: [DONE]
```

> **Nota klien:** `token` (dan `error`) dipetik sebagai JSON supaya aksara baris baru
> dalam jawapan dihantar dengan selamat dalam satu medan `data:`. Klien perlu
> `JSON.parse()` nilai tersebut sebelum memaparkannya (lihat [static/index.html](static/index.html)).
> Oleh sebab badan permintaan diperlukan (POST), gunakan `fetch()` + pembaca aliran,
> bukan `EventSource` native (yang hanya menyokong GET).

---

### `GET /documents`
Senaraikan semua dokumen yang telah di-ingest, beserta bilangan chunk setiap satu.

```bash
curl http://localhost:8080/documents \
  -H 'Authorization: Bearer <API_KEY>'   # jika API_KEY ditetapkan
```
```json
{
  "count": 2,
  "documents": [
    {
      "id": 3,
      "filename": "polisi-cuti.pdf",
      "path": "/opt/tsuyu-rag/docs/polisi-cuti.pdf",
      "size_bytes": 184320,
      "mtime_unix": 1748764800,
      "chunk_count": 12,
      "ingested_at": "2026-06-01 08:09:00+00"
    }
  ]
}
```

---

### `DELETE /documents/:id`
Padam satu dokumen dan **semua chunknya** (via `ON DELETE CASCADE`).

```bash
curl -X DELETE http://localhost:8080/documents/3 \
  -H 'Authorization: Bearer <API_KEY>'   # jika API_KEY ditetapkan
```
```json
{ "deleted": true, "id": 3 }
```
Pulang **404 Not Found** jika dokumen dengan id tersebut tidak wujud.

> Untuk membuang dokumen daripada sistem sepenuhnya, padam juga fail asalnya dari
> `DOCS_DIR` — jika tidak, ingest berikutnya akan memasukkannya semula.

---

### `DELETE /sessions/:id`
Kosongkan memori perbualan bagi satu sesi (padam semua mesej sesi tersebut).

```bash
curl -X DELETE http://localhost:8080/sessions/sesi-ali-123 \
  -H 'Authorization: Bearer <API_KEY>'   # jika API_KEY ditetapkan
```
```json
{ "cleared": true, "messages_deleted": 4 }
```

---

### UI pengurusan dokumen (`GET /admin`)

Halaman web dirender pelayan (templat **Askama**) untuk memudahkan ingest tanpa `curl`:
melihat senarai dokumen + metadata + bilangan chunk, mencetus ingest (biasa/paksa), dan
memadam dokumen. Halaman terbuka (seperti `/`); tindakan ingest/padam menghantar
`ADMIN_API_KEY` yang dimasukkan pada halaman sebagai header `Authorization: Bearer`.

Laluan sokongan yang memulangkan HTML (digunakan oleh halaman itu sendiri):
`GET /admin/documents` (fragmen jadual), `POST /admin/ingest`, `DELETE /admin/documents/:id`.
Endpoint JSON `/documents`, `/ingest` kekal untuk automasi.

---

## Aliran kerja biasa

```bash
# 1. Pastikan infra hidup
curl http://localhost:8080/health

# 2. Letak dokumen dalam DOCS_DIR
cp ~/dokumen-tsuyu/*.pdf /opt/tsuyu-rag/docs/

# 3. Ingest
curl -X POST http://localhost:8080/ingest
#    (pantau: journalctl -u tsuyu-rag -f  ATAU  log terminal cargo run)

# 4. Berbual
curl -X POST http://localhost:8080/chat \
  -H 'Content-Type: application/json' \
  -d '{ "question": "Soalan anda di sini" }'
```

---

## Guardrail anti-halusinasi

Untuk memastikan LLM **hanya menjawab dari dokumen TSUYU** (dan tidak mereka jawapan),
sistem menggunakan beberapa lapisan pertahanan:

1. **Ambang relevansi (pra-LLM)** — lapisan paling kukuh. Sebelum memanggil LLM, sistem
   menyemak skor relevansi konteks terbaik:
   - Bila reranker hidup: `rerank_score` chunk terbaik mesti ≥ `RELEVANCE_MIN_RERANK`.
   - Bila reranker mati: jarak cosine terkecil mesti ≤ `RELEVANCE_MAX_DISTANCE`.
   - Jika gagal → terus pulang **"maklumat tidak dijumpai dalam dokumen TSUYU"** tanpa
     memanggil Ollama. Ini menghalang halusinasi secara **deterministik** (model tak
     berpeluang menjawab) dan menjimatkan masa/sumber.
2. **Prompt grounding** — arahan tegas: *"Gunakan HANYA maklumat dalam KONTEKS… Jika tiada,
   katakan tidak menjumpai. Jangan reka jawapan."*
3. **Pengasingan data vs arahan** — lihat [mitigasi prompt injection](#nota-reka-bentuk).

**Menala ambang:**
- Terlalu banyak soalan sah ditolak? → **longgarkan**: turunkan `RELEVANCE_MIN_RERANK`
  (cth. `-2.0`) atau naikkan `RELEVANCE_MAX_DISTANCE` (cth. `1.3`).
- LLM masih jawab di luar dokumen? → **ketatkan**: naikkan `RELEVANCE_MIN_RERANK`
  (cth. `1.0`) atau turunkan `RELEVANCE_MAX_DISTANCE` (cth. `0.7`).
- Matikan sepenuhnya dengan `RELEVANCE_ENABLED=false` (LLM sentiasa dipanggil; bergantung
  pada prompt grounding sahaja).

> **Tip kalibrasi:** perhatikan medan `distance` dalam `sources[]` respons `/chat` untuk
> soalan yang anda *tahu* dijawab betul, lalu tetapkan ambang sedikit lebih longgar daripada
> nilai itu. Dengan reranker, skor lebih bermakna daripada jarak cosine mentah.

---

## Metadata dokumen

Setiap dokumen boleh disertakan **metadata** melalui fail **sidecar** bernama
`<nama-dokumen>.meta.json` dalam folder yang sama. Contohnya, untuk `polisi-cuti.pdf`,
cipta `polisi-cuti.pdf.meta.json`:

```json
{
  "category": "hr",
  "department": "Bahagian Sumber Manusia",
  "year": 2024,
  "security": "dalaman"
}
```

| Medan        | Jenis  | Contoh | Kegunaan |
|--------------|--------|--------|----------|
| `category`   | teks   | `kontrak`, `polisi`, `perolehan`, `hr` | Jenis dokumen |
| `department` | teks   | `Bahagian Sumber Manusia` | Jabatan/bahagian pemilik |
| `year`       | nombor | `2024` | Tahun dokumen |
| `security`   | teks   | `awam`, `dalaman`, `sulit` | Tahap keselamatan |

- **Semua medan pilihan** — dokumen tanpa sidecar tetap di-ingest (metadata kosong).
- Metadata disimpan pada peringkat **dokumen** (dikongsi oleh semua chunknya).
- Metadata dikembalikan dalam `sources[].meta` setiap jawapan, dan boleh **menapis**
  carian melalui medan `filter` dalam `/chat` (lihat [POST /chat](#post-chat)).
- **Mengubah sidecar mencetuskan re-ingest**: ingest tokokan mengambil kira mtime
  sidecar, jadi mengemas kini `.meta.json` sahaja sudah cukup untuk metadata baharu
  digunakan (tidak perlu `?force=true`).

---

## Kad Watak (persona)

Persona pembantu boleh **ditala oleh admin** tanpa mengubah kod — nama, peranan, nada,
bahasa, panjang jawapan, emoji, dan peraturan khas. Ia disuntik ke dalam *system prompt*.

Disimpan sebagai fail JSON di `CHARACTER_CARD_PATH` (lalai `character.json`). Jika fail
tiada, persona lalai yang munasabah digunakan. Contoh:

```json
{
  "name": "Ayu",
  "role": "Pembantu pegawai TSUYU",
  "tone": "Formal tetapi mesra",
  "language": "Bahasa Malaysia",
  "verbosity": "medium",
  "emoji": false,
  "special_rules": [
    "Sentiasa gunakan istilah rasmi kerajaan",
    "Berikan rujukan dokumen jika ada"
  ]
}
```

- `verbosity`: `short` | `medium` | `long`. Medan yang hilang dalam JSON guna lalai.
- **Cara edit:** UI `/admin` (bahagian "Kad Watak") — perubahan **berkuat kuasa
  serta-merta** untuk soalan seterusnya; atau edit fail terus & mula semula.
- **API:** `GET /admin/character` (baca), `PUT /admin/character` (kemas kini, auth admin).
- **Keselamatan:** persona ialah input admin dipercayai, tetapi peraturan keras
  (jawab dari konteks sahaja + anti-injection) dirantai **selepas** persona dalam prompt,
  jadi kad watak tidak boleh melemahkan guardrail.

---

## Struktur projek

```
tsuyu-rag-chatbot/
├── Cargo.toml
├── .env.example
├── migrations/                 # migrasi skema sqlx (sumber sebenar skema)
│   └── 0001_initial.sql
├── .sqlx/                      # cache query masa-kompil (di-commit; build offline tanpa DB)
├── schema.sql                  # snapshot skema (rujukan sahaja)
├── README.md
├── deploy/
│   └── tsuyu-rag.service        # unit systemd contoh
├── templates/                 # templat Askama (UI admin, dirender pelayan)
│   ├── admin.html
│   └── documents_table.html
├── static/
│   └── index.html              # frontend chat ringkas
├── tests/
│   └── integration.rs          # ujian integrasi DB via lib crate (bergerbang TEST_DATABASE_URL)
└── src/
    ├── main.rs                 # titik masuk NIPIS: #[tokio::main] → tsuyu_rag_chatbot::run()
    ├── lib.rs                  # titik masuk sebenar: run(), parse argumen, setup, dispatch perintah
    ├── cli.rs                  # perintah CLI: ingest/check/stats/prune-sessions/ask
    ├── config.rs               # baca konfigurasi dari persekitaran
    ├── auth.rs                  # middleware pengesahan API key (Bearer)
    ├── ratelimit.rs             # middleware had kadar per-IP (+ ujian unit)
    ├── metrics.rs               # kaunter atomik + render Prometheus (+ ujian unit)
    ├── error.rs                # AppError → respons HTTP (tiada unwrap)
    ├── state.rs                # AppState dikongsi (config, pool, http client)
    ├── db.rs                   # pool, run_migrations + reconcile dim/fts, vector_literal()
    ├── models.rs               # struct request/response
    ├── handlers/
    │   ├── mod.rs              # router (terbuka + dilindungi) + frontend
    │   ├── health.rs           # GET /health (DB + Ollama + reranker)
    │   ├── metrics.rs          # GET /metrics (Prometheus)
    │   ├── ingest.rs           # POST /ingest
    │   ├── chat.rs             # POST /chat, POST /chat/stream (retrieve→rerank→jana)
    │   ├── documents.rs        # GET /documents, DELETE /documents/:id
    │   ├── admin.rs            # UI pengurusan dokumen (Askama): GET /admin + tindakan HTML
    │   └── sessions.rs         # DELETE /sessions/:id (kosongkan memori)
    └── services/
        ├── mod.rs
        ├── character.rs        # kad watak (persona) — ditala admin (+ ujian unit)
        ├── chunk.rs            # pemecahan teks ikut token BPE (+ ujian unit)
        ├── embed.rs            # panggilan embedding Ollama (tunggal + batch)
        ├── retrieve.rs         # carian vektor + kata kunci + hybrid RRF (+ ujian unit)
        ├── rerank.rs           # reranking cross-encoder (servis luar)
        ├── retry.rs            # retry + backoff untuk panggilan Ollama/reranker
        ├── generate.rs         # bina prompt + jana + tapis <think> (+ ujian unit)
        ├── ingest.rs           # pipeline ingest (baca → chunk → embed → simpan)
        ├── memory.rs           # memori perbualan (muat/simpan sesi)
        ├── metadata.rs         # baca sidecar .meta.json (+ ujian unit)
        └── documents.rs        # senarai & padam dokumen
```

---

## Ujian

```bash
cargo test
```

**Ujian unit** (tiada kebergantungan luaran) meliputi:
- **Logik chunking** ([src/services/chunk.rs](src/services/chunk.rs)) — teks kosong,
  chunk tunggal, pecahan teks panjang, pertindihan token, dan had overlap.
- **Pembinaan prompt + penapis thinking** ([src/services/generate.rs](src/services/generate.rs)).
- **Gabungan RRF hybrid** ([src/services/retrieve.rs](src/services/retrieve.rs)).
- **Petikan ringkas** ([src/handlers/chat.rs](src/handlers/chat.rs)).
- **Had kadar** ([src/ratelimit.rs](src/ratelimit.rs)) & **padanan model** ([src/handlers/health.rs](src/handlers/health.rs)).

**Ujian integrasi** ([tests/integration.rs](tests/integration.rs)) — crate ujian
berasingan yang mengakses API melalui **lib crate** (`tsuyu_rag_chatbot`), memerlukan
PostgreSQL + pgvector sebenar, **bergerbang** oleh `TEST_DATABASE_URL`. Tanpa env itu,
ujian dilangkau secara bersih (tidak gagal). Untuk menjalankannya:

```bash
# Sediakan DB ujian (sekali) dengan pgvector:
createdb tsuyu_rag_test && psql -d tsuyu_rag_test -c 'CREATE EXTENSION IF NOT EXISTS vector'

# Jalankan ujian integrasi (WAJIB bersiri — setiap ujian mengosongkan jadual dikongsi):
TEST_DATABASE_URL=postgres://tsuyu:password@localhost/tsuyu_rag_test \
    cargo test --test integration -- --test-threads=1
```

> Struktur: `src/main.rs` ialah pembalut nipis (`#[tokio::main]` → `run()`); semua logik
> berada dalam `src/lib.rs` + submodul, supaya ujian integrasi boleh mengaksesnya melalui
> pustaka. Lihat [Struktur projek](#struktur-projek).

Liputan integrasi: memori sesi (simpan/muat/had/clear), pengurusan dokumen
(senarai/padam/cascade chunk), dan idempotensi skema. Skema ujian dikosongkan sebelum
setiap kes, jadi gunakan **pangkalan data berasingan** (bukan DB pengeluaran).

---

## Deploy ke Ubuntu (systemd)

Ubuntu guna **systemd** (bukan NSSM). Fail unit contoh: [deploy/tsuyu-rag.service](deploy/tsuyu-rag.service).

```bash
# 1. Build & letak binary
cargo build --release
sudo mkdir -p /opt/tsuyu-rag/docs
sudo cp target/release/tsuyu-rag-chatbot /opt/tsuyu-rag/
sudo cp .env /opt/tsuyu-rag/.env

# 2. Pasang unit systemd
sudo cp deploy/tsuyu-rag.service /etc/systemd/system/tsuyu-rag.service
sudo systemctl daemon-reload
sudo systemctl enable --now tsuyu-rag
sudo systemctl status tsuyu-rag

# 3. Lihat log
journalctl -u tsuyu-rag -f
```

> **Penting:** service systemd **tidak** mewarisi env shell. Unit menggunakan
> `EnvironmentFile=/opt/tsuyu-rag/.env` untuk memuat konfigurasi dari laluan mutlak.

### Servis reranker

Jika `RERANK_ENABLED=true`, servis reranker mesti hidup sebelum aplikasi boleh menjawab.
Cara paling mudah ialah Docker (lihat bahagian setup). Pastikan ia auto-start — sama ada
melalui `--restart unless-stopped` pada kontena Docker, atau unit systemd tersendiri.
Jika anda menjalankan reranker pada mesin/port lain, kemas kini `RERANKER_URL` dalam `.env`.

### (Pilihan) Reverse proxy + keselamatan dalaman

Letak Nginx di depan untuk TLS, dan hadkan akses dengan `ufw` supaya hanya subnet
dalaman TSUYU boleh sambung. `proxy_pass` ke `127.0.0.1:8080`.

### Padanan konsep Windows → Ubuntu

| Windows (sedia ada)       | Ubuntu (baru)                  |
|---------------------------|--------------------------------|
| NSSM service              | systemd unit                   |
| `nssm set AppDirectory`   | `WorkingDirectory=` dalam unit |
| `.env` via dotenv path    | `EnvironmentFile=` dalam unit  |
| `tsuyu-log` (tail log)     | `journalctl -u tsuyu-rag -f`    |
| restart service manual    | `Restart=on-failure` (auto)    |

---

## Penyelesaian masalah

| Gejala | Kemungkinan punca & tindakan |
|--------|------------------------------|
| `/health` pulang `database: false` | PostgreSQL tidak hidup, `DATABASE_URL` salah, atau sambungan `vector` belum dipasang. |
| `/health` pulang `ollama: false` | Ollama tidak hidup (`systemctl status ollama`) atau `OLLAMA_URL` salah. |
| `/health` pulang `reranker: false` | Servis reranker tidak hidup atau `RERANKER_URL` salah. Matikan dengan `RERANK_ENABLED=false` jika tidak digunakan. |
| `/health` pulang `models.gen`/`models.embed`: false | Model belum di-`pull`. Jalankan `ollama pull <GEN_MODEL>` / `ollama pull <EMBED_MODEL>`. |
| Ingest langkau semua fail | Periksa `DOCS_DIR` betul & ada fail `.pdf/.docx/.txt/.md`. Lihat log `journalctl`. |
| Jawapan kosong / "tidak menjumpai maklumat" | Dokumen belum di-ingest, atau soalan di luar skop dokumen. |
| Ralat dimensi vector | Pastikan `EMBED_DIM` sepadan model (bge-m3=1024). Selepas tukar model/dimensi, jalankan `POST /ingest?force=true`. |
| Jawapan ada teks `<think>` | Tetapkan `GEN_THINK=false` (lalai). Penapis juga membuang blok ini secara automatik. |
| Embedding "kosong" dari Ollama | Model embedding belum di-`pull` atau nama model salah. |

Untuk log lebih terperinci:
```bash
RUST_LOG=tsuyu_rag_chatbot=debug cargo run
```

---

## Nota reka bentuk

- **Tiada `unwrap()`/`expect()`** dalam kod produksi — semua ralat dikendalikan melalui
  `?`, `match`, dan jenis `AppError` (thiserror untuk domain, anyhow untuk lapisan atas).
- **Semua I/O async** (tokio): DB, HTTP ke Ollama, dan baca fail. Operasi blocking
  (baca PDF/DOCX) dilarikan dalam `spawn_blocking`.
- **Query sqlx runtime** (bukan makro `query!` compile-time) supaya boleh kompil tanpa
  DB hidup. Boleh ditukar ke makro compile-time jika mahu semakan ketat.
- **Chunking ikut token sebenar**: saiz chunk dikira dalam token BPE (`cl100k_base` via
  tiktoken-rs), bukan kiraan perkataan — lebih konsisten dengan had konteks model dan
  lebih tepat untuk teks campuran (BM, tanda baca). Tokenizer terbenam dalam binari (tiada
  fail luaran) dan dimuat sekali ke `AppState`. Lihat [src/services/chunk.rs](src/services/chunk.rs).
- **Petikan kaya**: setiap chunk merekod nombor muka surat (PDF di-ekstrak per-muka-surat
  via `extract_text_by_pages`); rujukan dalam `sources[]` menyertakan `page` + `snippet`
  teks sebenar untuk kebolehpercayaan. Format tanpa muka surat (TXT/MD/DOCX) → `page` null.
- **Ingest idempotent**: dokumen dikenali melalui laluan fail (`UNIQUE`); re-ingest
  membuang chunk lama dalam satu transaksi sebelum memasukkan yang baru.
- **Batch ingest**: embedding & INSERT dilakukan secara berkelompok (`EMBED_BATCH_SIZE`)
  untuk mengurangkan round-trip HTTP ke Ollama dan overhead pangkalan data.
- **Ingest tokokan (incremental)**: fail tidak berubah dilangkau berdasarkan saiz + mtime
  (`size_bytes`, `mtime_unix`); guna `?force=true` untuk paksa ingest semula.
- **Ingest rekursif**: `DOCS_DIR` dijelajah termasuk subfolder (stack eksplisit, bukan
  rekursi async); folder tersembunyi dilangkau. Lihat `list_supported_files`.
- **Retry + backoff Ollama**: panggilan ke Ollama & reranker dicuba semula bagi ralat
  sementara (timeout/connect/5xx/429) dengan exponential backoff (`OLLAMA_MAX_RETRIES`,
  `OLLAMA_RETRY_BASE_MS`); ralat 4xx kekal tidak dicuba semula. Untuk `/chat/stream`, hanya
  panggilan awal dicuba semula. Lihat [src/services/retry.rs](src/services/retry.rs).
- **Pengesahan dua peringkat**: middleware API key (`Authorization: Bearer`) dengan peranan
  pengguna (`API_KEY`) vs admin (`ADMIN_API_KEY`); admin ⊇ pengguna; admin jatuh balik ke
  `API_KEY` jika tak ditetapkan. Perbandingan masa-tetap; `/health` & frontend terbuka.
  Lihat [src/auth.rs](src/auth.rs).
- **Had kadar & saiz badan**: middleware fixed-window per-IP (`RATE_LIMIT_RPM`, → 429 jika
  dilampaui) + `DefaultBodyLimit` (`MAX_BODY_BYTES`). Tanpa dependency luaran; gagal-selamat
  (benarkan) jika kunci teracun. Lihat [src/ratelimit.rs](src/ratelimit.rs).
- **Graceful shutdown**: pelayan tangani **SIGTERM** (systemd) & **Ctrl-C**, menyiapkan
  permintaan dalam terbang sebelum keluar (`axum ... with_graceful_shutdown`). Lihat `main::shutdown_signal`.
- **Mitigasi prompt injection**: input tidak dipercayai (kandungan dokumen, soalan, sejarah)
  dineutralkan — baris penanda palsu (`=== ... ===`) dilucutkan supaya tak boleh memalsukan
  struktur prompt — dan arahan sistem menegaskan kandungan ialah **DATA, bukan arahan**.
  Lihat `generate::sanitize_untrusted` (+ ujian).
- **Guardrail relevansi (anti-halusinasi)**: sebelum memanggil LLM, sistem semak sama ada
  konteks yang diambil cukup relevan — guna skor reranker (`RELEVANCE_MIN_RERANK`) atau jarak
  cosine (`RELEVANCE_MAX_DISTANCE`). Jika tidak lepas ambang, terus pulang mesej "tidak
  dijumpai dalam dokumen TSUYU" **tanpa memanggil LLM** — menghalang jawapan luar konteks
  secara deterministik. Gabung dengan arahan prompt "guna HANYA konteks". Lihat
  `chat::nilai_relevan` (+ ujian) & [Guardrail anti-halusinasi](#guardrail-anti-halusinasi).
- **Metrik & pemerhatian**: kaunter atomik (`AtomicU64`) dalam `AppState` diinstrumen pada
  pipeline chat/ingest (kiraan, ralat, masa retrieval/generation); didedahkan sebagai teks
  Prometheus di `GET /metrics` — tanpa crate Prometheus. Lihat [src/metrics.rs](src/metrics.rs).
- **Pengurusan dokumen**: `GET /documents` (senarai + kiraan chunk) dan
  `DELETE /documents/:id` (padam + cascade chunk). Frontend ada panel ringkas untuk ini.
- **Embedding sebagai literal teks** `'[...]'::vector` — lihat `db::vector_literal()`.
- **Hybrid search**: gabung carian vektor (pgvector) + kata kunci (`tsvector`/GIN, dijana
  automatik) menggunakan **Reciprocal Rank Fusion** — tanpa enjin BM25 berasingan, kekal
  satu DB. Logik RRF ialah fungsi tulen yang diuji. Lihat [src/services/retrieve.rs](src/services/retrieve.rs).
- **Reranking dua peringkat**: carian luas (`RETRIEVE_N`) → cross-encoder
  (`bge-reranker-v2-m3`) → `TOP_K` terbaik. Reranker ialah servis luar (Ollama tiada
  endpoint rerank); boleh dimatikan. Lihat [src/services/rerank.rs](src/services/rerank.rs).
- **Memori perbualan**: sejarah sesi disimpan dalam jadual `messages` PostgreSQL (sama DB).
  Permintaan dengan `session_id` memuat `MEMORY_TURNS` giliran terakhir ke dalam prompt,
  membolehkan soalan susulan. Lihat [src/services/memory.rs](src/services/memory.rs).
- **Metadata sidecar**: metadata dokumen dibaca dari fail `<dokumen>.meta.json`, disimpan
  pada jadual `documents`, dan menapis carian melalui corak SQL `($n IS NULL OR col = $n)`
  (tiada SQL dinamik). Lihat [src/services/metadata.rs](src/services/metadata.rs) &
  [Metadata dokumen](#metadata-dokumen).
- **Migrasi DB berstruktur**: skema diuruskan oleh migrasi sqlx terbenam ([migrations/](migrations/)),
  dijejak dalam `_sqlx_migrations`. Hibrid: migrasi statik untuk struktur teras, +
  penyelarasan runtime untuk `EMBED_DIM`/`FTS_CONFIG` bukan-lalai (kerana migrasi statik
  tak boleh terima parameter). Lihat `db::run_migrations`.
- **Dimensi embedding & FTS boleh konfig** (`EMBED_DIM`/`FTS_CONFIG`): skema diselaraskan
  automatik selepas migrasi; tukar dimensi mengosongkan chunk lama (perlu re-ingest).
  Lihat `db::reconcile_embedding_dim` & `db::reconcile_fts_config`.
- **Penapis thinking**: blok `<think>...</think>` Qwen3 ditapis daripada jawapan (termasuk
  semasa streaming, merentas potongan token). Lihat `generate::strip_thinking` & `ThinkFilter`.
```

