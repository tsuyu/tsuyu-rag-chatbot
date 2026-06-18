# RUNBOOK — Operasi TSUYU RAG Chatbot

Panduan operasi harian untuk staf IT yang **menyelenggara** sistem ini di pelayan
(bukan panduan pembangunan). Untuk setup awal & API, lihat [README.md](README.md).

> **Andaian laluan** (ikut [deploy/tsuyu-rag.service](deploy/tsuyu-rag.service)):
> aplikasi di `/opt/tsuyu-rag/`, dijalankan sebagai pengguna `tsuyu`, unit systemd
> `tsuyu-rag`, konfigurasi di `/opt/tsuyu-rag/.env`. Laraskan jika persekitaran anda berbeza.

---

## 0. Senarai semak pantas (apabila sesuatu rosak)

| Gejala | Periksa dahulu | Bahagian |
|---|---|---|
| Chatbot tak balas langsung | `systemctl status tsuyu-rag` | [§2](#2-kawalan-servis) |
| Balas "tidak menjumpai maklumat" untuk soalan sah | Ollama/embedding hidup? Dokumen di-ingest? | [§6](#6-senario-kegagalan--pemulihan) |
| Jawapan lambat | Beban Ollama/GPU, masa retrieval di `/metrics` | [§5](#5-pemantauan) |
| Ralat 500 pada `/chat` | Log aplikasi, DB & Ollama hidup? | [§6](#6-senario-kegagalan--pemulihan) |
| `/health` tidak `ok` | Model hilang dari Ollama, DB putus | [§6](#6-senario-kegagalan--pemulihan) |

**Pemeriksaan kesihatan pantas:**
```bash
# Pelayan sedang berjalan:
curl -s http://127.0.0.1:8080/health | jq .      # status keseluruhan
systemctl is-active tsuyu-rag postgresql ollama   # ketiga-tiga patut "active"

# Pelayan TIDAK berjalan (cth. sebelum deploy / menyiasat) — guna CLI:
cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot check      # DB + Ollama + model + reranker
```

---

## 1. Komponen sistem

Sistem bergantung pada **empat servis**. Jika satu mati, kesannya:

| Servis | Peranan | Jika mati |
|---|---|---|
| `tsuyu-rag` (aplikasi) | API + saluran RAG | Tiada chatbot langsung |
| `postgresql` | Simpan dokumen, chunk, embedding, memori | `/chat` & `/ingest` gagal (500) |
| `ollama` | Sajikan Qwen3 (jana) + bge-m3 (embed) | Tiada jawapan / tiada embedding |
| Reranker (TEI, port 8081) | Susun semula calon | Sistem terus jalan tanpa rerank (degradasi anggun) jika `RERANK_ENABLED` kekal `true` tetapi panggilan gagal — pantau log |

Lihat [MODEL.md](MODEL.md) untuk butiran model.

---

## 2. Kawalan servis

```bash
# Status & kesihatan
systemctl status tsuyu-rag

# Mula / henti / mula semula
sudo systemctl start tsuyu-rag
sudo systemctl stop tsuyu-rag         # graceful: tangani SIGTERM, habiskan permintaan aktif
sudo systemctl restart tsuyu-rag

# Aktif semasa but
sudo systemctl enable tsuyu-rag

# Selepas tukar .env atau ganti binari
sudo systemctl restart tsuyu-rag
```

> **Nota graceful shutdown:** aplikasi menangani SIGTERM (yang `systemctl stop` hantar),
> jadi permintaan dalam proses dihabiskan dahulu sebelum keluar. Tidak perlu `kill -9`
> kecuali ia tersekat melebihi tempoh `TimeoutStopSec` systemd.

### Menggantikan binari (selepas build baharu)
```bash
sudo systemctl stop tsuyu-rag
sudo cp target/release/tsuyu-rag-chatbot /opt/tsuyu-rag/tsuyu-rag-chatbot
sudo chown tsuyu:tsuyu /opt/tsuyu-rag/tsuyu-rag-chatbot
sudo systemctl start tsuyu-rag
curl -s http://127.0.0.1:8080/health | jq .
```

---

## 3. Log

```bash
# Ikut log langsung
journalctl -u tsuyu-rag -f

# Log sejak but terakhir / sejam lepas / hari ini
journalctl -u tsuyu-rag -b
journalctl -u tsuyu-rag --since "1 hour ago"
journalctl -u tsuyu-rag --since today

# Tapis ralat sahaja
journalctl -u tsuyu-rag -p err --since today
```

**Tahap log** dikawal oleh `RUST_LOG` dalam `.env` (lalai `info`). Untuk diagnosis
mendalam, tukar sementara ke `RUST_LOG=debug`, `systemctl restart tsuyu-rag`, dan
**kembalikan ke `info`** selepas siasat (mod debug bising & boleh log lebih banyak butiran).

**Putaran log:** journald mengurus putaran sendiri. Periksa & hadkan saiz jika perlu:
```bash
journalctl --disk-usage
sudo journalctl --vacuum-time=30d      # simpan 30 hari sahaja
```

---

## 4. Pangkalan data

Skema (lihat [migrations/0001_initial.sql](migrations/0001_initial.sql)):
- `documents` — satu baris per fail (metadata, saiz/mtime untuk ingest tokokan)
- `chunks` — potongan teks + `embedding vector(1024)` + indeks HNSW & GIN
- `messages` — sejarah perbualan (memori sesi)

### Semak pantas (CLI)
```bash
# Gambaran ringkas tanpa masuk psql — kiraan + saiz DB
cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot stats
```

### Sambung & semak (psql)
```bash
sudo -u postgres psql tsuyu_rag      # atau guna DATABASE_URL anda

-- Kiraan asas
SELECT count(*) FROM documents;
SELECT count(*) FROM chunks;
SELECT count(*) FROM messages;

-- Dokumen terbaru di-ingest
SELECT id, filename, ingested_at FROM documents ORDER BY ingested_at DESC LIMIT 10;

-- Saiz pangkalan data
SELECT pg_size_pretty(pg_database_size('tsuyu_rag'));
```

### Penyelenggaraan berkala
```sql
-- Selepas pemadaman/ingest besar: kemas statistik & ruang
VACUUM ANALYZE chunks;
VACUUM ANALYZE documents;
```

### Membersih memori perbualan lama (pilihan, jika `messages` membengkak)
Cara mudah (disyorkan) — guna CLI, sesuai untuk cron:
```bash
cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot prune-sessions --older-than 90
```
Atau terus SQL:
```sql
DELETE FROM messages WHERE created_at < now() - interval '90 days';
```

---

## 4b. Ingest dokumen (manual & berjadual)

Ingest boleh dicetus **dua cara** (kedua-dua guna logik tokokan yang sama):

```bash
# CLI — tiada pelayan/API key perlu (sesuai untuk shell/cron)
cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot ingest            # tokokan
cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot ingest --force    # proses semula semua

# HTTP — semasa pelayan berjalan
curl -X POST http://127.0.0.1:8080/ingest -H "Authorization: Bearer $ADMIN_API_KEY"
```
CLI mencetak ringkasan dan **keluar dengan kod bukan-sifar jika ada fail gagal** —
mudah dipantau dalam cron/skrip. Lihat [PANDUAN-DOKUMEN.md](PANDUAN-DOKUMEN.md) §5.

### Penjadualan dengan systemd timer (disyorkan)

Unit contoh disediakan: [deploy/tsuyu-rag-ingest.service](deploy/tsuyu-rag-ingest.service)
(oneshot) + [deploy/tsuyu-rag-ingest.timer](deploy/tsuyu-rag-ingest.timer) (harian 01:00).

```bash
sudo cp deploy/tsuyu-rag-ingest.service deploy/tsuyu-rag-ingest.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now tsuyu-rag-ingest.timer

# Semak jadual & larian lepas
systemctl list-timers tsuyu-rag-ingest.timer
journalctl -u tsuyu-rag-ingest.service --since today

# Cetus ingest segera (di luar jadual)
sudo systemctl start tsuyu-rag-ingest.service
```

> Alternatif: cron mudah — `0 1 * * * cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot ingest`.

---

## 5. Pemantauan

Endpoint `/metrics` (format Prometheus) — lihat [src/metrics.rs](src/metrics.rs):

```bash
curl -s http://127.0.0.1:8080/metrics
```

| Metrik | Maksud | Perhatikan jika |
|---|---|---|
| `tsuyu_chat_requests_total` | Jumlah permintaan chat | — (trend penggunaan) |
| `tsuyu_chat_errors_total` | Permintaan chat gagal | Naik mendadak = ada masalah (DB/Ollama) |
| `tsuyu_ingest_runs_total` | Bilangan ingest dicetus | — |
| `tsuyu_retrieval_duration_ms_sum` / `_count` | Purata masa retrieval | Purata tinggi = DB/indeks perlu disemak |
| `tsuyu_generate_duration_ms_sum` / `_count` | Purata masa penjanaan LLM | Purata tinggi = beban GPU/model |

**Purata** = `_sum ÷ _count`. Cth. masa generation purata:
```bash
curl -s http://127.0.0.1:8080/metrics | awk '
  /tsuyu_generate_duration_ms_sum/{s=$2}
  /tsuyu_generate_duration_ms_count/{c=$2}
  END{ if(c>0) printf "Purata jana: %.0f ms\n", s/c }'
```

Kadar ralat tinggi (`tsuyu_chat_errors_total` melonjak) ialah isyarat utama untuk siasat.

---

## 6. Senario kegagalan & pemulihan

### A. `/health` bukan `ok` atau aplikasi tak balas
```bash
systemctl status tsuyu-rag
journalctl -u tsuyu-rag -p err --since "10 min ago"
```
- **Aplikasi mati / restart berulang:** lihat log untuk sebab (selalunya `.env` salah,
  DB tak dapat disambung, atau port `BIND_ADDR` sudah diguna). Betulkan & `restart`.
- **DB putus:** `systemctl status postgresql`; mula semula jika perlu.

### B. Ollama mati atau model hilang
`/health` menyemak `GEN_MODEL` & `EMBED_MODEL` wujud dalam Ollama.
```bash
systemctl status ollama
ollama list                          # sahkan qwen3:14b & bge-m3 ada
ollama pull qwen3:14b                # jika hilang
ollama pull bge-m3
sudo systemctl restart tsuyu-rag
```

### C. Reranker (port 8081) mati
- Gejala: log aplikasi tunjuk ralat panggilan `/rerank`; jawapan masih keluar tetapi
  kualiti susunan mungkin merosot.
- Mula semula servis TEI reranker (lihat README §"Servis reranker"). Untuk operasi
  sementara tanpa reranker, set `RERANK_ENABLED=false` dalam `.env` & `restart`.

### D. Chatbot balas "tidak menjumpai maklumat" untuk soalan yang sah
Bukan ralat — ini guardrail anti-halusinasi menolak konteks tak cukup relevan. Periksa:
1. Dokumen berkaitan telah di-ingest? (`SELECT count(*) FROM chunks;` > 0)
2. Embedding hidup? (Ollama + `bge-m3`)
3. Ambang terlalu ketat? Lihat `RELEVANCE_MIN_RERANK` / `RELEVANCE_MAX_DISTANCE` —
   lalai sengaja **longgar**; jangan ketatkan tanpa data. Tala guna medan `distance`
   dalam `sources[]` respons `/chat`. (Lihat README §"Guardrail anti-halusinasi".)

### E. Selepas ingest, dokumen tak muncul dalam jawapan
- `POST /ingest` melangkau fail tak berubah (semak saiz + mtime). Paksa proses semula:
  `POST /ingest?force=true`.
- Sahkan fail berada dalam `DOCS_DIR` (dijelajah rekursif termasuk subfolder).

### F. Menukar model embedding / `EMBED_DIM`
Jika `EMBED_MODEL` atau `EMBED_DIM` ditukar, embedding lama **tidak serasi**. Aplikasi
selaras skema automatik & kosongkan chunk lama; selepas itu **wajib** embed semula:
```bash
curl -X POST http://127.0.0.1:8080/ingest?force=true \
     -H "Authorization: Bearer $ADMIN_API_KEY"
```

---

## 7. Sandaran & pemulihan (DR)

> **Paling penting.** Lakukan sandaran berjadual sebelum sebarang kemas kini besar.

### Apa yang perlu disandar
1. **Pangkalan data** (`tsuyu_rag`) — dokumen, chunk, embedding, memori. *Wajib.*
2. **Fail konfigurasi** (`/opt/tsuyu-rag/.env`) — mengandungi API key & tetapan. *Wajib,
   simpan dengan selamat — ada rahsia.*
3. **Folder dokumen sumber** (`DOCS_DIR`) — boleh dijana semula via ingest jika asal kekal,
   tetapi sandar untuk keselamatan.

> Binari boleh dibina semula dari kod sumber; tidak perlu disandar.

### Sandaran pangkalan data
```bash
# Sandaran penuh (mampat)
sudo -u postgres pg_dump -Fc tsuyu_rag > /backup/tsuyu_rag_$(date +%F).dump

# Contoh kerja cron harian 02:00 (crontab pengguna postgres)
0 2 * * * pg_dump -Fc tsuyu_rag > /backup/tsuyu_rag_$(date +\%F).dump && \
          find /backup -name 'tsuyu_rag_*.dump' -mtime +30 -delete
```

### Pemulihan pangkalan data
```bash
sudo systemctl stop tsuyu-rag

# Pulihkan ke DB baharu/kosong (pastikan sambungan vector ada)
sudo -u postgres createdb tsuyu_rag_restore
sudo -u postgres psql tsuyu_rag_restore -c "CREATE EXTENSION IF NOT EXISTS vector;"
sudo -u postgres pg_restore -d tsuyu_rag_restore /backup/tsuyu_rag_2026-06-02.dump

# Setelah disahkan, tukar DATABASE_URL dalam .env menunjuk DB dipulihkan, atau
# namakan semula DB. Kemudian:
sudo systemctl start tsuyu-rag
curl -s http://127.0.0.1:8080/health | jq .
```

### Sandaran konfigurasi & dokumen
```bash
sudo cp /opt/tsuyu-rag/.env /backup/env_$(date +%F).bak   # SIMPAN SELAMAT (ada rahsia)
sudo tar czf /backup/docs_$(date +%F).tgz -C /opt/tsuyu-rag docs
```

### Uji pemulihan secara berkala
Sandaran tidak diuji = tiada sandaran. Sekurang-kurangnya setiap suku tahun, pulihkan
ke pelayan/DB ujian dan sahkan kiraan chunk + satu pertanyaan `/chat` berfungsi.

---

## 8. Putaran kunci & sijil

- **API key** (`API_KEY` / `ADMIN_API_KEY`) di `.env`. Untuk putar: jana key baharu,
  kemas kini `.env`, `systemctl restart tsuyu-rag`, dan agih key baharu kepada klien.
- Jika di belakang reverse proxy TLS, pantau tarikh luput sijil secara berasingan.

---

## 9. Kemas kini & tetingkap penyelenggaraan

Urutan disyorkan untuk kemas kini aplikasi:
```bash
# 1. Sandar DB & .env dahulu (§7)
# 2. Build versi baharu (mesin build), uji
cargo build --release && cargo test
# 3. Henti servis, ganti binari, jalankan migrasi (automatik semasa mula), mula
sudo systemctl stop tsuyu-rag
sudo cp target/release/tsuyu-rag-chatbot /opt/tsuyu-rag/
sudo systemctl start tsuyu-rag
# 4. Sahkan
curl -s http://127.0.0.1:8080/health | jq .
journalctl -u tsuyu-rag --since "2 min ago"
```

> Migrasi DB dijalankan automatik semasa aplikasi mula
> ([src/db.rs](src/db.rs) — `run_migrations`). Sentiasa sandar DB sebelum kemas kini
> yang mengandungi migrasi baharu.

---

## 10. Kenalan & eskalasi

> Isi mengikut organisasi anda.

| Peranan | Nama | Hubungan |
|---|---|---|
| Pentadbir sistem | _(isi)_ | _(isi)_ |
| Pemilik aplikasi | _(isi)_ | _(isi)_ |
| Pasukan pembangunan | _(isi)_ | _(isi)_ |
