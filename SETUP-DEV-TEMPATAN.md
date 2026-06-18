# Setup Dev Tempatan — Catatan Rujukan

Rujukan langkah-demi-langkah untuk menjalankan TSUYU RAG Chatbot pada **mesin pembangunan
CPU-sahaja**. Disediakan & disahkan berjalan pada **2026-06-02**.

> Untuk pemasangan pengeluaran penuh, lihat [README.md](README.md). Dokumen ini khusus
> untuk setup dev ringan (model kecil, tiada GPU yang mampu).

---

## 1. Mesin yang diuji

| Komponen | Spesifikasi | Nota |
|---|---|---|
| CPU | Intel i7-3770 (2012, 4C/8T) | Lama, **tiada AVX2** |
| GPU | NVIDIA GT 1030, **2 GB VRAM** | Terlalu kecil untuk LLM → guna CPU |
| RAM | 31 GB | Mencukupi |
| OS | Ubuntu 24.04, PostgreSQL 16, Ollama 0.21.1 | |

**Keputusan:** stack pengeluaran (qwen3:14b + bge-m3 + reranker) terlalu perlahan di sini.
Guna **model kecil di CPU** — satu jawapan RAG ~**5–6 saat**.

---

## 2. Langkah setup (sekali sahaja)

### 2.1 Pasang pgvector
```bash
sudo apt-get install -y postgresql-16-pgvector
```

### 2.2 Cipta pangkalan data + extension
```bash
createdb tsuyu_rag
psql -d tsuyu_rag -c "CREATE EXTENSION IF NOT EXISTS vector;"
```

### 2.3 Cipta role Postgres
TCP `localhost` memerlukan kata laluan (pg_hba guna scram), jadi cipta role khusus:
```bash
psql -d postgres -c "CREATE ROLE tsuyu LOGIN SUPERUSER PASSWORD 'password';"
```
> `SUPERUSER` hanya untuk dev tempatan (elak isu keizinan skema/migrasi). Jangan guna
> dalam pengeluaran.

### 2.4 Muat turun model kecil (Ollama)
```bash
ollama pull qwen3:1.7b          # penjana (~1.4 GB)
ollama pull nomic-embed-text    # embedding 768-dim (~274 MB)
```

### 2.5 Cipta fail `.env`
Salin nilai berikut ke `.env` (fail ini diabaikan git):
```ini
DATABASE_URL=postgres://tsuyu:password@localhost/tsuyu_rag
OLLAMA_URL=http://localhost:11434

GEN_MODEL=qwen3:1.7b
EMBED_MODEL=nomic-embed-text
EMBED_DIM=768
GEN_THINK=false
RERANK_ENABLED=false

DOCS_DIR=/home/tsuyu/rust_project/tsuyu-rag-chatbot/docs
BIND_ADDR=127.0.0.1:8080
API_KEY=
ADMIN_API_KEY=
RATE_LIMIT_RPM=0
RUST_LOG=info
```
> Profil ini juga didokumen sebagai komen dalam [.env.example](.env.example).
> **Penting:** `EMBED_DIM=768` untuk nomic (bukan 1024 bge-m3). Jika tukar model embed,
> jalankan `ingest --force` semula.

### 2.6 Bina binari
```bash
cargo build --release
```

---

## 3. Pengesahan (hasil sebenar sesi ini)

```bash
# 1. Pemeriksaan praterbang
./target/release/tsuyu-rag-chatbot check
# → ✓ Pangkalan data  ✓ Ollama  ✓ Model jana  ✓ Model embed  → Status: ok

# 2. Sediakan dokumen contoh & ingest
mkdir -p docs   # letak .pdf/.docx/.txt/.md di sini
./target/release/tsuyu-rag-chatbot ingest
# → Ingest selesai: 1 dokumen diproses, 1 chunk disimpan, 0 gagal.

# 3. Tanya (saluran RAG penuh)
./target/release/tsuyu-rag-chatbot ask "Berapa hari cuti tahunan untuk gred 41?"
# → "...gred 41 mendapat 30 hari cuti tahunan setahun."  (5.6s di CPU)
#   Sumber: contoh-cuti.txt (chunk 0)
```

| Ujian | Keputusan |
|---|---|
| `check` (DB + Ollama + model) | ✅ semua hijau |
| `ingest` | ✅ 1 dokumen, 1 chunk |
| `ask` (RAG CLI) | ✅ jawapan tepat, ~5.6 saat |
| `serve` → `GET /health` | ✅ `{"status":"ok",...}` |
| `GET /` (frontend chat) | ✅ |
| `GET /admin` (UI Askama) | ✅ render |
| `POST /chat` (RAG HTTP) | ✅ jawapan + sumber betul |

---

## 4. Penggunaan harian

```bash
# Pastikan servis hidup
systemctl is-active postgresql            # patut "active"
curl -s http://localhost:11434/api/tags   # Ollama hidup?

# Hidupkan pelayan + buka pelayar
./target/release/tsuyu-rag-chatbot serve
#   Chat   : http://127.0.0.1:8080/
#   Admin  : http://127.0.0.1:8080/admin

# Atau guna CLI terus
./target/release/tsuyu-rag-chatbot stats
./target/release/tsuyu-rag-chatbot ask "<soalan>"
```

---

## 5. Naik taraf ke pengeluaran (mesin lain berGPU)

Pada pelayan dengan GPU ≥16 GB, tukar `.env` sahaja (tiada perubahan kod):
```ini
GEN_MODEL=qwen3:14b
EMBED_MODEL=bge-m3
EMBED_DIM=1024
RERANK_ENABLED=true
```
Kemudian `ingest --force` untuk embed semula dengan model baharu. Lihat
[README.md](README.md) §Cadangan perkakasan dan [MODEL.md](MODEL.md).

---

## 6. Penyelesaian masalah ringkas

| Gejala | Sebab biasa | Tindakan |
|---|---|---|
| `check` gagal pada DB | Postgres mati / role salah | `systemctl status postgresql`; sahkan role `tsuyu` |
| `check` gagal pada model | Model belum ditarik | `ollama pull qwen3:1.7b nomic-embed-text` |
| Jawapan "tidak menjumpai" | Belum ingest / EMBED_DIM silap | `ingest --force`; pastikan `EMBED_DIM=768` |
| Jawapan sangat perlahan | Model terlalu besar untuk CPU | Guna qwen3:1.7b, bukan 14b |
